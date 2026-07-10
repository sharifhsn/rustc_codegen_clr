use crate::assembly::MethodCompileCtx;
use cilly::Interned;
use crate::operand::constant::load_const_value;
use crate::place::place_set;
use rustc_middle::{mir::Place, ty::Instance};
use rustc_span::Span;

type Root = Interned<cilly::ir::CILRoot>;

/// Lowers the `type_id` intrinsic to the real 128-bit `TypeId` constant.
///
/// On current nightlies `core::intrinsics::type_id` is `#[rustc_comptime]`: rustc const-evaluates
/// `const { type_id::<T>() }` to the genuine 128-bit id *before* codegen, so this runtime handler is
/// normally unreachable (the value arrives through the constant/static path). Should a rustc that
/// emits a *runtime* `type_id` call ever surface, we const-evaluate the intrinsic instance here to
/// obtain that same real 128-bit id (mirroring the `type_name` handler) — never the old 32-bit
/// `Object::GetHashCode` shortcut, which aliases distinct types and silently breaks `Any`/`TypeId`.
pub fn type_id<'tcx>(
    destination: &Place<'tcx>,
    call_instance: Instance<'tcx>,
    span: Span,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let const_val = ctx
        .tcx()
        .const_eval_instance(
            rustc_middle::ty::TypingEnv::fully_monomorphized(),
            call_instance,
            span,
        )
        .expect("the `type_id` intrinsic could not be const-evaluated");
    let ty = ctx.monomorphize(destination.ty(ctx.body(), ctx.tcx()).ty);
    let value = load_const_value(const_val, ty, ctx);
    place_set(destination, value, ctx)
}
