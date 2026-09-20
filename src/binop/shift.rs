use crate::{assembly::MethodCompileCtx, utilis::const_sizeof};
use cilly::{
    BinOp, Int, Interned, MethodRef, Type,
    cilnode::{ExtendKind, IsPure, MethodKind},
};

use crate::r#type::GetTypeExt;

use rustc_middle::ty::{IntTy, Ty, TyKind, UintTy};

type Node = Interned<cilly::ir::CILNode>;

fn ci32(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::I32, ExtendKind::SignExtend)
}
fn cu32(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::U32, ExtendKind::ZeroExtend)
}

fn wide_shift_count(
    type_b: Type,
    shift: Node,
    checked: bool,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Node {
    let target = if checked { Int::U32 } else { Int::I32 };
    let mut shift = crate::casts::int_to_int(type_b, Type::Int(target), shift, ctx);
    if checked {
        let cap = ctx.alloc_node(128_u32);
        shift = ctx.biop(shift, cap, BinOp::RemUn);
        shift = ci32(ctx, shift);
    }
    shift
}

fn wide_bcl_shift(
    int: Int,
    type_b: Type,
    value: Node,
    shift: Node,
    checked: bool,
    operation: &'static str,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Node {
    let class = int.class(ctx);
    let name = ctx.alloc_string(operation);
    let sig = ctx.sig([Type::Int(int), Type::Int(Int::I32)], Type::Int(int));
    let method = ctx.alloc_methodref(MethodRef::new(
        class,
        name,
        sig,
        MethodKind::Static,
        vec![].into(),
    ));
    let shift = wide_shift_count(type_b, shift, checked, ctx);
    ctx.call(method, &[value, shift], IsPure::NOT)
}

fn wide_builtin_shift(
    int: Int,
    type_b: Type,
    value: Node,
    shift: Node,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Node {
    let name = match int {
        Int::U128 => "shl_u128",
        Int::I128 => "shl_i128",
        _ => unreachable!("wide shift helper only supports 128-bit integers"),
    };
    let method = ctx.static_mref(name, [Type::Int(int), Type::Int(Int::I32)], Type::Int(int));
    let shift = wide_shift_count(type_b, shift, true, ctx);
    ctx.call(method, &[value, shift], IsPure::NOT)
}

fn shift<'tcx>(
    value_type: Ty<'tcx>,
    shift_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    value: Node,
    shift: Node,
    signed_op: BinOp,
    unsigned_op: BinOp,
    checked: bool,
    wide_builtin: bool,
    wide_operation: &'static str,
) -> Node {
    let type_b = ctx.type_from_cache(shift_type);
    let wide_int = match value_type.kind() {
        TyKind::Uint(UintTy::U128) => Some(Int::U128),
        TyKind::Int(IntTy::I128) => Some(Int::I128),
        _ => None,
    };
    if let Some(int) = wide_int {
        return if checked && wide_builtin {
            wide_builtin_shift(int, type_b, value, shift, ctx)
        } else {
            wide_bcl_shift(int, type_b, value, shift, checked, wide_operation, ctx)
        };
    }

    let op = match value_type.kind() {
        TyKind::Uint(_) => unsigned_op,
        TyKind::Int(_) => signed_op,
        _ => panic!("Can't bitshift type  {value_type:?}"),
    };
    let mut shift = match shift_type.kind() {
        TyKind::Uint(UintTy::U128 | UintTy::U64) | TyKind::Int(IntTy::I128 | IntTy::I64) => {
            crate::casts::int_to_int(type_b, Type::Int(Int::I32), shift, ctx)
        }
        _ => shift,
    };
    if checked {
        shift = cu32(ctx, shift);
        let bit_cap = u32::try_from(const_sizeof(value_type, ctx.tcx()) * 8)
            .expect("Intiger size over 2^32 bits.");
        let cap = ctx.alloc_node(bit_cap);
        shift = ctx.biop(shift, cap, BinOp::RemUn);
    }
    ctx.biop(value, shift, op)
}

pub fn shr_unchecked<'tcx>(
    value_type: Ty<'tcx>,
    shift_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    shift(
        value_type,
        shift_type,
        ctx,
        ops_a,
        ops_b,
        BinOp::Shr,
        BinOp::ShrUn,
        false,
        false,
        "op_RightShift",
    )
}

pub fn shr_checked<'tcx>(
    value_type: Ty<'tcx>,
    shift_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    shift(
        value_type,
        shift_type,
        ctx,
        ops_a,
        ops_b,
        BinOp::Shr,
        BinOp::ShrUn,
        true,
        false,
        "op_RightShift",
    )
}

pub fn shl_checked<'tcx>(
    value_type: Ty<'tcx>,
    shift_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    shift(
        value_type,
        shift_type,
        ctx,
        ops_a,
        ops_b,
        BinOp::Shl,
        BinOp::Shl,
        true,
        true,
        "op_LeftShift",
    )
}

pub fn shl_unchecked<'tcx>(
    value_type: Ty<'tcx>,
    shift_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    shift(
        value_type,
        shift_type,
        ctx,
        ops_a,
        ops_b,
        BinOp::Shl,
        BinOp::Shl,
        false,
        false,
        "op_LeftShift",
    )
}
