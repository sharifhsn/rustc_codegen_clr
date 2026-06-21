use crate::assembly::MethodCompileCtx;
use cilly::{
    cilnode::{ExtendKind, IsPure, MethodKind},
    CILRoot, Int, Interned, MethodRef, Type,
};
use rustc_codegen_clr_place::place_set;
use rustc_codegen_clr_type::GetTypeExt;
use rustc_codgen_clr_operand::handle_operand;
use rustc_middle::{
    mir::{Operand, Place},
    ty::Instance,
};
use rustc_span::Spanned;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

/// Takes in 3 args. dst, val, and count. writes count * sizeof(T) bytes of value `val` to dst.
pub fn write_bytes<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        3,
        "The intrinsic `write_bytes` MUST take in exactly 3 argument!"
    );
    let tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    // Writing bytes over N ZSTs writes zero bytes. A ZST lowers to `Type::Void`,
    // whose `size_of` is invalid, so short-circuit to a no-op.
    if ctx.layout_of(tpe).is_zst() {
        return ctx.alloc_root(CILRoot::Nop);
    }
    let tpe = ctx.type_from_cache(tpe);
    let dst = handle_operand(&args[0].node, ctx);
    let val = handle_operand(&args[1].node, ctx);
    let count = handle_operand(&args[2].node, ctx);
    let size = ctx.size_of(tpe);
    let size = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
    let count = ctx.biop(count, size, cilly::BinOp::Mul);
    ctx.init_blk(dst, val, count)
}
/// Takes in 3 args. dst, src, and count. copies count * sizeof(T) bytes from src to dst .
pub fn copy<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        3,
        "The intrinsic `copy` MUST take in exactly 3 argument!"
    );
    let tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    if ctx.layout_of(tpe).is_zst() {
        return ctx.alloc_root(CILRoot::Nop);
    }
    let tpe = ctx.type_from_cache(tpe);
    let src = handle_operand(&args[0].node, ctx);
    let dst = handle_operand(&args[1].node, ctx);
    let count = handle_operand(&args[2].node, ctx);
    let size = ctx.size_of(tpe);
    let size = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
    let count = ctx.biop(count, size, cilly::BinOp::Mul);

    ctx.cp_blk(dst, src, count)
}
pub fn raw_eq<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    // Raw eq returns 0 if values are not equal, and 1 if they are, unlike memcmp, which does the oposite.
    let tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("raw_eq works only on types!"),
    );
    // Raw eq always true for zsts.
    if ctx.layout_of(tpe).is_zst() {
        let t = ctx.alloc_node(true);
        return place_set(destination, t, ctx);
    }
    let tpe = ctx.type_from_cache(tpe);
    let size = match tpe {
        Type::Bool
        | Type::Int(
            Int::U8
            | Int::I8
            | Int::U16
            | Int::I16
            | Int::U32
            | Int::I32
            | Int::U64
            | Int::I64
            | Int::USize
            | Int::ISize,
        )
        | Type::Ptr(_) => {
            let a = handle_operand(&args[0].node, ctx);
            let b = handle_operand(&args[1].node, ctx);
            let eq = ctx.biop(a, b, cilly::BinOp::Eq);
            return place_set(destination, eq, ctx);
        }
        _ => ctx.size_of(tpe),
    };
    let a = handle_operand(&args[0].node, ctx);
    let u8_ptr = ctx.nptr(Type::Int(Int::U8));
    let a = ctx.cast_ptr_to(a, u8_ptr);
    let b = handle_operand(&args[1].node, ctx);
    let u8_ptr = ctx.nptr(Type::Int(Int::U8));
    let b = ctx.cast_ptr_to(b, u8_ptr);
    let len = ctx.int_cast(size, Int::USize, ExtendKind::ZeroExtend);
    let cmp = compare_bytes(a, b, len, ctx);
    let zero = ctx.alloc_node(0_i32);
    let eq = ctx.biop(cmp, zero, cilly::BinOp::Eq);
    place_set(destination, eq, ctx)
}
/// Calls `memcmp` to compare `len` bytes of `a` and `b`.
fn compare_bytes(a: Node, b: Node, len: Node, ctx: &mut MethodCompileCtx<'_, '_>) -> Node {
    let u8_ref = ctx.nptr(Type::Int(Int::U8));
    let mref = MethodRef::new(
        *ctx.main_module(),
        ctx.alloc_string("memcmp"),
        ctx.sig([u8_ref, u8_ref, Type::Int(Int::USize)], Type::Int(Int::I32)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = ctx.alloc_methodref(mref);
    ctx.call(mref, &[a, b, len], IsPure::NOT)
}
