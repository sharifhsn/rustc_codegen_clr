use crate::assembly::MethodCompileCtx;
use crate::operand::handle_operand;
use crate::place::{place_address, place_set};
use crate::r#type::GetTypeExt;
use crate::r#type::adt::field_descrptor;
use cilly::{
    ClassRef, Int, Interned, MethodRef, Type,
    cilnode::{ExtendKind, IsPure, MethodKind},
};
use rustc_middle::mir::{Operand, Place};
use rustc_span::Spanned;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

pub fn xchg<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let interlocked = ClassRef::interlocked(ctx);
    // *T
    let dst = handle_operand(&args[0].node, ctx);
    // T
    let new = handle_operand(&args[1].node, ctx);

    debug_assert_eq!(
        args.len(),
        2,
        "The intrinsic `atomic_xchg` MUST take in exactly 3 argument!"
    );
    let src_type = ctx.monomorphize(args[1].node.ty(ctx.body(), ctx.tcx()));
    let src_type = ctx.type_from_cache(src_type);
    let uint8_ref = ctx.nref(Type::Int(Int::U8));
    let xchng = MethodRef::new(
        *ctx.main_module(),
        ctx.alloc_string("atomic_xchng_u8"),
        ctx.sig([uint8_ref, Type::Int(Int::U8)], Type::Int(Int::U8)),
        MethodKind::Static,
        vec![].into(),
    );
    match src_type {
        // On .NET 9 (`config::dotnet9()`), all sub-word ints fall through to the general
        // `Interlocked.Exchange(ref T, T)` arm below (native byte/sbyte/short/ushort overloads,
        // no masked-word emulation). U8 otherwise uses the dedicated `atomic_xchng_u8` builtin.
        Type::Int(Int::U8) if !crate::config::dotnet9() => {
            let xchng = ctx.alloc_methodref(xchng);
            let call = ctx.call(xchng, &[dst, new], IsPure::NOT);
            return place_set(destination, call, ctx);
        }
        Type::Ptr(_) => {
            let usize_ref = ctx.nref(Type::Int(Int::USize));
            let call_site = MethodRef::new(
                interlocked,
                ctx.alloc_string("Exchange"),
                ctx.sig([usize_ref, Type::Int(Int::USize)], Type::Int(Int::USize)),
                MethodKind::Static,
                vec![].into(),
            );
            let call_site = ctx.alloc_methodref(call_site);
            let usize_ptr = ctx.nref(Type::Int(Int::USize));
            let dst = ctx.cast_ptr_to(dst, usize_ptr);
            let new = ctx.int_cast(new, Int::USize, ExtendKind::ZeroExtend);
            let call = ctx.call(call_site, &[dst, new], IsPure::NOT);
            let call = ctx.cast_ptr_to(call, src_type);
            return place_set(destination, call, ctx);
        }
        Type::Int(int @ (Int::I8 | Int::U16 | Int::I16)) if !crate::config::dotnet9() => {
            // Sub-word exchange via a masked 32-bit CAS loop (see `emulate_subword_xchng`).
            // U8 keeps its existing `atomic_xchng_u8` builtin (handled above).
            // (On .NET 9 this arm is skipped → native `Interlocked.Exchange` below.)
            let width = int.size().expect("sub-word int has a known size");
            let src_ref = ctx.nref(src_type);
            let call = ctx.call_static(
                &format!("atomic_xchng{}_correct", width * 8),
                [src_ref, src_type],
                src_type,
                &[dst, new],
            );
            return place_set(destination, call, ctx);
        }
        // `bool` is a 1-byte value; on .NET 8 it can reuse the dedicated `atomic_xchng_u8`
        // builtin (the U8 arm above). The checker does NOT treat `Bool` as assignable to `U8`,
        // so bridge the byref/value/result explicitly across the Bool<->U8 boundary.
        // REACHABLE from 100% safe/stable Rust: `AtomicBool::swap`/`AtomicU8::swap` both lower to
        // `core::sync::atomic::atomic_swap` -> `intrinsics::atomic_xchg` (see
        // library/core/src/sync/atomic.rs). NOTE: `atomic_xchng_u8` (the U8 arm above, reused
        // here for Bool) is a plain volatile-ld/volatile-st with no CAS — it is NOT atomic against
        // a racing writer of the same byte on .NET 8 (lost-update race). This is a known, disclosed
        // residual; see docs/MEMORY_MODEL.md §7/§8. Do not reintroduce an "unreachable" claim here.
        Type::Bool if !crate::config::dotnet9() => {
            let xchng = ctx.alloc_methodref(xchng);
            let u8_ref = ctx.nref(Type::Int(Int::U8));
            let dst = ctx.cast_ptr_to(dst, u8_ref);
            let new = ctx.transmute_on_stack(Type::Bool, Type::Int(Int::U8), new);
            let call = ctx.call(xchng, &[dst, new], IsPure::NOT);
            // `place_set` of a Bool destination from a U8 result: re-narrow to bool (0/1).
            let call = ctx.transmute_on_stack(Type::Int(Int::U8), Type::Bool, call);
            return place_set(destination, call, ctx);
        }
        // `PlatformChar` is a 2-byte interop char with no native sub-word `Interlocked.Exchange`
        // overload before .NET 9 and no width-correct emulation wired for it; routing it through
        // the 1-byte `atomic_xchng_u8` builtin would truncate it (a miscompile), so refuse loudly.
        // Unreachable from safe-stable Rust (there is no `AtomicChar`; the only producer is the
        // interop `dotnet::char` type) — kept as a documented wall per the I3 invariant.
        Type::Bool | Type::PlatformChar => rustc_middle::span_bug!(
            ctx.span(),
            "atomic exchange (`atomic_xchg`) of `{src_type:?}` is unsupported on this .NET target: \
             there is no native sub-word `Interlocked.Exchange` overload before .NET 9 and no \
             width-correct emulation is wired for this type. (Not produced by any stable atomic API.)"
        ),
        _ => (),
    }
    let src_ref = ctx.nref(src_type);
    let call_site = MethodRef::new(
        interlocked,
        ctx.alloc_string("Exchange"),
        ctx.sig([src_ref, src_type], src_type),
        MethodKind::Static,
        vec![].into(),
    );
    let call_site = ctx.alloc_methodref(call_site);
    // T
    let call = ctx.call(call_site, &[dst, new], IsPure::NOT);
    place_set(destination, call, ctx)
}
pub fn cxchg<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> [Root; 2] {
    let interlocked = ClassRef::interlocked(ctx);
    // *T
    let dst = handle_operand(&args[0].node, ctx);
    // T
    let comparand = handle_operand(&args[1].node, ctx);
    // T
    let src = handle_operand(&args[2].node, ctx);
    debug_assert_eq!(
        args.len(),
        3,
        "The intrinsic `atomic_cxchgweak_acquire_acquire` MUST take in exactly 3 argument!"
    );
    let src_type = ctx.monomorphize(args[2].node.ty(ctx.body(), ctx.tcx()));
    let src_type = ctx.type_from_cache(src_type);

    let value = src;

    #[allow(clippy::single_match_else)]
    let exchange_res = match &src_type {
        Type::Ptr(_) => {
            let usize_ref = ctx.nref(Type::Int(Int::USize));
            let call_site = MethodRef::new(
                interlocked,
                ctx.alloc_string("CompareExchange"),
                ctx.sig(
                    [usize_ref, Type::Int(Int::USize), Type::Int(Int::USize)],
                    Type::Int(Int::USize),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let call_site = ctx.alloc_methodref(call_site);
            let dst = ctx.cast_ptr_to(dst, usize_ref);
            let value = ctx.int_cast(value, Int::USize, ExtendKind::ZeroExtend);
            let comparand = ctx.int_cast(comparand, Int::USize, ExtendKind::ZeroExtend);
            let call = ctx.call(call_site, &[dst, value, comparand], IsPure::NOT);
            ctx.cast_ptr_to(call, src_type)
        }
        // .NET 9 has a native sub-word `Interlocked.CompareExchange(ref T, T, T)`; on that runtime
        // set, the sub-word ints fall through to the general arm below, which emits exactly that
        // (no masking, no pointer arithmetic on the managed byref → none of the emulation's
        // page-boundary hazard).
        //
        // On .NET 8 (the default) there is no such overload, so 8/16-bit CAS is emulated with a
        // masked 32-bit CAS loop. The `atomic_cmpxchng{8,16}_correct` builtins
        // (cilly::ir::builtins::atomics) check the comparand *inside* the loop and never write on a
        // mismatch, returning the real old sub-word — so `cxchng_res_val`'s `old == expected` is exact.
        // LE-only + page-boundary caveats documented on the builtin.
        Type::Int(int @ (Int::U8 | Int::I8 | Int::U16 | Int::I16)) if !crate::config::dotnet9() => {
            let width = int.size().expect("sub-word int has a known size");
            let src_ref = ctx.nref(src_type);
            let call_site = ctx.static_mref(
                &format!("atomic_cmpxchng{}_correct", width * 8),
                [src_ref, src_type, src_type],
                src_type,
            );
            // builtin arg order: (addr, comparand, new)
            ctx.call(call_site, &[dst, comparand, value], IsPure::NOT)
        }
        _ => {
            let src_ref = ctx.nref(src_type);
            let call_site = MethodRef::new(
                interlocked,
                ctx.alloc_string("CompareExchange"),
                ctx.sig([src_ref, src_type, src_type], src_type),
                MethodKind::Static,
                vec![].into(),
            );
            let call_site = ctx.alloc_methodref(call_site);
            ctx.call(call_site, &[dst, value, comparand], IsPure::NOT)
        }
    };
    let dst_ty = destination.ty(ctx.body(), ctx.tcx());
    let val_desc = field_descrptor(dst_ty.ty, 0, ctx);
    let flag_desc = field_descrptor(dst_ty.ty, 1, ctx);
    cxchng_res_val(
        exchange_res,
        comparand,
        place_address(destination, ctx),
        val_desc,
        flag_desc,
        ctx,
    )
}
/// Builds the two roots that store the compare-exchange result: the loaded `old_val`
/// into the value field, and the `old_val == expected` flag into the flag field.
fn cxchng_res_val(
    old_val: Node,
    expected: Node,
    destination_addr: Node,
    val_desc: Interned<cilly::FieldDesc>,
    flag_desc: Interned<cilly::FieldDesc>,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> [Root; 2] {
    // Set the value of the result.
    let set_val = ctx.set_field(val_desc, destination_addr, old_val);
    // Get the result back
    let val = ctx.ld_field(destination_addr, val_desc);
    let cmp = ctx.biop(val, expected, cilly::BinOp::Eq);
    let set_flag = ctx.set_field(flag_desc, destination_addr, cmp);
    [set_val, set_flag]
}
