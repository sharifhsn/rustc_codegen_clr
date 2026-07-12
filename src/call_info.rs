use crate::fn_ctx::MethodCompileCtx;
use crate::r#type::get_type;
use cilly::FnSig;
use rustc_abi::{CanonAbi, ExternAbi as TargetAbi};
use rustc_middle::ty::{Instance, List, PseudoCanonicalInput, TyKind};
/// A resolved function signature plus the ABI-level call-site adjustments the caller must apply.
/// `sig` is the plain CIL signature; `split_last_tuple` is set when the source ABI is `RustCall`
/// (used for closures), whose last argument is a tuple that Rust's calling convention spreads
/// across individual argument registers/slots rather than passing as one aggregate — the CIL
/// call site must likewise unpack that tuple into separate arguments matching `sig`.
pub struct CallInfo {
    sig: FnSig,
    split_last_tuple: bool,
}
impl CallInfo {
    /// Returns the signature of function behind `function`.
    pub fn sig_from_instance_<'tcx>(
        function: Instance<'tcx>,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Self {
        let fn_abi = ctx.tcx().fn_abi_of_instance(PseudoCanonicalInput {
            typing_env: rustc_middle::ty::TypingEnv::fully_monomorphized(),
            value: (function, List::empty()),
        });
        let fn_abi = match fn_abi {
            Ok(abi) => abi,
            // Bind and surface the layout/ABI error instead of swallowing it — `fn_abi_of_instance`
            // only fails on a layout-computation error (`FnAbiError::Layout`), which should be a
            // loud, informative compiler-internal abort, not a bare `todo!()`.
            Err(error) => {
                rustc_middle::bug!("`fn_abi_of_instance` failed for {function:?}: {error:?}")
            }
        };
        let conv = fn_abi.conv;
        // CIL is calling-convention-agnostic: every call lowers to a CIL `call`/`callvirt` using
        // the computed signature, so the *native* calling convention has no bearing on lowering.
        // Accept the conventions that legitimately appear in core/alloc/std and compiler_builtins:
        //  - the Rust family (`Rust`, plus `RustCold`/`RustPreserveNone`/`RustTail`, used for
        //    `#[cold]`/tail paths) — `is_rustic_abi()`,
        //  - `C` and the x86 variants,
        //  - `Custom`: compiler_builtins' naked/asm-only intrinsics (rustc "does not know how to
        //    call or define" them). Their signature is well-defined; their asm body can't be
        //    lowered to CIL but is handled by per-method panic recovery (a throwing stub) and, for
        //    the ones that matter (`mem*`), replaced by the linker's builtin implementations.
        // Genuinely exotic conventions (Swift, 32-bit Arm, GPU-kernel, interrupt) still fail loud,
        // as they must not silently appear for the .NET/CIL targets.
        #[allow(clippy::match_same_arms)]
        match conv {
            _ if conv.is_rustic_abi() => (),
            CanonAbi::C | CanonAbi::Custom => (),
            CanonAbi::X86(_) => (),
            _ => panic!("ERROR:calling using convention {conv:?} is not supported!"),
        }
        //assert!(!fn_abi.c_variadic);
        let ret = get_type(fn_abi.ret.layout.ty, ctx);
        let mut args = Vec::with_capacity(fn_abi.args.len());

        for arg in &fn_abi.args {
            args.push(get_type(arg.layout.ty, ctx));
        }
        // There are 2 ABI enums for some reasons(they differ in what memebers they have)
        let fn_ty = function.ty(
            ctx.tcx(),
            rustc_middle::ty::TypingEnv::fully_monomorphized(),
        );
        let internal_abi = match fn_ty.kind() {
            TyKind::FnDef(_, _) => fn_ty.fn_sig(ctx.tcx()).abi(),
            TyKind::Closure(_, args) => args.as_closure().sig().abi(),
            TyKind::Coroutine(_, _) => TargetAbi::Rust, // TODO: this assumes all coroutines have the ABI Rust. This *should* be correct.
            // Defensive catch-all for a genuinely novel fn-type kind. NOTE: this is NOT the firing
            // site for async closures (`CoroutineClosure`) — their `get_type` of the FnMut-shim's
            // self-arg panics first in crate::type (the real async-closure
            // wall), so this arm only ever sees an unexpected `fn_ty.kind()`.
            _ => todo!(
                "Cannot derive a CIL signature for instance type {fn_ty} (kind: {:?}). \
                 This function-type shape is unsupported on the .NET target.",
                fn_ty.kind()
            ),
        };
        // Only those ABIs are supported
        let split_last_tuple = match internal_abi {
            TargetAbi::C { unwind: _ }
            | TargetAbi::Cdecl { unwind: _ }
            | TargetAbi::Rust
            | TargetAbi::RustCold
            // `custom` ABI (compiler_builtins naked/asm intrinsics): not the tupled `rust_call`
            // convention, so arguments are passed straight through (see the `conv` match above).
            | TargetAbi::Custom
            | TargetAbi::Unadjusted
            | TargetAbi::SysV64 { unwind: _ } => false,

            TargetAbi::RustCall => true, /*Err(CodegenError::FunctionABIUnsuported(
            "\"rust_call\" ABI, used for things like clsoures, is not supported yet!",
            ))?,*/
            // `split_last_tuple` only governs the tupled `rust_call` argument-splitting; every
            // other ABI passes arguments straight through, so `false` is correct for all of them
            // (`extern "system"/"win64"/"efiapi"/"vectorcall"`, the rustic `RustPreserveNone`/
            // `RustTail`, etc.). This makes `internal_abi` AGREE with the `conv` gate above, which
            // already ran first (lines 43-48) and `panic!`s loudly on genuinely-exotic conventions
            // (Swift/Arm/GpuKernel/Interrupt) — so no exotic ABI can slip through here.
            _ => false,
        };
        let mut sig = FnSig::new(args, ret);
        if fn_abi.c_variadic {
            let remaining = fn_abi.args[(fn_abi.fixed_count as usize)..]
                .iter()
                .map(|ty| get_type(ctx.monomorphize(ty.layout.ty), ctx));
            let mut inputs = sig.inputs().to_vec();
            inputs.extend(remaining);
            sig.set_inputs(inputs);
        }
        Self {
            sig,
            split_last_tuple,
        }
    }

    pub fn sig(&self) -> &FnSig {
        &self.sig
    }

    /// When true, the call site must split `sig`'s last (tuple) argument into its individual
    /// fields — see the `CallInfo` doc comment for why (`RustCall`/closure ABI).
    pub fn split_last_tuple(&self) -> bool {
        self.split_last_tuple
    }
}
