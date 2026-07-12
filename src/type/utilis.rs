use cilly::{Assembly, ClassRef, Type, bimap::Interned, utilis::escape_class_name};
use rustc_hir::attrs::CrateType;
use rustc_middle::ty::Const;
use rustc_middle::ty::List;
use rustc_middle::ty::{
    AdtDef, ConstKind, EarlyBinder, GenericArg, Instance, PseudoCanonicalInput, Ty, TyCtxt,
    TypeFoldable,
};
use rustc_span::def_id::DefId;

/// This struct represetnts either a primitive .NET type (F32,F64), or stores information on how to lookup a more complex type (struct,class,array)
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, PartialEq, Clone, Eq, Hash, Debug)]
pub struct DotnetArray {
    pub element: Type,
    pub dimensions: u64,
}

pub const INTEROP_CLASS_TPE_NAME: &str = "RustcCLRInteropManagedClass";
pub const INTEROP_STRUCT_TPE_NAME: &str = "RustcCLRInteropManagedStruct";
pub const INTEROP_CHR_TPE_NAME: &str = "RustcCLRInteropManagedChar";
pub const INTEROP_ARR_TPE_NAME: &str = "RustcCLRInteropManagedArray";
/// A handle to a managed object of a *generic* .NET instantiation, e.g. `List<i32>`. Carries
/// `<ASSEMBLY, CLASS_PATH, ClassGenerics>` where `ClassGenerics` is a tuple of the concrete .NET type
/// arguments — lowers to a `ClassRef` with those generics. (WF-9 generic interop bridge.)
pub const INTEROP_GENERIC_TPE_NAME: &str = "RustcCLRInteropManagedGeneric";
/// A managed **value type** of a generic .NET instantiation, e.g. `Nullable<T>` / `Span<T>`. Carries
/// `<ASSEMBLY, CLASS_PATH, SIZE, ClassGenerics>` — lowers to a *value-type* `ClassRef` that also
/// carries the concrete generic arguments (the value-type counterpart to `RustcCLRInteropManagedGeneric`).
pub const INTEROP_GENERIC_STRUCT_TPE_NAME: &str = "RustcCLRInteropManagedGenericStruct";
/// Signature-shape marker lowering to the .NET *class* generic parameter `!N` — used to describe the
/// definition signature of a method called on a generic instantiation.
pub const INTEROP_TYPE_GENERIC_TPE_NAME: &str = "RustcCLRInteropTypeGeneric";
/// Signature-shape marker lowering to the .NET *method* generic parameter `!!N`.
pub const INTEROP_METHOD_GENERIC_TPE_NAME: &str = "RustcCLRInteropMethodGeneric";
/// Signature-shape marker lowering to a managed byref `Inner&` (`Type::Ref`) — the return shape of a
/// byref-returning member such as `Span<T>.get_Item(int) -> ref T` (`RustcCLRInteropByRef<gen!(0)>`).
pub const INTEROP_BYREF_TPE_NAME: &str = "RustcCLRInteropByRef";
#[must_use]
/// Checks if a type is a magic interop type.
pub fn is_name_magic(name: &str) -> bool {
    name.contains("RustcCLRInteropManaged")
}
#[must_use]
pub fn garg_to_usize<'tcx>(garg: GenericArg<'tcx>, _ctx: TyCtxt<'tcx>) -> u64 {
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
                ty.is_integral(),
                "Generic argument was not a unit type! ty:{ty:?}",
            );
            u64::try_from(scalar.to_uint(scalar.size()))
                .expect("Scalar of type usize has value over 2^64")
        }
        _ => todo!("Can't convert generic arg of const kind {kind:?} to string!"),
    }
}
#[must_use]
pub fn tuple_name(elements: &[Type], asm: &Assembly) -> String {
    let generics: String = elements.iter().map(|t| t.mangle(asm)).collect();
    format!(
        "Tuple{generic_count}{generics}",
        generic_count = generics.len()
    )
}
/// Creates a tuple with no more than 8 elements.
#[must_use]
pub fn simple_tuple(elements: &[cilly::Type], asm: &mut Assembly) -> Interned<ClassRef> {
    let name = tuple_name(elements, asm);
    let name = asm.alloc_string(name);
    asm.alloc_class_ref(ClassRef::new(name, None, true, [].into()))
}

#[must_use]
pub fn is_fat_ptr<'tcx>(
    ptr_type: Ty<'tcx>,
    tcx: TyCtxt<'tcx>,
    method: rustc_middle::ty::Instance<'tcx>,
) -> bool {
    use rustc_abi::BackendRepr;
    let ptr_type = monomorphize(&method, ptr_type, tcx);
    let layout = tcx
        .layout_of(PseudoCanonicalInput {
            typing_env: rustc_middle::ty::TypingEnv::fully_monomorphized(),
            value: ptr_type,
        })
        .expect("Can't get layout of a type.")
        .layout;
    let abi = layout.0.0.backend_repr;
    match abi {
        BackendRepr::Scalar(_) => false,
        BackendRepr::ScalarPair(_, _) => true,
        _ => panic!("Unexpected abi of pointer to {ptr_type:?}. The ABI was:{abi:?}"),
    }
}
/// Monomorphizes type `ty`
pub fn monomorphize<'tcx, T: TypeFoldable<TyCtxt<'tcx>> + Clone>(
    instance: &Instance<'tcx>,
    ty: T,
    ctx: TyCtxt<'tcx>,
) -> T {
    instance.instantiate_mir_and_normalize_erasing_regions(
        ctx,
        rustc_middle::ty::TypingEnv::fully_monomorphized(),
        EarlyBinder::bind(ty),
    )
}
/// Converts a generic argument to a string, and panics if it could not.
pub fn garg_to_string<'tcx>(garg: GenericArg<'tcx>, ctx: TyCtxt<'tcx>) -> String {
    let str_const = garg
        .as_const()
        .expect("Generic argument was not an constant!");
    let kind = str_const.kind();
    match kind {
        ConstKind::Value(val) => {
            let raw_bytes = val
                .try_to_raw_bytes(ctx)
                .expect("String const did not contain valid string!");
            let tpe = val
                .ty
                .builtin_deref(true)
                .expect("Type of generic argument was not a reference, can't resolve as string!");
            assert!(
                tpe.is_str(),
                "Generic argument was not a string, but {str_const:?}!"
            );
            String::from_utf8(raw_bytes.into()).expect("String constant invalid!")
        }
        _ => todo!("Can't convert generic arg of const kind {kind:?} to string!"),
    }
}
#[must_use]
pub fn ptr_is_fat<'tcx>(
    pointed_type: Ty<'tcx>,
    tcx: TyCtxt<'tcx>,
    method: rustc_middle::ty::Instance<'tcx>,
) -> bool {
    is_fat_ptr(
        Ty::new_ptr(tcx, pointed_type, rustc_hir::Mutability::Mut),
        tcx,
        method,
    )
}

pub fn is_zst<'tcx>(ty: rustc_middle::ty::Ty<'tcx>, tcx: TyCtxt<'tcx>) -> bool {
    let layout = tcx
        .layout_of(PseudoCanonicalInput {
            typing_env: rustc_middle::ty::TypingEnv::fully_monomorphized(),
            value: ty,
        })
        .expect("Can't get layout of a type.")
        .layout;
    layout.is_zst()
}
pub fn adt_name<'tcx>(
    adt: AdtDef<'tcx>,
    tcx: TyCtxt<'tcx>,
    gargs: &'tcx List<GenericArg<'tcx>>,
) -> String {
    // Do not manufacture an `Instance` merely to ask the symbol mangler for presentation text.
    // ADTs are definitions, not callable instances, and forcing them through instance resolution
    // is both fallible and conceptually the wrong identity layer. `def_path_str` supplies the stable,
    // crate-qualified readable prefix directly.
    let display_name = readable_def_path(tcx, adt.did());

    // Presentation names are not identities: macro-generated local ADTs can have the same
    // readable path while still being distinct DefIds (serde_with emits several local
    // `__DeserializeWith` structs this way). The identity must also include generic arguments:
    // `Result<LocalA, E>` and `Result<LocalB, E>` share Result's DefId and can have identical
    // demangled presentation text. `type_id_hash` is rustc's deterministic identity for the fully
    // instantiated type: it incorporates each definition's DefPathHash and recursively hashes the
    // instantiated arguments with regions erased. Keep the readable prefix, but always key internal
    // types by that full identity.
    let instantiated = Ty::new_adt(tcx, adt, gargs);
    let type_identity = format!("{:032x}", tcx.type_id_hash(instantiated));
    internal_adt_name(&display_name, &type_identity)
}

fn internal_adt_name(display_name: &str, type_identity: &str) -> String {
    let readable = display_name.replace("::", ".");
    escape_class_name(&format!("{readable}.tid_{type_identity}"))
}

/// Builds a crate-qualified readable path directly from rustc's stable definition data.
///
/// Unlike `TyCtxt::def_path_str`, this never enters the diagnostic-only `trimmed_def_paths` query,
/// which is invalid during backend codegen because that query promises to emit a diagnostic. The
/// full type hash remains the actual internal identity; this path is presentation only.
fn readable_def_path(tcx: TyCtxt<'_>, did: DefId) -> String {
    let path = tcx.def_path(did);
    let mut rendered = tcx.crate_name(path.krate).to_string();
    for component in path.data {
        rendered.push_str("::");
        rendered.push_str(component.as_sym(false).as_str());
    }
    rendered
}
/// Like [`adt_name`], but produces a **stable, de-mangled** public name (e.g. `rust_export.Point`
/// instead of the symbol-mangled `rust_export[<hash>].Point`) for types that form a library's
/// externally-visible surface, so a .NET consumer can reference them by a clean, build-stable name.
///
/// Returns `None` — and the caller falls back to [`adt_name`] (the mangled name) — unless the type is:
///
/// 1. **local** to the crate being compiled (`is_local`). Foreign types (`std`/`core`/`alloc`/deps)
///    keep their mangled names *everywhere*. This is what preserves cross-crate coherence: the linker
///    merges per-crate assemblies by matching `ClassRef` name strings, so a type's name must be a pure
///    function of its identity, not of which crate happens to be lowering it. A foreign type is only
///    ever de-mangled (if at all) in its *home* crate; keeping it mangled in every consumer guarantees
///    the def and all refs agree.
/// 2. **non-generic** (no un-erased type/const args). Monomorphized generics *must* stay mangled:
///    distinct instantiations share one Rust `AdtDef`, and only the mangled symbol disambiguates them
///    (also .NET bans explicit layout on generics — see ARCHITECTURE.md §5).
/// 3. compiled into an **export artifact** — `Cdylib`/`Dylib`/`StaticLib`. This is the signal that the
///    crate exists to be consumed. It deliberately excludes:
///    - **`Executable`** — every `::stable` test program is an executable, so de-mangling is a strict
///      no-op for the regression gate (its CIL stays byte-identical), and
///    - **`Rlib`** — the `core`/`alloc`/`std` crates produced by `build-std` are rlibs, so their local
///      types stay mangled, matching how a consuming cdylib references them (point 1).
///
/// The name is built from the **definition path**, which carries no mangling hash and is therefore
/// stable across builds. Because both the def-key (`class_def`→`ref_to`) and every use-site (`get_adt`)
/// flow through this one name with `asm = None`, interning stays coherent by construction.
pub fn stable_adt_name<'tcx>(
    adt: AdtDef<'tcx>,
    tcx: TyCtxt<'tcx>,
    gargs: &'tcx List<GenericArg<'tcx>>,
) -> Option<String> {
    // (1) Foreign types keep their mangled names in every crate -> cross-crate coherent.
    if !adt.did().is_local() {
        return None;
    }
    // A stable, hash-free name is an interop ABI promise. Preserve it for public exported types,
    // but keep private/compiler-generated helpers on the identity-bearing `adt_name` path so two
    // distinct DefPathHashes can never collapse into one CLR class.
    if !tcx.visibility(adt.did()).is_public() {
        return None;
    }
    // (2) Monomorphized generics must stay mangled (one `AdtDef`, many instantiations).
    if gargs
        .iter()
        .any(|g| g.as_type().is_some() || g.as_const().is_some())
    {
        return None;
    }
    // (3) Only export artifacts de-mangle. Executables (the gate) and rlibs (build-std deps) do not.
    let is_export_artifact = tcx.crate_types().iter().any(|ct| {
        matches!(
            ct,
            CrateType::Cdylib | CrateType::Dylib | CrateType::StaticLib
        )
    });
    if !is_export_artifact {
        return None;
    }
    // Build a clean, stable name from the definition path (no symbol-mangling hash). Always qualify
    // with the crate name so the C# type lands in a `Crate.Module.Type` namespace.
    Some(escape_class_name(&readable_def_path(tcx, adt.did())))
}

#[cfg(test)]
mod identity_tests {
    use super::internal_adt_name;

    #[test]
    fn same_display_name_with_different_def_path_hashes_stays_distinct() {
        let left = internal_adt_name("crate::f::__DeserializeWith", "11112222");
        let right = internal_adt_name("crate::f::__DeserializeWith", "33334444");
        assert_ne!(left, right);
    }

    #[test]
    fn same_type_identity_is_deterministic_across_codegen_shards() {
        let left = internal_adt_name("crate::f::__DeserializeWith", "11112222");
        let right = internal_adt_name("crate::f::__DeserializeWith", "11112222");
        assert_eq!(left, right);
    }
}
// WARNING: this is *wrong*: For some reason, `Instance::try_resolve` should not operate on structs(why?), and this just silences the newly introduced warning.
// Root cause not understood — this is a `.unwrap().unwrap()` around `resolve_instance_raw`
// wrapping what should probably be a non-panicking / non-Instance query for ADTs. It sits on
// the mangled-name path used for every non-`stable_adt_name`-eligible ADT (see `adt_name`
// below, and the `instance_try_resolve` call sites in `src/aggregate.rs`/`src/binop/mod.rs`),
// so a wrong resolution here can corrupt name mangling or panic on legitimate structs whose
// resolution doesn't fit whatever `Instance::try_resolve` actually expects.
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
/// Tries to get the value of Const `size` as usize.
pub fn resolve_const_size(size: Const) -> Result<usize, &'static str> {
    let value = match size.try_to_value() {
        Some(value) => Ok(value),
        None => Err("Can't resolve scalar array size!"),
    }?;
    let value = value.try_to_leaf().unwrap().to_u64();
    Ok(usize::try_from(value).expect("Const size value too big."))
}
