use crate::{GetTypeExt, utilis::simple_tuple};
use cilly::{Assembly, FieldDesc, Float, Int, Type, bimap::Interned};
use rustc_abi::{FieldIdx, FieldsShape, Layout, LayoutData, VariantIdx, Variants};
use rustc_codegen_clr_ctx::MethodCompileCtx;
use rustc_middle::ty::List;
use rustc_middle::ty::{AdtDef, CoroutineArgsExt, GenericArg, Ty, TyKind};
pub fn enum_variant_offsets(_: AdtDef, layout: Layout, vidix: VariantIdx) -> FieldOffsetIterator {
    FieldOffsetIterator::fields(get_variant_at_index(vidix, (*layout.0).clone()))
}

#[derive(Clone, Debug)]
pub enum FieldOffsetIterator {
    Explicit { offsets: Box<[u32]>, index: usize },
    NoOffset { count: u64 },
    Empty,
}
impl Iterator for FieldOffsetIterator {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        match self {
            Self::Explicit { offsets, index } => {
                let next = offsets.get(*index);
                *index += 1;
                next.copied()
            }
            Self::NoOffset { count } => {
                if *count > 0 {
                    *count -= 1;
                    Some(0)
                } else {
                    None
                }
            }
            Self::Empty => None,
        }
    }
}
impl FieldOffsetIterator {
    pub fn from_fields_shape(fields: &rustc_abi::FieldsShape<FieldIdx>) -> Self {
        match fields {
            FieldsShape::Arbitrary {
                offsets,
                in_memory_order,
            } => {
                // NOTE: `index` is the enumerate counter, not `_mem_idx`. Consumers call
                // `nth()` with the MIR/source field index, so indexing `offsets` by the
                // source-order counter yields the byte offset for that source field. The
                // `u32::try_from(..).unwrap()` rejects truly impossible (>4 GiB) offsets;
                // there is intentionally no u16 clamp here — a struct field can legitimately
                // sit past 64 KiB, and clamping such offsets to 0 made fields alias.
                let offsets: Box<[_]> = in_memory_order
                    .iter()
                    .enumerate()
                    .map(|(index, _mem_idx)| {
                        u32::try_from(
                            offsets[FieldIdx::from_u32(u32::try_from(index).unwrap())].bytes(),
                        )
                        .unwrap()
                    })
                    .collect();
                FieldOffsetIterator::Explicit { offsets, index: 0 }
            }
            FieldsShape::Union(count) => FieldOffsetIterator::NoOffset {
                count: Into::<usize>::into(*count) as u64,
            },
            FieldsShape::Primitive => Self::Empty,
            FieldsShape::Array { stride, count } => {
                let mut curr: u32 = 0;
                let mut offsets = Vec::new();
                for _ in 0..*count {
                    offsets.push(curr);
                    curr += std::convert::TryInto::<u32>::try_into(stride.bytes())
                        .expect("Array stride too large");
                }
                FieldOffsetIterator::Explicit {
                    offsets: offsets.into(),
                    index: 0,
                }
            }
        }
    }
    pub fn fields(parent: LayoutData<FieldIdx, rustc_abi::VariantIdx>) -> FieldOffsetIterator {
        //eprintln!("ADT fields:{:?}",parent.fields);
        Self::from_fields_shape(&parent.fields)
    }
}
/// Takes layout of an enum as input, and returns the type of its tag(Void if no tag) and the size of the tag(0 if no tag).
pub fn enum_tag_info(r#enum: Layout<'_>, asm: &mut Assembly) -> (Type, u32) {
    match r#enum.variants() {
        Variants::Single { .. } => (
            Type::Void,
            FieldOffsetIterator::from_fields_shape(r#enum.fields())
                .next()
                .unwrap_or(0),
        ),
        Variants::Multiple { tag, tag_field, .. } => (
            scalr_to_type(*tag, asm),
            FieldOffsetIterator::from_fields_shape(r#enum.fields())
                .nth((*tag_field).into())
                .unwrap_or(0),
        ),
        Variants::Empty => (Type::Void, 0),
    }
}
fn scalr_to_type(scalar: rustc_abi::Scalar, asm: &mut Assembly) -> Type {
    let primitive = match scalar {
        rustc_abi::Scalar::Union { value } | rustc_abi::Scalar::Initialized { value, .. } => value,
    };
    primitive_to_type(primitive, asm)
}
fn primitive_to_type(primitive: rustc_abi::Primitive, asm: &mut Assembly) -> Type {
    use rustc_abi::Integer;
    use rustc_abi::Primitive;
    match primitive {
        Primitive::Int(int, sign) => match (int, sign) {
            (Integer::I8, true) => Type::Int(Int::I8),
            (Integer::I16, true) => Type::Int(Int::I16),
            (Integer::I32, true) => Type::Int(Int::I32),
            (Integer::I64, true) => Type::Int(Int::I64),
            (Integer::I128, true) => Type::Int(Int::I128),
            (Integer::I8, false) => Type::Int(Int::U8),
            (Integer::I16, false) => Type::Int(Int::U16),
            (Integer::I32, false) => Type::Int(Int::U32),
            (Integer::I64, false) => Type::Int(Int::U64),
            (Integer::I128, false) => Type::Int(Int::U128),
        },
        Primitive::Float(rustc_abi::Float::F16) => Type::Float(Float::F16),
        Primitive::Float(rustc_abi::Float::F32) => Type::Float(Float::F32),
        Primitive::Float(rustc_abi::Float::F64) => Type::Float(Float::F64),
        Primitive::Float(rustc_abi::Float::F128) => todo!("No support for 128 bit floats yet!"),
        Primitive::Pointer(_) => asm.nptr(Type::Void),
    }
}
pub fn get_variant_at_index(
    variant_index: VariantIdx,
    layout: LayoutData<FieldIdx, rustc_abi::VariantIdx>,
) -> LayoutData<FieldIdx, rustc_abi::VariantIdx> {
    match layout.variants {
        Variants::Single { .. } => layout,
        // `Variants::Multiple.variants` now stores reduced `VariantLayout`s rather than full
        // `LayoutData`s; `LayoutData::for_variant` reconstructs the full per-variant layout.
        Variants::Multiple { .. } => LayoutData::for_variant(&layout, variant_index),
        Variants::Empty => todo!("Empty variants have no variants."),
    }
}
pub fn enum_field_descriptor<'tcx>(
    owner_ty: Ty<'tcx>,
    field_idx: u32,
    variant_idx: u32,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<FieldDesc> {
    let (adt, subst) = as_adt(owner_ty).expect("Tried to get a field of a non ADT type!");
    let variant = adt
        .variants()
        .iter()
        .nth(variant_idx as usize)
        .expect("No enum variant with such index!");
    let field = variant
        .fields
        .iter()
        .nth(field_idx as usize)
        .expect("No enum field with provided index!");
    let variant_name = variant.name.to_string();
    let field_name = ctx.alloc_string(format!(
        "{variant_name}_{fname}",
        fname = crate::r#type::escape_field_name(&field.name.to_string())
    ));
    let field_ty = field.ty(ctx.tcx(), subst).skip_normalization();
    let field_ty = ctx.monomorphize(field_ty);
    let field_ty = ctx.type_from_cache(field_ty);
    let owner_ty = ctx
        .type_from_cache(owner_ty)
        .as_class_ref()
        .expect("Error: tried to set a field of a non-object type!");

    ctx.alloc_field(FieldDesc::new(owner_ty, field_name, field_ty))
}
/// The name of coroutine variant `v`, matching `ty::CoroutineArgs::variant_name`. The first
/// `CoroutineArgs::RESERVED_VARIANTS` (3) variants are the reserved Unresumed/Returned/Panicked
/// states; the rest are suspend points. This MUST stay byte-identical to the field-name scheme
/// used when the coroutine class is declared in `type::coroutine_typedef`, so that
/// `ld_field`/`set_field`/`ld_field_addr` resolve against the declared per-variant fields.
#[must_use]
pub fn coroutine_variant_name(v: VariantIdx) -> String {
    match v.as_u32() {
        0 => "Unresumed".into(),
        1 => "Returned".into(),
        2 => "Panicked".into(),
        n => format!("Suspend{}", n - 3),
    }
}
/// Builds the [`FieldDesc`] for `(coroutine as variant#variant_idx).field_idx` — a saved-local
/// field of a coroutine state. Coroutines are enum-like (`Variants::Multiple`); the field type
/// comes from the coroutine's per-variant saved-local tys (via `state_tys`) and the field name
/// from [`coroutine_variant_name`]. This mirrors [`enum_field_descriptor`], but coroutines have
/// neither a `VariantDef` (for names) nor `FieldDef`s (for tys), so both are sourced from the
/// coroutine APIs instead of `as_adt`.
pub fn coroutine_field_descriptor<'tcx>(
    owner_ty: Ty<'tcx>,
    field_idx: u32,
    variant_idx: u32,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<FieldDesc> {
    let TyKind::Coroutine(def_id, args) = owner_ty.kind() else {
        panic!("coroutine_field_descriptor on non-coroutine {owner_ty:?}")
    };
    let var = VariantIdx::from_u32(variant_idx);
    // Field TYPE: the `field_idx`th saved local live across suspend point `variant_idx`.
    let field_ty = args
        .as_coroutine()
        .state_tys(*def_id, ctx.tcx())
        .nth(variant_idx as usize)
        .expect("No coroutine variant with such index!")
        .nth(field_idx as usize)
        .expect("No coroutine saved-local field with provided index!");
    let field_ty = ctx.monomorphize(field_ty);
    let field_ty = ctx.type_from_cache(field_ty);
    // Field NAME: must byte-match the typedef in `type::coroutine_typedef`.
    let field_name = ctx.alloc_string(format!(
        "{vname}_{field_idx}",
        vname = coroutine_variant_name(var)
    ));
    let owner = ctx
        .type_from_cache(owner_ty)
        .as_class_ref()
        .expect("Coroutine type is not a class!");
    ctx.alloc_field(FieldDesc::new(owner, field_name, field_ty))
}
/// Dispatches a variant-field descriptor request to the enum or coroutine builder. Both enums
/// and coroutines lower to `Variants::Multiple` and are accessed through `PlaceTy::EnumVariant`
/// (an enum/coroutine Downcast followed by a Field), so the place-handling code shares this one
/// entry point.
pub fn variant_field_descriptor<'tcx>(
    owner_ty: Ty<'tcx>,
    field_idx: u32,
    variant_idx: u32,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<FieldDesc> {
    if let TyKind::Coroutine(..) = owner_ty.kind() {
        coroutine_field_descriptor(owner_ty, field_idx, variant_idx, ctx)
    } else {
        enum_field_descriptor(owner_ty, field_idx, variant_idx, ctx)
    }
}
pub fn field_descrptor<'tcx>(
    owner_ty: Ty<'tcx>,
    field_idx: u32,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<FieldDesc> {
    if let TyKind::Tuple(elements) = owner_ty.kind() {
        let element = elements[field_idx as usize];
        let element = ctx.monomorphize(element);
        let element = ctx.type_from_cache(element);
        let elements = elements
            .iter()
            .map(|tpe| {
                let tpe = ctx.monomorphize(tpe);
                ctx.type_from_cache(tpe)
            })
            .collect::<Vec<_>>();
        let field_name = ctx.alloc_string(format!("Item{}", field_idx + 1));
        let tuple_type = simple_tuple(&elements, ctx);
        return ctx.alloc_field(FieldDesc::new(tuple_type, field_name, element));
    } else if let TyKind::Closure(_, args) = owner_ty.kind() {
        let closure = args.as_closure();
        let field_type = closure
            .upvar_tys()
            .iter()
            .nth(field_idx as usize)
            .expect("Could not find closure fields!");
        let field_type = ctx.monomorphize(field_type);
        let field_type = ctx.type_from_cache(field_type);
        let owner_ty = ctx.monomorphize(owner_ty);
        let owner_type = ctx.type_from_cache(owner_ty);
        let field_name = ctx.alloc_string(format!("f_{field_idx}"));
        return ctx.alloc_field(FieldDesc::new(
            owner_type.as_class_ref().expect("Closure type invalid!"),
            field_name,
            field_type,
        ));
    } else if let TyKind::Coroutine(_, args) = owner_ty.kind() {
        // A struct-path `Field` on a coroutine accesses one of its *upvar* fields (the captured
        // environment, laid out as `f_N` exactly like a closure). Saved-local fields, by
        // contrast, are reached through a `Downcast` (the `PlaceTy::EnumVariant` path ->
        // `coroutine_field_descriptor`) and never come here. Mirror the closure branch.
        let coroutine = args.as_coroutine();
        let field_type = coroutine
            .upvar_tys()
            .iter()
            .nth(field_idx as usize)
            .expect("Could not find coroutine upvar field!");
        let field_type = ctx.monomorphize(field_type);
        let field_type = ctx.type_from_cache(field_type);
        let owner_ty = ctx.monomorphize(owner_ty);
        let owner_type = ctx.type_from_cache(owner_ty);
        let field_name = ctx.alloc_string(format!("f_{field_idx}"));
        return ctx.alloc_field(FieldDesc::new(
            owner_type.as_class_ref().expect("Coroutine type invalid!"),
            field_name,
            field_type,
        ));
    }
    let (adt, subst) = as_adt(owner_ty).expect("Tried to get a field of a non ADT or tuple type!");
    let field = adt
        .all_fields()
        .nth(field_idx as usize)
        .expect("No field with provided index!");
    let field_name = crate::r#type::escape_field_name(&field.name.to_string());
    let field_ty = field.ty(ctx.tcx(), subst).skip_normalization();
    let field_ty = ctx.monomorphize(field_ty);
    let field_ty = ctx.type_from_cache(field_ty);
    let owner_ty = ctx
        .type_from_cache(owner_ty)
        .as_class_ref()
        .expect("Error: tried to set a field of a non-object type!");
    let field_name = ctx.alloc_string(field_name);
    ctx.alloc_field(FieldDesc::new(owner_ty, field_name, field_ty))
}
pub fn as_adt<'tcx>(ty: Ty<'tcx>) -> Option<(AdtDef<'tcx>, &'tcx List<GenericArg<'tcx>>)> {
    match ty.kind() {
        TyKind::Adt(adt, subst) => Some((*adt, subst)),
        _ => None,
    }
}
