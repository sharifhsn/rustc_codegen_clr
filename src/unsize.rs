use crate::assembly::MethodCompileCtx;

use crate::operand::constant::get_vtable;
use crate::operand::handle_operand;
use crate::place::place_address_raw;
use crate::r#type::GetTypeExt;
use cilly::cilnode::ExtendKind;
use cilly::{BinOp, Const, IntoAsmIndex, Type};
use cilly::{FieldDesc, Int, Interned};
use rustc_abi::{BackendRepr, FieldIdx};
use rustc_hir::LangItem;
use rustc_middle::{
    mir::{Operand, Place},
    traits::{self, ImplSource},
    ty::{self, Ty, TyKind, TypingEnv, adjustment::CustomCoerceUnsized, layout::TyAndLayout},
};

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

#[derive(Clone, Copy, Debug)]
struct FieldCopy<'tcx> {
    source_offset: u64,
    destination_offset: u64,
    ty: Ty<'tcx>,
}

#[derive(Clone, Copy, Debug)]
struct PointerCoercion<'tcx> {
    source_offset: u64,
    destination_offset: u64,
    source_ty: Ty<'tcx>,
    destination_ty: Ty<'tcx>,
}

/// A fully layout-derived coercion recipe. `CoerceUnsized` guarantees exactly one recursively
/// coerced field; every other physical field is copied at its own source/destination offset.
/// Keeping this as data lets all rustc queries and structural validation finish before we allocate
/// any CIL nodes.
#[derive(Debug)]
struct UnsizePlan<'tcx> {
    copies: Vec<FieldCopy<'tcx>>,
    pointer: PointerCoercion<'tcx>,
}

impl<'tcx> UnsizePlan<'tcx> {
    fn new(ctx: &mut MethodCompileCtx<'tcx, '_>, source: Ty<'tcx>, destination: Ty<'tcx>) -> Self {
        let mut copies = Vec::new();
        let mut pointer = None;
        build_unsize_plan(
            ctx,
            ctx.layout_of(source),
            ctx.layout_of(destination),
            0,
            0,
            &mut copies,
            &mut pointer,
        );
        Self {
            copies,
            pointer: pointer.expect("valid CoerceUnsized cast had no pointer leaf"),
        }
    }

    fn emit(
        &self,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
        source_base: Node,
        destination_base: Node,
    ) -> Vec<Root> {
        let mut roots = Vec::with_capacity(self.copies.len() + 2);
        for field in &self.copies {
            assert!(
                !crate::managed_storage::is_bitwise_managed_unsafe(field.ty, ctx),
                "managed-storage preflight missed unsize copy of {:?}",
                field.ty
            );
            let ty = ctx.type_from_cache(field.ty);
            if ty == Type::Void {
                continue;
            }
            let source = offset_address(source_base, field.source_offset, ctx);
            let destination = offset_address(destination_base, field.destination_offset, ctx);
            let field_pointer = ctx.nptr(ty);
            let source = ctx.cast_ptr_to(source, field_pointer);
            let destination = ctx.cast_ptr_to(destination, field_pointer);
            let value = ctx.load(source, ty);
            roots.push(ctx.st_ind(destination, value, ty, false));
        }
        roots.extend(emit_pointer_coercion(
            self.pointer,
            source_base,
            destination_base,
            ctx,
        ));
        roots
    }
}

/// Preforms an unsizing cast on operand `operand`, converting it to the `target` type.
pub fn unsize<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand: &Operand<'tcx>,
    target: Ty<'tcx>,
    destination: Place<'tcx>,
) -> (Vec<Root>, Node) {
    let target = ctx.monomorphize(target);
    let source = ctx.monomorphize(operand.ty(ctx.body(), ctx.tcx()));
    let plan = UnsizePlan::new(ctx, source, target);
    let source_base = operand_storage_address(operand, ctx);
    let destination_base = place_address_raw(&destination, ctx);
    let roots = plan.emit(ctx, source_base, destination_base);
    let target_type = ctx.type_from_cache(target);
    let ptr = ctx.nptr(target_type);
    let destination = ctx.cast_ptr_to(destination_base, ptr);
    (roots, ctx.load(destination, target_type))
}

fn operand_storage_address<'tcx>(
    operand: &Operand<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => place_address_raw(place, ctx),
        Operand::Constant(_) => {
            let value = handle_operand(operand, ctx);
            ctx.stack_addr(value)
        }
        Operand::RuntimeChecks(_) => {
            unreachable!("a runtime-check boolean cannot be the operand of an Unsize cast")
        }
    }
}

fn offset_address(base: Node, offset: u64, ctx: &mut MethodCompileCtx<'_, '_>) -> Node {
    if offset == 0 {
        base
    } else {
        let offset = ctx.alloc_node(Const::USize(offset));
        ctx.biop(base, offset, BinOp::Add)
    }
}

fn custom_coerce_field<'tcx>(
    ctx: &MethodCompileCtx<'tcx, '_>,
    source: Ty<'tcx>,
    destination: Ty<'tcx>,
) -> FieldIdx {
    let tcx = ctx.tcx();
    let trait_ref = ty::TraitRef::new(
        tcx,
        tcx.require_lang_item(LangItem::CoerceUnsized, ctx.span()),
        [source, destination],
    );
    let impl_source = tcx
        .codegen_select_candidate(TypingEnv::fully_monomorphized().as_query_input(trait_ref))
        .unwrap_or_else(|error| {
            panic!("could not select CoerceUnsized<{destination:?}> for {source:?}: {error:?}")
        });
    let ImplSource::UserDefined(traits::ImplSourceUserDefinedData { impl_def_id, .. }) =
        impl_source
    else {
        panic!(
            "aggregate CoerceUnsized<{destination:?}> for {source:?} did not select a custom impl: {impl_source:?}"
        );
    };
    let info = tcx
        .coerce_unsized_info(*impl_def_id)
        .unwrap_or_else(|_| panic!("invalid CoerceUnsized impl {impl_def_id:?}"));
    let Some(CustomCoerceUnsized::Struct(field)) = info.custom_kind else {
        panic!("CoerceUnsized impl {impl_def_id:?} did not identify a struct field");
    };
    field
}

#[allow(clippy::too_many_arguments)]
fn build_unsize_plan<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    source: TyAndLayout<'tcx>,
    destination: TyAndLayout<'tcx>,
    source_base: u64,
    destination_base: u64,
    copies: &mut Vec<FieldCopy<'tcx>>,
    pointer: &mut Option<PointerCoercion<'tcx>>,
) {
    let source = peel_pattern_type(ctx, source);
    let destination = peel_pattern_type(ctx, destination);
    match (source.ty.kind(), destination.ty.kind()) {
        (
            TyKind::Ref(_, _, _) | TyKind::RawPtr(_, _),
            TyKind::Ref(_, _, _) | TyKind::RawPtr(_, _),
        ) => {
            assert!(
                pointer.is_none(),
                "CoerceUnsized cast contained more than one pointer leaf"
            );
            *pointer = Some(PointerCoercion {
                source_offset: source_base,
                destination_offset: destination_base,
                source_ty: source.ty,
                destination_ty: destination.ty,
            });
        }
        (TyKind::Adt(source_def, _), TyKind::Adt(destination_def, _)) => {
            assert_eq!(
                source_def, destination_def,
                "CoerceUnsized changed aggregate definitions"
            );
            let coerce_field = custom_coerce_field(ctx, source.ty, destination.ty).as_usize();
            assert_eq!(
                source.fields.count(),
                destination.fields.count(),
                "CoerceUnsized changed aggregate field count"
            );
            assert!(
                coerce_field < source.fields.count(),
                "CoerceUnsized selected nonexistent field {coerce_field}"
            );

            for index in 0..source.fields.count() {
                let source_field = source.field(ctx, index);
                let destination_field = destination.field(ctx, index);
                let source_offset = source_base + source.fields.offset(index).bytes();
                let destination_offset =
                    destination_base + destination.fields.offset(index).bytes();
                if index == coerce_field {
                    build_unsize_plan(
                        ctx,
                        source_field,
                        destination_field,
                        source_offset,
                        destination_offset,
                        copies,
                        pointer,
                    );
                    continue;
                }

                if source_field.is_zst() && destination_field.is_zst() {
                    continue;
                }
                assert_eq!(
                    source_field.ty, destination_field.ty,
                    "non-coerced field {index} changed type during CoerceUnsized"
                );
                assert_eq!(
                    source_field.size, destination_field.size,
                    "non-coerced field {index} changed size during CoerceUnsized"
                );
                copies.push(FieldCopy {
                    source_offset,
                    destination_offset,
                    ty: source_field.ty,
                });
            }
        }
        _ => panic!(
            "invalid CoerceUnsized shape {:?} -> {:?}",
            source.ty, destination.ty
        ),
    }
}

fn pointer_leaf<'tcx>(layout: TyAndLayout<'tcx>) -> (Ty<'tcx>, bool) {
    let pointee = match layout.ty.kind() {
        TyKind::Ref(_, pointee, _) | TyKind::RawPtr(pointee, _) => *pointee,
        _ => panic!(
            "unsize plan pointer leaf was not a pointer: {:?}",
            layout.ty
        ),
    };
    let is_fat = match layout.layout.0.0.backend_repr {
        BackendRepr::Scalar(_) => false,
        BackendRepr::ScalarPair(_, _) => true,
        ref other => panic!("pointer leaf {:?} had non-pointer ABI {other:?}", layout.ty),
    };
    (pointee, is_fat)
}

fn fat_pointer_class<'tcx>(
    ty: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<cilly::ClassRef> {
    let Type::ClassRef(class) = ctx.type_from_cache(ty) else {
        panic!("fat pointer leaf {ty:?} did not lower to a CIL value class")
    };
    class
}

fn emit_pointer_coercion<'tcx>(
    pointer: PointerCoercion<'tcx>,
    source_base: Node,
    destination_base: Node,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> [Root; 2] {
    let source_layout = peel_pattern_type(ctx, ctx.layout_of(pointer.source_ty));
    let destination_layout = peel_pattern_type(ctx, ctx.layout_of(pointer.destination_ty));
    let (source_pointee, source_is_fat) = pointer_leaf(source_layout);
    let (destination_pointee, destination_is_fat) = pointer_leaf(destination_layout);
    assert!(
        destination_is_fat,
        "Unsize destination pointer leaf was not fat: {:?}",
        pointer.destination_ty
    );

    let source_address = offset_address(source_base, pointer.source_offset, ctx);
    let destination_address = offset_address(destination_base, pointer.destination_offset, ctx);
    let void_pointer = ctx.nptr(Type::Void);

    let (data, old_metadata) = if source_is_fat {
        let source_class = fat_pointer_class(pointer.source_ty, ctx);
        let source_pointer = ctx.nptr(Type::ClassRef(source_class));
        let source_address = ctx.cast_ptr_to(source_address, source_pointer);
        let data_name = ctx.alloc_string(crate::DATA_PTR);
        let metadata_name = ctx.alloc_string(crate::METADATA);
        let data_field = ctx.alloc_field(FieldDesc::new(source_class, data_name, void_pointer));
        let metadata_field = ctx.alloc_field(FieldDesc::new(
            source_class,
            metadata_name,
            Type::Int(Int::USize),
        ));
        (
            ctx.ld_field(source_address, data_field),
            Some(ctx.ld_field(source_address, metadata_field)),
        )
    } else {
        let source_type = ctx.type_from_cache(pointer.source_ty);
        let source_pointer = ctx.nptr(source_type);
        let source_address = ctx.cast_ptr_to(source_address, source_pointer);
        let data = ctx.load(source_address, source_type);
        (ctx.cast_ptr_to(data, void_pointer), None)
    };

    let metadata = unsized_info(ctx, source_pointee, destination_pointee, old_metadata);
    let destination_class = fat_pointer_class(pointer.destination_ty, ctx);
    let destination_pointer = ctx.nptr(Type::ClassRef(destination_class));
    let destination_address = ctx.cast_ptr_to(destination_address, destination_pointer);
    let data_name = ctx.alloc_string(crate::DATA_PTR);
    let metadata_name = ctx.alloc_string(crate::METADATA);
    let data_field = ctx.alloc_field(FieldDesc::new(destination_class, data_name, void_pointer));
    let metadata_field = ctx.alloc_field(FieldDesc::new(
        destination_class,
        metadata_name,
        Type::Int(Int::USize),
    ));
    let metadata = ctx.cast_ptr_to(metadata, Type::Int(Int::USize));
    [
        ctx.set_field(data_field, destination_address, data),
        ctx.set_field(metadata_field, destination_address, metadata),
    ]
}

/// Adopted from <https://github.com/rust-lang/rustc_codegen_cranelift/blob/45600348c009303847e8cddcfa8483f1f3d56625/src/unsize.rs#L64>
fn unsized_info<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    source: Ty<'tcx>,
    target: Ty<'tcx>,
    old_info: Option<Node>,
) -> Node {
    let (source, target) = ctx.tcx().struct_lockstep_tails_for_codegen(
        source,
        target,
        rustc_middle::ty::TypingEnv::fully_monomorphized(),
    );
    match (&source.kind(), &target.kind()) {
        (&TyKind::Array(_, len), &TyKind::Slice(_)) => {
            let len = len
                .try_to_target_usize(ctx.tcx())
                .expect("Could not eval array length.");
            ctx.alloc_node(Const::USize(len))
        }
        (&TyKind::Dynamic(data_a, _), &TyKind::Dynamic(data_b, _)) => {
            let old_info =
                old_info.expect("unsized_info: missing old info for trait upcasting coercion");
            if data_a.principal_def_id() == data_b.principal_def_id() {
                // A NOP cast that doesn't actually change anything, should be allowed even with invalid vtables.
                return old_info;
            }

            // trait upcasting coercion
            let vslot_idx = ctx.tcx().supertrait_vtable_slot((source, target));

            if let Some(entry_idx) = vslot_idx {
                let entry_idx = u32::try_from(entry_idx).unwrap();
                let entry_idx = ctx.alloc_node(entry_idx);
                let size = ctx.size_of(Int::USize).into_idx(ctx);
                let size = ctx.int_cast(size, Int::U32, ExtendKind::ZeroExtend);
                let entry_offset = ctx.biop(entry_idx, size, BinOp::Mul);
                let entry_offset = ctx.int_cast(entry_offset, Int::USize, ExtendKind::ZeroExtend);
                let addr = ctx.biop(old_info, entry_offset, BinOp::Add);
                let usize_ptr = ctx.nptr(Int::USize);
                let addr = ctx.cast_ptr_to(addr, usize_ptr);
                ctx.load(addr, Type::Int(Int::USize))
            } else {
                old_info
            }
        }
        (_, TyKind::Dynamic(data, ..)) => get_vtable(
            ctx,
            source,
            data.principal()
                .map(|principal| ctx.tcx().instantiate_bound_regions_with_erased(principal)),
        ),
        _ => panic!("unsized_info: invalid unsizing {source:?} -> {target:?}"),
    }
}

/// Pattern types (`T is <pattern>`, e.g. `NonNull`'s field `*const T is !null`) are
/// *layout-identical* to their base type — the pattern only refines validity. The unsizing logic
/// dispatches on `TyKind` (`RawPtr`/`Ref`/`Adt`) and operates on the underlying pointer + metadata,
/// so a `TyKind::Pat` wrapper must be peeled to its base first; otherwise the recursion through
/// `NonNull` reaches `Pat(*const T)` and falls through to the "invalid coercion" panic.
fn peel_pattern_type<'tcx>(
    fx: &mut MethodCompileCtx<'tcx, '_>,
    layout: TyAndLayout<'tcx>,
) -> TyAndLayout<'tcx> {
    if let TyKind::Pat(base, _) = layout.ty.kind() {
        // Recurse: patterns don't nest in practice, but base could itself be a pattern type.
        peel_pattern_type(fx, fx.layout_of(*base))
    } else {
        layout
    }
}
