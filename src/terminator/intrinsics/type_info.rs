use crate::assembly::MethodCompileCtx;
use cilly::{
    cilnode::ExtendKind, BinOp, Const, Int, Interned, Type, {FieldDesc},
};
use rustc_codegen_clr_place::place_set;
use rustc_codegen_clr_type::{
    utilis::{is_zst, pointer_to_is_fat},
    GetTypeExt,
};
use rustc_codgen_clr_operand::operand_address;
use rustc_middle::{
    mir::{Operand, Place},
    ty::{Instance, TyKind},
};
use rustc_span::Spanned;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

pub fn is_val_statically_known<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        1,
        "The intrinsic `is_val_statically_known` MUST take in exactly 1 argument!"
    );
    // assert_eq!(args.len(),1,"The intrinsic `unlikely` MUST take in exactly 1 argument!");
    let value_calc: Node = ctx.alloc_node(false);
    place_set(destination, value_calc, ctx)
}
pub fn size_of_val<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        1,
        "The intrinsic `size_of_val` MUST take in exactly 1 argument!"
    );

    let pointed_ty = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    if is_zst(pointed_ty, ctx.tcx()) {
        let value_calc: Node = ctx.alloc_node(Const::USize(0));
        return place_set(destination, value_calc, ctx);
    }
    if pointer_to_is_fat(pointed_ty, ctx.tcx(), ctx.instance()) {
        let ptr_ty = ctx.monomorphize(args[0].node.ty(ctx.body(), ctx.tcx()));
        match pointed_ty.kind() {
            TyKind::Str => {
                let slice_tpe = ctx.type_from_cache(ptr_ty).as_class_ref().unwrap();
                let descriptor = FieldDesc::new(
                    slice_tpe,
                    ctx.alloc_string(crate::METADATA),
                    Type::Int(Int::USize),
                );
                let addr = operand_address(&args[0].node, ctx);
                let field = ctx.alloc_field(descriptor);
                let value_calc = ctx.ld_field(addr, field);
                return place_set(destination, value_calc, ctx);
            }
            TyKind::Slice(inner) => {
                let slice_tpe = ctx.type_from_cache(ptr_ty).as_class_ref().unwrap();
                let inner = ctx.monomorphize(*inner);
                let inner_type = ctx.type_from_cache(inner);
                let descriptor = FieldDesc::new(
                    slice_tpe,
                    ctx.alloc_string(crate::METADATA),
                    Type::Int(Int::USize),
                );
                let addr = operand_address(&args[0].node, ctx);
                let field = ctx.alloc_field(descriptor);
                let len = ctx.ld_field(addr, field);
                let size = ctx.size_of(inner_type);
                let size = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
                let value_calc = ctx.biop(len, size, BinOp::Mul);
                return place_set(destination, value_calc, ctx);
            }
            // WARNING: ASSUMES ANY NON-SLICE DST IS A DYN.
            _ => {
                let slice_tpe = ctx.type_from_cache(ptr_ty).as_class_ref().unwrap();

                let descriptor = FieldDesc::new(
                    slice_tpe,
                    ctx.alloc_string(crate::METADATA),
                    Type::Int(Int::USize),
                );
                let addr = operand_address(&args[0].node, ctx);
                let field = ctx.alloc_field(descriptor);
                let meta = ctx.ld_field(addr, field);
                let meta = ctx.cast_ptr(meta, Type::Int(Int::USize));
                let size = ctx.size_of(Int::ISize);
                let size = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
                let ptr = ctx.biop(meta, size, BinOp::Add);
                let value_calc = ctx.load(ptr, Type::Int(Int::USize));
                return place_set(destination, value_calc, ctx);
            }
        }
    }
    let tpe = ctx.monomorphize(pointed_ty);
    let tpe = ctx.type_from_cache(tpe);
    let size = ctx.size_of(tpe);
    let value_calc = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
    place_set(destination, value_calc, ctx)
}
/// Lowering of the `align_of_val` intrinsic (formerly `min_align_of_val`): the alignment of the
/// value behind a `*const T` / `&T`, where `T: ?Sized`.
///
/// Mirrors [`size_of_val`]'s structure. Alignment is *statically known* for every type except
/// `dyn Trait`: sized types and slices/`str` (whose alignment is the element's, independent of the
/// length metadata). A `dyn Trait` value's alignment lives in its vtable — slot 2, after
/// `drop_in_place` (slot 0) and `size` (slot 1) — so it must be read at runtime from the metadata
/// pointer. (The old `min_align_of_val` lowering used the static path unconditionally, which is
/// wrong for `dyn`.)
pub fn align_of_val<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        1,
        "The intrinsic `align_of_val` MUST take in exactly 1 argument!"
    );
    let pointed_ty = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("align_of_val works only on types!"),
    );
    if pointer_to_is_fat(pointed_ty, ctx.tcx(), ctx.instance())
        && !matches!(pointed_ty.kind(), TyKind::Slice(_) | TyKind::Str)
    {
        // `dyn Trait`: read the alignment from vtable slot 2 (metadata is the vtable pointer).
        let ptr_ty = ctx.monomorphize(args[0].node.ty(ctx.body(), ctx.tcx()));
        let fat_tpe = ctx.type_from_cache(ptr_ty).as_class_ref().unwrap();
        let descriptor = FieldDesc::new(
            fat_tpe,
            ctx.alloc_string(crate::METADATA),
            Type::Int(Int::USize),
        );
        let addr = operand_address(&args[0].node, ctx);
        let field = ctx.alloc_field(descriptor);
        let vtable = ctx.ld_field(addr, field);
        let size = ctx.size_of(Int::ISize);
        let two = ctx.alloc_node(2_i32);
        let offset = ctx.biop(size, two, BinOp::Mul);
        let offset = ctx.int_cast(offset, Int::USize, ExtendKind::ZeroExtend);
        let sum = ctx.biop(vtable, offset, BinOp::Add);
        let align_ptr = ctx.cast_ptr(sum, Type::Int(Int::USize));
        let value_calc: Node = ctx.load(align_ptr, Type::Int(Int::USize));
        return place_set(destination, value_calc, ctx);
    }
    let align = rustc_codegen_clr_type::align_of(pointed_ty, ctx.tcx());
    let align = ctx.alloc_node(align);
    let value_calc = ctx.int_cast(align, Int::USize, ExtendKind::ZeroExtend);
    place_set(destination, value_calc, ctx)
}
