use super::PlaceTy;
use crate::pointer_to_is_fat;
use cilly::{
    Assembly, BinOp, Const, FieldDesc, Int, Interned, IntoAsmIndex, MethodRef, Type,
    cilnode::{ExtendKind, MethodKind},
};
use rustc_codegen_clr_ctx::MethodCompileCtx;
use rustc_codegen_clr_type::{
    GetTypeExt,
    adt::{FieldOffsetIterator, field_descrptor, variant_field_descriptor},
    r#type::{fat_ptr_to, get_type},
};
use rustc_middle::{
    mir::PlaceElem,
    ty::{Ty, TyKind},
};
pub fn local_address(
    local: usize,
    method: &rustc_middle::mir::Body,
    asm: &mut Assembly,
) -> Interned<cilly::ir::CILNode> {
    let local = if let Some(spread_arg) = method.spread_arg
        && local == spread_arg.as_usize()
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
        // Enums don't require any special handling
        PlaceTy::EnumVariant(_, _) => return addr_calc,
    };
    // Get the type curr_type points to!
    let curr_points_to = super::pointed_type(curr_type.into());
    let curr_type = ctx.type_from_cache(curr_type);
    let target_type = ctx.type_from_cache(target_ty);

    match (
        pointer_to_is_fat(curr_points_to, ctx.tcx(), ctx.instance()),
        pointer_to_is_fat(target_ty, ctx.tcx(), ctx.instance()),
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
        (false, true) => panic!("Invalid last dereference in address!"),
        (false, false) => addr_calc,
        (true, true) => ctx.load(addr_calc, curr_type),
    }
}
fn field_address<'a>(
    curr_type: super::PlaceTy<'a>,
    ctx: &mut MethodCompileCtx<'a, '_>,
    addr_calc: Interned<cilly::ir::CILNode>,
    field_index: u32,
    field_type: Ty<'a>,
) -> Interned<cilly::ir::CILNode> {
    match curr_type {
        super::PlaceTy::Ty(curr_type) => {
            let curr_type = ctx.monomorphize(curr_type);
            let field_ty = ctx.monomorphize(field_type);
            match (
                pointer_to_is_fat(curr_type, ctx.tcx(), ctx.instance()),
                pointer_to_is_fat(field_ty, ctx.tcx(), ctx.instance()),
            ) {
                (false, false) => {
                    let field_desc = field_descrptor(curr_type, field_index, ctx);
                    ctx.ld_field_addr(addr_calc, field_desc)
                }
                (false, true) => panic!(
                    "Sized type {curr_type:?} contains an unsized field of type {field_ty}. This is a bug."
                ),
                (true, false) => {
                    let mut explicit_offset_iter =
                        FieldOffsetIterator::fields(ctx.layout_of(curr_type).layout.0.0.clone());
                    let offset = explicit_offset_iter
                        .nth(field_index as usize)
                        .expect("Field index not in field offset iterator");
                    let curr_type_fat_ptr = ctx.type_from_cache(Ty::new_ptr(
                        ctx.tcx(),
                        curr_type,
                        rustc_middle::ty::Mutability::Mut,
                    ));
                    let data_ptr_name = ctx.alloc_string(cilly::DATA_PTR);
                    let void_ptr = ctx.nptr(Type::Void);
                    let addr_descr = ctx.alloc_field(FieldDesc::new(
                        curr_type_fat_ptr.as_class_ref().unwrap(),
                        data_ptr_name,
                        void_ptr,
                    ));
                    // Get the address of the unsized object.
                    let obj_addr = ctx.ld_field(addr_calc, addr_descr);
                    let obj = ctx.type_from_cache(field_type);
                    // Add the offset to the object.
                    let obj_addr = ctx.biop(obj_addr, Const::USize(u64::from(offset)), BinOp::Add);
                    // `cast_ptr(addr, pointee)` builds a `PtrCast` whose second arg is the POINTEE
                    // type, so pass the field value-type `obj` — NOT `nptr(obj)`, which would
                    // mislabel the result `Ptr(Ptr(field))`. That double-pointer mislabel is the
                    // `Weak<dyn T>::drop` `FieldAssignWrongType` (taking `&(*fat_ptr).sized_field`
                    // through a fat pointer). Pure pointer relabel, no runtime IL change.
                    ctx.cast_ptr(obj_addr, obj)
                }
                (true, true) => {
                    let mut explicit_offset_iter =
                        FieldOffsetIterator::fields(ctx.layout_of(curr_type).layout.0.0.clone());
                    let offset = explicit_offset_iter
                        .nth(field_index as usize)
                        .expect("Field index not in field offset iterator");
                    let curr_type_fat_ptr = ctx.type_from_cache(Ty::new_ptr(
                        ctx.tcx(),
                        curr_type,
                        rustc_middle::ty::Mutability::Mut,
                    ));
                    let data_ptr_name = ctx.alloc_string(cilly::DATA_PTR);
                    let metadata_name = ctx.alloc_string(cilly::METADATA);
                    let void_ptr = ctx.nptr(Type::Void);

                    let addr_descr = ctx.alloc_field(FieldDesc::new(
                        curr_type_fat_ptr.as_class_ref().unwrap(),
                        data_ptr_name,
                        void_ptr,
                    ));
                    // Get the address of the unsized object.
                    let obj_addr = ctx.ld_field(addr_calc, addr_descr);
                    let metadata_descr = ctx.alloc_field(FieldDesc::new(
                        curr_type_fat_ptr.as_class_ref().unwrap(),
                        metadata_name,
                        Type::Int(Int::USize),
                    ));
                    let metadata = ctx.ld_field(addr_calc, metadata_descr);
                    // The layout offset of an unsized tail is its MIN-alignment offset. For a `dyn`
                    // tail the real alignment is only known at runtime (vtable slot 2, after
                    // drop_in_place + size), so round the offset up to `align_of_val` before adding
                    // it. Otherwise an over-aligned payload (e.g. a `repr(align(32))` value behind
                    // `Arc<dyn T>`) is read at the wrong address — a silent miscompile, surfacing as
                    // the `Arc<dyn>::drop` AccessViolation in globset/regex. For a slice/str tail the
                    // alignment is static and already baked into `offset`, and `metadata` is a length
                    // (not a vtable), so it must be left unchanged there. Mirrors `align_of_val`.
                    let tail_is_dyn = matches!(
                        ctx.tcx()
                            .struct_tail_for_codegen(
                                field_ty,
                                rustc_middle::ty::TypingEnv::fully_monomorphized(),
                            )
                            .kind(),
                        TyKind::Dynamic(..)
                    );
                    let field_offset = if tail_is_dyn {
                        // align = *(vtable + 2 * size_of::<isize>())
                        let isize_sz = ctx.size_of(Int::ISize);
                        let two = ctx.alloc_node(2_i32);
                        let slot_off = ctx.biop(isize_sz, two, BinOp::Mul);
                        let slot_off = ctx.int_cast(slot_off, Int::USize, ExtendKind::ZeroExtend);
                        let align_addr = ctx.biop(metadata, slot_off, BinOp::Add);
                        let align_ptr = ctx.cast_ptr(align_addr, Type::Int(Int::USize));
                        let align = ctx.load(align_ptr, Type::Int(Int::USize));
                        // align_up(offset, align) = (offset + (align - 1)) & !(align - 1)
                        let one = ctx.alloc_node(Const::USize(1));
                        let am1 = ctx.biop(align, one, BinOp::Sub);
                        let off_const = ctx.alloc_node(Const::USize(u64::from(offset)));
                        let off_plus = ctx.biop(off_const, am1, BinOp::Add);
                        let all_ones = ctx.alloc_node(Const::USize(u64::MAX));
                        let mask = ctx.biop(am1, all_ones, BinOp::XOr);
                        ctx.biop(off_plus, mask, BinOp::And)
                    } else {
                        ctx.alloc_node(Const::USize(u64::from(offset)))
                    };
                    let ptr = ctx.biop(obj_addr, field_offset, BinOp::Add);
                    let field_fat_ptr = ctx.type_from_cache(Ty::new_ptr(
                        ctx.tcx(),
                        field_ty,
                        rustc_middle::ty::Mutability::Mut,
                    ));
                    ctx.create_slice(field_fat_ptr.as_class_ref().unwrap(), ptr, metadata)
                }
            }
        }
        super::PlaceTy::EnumVariant(enm, var_idx) => {
            let owner = ctx.monomorphize(enm);
            let field_desc = variant_field_descriptor(owner, field_index, var_idx, ctx);
            ctx.ld_field_addr(addr_calc, field_desc)
        }
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

    match place_elem {
        PlaceElem::Deref => address_last_dereference(place_ty, curr_type, ctx, addr_calc),
        PlaceElem::Field(field_index, field_ty) => {
            field_address(curr_type, ctx, addr_calc, field_index.as_u32(), *field_ty)
        }
        PlaceElem::Index(index) => {
            let curr_ty = curr_type
                .as_ty()
                .expect("INVALID PLACE: Indexing into enum variant???");
            let index = crate::local_get(index.as_usize(), ctx.body(), ctx);
            match curr_ty.kind() {
                TyKind::Slice(inner) => {
                    let inner = ctx.monomorphize(*inner);
                    let inner_type = ctx.type_from_cache(inner);
                    let slice = fat_ptr_to(inner, ctx);

                    let data_ptr_name = ctx.alloc_string(cilly::DATA_PTR);
                    let void_ptr = ctx.nptr(Type::Void);
                    let desc = ctx.alloc_field(FieldDesc::new(slice, data_ptr_name, void_ptr));
                    // This is a false positive
                    //    #[allow(unused_parens)]
                    let size = ctx.size_of(inner_type);
                    let size = size.into_idx(ctx);
                    let size = ctx.alloc_node(cilly::CILNode::IntCast {
                        input: size,
                        target: Int::USize,
                        extend: cilly::cilnode::ExtendKind::ZeroExtend,
                    });
                    let offset = ctx.biop(index, size, cilly::BinOp::Mul);
                    let data_ptr = ctx.ld_field(addr_calc, desc);
                    let data_ptr = ctx.cast_ptr(data_ptr, inner_type);
                    ctx.biop(data_ptr, offset, BinOp::Add)
                }
                TyKind::Array(element, _) => {
                    let mref = array_get_address(ctx, *element, curr_ty);
                    let mref = ctx.alloc_methodref(mref);
                    ctx.call(mref, &[addr_calc, index], cilly::cilnode::IsPure::NOT)
                }
                _ => {
                    todo!("Can't index into {curr_ty}!")
                }
            }
        }
        PlaceElem::Subslice { from, to, from_end } => {
            let base_ty = curr_type.as_ty().expect("Can't index into an enum!");
            let elem_ty = ctx.monomorphize(base_ty.sequence_element_type(ctx.tcx()));
            let elem_type = get_type(elem_ty, ctx);
            let curr_type = fat_ptr_to(base_ty, ctx);

            // ARRAY base (`[T; N]`): a Subslice of an array yields a SIZED sub-array `[T; to-from]`,
            // so its place address is a THIN pointer to that sub-array — the element-`from` address
            // inside the contiguous array. The slice-base arms below instead build a fat pointer
            // (`create_slice`) by reading the in-memory fat ptr's `d`/`m` fields, which an array value
            // does not have (it stores the elements inline, not a `FatPtr`). Without this the array
            // case both read a non-existent `FatPtr::d` (`FieldOwnerMismatch`) and produced a fat
            // slice where a `*[T; K]` is expected (`LocalAssigementWrong`). Surfaced by alloctests
            // `slice::subslice_patterns` (`sub @ ..` / `ref sub @ ..` sub-array bindings on arrays).
            if let TyKind::Array(_, array_len) = base_ty.kind() {
                let array_len = array_len
                    .try_to_target_usize(ctx.tcx())
                    .expect("Non-const array length in a Subslice projection");
                let sub_len = if *from_end {
                    array_len - (*to + *from)
                } else {
                    *to - *from
                };
                let sub_ty = Ty::new_array(ctx.tcx(), elem_ty, sub_len);
                let sub_ptr_ty = get_type(
                    Ty::new_ptr(ctx.tcx(), sub_ty, rustc_middle::ty::Mutability::Mut),
                    ctx,
                );
                let elem_ptr = ctx.cast_ptr(addr_calc, elem_type);
                let at_from = if *from != 0 {
                    let from_node = ctx.alloc_node(Const::USize(*from));
                    ctx.offset(elem_ptr, from_node, elem_type)
                } else {
                    elem_ptr
                };
                return ctx.cast_ptr_to(at_from, sub_ptr_ty);
            }

            if *from_end {
                //assert!(from >= to, "from_end:{from_end} from:{from} to:{to}");
                let metadata_name = ctx.alloc_string(cilly::METADATA);
                let metadata_field = ctx.alloc_field(FieldDesc::new(
                    curr_type,
                    metadata_name,
                    Type::Int(Int::USize),
                ));
                let data_ptr_name = ctx.alloc_string(cilly::DATA_PTR);
                let void_ptr = ctx.nptr(Type::Void);
                let ptr_field = ctx.alloc_field(FieldDesc::new(curr_type, data_ptr_name, void_ptr));
                // len = end - start
                // [from..slice.len() - to] -> (slice.len() - to) - from -> (slice.len() - (to + from)
                let meta_fld = ctx.ld_field(addr_calc, metadata_field);
                let metadata = ctx.biop(meta_fld, Const::USize(*to + from), BinOp::Sub);

                let data_ptr = if elem_type != Type::Void {
                    let base = ctx.ld_field(addr_calc, ptr_field);
                    let stride = ctx.size_of(elem_type).into_idx(ctx);
                    let stride = ctx.int_cast(stride, Int::USize, ExtendKind::ZeroExtend);
                    let scaled = ctx.biop(Const::USize(*from), stride, BinOp::Mul);
                    ctx.biop(base, scaled, BinOp::Add)
                } else {
                    ctx.ld_field(addr_calc, ptr_field)
                };
                ctx.create_slice(curr_type, data_ptr, metadata)
            } else {
                let void_ptr = ctx.nptr(Type::Void);
                let data_ptr = ctx.alloc_string(cilly::DATA_PTR);

                let ptr_field = ctx.alloc_field(FieldDesc::new(curr_type, data_ptr, void_ptr));
                let metadata = ctx.alloc_node(Const::USize(to - from));
                let base = ctx.ld_field(addr_calc, ptr_field);
                let stride = ctx.size_of(elem_type).into_idx(ctx);
                let stride = ctx.int_cast(stride, Int::USize, ExtendKind::ZeroExtend);
                let scaled = ctx.biop(Const::USize(*from), stride, BinOp::Mul);
                let data_ptr = ctx.biop(base, scaled, BinOp::Add);

                ctx.create_slice(curr_type, data_ptr, metadata)
            }
        }
        PlaceElem::ConstantIndex {
            offset,
            min_length,
            from_end,
        } => {
            let curr_ty = curr_type
                .as_ty()
                .expect("INVALID PLACE: Indexing into enum variant???");
            let _ = min_length;
            //assert!(!from_end, "Indexing slice form end");
            //println!("WARNING: ConstantIndex has required min_length of {min_length}, but bounds checking on const access not supported yet!");
            match curr_ty.kind() {
                TyKind::Slice(inner) => {
                    let inner = ctx.monomorphize(*inner);

                    let inner_type = ctx.type_from_cache(inner);
                    let slice = fat_ptr_to(Ty::new_slice(ctx.tcx(), inner), ctx);
                    let void_ptr = ctx.nptr(Type::Void);
                    let data_ptr = ctx.alloc_string(cilly::DATA_PTR);
                    let desc = ctx.alloc_field(FieldDesc::new(slice, data_ptr, void_ptr));
                    let metadata = ctx.alloc_string(cilly::METADATA);
                    let len =
                        ctx.alloc_field(FieldDesc::new(slice, metadata, Type::Int(Int::USize)));
                    let index = if *from_end {
                        //eprintln!("Slice index from end is:{offset}");
                        let len_fld = ctx.ld_field(addr_calc, len);
                        ctx.biop(len_fld, Const::USize(*offset), BinOp::Sub)
                    } else {
                        ctx.alloc_node(Const::USize(*offset))
                        //ops.extend(derf_op);
                    };

                    let base = ctx.ld_field(addr_calc, desc);
                    let base = ctx.cast_ptr(base, inner_type);
                    let stride = ctx.size_of(inner_type).into_idx(ctx);
                    let stride = ctx.int_cast(stride, Int::USize, ExtendKind::ZeroExtend);
                    let scaled = ctx.biop(index, stride, BinOp::Mul);
                    ctx.biop(base, scaled, BinOp::Add)
                }
                TyKind::Array(element, _) => {
                    let mref = array_get_address(ctx, *element, curr_ty);
                    if *from_end {
                        todo!("Can't index array from end!");
                    } else {
                        let mref = ctx.alloc_methodref(mref);
                        let offset = ctx.alloc_node(Const::USize(*offset));
                        ctx.call(mref, &[addr_calc, offset], cilly::cilnode::IsPure::NOT)
                    }
                }
                _ => {
                    rustc_middle::ty::print::with_no_trimmed_paths! { todo!("Can't index into {curr_ty}!")}
                }
            }
        }
        _ => {
            rustc_middle::ty::print::with_no_trimmed_paths! {todo!("Can't handle porojection {place_elem:?} in address")}
        }
    }
}
pub fn array_get_address<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    element: Ty<'tcx>,
    curr_ty: Ty<'tcx>,
) -> MethodRef {
    let element = ctx.monomorphize(element);
    let element = ctx.type_from_cache(element);
    let array_type = ctx.type_from_cache(curr_ty);
    let array_dotnet = array_type.as_class_ref().expect("Non array type");
    let arr_ref = ctx.nref(array_type);
    let element_ptr = ctx.nptr(element);
    MethodRef::new(
        array_dotnet,
        ctx.alloc_string("get_Address"),
        ctx.sig([arr_ref, Type::Int(Int::USize)], element_ptr),
        MethodKind::Instance,
        vec![].into(),
    )
}
