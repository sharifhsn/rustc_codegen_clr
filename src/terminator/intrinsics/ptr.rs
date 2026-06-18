use crate::assembly::MethodCompileCtx;
use cilly::{cilnode::ExtendKind, Int, Interned, Type};
use rustc_codegen_clr_place::place_set;
use rustc_codegen_clr_type::GetTypeExt;
use rustc_codgen_clr_operand::handle_operand;
use rustc_middle::{
    mir::{Operand, Place},
    ty::Instance,
};
use rustc_span::Spanned;

type Root = Interned<cilly::v2::CILRoot>;

pub fn arith_offset<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("arith_offset works only on types!"),
    );
    let tpe = ctx.type_from_cache(tpe);

    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let size = ctx.size_of(tpe);
    let size = ctx.int_cast(size, Int::ISize, ExtendKind::SignExtend);
    let offset = ctx.biop(b, size, cilly::BinOp::Mul);
    let calc = ctx.biop(a, offset, cilly::BinOp::Add);
    place_set(destination, calc, ctx)
}
pub fn ptr_offset_from_unsigned<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        2,
        "The intrinsic `ptr_offset_from_unsigned` MUST take in exactly 2 arguments!"
    );
    let ty = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("ptr_offset_from_unsigned works only on types!"),
    );
    let tpe = ctx.type_from_cache(ty);
    // This is UB, so we can do whatever.
    if ctx.layout_of(ty).is_zst() {
        return ctx.throw_msg(&format!("ptr_offset_from_unsigned called with zst type:{ty}"));
    }
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let diff = ctx.biop(a, b, cilly::BinOp::Sub);
    let diff = ctx.cast_ptr_to(diff, Type::Int(Int::USize));
    let size = ctx.size_of(tpe);
    let size = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
    let calc = ctx.biop(diff, size, cilly::BinOp::DivUn);
    place_set(destination, calc, ctx)
}
pub fn ptr_offset_from<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        2,
        "The intrinsic `ptr_offset_from` MUST take in exactly 1 argument!"
    );
    let ty = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    // This is UB, so we can do whatever.
    if ctx.layout_of(ty).is_zst() {
        return ctx.throw_msg(&format!("ptr_offset_from called with zst type:{ty}"));
    }
    let tpe = ctx.type_from_cache(ty);

    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let diff = ctx.biop(a, b, cilly::BinOp::Sub);
    let diff = ctx.cast_ptr_to(diff, Type::Int(Int::ISize));
    let size = ctx.size_of(tpe);
    let size = ctx.int_cast(size, Int::ISize, ExtendKind::SignExtend);
    let calc = ctx.biop(diff, size, cilly::BinOp::Div);
    place_set(destination, calc, ctx)
}
