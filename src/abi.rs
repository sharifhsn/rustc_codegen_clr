//! Rust ABI planning shared by definitions, calls, and function-pointer adapters.

use crate::fn_ctx::MethodCompileCtx;
use crate::r#type::{GetTypeExt, get_type};
use cilly::{CILNode, FieldDesc, FnSig, Interned, Type};
use rustc_abi::{CanonAbi, ExternAbi as TargetAbi};
use rustc_middle::mir::Operand;
use rustc_middle::ty::{Instance, List, PseudoCanonicalInput, Ty, TyKind};
use rustc_span::{Span, Spanned};
use rustc_target::callconv::{ArgAbi, FnAbi, PassMode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbiArgumentKind {
    Value,
    Ignored,
    ClosureReceiver,
    CallerLocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AbiArgument {
    cil_type: Type,
    kind: AbiArgumentKind,
}

/// One source of truth for a Rust callable's ABI-facing CIL shape.
///
/// `signature` retains the backend's positional `Type::Void` representation for ignored Rust ZST
/// arguments. `arguments` records *why* each slot exists, so adapters can omit a proven closure
/// receiver without guessing from the type and accidentally dropping an ordinary ZST parameter.
#[derive(Clone, Debug)]
pub struct AbiPlan {
    signature: FnSig,
    arguments: Box<[AbiArgument]>,
    rust_call: bool,
    c_variadic: bool,
    fixed_count: usize,
}

impl AbiPlan {
    #[must_use]
    pub fn from_instance<'tcx>(
        function: Instance<'tcx>,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Self {
        let fn_abi = ctx.tcx().fn_abi_of_instance(PseudoCanonicalInput {
            typing_env: rustc_middle::ty::TypingEnv::fully_monomorphized(),
            value: (function, List::empty()),
        });
        let fn_abi = match fn_abi {
            Ok(abi) => abi,
            Err(error) => {
                rustc_span::bug!("`fn_abi_of_instance` failed for {function:?}: {error:?}")
            }
        };
        let fn_ty = function.ty(
            ctx.tcx(),
            rustc_middle::ty::TypingEnv::fully_monomorphized(),
        );
        let internal_abi = match fn_ty.kind() {
            TyKind::FnDef(_, _) => fn_ty.fn_sig(ctx.tcx()).abi(),
            TyKind::Closure(_, args) => args.as_closure().sig().abi(),
            TyKind::Coroutine(_, _) => TargetAbi::Rust,
            _ => rustc_span::bug!(
                "cannot derive ABI plan for instance type {fn_ty} ({:?})",
                fn_ty.kind()
            ),
        };
        // `Instance::resolve_closure` usually returns a callable shim whose `function.ty()` is a
        // `FnDef`, not the source `Closure` type. Use rustc's instance identity instead of trying
        // to infer the receiver from a leading `Void` ABI slot (which would collide with an
        // ordinary leading ZST argument).
        let closure_instance = ctx.tcx().is_closure_like(function.def_id())
            || matches!(
                function.def,
                rustc_middle::ty::InstanceKind::Shim(
                    rustc_middle::ty::ShimKind::ClosureOnce { .. },
                )
            );
        let first_arg_is_closure = fn_abi
            .args
            .first()
            .is_some_and(|argument| matches!(argument.layout.ty.kind(), TyKind::Closure(..)));
        let closure_receiver = (closure_instance && first_arg_is_closure).then_some(0);
        let caller_location = if function.def.requires_caller_location(ctx.tcx()) {
            Some(fn_abi.args.len().checked_sub(1).unwrap_or_else(|| {
                panic!(
                    "track_caller FnAbi for {function:?} has no implicit caller-location argument"
                )
            }))
        } else {
            None
        };
        Self::from_fn_abi(fn_abi, internal_abi, closure_receiver, caller_location, ctx)
    }

    #[must_use]
    pub fn from_fn_ptr<'tcx>(
        signature: rustc_middle::ty::PolyFnSig<'tcx>,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Self {
        // MIR retains the generic fn-pointer type from the source body. Codegen runs on a
        // monomorphic `Instance`, so substitute that instance before asking rustc for layout;
        // querying `fn_abi_of_fn_ptr` with (for example) `fn() -> T` returns `TooGeneric` even
        // though this concrete codegen instance may have `T = u32`.
        let signature = ctx.monomorphize(signature);
        let fn_abi = ctx.tcx().fn_abi_of_fn_ptr(PseudoCanonicalInput {
            typing_env: rustc_middle::ty::TypingEnv::fully_monomorphized(),
            value: (signature, List::empty()),
        });
        let fn_abi = match fn_abi {
            Ok(abi) => abi,
            Err(error) => ctx.tcx().dcx().span_fatal(
                ctx.span(),
                format!(
                    "UnsupportedFeature(fn_pointer_abi): rustc could not compute the physical ABI \
                     for monomorphized function-pointer signature {signature:?}: {error:?}"
                ),
            ),
        };
        Self::from_fn_abi(fn_abi, signature.skip_binder().abi(), None, None, ctx)
    }

    /// Plan an actual indirect call. CIL `calli` needs a call-site vararg sentinel plus the
    /// physical ABI of every extra argument; the current IR/PE pipeline represents only fixed
    /// `FnSig`s. Reject this shape from rustc's monomorphized signature before allocating any CIL
    /// types. Merely storing or comparing a C-variadic function pointer remains valid and continues
    /// to use `from_fn_ptr`'s fixed-prefix representation.
    #[must_use]
    pub fn from_indirect_call<'tcx>(
        signature: rustc_middle::ty::PolyFnSig<'tcx>,
        span: Span,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Self {
        let signature = ctx.monomorphize(signature);
        if signature.skip_binder().c_variadic() {
            ctx.tcx().dcx().span_fatal(
                span,
                "UnsupportedFeature(indirect_c_variadic_fn_pointer): calling a C-variadic \
                 function pointer requires a CIL vararg call-site signature, which this backend \
                 does not yet represent",
            );
        }
        Self::from_fn_ptr(signature, ctx)
    }

    #[must_use]
    pub fn from_fn_ptr_ty<'tcx>(ty: Ty<'tcx>, ctx: &mut MethodCompileCtx<'tcx, '_>) -> Self {
        let ty = ctx.monomorphize(ty);
        let TyKind::FnPtr(signature, header) = ty.kind() else {
            rustc_span::bug!("expected fn-pointer type, got {ty:?}")
        };
        Self::from_fn_ptr(signature.with(*header), ctx)
    }

    fn from_fn_abi<'tcx>(
        fn_abi: &FnAbi<'tcx, Ty<'tcx>>,
        internal_abi: TargetAbi,
        closure_receiver: Option<usize>,
        caller_location: Option<usize>,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Self {
        validate_calling_convention(fn_abi.conv);
        let mut arguments: Vec<_> = fn_abi
            .args
            .iter()
            .enumerate()
            .map(|(index, argument)| AbiArgument {
                cil_type: get_type(argument.layout.ty, ctx),
                kind: argument_kind(argument, index, closure_receiver, caller_location),
            })
            .collect();
        if let Some(index) = caller_location {
            assert_eq!(
                index + 1,
                arguments.len(),
                "caller-location ABI slot is not trailing"
            );
            arguments[index].kind = AbiArgumentKind::CallerLocation;
        }
        let output = get_type(fn_abi.ret.layout.ty, ctx);
        let inputs: Vec<_> = arguments.iter().map(|slot| slot.cil_type).collect();
        let signature = FnSig::new(inputs, output);
        Self {
            signature,
            arguments: arguments.into_boxed_slice(),
            rust_call: internal_abi == TargetAbi::RustCall,
            c_variadic: fn_abi.c_variadic,
            fixed_count: fn_abi.fixed_count as usize,
        }
    }

    #[must_use]
    pub const fn signature(&self) -> &FnSig {
        &self.signature
    }

    #[must_use]
    pub const fn is_rust_call(&self) -> bool {
        self.rust_call
    }

    #[must_use]
    pub const fn is_c_variadic(&self) -> bool {
        self.c_variadic
    }

    #[must_use]
    pub fn caller_location_slot(&self) -> Option<usize> {
        self.arguments
            .iter()
            .position(|argument| argument.kind == AbiArgumentKind::CallerLocation)
    }

    #[must_use]
    pub fn closure_receiver_slot(&self) -> Option<usize> {
        self.arguments
            .iter()
            .position(|argument| argument.kind == AbiArgumentKind::ClosureReceiver)
    }

    /// Physical slots in this callable which are absent from `target`.
    ///
    /// The only supported shape-changing coercion is captureless closure -> bare fn pointer, which
    /// removes the proven closure receiver. Every retained slot must match exactly, including
    /// ordinary ZST/`Void` parameters.
    #[must_use]
    pub fn ignored_slots_for_fn_pointer(&self, target: &Self) -> Vec<usize> {
        if self.signature == target.signature {
            return vec![];
        }
        let Some(receiver) = self.closure_receiver_slot() else {
            panic!(
                "function-pointer ABI mismatch has no proven closure receiver: real={:?} target={:?}",
                self.signature, target.signature
            );
        };
        assert_eq!(
            self.arguments[receiver].cil_type,
            Type::Void,
            "only an ignored/ZST closure receiver may be omitted from a bare fn pointer"
        );
        let retained: Vec<_> = self
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| (index != receiver).then_some(argument.cil_type))
            .collect();
        assert_eq!(retained, target.signature.inputs());
        assert_eq!(self.signature.output(), target.signature.output());
        vec![receiver]
    }

    /// Signature exposed after removing a proven captureless-closure receiver.
    #[must_use]
    pub fn bare_fn_pointer_signature(&self) -> (FnSig, Vec<usize>) {
        let Some(receiver) = self.closure_receiver_slot() else {
            return (self.signature.clone(), vec![]);
        };
        assert_eq!(self.arguments[receiver].cil_type, Type::Void);
        let inputs: Vec<_> = self
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| (index != receiver).then_some(argument.cil_type))
            .collect();
        (FnSig::new(inputs, *self.signature.output()), vec![receiver])
    }

    /// Lower MIR call operands in their physical ABI order, including RustCall tuple spreading and
    /// the implicit track-caller location.
    pub fn lower_call_args<'tcx>(
        &self,
        args: &[Spanned<Operand<'tcx>>],
        source_info: rustc_middle::mir::SourceInfo,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Vec<Interned<CILNode>> {
        let mut lowered = Vec::with_capacity(self.arguments.len());
        if self.rust_call {
            let (tuple, prefix) = args
                .split_last()
                .expect("rust-call requires a trailing tuple operand");
            lowered.extend(
                prefix
                    .iter()
                    .map(|operand| crate::operand::handle_operand(&operand.node, ctx)),
            );
            let tuple_ty = ctx.monomorphize(tuple.node.ty(ctx.body(), ctx.tcx()));
            let TyKind::Tuple(elements) = tuple_ty.kind() else {
                panic!("rust-call trailing operand is not a tuple: {tuple_ty:?}")
            };
            let tuple_type = ctx.type_from_cache(tuple_ty);
            for (index, element) in elements.iter().enumerate() {
                let element = ctx.monomorphize(element);
                let element_type = ctx.type_from_cache(element);
                if element_type == Type::Void {
                    lowered.push(ctx.uninit_val(Type::Void));
                    continue;
                }
                let field = FieldDesc::new(
                    tuple_type
                        .as_class_ref()
                        .expect("non-ZST RustCall tuple must be a class"),
                    ctx.alloc_string(format!("Item{}", index + 1)),
                    element_type,
                );
                let tuple = crate::operand::handle_operand(&tuple.node, ctx);
                lowered.push(ctx.ld_field(tuple, field));
            }
        } else {
            lowered.extend(
                args.iter()
                    .map(|operand| crate::operand::handle_operand(&operand.node, ctx)),
            );
        }
        self.append_caller_location(&mut lowered, source_info, ctx);
        if self.c_variadic {
            assert!(
                lowered.len() >= self.fixed_count,
                "variadic call has fewer arguments than its fixed ABI prefix"
            );
        } else {
            assert_eq!(
                lowered.len(),
                self.signature.inputs().len(),
                "MIR operands do not match the physical ABI plan"
            );
        }
        lowered
    }

    pub fn append_caller_location<'tcx>(
        &self,
        lowered: &mut Vec<Interned<CILNode>>,
        source_info: rustc_middle::mir::SourceInfo,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) {
        if let Some(slot) = self.caller_location_slot() {
            assert_eq!(lowered.len(), slot, "caller-location slot is not next");
            lowered.push(crate::terminator::get_caller_location(ctx, source_info));
        }
    }

    /// Maps MIR argument names onto the physical signature without re-deriving hidden slots.
    #[must_use]
    pub fn physical_argument_names<'tcx>(
        &self,
        mut mir_names: Vec<Option<cilly::Interned<cilly::IString>>>,
        body: &rustc_middle::mir::Body<'tcx>,
        ctx: &MethodCompileCtx<'tcx, '_>,
    ) -> Vec<Option<cilly::Interned<cilly::IString>>> {
        if let Some(spread) = body.spread_arg {
            let spread_index = spread.as_usize() - 1;
            let spread_ty = ctx.monomorphize(body.local_decls[spread].ty);
            let TyKind::Tuple(elements) = spread_ty.kind() else {
                panic!("MIR spread_arg is not a tuple: {spread_ty:?}")
            };
            mir_names.truncate(spread_index);
            mir_names.resize(spread_index + elements.len(), None);
        }
        let ordinary = self
            .arguments
            .len()
            .checked_sub(usize::from(self.caller_location_slot().is_some()))
            .expect("invalid caller-location slot count");
        mir_names.resize(ordinary, None);
        if self.caller_location_slot().is_some() {
            mir_names.push(None);
        }
        assert_eq!(mir_names.len(), self.arguments.len());
        mir_names
    }

    /// Physical CIL argument slots used to rebuild MIR's RustCall tuple local.
    #[must_use]
    pub fn rust_call_tuple_fields<'tcx>(
        &self,
        body: &rustc_middle::mir::Body<'tcx>,
        ctx: &MethodCompileCtx<'tcx, '_>,
    ) -> Vec<(usize, Ty<'tcx>)> {
        let Some(spread) = body.spread_arg else {
            assert!(!self.rust_call, "RustCall ABI is missing MIR spread_arg");
            return vec![];
        };
        assert!(self.rust_call, "MIR spread_arg used by a non-RustCall ABI");
        let tuple = ctx.monomorphize(body.local_decls[spread].ty);
        let TyKind::Tuple(elements) = tuple.kind() else {
            panic!("MIR spread_arg is not a tuple: {tuple:?}")
        };
        let first_slot = spread.as_usize() - 1;
        let result: Vec<_> = elements
            .iter()
            .enumerate()
            .map(|(field, ty)| (first_slot + field, ctx.monomorphize(ty)))
            .collect();
        let trailing = usize::from(self.caller_location_slot().is_some());
        assert_eq!(
            first_slot + result.len() + trailing,
            self.arguments.len(),
            "RustCall tuple fields do not cover the physical ABI"
        );
        result
    }
}

fn argument_kind<'tcx>(
    argument: &ArgAbi<'tcx, Ty<'tcx>>,
    index: usize,
    closure_receiver: Option<usize>,
    caller_location: Option<usize>,
) -> AbiArgumentKind {
    if closure_receiver == Some(index) {
        AbiArgumentKind::ClosureReceiver
    } else if caller_location == Some(index) {
        AbiArgumentKind::CallerLocation
    } else if matches!(argument.mode, PassMode::Ignore) {
        AbiArgumentKind::Ignored
    } else {
        AbiArgumentKind::Value
    }
}

fn validate_calling_convention(conv: CanonAbi) {
    #[allow(clippy::match_same_arms)]
    match conv {
        _ if conv.is_rustic_abi() => (),
        CanonAbi::C | CanonAbi::Custom | CanonAbi::X86(_) => (),
        _ => panic!("calling convention {conv:?} is unsupported by the CIL backend"),
    }
}

#[cfg(test)]
mod tests {
    use super::{AbiArgument, AbiArgumentKind, AbiPlan};
    use cilly::{FnSig, Type};

    fn plan(inputs: &[Type], kinds: &[AbiArgumentKind]) -> AbiPlan {
        AbiPlan {
            signature: FnSig::new(inputs.to_vec(), Type::Void),
            arguments: inputs
                .iter()
                .zip(kinds)
                .map(|(cil_type, kind)| AbiArgument {
                    cil_type: *cil_type,
                    kind: *kind,
                })
                .collect(),
            rust_call: false,
            c_variadic: false,
            fixed_count: inputs.len(),
        }
    }

    #[test]
    fn closure_adaptation_drops_only_the_verified_receiver() {
        let real = plan(
            &[Type::Void, Type::Void, Type::Int(cilly::Int::U32)],
            &[
                AbiArgumentKind::ClosureReceiver,
                AbiArgumentKind::Ignored,
                AbiArgumentKind::Value,
            ],
        );
        let target = plan(
            &[Type::Void, Type::Int(cilly::Int::U32)],
            &[AbiArgumentKind::Ignored, AbiArgumentKind::Value],
        );
        assert_eq!(real.ignored_slots_for_fn_pointer(&target), [0]);
    }

    #[test]
    #[should_panic(expected = "no proven closure receiver")]
    fn ordinary_leading_zst_is_not_guessed_as_an_adapter_slot() {
        let real = plan(
            &[Type::Void, Type::Int(cilly::Int::U32)],
            &[AbiArgumentKind::Ignored, AbiArgumentKind::Value],
        );
        let target = plan(&[Type::Int(cilly::Int::U32)], &[AbiArgumentKind::Value]);
        let _ = real.ignored_slots_for_fn_pointer(&target);
    }
}
