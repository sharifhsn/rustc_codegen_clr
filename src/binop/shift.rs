use crate::{assembly::MethodCompileCtx, utilis::compiletime_sizeof};
use cilly::{
    cilnode::{ExtendKind, IsPure, MethodKind},
    BinOp, Int, Interned, Type,
    {ClassRef, MethodRef},
};

use rustc_codegen_clr_type::GetTypeExt;

use rustc_middle::ty::{IntTy, Ty, TyKind, UintTy};

type Node = Interned<cilly::ir::CILNode>;

fn ci32(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::I32, ExtendKind::SignExtend)
}
fn cu32(ctx: &mut MethodCompileCtx<'_, '_>, v: Node) -> Node {
    ctx.int_cast(v, Int::U32, ExtendKind::ZeroExtend)
}

pub fn shr_unchecked<'tcx>(
    value_type: Ty<'tcx>,
    shift_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    let type_b = ctx.type_from_cache(shift_type);
    match value_type.kind() {
        TyKind::Uint(UintTy::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("op_RightShift"),
                ctx.sig(
                    [Type::Int(Int::U128), Type::Int(Int::I32)],
                    Type::Int(Int::U128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, b], IsPure::NOT)
        }
        TyKind::Int(IntTy::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_RightShift"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I32)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, b], IsPure::NOT)
        }
        TyKind::Uint(_) => match shift_type.kind() {
            TyKind::Uint(UintTy::U128 | UintTy::U64) | TyKind::Int(IntTy::I128 | IntTy::I64) => {
                let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
                ctx.biop(ops_a, b, BinOp::ShrUn)
            }
            _ => ctx.biop(ops_a, ops_b, BinOp::ShrUn),
        },
        TyKind::Int(_) => match shift_type.kind() {
            TyKind::Uint(UintTy::U128 | UintTy::U64) | TyKind::Int(IntTy::I128 | IntTy::I64) => {
                let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
                ctx.biop(ops_a, b, BinOp::Shr)
            }

            _ => ctx.biop(ops_a, ops_b, BinOp::Shr),
        },
        _ => panic!("Can't bitshift type  {value_type:?}"),
    }
}

pub fn shr_checked<'tcx>(
    value_type: Ty<'tcx>,
    shift_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    let type_b = ctx.type_from_cache(shift_type);
    let bit_cap = u32::try_from(compiletime_sizeof(value_type, ctx.tcx()) * 8)
        .expect("Intiger size over 2^32 bits.");
    match value_type.kind() {
        TyKind::Uint(UintTy::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("op_RightShift"),
                ctx.sig(
                    [Type::Int(Int::U128), Type::Int(Int::I32)],
                    Type::Int(Int::U128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::U32), ops_b, ctx);
            let cap = ctx.alloc_node(128_u32);
            let b = ctx.biop(b, cap, BinOp::RemUn);
            let b = ci32(ctx, b);
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, b], IsPure::NOT)
        }
        TyKind::Int(IntTy::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_RightShift"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I32)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::U32), ops_b, ctx);
            let cap = ctx.alloc_node(128_u32);
            let b = ctx.biop(b, cap, BinOp::RemUn);
            let b = ci32(ctx, b);
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, b], IsPure::NOT)
        }
        TyKind::Uint(_) => match shift_type.kind() {
            TyKind::Uint(UintTy::U128 | UintTy::U64) | TyKind::Int(IntTy::I128 | IntTy::I64) => {
                let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
                let b = cu32(ctx, b);
                let cap = ctx.alloc_node(bit_cap);
                let b = ctx.biop(b, cap, BinOp::RemUn);
                ctx.biop(ops_a, b, BinOp::ShrUn)
            }
            _ => {
                let b = cu32(ctx, ops_b);
                let cap = ctx.alloc_node(bit_cap);
                let b = ctx.biop(b, cap, BinOp::RemUn);
                ctx.biop(ops_a, b, BinOp::ShrUn)
            }
        },
        TyKind::Int(_) => match shift_type.kind() {
            TyKind::Uint(UintTy::U128 | UintTy::U64) | TyKind::Int(IntTy::I128 | IntTy::I64) => {
                let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
                let b = cu32(ctx, b);
                let cap = ctx.alloc_node(bit_cap);
                let b = ctx.biop(b, cap, BinOp::RemUn);
                ctx.biop(ops_a, b, BinOp::Shr)
            }
            _ => {
                let b = cu32(ctx, ops_b);
                let cap = ctx.alloc_node(bit_cap);
                let b = ctx.biop(b, cap, BinOp::RemUn);
                ctx.biop(ops_a, b, BinOp::Shr)
            }
        },
        _ => panic!("Can't bitshift type  {value_type:?}"),
    }
}

pub fn shl_checked<'tcx>(
    value_type: Ty<'tcx>,
    shift_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    let type_b = ctx.type_from_cache(shift_type);
    let bit_cap = u32::try_from(compiletime_sizeof(value_type, ctx.tcx()) * 8)
        .expect("Intiger has over 2^32 bits.");
    match value_type.kind() {
        TyKind::Uint(UintTy::U128) => {
            let mref = ctx.static_mref(
                "shl_u128",
                [Type::Int(Int::U128), Type::Int(Int::I32)],
                Type::Int(Int::U128),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::U32), ops_b, ctx);
            let b = cu32(ctx, b);
            let cap = ctx.alloc_node(bit_cap);
            let b = ctx.biop(b, cap, BinOp::RemUn);
            let b = ci32(ctx, b);
            ctx.call(mref, &[ops_a, b], IsPure::NOT)
        }
        TyKind::Int(IntTy::I128) => {
            let mref = ctx.static_mref(
                "shl_i128",
                [Type::Int(Int::I128), Type::Int(Int::I32)],
                Type::Int(Int::I128),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::U32), ops_b, ctx);
            let b = cu32(ctx, b);
            let cap = ctx.alloc_node(bit_cap);
            let b = ctx.biop(b, cap, BinOp::RemUn);
            let b = ci32(ctx, b);
            ctx.call(mref, &[ops_a, b], IsPure::NOT)
        }
        TyKind::Uint(_) => match shift_type.kind() {
            TyKind::Uint(UintTy::U128 | UintTy::U64) | TyKind::Int(IntTy::I128 | IntTy::I64) => {
                let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
                let b = cu32(ctx, b);
                let cap = ctx.alloc_node(bit_cap);
                let b = ctx.biop(b, cap, BinOp::RemUn);
                ctx.biop(ops_a, b, BinOp::Shl)
            }
            _ => {
                let b = cu32(ctx, ops_b);
                let cap = ctx.alloc_node(bit_cap);
                let b = ctx.biop(b, cap, BinOp::RemUn);
                ctx.biop(ops_a, b, BinOp::Shl)
            }
        },
        TyKind::Int(_) => match shift_type.kind() {
            TyKind::Uint(UintTy::U128 | UintTy::U64) | TyKind::Int(IntTy::I128 | IntTy::I64) => {
                let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
                let b = cu32(ctx, b);
                let cap = ctx.alloc_node(bit_cap);
                let b = ctx.biop(b, cap, BinOp::RemUn);
                ctx.biop(ops_a, b, BinOp::Shl)
            }

            _ => {
                let b = cu32(ctx, ops_b);
                let cap = ctx.alloc_node(bit_cap);
                let b = ctx.biop(b, cap, BinOp::RemUn);
                ctx.biop(ops_a, b, BinOp::Shl)
            }
        },
        _ => panic!("Can't bitshift type  {value_type:?}"),
    }
}

pub fn shl_unchecked<'tcx>(
    value_type: Ty<'tcx>,
    shift_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    let type_b = ctx.type_from_cache(shift_type);
    match value_type.kind() {
        TyKind::Uint(UintTy::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("op_LeftShift"),
                ctx.sig(
                    [Type::Int(Int::U128), Type::Int(Int::I32)],
                    Type::Int(Int::U128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, b], IsPure::NOT)
        }
        TyKind::Int(IntTy::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("op_LeftShift"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I32)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, b], IsPure::NOT)
        }
        TyKind::Uint(_) | TyKind::Int(_) => match shift_type.kind() {
            TyKind::Uint(UintTy::U128 | UintTy::U64) | TyKind::Int(IntTy::I128 | IntTy::I64) => {
                let b = crate::casts::int_to_int(type_b, Type::Int(Int::I32), ops_b, ctx);
                ctx.biop(ops_a, b, BinOp::Shl)
            }
            _ => ctx.biop(ops_a, ops_b, BinOp::Shl),
        },
        _ => panic!("Can't bitshift type  {value_type:?}"),
    }
}
