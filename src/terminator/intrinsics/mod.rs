use crate::{assembly::MethodCompileCtx, casts};
use cilly::{
    cilnode::{ExtendKind, IsPure, MethodKind},
    Const, FieldDesc, Interned, MethodRef, Type, {ClassRef, Float, Int},
};
use ints::{ctlz, rotate_left, rotate_right};
use rustc_codegen_clr_place::{place_address, place_set, ptr_set_op};
use rustc_codegen_clr_type::GetTypeExt;
use rustc_codgen_clr_operand::{constant::load_const_value, handle_operand, operand_address};
use rustc_middle::ty::TypingEnv;
use rustc_middle::{
    mir::{Operand, Place},
    ty::{Instance, Ty, UintTy},
};
use rustc_span::Spanned;
use saturating::{saturating_add, saturating_sub};
use type_info::{align_of_val, is_val_statically_known, size_of_val};
use utilis::{
    atomic_add, atomic_and, atomic_max, atomic_min, atomic_nand, atomic_or, atomic_xor,
    compare_bytes,
};
mod bswap;
mod floats;
mod interop;
mod ints;
mod saturating;
mod type_info;
mod utilis;
use floats::{fmaf16, fmaf32, fmaf64, powf32, powf64, powif32, powif64, roundf32, roundf64};
mod ptr;
use ptr::arith_offset;
mod mem;
use mem::{copy, raw_eq, write_bytes};
mod atomic;
mod tpe;
mod vtable;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;
const EMPTY_ARGS: &[Node] = &[];

fn call_atomic<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    atomic: fn(addr: Node, addend: Node, tpe: Type, asm: &mut cilly::Assembly) -> Node,
) -> Vec<Root> {
    // *T
    let dst = handle_operand(&args[0].node, ctx);
    // T
    let arg = handle_operand(&args[1].node, ctx);

    let src_type = ctx.monomorphize(args[1].node.ty(ctx.body(), ctx.tcx()));
    let src_type = ctx.type_from_cache(src_type);

    vec![place_set(destination, atomic(dst, arg, src_type, ctx), ctx)]
}

pub fn breakpoint(args: &[Spanned<Operand<'_>>], ctx: &mut MethodCompileCtx<'_, '_>) -> Root {
    debug_assert_eq!(
        args.len(),
        0,
        "The intrinsic `breakpoint` MUST take in no arguments!"
    );
    ctx.alloc_root(cilly::CILRoot::Break)
}
pub fn black_box<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        1,
        "The intrinsic `black_box` MUST take in exactly 1 argument!"
    );
    let tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    let tpe = ctx.type_from_cache(tpe);
    if tpe == Type::Void {
        return ctx.alloc_root(cilly::CILRoot::Nop);
    }
    // assert_eq!(args.len(),1,"The intrinsic `unlikely` MUST take in exactly 1 argument!");
    let value = handle_operand(&args[0].node, ctx);
    place_set(destination, value, ctx)
}

/// Expands a "uniform-vector" SIMD passthrough arm body: every sig type is the vector type read from
/// `call_instance.args[0]`, the operands are the leading `args`, and the builtin is named after the
/// intrinsic itself (`fn_name`). Keyed by arity — `(vec) -> vec`, `(vec, vec) -> vec`,
/// `(vec, vec, vec) -> vec` — covering the elementwise unary/binary/fma families whose arms used to
/// be 14-line verbatim clones differing only by the name string. The shape `out == in == vec` is the
/// only thing this macro assumes; arms whose result or input type differs (cmp masks, cast, splat,
/// reduce-to-scalar) call `simd_passthrough_call` directly with explicit types instead.
macro_rules! simd_passthrough {
    // `(vec) -> vec`
    (1, $ctx:ident, $args:ident, $destination:ident, $call_instance:ident, $fn_name:ident) => {{
        let vec = simd_ty($ctx, $call_instance, 0);
        let a0 = handle_operand(&$args[0].node, $ctx);
        simd_passthrough_call($ctx, $destination, &[vec], vec, $fn_name, &[a0])
    }};
    // `(vec, vec) -> vec`
    (2, $ctx:ident, $args:ident, $destination:ident, $call_instance:ident, $fn_name:ident) => {{
        let vec = simd_ty($ctx, $call_instance, 0);
        let a0 = handle_operand(&$args[0].node, $ctx);
        let a1 = handle_operand(&$args[1].node, $ctx);
        simd_passthrough_call($ctx, $destination, &[vec, vec], vec, $fn_name, &[a0, a1])
    }};
    // `(vec, vec, vec) -> vec`
    (3, $ctx:ident, $args:ident, $destination:ident, $call_instance:ident, $fn_name:ident) => {{
        let vec = simd_ty($ctx, $call_instance, 0);
        let a0 = handle_operand(&$args[0].node, $ctx);
        let a1 = handle_operand(&$args[1].node, $ctx);
        let a2 = handle_operand(&$args[2].node, $ctx);
        simd_passthrough_call($ctx, $destination, &[vec, vec, vec], vec, $fn_name, &[a0, a1, a2])
    }};
}

pub fn handle_intrinsic<'tcx>(
    fn_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    call_instance: Instance<'tcx>,
    source_info: rustc_middle::mir::SourceInfo,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Vec<Root> {
    let span = source_info.span;
    match fn_name {
        "arith_offset" => vec![arith_offset(args, destination, call_instance, ctx)],
        "breakpoint" => vec![breakpoint(args, ctx)],
        "cold_path"
        | "assert_inhabited"
        | "assert_zero_valid"
        | "assert_mem_uninitialized_valid"
        | "const_deallocate" => {
            vec![ctx.alloc_root(cilly::CILRoot::Nop)]
        }
        "black_box" => vec![black_box(args, destination, call_instance, ctx)],
        "caller_location" => vec![caller_location(destination, ctx, source_info)],
        "compare_bytes" => {
            let a = handle_operand(&args[0].node, ctx);
            let b = handle_operand(&args[1].node, ctx);
            let len = handle_operand(&args[2].node, ctx);
            let cmp = compare_bytes(a, b, len, ctx);
            vec![place_set(destination, cmp, ctx)]
        }
        "ctpop" => vec![ints::ctpop(args, destination, call_instance, ctx)],
        "bitreverse" => vec![ints::bitreverse(args, destination, ctx, call_instance)],
        "ctlz" | "ctlz_nonzero" => vec![ctlz(args, destination, call_instance, ctx)],
        "unlikely" | "likely" => {
            debug_assert_eq!(
                args.len(),
                1,
                "The intrinsic `{fn_name}` MUST take in exactly 1 argument!"
            );
            // assert_eq!(args.len(),1,"The intrinsic `unlikely` MUST take in exactly 1 argument!");
            let value = handle_operand(&args[0].node, ctx);
            vec![place_set(destination, value, ctx)]
        }
        "is_val_statically_known" => vec![is_val_statically_known(args, destination, ctx)],
        "needs_drop" => {
            debug_assert_eq!(
                args.len(),
                0,
                "The intrinsic `needs_drop` MUST take in exactly 0 argument!"
            );
            let needs_drop = ctx
                .monomorphize(
                    call_instance.args[0]
                        .as_type()
                        .expect("needs_drop works only on types!"),
                )
                .needs_drop(
                    ctx.tcx(),
                    rustc_middle::ty::TypingEnv::fully_monomorphized(),
                );
            let needs_drop = i32::from(needs_drop);
            let needs_drop = ctx.alloc_node(needs_drop);
            vec![place_set(destination, needs_drop, ctx)]
        }
        "disjoint_bitor" => {
            let lhs = handle_operand(&args[0].node, ctx);
            let rhs = handle_operand(&args[1].node, ctx);
            let ty = args[0].node.ty(ctx.body(), ctx.tcx());
            let value = crate::binop::bitop::bit_or_unchecked(ty, ty, ctx, lhs, rhs);
            vec![place_set(destination, value, ctx)]
        }
        "fmaf32" => vec![fmaf32(args, destination, ctx)],
        "fmaf64" => vec![fmaf64(args, destination, ctx)],
        "raw_eq" => vec![raw_eq(args, destination, call_instance, ctx)],
        "bswap" => vec![bswap::bswap(args, destination, ctx)],
        "cttz" | "cttz_nonzero" => vec![ints::cttz(args, destination, ctx, call_instance)],
        "rotate_left" => vec![rotate_left(args, destination, ctx, call_instance)],
        "write_bytes" => vec![write_bytes(args, call_instance, ctx)],
        "copy" => vec![copy(args, call_instance, ctx)],
        "exact_div" => {
            debug_assert_eq!(
                args.len(),
                2,
                "The intrinsic `exact_div` MUST take in exactly 2 argument!"
            );

            let value = crate::binop::binop(
                rustc_middle::mir::BinOp::Div,
                &args[0].node,
                &args[1].node,
                ctx,
            );
            vec![place_set(destination, value, ctx)]
        }
        "type_id" => vec![tpe::type_id(destination, call_instance, span, ctx)],
        "atomic_load_acquire"
        | "atomic_load_seqcst"
        | "atomic_load_unordered"
        | "volatile_load" => vec![volitale_load(args, destination, ctx)],
        "volatile_store" => {
            let pointed_type = ctx.monomorphize(
                call_instance.args[0]
                    .as_type()
                    .expect("needs_drop works only on types!"),
            );
            let addr_calc = handle_operand(&args[0].node, ctx);
            let value_calc = handle_operand(&args[1].node, ctx);
            let st = ptr_set_op(pointed_type.into(), ctx, addr_calc, value_calc);
            vec![ctx.make_store_volatile(st)]
        }
        "atomic_store" => {
            // Lower `atomic_store` to a *volatile* store (CIL `volatile. stind`), which gives release
            // semantics per ECMA-335 I.12.6.8 — a plain store has none. .NET guarantees tear-free
            // naturally-aligned stores, so this matches the tear-free-load assumption `atomic_load`
            // relies on. The ordering suffix is stripped before this arm (the const-generic
            // `AtomicOrdering` parameter is never read on the live dispatch path — see the
            // `atomic_load` comment above), so every nominal ordering — including `SeqCst` — funnels
            // through here. `volatile. stind` alone is a release fence, not a full/SeqCst fence: it
            // does not prevent a later load from being reordered ahead of this store (the classic
            // StoreLoad reordering that `Ordering::SeqCst`'s total-order guarantee forbids). We
            // therefore unconditionally emit a trailing `Thread.MemoryBarrier()` (the same call the
            // `atomic_fence` arm below uses) after the volatile store. This is a real, full,
            // bidirectional fence, so it upgrades every store to at least as strong as SeqCst —
            // strictly stronger than Relaxed/Release require, which is safe (costs a barrier
            // instruction on `Release`/`Relaxed` atomic stores; only `SeqCst` strictly needs it).
            debug_assert_eq!(
                args.len(),
                2,
                "The intrinsic `{fn_name}` MUST take in exactly 1 argument!"
            );
            let addr = handle_operand(&args[0].node, ctx);
            let val = handle_operand(&args[1].node, ctx);
            let arg_ty = ctx.monomorphize(args[1].node.ty(ctx.body(), ctx.tcx()));

            let st = ptr_set_op(arg_ty.into(), ctx, addr, val);
            let st = ctx.make_store_volatile(st);

            let thread = ClassRef::thread(ctx);
            let fence = MethodRef::new(
                thread,
                ctx.alloc_string("MemoryBarrier"),
                ctx.sig([], Type::Void),
                MethodKind::Static,
                vec![].into(),
            );
            let fence = ctx.alloc_methodref(fence);
            let fence = ctx.call_root(fence, EMPTY_ARGS, IsPure::NOT);

            vec![st, fence]
        }
        "atomic_cxchg" | "atomic_cxchgweak" => atomic::cxchg(args, destination, ctx).into(),
        "atomic_xsub" => {
            // *T
            let dst = handle_operand(&args[0].node, ctx);
            // T
            let sub_amount = handle_operand(&args[1].node, ctx);
            // we sub by adding a negative number

            let src_type = ctx.monomorphize(args[1].node.ty(ctx.body(), ctx.tcx()));
            let src_type = ctx.type_from_cache(src_type);
            match src_type {
                Type::Int(int) => {
                    let add_amount = if int.is_signed() {
                        ctx.neg(sub_amount)
                    } else {
                        let signed = crate::casts::int_to_int(
                            src_type,
                            Type::Int(int.as_signed()),
                            sub_amount,
                            ctx,
                        );
                        let neg = ctx.neg(signed);
                        crate::casts::int_to_int(Type::Int(int.as_signed()), src_type, neg, ctx)
                    };
                    let value = atomic_add(dst, add_amount, src_type, ctx);
                    vec![place_set(destination, value, ctx)]
                }
                Type::Ptr(_) => {
                    let cast = ctx.cast_ptr_to(sub_amount, Type::Int(Int::ISize));
                    let neg = ctx.neg(cast);
                    let add_amount = crate::casts::int_to_int(
                        Type::Int(Int::ISize),
                        Type::Int(Int::USize),
                        neg,
                        ctx,
                    );
                    let added = atomic_add(dst, add_amount, src_type, ctx);
                    let value = ctx.cast_ptr_to(added, src_type);
                    vec![place_set(destination, value, ctx)]
                }
                _ => panic!("{src_type:?} is not an int."),
            }
        }
        "atomic_or" => call_atomic(args, destination, ctx, atomic_or),
        "atomic_xor" => call_atomic(args, destination, ctx, atomic_xor),
        "atomic_and" => call_atomic(args, destination, ctx, atomic_and),
        "atomic_nand" => call_atomic(args, destination, ctx, atomic_nand),
        // `atomic_singlethreadfence` is a COMPILER-only fence (orders ops within one thread, e.g. vs a
        // signal handler); a full `Thread.MemoryBarrier` is a correct — if stronger-than-required —
        // lowering on .NET, same as `atomic_fence`. Previously `atomic_singlethreadfence` (reachable
        // via `std::sync::atomic::compiler_fence`) fell through to a `span_bug!` ICE (seam-audit gap #8).
        "atomic_fence" | "atomic_singlethreadfence" => {
            let thread = ClassRef::thread(ctx);
            let fence = MethodRef::new(
                thread,
                ctx.alloc_string("MemoryBarrier"),
                ctx.sig([], Type::Void),
                MethodKind::Static,
                vec![].into(),
            );
            let fence = ctx.alloc_methodref(fence);
            vec![ctx.call_root(fence, EMPTY_ARGS, IsPure::NOT)]
        }
        "atomic_xadd" => call_atomic(args, destination, ctx, atomic_add),
        "atomic_umin" => call_atomic(args, destination, ctx, atomic_min),
        "atomic_umax" => call_atomic(args, destination, ctx, atomic_max),
        // Signed `atomic_max`/`atomic_min` (`AtomicI*::fetch_max`/`fetch_min`) previously `span_bug!`-ICE'd
        // (seam-audit gap #7). The `atomic_min`/`atomic_max` helpers are sign-aware via the operand type:
        // `call_atomic` reads `args[1].node.ty(..)`, so an `i32` arg mangles to `atomic_min_i32` (signed
        // compare) while a `u32` arg gives `atomic_min_u32` — so the SIGNED arms wire to the same helpers
        // as `atomic_umin`/`atomic_umax`, and signedness is carried correctly by the argument's type.
        "atomic_min" => call_atomic(args, destination, ctx, atomic_min),
        "atomic_max" => call_atomic(args, destination, ctx, atomic_max),
        "atomic_xchg" => vec![atomic::xchg(args, destination, ctx)],
        // TODO: ensure those intrinsics are sound in C. perhaps time for a new cillyIR node?
        "ptr_offset_from_unsigned" => {
            vec![ptr::ptr_offset_from_unsigned(
                args,
                destination,
                call_instance,
                ctx,
            )]
        }
        "ptr_mask" => {
            debug_assert_eq!(
                args.len(),
                2,
                "The intrinsic `ptr_mask` MUST take in exactly 2 arguments!"
            );
            let tpe = ctx.monomorphize(
                call_instance.args[0]
                    .as_type()
                    .expect("needs_drop works only on types!"),
            );
            let tpe = ctx.type_from_cache(tpe);
            let tpe = ctx.nptr(tpe);

            let lhs = handle_operand(&args[0].node, ctx);
            let lhs = ctx.cast_ptr_to(lhs, Type::Int(Int::USize));
            let rhs = handle_operand(&args[1].node, ctx);
            let masked = ctx.biop(lhs, rhs, cilly::BinOp::And);
            let value = ctx.cast_ptr_to(masked, tpe);
            vec![place_set(destination, value, ctx)]
        }
        "ptr_offset_from" => vec![ptr::ptr_offset_from(args, destination, call_instance, ctx)],
        "saturating_add" => vec![saturating_add(args, destination, ctx, call_instance)],
        "saturating_sub" => vec![saturating_sub(args, destination, ctx, call_instance)],
        // `min_align_of_val` is the historical name for `align_of_val`; both lower identically
        // (and correctly handle `dyn` via the vtable — see `type_info::align_of_val`).
        "min_align_of_val" | "align_of_val" => {
            vec![align_of_val(args, destination, ctx, call_instance)]
        }
        // .NET guarantees all loads are tear-free. The ordering suffix is stripped before this arm
        // (see the const-generic-ordering note on `atomic_store` below), so this one lowering must
        // be sound for Acquire/SeqCst as well as Relaxed. A *plain* `ldind` (volatile:false) only
        // guarantees tear-free access — it is NOT an acquire fence per ECMA-335 I.12.6.7, so the
        // CoreCLR JIT is free to reorder a later load/store above it, which is exactly what
        // `Ordering::Acquire`/`Ordering::SeqCst` forbid. Emitting `volatile.ldind` gives a real
        // acquire fence (I.12.6.7: "no ... read ... may be moved before" a volatile read), which is
        // correct for Acquire and sufficient for the load half of SeqCst; it is stronger than
        // Relaxed strictly requires, which is safe (just gives up some reordering headroom).
        "atomic_load" => {
            debug_assert_eq!(
                args.len(),
                1,
                "The intrinsic `atomic_load_relaxed` MUST take in exactly 1 argument!"
            );
            let ops = handle_operand(&args[0].node, ctx);
            let arg = ctx.monomorphize(args[0].node.ty(ctx.body(), ctx.tcx()));
            let arg_ty = arg.builtin_deref(true).unwrap();
            let arg_type = ctx.type_from_cache(arg_ty);
            let ops = ctx.load_volatile(ops, arg_type);
            vec![place_set(destination, ops, ctx)]
        }
        "sqrtf32" => float_unop(args, destination, ctx, Float::F32, "Sqrt"),
        "carrying_mul_add"
            if !matches!(
                ctx.type_from_cache(
                    call_instance.args[1]
                        .as_type()
                        .expect("needs_drop works only on types!"),
                )
                .as_int()
                .expect("carrying_mul_add with a non-int type"),
                Int::U128 | Int::I128
            ) =>
        {
            let wrapping = ctx.type_from_cache(
                call_instance.args[0]
                    .as_type()
                    .expect("needs_drop works only on types!"),
            );
            let wint = wrapping
                .as_int()
                .expect("carrying_mul_add with a non-int type");

            let overflow = ctx.type_from_cache(
                call_instance.args[1]
                    .as_type()
                    .expect("needs_drop works only on types!"),
            );
            let oint = overflow
                .as_int()
                .expect("carrying_mul_add with a non-int type");
            let promoted = oint
                .promoted()
                .expect("Can't carrying_mul_add cause type is too large");
            // The intrinsic is `carrying_mul_add<T, U>(multiplier: T, multiplicand: T, addend: T,
            // carry: T) -> (U, T)` with `U = T::Unsigned`. So all four runtime operands have type
            // `T` (== `wint`), NOT `U` (== `oint`). They must therefore be widened from `wint`: for
            // a signed `T` this sign-extends into the promoted type (the fallback is
            // `(self as $w) * (a as $w) + …` where `$w` is the double-width signed type), which is
            // exactly the bit pattern the subsequent unsigned mul/add/div needs. Widening from
            // `oint` instead routed a signed `i64` through an unsigned (zero-extending) conversion —
            // a real miscompile for negative operands, caught by the I1 CIL type-verifier (the
            // op_Implicit arg/param signedness mismatch). The unsigned case (`wint == oint`) is
            // unchanged. The promoted *arithmetic* stays unsigned: after correct sign-extension the
            // low 128/64 bits of a signed and unsigned multiply coincide, the low half truncates to
            // `U`, and the logical-shift (unsigned div) high half truncated to `T` equals
            // `(wide >> BITS) as T`.
            let mul_a_op = handle_operand(&args[0].node, ctx);
            let mul_a = casts::int_to_int(
                cilly::Type::Int(wint),
                cilly::Type::Int(promoted),
                mul_a_op,
                ctx,
            );
            let mul_b_op = handle_operand(&args[1].node, ctx);
            let mul_b = casts::int_to_int(
                cilly::Type::Int(wint),
                cilly::Type::Int(promoted),
                mul_b_op,
                ctx,
            );
            let carry_op = handle_operand(&args[2].node, ctx);
            let carry = casts::int_to_int(
                cilly::Type::Int(wint),
                cilly::Type::Int(promoted),
                carry_op,
                ctx,
            );
            let addend_op = handle_operand(&args[3].node, ctx);
            let addend = casts::int_to_int(
                cilly::Type::Int(wint),
                cilly::Type::Int(promoted),
                addend_op,
                ctx,
            );
            let sum = if promoted.size() == Some(16) {
                let op_mul = ctx.static_mref(
                    &format!("mul_{}", promoted.name()),
                    [Type::Int(promoted), Type::Int(promoted)],
                    Type::Int(promoted),
                );
                let op_add = ctx.static_mref(
                    &format!("add_{}", promoted.name()),
                    [Type::Int(promoted), Type::Int(promoted)],
                    Type::Int(promoted),
                );
                let mul = ctx.call(op_mul, &[mul_a, mul_b], IsPure::NOT);
                let add = ctx.call(op_add, &[carry, addend], IsPure::NOT);
                ctx.call(op_add, &[mul, add], IsPure::NOT)
            } else {
                let mul = ctx.biop(mul_a, mul_b, cilly::BinOp::Mul);
                let mul_carry = ctx.biop(mul, carry, cilly::BinOp::Add);
                ctx.biop(mul_carry, addend, cilly::BinOp::Add)
            };
            let ovf = if promoted.size() == Some(16) {
                let op_div = ctx.static_mref(
                    &format!("div_{}", promoted.name()),
                    [Type::Int(promoted), Type::Int(promoted)],
                    Type::Int(promoted),
                );
                let divisor_const = ctx.alloc_node(1_u128 << (oint.size().unwrap_or(8) * 8));
                let divisor = casts::int_to_int(
                    cilly::Type::Int(Int::U128),
                    cilly::Type::Int(promoted),
                    divisor_const,
                    ctx,
                );
                ctx.call(op_div, &[sum, divisor], IsPure::NOT)
            } else {
                let divisor_const = ctx.alloc_node(1_u64 << (oint.size().unwrap_or(8) * 8));
                let divisor = casts::int_to_int(
                    cilly::Type::Int(Int::U64),
                    cilly::Type::Int(promoted),
                    divisor_const,
                    ctx,
                );
                ctx.biop(sum, divisor, cilly::BinOp::DivUn)
            };

            let ovf =
                casts::int_to_int(cilly::Type::Int(promoted), cilly::Type::Int(wint), ovf, ctx);
            let wr =
                casts::int_to_int(cilly::Type::Int(promoted), cilly::Type::Int(oint), sum, ctx);
            let res_tpe = ctx
                .type_from_cache(destination.ty(ctx.body(), ctx.tcx()).ty)
                .as_class_ref()
                .unwrap();
            let dst = place_address(destination, ctx);
            let item1 = ctx.alloc_string("Item1");
            let item2 = ctx.alloc_string("Item2");
            let desc1 = ctx.alloc_field(FieldDesc::new(res_tpe, item1, cilly::Type::Int(oint)));
            let set1 = ctx.set_field(desc1, dst, wr);
            let desc2 = ctx.alloc_field(FieldDesc::new(res_tpe, item2, cilly::Type::Int(wint)));
            let set2 = ctx.set_field(desc2, dst, ovf);
            vec![set1, set2]
        }
        "powif32" => vec![powif32(args, destination, ctx)],
        "powif64" => vec![powif64(args, destination, ctx)],
        "size_of_val" => vec![size_of_val(args, destination, ctx, call_instance)],
        "typed_swap_nonoverlapping" => {
            let pointed_ty = ctx.monomorphize(
                call_instance.args[0]
                    .as_type()
                    .expect("needs_drop works only on types!"),
            );
            let tpe = ctx.monomorphize(pointed_ty);
            // Swapping a pair of ZSTs swaps zero bytes. A ZST lowers to
            // `Type::Void`, whose `size_of` is invalid, so short-circuit to a no-op.
            if ctx.layout_of(tpe).is_zst() {
                return vec![ctx.alloc_root(cilly::ir::CILRoot::Nop)];
            }
            let tpe = ctx.type_from_cache(tpe);
            let void_ptr = ctx.nptr(Type::Void);
            let generic = ctx.static_mref(
                "swap_at_generic",
                [void_ptr, void_ptr, Type::Int(Int::USize)],
                Type::Void,
            );
            let arg0 = handle_operand(&args[0].node, ctx);
            let arg0 = ctx.cast_ptr_to(arg0, void_ptr);
            let arg1 = handle_operand(&args[1].node, ctx);
            let arg1 = ctx.cast_ptr_to(arg1, void_ptr);
            let size = ctx.size_of(tpe);
            let size = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
            vec![ctx.call_root(generic, &[arg0, arg1, size], IsPure::NOT)]
        }
        "type_name" => {
            let const_val = ctx
                .tcx()
                .const_eval_instance(
                    rustc_middle::ty::TypingEnv::fully_monomorphized(),
                    call_instance,
                    span,
                )
                .unwrap();
            let value = load_const_value(const_val, Ty::new_static_str(ctx.tcx()), ctx);
            vec![place_set(destination, value, ctx)]
        }
        "float_to_int_unchecked" => {
            let tpe = ctx.monomorphize(
                call_instance.args[1]
                    .as_type()
                    .expect("needs_drop works only on types!"),
            );
            let tpe = ctx.monomorphize(tpe);
            let tpe = ctx.type_from_cache(tpe);
            let input = handle_operand(&args[0].node, ctx);
            let value = match tpe {
                Type::Int(Int::U8) => ctx.int_cast(input, Int::U8, ExtendKind::ZeroExtend),
                Type::Int(Int::U16) => ctx.int_cast(input, Int::U16, ExtendKind::ZeroExtend),
                Type::Int(Int::U32) => ctx.int_cast(input, Int::U32, ExtendKind::ZeroExtend),
                Type::Int(Int::U64) => ctx.int_cast(input, Int::U64, ExtendKind::ZeroExtend),
                Type::Int(Int::USize) => ctx.int_cast(input, Int::USize, ExtendKind::ZeroExtend),
                Type::Int(Int::I8) => ctx.int_cast(input, Int::I8, ExtendKind::SignExtend),
                Type::Int(Int::I16) => ctx.int_cast(input, Int::I16, ExtendKind::SignExtend),
                Type::Int(Int::I32) => ctx.int_cast(input, Int::I32, ExtendKind::SignExtend),
                Type::Int(Int::I64) => ctx.int_cast(input, Int::I64, ExtendKind::SignExtend),
                Type::Int(Int::ISize) => ctx.int_cast(input, Int::ISize, ExtendKind::SignExtend),
                // 128-bit targets: `IntCast` to {U,I}128 is unimplemented in the IL exporter, so
                // route through `casts::float_to_int`, which emits the BCL float->{U,I}128
                // `op_Explicit` conversion operator (mirrors the f32/f64 -> int cast paths above).
                Type::Int(Int::U128 | Int::I128) => {
                    let src = ctx.monomorphize(args[0].node.ty(ctx.body(), ctx.tcx()));
                    let src = ctx.type_from_cache(src);
                    crate::casts::float_to_int(src, tpe, input, ctx)
                }
                _ => todo!("can't float_to_int_unchecked on {tpe:?}"),
            };
            vec![place_set(destination, value, ctx)]
        }
        "fabsf32" => float_unop(args, destination, ctx, Float::F32, "Abs"),
        "fabsf64" => float_unop(args, destination, ctx, Float::F64, "Abs"),
        "expf32" => float_unop(args, destination, ctx, Float::F32, "Exp"),
        "expf64" => float_unop(args, destination, ctx, Float::F64, "Exp"),
        "logf32" => float_unop(args, destination, ctx, Float::F32, "Log"),
        "logf64" => float_unop(args, destination, ctx, Float::F64, "Log"),
        "log2f32" => float_unop(args, destination, ctx, Float::F32, "Log2"),
        "log2f64" => float_unop(args, destination, ctx, Float::F64, "Log2"),
        "log10f32" => float_unop(args, destination, ctx, Float::F32, "Log10"),
        "log10f64" => float_unop(args, destination, ctx, Float::F64, "Log10"),
        "powf32" => vec![powf32(args, destination, ctx)],
        "powf64" => vec![powf64(args, destination, ctx)],
        "copysignf32" => float_binop(args, destination, ctx, Float::F32, "CopySign"),
        "copysignf64" => float_binop(args, destination, ctx, Float::F64, "CopySign"),
        "copysignf128" => {
            let log = ctx.static_mref(
                "copysignf128",
                [Type::Float(Float::F128), Type::Float(Float::F128)],
                Type::Float(Float::F128),
            );
            let arg0 = handle_operand(&args[0].node, ctx);
            let arg1 = handle_operand(&args[1].node, ctx);
            let value = ctx.call(log, &[arg0, arg1], IsPure::NOT);
            vec![place_set(destination, value, ctx)]
        }
        "sinf32" => float_unop(args, destination, ctx, Float::F32, "Sin"),
        "sinf64" => float_unop(args, destination, ctx, Float::F64, "Sin"),
        "cosf32" => float_unop(args, destination, ctx, Float::F32, "Cos"),
        "cosf64" => float_unop(args, destination, ctx, Float::F64, "Cos"),
        "exp2f32" => float_unop(args, destination, ctx, Float::F32, "Exp2"),
        "exp2f64" => float_unop(args, destination, ctx, Float::F64, "Exp2"),
        "truncf32" => float_unop(args, destination, ctx, Float::F32, "Truncate"),
        "truncf64" => float_unop(args, destination, ctx, Float::F64, "Truncate"),
        // `roundf32` should be a differnt intrinsics, but it requires some .NET fuckery to implement(.NET enums are **wierd**)
        "nearbyintf32" | "rintf32" | "round_ties_even_f32" => {
            let round = MethodRef::new(
                ClassRef::mathf(ctx),
                ctx.alloc_string("Round"),
                ctx.sig([Type::Float(Float::F32)], Type::Float(Float::F32)),
                MethodKind::Static,
                vec![].into(),
            );
            let round = ctx.alloc_methodref(round);
            let arg0 = handle_operand(&args[0].node, ctx);
            let value_calc = ctx.call(round, &[arg0], IsPure::NOT);
            vec![place_set(destination, value_calc, ctx)]
        }
        "roundf32" => vec![roundf32(args, destination, ctx)],
        "roundf64" => vec![roundf64(args, destination, ctx)],
        "nearbyintf64" | "rintf64" | "round_ties_even_f64" => {
            let round = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Round"),
                ctx.sig([Type::Float(Float::F64)], Type::Float(Float::F64)),
                MethodKind::Static,
                vec![].into(),
            );
            let round = ctx.alloc_methodref(round);
            let arg0 = handle_operand(&args[0].node, ctx);
            let value_calc = ctx.call(round, &[arg0], IsPure::NOT);
            vec![place_set(destination, value_calc, ctx)]
        }
        "floorf32" => float_unop(args, destination, ctx, Float::F32, "Floor"),
        "floorf64" => float_unop(args, destination, ctx, Float::F64, "Floor"),
        "ceilf32" => float_unop(args, destination, ctx, Float::F32, "Ceiling"),
        "ceilf64" => float_unop(args, destination, ctx, Float::F64, "Ceiling"),
        // f16 math via `System.Half` (.NET 8 implements `IBinaryFloatingPointIeee754<Half>`, so
        // these static methods exist just like on `float`/`double`). Reached e.g. by proptest's
        // `f16` value strategies and any half-precision `core::simd`/`std::f16` code.
        "floorf16" => float_unop(args, destination, ctx, Float::F16, "Floor"),
        "ceilf16" => float_unop(args, destination, ctx, Float::F16, "Ceiling"),
        "truncf16" => float_unop(args, destination, ctx, Float::F16, "Truncate"),
        "sqrtf16" => float_unop(args, destination, ctx, Float::F16, "Sqrt"),
        "fabsf16" => float_unop(args, destination, ctx, Float::F16, "Abs"),
        "copysignf16" => float_binop(args, destination, ctx, Float::F16, "CopySign"),
        "maxnumf16" => float_binop(args, destination, ctx, Float::F16, "MaxNumber"),
        "minnumf16" => float_binop(args, destination, ctx, Float::F16, "MinNumber"),
        "fmaf16" => vec![fmaf16(args, destination, ctx)],
        "maxnumf64" => float_binop(args, destination, ctx, Float::F64, "MaxNumber"),
        "maxnumf32" => float_binop(args, destination, ctx, Float::F32, "MaxNumber"),
        "minnumf64" => float_binop(args, destination, ctx, Float::F64, "MinNumber"),
        "minnumf32" => float_binop(args, destination, ctx, Float::F32, "MinNumber"),
        // The `*_algebraic` float intrinsics permit the optimizer to reassociate/contract the
        // operation. .NET/RyuJIT does neither across these boundaries, so the faithful lowering is
        // the plain IEEE-754 op (never less precise than the source program). Generic over f32/f64
        // — the operand type carries the width. Needed by coretests (the stdlib test suite).
        "fadd_algebraic" => float_algebraic(args, destination, ctx, cilly::BinOp::Add),
        "fsub_algebraic" => float_algebraic(args, destination, ctx, cilly::BinOp::Sub),
        "fmul_algebraic" => float_algebraic(args, destination, ctx, cilly::BinOp::Mul),
        "fdiv_algebraic" => float_algebraic(args, destination, ctx, cilly::BinOp::Div),
        "frem_algebraic" => float_algebraic(args, destination, ctx, cilly::BinOp::Rem),
        // Float min/max/abs were reworked upstream (see docs/semantics_mapping.md for the
        // verified Rust<->.NET truth table). Two distinct families now exist:
        //  * IEEE 754-2019 maximum/minimum (`f32::maximum`/`minimum`): propagate NaN, order -0<+0.
        //    .NET `Single/Double::Max/Min` implement exactly this.
        "maximumf32" => float_binop(args, destination, ctx, Float::F32, "Max"),
        "maximumf64" => float_binop(args, destination, ctx, Float::F64, "Max"),
        "minimumf32" => float_binop(args, destination, ctx, Float::F32, "Min"),
        "minimumf64" => float_binop(args, destination, ctx, Float::F64, "Min"),
        //  * maxNum/minNum, no-signed-zero (`f32::max`/`min`): ignore NaN (return the number).
        //    .NET `::MaxNumber/::MinNumber` match; the nsz freedom on the zero sign is satisfied.
        "maximum_number_nsz_f32" => float_binop(args, destination, ctx, Float::F32, "MaxNumber"),
        "maximum_number_nsz_f64" => float_binop(args, destination, ctx, Float::F64, "MaxNumber"),
        "minimum_number_nsz_f32" => float_binop(args, destination, ctx, Float::F32, "MinNumber"),
        "minimum_number_nsz_f64" => float_binop(args, destination, ctx, Float::F64, "MinNumber"),
        // `fabs` is now a single generic intrinsic (replaces `fabsf32`/`fabsf64`); dispatch on the
        // argument's float width and call `<FloatClass>::Abs`.
        "fabs" => {
            let arg_ty = ctx.monomorphize(args[0].node.ty(ctx.body(), ctx.tcx()));
            match ctx.type_from_cache(arg_ty) {
                Type::Float(float) => float_unop(args, destination, ctx, float, "Abs"),
                other => todo!("`fabs` on non-float type {other:?}"),
            }
        }
        "variant_count" => {
            let const_val = ctx
                .tcx()
                .const_eval_instance(
                    rustc_middle::ty::TypingEnv::fully_monomorphized(),
                    call_instance,
                    span,
                )
                .unwrap();
            let value = load_const_value(const_val, Ty::new_uint(ctx.tcx(), UintTy::Usize), ctx);
            vec![place_set(destination, value, ctx)]
        }
        "sqrtf64" => float_unop(args, destination, ctx, Float::F64, "Sqrt"),
        "rotate_right" => vec![rotate_right(args, destination, ctx, call_instance)],
        "catch_unwind" => {
            debug_assert_eq!(
                args.len(),
                3,
                "The intrinsic `catch_unwind` MUST take in exactly 3 arguments!"
            );
            let try_fn = handle_operand(&args[0].node, ctx);
            let data_ptr = handle_operand(&args[1].node, ctx);
            let catch_fn = handle_operand(&args[2].node, ctx);
            let uint8_ptr = ctx.nptr(Type::Int(Int::U8));
            let try_ptr = ctx.sig([uint8_ptr], Type::Void);
            let catch_ptr = ctx.sig([uint8_ptr, uint8_ptr], Type::Void);
            let value = ctx.call_static(
                "catch_unwind",
                [Type::FnPtr(try_ptr), uint8_ptr, Type::FnPtr(catch_ptr)],
                Type::Int(Int::I32),
                &[try_fn, data_ptr, catch_fn],
            );
            vec![place_set(destination, value, ctx)]
        }
        "abort" => vec![ctx.throw_msg("Called abort!")],
        "const_allocate" => {
            let null = ctx.alloc_node(Const::USize(0));
            let null = ctx.cast_ptr(null, Int::U8);
            vec![place_set(destination, null, ctx)]
        }
        "vtable_size" => vec![vtable::vtable_size(args, destination, ctx)],
        "vtable_align" => vec![vtable::vtable_align(args, destination, ctx)],
        "simd_eq" | "simd_lt" | "simd_gt" | "simd_ge" | "simd_le" => {
            // Element-wise comparisons producing a per-lane mask: all share the
            // `(comparands, comparands) -> mask` shape (comparand vector from generic arg 0, mask
            // result from generic arg 1), and the builtin generator (cilly simd::binop::fallback_simd)
            // supplies the matching body for each `fn_name`.
            let comparands = simd_ty(ctx, call_instance, 0);
            let result = simd_ty(ctx, call_instance, 1);
            let lhs = handle_operand(&args[0].node, ctx);
            let rhs = handle_operand(&args[1].node, ctx);
            simd_passthrough_call(
                ctx,
                destination,
                &[comparands, comparands],
                result,
                fn_name,
                &[lhs, rhs],
            )
        }
        "simd_extract" | "simd_extract_dyn" => {
            // `simd_extract<T, U>(x: T, idx: u32) -> U`: read lane `idx` (element type `U`)
            // out of vector `x` (type `T`). Implemented via memory: take the address of the
            // vector, reinterpret it as `*U`, index, and load — the same spill-and-index idiom
            // the elementwise SIMD builtins use.
            let elem = ctx.type_from_cache(
                call_instance.args[1]
                    .as_type()
                    .expect("simd_extract works only on types!"),
            );
            let addr = operand_address(&args[0].node, ctx);
            // `cast_ptr` casts the vector address to a pointer-to-element (`*U`).
            let elem_ptr = ctx.cast_ptr(addr, elem);
            let idx = handle_operand(&args[1].node, ctx);
            let slot = ctx.offset(elem_ptr, idx, elem);
            let value = ctx.load(slot, elem);
            vec![place_set(destination, value, ctx)]
        }
        "simd_insert" | "simd_insert_dyn" => {
            // `simd_insert<T, U>(x: T, idx: u32, val: U) -> T`: return a copy of vector `x`
            // with lane `idx` replaced by `val`. Spill `x` to a local, overwrite the lane in
            // place through a reinterpreted `*U`, then yield the modified vector.
            let vec_ty = ctx.type_from_cache(
                call_instance.args[0]
                    .as_type()
                    .expect("simd_insert works only on types!"),
            );
            let elem = ctx.type_from_cache(
                call_instance.args[1]
                    .as_type()
                    .expect("simd_insert works only on types!"),
            );
            let x = handle_operand(&args[0].node, ctx);
            let idx = handle_operand(&args[1].node, ctx);
            let val = handle_operand(&args[2].node, ctx);
            // Stash the source vector into the destination place, then mutate that lane.
            let mut roots = vec![place_set(destination, x, ctx)];
            let addr = place_address(destination, ctx);
            let elem_ptr = ctx.cast_ptr(addr, elem);
            let slot = ctx.offset(elem_ptr, idx, elem);
            let st = ctx.alloc_root(cilly::ir::CILRoot::StInd(Box::new((
                slot, val, elem, false,
            ))));
            let _ = vec_ty;
            roots.push(st);
            roots
        }
        // Element-wise `(vec, vec) -> vec` ops: every one maps 1:1 to a same-named per-lane builtin
        // supplied by `fallback_simd`/`register_value_lane_ops`. `simd_div` has no single BCL
        // `Vector` static (the lane body is generated). `simd_shr` resolves arithmetic-vs-logical
        // shift, and `simd_rem` signed-vs-unsigned remainder, from the SIMD element type's signedness
        // inside the generator.
        "simd_or" | "simd_add" | "simd_and" | "simd_sub" | "simd_mul" | "simd_div" | "simd_shl"
        | "simd_shr" | "simd_xor" | "simd_rem" | "simd_maximum_number_nsz"
        | "simd_minimum_number_nsz" => {
            simd_passthrough!(2, ctx, args, destination, call_instance, fn_name)
        }
        "simd_cast" | "simd_as" => {
            // `simd_cast<T, U>(x: T) -> U`: per-lane numeric conversion (T and U have the same
            // lane count, different element types; both `simd_cast` and `simd_as` map to the
            // `simd_cast` builtin in `fallback_simd`).
            let src = simd_ty(ctx, call_instance, 0);
            let dst = simd_ty(ctx, call_instance, 1);
            let val = handle_operand(&args[0].node, ctx);
            simd_passthrough_call(ctx, destination, &[src], dst, "simd_cast", &[val])
        }
        "simd_fabs" => {
            // `simd_fabs<T>(x: T) -> T`: per-lane absolute value, served by the `simd_abs` builtin.
            let vec = simd_ty(ctx, call_instance, 0);
            let lhs = handle_operand(&args[0].node, ctx);
            simd_passthrough_call(ctx, destination, &[vec], vec, "simd_abs", &[lhs])
        }
        "simd_bitmask" => {
            let vec: Type = ctx.type_from_cache(
                call_instance.args[0]
                    .as_type()
                    .expect("simd_bitmask works only on types!"),
            );
            let int = ctx.type_from_cache(
                call_instance.args[1]
                    .as_type()
                    .expect("simd_bitmask works only on types!"),
            );
            let int = int
                .as_int()
                .expect("simd_bitmask only currently supports bitpacking ints.");
            let lhs = handle_operand(&args[0].node, ctx);
            let name = ctx.alloc_string("simd_get_most_significant_bits");
            let main_module = ctx.main_module();
            let main_module = ctx[*main_module].clone();
            let most_significant_bits = main_module.static_mref(&[vec], Type::Int(int), name, ctx);
            let value = ctx.call(most_significant_bits, &[lhs], IsPure::NOT);
            vec![place_set(destination, value, ctx)]
        }
        // Unary `(vec) -> vec` ops, each served by a per-lane builtin of the same name. `simd_neg`
        // lives in `register_value_lane_ops`; the SIMD-tail set (per-lane integer bit ops and float
        // rounders) lives in `cilly/src/ir/builtins/simd/tail.rs`.
        "simd_neg" | "simd_ctlz" | "simd_cttz" | "simd_ctpop" | "simd_bswap" | "simd_bitreverse"
        | "simd_fsqrt" | "simd_floor" | "simd_ceil" | "simd_trunc" | "simd_round"
        | "simd_round_ties_even" => {
            simd_passthrough!(1, ctx, args, destination, call_instance, fn_name)
        }
        // SIMD fused multiply-add `(vec, vec, vec) -> vec`, single-rounding, per-lane builtin.
        "simd_fma" | "simd_relaxed_fma" => {
            simd_passthrough!(3, ctx, args, destination, call_instance, fn_name)
        }
        "simd_shuffle" => {
            // `simd_shuffle<T, U, V>(x: T, y: T, idx: U) -> V`: gather lanes of the concatenation of
            // `x`/`y` per the index vector `idx`. NOTE: special-cased — the scalar fast-path below
            // returns early, so this arm is NOT a plain passthrough.
            let t_type = simd_ty(ctx, call_instance, 0);
            let u_type = simd_ty(ctx, call_instance, 1);
            let v_type = simd_ty(ctx, call_instance, 2);
            let x = handle_operand(&args[0].node, ctx);
            let y = handle_operand(&args[1].node, ctx);
            // When the two vectors provided to simd shuffles are always the same, and have a length of 1(are scalar), the shuffle is equivalent to creating a vector [scalar,scalar].
            if x == y && matches!(t_type, Type::Int(_) | Type::Float(_)) {
                let name = ctx.alloc_string("simd_vec_from_val");
                let main_module = ctx.main_module();
                let main_module = ctx[*main_module].clone();
                let shuffle = main_module.static_mref(&[t_type], v_type, name, ctx);
                // SANITY: for this optimzation to work, the u(index vector) and v(result vector) both have to have be vectors.
                let (_u_type, _v_type) = (
                    u_type.as_simdvector().unwrap(),
                    v_type.as_simdvector().unwrap(),
                );
                let value = ctx.call(shuffle, &[x], IsPure::NOT);
                return vec![place_set(destination, value, ctx)];
            }
            let idx = handle_operand(&args[2].node, ctx);
            let name = ctx.alloc_string("simd_shuffle");
            let main_module = ctx.main_module();
            let main_module = ctx[*main_module].clone();
            let shuffle = main_module.static_mref(&[t_type, t_type, u_type], v_type, name, ctx);
            let value = ctx.call(shuffle, &[x, y, idx], IsPure::NOT);
            vec![place_set(destination, value, ctx)]
        }
        "simd_ne" => {
            // `simd_ne` has no direct builtin: compute `simd_eq` then bit-flip the mask with
            // `simd_ones_compliment` (two calls), so it stays out of the plain passthrough path.
            let comparands = simd_ty(ctx, call_instance, 0);
            let result = simd_ty(ctx, call_instance, 1);
            let lhs = handle_operand(&args[0].node, ctx);
            let rhs = handle_operand(&args[1].node, ctx);
            let eq = ctx.alloc_string("simd_eq");
            let ones_compliment = ctx.alloc_string("simd_ones_compliment");
            let main_module = ctx.main_module();
            let main_module = ctx[*main_module].clone();
            let eq = main_module.static_mref(&[comparands, comparands], result, eq, ctx);
            let eq = ctx.call(eq, &[lhs, rhs], IsPure::NOT);
            let ones_compliment = main_module.static_mref(&[result], result, ones_compliment, ctx);
            let ne = ctx.call(ones_compliment, &[eq], IsPure::NOT);
            vec![place_set(destination, ne, ctx)]
        }
        "simd_splat" => {
            // `simd_splat<T, U>(value: U) -> T`: broadcast scalar `value` (type `U`,
            // the element type) into every lane of result vector `T`. This maps 1:1 to
            // the existing `simd_vec_from_val` builtin (.NET `Vector<E>.Create(scalar)`),
            // the same helper the `x == y` scalar case of `simd_shuffle` reuses. The result is
            // normally a managed SIMD vector, but may be the fixed-array fallback for an unsupported
            // vector size — the `simd_vec_from_val` builtin handles both.
            let vec_type = simd_ty(ctx, call_instance, 0);
            let scalar_type = simd_ty(ctx, call_instance, 1);
            let value = handle_operand(&args[0].node, ctx);
            simd_passthrough_call(
                ctx,
                destination,
                &[scalar_type],
                vec_type,
                "simd_vec_from_val",
                &[value],
            )
        }
        "simd_reduce_any" => {
            // Special case: `x` is "any lane set?" iff it is NOT equal to the all-clear vector, so
            // this folds against `simd_allset` + `simd_eq_any` (two builtins), not a plain reduce.
            let vec = simd_ty(ctx, call_instance, 0);
            let x = handle_operand(&args[0].node, ctx);
            let simd_eq = ctx.alloc_string("simd_eq_any");
            let allset = ctx.alloc_string("simd_allset");
            let main_module = ctx.main_module();
            let main_module = ctx[*main_module].clone();
            let eq = main_module.static_mref(&[vec, vec], Type::Bool, simd_eq, ctx);
            let allset = main_module.static_mref(&[], vec, allset, ctx);
            let allset = ctx.call(allset, EMPTY_ARGS, IsPure::NOT);
            let value = ctx.call(eq, &[x, allset], IsPure::NOT);
            vec![place_set(destination, value, ctx)]
        }
        "select_unpredictable" => {
            let tpe = ctx.type_from_cache(
                call_instance.args[0]
                    .as_type()
                    .expect("select_unpredictable works only on types!"),
            );
            let cond = handle_operand(&args[0].node, ctx);

            if let Some(_) = tpe.as_class_ref() {
                let true_val = operand_address(&args[1].node, ctx);
                let false_val = operand_address(&args[2].node, ctx);
                let ptr_tpe = ctx.nptr(tpe);
                let select = ctx.select(ptr_tpe, true_val, false_val, cond);
                let select = ctx.load(select, tpe);
                return vec![place_set(destination, select, ctx)];
            }
            let true_val = handle_operand(&args[1].node, ctx);
            let false_val = handle_operand(&args[2].node, ctx);
            let select = ctx.select(tpe, true_val, false_val, cond);
            vec![place_set(destination, select, ctx)]
        }
        "simd_reduce_all" => {
            // Special case: `x` is "all lanes set?" iff it equals the all-set vector, so this folds
            // against `simd_allset` + `simd_eq_all` (two builtins), not a plain reduce.
            let vec = simd_ty(ctx, call_instance, 0);
            let x = handle_operand(&args[0].node, ctx);
            let simd_eq = ctx.alloc_string("simd_eq_all");
            let allset = ctx.alloc_string("simd_allset");
            let main_module = ctx.main_module();
            let main_module = ctx[*main_module].clone();
            let eq = main_module.static_mref(&[vec, vec], Type::Bool, simd_eq, ctx);
            let allset = main_module.static_mref(&[], vec, allset, ctx);
            let allset = ctx.call(allset, EMPTY_ARGS, IsPure::NOT);
            let value = ctx.call(eq, &[x, allset], IsPure::NOT);
            vec![place_set(destination, value, ctx)]
        }
        "simd_select" => {
            // `simd_select<M, T>(mask: M, if_true: T, if_false: T) -> T`: per-lane blend. The
            // builtin (`simd_select` in `register_value_lane_ops`) does `mask[i] != 0 ? a[i] : b[i]`.
            let mask_ty = simd_ty(ctx, call_instance, 0);
            let val_ty = simd_ty(ctx, call_instance, 1);
            let mask = handle_operand(&args[0].node, ctx);
            let a = handle_operand(&args[1].node, ctx);
            let b = handle_operand(&args[2].node, ctx);
            simd_passthrough_call(
                ctx,
                destination,
                &[mask_ty, val_ty, val_ty],
                val_ty,
                "simd_select",
                &[mask, a, b],
            )
        }
        "simd_reduce_add_ordered" | "simd_reduce_mul_ordered" => {
            // `simd_reduce_{add,mul}_ordered<T, U>(x: T, acc: U) -> U`: horizontal fold seeded with
            // `acc`, left-to-right (for float bit-exactness). Result is the scalar element type `U`.
            let vec = simd_ty(ctx, call_instance, 0);
            let scalar = simd_ty(ctx, call_instance, 1);
            let x = handle_operand(&args[0].node, ctx);
            let acc = handle_operand(&args[1].node, ctx);
            simd_passthrough_call(ctx, destination, &[vec, scalar], scalar, fn_name, &[x, acc])
        }
        "simd_reduce_add_unordered"
        | "simd_reduce_mul_unordered"
        | "simd_reduce_and"
        | "simd_reduce_or"
        | "simd_reduce_xor"
        | "simd_reduce_min"
        | "simd_reduce_max" => {
            // `simd_reduce_*<T, U>(x: T) -> U`: horizontal fold over all lanes (no seed; starts from
            // lane 0). The per-lane fold body is supplied by the matching `simd_reduce` builtin.
            let vec = simd_ty(ctx, call_instance, 0);
            let scalar = simd_ty(ctx, call_instance, 1);
            let x = handle_operand(&args[0].node, ctx);
            simd_passthrough_call(ctx, destination, &[vec], scalar, fn_name, &[x])
        }
        // SIMD WALLS — intrinsics with no clean BCL `Vector` primitive (gather/scatter walk a vector
        // of raw pointers per lane; masked_load/store add a const ALIGN generic on top of that;
        // funnel shifts are out of the current target list). These are left as explicit walls so the
        // failure mode is a clear message rather than a confusing generic-call fall-through. The safe
        // path when wiring them later is the per-lane spill-and-index builtin (see
        // `cilly/src/ir/builtins/simd/tail.rs`).
        "simd_gather" | "simd_scatter" | "simd_masked_load" | "simd_masked_store"
        | "simd_funnel_shl" | "simd_funnel_shr" => {
            todo!("SIMD intrinsic `{fn_name}` is not yet supported (no clean BCL Vector primitive; wire per-lane in cilly/src/ir/builtins/simd/tail.rs)")
        }
        _ => intrinsic_slow(fn_name, args, destination, ctx, call_instance, source_info),
    }
}
use rustc_middle::span_bug;
fn intrinsic_slow<'tcx>(
    fn_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
    source_info: rustc_middle::mir::SourceInfo,
) -> Vec<Root> {
    let span = source_info.span;
    // Then, demangle the type name, converting it to a Rust-style one (eg. `core::option::Option::h8zc8s`)
    let demangled = rustc_demangle::demangle(fn_name);
    // Using formating preserves the generic hash.
    let demangled = format!("{demangled:#}");
    if demangled == fn_name {
        let intrinsic = ctx.tcx().intrinsic(call_instance.def_id()).unwrap();
        if intrinsic.must_be_overridden {
            span_bug!(
                span,
                "intrinsic {} must be overridden by rustc_codgen_clr, but isn't",
                intrinsic.name,
            );
        }
        super::call::call_inner(
            Instance::new_raw(call_instance.def_id(), call_instance.args)
                .ty(ctx.tcx(), TypingEnv::fully_monomorphized()),
            Instance::new_raw(call_instance.def_id(), call_instance.args),
            ctx,
            args,
            destination,
            source_info,
        )
    } else {
        assert!(demangled.contains("::"));
        let striped = demangled_to_stem(&demangled);
        handle_intrinsic(striped, args, destination, call_instance, source_info, ctx)
    }
}
fn volitale_load<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    //TODO:fix volitale prefix!
    debug_assert_eq!(
        args.len(),
        1,
        "The intrinsic `volatile_load` MUST take in exactly 1 argument!"
    );
    let arg = ctx.monomorphize(args[0].node.ty(ctx.body(), ctx.tcx()));
    let arg_ty = arg.builtin_deref(true).unwrap();
    let arg_type = ctx.type_from_cache(arg_ty);
    let arg = handle_operand(&args[0].node, ctx);
    let ops = ctx.load_volatile(arg, arg_type);
    place_set(destination, ops, ctx)
}
fn caller_location<'tcx>(
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    source_info: rustc_middle::mir::SourceInfo,
) -> Root {
    // `caller_location` only ever appears inside a `#[track_caller]` fn (e.g. `Location::caller`).
    // `get_caller_location` forwards that fn's implicit trailing `&Location` argument (and walks any
    // MIR-inlined track_caller frames) rather than materializing a constant from this intrinsic's own
    // span (which was `location.rs` itself).
    let value = crate::terminator::get_caller_location(ctx, source_info);
    place_set(destination, value, ctx)
}
fn demangled_to_stem(s: &str) -> &str {
    let mut res = None;
    //.filter(|part|!(part.contains('<') | part.contains('>'))).last().unwrap()
    for element in s.split("::") {
        if element.contains('<') {
            break;
        }
        res = Some(element);
    }
    res.unwrap()
}
fn float_unop<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    float: Float,
    name: &str,
) -> Vec<Root> {
    let log = MethodRef::new(
        float.class(ctx),
        ctx.alloc_string(name),
        ctx.sig([Type::Float(float)], float),
        MethodKind::Static,
        vec![].into(),
    );
    let log = ctx.alloc_methodref(log);
    let arg0 = handle_operand(&args[0].node, ctx);
    let value = ctx.call(log, &[arg0], IsPure::NOT);
    vec![place_set(destination, value, ctx)]
}
/// Lower an `*_algebraic` float intrinsic to the plain IR binary op (`a OP b`). The operand type
/// (f32/f64) is carried by the args, so one helper covers both widths.
fn float_algebraic<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    op: cilly::BinOp,
) -> Vec<Root> {
    let arg0 = handle_operand(&args[0].node, ctx);
    let arg1 = handle_operand(&args[1].node, ctx);
    let value = ctx.biop(arg0, arg1, op);
    vec![place_set(destination, value, ctx)]
}
fn float_binop<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    float: Float,
    name: &str,
) -> Vec<Root> {
    let log = MethodRef::new(
        float.class(ctx),
        ctx.alloc_string(name),
        ctx.sig([Type::Float(float), Type::Float(float)], float),
        MethodKind::Static,
        vec![].into(),
    );
    let log = ctx.alloc_methodref(log);
    let arg0 = handle_operand(&args[0].node, ctx);
    let arg1 = handle_operand(&args[1].node, ctx);
    let value = ctx.call(log, &[arg0, arg1], IsPure::NOT);
    vec![place_set(destination, value, ctx)]
}
/// Read the `idx`-th generic type argument of a SIMD intrinsic instance (`call_instance.args[idx]`)
/// and lower it to a cilly `Type`. SIMD intrinsics carry their vector/element/mask types as type
/// generics, so every passthrough arm starts by pulling one or more of them out of the instance.
fn simd_ty<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
    idx: usize,
) -> Type {
    ctx.type_from_cache(
        call_instance.args[idx]
            .as_type()
            .unwrap_or_else(|| panic!("simd intrinsic generic arg {idx} works only on types!")),
    )
}
/// Emit the body shared by every "passthrough" SIMD intrinsic: look up a same-shaped static builtin
/// `name` in the main module over `inputs -> output`, call it with `ops`, and store the result into
/// `destination`. This is the ~6-line tail (`alloc_string` + `main_module` clone + `static_mref` +
/// `call` + `place_set`) that the elementwise/reduce/cast/etc. arms all repeat verbatim, differing
/// only by the sig types, the operands, and the builtin name (which is often `fn_name` itself —
/// proof this folding is name-preserving). Irregular SIMD arms (shuffle's scalar fast-path,
/// extract/insert's memory idioms, ne's two-call sequence, the reduce_any/all allset dance, and the
/// gather/scatter walls) keep their own bodies and do NOT route through here.
fn simd_passthrough_call<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    destination: &Place<'tcx>,
    inputs: &[Type],
    output: Type,
    name: &str,
    ops: &[Node],
) -> Vec<Root> {
    let name = ctx.alloc_string(name);
    let main_module = ctx.main_module();
    let main_module = ctx[*main_module].clone();
    let op = main_module.static_mref(inputs, output, name, ctx);
    let value = ctx.call(op, ops, IsPure::NOT);
    vec![place_set(destination, value, ctx)]
}
#[test]
fn test_intrinsic_slow_escape() {
    const BAD:&str = "core::intrinsics::ptr_offset_from_unsigned::<(rustc_hir::def::LifetimeRes, rustc_resolve::late::diagnostics::LifetimeElisionCandidate)";
    assert_eq!(demangled_to_stem(BAD), "ptr_offset_from_unsigned");
}
