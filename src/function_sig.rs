use crate::codegen_error::CodegenError;
use cilly::{FnSig, Type};
use crate::call_info::CallInfo;
use crate::fn_ctx::MethodCompileCtx;
use crate::r#type::get_type;
use rustc_middle::ty::{Instance, Ty, TyCtxt};

/// Creates a `FnSig` from ` `. May not match the result of `sig_from_instance_`!
/// Use ONLY for function pointers!
pub fn from_poly_sig<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    sig: rustc_middle::ty::FnSigTys<TyCtxt<'tcx>>,
) -> FnSig {
    let output = get_type(ctx.monomorphize(sig.output()), ctx);
    let inputs: Box<[Type]> = sig
        .inputs()
        .iter()
        .map(|input| get_type(ctx.monomorphize(*input), ctx))
        .collect();
    FnSig::new(inputs, output)
}
/// Returns the signature of function behind `function`.
///
/// Delegates to [`CallInfo::sig_from_instance_`] so that the accepted ABI set stays in lockstep
/// with the call path (the Rust family, `C`, `Custom`, and the x86 variants). This historically had
/// its own stricter `Rust`/`C`-only checker, which made drop-glue and managed-interop calls panic
/// on ABIs that an ordinary call would accept.
pub fn sig_from_instance_<'tcx>(
    function: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Result<FnSig, CodegenError> {
    Ok(CallInfo::sig_from_instance_(function, ctx).sig().clone())
}

/// Checks if this function is variadic.
#[must_use]
pub fn is_fn_variadic<'tcx>(ty: Ty<'tcx>, tcx: TyCtxt<'tcx>) -> bool {
    ty.fn_sig(tcx).skip_binder().c_variadic()
}
