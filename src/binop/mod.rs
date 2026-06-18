use self::checked::{add_signed, add_unsigned, sub_signed, sub_unsigned};
use crate::assembly::MethodCompileCtx;
use bitop::{bit_and_unchecked, bit_or_unchecked, bit_xor_unchecked};
use cilly::{
    cilnode::{ExtendKind, IsPure},
    BinOp as V2BinOp, Interned, IntoAsmIndex, Type,
    {cilnode::MethodKind, Float, Int, MethodRef},
};
use cmp::{eq_unchecked, gt_unchecked, lt_unchecked, ne_unchecked};
use rustc_codegen_clr_type::{utilis::instance_try_resolve, GetTypeExt};

use rustc_codgen_clr_operand::handle_operand;
use rustc_hir::lang_items::LangItem;
use rustc_middle::{
    mir::{BinOp, Operand},
    ty::{FloatTy, IntTy, List, Ty, TyKind, UintTy},
};
use shift::{shl_checked, shl_unchecked, shr_checked, shr_unchecked};

pub mod bitop;
pub mod checked;
pub mod cmp;
pub mod shift;

type Node = Interned<cilly::ir::CILNode>;

/// Preforms an unchecked binary operation.
pub(crate) fn binop<'tcx>(
    binop: BinOp,
    operand_a: &Operand<'tcx>,
    operand_b: &Operand<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    let ops_a = handle_operand(operand_a, ctx);
    let ops_b = handle_operand(operand_b, ctx);
    let ty_a = operand_a.ty(&ctx.body().local_decls, ctx.tcx());
    let ty_b = operand_b.ty(&ctx.body().local_decls, ctx.tcx());
    match binop {
        BinOp::AddWithOverflow => {
            if ty_a.is_signed() {
                add_signed(ops_a, ops_b, ty_a, ctx)
            } else {
                add_unsigned(ops_a, ops_b, ty_a, ctx)
            }
        }
        BinOp::Add | BinOp::AddUnchecked => add_unchecked(ty_a, ty_b, ctx, ops_a, ops_b),
        BinOp::SubWithOverflow => {
            if ty_a.is_signed() {
                sub_signed(ops_a, ops_b, ty_a, ctx)
            } else {
                sub_unsigned(ops_a, ops_b, ty_a, ctx)
            }
        }
        BinOp::Sub | BinOp::SubUnchecked => sub_unchecked(ty_a, ty_b, ctx, ops_a, ops_b),
        BinOp::Ne => ne_unchecked(ty_a, ops_a, ops_b, ctx),
        BinOp::Eq => eq_unchecked(ty_a, ops_a, ops_b, ctx),
        BinOp::Lt => lt_unchecked(ty_a, ops_a, ops_b, ctx),
        BinOp::Gt => gt_unchecked(ty_a, ops_a, ops_b, ctx),
        BinOp::BitAnd => bit_and_unchecked(ty_a, ty_b, ctx, ops_a, ops_b),
        BinOp::BitOr => bit_or_unchecked(ty_a, ty_b, ctx, ops_a, ops_b),
        BinOp::BitXor => bit_xor_unchecked(ty_a, ty_b, ctx, ops_a, ops_b),
        BinOp::Rem => rem_unchecked(ty_a, ty_b, ctx, ops_a, ops_b),
        BinOp::Shl => shl_checked(ty_a, ty_b, ctx, ops_a, ops_b),
        BinOp::ShlUnchecked => shl_unchecked(ty_a, ty_b, ctx, ops_a, ops_b),
        BinOp::Shr => shr_checked(ty_a, ty_b, ctx, ops_a, ops_b),
        BinOp::ShrUnchecked => shr_unchecked(ty_a, ty_b, ctx, ops_a, ops_b),

        BinOp::Mul | BinOp::MulUnchecked => mul_unchecked(ty_a, ctx, ops_a, ops_b),
        BinOp::MulWithOverflow => checked::mul(ops_a, ops_b, ty_a, ctx),
        BinOp::Div => div_unchecked(ty_a, ctx, ops_a, ops_b),

        BinOp::Ge => match ty_a.kind() {
            // Unordered, to handle NaNs propely
            TyKind::Float(FloatTy::F32 | FloatTy::F64) => {
                let lt = ctx.biop(ops_a, ops_b, V2BinOp::LtUn);
                let f = ctx.alloc_node(false);
                ctx.biop(lt, f, V2BinOp::Eq)
            }
            TyKind::Float(FloatTy::F128) => {
                let mref = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string("__getf2"),
                    ctx.sig(
                        [Type::Float(Float::F128), Type::Float(Float::F128)],
                        Type::Bool,
                    ),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
            }
            _ => {
                let lt = lt_unchecked(ty_a, ops_a, ops_b, ctx);
                let f = ctx.alloc_node(false);
                ctx.biop(lt, f, V2BinOp::Eq)
            }
        },
        BinOp::Le => match ty_a.kind() {
            // Unordered, to handle NaNs propely
            TyKind::Float(FloatTy::F32 | FloatTy::F64) => {
                let gt = ctx.biop(ops_a, ops_b, V2BinOp::GtUn);
                let f = ctx.alloc_node(false);
                ctx.biop(gt, f, V2BinOp::Eq)
            }
            TyKind::Float(FloatTy::F128) => {
                let mref = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string("__letf2"),
                    ctx.sig(
                        [Type::Float(Float::F128), Type::Float(Float::F128)],
                        Type::Bool,
                    ),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
            }
            _ => {
                let gt = gt_unchecked(ty_a, ops_a, ops_b, ctx);
                let f = ctx.alloc_node(false);
                ctx.biop(gt, f, V2BinOp::Eq)
            }
        },
        BinOp::Offset => {
            let pointed_ty = if let TyKind::RawPtr(inner, _) = ty_a.kind() {
                *inner
            } else {
                todo!("Can't offset pointer of type {ty_a:?}");
            };
            let pointed_ty = ctx.monomorphize(pointed_ty);
            let layout = ctx.layout_of(pointed_ty);
            if layout.is_zst() {
                ops_a
            } else {
                let pointed_type = ctx.type_from_cache(pointed_ty);
                let offset_tpe = ctx.type_from_cache(ty_b);
                let size = ctx.size_of(pointed_type).into_idx(ctx);
                let scaled = crate::casts::int_to_int(Type::Int(Int::U64), offset_tpe, size, ctx);
                let off = ctx.biop(ops_b, scaled, V2BinOp::Mul);
                ctx.biop(ops_a, off, V2BinOp::Add)
            }
        }
        BinOp::Cmp => {
            let ordering = ctx
                .tcx()
                .get_lang_items(())
                .get(LangItem::OrderingEnum)
                .unwrap();
            let ordering = instance_try_resolve(ordering, ctx.tcx(), List::empty());
            let ordering_ty = ordering.ty(
                ctx.tcx(),
                rustc_middle::ty::TypingEnv::fully_monomorphized(),
            );
            let ordering_type = ctx.type_from_cache(ordering_ty);
            let lt = lt_unchecked(ty_a, ops_a, ops_b, ctx);
            let lt = ctx.int_cast(lt, Int::I8, ExtendKind::SignExtend);
            let lt = ctx.neg(lt);
            let gt = gt_unchecked(ty_a, ops_a, ops_b, ctx);
            let gt = ctx.int_cast(gt, Int::I8, ExtendKind::SignExtend);
            let res = ctx.biop(lt, gt, V2BinOp::Or);

            ctx.transmute_on_stack(Type::Int(Int::I8), ordering_type, res)
        }
    }
}
/// Preforms unchecked addition
pub fn add_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    match ty_a.kind() {
        TyKind::Int(int_ty) => {
            if let IntTy::I128 = int_ty {
                let mref = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string("add_i128"),
                    ctx.sig(
                        [Type::Int(Int::I128), Type::Int(Int::I128)],
                        Type::Int(Int::I128),
                    ),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
            } else {
                ctx.biop(ops_a, ops_b, V2BinOp::Add)
            }
        }
        TyKind::Uint(uint_ty) => {
            if let UintTy::U128 = uint_ty {
                let mref = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string("add_u128"),
                    ctx.sig(
                        [Type::Int(Int::U128), Type::Int(Int::U128)],
                        Type::Int(Int::U128),
                    ),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
            } else {
                let sum = ctx.biop(ops_a, ops_b, V2BinOp::Add);
                match uint_ty {
                    UintTy::U8 => ctx.int_cast(sum, Int::U8, ExtendKind::ZeroExtend),
                    UintTy::U16 => ctx.int_cast(sum, Int::U16, ExtendKind::ZeroExtend),
                    UintTy::U32 => ctx.int_cast(sum, Int::U32, ExtendKind::ZeroExtend),
                    UintTy::U64 => ctx.int_cast(sum, Int::U64, ExtendKind::ZeroExtend),
                    _ => sum,
                }
            }
        }
        TyKind::Float(FloatTy::F32 | FloatTy::F64) => ctx.biop(ops_a, ops_b, V2BinOp::Add),
        TyKind::Float(FloatTy::F128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("__addtf3"),
                ctx.sig(
                    [Type::Float(Float::F128), Type::Float(Float::F128)],
                    Type::Float(Float::F128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        TyKind::Float(FloatTy::F16) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("add_f16"),
                ctx.sig(
                    [Type::Float(Float::F16), Type::Float(Float::F16)],
                    Type::Float(Float::F16),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        _ => todo!("can't add numbers of types {ty_a} and {ty_b}"),
    }
}
/// Preforms unchecked subtraction
pub fn sub_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    match ty_a.kind() {
        TyKind::Int(int_ty) => {
            if let IntTy::I128 = int_ty {
                let mref = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string("sub_i128"),
                    ctx.sig(
                        [Type::Int(Int::I128), Type::Int(Int::I128)],
                        Type::Int(Int::I128),
                    ),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
            } else {
                ctx.biop(ops_a, ops_b, V2BinOp::Sub)
            }
        }
        TyKind::Uint(uint_ty) => {
            if let UintTy::U128 = uint_ty {
                let mref = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string("sub_u128"),
                    ctx.sig(
                        [Type::Int(Int::U128), Type::Int(Int::U128)],
                        Type::Int(Int::U128),
                    ),
                    MethodKind::Static,
                    vec![].into(),
                );
                let mref = ctx.alloc_methodref(mref);
                ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
            } else {
                ctx.biop(ops_a, ops_b, V2BinOp::Sub)
            }
        }
        TyKind::Float(FloatTy::F32 | FloatTy::F64) => ctx.biop(ops_a, ops_b, V2BinOp::Sub),
        TyKind::Float(FloatTy::F128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("__subtf3"),
                ctx.sig(
                    [Type::Float(Float::F128), Type::Float(Float::F128)],
                    Type::Float(Float::F128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        TyKind::Float(FloatTy::F16) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("sub_f16"),
                ctx.sig(
                    [Type::Float(Float::F16), Type::Float(Float::F16)],
                    Type::Float(Float::F16),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        _ => todo!("can't sub numbers of types {ty_a} and {ty_b}"),
    }
}

fn rem_unchecked<'tcx>(
    ty_a: Ty<'tcx>,
    _ty_b: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ops_a: Node,
    ops_b: Node,
) -> Node {
    match ty_a.kind() {
        TyKind::Int(IntTy::I128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("mod_i128"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        TyKind::Uint(UintTy::U128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("mod_u128"),
                ctx.sig(
                    [Type::Int(Int::U128), Type::Int(Int::U128)],
                    Type::Int(Int::U128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        TyKind::Int(_) | TyKind::Char | TyKind::Float(FloatTy::F32 | FloatTy::F64) => {
            ctx.biop(ops_a, ops_b, V2BinOp::Rem)
        }
        TyKind::Float(FloatTy::F128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("fmodl"),
                ctx.sig(
                    [Type::Float(Float::F128), Type::Float(Float::F128)],
                    Type::Float(Float::F128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        TyKind::Float(FloatTy::F16) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("mod_f16"),
                ctx.sig(
                    [Type::Float(Float::F16), Type::Float(Float::F16)],
                    Type::Float(Float::F16),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[ops_a, ops_b], IsPure::NOT)
        }
        TyKind::Uint(_) => ctx.biop(ops_a, ops_b, V2BinOp::RemUn),

        _ => todo!(),
    }
}

fn mul_unchecked<'tcx>(
    ty_a: Ty<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand_a: Node,
    operand_b: Node,
) -> Node {
    match ty_a.kind() {
        TyKind::Int(IntTy::I128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("mul_i128"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Uint(UintTy::U128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("mul_u128"),
                ctx.sig(
                    [Type::Int(Int::U128), Type::Int(Int::U128)],
                    Type::Int(Int::U128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Float(FloatTy::F128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("__multf3"),
                ctx.sig(
                    [Type::Float(Float::F128), Type::Float(Float::F128)],
                    Type::Float(Float::F128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Float(FloatTy::F16) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("mul_f16"),
                ctx.sig(
                    [Type::Float(Float::F16), Type::Float(Float::F16)],
                    Type::Float(Float::F16),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        _ => ctx.biop(operand_a, operand_b, V2BinOp::Mul),
    }
}
fn div_unchecked<'tcx>(
    ty_a: Ty<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand_a: Node,
    operand_b: Node,
) -> Node {
    match ty_a.kind() {
        TyKind::Int(IntTy::I128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("div_i128"),
                ctx.sig(
                    [Type::Int(Int::I128), Type::Int(Int::I128)],
                    Type::Int(Int::I128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Uint(UintTy::U128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("div_u128"),
                ctx.sig(
                    [Type::Int(Int::U128), Type::Int(Int::U128)],
                    Type::Int(Int::U128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Uint(_) => ctx.biop(operand_a, operand_b, V2BinOp::DivUn),
        TyKind::Int(_) | TyKind::Char | TyKind::Float(FloatTy::F32 | FloatTy::F64) => {
            ctx.biop(operand_a, operand_b, V2BinOp::Div)
        }
        TyKind::Float(FloatTy::F128) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("__divtf3"),
                ctx.sig(
                    [Type::Float(Float::F128), Type::Float(Float::F128)],
                    Type::Float(Float::F128),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        TyKind::Float(FloatTy::F16) => {
            let mref = MethodRef::new(
                *ctx.main_module(),
                ctx.alloc_string("div_f16"),
                ctx.sig(
                    [Type::Float(Float::F16), Type::Float(Float::F16)],
                    Type::Float(Float::F16),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            ctx.call(mref, &[operand_a, operand_b], IsPure::NOT)
        }
        _ => todo!(),
    }
}
