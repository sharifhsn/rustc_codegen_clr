use core::f16;

use crate::fn_ctx::MethodCompileCtx;
use crate::r#type::{GetTypeExt, utilis::is_fat_ptr};
use cilly::{
    Assembly, CILNode, ClassRef, Const, Float, Int, Interned, MethodRef, StaticFieldDesc, Type,
    cilnode::{IsPure, MethodKind},
    hashable::{HashableF32, HashableF64},
};
use rustc_middle::ty::ExistentialTraitRef;
use rustc_middle::{
    mir::{
        Const as MirConst, ConstOperand, ConstValue, Location,
        interpret::Scalar,
        interpret::{AllocId, GlobalAlloc},
        visit::Visitor,
    },
    ty::{FloatTy, IntTy, Ty, TyCtxt, TyKind, UintTy},
};
use rustc_span::def_id::DefId;

use crate::operand::static_data::{
    AllocationOrigin, add_allocation, reify_allocation_function, static_address,
};

fn constant_use_site<'tcx>(
    needle: &ConstOperand<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> Option<(usize, usize, u64)> {
    struct Locator<'a, 'tcx> {
        needle: &'a ConstOperand<'tcx>,
        counts: std::collections::HashMap<(usize, usize), u64>,
        found: Option<(usize, usize, u64)>,
    }

    impl<'tcx> Visitor<'tcx> for Locator<'_, 'tcx> {
        fn visit_const_operand(&mut self, constant: &ConstOperand<'tcx>, location: Location) {
            let key = (location.block.index(), location.statement_index);
            let ordinal = self.counts.entry(key).or_default();
            if std::ptr::eq(constant, self.needle) {
                self.found = Some((key.0, key.1, *ordinal));
            }
            *ordinal += 1;
        }
    }

    let body = ctx.body_opt()?;
    let mut locator = Locator {
        needle,
        counts: std::collections::HashMap::new(),
        found: None,
    };
    locator.visit_body(body);
    locator.found
}

fn allocation_origin_for_constant<'tcx>(
    const_op: &ConstOperand<'tcx>,
    constant: MirConst<'tcx>,
    root: AllocId,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> AllocationOrigin {
    let mut details = Vec::new();
    match constant {
        MirConst::Unevaluated(unevaluated, _) => {
            details.push("unevaluated".to_owned());
            details.push(
                ctx.tcx()
                    .def_path_str_with_args(unevaluated.def, unevaluated.args),
            );
            if let Some(promoted) = unevaluated.promoted {
                details.push("promoted".to_owned());
                details.push(promoted.index().to_string());
            } else {
                details.push("item-const".to_owned());
            }
        }
        MirConst::Ty(..) => details.push("type-const".to_owned()),
        MirConst::Val(..) => details.push("evaluated-const".to_owned()),
    }
    if let Some((block, statement, ordinal)) = constant_use_site(const_op, ctx) {
        details.push("mir-use".to_owned());
        details.push(block.to_string());
        details.push(statement.to_string());
        details.push(ordinal.to_string());
    } else {
        // Synthetic constants do not belong to the body's operand graph. Their callers supply a
        // distinct domain; the allocation graph digest still distinguishes different contents.
        details.push("synthetic-use".to_owned());
    }
    AllocationOrigin::for_current_instance(root, "mir-constant", details, ctx)
}

fn allocation_root(const_val: ConstValue) -> Option<AllocId> {
    match const_val {
        ConstValue::Scalar(Scalar::Ptr(ptr, _)) => Some(ptr.into_raw_parts().0.alloc_id()),
        ConstValue::Slice { alloc_id, .. } | ConstValue::Indirect { alloc_id, .. } => {
            Some(alloc_id)
        }
        ConstValue::Scalar(Scalar::Int(_)) | ConstValue::ZeroSized => None,
    }
}
pub fn handle_constant<'tcx>(
    const_op: &ConstOperand<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<CILNode> {
    let constant = const_op.const_;
    let constant = ctx.monomorphize(constant);
    let val = constant
        .eval(
            ctx.tcx(),
            rustc_middle::ty::TypingEnv::fully_monomorphized(),
            const_op.span,
        )
        .expect("Could not evaluate constant!");
    let origin = allocation_root(val)
        .map(|root| allocation_origin_for_constant(const_op, constant, root, ctx));
    load_const_value_with_origin(val, constant.ty(), origin.as_ref(), ctx)
}

/// Returns the ops neceasry to create constant value of type `ty` with byte values matching the ones in the allocation
fn create_const_from_data<'tcx>(
    ty: Ty<'tcx>,
    alloc_id: AllocId,
    offset_bytes: u64,
    origin: &AllocationOrigin,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<CILNode> {
    let ty = ctx.monomorphize(ty);
    let tpe = ctx.type_from_cache(ty);
    // Optimization - check if this can be replaced by a scalar.
    if let GlobalAlloc::Memory(alloc) = ctx.tcx().global_alloc(alloc_id) {
        let const_alloc = alloc.inner();
        let align = const_alloc.align.bytes().max(1);
        let bytes: Vec<u8> = const_alloc
            .inspect_with_uninit_and_ptr_outside_interpreter(0..const_alloc.len())
            .into();
        // Right aligment, fits, and has no pointers - can be a scalar. ONLY at offset 0: this path
        // materializes the WHOLE allocation, so a nonzero offset (a const pointing into the MIDDLE of a
        // larger alloc — e.g. GVN const-propagating `ARR[2]` out of a 4-elem const array) would read the
        // wrong sub-object. For offset != 0 fall through to the by-ref path, which applies the offset
        // (seam-audit gap #2: the offset was previously discarded entirely via `let _ = offset_bytes;`).
        if offset_bytes == 0
            && align <= 8
            && bytes.len() <= 16
            && const_alloc.provenance().ptrs().is_empty()
        {
            let scalar = Scalar::from_u128(ctx.target_layout().decode_uint(&bytes));
            return load_const_scalar(scalar, ty, None, ctx).into();
        }
        let (ptr, align) = alloc_ptr_unaligned(alloc_id, &alloc, origin, ctx);
        // Apply the byte offset on the raw pointer (CIL `add` is byte arithmetic), mirroring
        // `load_scalar_ptr`'s `GlobalAlloc::Memory` arm.
        let ptr = if offset_bytes != 0 {
            ctx.biop(ptr, cilly::Const::USize(offset_bytes), cilly::BinOp::Add)
        } else {
            ptr
        };
        let ty = ctx.monomorphize(ty);

        let tpe = ctx.type_from_cache(ty);
        let ptr = ctx.cast_ptr(ptr, tpe);
        if align.is_none() {
            return ctx.load(ptr, tpe);
        } else {
            let unaligned_read = Interned::unaligned_read(ctx, tpe);
            return ctx.call(unaligned_read, &[ptr], IsPure::NOT);
        }
    }

    let ptr = add_allocation(alloc_id, origin, ctx);
    let ptr = if offset_bytes != 0 {
        ctx.biop(ptr, cilly::Const::USize(offset_bytes), cilly::BinOp::Add)
    } else {
        ptr
    };
    let ptr = ctx.cast_ptr(ptr, tpe);
    return ctx.load(ptr, tpe);
}
pub fn load_const_value<'tcx>(
    const_val: ConstValue,
    const_ty: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<CILNode> {
    let origin = allocation_root(const_val).map(|root| {
        AllocationOrigin::for_current_instance(
            root,
            "compiler-generated-constant",
            [format!(
                "{:032x}",
                ctx.tcx().type_id_hash(ctx.monomorphize(const_ty))
            )],
            ctx,
        )
    });
    load_const_value_with_origin(const_val, const_ty, origin.as_ref(), ctx)
}

fn load_const_value_with_origin<'tcx>(
    const_val: ConstValue,
    const_ty: Ty<'tcx>,
    origin: Option<&AllocationOrigin>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<CILNode> {
    match const_val {
        ConstValue::Scalar(scalar) => load_const_scalar(scalar, const_ty, origin, ctx),
        ConstValue::ZeroSized => {
            let tpe = ctx.monomorphize(const_ty);
            assert!(
                crate::r#type::utilis::is_zst(tpe, ctx.tcx()),
                "Zero sized const with a non-zero size. It is {tpe:?}"
            );
            let tpe = ctx.type_from_cache(tpe);
            ctx.uninit_val(tpe)
        }
        ConstValue::Slice { meta, alloc_id } => {
            // SUS
            let data = ctx.tcx().global_alloc(alloc_id).unwrap_memory();
            let slice_type = ctx.type_from_cache(const_ty);
            let slice_dotnet = slice_type.as_class_ref().expect("Slice type invalid!");

            let alloc_id = alloc_id;

            let ptr = if meta == 0 {
                ctx.alloc_node(Const::USize(1 << 30))
            } else {
                alloc_ptr(
                    alloc_id,
                    &data,
                    origin.expect("slice allocation must have a semantic origin"),
                    ctx,
                )
            };
            let ptr = ctx.cast_ptr(ptr, Type::Void);
            let meta = ctx.alloc_node(Const::USize(meta));
            ctx.create_slice(slice_dotnet, ptr, meta)
        }
        ConstValue::Indirect { alloc_id, offset } => {
            create_const_from_data(
                const_ty,
                alloc_id,
                offset.bytes(),
                origin.expect("indirect allocation must have a semantic origin"),
                ctx,
            )
            //todo!("Can't handle by-ref allocation {alloc_id:?} {offset:?}")
        } //_ => todo!("Unhandled const value {const_val:?} of type {const_ty:?}"),
    }
}
pub fn static_ty<'tcx>(def_id: DefId, tcx: TyCtxt<'tcx>) -> Ty<'tcx> {
    tcx.type_of(def_id)
        .instantiate_identity()
        .skip_normalization()
}
fn load_scalar_ptr(
    ctx: &mut MethodCompileCtx<'_, '_>,
    ptr: rustc_middle::mir::interpret::Pointer,
    origin: &AllocationOrigin,
) -> Interned<CILNode> {
    let (alloc_id, offset) = ptr.into_raw_parts();
    let global_alloc = ctx.tcx().global_alloc(alloc_id.alloc_id());
    let u8_ptr = ctx.nptr(Type::Int(Int::U8));
    let u8_ptr_ptr = ctx.nptr(u8_ptr);
    match global_alloc {
        GlobalAlloc::Static(def_id) => {
            assert!(ctx.tcx().is_static(def_id));
            assert_eq!(offset.bytes(), 0);
            let name = ctx
                .tcx()
                .opt_item_name(def_id)
                .expect("Static without name")
                .to_string();
            /* */
            if name == "__rust_alloc_error_handler_should_panic"
                || name == "__rust_no_alloc_shim_is_unstable"
            {
                let stotic = StaticFieldDesc::new(
                    *ctx.main_module(),
                    ctx.alloc_string(name),
                    Type::Int(Int::U8),
                );
                return ctx.static_addr(stotic);
            }
            if name == "environ" {
                let mref = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string("get_environ"),
                    ctx.sig([], u8_ptr_ptr),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                let environ = ctx.alloc_node(cilly::ir::CILNode::call(mref, []));
                let environ = ctx.annon_const(environ);
                return ctx.alloc_node(cilly::ir::CILNode::LdStaticFieldAddress(environ));
            }
            let attrs = ctx.tcx().codegen_fn_attrs(def_id);

            if attrs.import_linkage.is_some() {
                // TODO: this could cause issues if the pointer to the static is not imediatly dereferenced.
                let site = get_fn_from_static_name(&name, ctx);
                let cst = ctx.annon_const(cilly::ir::CILNode::LdFtn(site));
                return ctx.alloc_node(cilly::ir::CILNode::LdStaticFieldAddress(cst));
            }
            if let Some(section) = attrs.link_section {
                panic!("static {name} requires special linkage in section {section:?}");
            }
            // Preserve the static's Rust identity. Re-evaluating its initializer into a fresh
            // anonymous Memory allocation gives every use of `static mut` a different backing
            // cell and makes its emitted name depend on rustc's session-local AllocId counter.
            static_address(def_id, ctx)
        }
        GlobalAlloc::Memory(const_allocation) => {
            let ptr = alloc_ptr(alloc_id.alloc_id(), &const_allocation, origin, ctx);
            if offset.bytes() != 0 {
                ctx.biop(ptr, cilly::Const::USize(offset.bytes()), cilly::BinOp::Add)
            } else {
                ptr
            }
        }
        GlobalAlloc::Function {
            instance: finstance,
        } => {
            assert_eq!(offset.bytes(), 0);
            reify_allocation_function(finstance, ctx)
        }
        GlobalAlloc::TypeId { .. } => {
            // A `TypeId` pointer is opaque: its integer value (`offset`) is one
            // pointer-sized segment of the 128-bit type-id hash. There is no real
            // allocation to point at, so materialize the segment value directly as
            // a (base-0) pointer. `TypeId::of::<T>()` only requires this to be
            // self-consistent for equality, which holds since identical types yield
            // identical hash segments. See `static_data.rs`'s TypeId arms.
            let val = ctx.alloc_node(cilly::Const::USize(offset.bytes()));
            ctx.cast_ptr(val, Int::U8)
        }
        GlobalAlloc::VTable(_, _) => {
            let (ty, polyref) = global_alloc.unwrap_vtable();
            get_vtable(
                ctx,
                ctx.monomorphize(ty),
                polyref.map(|principal| ctx.tcx().instantiate_bound_regions_with_erased(principal)),
            )
        }
    }
    //panic!("alloc_id:{alloc_id:?}")
}
/// Returns a pointer to an immutable buffer, representing a given allocation.
fn alloc_ptr<'tcx>(
    alloc_id: AllocId,
    const_alloc: &rustc_middle::mir::interpret::ConstAllocation,
    origin: &AllocationOrigin,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<CILNode> {
    let (ptr, align) = alloc_ptr_unaligned(alloc_id, const_alloc, origin, ctx);
    // If alignment is small enough to be *guaranteed*, and no pointers are present.
    if align.is_some_and(|align| align <= ctx.const_align()) {
        add_allocation(alloc_id, origin, ctx)
    } else {
        ptr
    }
}
/// Returns a pointer to an immutable buffer, representing a given allocation. Pointer may be underaligned; alignment of `u64::MAX` signals that the pointer
/// will be sufficently aligned for `const_alloc`.
fn alloc_ptr_unaligned<'tcx>(
    alloc_id: AllocId,
    const_alloc: &rustc_middle::mir::interpret::ConstAllocation,
    origin: &AllocationOrigin,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> (Interned<CILNode>, Option<u64>) {
    let const_alloc = const_alloc.inner();
    // If alignment is small enough to be *guaranteed*, and no pointers are present.
    if const_alloc.provenance().ptrs().is_empty() {
        if const_alloc.align.bytes() <= ctx.const_align() {
            (
                ctx.bytebuffer(
                    const_alloc
                        .inspect_with_uninit_and_ptr_outside_interpreter(0..const_alloc.len()),
                    Int::U8,
                ),
                None,
            )
        } else {
            //unaligned_read
            (
                ctx.bytebuffer(
                    const_alloc
                        .inspect_with_uninit_and_ptr_outside_interpreter(0..const_alloc.len()),
                    Int::U8,
                ),
                Some(ctx.const_align()),
            )
        }
    } else {
        (add_allocation(alloc_id, origin, ctx), None)
    }
}
/// Load a scalar integer constant of `byte_size` bytes (its value already in `bits`), then
/// transmute it to `dst`. `transmute_on_stack` is a size-exact reinterpret, so the SOURCE integer
/// must match `dst`'s width — using a fixed `U128` for, say, a 1-byte fieldless enum writes 16
/// bytes into a 1-byte slot and produces invalid IL ("Bad IL format" at JIT time). This picks the
/// CIL integer type matching the destination's actual size.
fn transmute_scalar_to(
    bits: u128,
    byte_size: u64,
    dst: Type,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Interned<CILNode> {
    let (src_int, val) = match byte_size {
        0 => {
            // A ZST destination: nothing to reinterpret.
            let dptr = ctx.nptr(dst);
            return ctx.uninit_val(dptr);
        }
        1 => (Int::U8, Const::U8(bits as u8)),
        2 => (Int::U16, Const::U16(bits as u16)),
        3 | 4 => (Int::U32, Const::U32(bits as u32)),
        5..=8 => (Int::U64, Const::U64(bits as u64)),
        16 => (Int::U128, Const::U128(bits)),
        // `transmute_on_stack` is size-EXACT: the source integer must be the same width as `dst`.
        // Only 1/2/4/8/16-byte dsts have a matching CIL integer; a 9..=15-byte dst would reinterpret
        // a 16-byte U128 source onto a narrower slot and emit invalid IL ("Bad IL format") at JIT.
        // `byte_size` is at most 16 (a scalar is u128-backed), so this arm cannot fire on valid code;
        // fail loud at codegen rather than silently produce bad IL. (I3 totality.)
        other => panic!(
            "transmute_scalar_to: cannot size-exactly reinterpret a 16-byte source onto a {other}-byte scalar destination ({dst:?}); transmute_on_stack requires src width == dst width, so a constant of an odd 9..=15-byte size would emit invalid IL (\"Bad IL format\") at JIT time"
        ),
    };
    let val = ctx.alloc_node(val);
    ctx.transmute_on_stack(Type::Int(src_int), dst, val)
}
fn load_const_scalar<'tcx>(
    scalar: Scalar,
    scalar_type: Ty<'tcx>,
    origin: Option<&AllocationOrigin>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<CILNode> {
    let scalar_ty = ctx.monomorphize(scalar_type);
    let scalar_type = ctx.type_from_cache(scalar_ty);

    let scalar_u128 = match scalar {
        Scalar::Int(scalar_int) => scalar_int.to_uint(scalar.size()),
        Scalar::Ptr(ptr, _size) => {
            let const_type = scalar_ty
                .builtin_deref(true)
                .map(|ty| ctx.type_from_cache(ty))
                .unwrap_or(Int::USize.into());
            let const_type_idx = ctx.alloc_type(const_type);
            let ptr = load_scalar_ptr(
                ctx,
                ptr,
                origin.expect("pointer constant must have a semantic allocation origin"),
            );

            if matches!(scalar_type, Type::Ptr(_)) {
                return ctx.cast_ptr(ptr, const_type_idx);
            } else if matches!(scalar_type, Type::FnPtr(_)) {
                return ptr;
            }
            let src_ptr = ctx.nptr(Int::U8);
            let ptr = ctx.cast_ptr(ptr, Int::U8);
            return ctx.transmute_on_stack(src_ptr, scalar_type, ptr);
        }
    };

    match scalar_ty.kind() {
        TyKind::Int(int_type) => load_const_int(scalar_u128, *int_type, ctx),
        TyKind::Uint(uint_type) => load_const_uint(scalar_u128, *uint_type, ctx),
        TyKind::Float(ftype) => load_const_float(scalar_u128, *ftype, ctx).into(),
        TyKind::Bool => ctx.alloc_node(scalar_u128 != 0),
        TyKind::RawPtr(..) | TyKind::Ref(..) => {
            if is_fat_ptr(scalar_ty, ctx.tcx(), ctx.instance()) {
                let val = ctx.alloc_node(scalar_u128);
                ctx.transmute_on_stack(Type::Int(Int::U128), scalar_type, val)
            } else {
                let val = ctx.alloc_node(Const::USize(
                    u64::try_from(scalar_u128).expect("pointers must be smaller than 2^64"),
                ));
                let const_type = scalar_ty
                    .builtin_deref(true)
                    .map(|ty| ctx.type_from_cache(ty))
                    .unwrap_or(Int::USize.into());
                ctx.cast_ptr(val, const_type)
            }
        }
        TyKind::Tuple(elements) => {
            if elements.is_empty() {
                let scalar_ptr = ctx.nptr(scalar_type);
                ctx.uninit_val(scalar_ptr)
            } else {
                transmute_scalar_to(scalar_u128, scalar.size().bytes(), scalar_type, ctx)
            }
        }
        TyKind::Adt(_, _) | TyKind::Closure(_, _) | TyKind::Array(_, _) => {
            transmute_scalar_to(scalar_u128, scalar.size().bytes(), scalar_type, ctx)
        }
        TyKind::Char => ctx.alloc_node(u32::try_from(scalar_u128).unwrap()),
        _ => todo!("Can't load scalar constants of type {scalar_ty:?}!"),
    }
}
fn load_const_float(
    value: u128,
    float_type: FloatTy,
    asm: &mut Assembly,
) -> Interned<cilly::ir::CILNode> {
    match float_type {
        FloatTy::F16 => {
            #[cfg(not(target_family = "windows"))]
            {
                let mref = MethodRef::new(
                    ClassRef::half(asm),
                    asm.alloc_string("op_Explicit"),
                    asm.sig([Type::Float(Float::F32)], Type::Float(Float::F16)),
                    MethodKind::Static,
                    vec![].into(),
                );
                let cst = asm.alloc_node(Const::F32(HashableF32(f16::from_bits(
                    u16::try_from(value).unwrap(),
                ) as f32)));
                asm.call(mref, &[cst], IsPure::PURE)
            }
            #[cfg(target_family = "windows")]
            {
                todo!("building a program using 16 bit floats is not supported on windwows yet")
                // TODO: check if this still causes a linker error on windows
            }
        }
        FloatTy::F32 => {
            let value = f32::from_bits(u32::try_from(value).unwrap());
            asm.alloc_node(Const::F32(HashableF32(value))).into()
        }
        FloatTy::F64 => {
            let value = f64::from_bits(u64::try_from(value).unwrap());
            asm.alloc_node(Const::F64(HashableF64(value))).into()
        }
        FloatTy::F128 => {
            let u128_const = asm.alloc_node(Const::U128(value));
            asm.transmute_on_stack(Type::Int(Int::U128), Type::Float(Float::F128), u128_const)
        }
    }
}
pub fn load_const_int(
    value: u128,
    int_type: IntTy,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Interned<cilly::ir::CILNode> {
    match int_type {
        #[allow(clippy::cast_possible_wrap)]
        IntTy::I8 => ctx.alloc_node(u8::try_from(value).unwrap() as i8),
        #[allow(clippy::cast_possible_wrap)]
        IntTy::I16 => ctx.alloc_node(u16::try_from(value).unwrap() as i16),
        #[allow(clippy::cast_possible_wrap)]
        IntTy::I32 => ctx.alloc_node(u32::try_from(value).unwrap() as i32),
        #[allow(clippy::cast_possible_wrap)]
        IntTy::I64 => ctx.alloc_node(u64::try_from(value).unwrap() as i64),
        IntTy::Isize => {
            let signed = match ctx.target_layout().pointer_bits() {
                32 => i64::from(u32::try_from(value).unwrap() as i32),
                64 => u64::try_from(value).unwrap() as i64,
                width => unreachable!("unsupported target pointer width {width}"),
            };
            ctx.alloc_node(cilly::Const::ISize(signed))
        }
        #[allow(clippy::cast_possible_wrap)]
        IntTy::I128 => ctx.alloc_node(value as i128),
    }
}
pub fn load_const_uint(
    value: u128,
    int_type: UintTy,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Interned<cilly::ir::CILNode> {
    match int_type {
        UintTy::U8 => ctx.alloc_node(u8::try_from(value).unwrap()),
        UintTy::U16 => ctx.alloc_node(u16::try_from(value).unwrap()),
        UintTy::U32 => ctx.alloc_node(u32::try_from(value).unwrap()),
        UintTy::U64 => ctx.alloc_node(u64::try_from(value).unwrap()),
        UintTy::Usize => {
            assert!(value <= ctx.target_layout().unsigned_pointer_max());
            ctx.alloc_node(cilly::Const::USize(u64::try_from(value).unwrap()))
        }
        UintTy::U128 => ctx.alloc_node(value),
    }
}

fn get_fn_from_static_name(name: &str, ctx: &mut MethodCompileCtx<'_, '_>) -> Interned<MethodRef> {
    let int8_ptr = ctx.nptr(Type::Int(Int::I8));
    let int64_ptr = ctx.nptr(Type::Int(Int::I64));
    let void_ptr = ctx.nptr(Type::Void);
    let int8_ptr_ptr = ctx.nptr(int8_ptr);
    let mref = match name {
        "statx" => MethodRef::new(
            *ctx.main_module(),
            ctx.alloc_string("statx"),
            ctx.sig(
                [
                    Type::Int(Int::I32),
                    int8_ptr,
                    Type::Int(Int::I32),
                    Type::Int(Int::U32),
                    void_ptr,
                ],
                Type::Int(Int::I32),
            ),
            MethodKind::Static,
            vec![].into(),
        ),
        "getrandom" => MethodRef::new(
            *ctx.main_module(),
            ctx.alloc_string("getrandom"),
            ctx.sig(
                [int8_ptr, Type::Int(Int::USize), Type::Int(Int::U32)],
                Type::Int(Int::USize),
            ),
            MethodKind::Static,
            vec![].into(),
        ),
        "posix_spawn" => MethodRef::new(
            *ctx.main_module(),
            ctx.alloc_string("posix_spawn"),
            ctx.sig(
                [int8_ptr, int8_ptr, int8_ptr, int8_ptr, int8_ptr, int8_ptr],
                Type::Int(Int::I32),
            ),
            MethodKind::Static,
            vec![].into(),
        ),
        "posix_spawn_file_actions_addchdir_np" => MethodRef::new(
            *ctx.main_module(),
            ctx.alloc_string("posix_spawn_file_actions_addchdir_np"),
            ctx.sig([int8_ptr, int8_ptr], Type::Int(Int::I32)),
            MethodKind::Static,
            vec![].into(),
        ),
        "__dso_handle" => MethodRef::new(
            *ctx.main_module(),
            ctx.alloc_string("__dso_handle"),
            ctx.sig([], Type::Void),
            MethodKind::Static,
            vec![].into(),
        ),
        "__cxa_thread_atexit_impl" => {
            let fn_ptr_sig = Type::FnPtr(ctx.sig([void_ptr], Type::Void));
            MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("__cxa_thread_atexit_impl"),
                ctx.sig([fn_ptr_sig, void_ptr, void_ptr], Type::Void),
                MethodKind::Static,
                vec![].into(),
            )
        }
        "copy_file_range" => {
            let i64_ptr = ctx.nptr(Type::Int(Int::I64));
            MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("copy_file_range"),
                ctx.sig(
                    [
                        Type::Int(Int::I32),
                        int64_ptr,
                        Type::Int(Int::I32),
                        i64_ptr,
                        Type::Int(Int::ISize),
                        Type::Int(Int::U32),
                    ],
                    Type::Int(Int::ISize),
                ),
                MethodKind::Static,
                vec![].into(),
            )
        }
        "pidfd_spawnp" => {
            let i32_ptr = ctx.nptr(Type::Int(Int::I32));
            let i8_ptr = ctx.nptr(Type::Int(Int::I8));
            MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("pidfd_spawnp"),
                ctx.sig(
                    [
                        i32_ptr,
                        i8_ptr,
                        void_ptr,
                        void_ptr,
                        int8_ptr_ptr,
                        int8_ptr_ptr,
                    ],
                    Type::Int(Int::I32),
                ),
                MethodKind::Static,
                vec![].into(),
            )
        }
        "pidfd_getpid" => MethodRef::new(
            *ctx.main_module(),
            ctx.alloc_string("pidfd_getpid"),
            ctx.sig([Type::Int(Int::I32)], Type::Int(Int::I32)),
            MethodKind::Static,
            vec![].into(),
        ),
        // `gettid()` (glibc >= 2.30, `() -> pid_t`) is referenced as a weak static by
        // `std::sys::thread::unix::current_os_id`. Resolve it like the other libc weak statics —
        // a main-module methodref the linker PInvokes to host libc (it is in `LIBC_FNS`).
        "gettid" => MethodRef::new(
            *ctx.main_module(),
            ctx.alloc_string("gettid"),
            ctx.sig([], Type::Int(Int::I32)),
            MethodKind::Static,
            vec![].into(),
        ),
        _ => {
            todo!("Unsuported function refered to using a weak static. Function name is {name:?}.")
        }
    };
    ctx.alloc_methodref(mref)
}
pub fn get_vtable<'tcx>(
    fx: &mut MethodCompileCtx<'tcx, '_>,
    ty: Ty<'tcx>,
    trait_ref: Option<ExistentialTraitRef<'tcx>>,
) -> Interned<cilly::ir::CILNode> {
    let ty = fx.monomorphize(ty);

    let alloc_id = fx.tcx().vtable_allocation((ty, trait_ref));
    // `vtable_allocation` has already materialized the exact self-describing memory allocation;
    // `add_allocation` derives its field size/alignment and relocations directly from that source.
    let origin = AllocationOrigin::for_current_instance(
        alloc_id,
        "vtable",
        [format!("{:032x}", fx.tcx().type_id_hash(ty))],
        fx,
    );
    add_allocation(alloc_id, &origin, fx)
}
