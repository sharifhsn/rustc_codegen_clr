use crate::assembly::MethodCompileCtx;

use cilly::cilnode::ExtendKind;
use cilly::{BinOp, Const, IntoAsmIndex, Type};
use cilly::{FieldDesc, Int, Interned};
use rustc_abi::FieldIdx;
use rustc_abi::FIRST_VARIANT;
use rustc_codegen_clr_place::place_address_raw;
use rustc_codegen_clr_type::r#type::fat_ptr_to;
use rustc_codegen_clr_type::utilis::is_fat_ptr;
use rustc_codegen_clr_type::GetTypeExt;
use rustc_codgen_clr_operand::constant::get_vtable;
use rustc_codgen_clr_operand::{handle_operand, operand_address};
use rustc_middle::{
    mir::{Operand, Place},
    ty::{layout::TyAndLayout, Ty, TyKind, UintTy},
};

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

/// Preforms an unsizing cast on operand `operand`, converting it to the `target` type.
pub fn unsize<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    operand: &Operand<'tcx>,
    target: Ty<'tcx>,
    destination: Place<'tcx>,
) -> (Vec<Root>, Node) {
    // Get the monomorphized source and target type
    let target = ctx.monomorphize(target);
    let source = ctx.monomorphize(operand.ty(ctx.body(), ctx.tcx()));
    // Get the source and target types as .NET types

    let target_type = ctx.type_from_cache(target);
    // Get the target type as a fat pointer.

    let src_cil = operand_address(operand, ctx);

    let metadata = unsize_metadata(
        ctx,
        src_cil,
        ctx.layout_of(operand.ty(ctx.body(), ctx.tcx())),
        ctx.layout_of(target),
    );
    let fat_ptr_type = fat_ptr_to(Ty::new_uint(ctx.tcx(), UintTy::U8), ctx);

    let metadata_field = FieldDesc::new(
        fat_ptr_type,
        ctx.alloc_string(crate::METADATA),
        cilly::Type::Int(Int::USize),
    );
    let ptr_field = FieldDesc::new(
        fat_ptr_type,
        ctx.alloc_string(crate::DATA_PTR),
        ctx.nptr(cilly::Type::Void),
    );
    let dst = place_address_raw(&destination, ctx);
    let target_ptr = dst;

    let fat_ptr_ptr = ctx.nptr(fat_ptr_type);
    let init_metadata = {
        let addr = ctx.cast_ptr_to(target_ptr, fat_ptr_ptr);
        let val = ctx.cast_ptr_to(metadata, Type::Int(Int::USize));
        let desc = ctx.alloc_field(metadata_field);
        ctx.set_field(desc, addr, val)
    };

    let init_ptr = if is_fat_ptr(source, ctx.tcx(), ctx.instance()) {
        let void_ptr = ctx.nptr(Type::Void);
        let addr = ctx.cast_ptr_to(target_ptr, fat_ptr_ptr);
        let src_addr = operand_address(operand, ctx);
        let void_ptr_ptr = ctx.nptr(void_ptr);
        let src_addr = ctx.cast_ptr_to(src_addr, void_ptr_ptr);
        let loaded = ctx.nptr(Type::Void);
        let val = ctx.load(src_addr, loaded);
        let desc = ctx.alloc_field(ptr_field);
        ctx.set_field(desc, addr, val)
    } else {
        let operand = if source.is_any_ptr() {
            handle_operand(operand, ctx)
        } else {
            let source_type = ctx.type_from_cache(source);
            // If this type is a box<thin>, then its layout *should* be equivalent to a pointer, so this *should* be OK.
            let op = handle_operand(operand, ctx);
            ctx.transmute_on_stack(source_type, Type::Int(Int::USize), op)
        };
        // `source` is not a fat pointer, so operand should be a pointer.

        let addr = ctx.cast_ptr_to(target_ptr, fat_ptr_ptr);
        let void_ptr = ctx.nptr(Type::Void);
        let val = ctx.cast_ptr_to(operand, void_ptr);
        let desc = ctx.alloc_field(ptr_field);
        ctx.set_field(desc, addr, val)
    };
    let source_size = ctx.layout_of(source).size.bytes();
    let target_size = ctx.layout_of(source).size.bytes();
    // Assumes a 64 bit pointer!
    let copy_val = if source_size > 8 && !source.is_any_ptr() && target_size != source_size {
        let addr = operand_address(operand, ctx);

        let eight = ctx.alloc_node(8_isize);
        let addr = ctx.biop(addr, eight, BinOp::Add);
        let dst_addr = ctx.ref_to_ptr(dst);
        let const_16 = ctx.alloc_node(16_isize);
        let dst_addr = ctx.biop(dst_addr, const_16, BinOp::Add);
        eprintln!("WARNING:Can't propely unsize types with sized fields yet. unsize assumes that layout of Wrapper<&T> ==   layout of Wrapper<FatPtr<T>>!");
        let len = ctx.alloc_node(Const::USize(source_size - 8));
        ctx.cp_blk(dst_addr, addr, len)
    } else {
        ctx.alloc_root(cilly::CILRoot::Nop)
    };
    let ptr = ctx.nptr(target_type);
    let dst = ctx.cast_ptr_to(dst, ptr);
    (
        [copy_val, init_metadata, init_ptr].into(),
        ctx.load(dst, target_type),
    )
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
        (
            &TyKind::Dynamic(data_a, _),
            &TyKind::Dynamic(data_b, _),
        ) => {
            let old_info =
                old_info.expect("unsized_info: missing old info for trait upcasting coercion");
            if data_a.principal_def_id() == data_b.principal_def_id() {
                // A NOP cast that doesn't actually change anything, should be allowed even with invalid vtables.
                return old_info;
            }

            // trait upcasting coercion
            let vptr_entry_idx = ctx.tcx().supertrait_vtable_slot((source, target));

            if let Some(entry_idx) = vptr_entry_idx {
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

fn load_scalar_pair(addr: Node, ctx: &mut MethodCompileCtx<'_, '_>) -> (Node, Node) {
    let usize_ptr = ctx.nptr(Type::Int(Int::USize));
    let first_addr = ctx.cast_ptr_to(addr, usize_ptr);
    let first = ctx.load(first_addr, Type::Int(Int::USize));

    let size = ctx.size_of(Int::ISize).into_idx(ctx);
    let size = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
    let second_addr = ctx.biop(addr, size, BinOp::Add);
    let usize_ptr = ctx.nptr(Type::Int(Int::USize));
    let second_addr = ctx.cast_ptr_to(second_addr, usize_ptr);
    let second = ctx.load(second_addr, Type::Int(Int::USize));
    (first, second)
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
/// Coerce `src`, which is a reference to a value of type `src_ty`,
/// to a value of type `dst_ty` and store the result in `dst`
fn unsize_metadata<'tcx>(
    fx: &mut MethodCompileCtx<'tcx, '_>,
    src_cil: Node,
    src_ty: TyAndLayout<'tcx>,
    dst_ty: TyAndLayout<'tcx>,
) -> Node {
    // Pattern types are layout-transparent; see `peel_pattern_type`. The address (`src_cil`) is
    // unchanged because the layout is identical.
    let src_ty = peel_pattern_type(fx, src_ty);
    let dst_ty = peel_pattern_type(fx, dst_ty);
    let coerce_ptr = |fx: &mut MethodCompileCtx<'tcx, '_>| {
        if fx
            .layout_of(src_ty.ty.builtin_deref(true).unwrap())
            .is_unsized()
        {
            let (_, old_info) = load_scalar_pair(src_cil, fx);
            unsize_ptr_metadata(fx, src_ty, dst_ty, Some(old_info))
        } else {
            unsize_ptr_metadata(fx, src_ty, dst_ty, None)
        }
    };

    match (&src_ty.ty.kind(), &dst_ty.ty.kind()) {
        (&TyKind::Ref(..), &TyKind::Ref(..) | &TyKind::RawPtr(..))
        | (&TyKind::RawPtr(..), &TyKind::RawPtr(..)) => coerce_ptr(fx),
        (&TyKind::Adt(def_a, subst_a), &TyKind::Adt(def_b, subst_b)) => {
            assert_eq!(def_a, def_b);

            for i in 0..def_a.variant(FIRST_VARIANT).fields.len() {
                let src_f = &def_a.variant(FIRST_VARIANT).fields[FieldIdx::from_usize(i)];
                let dst_f = &def_b.variant(FIRST_VARIANT).fields[FieldIdx::from_usize(i)];
                let src_f_ty = fx.layout_of(src_f.ty(fx.tcx(), subst_a).skip_normalization());
                let dst_f_ty = fx.layout_of(dst_f.ty(fx.tcx(), subst_b).skip_normalization());
                if src_f_ty.layout.is_zst() {
                    // No data here, nothing to copy/coerce.
                    continue;
                }
                if src_f_ty.ty != dst_f_ty.ty {
                    return unsize_metadata(fx, src_cil, src_f_ty, dst_f_ty);
                }
            }
            todo!()
        }
        _ => panic!("unsize_metadata: invalid coercion {src_ty:?} -> {dst_ty:?}",),
    }
}
/// Coerce `src` to `dst_ty`.
fn unsize_ptr_metadata<'tcx>(
    fx: &mut MethodCompileCtx<'tcx, '_>,

    src_layout: TyAndLayout<'tcx>,
    dst_layout: TyAndLayout<'tcx>,
    old_info: Option<Node>,
) -> Node {
    // Peel layout-transparent pattern types (e.g. `NonNull`'s `*const T is !null`) so the
    // pointer/metadata dispatch below sees the underlying `RawPtr`/`Adt`.
    let src_layout = peel_pattern_type(fx, src_layout);
    let dst_layout = peel_pattern_type(fx, dst_layout);
    match (&src_layout.ty.kind(), &dst_layout.ty.kind()) {
        (&TyKind::Ref(_, a, _), &TyKind::Ref(_, b, _) | &TyKind::RawPtr(b, _))
        | (&TyKind::RawPtr(a, _), &TyKind::RawPtr(b, _)) => unsized_info(fx, *a, *b, old_info),
        (&TyKind::Adt(def_a, _), &TyKind::Adt(def_b, _)) => {
            assert_eq!(def_a, def_b);

            if src_layout == dst_layout {
                return old_info.unwrap();
            }

            let mut result = None;
            for i in 0..src_layout.fields.count() {
                let src_f = src_layout.field(fx, i);

                assert_eq!(
                    src_layout.fields.offset(i).bytes(),
                    0,
                    "{:?}",
                    src_layout.ty
                );
                assert_eq!(dst_layout.fields.offset(i).bytes(), 0);
                if src_f.is_1zst() {
                    // We are looking for the one non-1-ZST field; this is not it.
                    continue;
                }
                assert_eq!(src_layout.size, src_f.size);

                let dst_f = dst_layout.field(fx, i);
                assert_ne!(src_f.ty, dst_f.ty);
                assert_eq!(result, None);
                result = Some(unsize_ptr_metadata(fx, src_f, dst_f, old_info));
            }
            result.unwrap()
        }
        _ => panic!("unsize_ptr_metadata: called on bad types"),
    }
}
// New unsizing semantics should use new local allocator
