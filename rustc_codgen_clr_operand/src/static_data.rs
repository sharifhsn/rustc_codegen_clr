

use crate::constant::{get_vtable, static_ty};
use cilly::{
    Access, CILRoot, Const, FnSig, Int, Interned, MethodDef, MethodDefIdx, MethodRef,
    StaticFieldDesc, Type,
    cilnode::MethodKind,
    utilis::encode,
    ir::{BasicBlock, CILNode},
};

type Root = Interned<cilly::ir::CILRoot>;
use rustc_codegen_clr_call::CallInfo;
pub use rustc_codegen_clr_ctx::MethodCompileCtx;
use rustc_codegen_clr_ctx::fn_name;
use rustc_codegen_clr_type::{GetTypeExt, align_of, r#type::fixed_array};
use rustc_hir::def::DefKind;
use rustc_middle::{
    mir::interpret::{AllocId, Allocation, GlobalAlloc},
    ty::{Instance, List, TyCtxt, TypingEnv},
};
use rustc_span::def_id::DefId;

/// A `static X: &[T] = &[..]` (and similar `&[..]`-valued statics) lifts its array
/// literal into an *anonymous nested static*: `DefKind::Static { nested: true, .. }`.
/// Such a static is **untyped** — its HIR owner node is `Node::Synthetic`, so
/// `tcx.type_of(def_id)` has no valid arm and ICEs with
/// `unexpected sort of node in type_of(): Synthetic`. We must therefore obtain its
/// storage size/align from the allocation itself, never from `type_of`. This mirrors
/// rustc's own `GlobalAlloc::size_and_align`, which branches on exactly this flag.
fn static_is_nested(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    matches!(tcx.def_kind(def_id), DefKind::Static { nested: true, .. })
}

/// Build the .NET storage `Type` for a static's backing field directly from its
/// evaluated allocation's real `len` + `align`, with no `type_of` query. Used for
/// nested/anonymous statics (whose `type_of` would ICE) and as the type-of-free
/// fallback. The shape mirrors the `add_allocation` Memory-arm blob: a fixed-size
/// array of the largest integer the alignment guarantees, sized to cover the bytes.
fn nested_static_blob_type(alloc: &Allocation, ctx: &mut MethodCompileCtx<'_, '_>) -> Type {
    let align = alloc.align.bytes().max(1);
    let elem = match align {
        ..1 => Int::U8,
        ..2 => Int::U16,
        ..4 => Int::U32,
        _ => Int::U64,
    };
    let elem_size = elem.size().unwrap_or(8) as u64;
    let len = alloc.len() as u64;
    if len == 0 {
        return Type::Void;
    }
    let blob_arr = fixed_array(
        ctx,
        Type::Int(elem),
        len.div_ceil(elem_size),
        len.next_multiple_of(elem_size),
        elem_size,
    );
    Type::ClassRef(blob_arr)
}

pub fn add_static(def_id: DefId, ctx: &mut MethodCompileCtx<'_, '_>) -> Interned<CILNode> {
    let main_module_id = ctx.main_module();
    let attrs = ctx.tcx().codegen_fn_attrs(def_id);

    let thread_local = attrs
        .flags
        .contains(rustc_middle::middle::codegen_fn_attrs::CodegenFnAttrFlags::THREAD_LOCAL);
    // An anonymous nested static (`static X: &[T] = &[..]` lifts its `&[..]` into one)
    // is untyped: `tcx.type_of` has no arm for its `Node::Synthetic` owner and ICEs.
    // For that case derive the backing-field type from the evaluated allocation's real
    // len+align (the type_of-free path rustc itself uses in `GlobalAlloc::size_and_align`),
    // never from `static_ty`. Top-level named statics keep the exact existing behaviour.
    let nested = static_is_nested(ctx.tcx(), def_id);
    // `eval_static_initializer` is valid for nested statics too (it asserts only
    // `is_static`, and we need the alloc for the type below as well as for Phase 2).
    let alloc = ctx.tcx().eval_static_initializer(def_id).unwrap();
    let tpe = if nested {
        nested_static_blob_type(&alloc.0, ctx)
    } else {
        let ty = static_ty(def_id, ctx.tcx());
        assert!(ty.is_sized(ctx.tcx(), TypingEnv::fully_monomorphized()));
        let tpe = ctx.type_from_cache(ty);
        // Cross-check the alloc's align against the type's (named statics only; a nested
        // static has no type to compare and is the whole reason this branch is split).
        assert_eq!(alloc.0.align.bytes().max(1), align_of(ty, ctx.tcx()));
        tpe
    };
    let symbol: String = ctx
        .tcx()
        .symbol_name(Instance::new_raw(def_id, List::empty()))
        .to_string();

    // PHASE 1 — reserve the static field BEFORE evaluating the initializer or
    // recursing into its provenance. Statics can be mutually (or self-)
    // referential: a static A whose initializer's provenance points at static B
    // (and B back at A, or A at itself). The initializer for A reaches B through
    // `allocation_initializer_method` -> `add_allocation`'s `GlobalAlloc::Static`
    // arm -> `add_static(B)`, which can come straight back to `add_static(A)`.
    // Without an idempotency guard this recurses forever and overflows rustc's
    // C stack (a hard SIGSEGV that bypasses the per-item `catch_unwind` recovery
    // and fails the whole crate). `add_allocation`'s Memory arm already memoizes
    // on `has_static_field`; mirror that here, keyed on the static's symbol/type,
    // so the field is present (and the recursion short-circuits) before the
    // initializer is built. The static-fields list on the persistent `Assembly`
    // is the memo — `MethodCompileCtx` is per-method and is even re-created
    // inside `allocation_initializer_method`, so a visited-set must not live on
    // the ctx.
    let name = ctx.alloc_string(symbol.clone());
    let present = ctx.class_mut(main_module_id).has_static_field(name, tpe);
    let sfld = ctx.add_static(
        tpe,
        symbol.clone(),
        thread_local,
        main_module_id,
        None,
        false,
    );
    let ptr = ctx.alloc_node(CILNode::LdStaticFieldAddress(sfld));
    let ptr = ctx.cast_ptr(ptr, Int::U8);
    if present {
        // The field (and its initializer) were registered by an earlier call;
        // return the same U8-ptr address node every call site expects, without
        // re-evaluating the initializer or recursing again.
        return ptr;
    }

    // PHASE 2 — build and register the initializer exactly once. Recursion back
    // into `add_static(def_id)` from here now hits the `already_present`
    // short-circuit above. The allocation was evaluated above (it determines the
    // field type for nested statics and is cross-checked for named ones).
    let initialzer = allocation_initializer_method(&alloc.0, &symbol, ctx, ptr, true);
    let root = ctx.alloc_root(cilly::CILRoot::call(*initialzer, []));

    if thread_local {
        ctx.add_tcctor(&[root]);
    } else {
        ctx.add_cctor(&[root]);
    }

    ptr
}
fn alloc_default_type(alloc_id: u64, ctx: &mut MethodCompileCtx<'_, '_>) -> Type {
    let alloc = match ctx
        .tcx()
        .global_alloc(AllocId(alloc_id.try_into().expect("0 alloc id?")))
    {
        GlobalAlloc::Memory(alloc) => alloc,
        GlobalAlloc::Static(def_id) => {
            // A nested/anonymous static (`static X: &[T] = &[..]`) is untyped — its
            // `type_of` ICEs (`Node::Synthetic`). Derive the type from the allocation
            // instead, the way `add_static` and the Memory arm already do; only
            // top-level named statics have a `type_of` to use.
            if static_is_nested(ctx.tcx(), def_id) {
                let alloc = ctx.tcx().eval_static_initializer(def_id).unwrap();
                return nested_static_blob_type(&alloc.0, ctx);
            }
            return ctx.type_from_cache(static_ty(def_id, ctx.tcx()));
        }
        // A VTable alloc has no readable backing memory (`Size::ZERO`); its slots
        // (drop-glue ptr, size, align, method ptrs) are materialized by codegen, not
        // copied from raw bytes. The matching `add_allocation` arm emits a single
        // pointer-sized static and ignores the type returned here, so a pointer-sized
        // type is all that is required to keep the relocation path well-typed.
        GlobalAlloc::VTable(..) => return Type::Int(Int::USize),
        // A function alloc materializes a function pointer (pointer-sized). The
        // function-provenance branch of `allocation_initializer_method` intercepts
        // these before this is reached; this arm only exists so a stray
        // function-typed relocation target can no longer ICE.
        GlobalAlloc::Function { .. } => return Type::Int(Int::USize),
        // A `TypeId` alloc is opaque: it has no backing memory. The pointer's
        // *offset* is one pointer-sized segment of the 128-bit type-id hash, and
        // that value is already present in the raw allocation bytes. We therefore
        // give it a zero-sized type and (in `allocation_initializer_method`) skip
        // the relocation patch, leaving the raw hash fragment in place. See the
        // `GlobalAlloc::TypeId` arm there for the rationale.
        GlobalAlloc::TypeId { .. } => return Type::Void,
    };
    let tpe = match alloc.0.0.align.bytes() {
        ..1 => Int::U8,
        ..2 => Int::U16,
        ..4 => Int::U32,
        ..8 => Int::U64,
        _ => {
            ctx.tcx().dcx().span_warn(
                ctx.span(),
                format!(
                    "Alloc of align {} required, but that can't be guranteed!",
                    alloc.0.0.align.bytes()
                ),
            );
            Int::U64
        }
    };
    let arr_size = alloc.0.len() as u64;
    if arr_size == 0 {
        return Type::Void;
    }
    let size = tpe.size().unwrap_or(8) as u64;
    let tpe = fixed_array(
        ctx,
        Type::Int(tpe),
        arr_size.div_ceil(size),
        arr_size.next_multiple_of(size),
        tpe.size().unwrap_or(8) as u64,
    );
    Type::ClassRef(tpe)
}
/// Returns a pointer to the backing buffer of a const-allocation static, rounded up to `align` at
/// runtime when the allocation is over-aligned (`over_aligned == align > elem_size`).
///
/// .NET does not guarantee >8-byte alignment for value-type *static* fields, so an over-aligned
/// const allocation (a `#[repr(align(N>8))]` value, or the `const_allocate(_, 64)` metadata buffer
/// behind `ThinBox::<dyn>::new_unsize_zst`) has its field over-allocated by `align` bytes (see
/// `add_allocation`) and the usable buffer base is `align_up(field_addr, align)`. Computing it here
/// — once, deterministically from the fixed static address — keeps the initializer's writes and
/// every consumer's reads in agreement. For the common `align <= elem_size` case the field address
/// is already adequately aligned, so it is returned unchanged.
fn aligned_static_buf(
    ctx: &mut MethodCompileCtx<'_, '_>,
    field_desc: StaticFieldDesc,
    align: u64,
    over_aligned: bool,
) -> Interned<CILNode> {
    let base = ctx.static_addr(field_desc);
    if !over_aligned {
        return base;
    }
    // align_up(p, a) == (p + (a - 1)) & !(a - 1), computed in usize then cast back to *u8.
    let base_int = ctx.cast_ptr_to(base, Type::Int(Int::USize));
    let added = ctx.biop(base_int, Const::USize(align - 1), cilly::BinOp::Add);
    let aligned = ctx.biop(added, Const::USize(!(align - 1)), cilly::BinOp::And);
    let u8_ptr = ctx.nptr(Int::U8);
    ctx.cast_ptr_to(aligned, u8_ptr)
}
/// Adds a static field and initialized for allocation represented by `alloc_id`.
pub fn add_allocation(
    alloc_id: u64,
    ctx: &mut MethodCompileCtx<'_, '_>,
    tpe: Interned<Type>,
) -> Interned<CILNode> {
    // `tpe` is the caller's *expected* type for the allocation, but it is frequently
    // pointer-sized (a `&T` const, or the `USize` `alloc_default_type` hands back for
    // VTable/Function reloc targets) and would under-size the backing field. The Memory
    // arm below now derives the field's storage type from the allocation's real len+align
    // instead, so this hint is intentionally unused. Kept in the signature for the stable
    // API and to keep call sites self-documenting.
    let _ = tpe;
    let main_module_id = ctx.main_module();
    let const_alloc = match ctx
        .tcx()
        .global_alloc(AllocId(alloc_id.try_into().expect("0 alloc id?")))
    {
        GlobalAlloc::Memory(alloc) => alloc,
        GlobalAlloc::Static(def_id) => return add_static(def_id, ctx),
        GlobalAlloc::VTable(..) => {
            // Resolve the symbolic VTable alloc into the real vtable blob exactly as
            // `load_scalar_ptr`'s VTable arm (constant.rs) does. `get_vtable` queries
            // `tcx.vtable_allocation`, which returns a *separate* `GlobalAlloc::Memory`
            // alloc holding the actual vtable bytes (drop-glue/size/align/method ptrs);
            // its Memory arm materializes that blob and its reloc loop patches the method
            // pointers. The returned node is the ADDRESS of that blob = the correct vtable
            // pointer. Previously this arm registered an UNINITIALIZED null `v_{id}` static
            // and returned its (null) *value*, silently null-ing the vtable field of any
            // `static OBJ: &dyn T = &S;` and faulting at first virtual dispatch. Delegating
            // to the shared `get_vtable` eliminates that drift (one resolver, both paths).
            let global_alloc = ctx
                .tcx()
                .global_alloc(AllocId(alloc_id.try_into().expect("0 alloc id?")));
            let (ty, polyref) = global_alloc.unwrap_vtable();
            return get_vtable(
                ctx,
                ty,
                polyref
                    .map(|principal| ctx.tcx().instantiate_bound_regions_with_erased(principal)),
            );
        }
        GlobalAlloc::Function { instance } => {
            // Defensive: `allocation_initializer_method`'s Function-provenance branch
            // intercepts function relocations before they reach here, so for reloc-walking
            // this arm is dead. But `add_allocation` is also a public entry point, so resolve
            // a function alloc to a real fn-ptr (mirroring `load_scalar_ptr`'s Function arm in
            // constant.rs) instead of returning a null `f_{id}` static, closing the latent
            // null for any direct `add_allocation(Function)` caller.
            let call_info = CallInfo::sig_from_instance_(instance, ctx);
            let function_name = fn_name(ctx.tcx().symbol_name(instance));
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string(function_name),
                ctx.alloc_sig(call_info.sig().clone()),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            return ctx.alloc_node(CILNode::LdFtn(mref));
        }
        // A `TypeId` alloc has no backing memory: the pointer's *offset* is the
        // type-id hash fragment and equality only requires it be self-consistent.
        // Use a zero base, so the materialized pointer value equals the offset
        // (the hash fragment). In practice the reloc loop in
        // `allocation_initializer_method` short-circuits the TypeId case before
        // reaching here, but keep this for any other caller.
        GlobalAlloc::TypeId { .. } => return ctx.alloc_node(Const::USize(0)),
    };

    let const_alloc = const_alloc.inner();

    let bytes: &[u8] =
        const_alloc.inspect_with_uninit_and_ptr_outside_interpreter(0..const_alloc.len());
    let align = const_alloc.align.bytes().max(1);
    if const_alloc.len() == 0 {
        return ctx.alloc_node(Const::USize(align));
    }
    // Check if const literal can be used
    if const_alloc.provenance().ptrs().is_empty() && align <= 1 {
        return ctx.bytebuffer(bytes, Int::U8);
    }
    // Alloc ids are *not* unique across all crates. Adding the hash here ensures we don't overwrite allocations during linking
    // TODO:consider using something better here / making the hashes stable.
    let byte_hash = calculate_hash(&bytes);
    match (align, bytes.len()) {
        _ => {
            // The initializer `cpblk`s the *full* `const_alloc.len()` bytes (see
            // `allocation_initializer_method`), so the backing field MUST be at least
            // that large. The caller-supplied `tpe` is frequently pointer-sized — e.g. a
            // `&T` const, or the `USize` that `alloc_default_type` returns for the
            // `VTable`/`Function` relocation-target arms — which would under-size the
            // field and let the `cpblk` overrun into whatever static the linker happens
            // to place next (the corrupted `DECIMAL_PAIRS` -> `__fmt_inner`
            // NullReferenceException). Build a correctly-sized blob value-type from the
            // allocation's real len+align and declare the field with *that*, ignoring the
            // (possibly undersized) incoming `tpe`. Every consumer `cast_ptr`s the address
            // we return, so widening the storage type is transparent — only the size grows
            // to be correct.
            let elem = match align {
                ..1 => Int::U8,
                ..2 => Int::U16,
                ..4 => Int::U32,
                _ => Int::U64,
            };
            let elem_size = elem.size().unwrap_or(8) as u64;
            let len = const_alloc.len() as u64;
            // .NET does NOT guarantee >8-byte alignment for a value-type *static* field (a
            // `[FieldOffset]`/classlayout `.pack` controls *instance* layout, not where the runtime
            // places the static's storage), so an OVER-aligned const allocation — e.g. a
            // `#[repr(align(64))]` value, or the `const_allocate(_, 64)` metadata buffer that
            // `ThinBox::<dyn>::new_unsize_zst` const-makes-global for a 64-aligned ZST — cannot rely
            // on the field landing 64-aligned (it lands ~8-aligned, silently corrupting any pointer
            // round-trip that asserts alignment — the ThinBox `verify_aligned` 32/8-vs-64 failure).
            // Fix it at RUNTIME: when `align > elem_size`, over-allocate the field by `align` bytes
            // and return an interior pointer rounded up to `align` (see `aligned_static_buf`). For
            // the overwhelmingly common `align <= 8` case nothing changes — no padding, no rounding.
            let over_aligned = align > elem_size;
            let pad = if over_aligned { align } else { 0 };
            let arr_size = (len + pad).next_multiple_of(elem_size);
            let blob_arr = fixed_array(
                ctx,
                Type::Int(elem),
                arr_size / elem_size,
                arr_size,
                elem_size,
            );
            let field_tpe = Type::ClassRef(blob_arr);
            let field_tpe_idx = ctx.alloc_type(field_tpe);
            // Content-based dedup of READ-ONLY (immutable) allocations: identical immutable
            // allocations must share ONE backing static so `ptr::eq` on two references to the same
            // promoted const holds — most visibly `Waker::will_wake`, which compares the addresses
            // of two `RawWakerVTable` promotions (the `Waker::from`/`clone_waker` sites get distinct
            // `AllocId`s for the same const). Native does this via LLVM merging identical read-only
            // `unnamed_addr` globals; Rust permits it (const/promoted addresses are NOT guaranteed
            // distinct). Naming a read-only alloc by its CONTENT (bytes + align + len + relocation
            // targets) instead of its `AllocId` lets the linker's merge-by-name collapse the
            // duplicates. The relocation targets are part of the fingerprint so two byte-identical
            // allocations pointing at DIFFERENT functions/statics never wrongly merge. MUTABLE
            // statics must stay distinct, so they keep the unique `AllocId` in the name.
            let alloc_name = if const_alloc.mutability == rustc_middle::mir::Mutability::Not {
                let relocs: Vec<(u32, u64)> = const_alloc
                    .provenance()
                    .ptrs()
                    .iter()
                    .map(|(off, prov)| (off.bytes_usize() as u32, prov.alloc_id().0.get()))
                    .collect();
                let content_hash = calculate_hash(&(bytes, align, const_alloc.len(), relocs));
                format!(
                    "ro_{}_{}_{}",
                    encode(content_hash),
                    encode(field_tpe_idx.inner().into()),
                    const_alloc.len()
                )
            } else {
                format!(
                    "al_{}_{}_{}_{}",
                    encode(alloc_id),
                    encode(byte_hash),
                    encode(field_tpe_idx.inner().into()),
                    const_alloc.len()
                )
            };
            let name = ctx.alloc_string(alloc_name.clone());
            let field_desc = StaticFieldDesc::new(*ctx.main_module(), name, field_tpe);
            // Currently, all static fields are in one module. Consider spliting them up.

            let main_module = ctx.class_mut(main_module_id);

            if main_module.has_static_field(name, field_desc.tpe()) {
                return aligned_static_buf(ctx, field_desc, align, over_aligned);
            }
            ctx.add_static(field_tpe, &*alloc_name, false, main_module_id, None, false);

            // The runtime-aligned interior pointer is the canonical buffer base: the initializer
            // writes (and patches relocations) at it, and every consumer reads from it, so the two
            // always agree. `Interned` is `Copy`, so we reuse `buf` for both the init and the return.
            let buf = aligned_static_buf(ctx, field_desc, align, over_aligned);
            let ptr = ctx.cast_ptr(buf, Int::U8);

            let initialzer: MethodDefIdx =
                allocation_initializer_method(const_alloc, &alloc_name, ctx, ptr.into(), true);

            // Calls the static initialzer, and sets the static field to the returned pointer.
            let root = ctx.alloc_root(cilly::CILRoot::call(*initialzer, []));
            ctx.add_cctor(&[root]);

            buf
        }
    }
}
pub fn add_const_value(asm: &mut cilly::Assembly, bytes: u128) -> StaticFieldDesc {
    let uint8_ptr = Type::Int(Int::U128);
    let main_module_id = asm.main_module();
    let alloc_fld = format!("a_{bytes:x}");
    let alloc_fld_name = asm.alloc_string(alloc_fld.clone());

    let field_desc = StaticFieldDesc::new(*asm.main_module(), alloc_fld_name, Type::Int(Int::U128));

    let main_module = asm.class_mut(main_module_id);
    if main_module.has_static_field(alloc_fld_name, field_desc.tpe()) {
        return field_desc;
    }
    asm.add_static(uint8_ptr, alloc_fld, false, main_module_id, None, false);

    let field = asm.alloc_sfld(field_desc);
    let val = asm.alloc_node(Const::U128(bytes));
    let set = asm.alloc_root(cilly::CILRoot::SetStaticField { field, val });

    asm.add_cctor(&[set]);

    field_desc
}
fn allocation_initializer_method(
    const_allocation: &Allocation,
    name: &str,
    ctx: &mut MethodCompileCtx<'_, '_>,
    ptr: Interned<CILNode>,
    void_ret: bool,
) -> MethodDefIdx {
    let bytes: &[u8] =
        const_allocation.inspect_with_uninit_and_ptr_outside_interpreter(0..const_allocation.len());
    let ptrs = const_allocation.provenance().ptrs();
    let mut trees: Vec<Root> = Vec::new();

    // Emit the static-initialization roots directly.
    // STLoc(0, ptr)
    trees.push(ctx.alloc_root(CILRoot::StLoc(0, ptr)));
    // CpBlk(dst = LdLoc(0), src = bytebuffer, len = const)
    {
        let dst = ctx.alloc_node(CILNode::LdLoc(0));
        let src = ctx.bytebuffer(bytes, Int::U8);
        let len = ctx.alloc_node(Const::USize(bytes.len() as u64));
        let cpblk = ctx.cp_blk(dst, src, len);
        trees.push(cpblk);
    }

    if !ptrs.is_empty() {
        for (offset, prov) in ptrs.iter() {
            let offset = u32::try_from(offset.bytes_usize()).unwrap();
            // Check if this allocation is a function
            let target_alloc = ctx.tcx().global_alloc(prov.alloc_id());
            // `TypeId` provenance is opaque and has no real address: the pointer's
            // offset (already written into the raw bytes copied above by `CpBlk`) is
            // a segment of the 128-bit type-id hash. Leaving the raw bytes in place
            // (base address 0 + offset == hash fragment) keeps `TypeId::of::<T>()`
            // self-consistent for equality, which is all the program can observe.
            if matches!(target_alloc, GlobalAlloc::TypeId { .. }) {
                continue;
            }
            if let GlobalAlloc::Function {
                instance: finstance,
            } = target_alloc
            {
                // If it is a function, patch its pointer up.
                let mut ctx = MethodCompileCtx::new(ctx.tcx(), None, finstance, ctx);
                let call_info = CallInfo::sig_from_instance_(finstance, &mut ctx);
                let keep_zst_sig = call_info.sig().clone();
                let function_name = fn_name(ctx.tcx().symbol_name(finstance));
                let mref = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string(function_name),
                    ctx.alloc_sig(keep_zst_sig.clone()),
                    MethodKind::Static,
                    vec![].into(),
                );
                // addr = (LdLoc(0) + offset) cast to *usize
                let ld_loc = ctx.alloc_node(CILNode::LdLoc(0));
                let off = ctx.alloc_node(Const::USize(offset.into()));
                let addr = ctx.biop(ld_loc, off, cilly::BinOp::Add);
                let usize_ptr = ctx.nptr(Type::Int(Int::USize));
                let addr = ctx.cast_ptr_to(addr, usize_ptr);
                // A const `fn`-pointer relocation must store a pointer whose CIL arity matches the
                // bare `fn`-pointer type it will be invoked through (`from_poly_sig`: receiver-free).
                // The physical method `mref` keeps every `fn_abi.args` entry, including the closure's
                // ZST/Ignore receiver, which is always the FIRST arg and is lowered to a `Type::Void`
                // (`RustVoid`) param. If we stored a plain `ldftn` of that method, a later indirect
                // `calli` through the narrower fn-ptr type would push too few args and the callee
                // would read a garbage extra slot (the TLS `LazyStorage::initialize` AccessViolation).
                // Strip only the leading `Void` receiver param(s) to form the receiver-free target
                // sig, then route through `reify_fnptr`, which emits an arity-matching adapter thunk
                // when (and only when) params were elided. A non-leading `Void` (a genuine ZST value
                // argument) is preserved, so regular `fn`-item pointers keep their exact arity and hit
                // `reify_fnptr`'s fast path (no adapter, identical to the previous behaviour).
                let inputs = keep_zst_sig.inputs();
                let lead_void = inputs.iter().take_while(|t| **t == Type::Void).count();
                let target_inputs: Vec<Type> = inputs[lead_void..].to_vec();
                let target_sig = ctx.alloc_sig(FnSig::new(target_inputs, *keep_zst_sig.output()));
                // val = LdFtn(adapter-or-method) cast to usize
                let ftn = ctx.reify_fnptr(mref, target_sig);
                let val = ctx.cast_ptr_to(ftn, Type::Int(Int::USize));
                trees.push(ctx.alloc_root(CILRoot::StInd(Box::new((
                    addr,
                    val,
                    Type::Int(Int::ISize),
                    false,
                )))));
            } else {
                let tpe = alloc_default_type(prov.alloc_id().0.into(), ctx);
                let tpe = ctx.alloc_type(tpe);
                let ptr_alloc = add_allocation(prov.alloc_id().0.into(), ctx, tpe);

                // A provenance pointer embedded in this static stores its offset INTO the
                // target allocation inline in the raw bytes (already copied by the `CpBlk`
                // above); the relocation itself only names the target's BASE. Recover that
                // inline addend and add it back — otherwise a `&OTHER_STATIC.field`
                // reference (or any interior pointer into another static) collapses to the
                // START of the target allocation. This was the encoding_rs single-byte-table
                // miscompile: every `&SINGLE_BYTE_DATA.<encoding>` read the FIRST field
                // (`ibm866`), so windows-1252 decoded as IBM866. Mirrors the scalar-pointer
                // offset handling in `constant.rs` (`create_const_from_data`).
                let ptr_size = ctx.tcx().data_layout.pointer_size().bytes_usize();
                let addend = {
                    let start = offset as usize;
                    let mut buf = [0u8; 8];
                    buf[..ptr_size].copy_from_slice(&bytes[start..start + ptr_size]);
                    u64::from_le_bytes(buf)
                };

                // addr = (LdLoc(0) + offset) cast to *usize
                let ld_loc = ctx.alloc_node(CILNode::LdLoc(0));
                let off = ctx.alloc_node(Const::USize(offset.into()));
                let addr = ctx.biop(ld_loc, off, cilly::BinOp::Add);
                let usize_ptr = ctx.nptr(Type::Int(Int::USize));
                let addr = ctx.cast_ptr_to(addr, usize_ptr);
                // val = (ptr_alloc base + inline addend) cast to usize
                let val = ctx.cast_ptr_to(ptr_alloc, Type::Int(Int::USize));
                let val = if addend != 0 {
                    let addend = ctx.alloc_node(Const::USize(addend));
                    ctx.biop(val, addend, cilly::BinOp::Add)
                } else {
                    val
                };
                trees.push(ctx.alloc_root(CILRoot::StInd(Box::new((
                    addr,
                    val,
                    Type::Int(Int::ISize),
                    false,
                )))));
            }
        }
    }
    if void_ret {
        trees.push(ctx.alloc_root(CILRoot::VoidRet));
    } else {
        let ld_loc = ctx.alloc_node(CILNode::LdLoc(0));
        trees.push(ctx.alloc_root(CILRoot::Ret(ld_loc)));
    }
    let uint8_ptr = ctx.nptr(Type::Int(Int::U8));
    let ret = if void_ret { Type::Void } else { uint8_ptr };
    let uint8_ptr_idx = ctx.alloc_type(uint8_ptr);
    let alloc_ptr_name = ctx.alloc_string("alloc_ptr");
    let sig = ctx.alloc_sig(FnSig::new([], ret));
    let main_module_id = ctx.main_module();
    let init_method = MethodDef::from_blocks(
        Access::Private,
        main_module_id,
        &format!("init_{name}"),
        sig,
        MethodKind::Static,
        vec![BasicBlock::new(trees, 0, None)],
        vec![(Some(alloc_ptr_name), uint8_ptr_idx)],
        vec![],
        ctx,
    );
    ctx.new_method(init_method)
}
fn calculate_hash<T: std::hash::Hash>(t: &T) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
