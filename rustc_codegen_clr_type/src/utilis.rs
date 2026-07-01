use cilly::{
    Assembly, ClassRef, Type, bimap::Interned,
    utilis::escape_class_name,
};
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
pub fn pointer_to_is_fat<'tcx>(
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
    //TODO: find a better way to get adt instances!
    let krate = adt.did().krate;
    let adt_instance = instance_try_resolve(adt.did(), tcx, gargs);
    // Get the mangled path: it is absolute, and not poluted by types being rexported
    let auto_mangled =
        rustc_symbol_mangling::symbol_name_for_instance_in_crate(tcx, adt_instance, krate);
    // Then, demangle the type name, converting it to a Rust-style one (eg. `core::option::Option::h8zc8s`)
    let demangled = rustc_demangle::demangle(&auto_mangled);
    // Using formating preserves the generic hash.
    let demangled = format!("{demangled}");
    // Replace Rust namespace(module) spearators with C# ones.
    let dotnet_class_name = demangled.replace("::", ".");
    escape_class_name(&dotnet_class_name)
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
    // (2) Monomorphized generics must stay mangled (one `AdtDef`, many instantiations).
    if gargs
        .iter()
        .any(|g| g.as_type().is_some() || g.as_const().is_some())
    {
        return None;
    }
    // (3) Only export artifacts de-mangle. Executables (the gate) and rlibs (build-std deps) do not.
    let is_export_artifact = tcx
        .crate_types()
        .iter()
        .any(|ct| matches!(ct, CrateType::Cdylib | CrateType::Dylib | CrateType::StaticLib));
    if !is_export_artifact {
        return None;
    }
    // Build a clean, stable name from the definition path (no symbol-mangling hash). Always qualify
    // with the crate name so the C# type lands in a `Crate.Module.Type` namespace.
    let krate = tcx.crate_name(adt.did().krate);
    let def_path = tcx.def_path_str(adt.did());
    let qualified = if def_path.starts_with(krate.as_str()) {
        def_path
    } else {
        format!("{krate}::{def_path}")
    };
    Some(escape_class_name(&qualified))
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
/// Tries to get the value of Const `size` as usize.
pub fn try_resolve_const_size(size: Const) -> Result<usize, &'static str> {
    let value = match size.try_to_value() {
        Some(value) => Ok(value),
        None => Err("Can't resolve scalar array size!"),
    }?;
    let value = value
        .try_to_leaf()
        .unwrap()
        .to_u64();
    Ok(usize::try_from(value).expect("Const size value too big."))
}
