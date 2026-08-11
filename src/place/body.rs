use super::{PlaceTy, pointed_type};
use crate::fn_ctx::MethodCompileCtx;
use crate::place::{body_ty_is_by_address, deref_op};
use crate::r#type::GetTypeExt;
use cilly::{Const, Interned};
use rustc_middle::mir::{Local, PlaceElem};

/// Seeds a projection walk with the local's value or address according to its MIR representation.
pub fn local_body<'tcx>(
    local: usize,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> (Interned<cilly::ir::CILNode>, rustc_middle::ty::Ty<'tcx>) {
    let ty = ctx.monomorphize(ctx.body().local_decls[Local::from_usize(local)].ty);
    let layout = ctx.layout_of(ty);
    if layout.is_zst() {
        // Storage-less locals use the correctly aligned dangling base. Shared field/sequence plans
        // preserve or layout-adjust this base for every following projected ZST.
        let lowered_ty = ctx.type_from_cache(ty);
        let base = ctx.alloc_node(Const::USize(layout.align.abi.bytes()));
        return (ctx.cast_ptr(base, lowered_ty), ty);
    }
    if body_ty_is_by_address(ty, ctx) {
        (super::address::local_address(local, ctx.body(), ctx), ty)
    } else {
        (super::get::local_get(local, ctx.body(), ctx), ty)
    }
}

/// Lowers one non-final projection while retaining the representation required by the next step.
pub fn place_elem_body<'tcx>(
    place_elem: &PlaceElem<'tcx>,
    curr_type: PlaceTy<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    node: Interned<cilly::ir::CILNode>,
) -> (PlaceTy<'tcx>, Interned<cilly::ir::CILNode>) {
    let curr_type = curr_type.monomorphize(ctx);
    if let Some(field) = super::projection::FieldProjection::lower(place_elem, curr_type, node, ctx)
    {
        return field.body(node, ctx);
    }
    if let Some(sequence) =
        super::projection::SequenceProjection::lower(place_elem, curr_type, node, ctx)
    {
        return sequence.body(ctx);
    }
    if let Some(subslice) =
        super::projection::SubsliceProjection::lower(place_elem, curr_type, node, ctx)
    {
        return (subslice.result_ty.into(), subslice.address);
    }

    match place_elem {
        PlaceElem::Deref => {
            let pointed = pointed_type(curr_type);
            if body_ty_is_by_address(pointed, ctx) {
                (pointed.into(), node)
            } else {
                (pointed.into(), deref_op(pointed.into(), ctx, node))
            }
        }
        PlaceElem::Downcast(_, variant) => {
            let owner = curr_type
                .as_ty()
                .expect("cannot downcast an enum-variant marker twice");
            (PlaceTy::EnumVariant(owner, variant.as_u32()), node)
        }
        PlaceElem::OpaqueCast(ty) | PlaceElem::UnwrapUnsafeBinder(ty) => {
            (ctx.monomorphize(*ty).into(), node)
        }
        _ => rustc_middle::ty::print::with_no_trimmed_paths! {
            todo!("cannot lower intermediate projection {place_elem:?}")
        },
    }
}
