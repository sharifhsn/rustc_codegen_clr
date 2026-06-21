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
        // Discriminate dyn-vs-slice/str on the *struct tail* of the fat pointee, not on the
        // pointee's own `TyKind`. A DST-tailed struct (e.g. `ArcInner<[u8]>`) has `kind() == Adt`
        // but its tail is `[u8]`/`str`; matching on the pointee kind would route it to the `dyn`
        // vtable path and dereference the slice *length* as a vtable pointer (NullReferenceException).
        let tail = ctx
            .tcx()
            .struct_tail_for_codegen(pointed_ty, rustc_middle::ty::TypingEnv::fully_monomorphized());
        match tail.kind() {
            TyKind::Slice(_) | TyKind::Str => {
                // size = align_up(prefix + len * stride, align), where:
                //   prefix = offset of the unsized tail (0 for a bare slice/str, >0 for a tailed
                //            struct: this is `layout.size()` of the unsized type, i.e. the sized
                //            prefix's size),
                //   len    = the fat-pointer metadata (element count),
                //   stride = size of the tail element (1 for `str`),
                //   align  = the type's ABI alignment (statically known).
                let layout = ctx.layout_of(pointed_ty);
                let prefix = layout.layout.size().bytes();
                let align = layout.layout.align().abi.bytes();
                let stride = match tail.kind() {
                    TyKind::Str => 1,
                    TyKind::Slice(elem) => {
                        let elem = ctx.monomorphize(*elem);
                        ctx.layout_of(elem).layout.size().bytes()
                    }
                    _ => unreachable!(),
                };
                let fat_tpe = ctx.type_from_cache(ptr_ty).as_class_ref().unwrap();
                let descriptor = FieldDesc::new(
                    fat_tpe,
                    ctx.alloc_string(crate::METADATA),
                    Type::Int(Int::USize),
                );
                let addr = operand_address(&args[0].node, ctx);
                let field = ctx.alloc_field(descriptor);
                let len = ctx.ld_field(addr, field);
                let stride = ctx.alloc_node(Const::USize(stride));
                let body = ctx.biop(len, stride, BinOp::Mul);
                let body = if prefix != 0 {
                    let prefix = ctx.alloc_node(Const::USize(prefix));
                    ctx.biop(body, prefix, BinOp::Add)
                } else {
                    body
                };
                // align_up(body, align) = (body + (align - 1)) & !(align - 1).
                let value_calc = if align > 1 {
                    let align_m1 = ctx.alloc_node(Const::USize(align - 1));
                    let rounded = ctx.biop(body, align_m1, BinOp::Add);
                    let mask = ctx.alloc_node(Const::USize(!(align - 1)));
                    ctx.biop(rounded, mask, BinOp::And)
                } else {
                    body
                };
                return place_set(destination, value_calc, ctx);
            }
            // `dyn Trait`: the metadata is the vtable pointer; size lives in vtable slot 1.
            _ => {
                let fat_tpe = ctx.type_from_cache(ptr_ty).as_class_ref().unwrap();

                let descriptor = FieldDesc::new(
                    fat_tpe,
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
    // Alignment is statically known for every fat pointee except `dyn Trait`. Discriminate on the
    // *struct tail* (not the pointee's own `TyKind`): a slice/str-tailed struct like `ArcInner<[u8]>`
    // has `kind() == Adt` but a statically-known alignment, and must NOT be routed to the vtable path
    // (which would deref the slice length as a vtable pointer -> NullReferenceException).
    let tail_is_dyn = pointer_to_is_fat(pointed_ty, ctx.tcx(), ctx.instance())
        && matches!(
            ctx.tcx()
                .struct_tail_for_codegen(
                    pointed_ty,
                    rustc_middle::ty::TypingEnv::fully_monomorphized()
                )
                .kind(),
            TyKind::Dynamic(..)
        );
    if tail_is_dyn {
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
