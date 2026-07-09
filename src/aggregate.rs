use crate::{
    assembly::MethodCompileCtx,
    utilis::{adt::set_discr, field_name, instance_try_resolve, variant_name},
};
use cilly::{
    cilnode::{IsPure, MethodKind},
    ClassRef, Const, FieldDesc, FnSig, Int, Interned, MethodRef, Type,
};
use rustc_abi::FieldIdx;
use rustc_codegen_clr_place::{place_address, place_get, place_set};
use rustc_codegen_clr_type::{
    adt::{enum_tag_info, field_descrptor},
    r#type::{escape_field_name, get_type},
    utilis::{is_zst, ptr_is_fat, simple_tuple},
    GetTypeExt,
};
use rustc_codgen_clr_operand::{handle_operand, is_uninit};
use rustc_index::IndexVec;
use rustc_middle::{
    mir::{AggregateKind, Operand, Place},
    ty::{AdtDef, AdtKind, GenericArg, List, Ty, TyKind},
};

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

/// Returns the CIL ops to create the aggreagate value specifed by `aggregate_kind` at `dst_place`. Uses indivlidual values specifed by `value_index`
pub fn handle_aggregate<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    dst_place: &Place<'tcx>,
    aggregate_kind: &AggregateKind<'tcx>,
    value_index: &IndexVec<FieldIdx, Operand<'tcx>>,
) -> (Vec<Root>, Node) {
    // Get CIL ops for each value
    let values: Vec<_> = value_index
        .iter()
        .enumerate()
        .map(|operand| {
            (
                u32::try_from(operand.0).unwrap(),
                handle_operand(operand.1, ctx),
            )
        })
        .collect();
    match aggregate_kind {
        AggregateKind::Adt(adt_def, variant_idx, subst, _utai, active_field) => {
            let penv = rustc_middle::ty::TypingEnv::fully_monomorphized();
            let subst = ctx.monomorphize(*subst);
            //eprintln!("Preparing to resolve {adt_def:?} {subst:?}");
            let adt_type = instance_try_resolve(*adt_def, ctx.tcx(), subst);
            let adt_type = adt_type.ty(ctx.tcx(), penv);
            let adt_type = ctx.monomorphize(adt_type);
            let TyKind::Adt(adt_def, subst) = adt_type.kind() else {
                panic!("Type {adt_type:?} is not a Algebraic Data Type!");
            };
            aggregate_adt(
                ctx,
                dst_place,
                *adt_def,
                adt_type,
                subst,
                variant_idx.as_u32(),
                values,
                *active_field,
            )
        }
        AggregateKind::Array(element) => {
            // Check if this array is made up from uninit values
            if is_uninit(&value_index[FieldIdx::from_usize(0)], ctx) {
                // This array is created from uninitalized data, so it itsefl is uninitialzed, so we can skip initializing it.
                return (vec![], place_get(dst_place, ctx));
            }
            let element = ctx.monomorphize(*element);
            let element = ctx.type_from_cache(element);
            let array_type = ClassRef::fixed_array(element, value_index.len() as u64, ctx);
            let array_getter = place_address(dst_place, ctx);
            let sig = FnSig::new(
                [ctx.nref(array_type), Type::Int(Int::USize), element],
                Type::Void,
            );
            let site = MethodRef::new(
                array_type,
                ctx.alloc_string("set_Item"),
                ctx.alloc_sig(sig),
                MethodKind::Instance,
                vec![].into(),
            );
            let mut sub_trees = Vec::new();
            for value in values {
                let site = ctx.alloc_methodref(site.clone());
                let idx = ctx.alloc_node(Const::USize(u64::from(value.0)));
                let root = ctx.call_root(site, &[array_getter, idx, value.1], IsPure::NOT);
                sub_trees.push(root);
            }
            (sub_trees, (place_get(dst_place, ctx)))
        }
        AggregateKind::Tuple => {
            let tuple_getter = place_address(dst_place, ctx);
            let types: Vec<_> = value_index
                .iter()
                .map(|operand| {
                    let operand_ty = ctx.monomorphize(operand.ty(ctx.body(), ctx.tcx()));
                    get_type(operand_ty, ctx)
                })
                .collect();
            let dotnet_tpe = simple_tuple(&types, ctx);
            let mut sub_trees = Vec::new();
            for field in &values {
                // Assigining to a Void field is a NOP and must be skipped(since it can have wierd side-effects).
                if types[field.0 as usize] == cilly::Type::Void {
                    continue;
                }
                let name = format!("Item{}", field.0 + 1);

                let field_name = ctx.alloc_string(name);
                let desc = ctx.alloc_field(FieldDesc::new(
                    dotnet_tpe,
                    field_name,
                    types[field.0 as usize],
                ));
                let root = ctx.set_field(desc, tuple_getter, field.1);
                sub_trees.push(root);
            }
            (sub_trees, (place_get(dst_place, ctx)))
        }
        AggregateKind::Closure(_def_id, _args) => {
            let closure_ty = ctx
                .monomorphize(dst_place.ty(ctx.body(), ctx.tcx()))
                .ty;
            let closure_type = get_type(closure_ty, ctx);
            let closure_dotnet = closure_type.as_class_ref().expect("Invalid closure type!");
            let closure_getter = place_address(dst_place, ctx);
            let mut sub_trees = vec![];
            for (index, value) in value_index.iter_enumerated() {
                let field_ty = ctx.monomorphize(value.ty(ctx.body(), ctx.tcx()));
                let field_type = get_type(field_ty, ctx);
                if field_type == cilly::Type::Void {
                    continue;
                }
                let field_name = ctx.alloc_string(format!("f_{}", index.as_u32()));
                let value = handle_operand(value, ctx);
                let desc = ctx.alloc_field(FieldDesc::new(closure_dotnet, field_name, field_type));
                let root = ctx.set_field(desc, closure_getter, value);
                sub_trees.push(root);
            }

            (sub_trees, (place_get(dst_place, ctx)))
        }
        AggregateKind::Coroutine(_def_id, _args) => {
            let coroutine_ty = ctx
                .monomorphize(dst_place.ty(ctx.body(), ctx.tcx()))
                .ty;
            let coroutine_type = get_type(coroutine_ty, ctx);
            let closure_dotnet = coroutine_type
                .as_class_ref()
                .expect("Invalid closure type!");
            let closure_getter = place_address(dst_place, ctx);
            let mut sub_trees = vec![];
            for (index, value) in value_index.iter_enumerated() {
                let field_ty = ctx.monomorphize(value.ty(ctx.body(), ctx.tcx()));
                let field_type = get_type(field_ty, ctx);
                if field_type == cilly::Type::Void {
                    continue;
                }
                let field_name = ctx.alloc_string(format!("f_{}", index.as_u32()));
                let value = handle_operand(value, ctx);
                let desc = ctx.alloc_field(FieldDesc::new(closure_dotnet, field_name, field_type));
                let root = ctx.set_field(desc, closure_getter, value);
                sub_trees.push(root);
            }
            let layout = ctx.layout_of(coroutine_ty);
            let (disrc_type, _) = enum_tag_info(layout.layout, ctx);
            if disrc_type != Type::Void {
                sub_trees.push(set_discr(
                    layout.layout,
                    rustc_abi::VariantIdx::from_u32(0), // TODO: this assumes all coroutines start with a tag of 0
                    closure_getter,
                    closure_dotnet,
                    layout.ty,
                    ctx,
                ));
            }
            (sub_trees, (place_get(dst_place, ctx)))
        }
        AggregateKind::RawPtr(pointee, mutability) => {
            let pointee = ctx.monomorphize(*pointee);
            let [data, meta] = &*value_index.raw else {
                panic!("RawPtr fields: {value_index:?}");
            };
            let fat_ptr = Ty::new_ptr(ctx.tcx(), pointee, *mutability);
            // Get the addres of the initialized structure
            let init_addr = place_address(dst_place, ctx);
            let meta_ty = ctx.monomorphize(meta.ty(ctx.body(), ctx.tcx()));
            let data_ty = ctx.monomorphize(data.ty(ctx.body(), ctx.tcx()));
            let fat_ptr_type = ctx.type_from_cache(fat_ptr);
            if !ptr_is_fat(pointee, ctx.tcx(), ctx.instance()) {
                // Double-check the pointer is REALLY thin
                assert!(fat_ptr_type.as_class_ref().is_none());
                assert!(
                    !is_zst(data_ty, ctx.tcx()),
                    "data_ty:{data_ty:?} is a zst. That is bizzare, cause it should be a pointer?"
                );
                let data_type = ctx.type_from_cache(data_ty);
                let ptr_tpe = ctx.type_from_cache(pointee);
                assert_ne!(data_type, Type::Void);
                // Pointer is thin, just directly assign
                let data = handle_operand(data, ctx);
                let ptr = ctx.nptr(ptr_tpe);
                // `ptr` is the FULL thin-pointer type (`*pointee`). `cast_ptr_to` produces a value of
                // exactly that type; `cast_ptr` would instead WRAP `ptr` in another `Ptr(..)` (its
                // second arg is the *pointee*), yielding `**pointee`. The bits are the same single
                // data pointer, so it runs — but the value's *type* is one indirection too deep, and
                // that ill-typed value, passed on (e.g. `ThinBox`'s `from_raw_parts_mut` result fed
                // to `WithHeader::drop(value: *mut T)`), is a `CallArgTypeWrong`. This mirrors the
                // already-correct fat-ptr DATA_PTR arm below. Surfaced by alloctests `thin_box`.
                let data = ctx.cast_ptr_to(data, ptr);
                return (
                    [place_set(dst_place, data, ctx)].into(),
                    (place_get(dst_place, ctx)),
                );
            }
            assert!(ptr_is_fat(pointee,ctx.tcx(), ctx.instance()), "A pointer to {pointee:?} is not fat, but its metadata is {meta_ty:?}, and not a zst:{is_meta_zst}",is_meta_zst = is_zst(meta_ty,  ctx.tcx()));
            let fat_ptr_type = get_type(fat_ptr, ctx);
            // Assign the components
            let data_ptr_name = ctx.alloc_string(crate::DATA_PTR);
            let void_ptr = ctx.nptr(cilly::Type::Void);
            let data_val = values[0].1;
            // The DATA_PTR field is typed `*void`. `cast_ptr_to` produces a value of exactly the
            // given pointer type — `cast_ptr` would instead WRAP its argument in another `Ptr(..)`
            // (its second arg is the pointee), yielding `**void` and a `FieldAssignWrongType`
            // miscompile that the .NET JIT rejects as "Bad IL format" at scale.
            let data_val = ctx.cast_ptr_to(data_val, void_ptr);
            let data_desc = ctx.alloc_field(FieldDesc::new(
                fat_ptr_type.as_class_ref().unwrap(),
                data_ptr_name,
                void_ptr,
            ));
            let assign_ptr = ctx.set_field(data_desc, init_addr, data_val);
            let name = ctx.alloc_string(crate::METADATA);
            let meta_type = get_type(meta.ty(ctx.body(), ctx.tcx()), ctx);
            let meta_val = handle_operand(meta, ctx);
            let meta_val =
                ctx.transmute_on_stack(meta_type, cilly::Type::Int(Int::USize), meta_val);
            let meta_desc = ctx.alloc_field(FieldDesc::new(
                fat_ptr_type.as_class_ref().unwrap(),
                name,
                cilly::Type::Int(Int::USize),
            ));
            let assign_metadata = ctx.set_field(meta_desc, init_addr, meta_val);

            (
                [assign_ptr, assign_metadata].into(),
                (place_get(dst_place, ctx)),
            )
        }
        AggregateKind::CoroutineClosure(..) => {
            todo!("Unsuported aggregate kind {aggregate_kind:?}")
        }
    }
}
/// Builds an Algebraic Data Type (struct,enum,union) at location `dst_place`, with fields set using ops in `fields`.
fn aggregate_adt<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    dst_place: &Place<'tcx>,
    adt: AdtDef<'tcx>,
    adt_type: Ty<'tcx>,
    subst: &'tcx List<GenericArg<'tcx>>,
    variant_idx: u32,
    fields: Vec<(u32, Node)>,
    active_field: Option<FieldIdx>,
) -> (Vec<Root>, Node) {
    let adt_type = ctx.monomorphize(adt_type);
    let adt_cil = get_type(adt_type, ctx);
    // `repr(simd)` ADTs lower to `Type::SIMDVector`, which is not a ClassRef and so can't be
    // built field-by-field like a normal struct. Construct the vector through memory instead:
    // take the address of the destination place, then store each provided lane via a
    // reinterpreted element pointer (the same spill-and-index idiom used by `simd_insert`).
    //
    // This is only valid when the aggregate's fields ARE the scalar lanes, i.e. one field per
    // lane (the legacy `struct f32x4(f32,f32,f32,f32)` shape). The modern stdlib shape is a
    // single inner-array field `struct Simd<T,N>([T;N])`; storing that `[T;N]` value through a
    // `*elem` slot would write only `sizeof(elem)` bytes and miscompile. Guard on the lane
    // count so the mismatched single-field case falls through to the (loud) class-ref path
    // rather than silently producing a wrong vector.
    if let Type::SIMDVector(simd) = adt_cil {
        if fields.len() as u64 == u64::from(simd.count()) {
            let elem: Type = simd.elem().into();
            let addr = place_address(dst_place, ctx);
            let elem_ptr = ctx.cast_ptr(addr, elem);
            let mut roots = Vec::new();
            for (lane, value) in fields {
                let idx = ctx.alloc_node(Const::USize(u64::from(lane)));
                let slot = ctx.offset(elem_ptr, idx, elem);
                roots.push(ctx.alloc_root(cilly::ir::CILRoot::StInd(Box::new((
                    slot, value, elem, false,
                )))));
            }
            return (roots, place_get(dst_place, ctx));
        }
        // The modern stdlib shape `struct Simd<T,N>([T;N])` has a single inner-array field
        // whose value IS the whole vector (the array `[T;N]` and the `SIMDVector` are
        // bit-identical). Build the vector by transmuting that one field value to the vector
        // type and storing it through the place. (The lane-by-lane path above only handles the
        // legacy one-field-per-lane shape; without this arm we'd fall through to the class-ref
        // `unwrap` below and ICE, leaving the enclosing method as malformed IL.)
        if fields.len() == 1 {
            let (field_idx, value) = fields[0];
            let field_def = adt
                .all_fields()
                .nth(field_idx as usize)
                .expect("Could not find SIMD inner field!");
            let field_ty = field_def.ty(ctx.tcx(), subst).skip_normalization();
            let field_ty = ctx.monomorphize(field_ty);
            let field_cil = ctx.type_from_cache(field_ty);
            let as_vec = ctx.transmute_on_stack(field_cil, adt_cil, value);
            let root = place_set(dst_place, as_vec, ctx);
            return (vec![root], place_get(dst_place, ctx));
        }
    }
    let adt_type_ref = adt_cil
        .as_class_ref()
        .unwrap_or_else(|| panic!("Type {adt_type:?} is not a valuetype."));
    match adt.adt_kind() {
        AdtKind::Struct => {
            let obj_getter = place_address(dst_place, ctx);

            let mut sub_trees = Vec::new();
            for field in fields {
                let field_def = adt
                    .all_fields()
                    .nth(field.0 as usize)
                    .expect("Could not find field!");
                let field_type = field_def.ty(ctx.tcx(), subst).skip_normalization();
                let field_type = ctx.monomorphize(field_type);
                let field_type = ctx.type_from_cache(field_type);
                // Seting a void field is a no-op.
                if field_type == Type::Void {
                    continue;
                }
                let field_desc = field_descrptor(adt_type, field.0, ctx);

                let root = ctx.set_field(field_desc, obj_getter, field.1);
                sub_trees.push(root);
            }
            (sub_trees, (place_get(dst_place, ctx)))
        }
        AdtKind::Enum => {
            let adt_address_ops = place_address(dst_place, ctx);

            let variant_name = variant_name(adt_type, variant_idx);

            let variant_address = adt_address_ops;
            let mut sub_trees = Vec::new();
            let enum_variant = adt
                .variants()
                .iter()
                .nth(variant_idx as usize)
                .expect("Can't get variant index");
            for (field, field_value) in enum_variant.fields.iter().zip(fields.iter()) {
                let field_name = ctx.alloc_string(format!(
                    "{variant_name}_{fname}",
                    fname = escape_field_name(&field.name.to_string())
                ));
                let field_type = get_type(field.ty(ctx.tcx(), subst).skip_normalization(), ctx);
                // Seting a void field is a no-op.
                if field_type == cilly::Type::Void {
                    continue;
                }

                let desc = ctx.alloc_field(FieldDesc::new(adt_type_ref, field_name, field_type));
                let root = ctx.set_field(desc, variant_address, field_value.1);
                sub_trees.push(root);
            }

            let layout = ctx.layout_of(adt_type);
            let (disrc_type, _) = enum_tag_info(layout.layout, ctx);
            if disrc_type != Type::Void {
                sub_trees.push(set_discr(
                    layout.layout,
                    variant_idx.into(),
                    adt_address_ops,
                    adt_type_ref,
                    layout.ty,
                    ctx,
                ));
            }

            (sub_trees, (place_get(dst_place, ctx)))
        }
        AdtKind::Union => {
            let obj_getter = place_address(dst_place, ctx);

            let mut sub_trees = Vec::new();
            let active_field = active_field.unwrap();
            assert_eq!(fields.len(), 1);
            let field_def = adt
                .all_fields()
                .nth(active_field.as_u32() as usize)
                .expect("Could not find field!");

            let field_ty = ctx.monomorphize(field_def.ty(ctx.tcx(), subst).skip_normalization());
            let field_type = get_type(field_ty, ctx);
            // Seting a void field is a no-op.
            if field_type == cilly::Type::Void {
                return (vec![], place_get(dst_place, ctx));
            }

            let field_name = field_name(adt_type, active_field.as_u32());

            let desc = FieldDesc::new(adt_type_ref, ctx.alloc_string(field_name), field_type);
            let desc = ctx.alloc_field(desc);
            let root = ctx.set_field(desc, obj_getter, fields[0].1);
            sub_trees.push(root);
            (sub_trees, (place_get(dst_place, ctx)))
        }
    }
}
