use crate::r#type::escape_field_name;
use rustc_abi::{ExternAbi, VariantIdx};
use rustc_hir::def::DefKind;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::{
    ConstKind, GenericArg, Instance, List, PseudoCanonicalInput, Ty, TyCtxt, TyKind,
};
use rustc_span::Symbol;

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

/// Exact marker emitted on generated managed-type definition entrypoints.
///
/// These functions live in the consuming crate, so crate provenance cannot identify them the way
/// it identifies the `mycorrhiza::intrinsics` magic functions. An explicit generated marker avoids
/// interpreting an unrelated user function merely because its symbol contains a magic substring.
pub const COMPTIME_ENTRYPOINT_MARKER: &str = "__rustc_codegen_clr_comptime_entrypoint_v1";
/// Exact marker emitted on a schema-derived DTO constructor bridge.
///
/// These helpers are generated in the consuming crate because their arity follows the DTO schema,
/// so they cannot use mycorrhiza crate provenance. [`is_generated_ctor`] additionally requires an
/// unsafe Rust function whose exact identifier encodes a decimal arity; a safe or merely same-named
/// local function remains ordinary Rust code.
pub const GENERATED_CTOR_MARKER: &str = "__rustc_codegen_clr_generated_ctor_v1";
/// Diagnostic-item identities carried by the two managed-box declarations injected into pinned
/// `std` thread lifecycle code. Rustc does not preserve private doc attributes in downstream crate
/// metadata, while diagnostic items are explicitly encoded for exact cross-crate lookup.
pub const STD_THREAD_BOX_NEW_DIAGNOSTIC_ITEM: &str = "rustc_codegen_clr_std_thread_managed_box_new";
pub const STD_THREAD_BOX_TAKE_DIAGNOSTIC_ITEM: &str =
    "rustc_codegen_clr_std_thread_managed_box_take";
/// Exact marker emitted on a `#[dotnet_export]` generated managed-ABI seam.
///
/// A marker alone is not authority: [`is_managed_export`] additionally requires an unsafe
/// `extern "C-unwind"` function. That explicit unsafe boundary prevents an ordinary safe native
/// export from opting into CLR-reference ABI merely by copying this inert documentation string.
pub const MANAGED_EXPORT_MARKER: &str = "__rustc_codegen_clr_managed_export_v1";
/// Exact private marker carried by every backend-recognized mycorrhiza declaration.
pub const MYCORRHIZA_INTRINSIC_MARKER: &str = "__rustc_codegen_clr_intrinsic_v1";

/// Whether `def_id` names an explicitly marked item from a known mycorrhiza interop module.
///
/// The final source identifier is not sufficient identity: any Rust crate can legally declare a
/// same-named function or marker ADT. Requiring both the defining crate and module keeps dependency
/// renaming/re-exporting working because those operations do not change the item's `DefId` origin.
pub fn is_mycorrhiza_intrinsic(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    if tcx.crate_name(def_id.krate).as_str() != "mycorrhiza" {
        return false;
    }
    let path = tcx.def_path_str(def_id);
    let mut segments = path.rsplit("::");
    let _item = segments.next();
    if !matches!(
        segments.next(),
        Some("intrinsics" | "memory" | "cancellation" | "managed_option" | "error" | "enums")
    ) {
        return false;
    }
    #[allow(deprecated)]
    tcx.get_all_attrs(def_id)
        .iter()
        .filter_map(|attr| attr.doc_str())
        .any(|doc| doc.as_str() == MYCORRHIZA_INTRINSIC_MARKER)
}

/// Whether this exact definition opted into comptime managed-type interpretation.
pub fn is_comptime_entrypoint(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    if tcx
        .opt_item_name(def_id)
        .is_none_or(|name| name.as_str() != "rustc_codegen_clr_comptime_entrypoint")
    {
        return false;
    }
    #[allow(deprecated)]
    tcx.get_all_attrs(def_id)
        .iter()
        .filter_map(|attr| attr.doc_str())
        .any(|doc| doc.as_str() == COMPTIME_ENTRYPOINT_MARKER)
}

/// Whether this exact definition is a generated, unsafe managed-constructor bridge.
pub fn is_generated_ctor(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    if tcx.def_kind(def_id) != DefKind::Fn {
        return false;
    }
    let signature = tcx.fn_sig(def_id).skip_binder();
    if !signature.safety().is_unsafe() || signature.abi() != ExternAbi::Rust {
        return false;
    }
    let path = tcx.def_path_str(def_id);
    let name = path.rsplit("::").next().unwrap_or(path.as_str());
    if !name
        .strip_prefix(CTOR_FN_NAME)
        .and_then(|suffix| suffix.strip_suffix('_'))
        .is_some_and(|arity| !arity.is_empty() && arity.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }
    #[allow(deprecated)]
    tcx.get_all_attrs(def_id)
        .iter()
        .filter_map(|attr| attr.doc_str())
        .any(|doc| doc.as_str() == GENERATED_CTOR_MARKER)
}

/// Whether this is one of the exact unsafe managed-box declarations injected into pinned `std`.
pub fn is_std_thread_intrinsic(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    let path = tcx.def_path_str(def_id);
    let diagnostic_item = match path.as_str() {
        "thread::lifecycle::rustc_clr_interop_managed_box_new"
        | "std::thread::lifecycle::rustc_clr_interop_managed_box_new" => {
            STD_THREAD_BOX_NEW_DIAGNOSTIC_ITEM
        }
        "thread::lifecycle::rustc_clr_interop_managed_box_take"
        | "std::thread::lifecycle::rustc_clr_interop_managed_box_take" => {
            STD_THREAD_BOX_TAKE_DIAGNOSTIC_ITEM
        }
        _ => return false,
    };
    if tcx.crate_name(def_id.krate).as_str() != "std"
        || tcx.def_kind(def_id) != DefKind::Fn
        || !tcx.fn_sig(def_id).skip_binder().safety().is_unsafe()
    {
        return false;
    }
    tcx.get_diagnostic_item(Symbol::intern(diagnostic_item)) == Some(def_id)
}

/// Whether this exact definition is an unsafe CLR-managed export generated by `dotnet_macros`.
///
/// Rust still describes the shim with `extern "C-unwind"` so managed exceptions can leave it, but
/// its arguments and result are CLR values rather than a native C ABI. Managed-storage validation
/// may therefore permit direct managed values at this one marked definition boundary while keeping
/// unmarked `extern "C"`/`extern "C-unwind"` exports fail-closed.
pub fn is_managed_export(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    if tcx.def_kind(def_id) != DefKind::Fn {
        return false;
    }
    let signature = tcx.fn_sig(def_id).skip_binder();
    if !signature.safety().is_unsafe() || !matches!(signature.abi(), ExternAbi::C { unwind: true })
    {
        return false;
    }
    #[allow(deprecated)]
    tcx.get_all_attrs(def_id)
        .iter()
        .filter_map(|attr| attr.doc_str())
        .any(|doc| doc.as_str() == MANAGED_EXPORT_MARKER)
}

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
    /// `rustc_clr_interop_managed_is_null` → reference comparison with `ldnull`.
    IsNull,
    /// `rustc_clr_interop_managed_checked_cast` → `castclass`.
    CheckedCast,
    /// `rustc_clr_interop_managed_is_inst` → `isinst`.
    IsInst,
    /// `rustc_clr_interop_managed_ld_elem_ref` → `ldelem.ref`.
    LdElemRef,
    /// `rustc_clr_interop_managed_get_elem` → typed `ldelem`.
    LdElem,
    /// `rustc_clr_interop_managed_new_arr` → `newarr`.
    NewArr,
    /// `rustc_clr_interop_managed_set_elem` → `stelem`.
    SetElem,
    /// `rustc_clr_interop_managed_get_field` → typed `ldfld`.
    ManagedGetField,
    /// `rustc_clr_interop_box` → `box` (a value type into `System.Object`).
    Box,
    /// `rustc_clr_interop_managed_box_new` → CLR-box + GCHandle root, returned as an opaque token.
    ManagedBoxNew,
    /// `rustc_clr_interop_managed_box_get` → copy a rooted value without freeing its GCHandle.
    ManagedBoxGet,
    /// `rustc_clr_interop_managed_box_take` → recover/unbox the rooted value and free its GCHandle.
    ManagedBoxTake,
    /// `rustc_clr_interop_managed_box_free` → release a GCHandle without materializing its target.
    ManagedBoxFree,
    /// `rustc_clr_interop_managed_default` → initialize the managed destination with CLR default.
    ManagedDefault,
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
    /// Stack-only integer ↔ CLR-enum representation conversion.
    EnumReprTransmute,
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
    if is_generated_ctor(tcx, def_id) {
        return Some(MagicFn::Ctor);
    }
    if is_std_thread_intrinsic(tcx, def_id) {
        let path = tcx.def_path_str(def_id);
        return match path.rsplit("::").next() {
            Some("rustc_clr_interop_managed_box_new") => Some(MagicFn::ManagedBoxNew),
            Some("rustc_clr_interop_managed_box_take") => Some(MagicFn::ManagedBoxTake),
            _ => unreachable!("std thread intrinsic identity was validated above"),
        };
    }
    if !is_mycorrhiza_intrinsic(tcx, def_id) {
        return None;
    }
    let path = tcx.def_path_str(def_id);
    let name = path.rsplit("::").next().unwrap_or(path.as_str());
    if matches!(
        name,
        "rustc_clr_interop_enum_from_repr"
            | "rustc_clr_interop_enum_to_repr"
            | "rustc_clr_interop_managed_box_new"
            | "rustc_clr_interop_managed_box_get"
            | "rustc_clr_interop_managed_box_take"
            | "rustc_clr_interop_managed_box_free"
            | "rustc_clr_interop_managed_default"
            | "rustc_clr_interop_try_catch"
    ) && (tcx.def_kind(def_id) != DefKind::Fn
        || !tcx.fn_sig(def_id).skip_binder().safety().is_unsafe())
    {
        return None;
    }
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
        "rustc_clr_interop_managed_is_null" => MagicFn::IsNull,
        "rustc_clr_interop_managed_checked_cast" => MagicFn::CheckedCast,
        "rustc_clr_interop_managed_is_inst" => MagicFn::IsInst,
        "rustc_clr_interop_managed_ld_elem_ref" => MagicFn::LdElemRef,
        "rustc_clr_interop_managed_get_elem" => MagicFn::LdElem,
        "rustc_clr_interop_managed_new_arr" => MagicFn::NewArr,
        "rustc_clr_interop_managed_set_elem" => MagicFn::SetElem,
        "rustc_clr_interop_managed_get_field" => MagicFn::ManagedGetField,
        "rustc_clr_interop_box" => MagicFn::Box,
        "rustc_clr_interop_managed_box_new" => MagicFn::ManagedBoxNew,
        "rustc_clr_interop_managed_box_get" => MagicFn::ManagedBoxGet,
        "rustc_clr_interop_managed_box_take" => MagicFn::ManagedBoxTake,
        "rustc_clr_interop_managed_box_free" => MagicFn::ManagedBoxFree,
        "rustc_clr_interop_managed_default" => MagicFn::ManagedDefault,
        "rustc_clr_interop_try_catch" => MagicFn::TryCatch,
        "rustc_clr_interop_throw" => MagicFn::Throw,
        "rustc_clr_interop_enum_from_repr" | "rustc_clr_interop_enum_to_repr" => {
            MagicFn::EnumReprTransmute
        }
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

/// Converts a `usize` const-generic argument to its host index representation.
pub fn garg_to_usize<'tcx>(garg: GenericArg<'tcx>, _ctx: TyCtxt<'tcx>) -> usize {
    let value = garg
        .as_const()
        .expect("Generic argument was not a constant!");
    match value.kind() {
        ConstKind::Value(value) => {
            let scalar = value
                .try_to_leaf()
                .expect("usize const did not contain a scalar");
            assert!(value.ty.is_usize(), "Generic argument was not usize");
            usize::try_from(scalar.to_uint(scalar.size()))
                .expect("usize const-generic value exceeds host usize")
        }
        kind => todo!("Can't convert generic arg of const kind {kind:?} to usize!"),
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
