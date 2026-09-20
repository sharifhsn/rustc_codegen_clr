use crate::assembly::MethodCompileCtx;
use crate::r#type::GetTypeExt;
use cilly::{
    BinOp, Interned, Type,
    cilnode::{IsPure, MethodKind},
    {ClassRef, Int, MethodRef},
};
use rustc_middle::ty::{IntTy, Ty, TyKind, UintTy};
use rustc_span::span_bug;

type Node = Interned<cilly::ir::CILNode>;

fn wide_bitop(
    ctx: &mut MethodCompileCtx<'_, '_>,
    int: Int,
    method: &'static str,
    operand_a: Node,
    operand_b: Node,
    type_b: Type,
    cast_b: bool,
) -> Node {
    let b_type = if cast_b { Type::Int(int) } else { type_b };
    let operand_b = if cast_b {
        crate::casts::int_to_int(type_b, b_type, operand_b, ctx)
    } else {
        operand_b
    };
    let class = if int.is_signed() {
        ClassRef::int_128(ctx)
    } else {
        ClassRef::uint_128(ctx)
    };
    let mref = MethodRef::new(
        class,
        ctx.alloc_string(method),
        ctx.sig([Type::Int(int), b_type], Type::Int(int)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = ctx.alloc_methodref(mref);
    ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
}

fn bitop_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand_a: Node,
    operand_b: Node,
    op: BinOp,
    method: &'static str,
    cast_b: bool,
    reject_ptr: bool,
) -> Node {
    let type_b = ctx.type_from_cache(ty_b);
    match ty_a.kind() {
        TyKind::Uint(UintTy::U128) => {
            wide_bitop(ctx, Int::U128, method, operand_a, operand_b, type_b, cast_b)
        }
        TyKind::Int(IntTy::I128) => {
            wide_bitop(ctx, Int::I128, method, operand_a, operand_b, type_b, cast_b)
        }
        TyKind::RawPtr(..) if reject_ptr => span_bug!(ctx.span(), "bitand of ptr"),
        _ => ctx.biop(operand_a, operand_b, op),
    }
}

pub fn bit_and_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand_a: Node,
    operand_b: Node,
) -> Node {
    bitop_unchecked(
        ty_a,
        ty_b,
        ctx,
        operand_a,
        operand_b,
        BinOp::And,
        "op_BitwiseAnd",
        true,
        true,
    )
}

pub fn bit_or_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand_a: Node,
    operand_b: Node,
) -> Node {
    bitop_unchecked(
        ty_a,
        ty_b,
        ctx,
        operand_a,
        operand_b,
        BinOp::Or,
        "op_BitwiseOr",
        false,
        false,
    )
}

pub fn bit_xor_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand_a: Node,
    operand_b: Node,
) -> Node {
    bitop_unchecked(
        ty_a,
        ty_b,
        ctx,
        operand_a,
        operand_b,
        BinOp::XOr,
        "op_ExclusiveOr",
        false,
        false,
    )
}
