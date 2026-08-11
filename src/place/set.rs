use crate::fn_ctx::MethodCompileCtx;
use crate::place::{PlaceTy, pointed_type};
use crate::r#type::{GetTypeExt, utilis::ptr_is_fat};
use cilly::{Assembly, ClassRef, Int, Interned, Type};
use rustc_middle::{
    mir::PlaceElem,
    ty::{FloatTy, IntTy, TyKind, UintTy},
};

pub fn local_set(
    local: usize,
    method: &rustc_middle::mir::Body,
    tree: Interned<cilly::ir::CILNode>,
    asm: &mut Assembly,
) -> Interned<cilly::ir::CILRoot> {
    if method
        .spread_arg
        .is_some_and(|spread_arg| local == spread_arg.as_usize())
    {
        return asm.st_loc(
            (method.local_decls.len() - method.arg_count)
                .try_into()
                .unwrap(),
            tree,
        );
    }
    if local == 0 {
        asm.st_loc(0, tree)
    } else if local > method.arg_count {
        asm.st_loc(u32::try_from(local - method.arg_count).unwrap(), tree)
    } else {
        asm.st_arg(u32::try_from(local - 1).unwrap(), tree)
    }
}

pub fn place_elem_set<'tcx>(
    place_elem: &PlaceElem<'tcx>,
    curr_type: PlaceTy<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    base: Interned<cilly::ir::CILNode>,
    value: Interned<cilly::ir::CILNode>,
) -> Interned<cilly::ir::CILRoot> {
    let curr_type = curr_type.monomorphize(ctx);
    if let Some(field) = super::projection::FieldProjection::lower(place_elem, curr_type, base, ctx)
    {
        return field.set(base, value, ctx);
    }
    if let Some(sequence) =
        super::projection::SequenceProjection::lower(place_elem, curr_type, base, ctx)
    {
        return ptr_set_op(sequence.element_ty.into(), ctx, sequence.address, value);
    }
    if super::projection::SubsliceProjection::lower(place_elem, curr_type, base, ctx).is_some() {
        panic!("cannot assign to a subslice by value");
    }

    match place_elem {
        PlaceElem::Deref => ptr_set_op(pointed_type(curr_type).into(), ctx, base, value),
        PlaceElem::Downcast(..) | PlaceElem::OpaqueCast(..) | PlaceElem::UnwrapUnsafeBinder(..) => {
            let ty = curr_type
                .as_ty()
                .expect("cannot assign an enum-variant marker without a field");
            ptr_set_op(ty.into(), ctx, base, value)
        }
        _ => rustc_middle::ty::print::with_no_trimmed_paths! {
            todo!("cannot assign through projection {place_elem:?}")
        },
    }
}

/// Stores a value through a pointer to a Rust place type.
pub fn ptr_set_op<'tcx>(
    pointed_type: PlaceTy<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    addr_calc: Interned<cilly::ir::CILNode>,
    value_calc: Interned<cilly::ir::CILNode>,
) -> Interned<cilly::ir::CILRoot> {
    if let PlaceTy::Ty(pointed_type) = pointed_type {
        match pointed_type.kind() {
            TyKind::Int(int_ty) => match int_ty {
                IntTy::I8 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I8), false),
                IntTy::I16 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I16), false),
                IntTy::I32 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I32), false),
                IntTy::I64 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I64), false),
                IntTy::Isize => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::ISize), false),
                IntTy::I128 => {
                    let tpe = ClassRef::int_128(ctx).into();
                    ctx.st_ind(addr_calc, value_calc, tpe, false)
                }
            },
            TyKind::Uint(int_ty) => match int_ty {
                UintTy::U8 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I8), false),
                UintTy::U16 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I16), false),
                UintTy::U32 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I32), false),
                UintTy::U64 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I64), false),
                UintTy::Usize => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::ISize), false),
                UintTy::U128 => {
                    let tpe = ClassRef::uint_128(ctx).into();
                    ctx.st_ind(addr_calc, value_calc, tpe, false)
                }
            },
            TyKind::Float(float_ty) => match float_ty {
                FloatTy::F32 => {
                    ctx.st_ind(addr_calc, value_calc, Type::Float(cilly::Float::F32), false)
                }
                FloatTy::F64 => {
                    ctx.st_ind(addr_calc, value_calc, Type::Float(cilly::Float::F64), false)
                }
                FloatTy::F128 => ctx.st_ind(
                    addr_calc,
                    value_calc,
                    Type::Float(cilly::Float::F128),
                    false,
                ),
                FloatTy::F16 => {
                    ctx.st_ind(addr_calc, value_calc, Type::Float(cilly::Float::F16), false)
                }
            },
            TyKind::Bool => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I8), false),
            TyKind::Char => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I32), false),
            TyKind::Adt(_, _)
            | TyKind::Tuple(_)
            | TyKind::Array(_, _)
            | TyKind::Closure(_, _)
            | TyKind::Coroutine(_, _) => {
                let pointed_type = ctx.type_from_cache(pointed_type);
                ctx.st_ind(addr_calc, value_calc, pointed_type, false)
            }
            TyKind::Ref(_, inner, _) => {
                if ptr_is_fat(*inner, ctx.tcx(), ctx.instance()) {
                    let tpe = ctx.type_from_cache(pointed_type);
                    ctx.st_ind(addr_calc, value_calc, tpe, false)
                } else {
                    let inner = ctx.type_from_cache(*inner);
                    let ptr = ctx.nptr(inner);
                    ctx.st_ind(addr_calc, value_calc, ptr, false)
                }
            }
            TyKind::RawPtr(ty, _) => {
                if ptr_is_fat(*ty, ctx.tcx(), ctx.instance()) {
                    let tpe = ctx.type_from_cache(pointed_type);
                    ctx.st_ind(addr_calc, value_calc, tpe, false)
                } else {
                    let inner = ctx.type_from_cache(*ty);
                    let ptr = ctx.nptr(inner);
                    ctx.st_ind(addr_calc, value_calc, ptr, false)
                }
            }
            TyKind::FnPtr(..) => {
                let pointed_type = ctx.type_from_cache(pointed_type);
                ctx.st_ind(addr_calc, value_calc, pointed_type, false)
            }
            _ => todo!("cannot store through pointer to {pointed_type:?}"),
        }
    } else {
        todo!("cannot store through a pointer to an enum-variant marker");
    }
}
