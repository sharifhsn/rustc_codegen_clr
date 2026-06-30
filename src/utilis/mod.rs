use rustc_abi::VariantIdx;
use rustc_codegen_clr_type::r#type::escape_field_name;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::{
    ConstKind, GenericArg, Instance, List, PseudoCanonicalInput, Ty, TyCtxt, TyKind,
};

pub mod adt;
pub const CTOR_FN_NAME: &str = "rustc_clr_interop_managed_ctor";
pub const MANAGED_CALL_FN_NAME: &str = "rustc_clr_interop_managed_call";
pub const MANAGED_CALL_VIRT_FN_NAME: &str = "rustc_clr_interop_managed_call_virt";
pub const MANAGED_LD_LEN: &str = "rustc_clr_interop_managed_ld_len";
pub const MANAGED_LD_NULL: &str = "rustc_clr_interop_managed_ld_null";
pub const MANAGED_CHECKED_CAST: &str = "rustc_clr_interop_managed_checked_cast";
pub const MANAGED_IS_INST: &str = "rustc_clr_interop_managed_is_inst";
pub const MANAGED_LD_ELEM_REF: &str = "rustc_clr_interop_managed_ld_elem_ref";
pub const MANAGED_NEW_ARR: &str = "rustc_clr_interop_managed_new_arr";
pub const MANAGED_SET_ELEM: &str = "rustc_clr_interop_managed_set_elem";
pub const MANAGED_TRY_CATCH: &str = "rustc_clr_interop_try_catch";
/// Calls a method on a *generic* .NET instantiation (e.g. `List<i32>::Add`). Unlike the
/// `rustc_clr_interop_managed_*` family, the target class carries concrete generic arguments and the
/// method signature is described in its *definition* shape (`!N`/`!!N` markers) — see WF-9.
pub const GENERIC_CALL_FN_NAME: &str = "rustc_clr_interop_generic_call";
/// Constructs a managed object of a *generic* .NET instantiation (e.g. `new List<i32>()`).
pub const GENERIC_CTOR_FN_NAME: &str = "rustc_clr_interop_generic_ctor";
/// Raises a managed `System.Exception` directly (so a .NET caller can `catch` it) — distinct from a
/// Rust `panic!`, which goes through the unwinder and does not propagate cleanly out to managed callers.
pub const MANAGED_THROW: &str = "rustc_clr_interop_throw";
pub fn is_function_magic(name: &str) -> bool {
    name.contains(CTOR_FN_NAME)
        || name.contains(MANAGED_CALL_FN_NAME)
        || name.contains(MANAGED_THROW)
        || name.contains(GENERIC_CALL_FN_NAME)
        || name.contains(GENERIC_CTOR_FN_NAME)
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
pub fn garag_to_bool<'tcx>(garg: GenericArg<'tcx>, _ctx: TyCtxt<'tcx>) -> bool {
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
pub fn compiletime_sizeof<'tcx>(ty: Ty<'tcx>, tcx: TyCtxt<'tcx>) -> u64 {
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
