use crate::assembly::MethodCompileCtx;
use crate::place::place_set;
use crate::r#type::GetTypeExt;
use cilly::{
    BinOp, Interned, MethodRef, Type,
    cilnode::{IsPure, MethodKind},
    {ClassRef, Int},
};

use crate::operand::handle_operand;
use rustc_middle::{
    mir::{Operand, Place},
    ty::Instance,
};
use rustc_span::Spanned;

type Root = Interned<cilly::ir::CILRoot>;
type Node = Interned<cilly::ir::CILNode>;

fn clamp(
    value: Node,
    class: Interned<ClassRef>,
    int: Int,
    min: Node,
    max: Node,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Node {
    let method = MethodRef::new(
        class,
        ctx.alloc_string("Clamp"),
        ctx.sig(
            [Type::Int(int), Type::Int(int), Type::Int(int)],
            Type::Int(int),
        ),
        MethodKind::Static,
        vec![].into(),
    );
    let method = ctx.alloc_methodref(method);
    ctx.call(method, &[value, min, max], IsPure::NOT)
}

fn signed_narrow(
    a: Node,
    b: Node,
    ctx: &mut MethodCompileCtx<'_, '_>,
    operation: BinOp,
    wide: Int,
    result: Int,
    min: Node,
    max: Node,
) -> Node {
    let a = ctx.int_cast(a, wide, cilly::cilnode::ExtendKind::SignExtend);
    let b = ctx.int_cast(b, wide, cilly::cilnode::ExtendKind::SignExtend);
    let value = ctx.biop(a, b, operation);
    let value = clamp(value, ClassRef::math(ctx), wide, min, max, ctx);
    ctx.int_cast(value, result, cilly::cilnode::ExtendKind::SignExtend)
}

fn signed_wide(
    a: Node,
    b: Node,
    ctx: &mut MethodCompileCtx<'_, '_>,
    source: Int,
    result: Int,
    operation: &'static str,
    min: Node,
    max: Node,
) -> Node {
    let a = crate::casts::int_to_int(Type::Int(source), Type::Int(Int::I128), a, ctx);
    let b = crate::casts::int_to_int(Type::Int(source), Type::Int(Int::I128), b, ctx);
    let class = ClassRef::int_128(ctx);
    let name = ctx.alloc_string(operation);
    let sig = ctx.sig(
        [Type::Int(Int::I128), Type::Int(Int::I128)],
        Type::Int(Int::I128),
    );
    let method = ctx.alloc_methodref(MethodRef::new(
        class,
        name,
        sig,
        MethodKind::Static,
        vec![].into(),
    ));
    let value = ctx.call(method, &[a, b], IsPure::NOT);
    let value = clamp(value, ClassRef::int_128(ctx), Int::I128, min, max, ctx);
    crate::casts::int_to_int(Type::Int(Int::I128), Type::Int(result), value, ctx)
}

fn saturating_impl<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
    add: bool,
) -> Root {
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let a_ty = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("saturating intrinsic works only on types!"),
    );
    let a_type = ctx.type_from_cache(a_ty);
    let calc = match a_type {
        Type::Int(int @ (Int::USize | Int::U128 | Int::U64 | Int::U32 | Int::U16 | Int::U8))
            if add =>
        {
            let sum = crate::binop::add_unchecked(a_ty, a_ty, ctx, a, b);
            let or = crate::binop::bitop::bit_or_unchecked(a_ty, a_ty, ctx, a, b);
            let flag = crate::binop::cmp::lt_unchecked(a_ty, sum, or, ctx);
            let max_value = int.max(ctx);
            let max = ctx.alloc_node(max_value);
            ctx.select(a_type, max, sum, flag)
        }
        Type::Int(_int @ (Int::USize | Int::U128 | Int::U64 | Int::U32 | Int::U16 | Int::U8)) => {
            let underflow = crate::binop::cmp::lt_unchecked(a_ty, a, b, ctx);
            let diff = crate::binop::sub_unchecked(a_ty, a_ty, ctx, a, b);
            let zero = crate::binop::checked::zero(a_ty, ctx);
            ctx.select(a_type, zero, diff, underflow)
        }
        Type::Int(int @ (Int::I32 | Int::I16 | Int::I8)) => {
            let (wide, min, max) = match int {
                Int::I32 => (Int::I64, i128::from(i32::MIN), i128::from(i32::MAX)),
                Int::I16 => (Int::I32, i128::from(i16::MIN), i128::from(i16::MAX)),
                Int::I8 => (Int::I32, i128::from(i8::MIN), i128::from(i8::MAX)),
                _ => unreachable!(),
            };
            // `Clamp` is declared at the widened integer width.  Keep its bounds at that same
            // width instead of passing the `i128` literals through as `Int::I128` nodes; the
            // latter is rejected by the fatal verifier for i16/i8 (and similarly i32) cases.
            let (min, max) = match wide {
                Int::I32 => (
                    ctx.alloc_node(i32::try_from(min).expect("i32 saturating bound")),
                    ctx.alloc_node(i32::try_from(max).expect("i32 saturating bound")),
                ),
                Int::I64 => (
                    ctx.alloc_node(i64::try_from(min).expect("i64 saturating bound")),
                    ctx.alloc_node(i64::try_from(max).expect("i64 saturating bound")),
                ),
                _ => unreachable!("signed narrow saturation uses i32 or i64 widening"),
            };
            signed_narrow(
                a,
                b,
                ctx,
                if add { BinOp::Add } else { BinOp::Sub },
                wide,
                int,
                min,
                max,
            )
        }
        Type::Int(int @ (Int::I64 | Int::ISize)) => {
            let (min, max) = match int {
                Int::I64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
                Int::ISize => (
                    ctx.target_layout().signed_pointer_min(),
                    ctx.target_layout().signed_pointer_max(),
                ),
                _ => unreachable!(),
            };
            let min = ctx.alloc_node(min);
            let max = ctx.alloc_node(max);
            signed_wide(
                a,
                b,
                ctx,
                int,
                int,
                if add { "op_Addition" } else { "op_Subtraction" },
                min,
                max,
            )
        }
        Type::Int(Int::I128) => {
            if add {
                // No wider integer exists to clamp into. Signed add overflows iff both operands
                // share a sign and the result's sign differs: `(a ^ sum) & (b ^ sum) < 0`.
                let sum = crate::binop::add_unchecked(a_ty, a_ty, ctx, a, b);
                let a_xor_sum = crate::binop::bitop::bit_xor_unchecked(a_ty, a_ty, ctx, a, sum);
                let b_xor_sum = crate::binop::bitop::bit_xor_unchecked(a_ty, a_ty, ctx, b, sum);
                let and =
                    crate::binop::bitop::bit_and_unchecked(a_ty, a_ty, ctx, a_xor_sum, b_xor_sum);
                let overflow =
                    crate::binop::cmp::lt_unchecked(a_ty, and, ctx.alloc_node(0_i128), ctx);
                let b_neg = crate::binop::cmp::lt_unchecked(a_ty, b, ctx.alloc_node(0_i128), ctx);
                let min = ctx.alloc_node(i128::MIN);
                let max = ctx.alloc_node(i128::MAX);
                let saturated = ctx.select(a_type, min, max, b_neg);
                ctx.select(a_type, saturated, sum, overflow)
            } else {
                // Signed sub overflows iff the operands have different signs and the result's sign
                // differs from `a`: `(a ^ b) & (a ^ diff) < 0`.
                let diff = crate::binop::sub_unchecked(a_ty, a_ty, ctx, a, b);
                let a_xor_b = crate::binop::bitop::bit_xor_unchecked(a_ty, a_ty, ctx, a, b);
                let a_xor_diff = crate::binop::bitop::bit_xor_unchecked(a_ty, a_ty, ctx, a, diff);
                let and =
                    crate::binop::bitop::bit_and_unchecked(a_ty, a_ty, ctx, a_xor_b, a_xor_diff);
                let overflow =
                    crate::binop::cmp::lt_unchecked(a_ty, and, ctx.alloc_node(0_i128), ctx);
                let b_neg = crate::binop::cmp::lt_unchecked(a_ty, b, ctx.alloc_node(0_i128), ctx);
                let max = ctx.alloc_node(i128::MAX);
                let min = ctx.alloc_node(i128::MIN);
                let saturated = ctx.select(a_type, max, min, b_neg);
                ctx.select(a_type, saturated, diff, overflow)
            }
        }
        _ => todo!(
            "Can't use the intrinsic `{}` on {a_type:?}",
            if add {
                "saturating_add"
            } else {
                "saturating_sub"
            }
        ),
    };
    place_set(destination, calc, ctx)
}

pub fn saturating_add<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    saturating_impl(args, destination, ctx, call_instance, true)
}

pub fn saturating_sub<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    saturating_impl(args, destination, ctx, call_instance, false)
}
