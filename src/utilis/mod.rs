use crate::r#type::escape_field_name;
use rustc_abi::VariantIdx;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::{
    ConstKind, GenericArg, Instance, List, PseudoCanonicalInput, Ty, TyCtxt, TyKind,
};

pub mod adt;
/// The common prefix of `rustc_clr_interop_managed_ctor{0,1,2,3}_` — still needed (unlike every other
/// magic-fn name constant this module used to export) because `call_ctor` parses the arity digit back
/// out of the mangled call-site symbol via [`crate::terminator::call::argc_from_fn_name`]. Recognizing
/// *which* fn is magic no longer goes through this constant — see [`classify_magic_fn`].
pub const CTOR_FN_NAME: &str = "rustc_clr_interop_managed_ctor";
/// See [`CTOR_FN_NAME`] — same reason (`call_managed`'s arity parsing), same caveat.
pub const MANAGED_CALL_FN_NAME: &str = "rustc_clr_interop_managed_call";
/// See [`CTOR_FN_NAME`] — same reason (`callvirt_managed`'s arity parsing), same caveat.
pub const MANAGED_CALL_VIRT_FN_NAME: &str = "rustc_clr_interop_managed_call_virt";

/// The canonical, exhaustive classification of every "magic" interop fn the backend recognizes and
/// substitutes real CIL for (see [`classify_magic_fn`]). One variant per *dispatch shape* in
/// `src/terminator/call.rs::call_inner`, not one per concrete arity-ladder function — e.g. `Ctor`
/// covers `rustc_clr_interop_managed_ctor{0,1,2,3}_` uniformly, since the callee (`call_ctor`) already
/// reads the concrete arity back out of the mangled name itself via `argc_from_fn_name`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MagicFn {
    /// `rustc_clr_interop_managed_ctor{0..=3}_` → `newobj`.
    Ctor,
    /// `rustc_clr_interop_managed_call{0..=4}_` → `call` (static or instance).
    ManagedCall,
    /// `rustc_clr_interop_managed_call_virt{0..=3}_` → `callvirt`.
    ManagedCallVirt,
    /// `rustc_clr_interop_managed_ld_len` → `ldlen`.
    LdLen,
    /// `rustc_clr_interop_managed_ld_null` → `ldnull`.
    LdNull,
    /// `rustc_clr_interop_managed_checked_cast` → `castclass`.
    CheckedCast,
    /// `rustc_clr_interop_managed_is_inst` → `isinst`.
    IsInst,
    /// `rustc_clr_interop_managed_ld_elem_ref` → `ldelem.ref`.
    LdElemRef,
    /// `rustc_clr_interop_managed_new_arr` → `newarr`.
    NewArr,
    /// `rustc_clr_interop_managed_set_elem` → `stelem`.
    SetElem,
    /// `rustc_clr_interop_box` → `box` (a value type into `System.Object`).
    Box,
    /// `rustc_clr_interop_managed_box_new` → CLR-box + GCHandle root, returned as an opaque token.
    ManagedBoxNew,
    /// `rustc_clr_interop_managed_box_take` → recover/unbox the rooted value and free its GCHandle.
    ManagedBoxTake,
    /// `rustc_clr_interop_try_catch` → a CIL try/catch region catching any .NET exception.
    TryCatch,
    /// `rustc_clr_interop_generic_call{0..=4}` (WF-9) — a method on a generic .NET instantiation.
    GenericCall,
    /// `rustc_clr_interop_generic_ctor{0..=2}` (WF-9) — `newobj` on a generic .NET instantiation.
    GenericCtor,
    /// `rustc_clr_interop_generic_method_call{0..=5}` (WF-9) — a generic *method* (`!!N`) call.
    GenericMethodCall,
    /// `rustc_clr_interop_throw` → `throw` (a managed exception a .NET caller can `catch`, distinct
    /// from a Rust `panic!`).
    Throw,
    /// `rustc_clr_interop_delegate` — wraps a capture-less fn pointer into a managed delegate.
    Delegate,
    /// `rustc_clr_interop_delegate_closure` — wraps a **capturing** closure into a managed delegate.
    DelegateClosure,
}

/// Classifies `def_id` as one of the interop "magic" fns, or `None` for an ordinary function.
///
/// This is the **single canonical list** every call site now shares — the codegen-skip gate
/// (`assembly::add_fn`), the CIL-substitution dispatch (`terminator::call::call_inner`), and the
/// unwind-boundary exception guard (`basic_block::handler_for_block`) all call this instead of each
/// keeping their own hand-copied name list. There used to be three: the skip-gate's list had already
/// drifted out of sync with the dispatch list (missing 9 of 18 families — those fns' dummy
/// `core::intrinsics::abort()` bodies were harmlessly but needlessly being monomorphized and codegen'd,
/// since their call sites still dispatched correctly), which is exactly the failure mode duplicated
/// lists invite.
///
/// This also matches differently than the old mechanism did: it compares the **exact** source
/// identifier from `tcx.def_path_str(def_id)` (the item's declaration path — independent of mangling
/// and monomorphization) against a fixed set of literal names, instead of substring-searching the
/// mangled *symbol name* of the call site. Two consequences: (1) there is no substring-collision or
/// check-ordering hazard — matching a mangled symbol name required careful ordering (`_delegate_closure`
/// contains `_delegate`; `_generic_method_call` had to be checked before `_generic_call`) that an exact
/// match doesn't need at all; (2) an ordinary user function can never be accidentally misclassified as
/// magic just because its mangled name happens to contain one of these strings as a substring.
pub fn classify_magic_fn(tcx: TyCtxt, def_id: DefId) -> Option<MagicFn> {
    let path = tcx.def_path_str(def_id);
    let name = path.rsplit("::").next().unwrap_or(path.as_str());
    // DTO primary constructors are schema-arity generated and may legitimately exceed the small
    // hand-written ctor0..ctor3 convenience ladder. Keep the exact identifier boundary while
    // accepting any decimal arity the call decoder can validate against generics/arguments.
    if name
        .strip_prefix(CTOR_FN_NAME)
        .and_then(|suffix| suffix.strip_suffix('_'))
        .is_some_and(|arity| !arity.is_empty() && arity.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Some(MagicFn::Ctor);
    }
    Some(match name {
        "rustc_clr_interop_managed_call_virt0_"
        | "rustc_clr_interop_managed_call_virt1_"
        | "rustc_clr_interop_managed_call_virt2_"
        | "rustc_clr_interop_managed_call_virt3_" => MagicFn::ManagedCallVirt,
        "rustc_clr_interop_managed_call0_"
        | "rustc_clr_interop_managed_call1_"
        | "rustc_clr_interop_managed_call2_"
        | "rustc_clr_interop_managed_call3_"
        | "rustc_clr_interop_managed_call4_" => MagicFn::ManagedCall,
        "rustc_clr_interop_managed_ld_len" => MagicFn::LdLen,
        "rustc_clr_interop_managed_ld_null" => MagicFn::LdNull,
        "rustc_clr_interop_managed_checked_cast" => MagicFn::CheckedCast,
        "rustc_clr_interop_managed_is_inst" => MagicFn::IsInst,
        "rustc_clr_interop_managed_ld_elem_ref" => MagicFn::LdElemRef,
        "rustc_clr_interop_managed_new_arr" => MagicFn::NewArr,
        "rustc_clr_interop_managed_set_elem" => MagicFn::SetElem,
        "rustc_clr_interop_box" => MagicFn::Box,
        "rustc_clr_interop_managed_box_new" => MagicFn::ManagedBoxNew,
        "rustc_clr_interop_managed_box_take" => MagicFn::ManagedBoxTake,
        "rustc_clr_interop_try_catch" => MagicFn::TryCatch,
        "rustc_clr_interop_throw" => MagicFn::Throw,
        "rustc_clr_interop_generic_call0"
        | "rustc_clr_interop_generic_call1"
        | "rustc_clr_interop_generic_call2"
        | "rustc_clr_interop_generic_call3"
        | "rustc_clr_interop_generic_call4" => MagicFn::GenericCall,
        "rustc_clr_interop_generic_ctor0"
        | "rustc_clr_interop_generic_ctor1"
        | "rustc_clr_interop_generic_ctor2" => MagicFn::GenericCtor,
        "rustc_clr_interop_generic_method_call0"
        | "rustc_clr_interop_generic_method_call1"
        | "rustc_clr_interop_generic_method_call2"
        | "rustc_clr_interop_generic_method_call3"
        | "rustc_clr_interop_generic_method_call4"
        | "rustc_clr_interop_generic_method_call5" => MagicFn::GenericMethodCall,
        "rustc_clr_interop_delegate" => MagicFn::Delegate,
        "rustc_clr_interop_delegate_closure" => MagicFn::DelegateClosure,
        _ => return None,
    })
}

// WARNING: this is *wrong*: For some reason, `Instance::try_resolve` should not operate on structs(why?), and this just silences the newly introduced warning.
pub fn instance_try_resolve<'tcx>(
    adt: DefId,
    tcx: TyCtxt<'tcx>,
    gargs: &'tcx List<GenericArg<'tcx>>,
) -> Instance<'tcx> {
    tcx.resolve_instance_raw(PseudoCanonicalInput {
        typing_env: rustc_middle::ty::TypingEnv::fully_monomorphized(),
        value: (adt, gargs),
    })
    .unwrap()
    .unwrap()
}

/// Gets the name of a field with index `idx`
pub fn field_name(ty: Ty, idx: u32) -> crate::IString {
    match ty.kind() {
        TyKind::Adt(adt_def, _subst) => {
            let field_def = adt_def
                .all_fields()
                .nth(idx as usize)
                .expect("Field index out of range.");
            escape_field_name(&field_def.name.to_string()).into()
        }
        TyKind::Tuple(_) => format!("Item{}", idx + 1).into(),
        _ => todo!("Can't yet get fields of typr {ty:?}"),
    }
}
/// Gets the name of a enum variant with index `idx`
pub fn variant_name(ty: Ty, idx: u32) -> crate::IString {
    match ty.kind() {
        TyKind::Adt(adt_def, _subst) => {
            let variant_def = &adt_def.variants()[VariantIdx::from_u32(idx)];
            variant_def.name.to_string().into()
        }
        _ => todo!("Can't yet get fields of typr {ty:?}"),
    }
}

/// Converts a generic argument to a boolean, and panics if it could not.
pub fn garg_to_bool<'tcx>(garg: GenericArg<'tcx>, _ctx: TyCtxt<'tcx>) -> bool {
    let usize_const = garg
        .as_const()
        .expect("Generic argument was not an constant!");

    let kind = usize_const.kind();
    match kind {
        ConstKind::Value(val) => {
            let scalar = val
                .try_to_leaf()
                .expect("String const did not contain valid scalar!");
            let ty = val.ty;
            assert!(
                ty.is_bool(),
                "Generic argument was not a bool type! ty:{ty:?}"
            );
            scalar.to_uint(scalar.size()) != 0
        }
        _ => todo!("Can't convert generic arg of const kind {kind:?} to string!"),
    }
}
/// This function returns the size of a type at the compile time. This should be used ONLY for handling constants. It currently assumes a 64 bit env
pub fn const_sizeof<'tcx>(ty: Ty<'tcx>, tcx: TyCtxt<'tcx>) -> u64 {
    let layout = tcx
        .layout_of(PseudoCanonicalInput {
            typing_env: rustc_middle::ty::TypingEnv::fully_monomorphized(),
            value: ty,
        })
        .expect("Can't get layout of a type.")
        .layout;
    layout.size.bytes()
}
/// Ensures that a type is morphic.
#[macro_export]
macro_rules! assert_morphic {
    ($ty:ident) => {
        let ty_kind = $ty.kind();
        debug_assert!(
            !matches!(ty_kind, TyKind::Alias(_, _)),
            "ERROR: NON MORPHIC TYPE(ALIAS TYPE) {ty:?} WHERE MORPHIC TYPE EXPECTED!",
            ty = $ty
        );
        debug_assert!(
            !matches!(ty_kind, TyKind::Param(_)),
            "ERROR: NON MORPHIC TYPE(GENERIC PARAM TYPE) {ty:?} WHERE MORPHIC TYPE EXPECTED!",
            ty = $ty
        );
    };
}
