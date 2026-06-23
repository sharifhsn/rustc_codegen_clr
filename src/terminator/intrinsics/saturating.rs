use crate::assembly::MethodCompileCtx;
use cilly::{
    cilnode::{IsPure, MethodKind},
    Interned, MethodRef, Type, {ClassRef, Int},
};
use rustc_codegen_clr_place::place_set;
use rustc_codegen_clr_type::GetTypeExt;

use rustc_codgen_clr_operand::handle_operand;
use rustc_middle::{
    mir::{Operand, Place},
    ty::Instance,
};
use rustc_span::Spanned;

type Root = Interned<cilly::ir::CILRoot>;

pub fn saturating_add<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let a_ty = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("saturating_sub works only on types!"),
    );
    let a_type = ctx.type_from_cache(
        ctx.monomorphize(
            call_instance.args[0]
                .as_type()
                .expect("needs_drop works only on types!"),
        ),
    );
    let calc = match a_type {
        Type::Int(
            int @ (Int::USize | Int::U128 | Int::U64 | Int::U32 | Int::U16 | Int::U8),
        ) => {
            let sum = crate::binop::add_unchecked(a_ty, a_ty, ctx, a, b);
            let or = crate::binop::bitop::bit_or_unchecked(a_ty, a_ty, ctx, a, b);
            let flag = crate::binop::cmp::lt_unchecked(a_ty, sum, or, ctx);
            let max = int.max(ctx);
            let max = ctx.alloc_node(max);
            ctx.select(a_type, max, sum, flag)
        }
        Type::Int(Int::I32) => {
            let a = ctx.int_cast(a, Int::I64, cilly::cilnode::ExtendKind::SignExtend);
            let b = ctx.int_cast(b, Int::I64, cilly::cilnode::ExtendKind::SignExtend);
            let diff = ctx.biop(a, b, cilly::BinOp::Add);
            let clamp = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I64),
                        Type::Int(Int::I64),
                        Type::Int(Int::I64),
                    ],
                    Type::Int(Int::I64),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let clamp = ctx.alloc_methodref(clamp);
            let min = ctx.alloc_node(i64::from(i32::MIN));
            let max = ctx.alloc_node(i64::from(i32::MAX));
            let diff_capped = ctx.call(clamp, &[diff, min, max], IsPure::NOT);
            ctx.int_cast(diff_capped, Int::I32, cilly::cilnode::ExtendKind::SignExtend)
        }

        Type::Int(Int::I64) => {
            let a = crate::casts::int_to_int(Type::Int(Int::I64), Type::Int(Int::I128), a, ctx);
            let b = crate::casts::int_to_int(Type::Int(Int::I64), Type::Int(Int::I128), b, ctx);
            let add = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_Addition"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let add = ctx.alloc_methodref(add);
            let diff = ctx.call(add, &[a, b], IsPure::NOT);
            let clamp = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I128),
                        Type::Int(Int::I128),
                        Type::Int(Int::I128),
                    ],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let clamp = ctx.alloc_methodref(clamp);
            #[allow(clippy::cast_sign_loss)]
            let min = ctx.alloc_node((i128::from(i64::MIN) as u128) as i128);
            #[allow(clippy::cast_sign_loss)]
            let max = ctx.alloc_node((i128::from(i64::MAX) as u128) as i128);
            let diff_capped = ctx.call(clamp, &[diff, min, max], IsPure::NOT);
            crate::casts::int_to_int(Type::Int(Int::I128), Type::Int(Int::I64), diff_capped, ctx)
        }

        Type::Int(Int::ISize) => {
            let a = crate::casts::int_to_int(Type::Int(Int::ISize), Type::Int(Int::I128), a, ctx);
            let b = crate::casts::int_to_int(Type::Int(Int::ISize), Type::Int(Int::I128), b, ctx);
            let sum = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_Addition"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let sum = ctx.alloc_methodref(sum);
            let diff = ctx.call(sum, &[a, b], IsPure::NOT);
            let clamp = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I128),
                        Type::Int(Int::I128),
                        Type::Int(Int::I128),
                    ],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let clamp = ctx.alloc_methodref(clamp);
            // TODO: this assumes isize::MAX == i64::MAX
            #[allow(clippy::cast_sign_loss)]
            let min = ctx.alloc_node((i128::from(i64::MIN) as u128) as i128);
            #[allow(clippy::cast_sign_loss)]
            let max = ctx.alloc_node((i128::from(i64::MAX) as u128) as i128);
            let diff_capped = ctx.call(clamp, &[diff, min, max], IsPure::NOT);
            crate::casts::int_to_int(
                Type::Int(Int::I128),
                Type::Int(Int::ISize),
                diff_capped,
                ctx,
            )
        }
        Type::Int(Int::I16) => {
            let a = ctx.int_cast(a, Int::I32, cilly::cilnode::ExtendKind::SignExtend);
            let b = ctx.int_cast(b, Int::I32, cilly::cilnode::ExtendKind::SignExtend);
            let diff = ctx.biop(a, b, cilly::BinOp::Add);
            let mref = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I32),
                        Type::Int(Int::I32),
                        Type::Int(Int::I32),
                    ],
                    Type::Int(Int::I32),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let min = ctx.alloc_node(i32::from(i16::MIN));
            let max = ctx.alloc_node(i32::from(i16::MAX));
            let diff_capped = ctx.call(mref, &[diff, min, max], IsPure::NOT);
            ctx.int_cast(diff_capped, Int::I16, cilly::cilnode::ExtendKind::SignExtend)
        }
        Type::Int(Int::I8) => {
            let a = ctx.int_cast(a, Int::I32, cilly::cilnode::ExtendKind::SignExtend);
            let b = ctx.int_cast(b, Int::I32, cilly::cilnode::ExtendKind::SignExtend);
            let diff = ctx.biop(a, b, cilly::BinOp::Add);
            let mref = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I32),
                        Type::Int(Int::I32),
                        Type::Int(Int::I32),
                    ],
                    Type::Int(Int::I32),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let min = ctx.alloc_node(i32::from(i8::MIN));
            let max = ctx.alloc_node(i32::from(i8::MAX));
            let diff_capped = ctx.call(mref, &[diff, min, max], IsPure::NOT);
            ctx.int_cast(diff_capped, Int::I8, cilly::cilnode::ExtendKind::SignExtend)
        }
        Type::Int(Int::I128) => {
            // There is no integer wider than 128 bits to widen-and-clamp into (the trick used by
            // the <=64-bit signed arms), so detect overflow from the sign bits directly:
            // signed add overflows iff both operands share a sign and the result's sign differs,
            // i.e. `(a ^ sum) & (b ^ sum) < 0`. On overflow, saturate toward the operands' shared
            // sign: `i128::MAX` if `b >= 0`, else `i128::MIN`.
            let sum = crate::binop::add_unchecked(a_ty, a_ty, ctx, a, b);
            let a_xor_sum = crate::binop::bitop::bit_xor_unchecked(a_ty, a_ty, ctx, a, sum);
            let b_xor_sum = crate::binop::bitop::bit_xor_unchecked(a_ty, a_ty, ctx, b, sum);
            let and = crate::binop::bitop::bit_and_unchecked(a_ty, a_ty, ctx, a_xor_sum, b_xor_sum);
            let zero = ctx.alloc_node(0_i128);
            let overflow = crate::binop::cmp::lt_unchecked(a_ty, and, zero, ctx);
            // Pick the saturation target based on the sign of `b`.
            let zero2 = ctx.alloc_node(0_i128);
            let b_neg = crate::binop::cmp::lt_unchecked(a_ty, b, zero2, ctx);
            let max = ctx.alloc_node(i128::MAX);
            let min = ctx.alloc_node(i128::MIN);
            let saturated = ctx.select(a_type, min, max, b_neg);
            ctx.select(a_type, saturated, sum, overflow)
        }
        _ => todo!("Can't use the intrinsic `saturating_add` on {a_type:?}"),
    };
    place_set(destination, calc, ctx)
}
pub fn saturating_sub<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let a_ty = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("saturating_sub works only on types!"),
    );
    let a_type = ctx.type_from_cache(a_ty);
    let calc = match a_type {
        Type::Int(Int::U128 | Int::U64 | Int::U32 | Int::U16 | Int::U8 | Int::USize) => {
            let undeflow = crate::binop::cmp::lt_unchecked(a_ty, a, b, ctx);
            let diff = crate::binop::sub_unchecked(a_ty, a_ty, ctx, a, b);
            let zero = crate::binop::checked::zero(a_ty, ctx);
            ctx.select(a_type, zero, diff, undeflow)
        }
        Type::Int(Int::I64) => {
            let a = crate::casts::int_to_int(Type::Int(Int::I64), Type::Int(Int::I128), a, ctx);
            let b = crate::casts::int_to_int(Type::Int(Int::I64), Type::Int(Int::I128), b, ctx);
            let sub = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_Subtraction"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let sub = ctx.alloc_methodref(sub);
            let diff = ctx.call(sub, &[a, b], IsPure::NOT);
            let clamp = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I128),
                        Type::Int(Int::I128),
                        Type::Int(Int::I128),
                    ],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let clamp = ctx.alloc_methodref(clamp);
            #[allow(clippy::cast_sign_loss)]
            let min = ctx.alloc_node((i128::from(i64::MIN) as u128) as i128);
            #[allow(clippy::cast_sign_loss)]
            let max = ctx.alloc_node((i128::from(i64::MAX) as u128) as i128);
            let diff_capped = ctx.call(clamp, &[diff, min, max], IsPure::NOT);
            crate::casts::int_to_int(Type::Int(Int::I128), Type::Int(Int::I64), diff_capped, ctx)
        }
        Type::Int(Int::ISize) => {
            let a = crate::casts::int_to_int(Type::Int(Int::ISize), Type::Int(Int::I128), a, ctx);
            let b = crate::casts::int_to_int(Type::Int(Int::ISize), Type::Int(Int::I128), b, ctx);
            let sub = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_Subtraction"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let sub = ctx.alloc_methodref(sub);
            let diff = ctx.call(sub, &[a, b], IsPure::NOT);
            let clamp = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I128),
                        Type::Int(Int::I128),
                        Type::Int(Int::I128),
                    ],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let clamp = ctx.alloc_methodref(clamp);
            // TODO: this assumes isize::MAX == i64::MAX
            #[allow(clippy::cast_sign_loss)]
            let min = ctx.alloc_node((i128::from(i64::MIN) as u128) as i128);
            #[allow(clippy::cast_sign_loss)]
            let max = ctx.alloc_node((i128::from(i64::MAX) as u128) as i128);
            let diff_capped = ctx.call(clamp, &[diff, min, max], IsPure::NOT);
            crate::casts::int_to_int(
                Type::Int(Int::I128),
                Type::Int(Int::ISize),
                diff_capped,
                ctx,
            )
        }
        Type::Int(Int::I32) => {
            let a = ctx.int_cast(a, Int::I64, cilly::cilnode::ExtendKind::SignExtend);
            let b = ctx.int_cast(b, Int::I64, cilly::cilnode::ExtendKind::SignExtend);
            let diff = ctx.biop(a, b, cilly::BinOp::Sub);
            let clamp = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I64),
                        Type::Int(Int::I64),
                        Type::Int(Int::I64),
                    ],
                    Type::Int(Int::I64),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let clamp = ctx.alloc_methodref(clamp);
            let min = ctx.alloc_node(i64::from(i32::MIN));
            let max = ctx.alloc_node(i64::from(i32::MAX));
            let diff_capped = ctx.call(clamp, &[diff, min, max], IsPure::NOT);
            ctx.int_cast(diff_capped, Int::I32, cilly::cilnode::ExtendKind::SignExtend)
        }
        Type::Int(Int::I16) => {
            let a = ctx.int_cast(a, Int::I32, cilly::cilnode::ExtendKind::SignExtend);
            let b = ctx.int_cast(b, Int::I32, cilly::cilnode::ExtendKind::SignExtend);
            let diff = ctx.biop(a, b, cilly::BinOp::Sub);
            let clamp = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I32),
                        Type::Int(Int::I32),
                        Type::Int(Int::I32),
                    ],
                    Type::Int(Int::I32),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let clamp = ctx.alloc_methodref(clamp);
            let min = ctx.alloc_node(i32::from(i16::MIN));
            let max = ctx.alloc_node(i32::from(i16::MAX));
            let diff_capped = ctx.call(clamp, &[diff, min, max], IsPure::NOT);
            ctx.int_cast(diff_capped, Int::I16, cilly::cilnode::ExtendKind::SignExtend)
        }
        Type::Int(Int::I8) => {
            let a = ctx.int_cast(a, Int::I32, cilly::cilnode::ExtendKind::SignExtend);
            let b = ctx.int_cast(b, Int::I32, cilly::cilnode::ExtendKind::SignExtend);
            let diff = ctx.biop(a, b, cilly::BinOp::Sub);
            let clamp = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Clamp"),
                ctx.sig(
                    [
                        Type::Int(Int::I32),
                        Type::Int(Int::I32),
                        Type::Int(Int::I32),
                    ],
                    Type::Int(Int::I32),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let clamp = ctx.alloc_methodref(clamp);
            let min = ctx.alloc_node(i32::from(i8::MIN));
            let max = ctx.alloc_node(i32::from(i8::MAX));
            let diff_capped = ctx.call(clamp, &[diff, min, max], IsPure::NOT);
            ctx.int_cast(diff_capped, Int::I8, cilly::cilnode::ExtendKind::SignExtend)
        }
        Type::Int(Int::I128) => {
            // No wider integer exists to clamp into (see the I128 add arm). Signed sub overflows
            // iff the operands have different signs and the result's sign differs from `a`, i.e.
            // `(a ^ b) & (a ^ diff) < 0`. On overflow saturate to `i128::MAX` when `b < 0` (the
            // subtraction pushed the value up) else to `i128::MIN`.
            let diff = crate::binop::sub_unchecked(a_ty, a_ty, ctx, a, b);
            let a_xor_b = crate::binop::bitop::bit_xor_unchecked(a_ty, a_ty, ctx, a, b);
            let a_xor_diff = crate::binop::bitop::bit_xor_unchecked(a_ty, a_ty, ctx, a, diff);
            let and = crate::binop::bitop::bit_and_unchecked(a_ty, a_ty, ctx, a_xor_b, a_xor_diff);
            let zero = ctx.alloc_node(0_i128);
            let overflow = crate::binop::cmp::lt_unchecked(a_ty, and, zero, ctx);
            let zero2 = ctx.alloc_node(0_i128);
            let b_neg = crate::binop::cmp::lt_unchecked(a_ty, b, zero2, ctx);
            let max = ctx.alloc_node(i128::MAX);
            let min = ctx.alloc_node(i128::MIN);
            let saturated = ctx.select(a_type, max, min, b_neg);
            ctx.select(a_type, saturated, diff, overflow)
        }
        _ => todo!("Can't use the intrinsic `saturating_sub` on {a_type:?}"),
    };
    place_set(destination, calc, ctx)
}
