//! Tier-2 interop surface on the REAL `cargo dotnet` PAL flow: a managed `System.String` return and a
//! Rust-raises-a-.NET-exception function. Both pull `mycorrhiza` and were the surrogate-only shapes
//! whose public CoreLib type attribution made a C# consumer fail with CS0012. A cdylib (no `main`).

#![feature(adt_const_params, unsized_const_params)]
#![allow(incomplete_features)]

/// Non-Tier-2 baseline call in the same assembly.
#[unsafe(no_mangle)]
pub extern "C" fn rust_add(a: i32, b: i32) -> i32 {
    a + b
}

/// Returns a managed `System.String` DIRECTLY. The public signature references `System.String`; the
/// CS0012 bug is that this reference was attributed to System.Private.CoreLib not System.Runtime.
///
/// # Safety
/// `ptr` must point to `len` valid, initialized bytes for the duration of the call (C# pins them).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn greet_managed(ptr: *const u8, len: usize) -> mycorrhiza::system::MString {
    let name =
        core::str::from_utf8(core::slice::from_raw_parts(ptr, len)).unwrap_or("<invalid utf8>");
    let greeting = format!("Hello, {name}, from Rust (managed)!");
    mycorrhiza::system::MString::from(greeting.as_str())
}

/// On `None`, raises a `System.Exception` (a `throw` IL op) the C# caller can `catch`.
#[unsafe(no_mangle)]
pub extern "C" fn try_div(a: i32, b: i32) -> i32 {
    match a.checked_div(b) {
        Some(q) => q,
        None => mycorrhiza::intrinsics::rustc_clr_interop_throw::<"try_div: division by zero">(),
    }
}

/// A first-class managed `System.Int32[]` returned to C#. Rust constructs a real .NET array via
/// `newarr`, populates it via `stelem`, and returns it; the public signature lowers to `int[]`.
type IntArray = mycorrhiza::intrinsics::RustcCLRInteropManagedArray<i32, 1>;

#[unsafe(no_mangle)]
pub extern "C" fn make_ints() -> IntArray {
    let a: IntArray = mycorrhiza::intrinsics::rustc_clr_interop_managed_new_arr::<i32>(3);
    mycorrhiza::intrinsics::rustc_clr_interop_managed_set_elem::<i32>(a, 0, 10);
    mycorrhiza::intrinsics::rustc_clr_interop_managed_set_elem::<i32>(a, 1, 20);
    mycorrhiza::intrinsics::rustc_clr_interop_managed_set_elem::<i32>(a, 2, 30);
    a
}
