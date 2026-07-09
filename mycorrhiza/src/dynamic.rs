//! Raw dynamic (late-bound) `.NET` reflection invoke — the escape hatch for calling a method whose
//! `(assembly, type, method, argument shape)` isn't known until *runtime*.
//!
//! Everything else `mycorrhiza`/`spinacz` does — the generated `bindings.rs`, `add-nuget`, the
//! hand-written `bcl` wrappers — is STATIC binding: the `(ASSEMBLY, CLASS_PATH, METHOD)` triple is a
//! Rust-compile-time constant (a `const &'static str` generic parameter), so the backend emits a real
//! CIL `call`/`callvirt` for it. That's the right default (it's checked, fast, and shows up in a
//! disassembly the way you'd expect) but it structurally cannot express "call whatever method this
//! config string names" — a plugin system, a method chosen at runtime, a truly dynamic API. This
//! module is that other case: it takes the whole `(assembly, type, method, args)` tuple as *runtime
//! values* and reaches `System.Reflection` (`Assembly.Load` / `Type.GetMethod` / `MethodInfo.Invoke`)
//! through a small bundled C# helper, `Mycorrhiza.Reflection.DynamicInvoker` (see
//! `mycorrhiza_interop_helpers/DynamicInvoker.cs`, built and copied next to any consumer of
//! `mycorrhiza` automatically — same delivery mechanism as [`crate::linq`]'s `ParameterRebinder`).
//!
//! # Arity, not a slice
//!
//! `invoke_dynamic0..4` take up to four individual [`DynArg`]s rather than `args: &[DynArg]`. An
//! earlier version of this module took a Rust slice and built the helper's `object?[]` with a `for
//! arg in args { .. }` loop; that lowering trips a backend codegen gap in the generic
//! slice-element-load path for managed-class-containing elements (a spurious self-`ManagedPtrCast`
//! the CIL typechecker correctly rejects — verified empirically, both with a bare `&[MObject]` and a
//! newtype wrapper). The rest of this crate's interop primitives (`static0..static2`,
//! `instance0..instance2`, ...) are already a fixed arity ladder for the exact same reason CIL calls
//! are — this module just extends that convention up to four explicit args (matching every proof
//! case below) instead of introducing the one construct that doesn't lower cleanly yet.
//!
//! # What `unsafe` means here
//!
//! [`invoke_dynamic1`] (and its `0`/`2`/`3`/`4`-arity siblings) are `unsafe`, but **not** because they
//! can cause memory unsafety — a boxed `System.Object[]` argument array and a `MethodInfo.Invoke` are
//! just as memory-safe as any other managed call this crate makes. The `unsafe` marks a *different*
//! contract: unlike every other call in this crate, the `(assembly, type, method)` triple and the
//! argument *count and types* are not checked by `rustc` or by the CIL typechecker at build time —
//! they are checked by the CLR's own overload resolution, at the moment the call actually runs. A
//! typo'd method name, a missing overload, or the wrong argument count/type all surface as an
//! ordinary **runtime .NET exception**, in a place the Rust type system had no way to prevent. That's
//! a framework-level footgun (the same one any reflection API in any language has), not a soundness
//! hole — but it's real enough to be worth an explicit `unsafe` marker so a caller doesn't reach for
//! this by reflex.
//!
//! The parts of the job that genuinely *are* checkable stay safe: building each argument
//! ([`box_arg`] / [`ref_arg`] / [`str_arg`]) can't go wrong (boxing a `ManagedSafe` value type, or
//! upcasting an existing managed reference to `object`, is always valid), and if you'd rather get a
//! [`Result`] than crash the process on a bad call, [`invoke_dynamic1_checked`] (and siblings) wrap
//! the same call in [`crate::error::try_managed`] and are **safe** to call — the "genuinely
//! unvalidatable" failure mode becomes a caught [`ManagedException`](crate::error::ManagedException)
//! instead of an unhandled one. `unsafe` is reserved for the raw `invoke_dynamicN` functions: the raw
//! call, with no safety net, for callers who have already decided how they want to handle (or
//! deliberately not handle) a resolution failure.

use crate::error::{try_managed, ManagedException};
use crate::intrinsics::{
    rustc_clr_interop_box as box_value, rustc_clr_interop_managed_checked_cast as checked_cast,
    rustc_clr_interop_managed_new_arr as new_arr, rustc_clr_interop_managed_set_elem as set_elem,
    RustcCLRInteropManagedArray, RustcCLRInteropManagedClass,
};
use crate::system::{MObject, MString};
use crate::ManagedSafe;

/// The bundled helper assembly's simple name. Must match `mycorrhiza_interop_helpers`'s
/// `<AssemblyName>` and [`crate::linq::PARAMETER_REBINDER_ASSEMBLY`] exactly (same assembly, two
/// helper classes in it) — see the module docs for the delivery mechanism.
pub const DYNAMIC_INVOKER_ASSEMBLY: &str = "Mycorrhiza.Interop.Helpers";
/// The bundled helper's fully-qualified class name (see [`DYNAMIC_INVOKER_ASSEMBLY`]).
pub const DYNAMIC_INVOKER_CLASS: &str = "Mycorrhiza.Reflection.DynamicInvoker";

type MObjArray = RustcCLRInteropManagedArray<MObject, 1>;
type Invoker = RustcCLRInteropManagedClass<DYNAMIC_INVOKER_ASSEMBLY, DYNAMIC_INVOKER_CLASS>;

/// One dynamic-invoke argument — a value already boxed/upcast to `System.Object`. Build one with
/// [`box_arg`] (value types) or [`ref_arg`] / [`str_arg`] (reference types) and pass it to
/// `invoke_dynamicN` / `invoke_dynamicN_checked`.
#[derive(Clone, Copy)]
pub struct DynArg(MObject);

/// Box a [`ManagedSafe`] value (a Rust primitive, or a `.NET` value-type struct laid out the way this
/// crate expects) into a `System.Object` argument. This is always valid — boxing a value type can't
/// fail or misbehave — it is only the *call* that resolves the boxed value's runtime type dynamically.
#[inline(always)]
pub fn box_arg<T: ManagedSafe>(v: T) -> DynArg {
    DynArg(box_value::<T>(v))
}

/// Upcast an existing managed reference (an [`MString`], or any other
/// [`RustcCLRInteropManagedClass`] handle) to `System.Object`, as a dynamic-invoke argument.
/// Reference types don't need boxing — this is a plain, always-valid reference upcast (every managed
/// reference IS-A `System.Object`).
#[inline(always)]
pub fn ref_arg<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str>(
    v: RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>,
) -> DynArg {
    DynArg(checked_cast::<MObject, RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>>(v))
}

/// A `&str` argument, as a boxed `System.Object` — shorthand for `ref_arg(MString::from(s))`.
#[inline(always)]
pub fn str_arg(s: &str) -> DynArg {
    ref_arg(MString::from(s))
}

/// Call the bundled `Mycorrhiza.Reflection.DynamicInvoker.InvokeStatic` helper with an already-built
/// `object?[]`. Shared by every `invoke_dynamicN` below.
#[inline(always)]
fn invoke_helper(assembly: &str, type_name: &str, method_name: &str, args: MObjArray) -> MObject {
    Invoker::static4::<"InvokeStatic", MString, MString, MString, MObjArray, MObject>(
        MString::from(assembly),
        MString::from(type_name),
        MString::from(method_name),
        args,
    )
}

/// Call `{assembly}!{type_name}.{method_name}()` — a **public static**, zero-argument `.NET` method
/// resolved entirely at runtime via `System.Reflection`. See the module docs for the `unsafe`
/// contract and [`invoke_dynamic0_checked`] for a safe, `Result`-returning wrapper.
///
/// # Safety
/// See the module-level "What `unsafe` means here" section: this does not risk memory unsafety, but
/// `assembly`/`type_name`/`method_name` are not validated ahead of time — a bad target raises an
/// unhandled `.NET` exception at the call site.
#[inline(always)]
pub unsafe fn invoke_dynamic0(assembly: &str, type_name: &str, method_name: &str) -> MObject {
    let args: MObjArray = new_arr::<MObject>(0);
    invoke_helper(assembly, type_name, method_name, args)
}

/// One-argument counterpart of [`invoke_dynamic0`].
///
/// # Safety
/// See [`invoke_dynamic0`] / the module docs.
#[inline(always)]
pub unsafe fn invoke_dynamic1(
    assembly: &str,
    type_name: &str,
    method_name: &str,
    a0: DynArg,
) -> MObject {
    let args: MObjArray = new_arr::<MObject>(1);
    set_elem::<MObject>(args, 0, a0.0);
    invoke_helper(assembly, type_name, method_name, args)
}

/// Two-argument counterpart of [`invoke_dynamic0`].
///
/// # Safety
/// See [`invoke_dynamic0`] / the module docs.
#[inline(always)]
pub unsafe fn invoke_dynamic2(
    assembly: &str,
    type_name: &str,
    method_name: &str,
    a0: DynArg,
    a1: DynArg,
) -> MObject {
    let args: MObjArray = new_arr::<MObject>(2);
    set_elem::<MObject>(args, 0, a0.0);
    set_elem::<MObject>(args, 1, a1.0);
    invoke_helper(assembly, type_name, method_name, args)
}

/// Three-argument counterpart of [`invoke_dynamic0`].
///
/// # Safety
/// See [`invoke_dynamic0`] / the module docs.
#[inline(always)]
pub unsafe fn invoke_dynamic3(
    assembly: &str,
    type_name: &str,
    method_name: &str,
    a0: DynArg,
    a1: DynArg,
    a2: DynArg,
) -> MObject {
    let args: MObjArray = new_arr::<MObject>(3);
    set_elem::<MObject>(args, 0, a0.0);
    set_elem::<MObject>(args, 1, a1.0);
    set_elem::<MObject>(args, 2, a2.0);
    invoke_helper(assembly, type_name, method_name, args)
}

/// Four-argument counterpart of [`invoke_dynamic0`].
///
/// # Safety
/// See [`invoke_dynamic0`] / the module docs.
#[inline(always)]
pub unsafe fn invoke_dynamic4(
    assembly: &str,
    type_name: &str,
    method_name: &str,
    a0: DynArg,
    a1: DynArg,
    a2: DynArg,
    a3: DynArg,
) -> MObject {
    let args: MObjArray = new_arr::<MObject>(4);
    set_elem::<MObject>(args, 0, a0.0);
    set_elem::<MObject>(args, 1, a1.0);
    set_elem::<MObject>(args, 2, a2.0);
    set_elem::<MObject>(args, 3, a3.0);
    invoke_helper(assembly, type_name, method_name, args)
}

// ---- checked wrappers -----------------------------------------------------------------------
//
// These do NOT return the `MObject` result through `try_managed`'s own `Result` (i.e. NOT
// `try_managed(|| invoke_dynamicN(..)) `). `Result<MObject, _>` would place a managed reference
// inside a Rust enum niche, which the CLR layout rejects (a managed ref cannot be overlapped by a
// discriminant) -- the exact wall `mycorrhiza::bcl::json::Json::parse` already documents and works
// around. Same fix here: the managed result is written into `out` via the closure's captured
// `&mut`, the closure itself returns `()` (nothing managed crosses the `try/catch` boundary), and
// `try_managed`'s `Result<(), ManagedException>` only gates whether `out` is meaningful.

/// Safe wrapper over [`invoke_dynamic0`]: catches the failure mode `unsafe` warns about as an
/// `Err(`[`ManagedException`]`)` instead of letting it abort the process.
#[inline(always)]
pub fn invoke_dynamic0_checked(
    assembly: &str,
    type_name: &str,
    method_name: &str,
) -> Result<MObject, ManagedException> {
    let mut out = MObject::null();
    try_managed(|| out = unsafe { invoke_dynamic0(assembly, type_name, method_name) })?;
    Ok(out)
}

/// Safe wrapper over [`invoke_dynamic1`]. See [`invoke_dynamic0_checked`].
#[inline(always)]
pub fn invoke_dynamic1_checked(
    assembly: &str,
    type_name: &str,
    method_name: &str,
    a0: DynArg,
) -> Result<MObject, ManagedException> {
    let mut out = MObject::null();
    try_managed(|| out = unsafe { invoke_dynamic1(assembly, type_name, method_name, a0) })?;
    Ok(out)
}

/// Safe wrapper over [`invoke_dynamic2`]. See [`invoke_dynamic0_checked`].
#[inline(always)]
pub fn invoke_dynamic2_checked(
    assembly: &str,
    type_name: &str,
    method_name: &str,
    a0: DynArg,
    a1: DynArg,
) -> Result<MObject, ManagedException> {
    let mut out = MObject::null();
    try_managed(|| out = unsafe { invoke_dynamic2(assembly, type_name, method_name, a0, a1) })?;
    Ok(out)
}

/// Safe wrapper over [`invoke_dynamic3`]. See [`invoke_dynamic0_checked`].
#[inline(always)]
pub fn invoke_dynamic3_checked(
    assembly: &str,
    type_name: &str,
    method_name: &str,
    a0: DynArg,
    a1: DynArg,
    a2: DynArg,
) -> Result<MObject, ManagedException> {
    let mut out = MObject::null();
    try_managed(|| out = unsafe { invoke_dynamic3(assembly, type_name, method_name, a0, a1, a2) })?;
    Ok(out)
}

/// Safe wrapper over [`invoke_dynamic4`]. See [`invoke_dynamic0_checked`].
#[inline(always)]
pub fn invoke_dynamic4_checked(
    assembly: &str,
    type_name: &str,
    method_name: &str,
    a0: DynArg,
    a1: DynArg,
    a2: DynArg,
    a3: DynArg,
) -> Result<MObject, ManagedException> {
    let mut out = MObject::null();
    try_managed(|| {
        out = unsafe { invoke_dynamic4(assembly, type_name, method_name, a0, a1, a2, a3) }
    })?;
    Ok(out)
}
