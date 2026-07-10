use crate::{
    assembly::MethodCompileCtx,
    utilis::{adt::get_discr, const_sizeof},
};
use cilly::{
    cilnode::{ExtendKind, IsPure, MethodKind},
    BinOp, Const, FieldDesc, Float, Int, Interned, MethodRef, Type,
};

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;
use crate::call_info::CallInfo;
use crate::fn_ctx::fn_name;
use crate::place::{place_address, place_get};
use crate::r#type::{
    adt::enum_tag_info,
    get_type,
    utilis::ptr_is_fat,
    GetTypeExt,
};
use crate::operand::{
    handle_operand, is_const_zero, is_uninit, operand_address, static_data::add_allocation,
};
use rustc_middle::{
    mir::{CastKind, Operand, Place, Rvalue},
    ty::{adjustment::PointerCoercion, Instance, Ty, TyKind},
};
macro_rules! cast {
    ($ctx:ident,$operand:ident,$target:ident,$cast_name:path,$asm:expr) => {{
        let target = $ctx.monomorphize(*$target);
        let target = $ctx.type_from_cache(target);
        let src = $operand.ty(&$ctx.body().local_decls, $ctx.tcx());
        let src = $ctx.monomorphize(src);
        let src = $ctx.type_from_cache(src);
        $cast_name(
            src,
            target,
            crate::operand::handle_operand($operand, $ctx),
            $asm,
        )
    }};
}
pub fn is_rvalue_unint<'tcx>(rvalue: &Rvalue<'tcx>, ctx: &mut MethodCompileCtx<'tcx, '_>) -> bool {
    match rvalue {
        Rvalue::Repeat(operand, _) | Rvalue::Use(operand, _) => is_uninit(operand, ctx),
        /* TODO: before enabling this, check if the aggregate is an enum, and if so, check if it has a discriminant.
        Rvalue::Aggregate(_, field_index) => field_index
        .iter()
        .all(|operand| is_uninit(operand, ctx)),*/
        _ => false,
    }
}
pub fn is_rvalue_const_0<'tcx>(
    rvalue: &Rvalue<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> bool {
    match rvalue {
        Rvalue::Repeat(operand, _) | Rvalue::Use(operand, _) => is_const_zero(operand, ctx),
        _ => false,
    }
}
/// Discriminant of a `Variants::Single` enum (a layout with a single inhabited
/// variant — either a ZST enum or a tag-less sized enum such as the `Result<Infallible, _>`
/// residual of `Try::branch`). Such layouts carry no in-memory tag, so the discriminant is
/// the *constant* discriminant of that sole inhabited variant (mapped through any explicit
/// `#[repr]` discriminant), cast to `target`. Falls back to `index` if there is no explicit
/// discriminant, and to 0 for any non-`Single` layout (defensive; the callers only reach
/// this for tag-less layouts).
fn single_variant_discr<'tcx>(
    owner_ty: Ty<'tcx>,
    target: Type,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    let index = match ctx.layout_of(owner_ty).layout.variants {
        rustc_abi::Variants::Single { index } => Some(index),
        _ => None,
    };
    let discr_val: u128 = match index {
        Some(index) => owner_ty
            .discriminant_for_variant(ctx.tcx(), index)
            .map_or(u128::from(index.as_u32()), |discr| discr.val),
        None => 0,
    };
    match target {
        Type::Int(Int::U128) => ctx.alloc_node(discr_val),
        Type::Int(Int::I128) => ctx.alloc_node(discr_val as i128),
        _ => {
            let v = ctx
                .alloc_node(u64::try_from(discr_val).expect("single-variant discriminant fits in u64"));
            crate::casts::int_to_int(Type::Int(Int::U64), target, v, ctx)
        }
    }
}
pub fn handle_rvalue<'tcx>(
    rvalue: &Rvalue<'tcx>,
    dst_place: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> (Vec<Root>, Node) {
    match rvalue {
       
        Rvalue::Use(operand, _) => (vec![], handle_operand(operand, ctx)),
        // TODO: check the exact semantics of `WrapUnsafeBinder` once it has some documentation.
        Rvalue::WrapUnsafeBinder(operand, _unknown_ty) => (vec![], handle_operand(operand, ctx)),
        Rvalue::CopyForDeref(place) => (vec![], place_get(place, ctx)),
        // `Reborrow` creates a bitwise copy of the place (same memory layout as the source). This
        // matches how rustc_codegen_ssa lowers it: as a plain `Operand::Copy(place)` read.
        Rvalue::Reborrow(_ty, _mutability, place) => (vec![], place_get(place, ctx)),
        Rvalue::Ref(_region, _borrow_kind, place) => (vec![], place_address(place, ctx)),
        Rvalue::RawPtr(_mutability, place) => (vec![], place_address(place, ctx)),
        Rvalue::Cast(
            CastKind::PointerCoercion(PointerCoercion::UnsafeFnPointer, _),
            operand,
            _dst,
        ) => (vec![], handle_operand(operand, ctx)),
        Rvalue::Cast(
            CastKind::PointerCoercion(
                PointerCoercion::MutToConstPointer | PointerCoercion::ArrayToPointer,
                _,
            )
            | CastKind::PtrToPtr,
            operand,
            dst,
        ) => (vec![], ptr_to_ptr(ctx, operand, *dst)),
        Rvalue::Cast(CastKind::PointerCoercion(PointerCoercion::Unsize, _), operand, target) => {
            crate::unsize::unsize(ctx, operand, *target, *dst_place)
        }
        Rvalue::BinaryOp(binop, operands) => (
            vec![],
            crate::binop::binop(*binop, &operands.0, &operands.1, ctx),
        ),
        Rvalue::UnaryOp(binop, operand) => {
            (vec![], crate::unop::unop(*binop, operand, ctx, rvalue))
        }
        Rvalue::Cast(CastKind::IntToInt, operand, target) => (
            vec![],
            cast!(ctx, operand, target, crate::casts::int_to_int, ctx),
        ),
        Rvalue::Cast(CastKind::FloatToInt, operand, target) => (
            vec![],
            cast!(ctx, operand, target, crate::casts::float_to_int, ctx),
        ),
        Rvalue::Cast(CastKind::IntToFloat, operand, target) => (
            vec![],
            cast!(ctx, operand, target, crate::casts::int_to_float, ctx),
        ),
        // `Rvalue::NullaryOp` (SizeOf/AlignOf/OffsetOf/UbChecks/ContractChecks) no longer exists in
        // MIR for this rustc version. SizeOf/AlignOf/OffsetOf are now const intrinsics that are
        // fully const-evaluated before reaching the backend (they arrive as `Operand::Constant`),
        // and UbChecks/ContractChecks are now `Operand::RuntimeChecks` (handled in the operand crate).
        Rvalue::Aggregate(aggregate_kind, field_index) => crate::aggregate::handle_aggregate(
            ctx,
            dst_place,
            aggregate_kind.as_ref(),
            field_index,
        ),

        Rvalue::Cast(
            CastKind::PointerCoercion(PointerCoercion::ClosureFnPointer(_), _),
            ref operand,
            to_ty,
        ) => match ctx.monomorphize(operand.ty(ctx.body(), ctx.tcx())).kind() {
            TyKind::Closure(def_id, args) => {
                let instance = Instance::resolve_closure(
                    ctx.tcx(),
                    *def_id,
                    args,
                    rustc_middle::ty::ClosureKind::FnOnce,
                );
                let call_info = CallInfo::sig_from_instance_(instance, ctx);

                let function_name = fn_name(ctx.tcx().symbol_name(instance));
                let fn_ptr_sig = ctx.alloc_sig(call_info.sig().clone());
                let call_site = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string(function_name),
                    fn_ptr_sig,
                    MethodKind::Static,
                    vec![].into(),
                );
                let target_type = ctx.type_from_cache(*to_ty);
                let Type::FnPtr(target_sig) = target_type else {
                    rustc_middle::bug!(
                        "ClosureFnPointer target not a fn ptr. {}",
                        target_type.mangle(ctx)
                    )
                };
                // Route through the adapter-thunk helper: when the physical method has elided
                // (Void/ZST) params that the fn-ptr type lacks, this synthesises an arity-matching
                // adapter instead of lying about the pointer's ABI with a bare cast.
                (vec![], ctx.reify_fnptr(call_site, target_sig))
            }
            _ => panic!(
                "{} cannot be cast to a fn ptr",
                operand.ty(ctx.body(), ctx.tcx())
            ),
        },
        Rvalue::Cast(CastKind::Transmute, operand, dst) => {
            let dst = ctx.monomorphize(*dst);
            let dst = ctx.type_from_cache(dst);
            let src = operand.ty(&ctx.body().local_decls, ctx.tcx());
            let src = ctx.monomorphize(src);
            let src = ctx.type_from_cache(src);
            match (&src, &dst) {
                (
                    Type::Int(Int::ISize | Int::USize) | Type::Ptr(_) | Type::FnPtr(_),
                    Type::Int(Int::ISize | Int::USize) | Type::Ptr(_) | Type::FnPtr(_),
                ) => {
                    let val = handle_operand(operand, ctx);
                    (vec![], ctx.cast_ptr_to(val, dst))
                }

                (Type::Int(Int::U16), Type::PlatformChar) => (vec![], handle_operand(operand, ctx)),
                (_, _) => {
                    let val = handle_operand(operand, ctx);
                    (vec![], ctx.transmute_on_stack(src, dst, val))
                }
            }
        }
        // `Rvalue::ShallowInitBox` no longer exists in MIR for this rustc version; box construction
        // is now expressed through ordinary allocation + assignment, so there is nothing to handle here.
        Rvalue::Cast(CastKind::PointerWithExposedProvenance, operand, target) => {
            //FIXME: the documentation of this cast(https://doc.rust-lang.org/nightly/std/ptr/fn.from_exposed_addr.html) is a bit confusing,
            //since this seems to be something deeply linked to the rust memory model.
            // I assume this to be ALWAYS equivalent to `usize as *const/mut T`, but this may not always be the case.
            // If something breaks in the fututre, this is a place that needs checking.
            let target = ctx.monomorphize(*target);
            let target = ctx.type_from_cache(target);
            // Cast from usize/isize to any *T is a NOP, so we just have to load the operand.
            let val = handle_operand(operand, ctx);
            (vec![], ctx.cast_ptr_to(val, target))
        }
        Rvalue::Cast(CastKind::PointerExposeProvenance, operand, target) => {
            //FIXME: the documentation of this cast(https://doc.rust-lang.org/nightly/std/primitive.pointer.html#method.expose_addrl) is a bit confusing,
            //since this seems to be something deeply linked to the rust memory model.
            // I assume this to be ALWAYS equivalent to `*const/mut T as usize`, but this may not always be the case.
            // If something breaks in the fututre, this is a place that needs checking.
            let target = ctx.monomorphize(*target);
            let target = ctx.type_from_cache(target);
            // Cast to usize/isize from any *T is a NOP, so we just have to load the operand.

            let val = handle_operand(operand, ctx);
            let res = match target {
                Type::Int(Int::USize | Int::ISize) | Type::Ptr(_) | Type::FnPtr(_) => {
                    ctx.cast_ptr_to(val, target)
                }
                // Any other integer width (u64/i64 and the narrow u32/u16/u8 / i32/i16/i8 / u128/i128):
                // expose the address as `usize`, then narrow/widen to the target like a normal int cast.
                // Previously only u64/i64 were handled and a narrow target (`ptr as u32`) hit the `todo!`
                // ICE (seam-audit gap #9).
                Type::Int(_) => {
                    let us = ctx.cast_ptr_to(val, Type::Int(Int::USize));
                    crate::casts::int_to_int(Type::Int(Int::USize), target, us, ctx)
                }
                _ => todo!("Can't cast using `PointerExposeProvenance` to {target:?}"),
            };
            (vec![], res)
        }
        Rvalue::Cast(CastKind::FloatToFloat, operand, target) => {
            let target = ctx.monomorphize(*target);
            let target = ctx.type_from_cache(target);
            let src = ctx.monomorphize(operand.ty(&ctx.body().local_decls, ctx.tcx()));
            let src = ctx.type_from_cache(src);
            let mut ops = handle_operand(operand, ctx);
            // `f16` has no native CIL float type, so its conversions are routed through
            // `System.Half`'s explicit conversion operators instead of the `conv.r*` opcodes
            // (the `FloatCast` IR node `todo!()`s on `f16` in every exporter).
            // `f128` has no .NET equivalent (it would need softfloat emulation); leave it deferred.
            match (src, target) {
                // f16 -> {f32, f64}
                (Type::Float(Float::F16), Type::Float(dst @ (Float::F32 | Float::F64))) => {
                    ops = cilly::ir::builtins::f16::f16_to_float(ctx, ops, dst);
                }
                // {f32, f64} -> f16
                (Type::Float(src @ (Float::F32 | Float::F64)), Type::Float(Float::F16)) => {
                    ops = cilly::ir::builtins::f16::float_to_f16(ctx, ops, src);
                }
                // f16 -> f16 is a no-op.
                (Type::Float(Float::F16), Type::Float(Float::F16)) => (),
                (Type::Float(Float::F128), _) | (_, Type::Float(Float::F128)) => {
                    todo!("f128 FloatToFloat casts are unsupported: .NET has no quadruple-precision float type, so this needs softfloat emulation (src:{src:?} target:{target:?})")
                }
                (_, Type::Float(Float::F32)) => ops = ctx.float_cast(ops, Float::F32, true),
                (_, Type::Float(Float::F64)) => ops = ctx.float_cast(ops, Float::F64, true),
                _ => panic!("Can't preform a FloatToFloat cast to type {target:?}"),
            }
            (vec![], ops)
        }
        Rvalue::Cast(
            CastKind::PointerCoercion(PointerCoercion::ReifyFnPointer(_), _),
            operand,
            target,
        ) => {
            let operand_ty = operand.ty(ctx.body(), ctx.tcx());
            operand
                .constant()
                .expect("function must be constant in order to take its address!");
            let operand_ty = ctx.monomorphize(operand_ty);

            let (instance, _subst_ref) = if let TyKind::FnDef(def_id, subst_ref) = operand_ty.kind()
            {
                let subst = ctx.monomorphize(*subst_ref);
                let env = rustc_middle::ty::TypingEnv::fully_monomorphized();
                let Some(instance) = Instance::try_resolve(ctx.tcx(), env, *def_id, subst)
                    .expect("Invalid function def")
                else {
                    panic!("ERROR: Could not get function instance. fn type:{operand_ty:?}")
                };
                (instance, subst_ref)
            } else {
                todo!("Trying to call a type which is not a function definition!");
            };
            let function_name = fn_name(ctx.tcx().symbol_name(instance));
            let function_sig = crate::function_sig::sig_from_instance_(instance, ctx)
                .expect("Could not get function signature when trying to get a function pointer!");
            //FIXME: properly handle `#[track_caller]`
            let call_site = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string(function_name),
                ctx.alloc_sig(function_sig),
                MethodKind::Static,
                vec![].into(),
            );
            // The destination type is a bare `fn`-pointer type built receiver-free (`from_poly_sig`),
            // so it may have fewer params than the physical method's keep-ZST signature. Reconcile
            // arity via the adapter-thunk helper (a no-op fast path when the sigs already agree).
            let target_type = ctx.type_from_cache(*target);
            if let Type::FnPtr(target_sig) = target_type {
                (vec![], ctx.reify_fnptr(call_site, target_sig))
            } else {
                // Defensive: the destination is not a fn-ptr type (should not happen for
                // ReifyFnPointer). Fall back to taking the method's address directly.
                let m = ctx.alloc_methodref(call_site);
                (vec![], ctx.ld_ftn(m))
            }
        }

        Rvalue::Discriminant(place) => {
            let addr = place_address(place, ctx);
            let owner_ty = ctx.monomorphize(place.ty(ctx.body(), ctx.tcx()).ty);
            let owner = ctx.type_from_cache(owner_ty);

            let layout = ctx.layout_of(owner_ty);
            let target = ctx.type_from_cache(owner_ty.discriminant_ty(ctx.tcx()));
            let (disrc_type, _) = enum_tag_info(layout.layout, ctx);
            let Type::ClassRef(owner) = owner else {
                // ZST enum (e.g. `Result<Infallible, ()>` — uninhabited `Ok`, ZST `Err`
                // payload): no in-memory discriminant. It is a `Variants::Single` layout,
                // so the discriminant is the constant discriminant of the sole inhabited
                // variant. A hardcoded 0 misrouted such values into the layout's first
                // variant (the uninhabited one) and aborted in the dead arm.
                return (vec![], single_variant_discr(owner_ty, target, ctx));
            };

            if disrc_type == Type::Void {
                // Tag-less but sized enum (one inhabited variant carrying a payload, e.g.
                // `Result<Infallible, i32>` where `Ok` is uninhabited): same as the ZST
                // case — the discriminant is the constant discriminant of that one
                // inhabited variant, not a hardcoded 0. Returning 0 made a generic `match`
                // monomorphized for `Result<Infallible, _>` read `Ok` and walk into the
                // uninhabited arm.
                (vec![], single_variant_discr(owner_ty, target, ctx))
            } else {
                let discr = get_discr(layout.layout, addr, owner, owner_ty, ctx);
                (
                    vec![],
                    crate::casts::int_to_int(disrc_type, target, discr, ctx),
                )
            }
        }
        Rvalue::Repeat(operand, times) => repeat(rvalue, ctx, operand, *times, dst_place),
        Rvalue::ThreadLocalRef(def_id) => {
            if !def_id.is_local() && ctx.tcx().needs_thread_local_shim(*def_id) {
                // Cross-crate `#[thread_local]` shim. UNREACHABLE from safe/stable Rust on the
                // .NET target: `needs_thread_local_shim` requires `is_thread_local_static` (the
                // unstable `#![feature(thread_local)]` attribute) AND a cross-crate (`!is_local`)
                // access. std's stable `thread_local!` macro routes through the runtime-key PAL
                // arm, never this native `ThreadLocalRef` path. Lowering the shim would mean
                // emitting a call to the synthesised `InstanceKind::ThreadLocalShim`, which the
                // .NET TLS model does not yet support.
                todo!(
                    "Cross-crate `#[thread_local]` shim for {def_id:?} is unsupported on the .NET \
                     target (requires unstable `#![feature(thread_local)]` + cross-crate access; \
                     std's stable `thread_local!` uses the runtime-key PAL arm instead)."
                )
            } else {
                let alloc_id = ctx.tcx().reserve_and_set_static_alloc(*def_id);
                let rvalue_ty = rvalue.ty(ctx.body(), ctx.tcx());
                let rvalue_type = ctx.type_from_cache(rvalue_ty);
                let tpe = ctx.alloc_type(rvalue_type);
                let ptr = add_allocation(alloc_id.0.into(), ctx, tpe);
                let tpe = ctx[tpe].pointed_to().unwrap();
                (vec![], ctx.cast_ptr(ptr, tpe))
            }
        }
        Rvalue::Cast(rustc_middle::mir::CastKind::FnPtrToPtr, operand, target) => {
            let target = ctx.type_from_cache(*target);
            let val = handle_operand(operand, ctx);
            (vec![], ctx.cast_ptr_to(val, target))
        }
        // A `Subtype` cast is a representation-preserving coercion (subtyping — e.g. variance or
        // higher-ranked-lifetime adjustments). It never changes the value's layout, so forward the
        // operand unchanged.
        Rvalue::Cast(rustc_middle::mir::CastKind::Subtype, operand, _target) => {
            (vec![], handle_operand(operand, ctx))
        }
    }
}
const SIMPLE_REPEAT_CAP: u64 = 16;
fn repeat<'tcx>(
    rvalue: &Rvalue<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    element: &Operand<'tcx>,
    times: rustc_middle::ty::Const<'tcx>,
    dst_place: &Place<'tcx>,
) -> (Vec<Root>, Node) {
    // Get the type of the operand
    let element_ty = ctx.monomorphize(element.ty(ctx.body(), ctx.tcx()));
    let element_type = ctx.type_from_cache(element_ty);
    let element = handle_operand(element, ctx);
    // Array size
    let times = ctx.monomorphize(times);
    let times = times
        .try_to_target_usize(ctx.tcx())
        .expect("Could not evalute array size as usize.");
    // Array type
    let array = ctx.monomorphize(rvalue.ty(ctx.body(), ctx.tcx()));
    let array = ctx.type_from_cache(array);
    let array_dotnet = array.clone().as_class_ref().expect("Invalid array type.");
    // Check if the element is byte sized. If so, use initblk to quickly initialize this array.
    if const_sizeof(element_ty, ctx.tcx()) == 1 {
        let place_address = place_address(dst_place, ctx);
        let val = ctx.transmute_on_stack(element_type, Type::Int(Int::U8), element);
        let u8_ptr = ctx.nptr(Type::Int(Int::U8));
        let dst = ctx.cast_ptr(place_address, u8_ptr);
        let count = ctx.alloc_node(Const::USize(times));
        let init = ctx.init_blk(dst, val, count);
        return (vec![init], place_get(dst_place, ctx));
    }
    // Check if there are more than SIMPLE_REPEAT_CAP elements. If so, use mecmpy to accelerate initialzation
    if times > SIMPLE_REPEAT_CAP {
        let place_address = place_address(dst_place, ctx);
        let mut branches = Vec::new();
        let arr_ref = ctx.nref(array);
        let mref = MethodRef::new(
            array_dotnet,
            ctx.alloc_string("set_Item"),
            ctx.sig([arr_ref, Type::Int(Int::USize), element_type], Type::Void),
            MethodKind::Instance,
            vec![].into(),
        );
        let mref = ctx.alloc_methodref(mref);
        for idx in 0..SIMPLE_REPEAT_CAP {
            let idx = ctx.alloc_node(Const::USize(idx));
            let root = ctx.call_root(mref, &[place_address, idx, element], IsPure::NOT);
            branches.push(root);
        }
        let mut curr_len = SIMPLE_REPEAT_CAP;
        while curr_len < times {
            // Copy curr_len elements if possible, otherwise this is the last iteration, so copy the reminder.
            let curr_copy_size = curr_len.min(times - curr_len);
            let elem_size: Node = ctx.size_of(element_type);
            // Copy curr_copy_size elements from the start of the array, starting at curr_len(the amount of already initialized buffers)
            let base = ctx.ref_to_ptr(place_address);
            let stride = ctx.int_cast(elem_size, Int::USize, ExtendKind::ZeroExtend);
            let cl = ctx.alloc_node(Const::USize(curr_len));
            let off = ctx.biop(cl, stride, BinOp::Mul);
            let dst = ctx.biop(base, off, BinOp::Add);
            let stride2 = ctx.int_cast(elem_size, Int::USize, ExtendKind::ZeroExtend);
            let cs = ctx.alloc_node(Const::USize(curr_copy_size));
            let len = ctx.biop(cs, stride2, BinOp::Mul);
            let root = ctx.cp_blk(dst, place_address, len);
            branches.push(root);
            curr_len *= 2;
        }
        (branches, place_get(dst_place, ctx))
    } else {
        let mut branches = Vec::new();
        let arr_ref = ctx.nref(array);
        let mref = MethodRef::new(
            array_dotnet,
            ctx.alloc_string("set_Item"),
            ctx.sig([arr_ref, Type::Int(Int::USize), element_type], Type::Void),
            MethodKind::Instance,
            vec![].into(),
        );
        let place_address = place_address(dst_place, ctx);
        let mref = ctx.alloc_methodref(mref);
        for idx in 0..times {
            let idx = ctx.alloc_node(Const::USize(idx));
            let root = ctx.call_root(mref, &[place_address, idx, element], IsPure::NOT);
            branches.push(root);
        }
        (branches, place_get(dst_place, ctx))
    }
}
fn ptr_to_ptr<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand: &Operand<'tcx>,
    dst: Ty<'tcx>,
) -> Node {
    let target = ctx.monomorphize(dst);
    let target_pointed_to = match target.kind() {
        TyKind::RawPtr(typ, _) => typ,
        TyKind::Ref(_, inner, _) => inner,
        _ => panic!("Type is not ptr {target:?}."),
    };
    let source = ctx.monomorphize(operand.ty(ctx.body(), ctx.tcx()));
    let source_pointed_to = match source.kind() {
        TyKind::RawPtr(typ, _) => *typ,
        TyKind::Ref(_, inner, _) => *inner,
        _ => panic!("Type is not ptr {target:?}."),
    };
    let source_type = ctx.type_from_cache(source);
    let target_type = ctx.type_from_cache(target);

    let src_fat = ptr_is_fat(source_pointed_to, ctx.tcx(), ctx.instance());
    let target_fat = ptr_is_fat(*target_pointed_to, ctx.tcx(), ctx.instance());
    match (src_fat, target_fat) {
        (true, true) => {
            let val = handle_operand(operand, ctx);
            ctx.transmute_on_stack(source_type, target_type, val)
        }
        (true, false) => {
            let field_desc = FieldDesc::new(
                get_type(source, ctx).as_class_ref().unwrap(),
                ctx.alloc_string(crate::DATA_PTR),
                ctx.nptr(cilly::Type::Void),
            );
            let desc = ctx.alloc_field(field_desc);
            let addr = operand_address(operand, ctx);
            let data = ctx.ld_field(addr, desc);
            ctx.cast_ptr_to(data, target_type)
        }
        (false, true) => {
            panic!("ERROR: a non-unsizing cast turned a sized ptr into an unsized one")
        }
        _ => {
            let val = handle_operand(operand, ctx);
            ctx.cast_ptr_to(val, target_type)
        }
    }
}
