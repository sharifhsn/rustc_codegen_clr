use crate::fn_ctx::MethodCompileCtx;
use crate::r#type::GetTypeExt;
use cilly::{Assembly, Interned, Type};
use rustc_middle::mir::{Place, PlaceElem};

pub(super) fn local_get(
    local: usize,
    method: &rustc_middle::mir::Body,
    asm: &mut Assembly,
) -> Interned<cilly::ir::CILNode> {
    asm.alloc_node(
        if method
            .spread_arg
            .is_some_and(|spread_arg| local == spread_arg.as_usize())
        {
            cilly::CILNode::LdLoc(
                (method.local_decls.len() - method.arg_count)
                    .try_into()
                    .unwrap(),
            )
        } else if local == 0 {
            cilly::CILNode::LdLoc(0)
        } else if local > method.arg_count {
            cilly::CILNode::LdLoc(
                u32::try_from(local - method.arg_count)
                    .expect("method has more than 2^32 local variables"),
            )
        } else {
            cilly::CILNode::LdArg(
                u32::try_from(local - 1).expect("method has more than 2^32 local variables"),
            )
        },
    )
}

/// Returns the CIL value of a MIR place.
pub fn place_get<'tcx>(
    place: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<cilly::ir::CILNode> {
    let lowered = super::projection::LoweredPlace::new(place, ctx);
    let Some(projection) = lowered.projection else {
        return local_get(lowered.local, ctx.body(), ctx);
    };
    place_elem_get(
        &projection.elem,
        projection.owner_ty,
        lowered.result_ty,
        ctx,
        projection.base,
    )
}

fn place_elem_get<'tcx>(
    place_elem: &PlaceElem<'tcx>,
    curr_type: super::PlaceTy<'tcx>,
    result_ty: rustc_middle::ty::Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    base: Interned<cilly::ir::CILNode>,
) -> Interned<cilly::ir::CILNode> {
    let curr_type = curr_type.monomorphize(ctx);
    if let Some(field) = super::projection::FieldProjection::lower(place_elem, curr_type, base, ctx)
    {
        return field.get(base, ctx);
    }
    if let Some(sequence) =
        super::projection::SequenceProjection::lower(place_elem, curr_type, base, ctx)
    {
        let lowered = ctx.type_from_cache(sequence.element_ty);
        return if lowered == Type::Void {
            ctx.uninit_val(Type::Void)
        } else {
            super::deref_op(sequence.element_ty.into(), ctx, sequence.address)
        };
    }
    if let Some(subslice) =
        super::projection::SubsliceProjection::lower(place_elem, curr_type, base, ctx)
    {
        let lowered = ctx.type_from_cache(subslice.result_ty);
        if lowered == Type::Void {
            return ctx.uninit_val(Type::Void);
        }
        assert!(
            !super::body_ty_is_by_address(subslice.result_ty, ctx)
                || matches!(
                    subslice.result_ty.kind(),
                    rustc_middle::ty::TyKind::Array(..)
                ),
            "cannot read an unsized subslice by value"
        );
        return ctx.load(subslice.address, lowered);
    }

    match place_elem {
        PlaceElem::Deref => super::deref_op(super::pointed_type(curr_type).into(), ctx, base),
        PlaceElem::Downcast(..) | PlaceElem::OpaqueCast(..) | PlaceElem::UnwrapUnsafeBinder(..) => {
            let lowered = ctx.type_from_cache(result_ty);
            if lowered == Type::Void {
                ctx.uninit_val(Type::Void)
            } else if super::body_ty_is_by_address(result_ty, ctx) {
                ctx.load(base, lowered)
            } else {
                base
            }
        }
        _ => rustc_middle::ty::print::with_no_trimmed_paths! {
            todo!("cannot read through projection {place_elem:?}")
        },
    }
}
