use crate::assembly::MethodCompileCtx;
use crate::operand::handle_operand;
use crate::place::place_set;
use cilly::cilnode::IsPure;
use cilly::{
    Int, Interned, Type,
    cilnode::MethodKind,
    {ClassRef, Float},
};
use rustc_middle::mir::{Operand, Place};
use rustc_span::Spanned;

type Root = Interned<cilly::ir::CILRoot>;

fn fmaf<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    float: Float,
) -> Root {
    let sig = ctx.sig([Type::Float(float); 3], Type::Float(float));
    let class = float.class(ctx);
    let method = ctx.new_methodref(class, "FusedMultiplyAdd", sig, MethodKind::Static, []);
    let values = [
        handle_operand(&args[0].node, ctx),
        handle_operand(&args[1].node, ctx),
        handle_operand(&args[2].node, ctx),
    ];
    place_set(destination, ctx.call(method, &values, IsPure::NOT), ctx)
}

fn powf<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    float: Float,
    integer_exponent: bool,
) -> Root {
    debug_assert_eq!(args.len(), 2, "float power intrinsic takes two arguments");
    let sig = ctx.sig([Type::Float(float); 2], Type::Float(float));
    let class = float.class(ctx);
    let method = ctx.new_methodref(class, "Pow", sig, MethodKind::Static, []);
    let base = handle_operand(&args[0].node, ctx);
    let exponent = handle_operand(&args[1].node, ctx);
    let exponent = integer_exponent
        .then(|| ctx.float_cast(exponent, float, true))
        .unwrap_or(exponent);
    place_set(
        destination,
        ctx.call(method, &[base, exponent], IsPure::NOT),
        ctx,
    )
}

fn round<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    float: Float,
) -> Root {
    let rounding = ClassRef::midpoint_rounding(ctx);
    let sig = ctx.sig(
        [Type::Float(float), Type::ClassRef(rounding)],
        Type::Float(float),
    );
    let math = match float {
        Float::F32 => ClassRef::mathf(ctx),
        Float::F64 => ClassRef::math(ctx),
        _ => unreachable!("round only supports f32/f64"),
    };
    let method = ctx.new_methodref(math, "Round", sig, MethodKind::Static, []);
    let value = handle_operand(&args[0].node, ctx);
    let midpoint = ctx.alloc_node(1_i32);
    let midpoint = ctx.transmute_on_stack(Type::Int(Int::I32), Type::ClassRef(rounding), midpoint);
    place_set(
        destination,
        ctx.call(method, &[value, midpoint], IsPure::NOT),
        ctx,
    )
}

macro_rules! float_intrinsic {
    ($name:ident, $helper:ident, $float:ident $(, $arg:expr)*) => {
        pub fn $name<'tcx>(
            args: &[Spanned<Operand<'tcx>>],
            destination: &Place<'tcx>,
            ctx: &mut MethodCompileCtx<'tcx, '_>,
        ) -> Root {
            $helper(args, destination, ctx, Float::$float $(, $arg)*)
        }
    };
}

float_intrinsic!(fmaf32, fmaf, F32);
float_intrinsic!(fmaf16, fmaf, F16);
float_intrinsic!(fmaf64, fmaf, F64);
float_intrinsic!(powif32, powf, F32, true);
float_intrinsic!(powif64, powf, F64, true);
float_intrinsic!(powf32, powf, F32, false);
float_intrinsic!(powf64, powf, F64, false);
float_intrinsic!(roundf32, round, F32);
float_intrinsic!(roundf64, round, F64);
