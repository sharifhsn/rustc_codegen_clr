use crate::assembly::MethodCompileCtx;
use crate::operand::handle_operand;
use crate::place::place_set;
use cilly::{cilnode::ExtendKind, BinOp, Int, Interned, Type};
use rustc_middle::mir::{Operand, Place};
use rustc_span::Spanned;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

/// Gets the alignment of a dynamic object from a fat pointer by looking it up in the vtable.
pub fn vtable_align<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let vtableptr = handle_operand(&args[0].node, ctx);
    let size = ctx.size_of(Int::ISize);
    let two = ctx.alloc_node(2_i32);
    let offset = ctx.biop(size, two, BinOp::Mul);
    let offset = ctx.int_cast(offset, Int::USize, ExtendKind::ZeroExtend);
    let sum = ctx.biop(vtableptr, offset, BinOp::Add);
    let align_ptr = ctx.cast_ptr(sum, Type::Int(Int::USize));
    let value_calc: Node = ctx.load(align_ptr, Type::Int(Int::USize));
    place_set(destination, value_calc, ctx)
}
/// Gets the size of a dynamic object from a fat pointer, by looking it up from the vtable.
pub fn vtable_size<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let vtableptr = handle_operand(&args[0].node, ctx);
    let size = ctx.size_of(Int::ISize);
    let offset = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
    let sum = ctx.biop(vtableptr, offset, BinOp::Add);
    let size_ptr = ctx.cast_ptr(sum, Type::Int(Int::USize));
    let value_calc: Node = ctx.load(size_ptr, Type::Int(Int::USize));
    place_set(destination, value_calc, ctx)
}
