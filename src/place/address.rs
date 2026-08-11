use super::PlaceTy;
use crate::fn_ctx::MethodCompileCtx;
use crate::place::ptr_is_fat;
use crate::r#type::GetTypeExt;
use cilly::{Assembly, FieldDesc, Interned, MethodRef, Type, cilnode::MethodKind};
use rustc_middle::{mir::PlaceElem, ty::Ty};

pub fn local_address(
    local: usize,
    method: &rustc_middle::mir::Body,
    asm: &mut Assembly,
) -> Interned<cilly::ir::CILNode> {
    let local = if method
        .spread_arg
        .is_some_and(|spread_arg| local == spread_arg.as_usize())
    {
        cilly::CILNode::LdLocA(
            (method.local_decls.len() - method.arg_count)
                .try_into()
                .unwrap(),
        )
    } else if local == 0 {
        cilly::CILNode::LdLocA(0)
    } else if local > method.arg_count {
        cilly::CILNode::LdLocA(u32::try_from(local - method.arg_count).unwrap())
    } else {
        cilly::CILNode::LdArgA(u32::try_from(local - 1).unwrap())
    };
    let local = asm.alloc_node(local);
    asm.alloc_node(cilly::CILNode::RefToPtr(local))
}

pub fn address_last_dereference<'tcx>(
    target_ty: Ty<'tcx>,
    curr_type: PlaceTy<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    addr_calc: Interned<cilly::ir::CILNode>,
) -> Interned<cilly::ir::CILNode> {
    let curr_type = match curr_type {
        PlaceTy::Ty(curr_type) => curr_type,
        PlaceTy::EnumVariant(_, _) => return addr_calc,
    };
    let curr_points_to = super::pointed_type(curr_type.into());
    let curr_type = ctx.type_from_cache(curr_type);
    let target_type = ctx.type_from_cache(target_ty);

    match (
        ptr_is_fat(curr_points_to, ctx.tcx(), ctx.instance()),
        ptr_is_fat(target_ty, ctx.tcx(), ctx.instance()),
    ) {
        (true, false) => {
            let data_ptr_name = ctx.alloc_string(cilly::DATA_PTR);
            let void_ptr = ctx.nptr(Type::Void);
            let field = ctx.alloc_field(FieldDesc::new(
                curr_type.as_class_ref().unwrap(),
                data_ptr_name,
                void_ptr,
            ));
            let data_ptr = ctx.ld_field(addr_calc, field);
            let loaded_ptr = ctx.nptr(target_type);
            ctx.load(data_ptr, loaded_ptr)
        }
        (false, true) => panic!("invalid final dereference in place address"),
        (false, false) => addr_calc,
        (true, true) => ctx.load(addr_calc, curr_type),
    }
}

pub fn place_elem_address<'tcx>(
    place_elem: &PlaceElem<'tcx>,
    curr_type: PlaceTy<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    place_ty: Ty<'tcx>,
    addr_calc: Interned<cilly::ir::CILNode>,
) -> Interned<cilly::ir::CILNode> {
    let curr_type = curr_type.monomorphize(ctx);
    if let Some(field) =
        super::projection::FieldProjection::lower(place_elem, curr_type, addr_calc, ctx)
    {
        return field.address(addr_calc, ctx);
    }
    if let Some(sequence) =
        super::projection::SequenceProjection::lower(place_elem, curr_type, addr_calc, ctx)
    {
        return sequence.address;
    }
    if let Some(subslice) =
        super::projection::SubsliceProjection::lower(place_elem, curr_type, addr_calc, ctx)
    {
        return subslice.address;
    }

    match place_elem {
        PlaceElem::Deref => address_last_dereference(place_ty, curr_type, ctx, addr_calc),
        PlaceElem::Downcast(..) | PlaceElem::OpaqueCast(..) | PlaceElem::UnwrapUnsafeBinder(..) => {
            addr_calc
        }
        _ => rustc_middle::ty::print::with_no_trimmed_paths! {
            todo!("cannot take address through projection {place_elem:?}")
        },
    }
}

pub(super) fn array_element_address<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    element: Ty<'tcx>,
    curr_ty: Ty<'tcx>,
    array_address: Interned<cilly::ir::CILNode>,
    index: Interned<cilly::ir::CILNode>,
) -> Interned<cilly::ir::CILNode> {
    let element = ctx.monomorphize(element);
    let element = ctx.type_from_cache(element);
    let array_type = ctx.type_from_cache(curr_ty);

    // A ZST array has no CLR class, but its semantic element address is still the container-derived
    // base. The shared sequence plan has already applied the Rust zero stride.
    if array_type == Type::Void {
        return ctx.cast_ptr(array_address, element);
    }

    let array_dotnet = array_type.as_class_ref().expect("non-array type");
    let arr_ref = ctx.nref(array_type);
    let element_ptr = ctx.nptr(element);
    let mref = MethodRef::new(
        array_dotnet,
        ctx.alloc_string("get_Address"),
        ctx.sig([arr_ref, Type::Int(cilly::Int::USize)], element_ptr),
        MethodKind::Instance,
        vec![].into(),
    );
    let mref = ctx.alloc_methodref(mref);
    ctx.call(mref, &[array_address, index], cilly::cilnode::IsPure::NOT)
}
