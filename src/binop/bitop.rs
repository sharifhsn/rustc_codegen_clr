use crate::assembly::MethodCompileCtx;
use cilly::{
    cilnode::{IsPure, MethodKind},
    BinOp, Interned, Type,
    {ClassRef, Int, MethodRef},
};
use crate::r#type::GetTypeExt;
use rustc_middle::span_bug;
use rustc_middle::ty::{IntTy, Ty, TyKind, UintTy};

type Node = Interned<cilly::ir::CILNode>;

pub fn bit_and_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand_a: Node,
    operand_b: Node,
) -> Node {
    let type_b = ctx.type_from_cache(ty_b);
    match ty_a.kind() {
        TyKind::Uint(UintTy::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("op_BitwiseAnd"),
                ctx.sig(
                    [Type::Int(Int::U128), Type::Int(Int::U128)],
                    Type::Int(Int::U128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::U128), operand_b, ctx);
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, b], IsPure::NOT)
        }
        TyKind::Int(IntTy::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_BitwiseAnd"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::I128), operand_b, ctx);
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, b], IsPure::NOT)
        }
        TyKind::RawPtr(..) => span_bug!(ctx.span(), "bitand of ptr"),
        _ => ctx.biop(operand_a, operand_b, BinOp::And),
    }
}
pub fn bit_or_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand_a: Node,
    operand_b: Node,
) -> Node {
    match ty_a.kind() {
        TyKind::Int(IntTy::I128) => {
            let ty_a = ctx.type_from_cache(ty_a);
            let ty_b = ctx.type_from_cache(ty_b);
            let mref = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_BitwiseOr"),
                ctx.sig([ty_a, ty_b], ty_a),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Uint(UintTy::U128) => {
            let ty_a = ctx.type_from_cache(ty_a);
            let ty_b = ctx.type_from_cache(ty_b);
            let mref = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("op_BitwiseOr"),
                ctx.sig([ty_a, ty_b], ty_a),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        _ => ctx.biop(operand_a, operand_b, BinOp::Or),
    }
}
pub fn bit_xor_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    match ty_a.kind() {
        TyKind::Int(IntTy::I128) => {
            let ty_a = ctx.type_from_cache(ty_a);
            let ty_b = ctx.type_from_cache(ty_b);
            let mref = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_ExclusiveOr"),
                ctx.sig([ty_a, ty_b], ty_a),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        TyKind::Uint(UintTy::U128) => {
            let ty_a = ctx.type_from_cache(ty_a);
            let ty_b = ctx.type_from_cache(ty_b);
            let mref = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("op_ExclusiveOr"),
                ctx.sig([ty_a, ty_b], ty_a),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        _ => ctx.biop(ops_a, ops_b, BinOp::XOr),
    }
}
