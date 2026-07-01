//! Mycorrhiza is a Rust .NET interop framework. It is part of the `rustc_codegen_clr` project, and allows you to interact with the .NET runtime directly.
//! One of aims of the `rustc_codegen_clr` is to reuse existing Rust features and syntax to allow semless integration between Rust and the .NET runtime
//! Mycorrhiza must "look" like a normal crate from the outside, even tough it deeply interacts with `rustc_codegen_clr`. It also should be possible to
//! implement an equivalent APIs in standard Rust.

#![allow(internal_features, incomplete_features)]
#![feature(
    core_intrinsics,
    adt_const_params,
    unsized_const_params,
    inherent_associated_types
)]
#[allow(non_snake_case, unused_imports)]
pub mod bindings;
pub use bindings::*;
// Method-wrapper SLICE proof (Console / Math / StringBuilder / String), retired: it has been
// SUPERSEDED by the full method-bearing `bindings.rs` the `spinacz` generator now emits. Its
// hand-picked overloads (`Math::abs(i32)`, `StringBuilder::append(i32)`, …) define inherent impls
// on the SAME concrete `RustcCLRInteropManagedClass<A, B>` types that `bindings.rs` now also impls
// (the slice's distinct `crate::slice::…` alias path doesn't matter — inherent impls bind to the
// concrete type, not the alias), so wiring both is `E0592 duplicate definitions`. The full
// bindings cover this surface, so the slice module is no longer compiled in.
//   (The standalone `cargo_tests/slice_call_test` crate still `use`s `mycorrhiza::slice_bindings`;
//    it is not a workspace member and is superseded by the full generated surface.)
pub mod class;
/// Ready-to-use, idiomatic wrappers over the common .NET generic collections (`List<T>`,
/// `Dictionary<K, V>`, `HashSet<T>`, `Stack<T>`, `Queue<T>`) — backed by real managed objects, used
/// like `std`. Built on [`generic_bridge`]; no CLR-interop knowledge required at the call site.
pub mod collections;
/// The reusable C#→Rust generic container: [`export_rust_containers!`] emits a size-erased byte
/// vector into your `cdylib`, backing the shipped C# `RustDotnet.RustVec<T>` / `RustBoxVec<T>`.
pub mod containers;
/// Comptime type-export intrinsics — defining a managed .NET class from Rust (used by the
/// `dotnet_macros::dotnet_class` proc-macro and the declarative `dotnet_typedef!`).
pub mod comptime;
/// WF-9 generic-interop ergonomics macros (`dotnet_generic!` / `dotnet_generic_impl!` / `gen!`),
/// which remove the hand-written `rustc_clr_interop_generic_*` turbofish boilerplate. The macros are
/// `#[macro_export]`ed, so they are also reachable at the crate root (`mycorrhiza::dotnet_generic!`).
pub mod generic_bridge;
/// Very low-level interop stuff. Don't use unless you need to.
pub mod intrinsics;
use class::*;
/// Wrappers around types from the `System` namespace
pub mod system;
/// C# `char` type
pub type DotNetChar = crate::intrinsics::RustcCLRInteropManagedChar;

#[macro_export]
macro_rules! panic_handler {
    () => {
        #[panic_handler]
        fn panic(_panic: &core::panic::PanicInfo<'_>) -> ! {
            core::intrinsics::abort();
        }
    };
}
#[macro_export]
macro_rules! start {
    () => {
        #[start]
        fn start(_argc: isize, _argv: *const *const u8) -> isize {
            main();
            0
        }
    };
    ($entry_fn:ident) => {
        #[start]
        fn start(_argc: isize, _argv: *const *const u8) -> isize {
            $entry_fn();
            0
        }
    };
}
/// Marker trait, which signals that a type can be safely passed to and from managed code.
/// # Safety
/// Passing this type to .NET code can't cause any UB.
/// This is always true for:
/// 1. Primitive types
/// 2. Copy + Send + Sync types.
/// 3. .NET objects
/// 4. .NET valuetypes
pub unsafe trait ManagedSafe {}
macro_rules! managed_safe {
    ($t:ty) => {
       unsafe impl ManagedSafe for $t{}
    };
    ($e:ty, $($es:ty),+) => {
        managed_safe! { $e }
        managed_safe! { $($es),+ }
    };
}
managed_safe! {u8,i8,u16,i16,u32,i32,u64,i64,u128,i128,usize,isize,f32,f64}
unsafe impl<T> ManagedSafe for *mut T {}
unsafe impl<T> ManagedSafe for *const T {}
pub trait IntoManagedSafe<Target: ManagedSafe> {
    fn into_managed(self) -> Target;
}
pub trait FromManagedSafe<From: ManagedSafe> {
    fn from_managed(from: From) -> Self;
}
/// A marker trait, implemented for internal types which have very specific safety requirements.
///
/// Those types are exposed only beccause they are sometimes needed for high perfromance / low level code.
/// **Don't use types marked with this trait** unless you know exactly what you are doing.
/// # Safety
/// This kind of type can be:
/// 1. Stored directly on the stack - *not inside any other type*. You could, in theory, store it safely in some types, but the rustc_codegen_clr is not able to check the safety of that, and may raise false alarms, so just don't dop
/// 2. Stored inside a object .NET type.
/// 3. Stored inside a .NET value type.
pub unsafe trait StackOnly {}
