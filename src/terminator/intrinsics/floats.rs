use crate::assembly::MethodCompileCtx;
use cilly::{
    cilnode::MethodKind, Int, Interned, MethodRef, Type, {ClassRef, Float},
};
use cilly::cilnode::IsPure;
use rustc_codegen_clr_place::place_set;
use rustc_codgen_clr_operand::handle_operand;
use rustc_middle::mir::{Operand, Place};
use rustc_span::Spanned;

type Root = Interned<cilly::ir::CILRoot>;

/// Implementation of the fmaf32 intrinsics. Takes in 3 arguments: a, b, c. Calcualtes a * b + c
pub fn fmaf32<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let sig = ctx.sig(
        [
            Type::Float(Float::F32),
            Type::Float(Float::F32),
            Type::Float(Float::F32),
        ],
        Type::Float(Float::F32),
    );
    let mref = MethodRef::new(
        ClassRef::single(ctx),
        ctx.alloc_string("FusedMultiplyAdd"),
        sig,
        MethodKind::Static,
        vec![].into(),
    );
    let mref = ctx.alloc_methodref(mref);
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let c = handle_operand(&args[2].node, ctx);
    let value_calc = ctx.call(mref, &[a, b, c], IsPure::NOT);
    place_set(destination, value_calc, ctx)
}
/// Implementation of the fmaf64 intrinsics. Takes in 3 arguments: a, b, c. Calcualtes a * b + c
pub fn fmaf64<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let sig = ctx.sig(
        [
            Type::Float(Float::F64),
            Type::Float(Float::F64),
            Type::Float(Float::F64),
        ],
        Type::Float(Float::F64),
    );
    let mref = MethodRef::new(
        ClassRef::double(ctx),
        ctx.alloc_string("FusedMultiplyAdd"),
        sig,
        MethodKind::Static,
        vec![].into(),
    );
    let mref = ctx.alloc_methodref(mref);
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let c = handle_operand(&args[2].node, ctx);
    let value_calc = ctx.call(mref, &[a, b, c], IsPure::NOT);
    place_set(destination, value_calc, ctx)
}
pub fn powif32<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        2,
        "The intrinsic `powif32` MUST take in exactly 2 arguments!"
    );
    let sig = ctx.sig(
        [Type::Float(Float::F32), Type::Float(Float::F32)],
        Type::Float(Float::F32),
    );
    let pow = MethodRef::new(
        ClassRef::single(ctx),
        ctx.alloc_string("Pow"),
        sig,
        MethodKind::Static,
        vec![].into(),
    );
    let pow = ctx.alloc_methodref(pow);
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let b = ctx.float_cast(b, Float::F32, true);
    let value_calc = ctx.call(pow, &[a, b], IsPure::NOT);
    place_set(destination, value_calc, ctx)
}
pub fn powif64<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        2,
        "The intrinsic `powif64` MUST take in exactly 2 arguments!"
    );
    let sig = ctx.sig(
        [Type::Float(Float::F64), Type::Float(Float::F64)],
        Type::Float(Float::F64),
    );
    let pow = MethodRef::new(
        ClassRef::double(ctx),
        ctx.alloc_string("Pow"),
        sig,
        MethodKind::Static,
        vec![].into(),
    );
    let pow = ctx.alloc_methodref(pow);
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let b = ctx.float_cast(b, Float::F64, true);
    let value_calc = ctx.call(pow, &[a, b], IsPure::NOT);
    place_set(destination, value_calc, ctx)
}
pub fn powf32<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let sig = ctx.sig(
        [Type::Float(Float::F32), Type::Float(Float::F32)],
        Type::Float(Float::F32),
    );
    let pow = MethodRef::new(
        ClassRef::single(ctx),
        ctx.alloc_string("Pow"),
        sig,
        MethodKind::Static,
        vec![].into(),
    );
    let pow = ctx.alloc_methodref(pow);
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let value_calc = ctx.call(pow, &[a, b], IsPure::NOT);
    place_set(destination, value_calc, ctx)
}
pub fn powf64<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let sig = ctx.sig(
        [Type::Float(Float::F64), Type::Float(Float::F64)],
        Type::Float(Float::F64),
    );
    let pow = MethodRef::new(
        ClassRef::double(ctx),
        ctx.alloc_string("Pow"),
        sig,
        MethodKind::Static,
        vec![].into(),
    );
    let pow = ctx.alloc_methodref(pow);
    let a = handle_operand(&args[0].node, ctx);
    let b = handle_operand(&args[1].node, ctx);
    let value_calc = ctx.call(pow, &[a, b], IsPure::NOT);
    place_set(destination, value_calc, ctx)
}
pub fn roundf32<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let rounding = ClassRef::midpoint_rounding(ctx);
    let sig = ctx.sig(
        [Type::Float(Float::F32), Type::ClassRef(rounding)],
        Type::Float(Float::F32),
    );
    let round = MethodRef::new(
        ClassRef::mathf(ctx),
        ctx.alloc_string("Round"),
        sig,
        MethodKind::Static,
        vec![].into(),
    );
    let round = ctx.alloc_methodref(round);
    let a = handle_operand(&args[0].node, ctx);
    let one = ctx.alloc_node(1_i32);
    let one = ctx.transmute_on_stack(Type::Int(Int::I32), Type::ClassRef(rounding), one);
    let value_calc = ctx.call(round, &[a, one], IsPure::NOT);
    place_set(destination, value_calc, ctx)
}
pub fn roundf64<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let rounding = ClassRef::midpoint_rounding(ctx);
    let sig = ctx.sig(
        [Type::Float(Float::F64), Type::ClassRef(rounding)],
        Type::Float(Float::F64),
    );
    let round = MethodRef::new(
        ClassRef::math(ctx),
        ctx.alloc_string("Round"),
        sig,
        MethodKind::Static,
        vec![].into(),
    );
    let round = ctx.alloc_methodref(round);
    let a = handle_operand(&args[0].node, ctx);
    let one = ctx.alloc_node(1_i32);
    let one = ctx.transmute_on_stack(Type::Int(Int::I32), Type::ClassRef(rounding), one);
    let value_calc = ctx.call(round, &[a, one], IsPure::NOT);
    place_set(destination, value_calc, ctx)
}
