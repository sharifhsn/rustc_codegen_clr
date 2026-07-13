//! Mycorrhiza is the Rust ↔ .NET interop framework for [`rustc_codegen_clr`](https://github.com/FractalFir/rustc_codegen_clr),
//! a `rustc` codegen backend that compiles Rust straight to .NET assemblies. Ordinary Rust running
//! under that backend can create, call, and be called by real managed objects — `List<T>`,
//! `Dictionary<K, V>`, `Task<T>`, exceptions, delegates, LINQ expression trees — without hand-written
//! FFI shims or a separate IDL. One of the project's aims is to reuse existing Rust syntax and
//! features (traits, `?`, iterators, `async`/`.await`) for that integration, so interop code still
//! *reads* like ordinary Rust rather than a foreign-function binding layer. Mycorrhiza must "look"
//! like a normal crate from the outside, even though it deeply interacts with `rustc_codegen_clr`'s
//! codegen; it should also be possible, in principle, to implement equivalent APIs in standard Rust.
//!
//! **Start with [`prelude`]** — `use mycorrhiza::prelude::*;` pulls in the collection wrappers, the
//! managed string, the Task/error/nullable bridges, and the generic-bridge macros, which covers most
//! day-to-day interop code. Reach into the specific module paths below only when you need something
//! the prelude doesn't re-export, or the raw handle underneath an idiomatic wrapper.
//!
//! # A tour of the major modules
//!
//! **Collections & synchronization**
//! - [`collections`] — `List<T>`, `Dictionary<K, V>`, `HashSet<T>`, `Stack<T>`, `Queue<T>`, and more,
//!   backed by real managed objects and used like `std`.
//! - [`sync`] — cross-thread/cross-language primitives (`Semaphore`, `Signal`, `Barrier`,
//!   [`sync::SharedLock`], [`sync::SharedOnce`]) plus MPMC channels over
//!   `System.Threading.Channels`.
//! - [`enumerate`] — wraps any .NET `IEnumerator<T>` as a Rust `impl Iterator<Item = T>`; backs
//!   `for x in &list`-style iteration over the collection wrappers.
//! - [`enumerate_async`] — consumes managed `IAsyncEnumerable<T>` incrementally from Rust through
//!   `MoveNextAsync`, including genuinely incomplete `ValueTask<bool>` operations.
//! - [`span`] — `Span<T>` / `ReadOnlySpan<T>`, zero-copy views over a Rust slice.
//! - [`memory`] — `Memory<T>` / `ReadOnlyMemory<T>`, GC-owned buffers for managed code that retains
//!   data or carries it across an async boundary (construction copies a Rust slice).
//! - [`nullable`] — `System.Nullable<T>` (a generic value type) ↔ Rust `Option<T>`.
//!
//! **BCL wrappers**
//! - [`bcl`] — idiomatic wrappers over the most-used Base Class Library types and statics
//!   (`DateTime`, `TimeSpan`, `Guid`, `Uri`, `Regex`, `Random`, `Stopwatch`, `StringBuilder`,
//!   `Environment`, `Math`, `DotNetDecimal`) — used like normal Rust types, no CLR-interop knowledge
//!   required at the call site.
//! - [`system`] — the managed `System.String` wrapper ([`system::DotNetString`]) and its raw handle.
//!
//! **LINQ & expression trees**
//! - [`linq`] — builds `System.Linq.Expressions` trees, the shape `IQueryable`/EF Core consumes, so
//!   Rust code can construct real LINQ predicates and queries.
//!
//! **Dynamic reflection**
//! - [`dynamic`] — the `unsafe` late-bound escape hatch: call a `.NET` method whose
//!   `(assembly, type, method, args)` isn't known until runtime, via `System.Reflection`. Everything
//!   else in this crate is static binding; reach for this only when the target truly can't be known
//!   at Rust-compile time.
//!
//! **Async / task bridge**
//! - [`task`] — `.await` a .NET `Task`/`Task<T>` from Rust, and expose a Rust `async fn` as a .NET
//!   `Task`/`Task<T>` in return; see [`task::await_task`] / [`task::future_to_task`] for the two
//!   directions, and the module docs for the documented coroutine-layout limits.
//! - [`delegate`] — wrap a Rust `extern "C" fn` as a managed `Action<..>`/`Func<.., R>` delegate,
//!   invoke a held .NET delegate, and pass delegates to .NET APIs/events.
//!
//! **Error handling**
//! - [`error`] — managed `null` ↔ [`Option`] and a thrown .NET exception ↔ [`Result`] via the
//!   interop `try`/`catch` primitive ([`error::try_managed`] / the `.try_()` combinator), plus
//!   `?`-operator ergonomics for [`error::ManagedException`].
//!
//! **Defining/exporting your own types**
//! - [`comptime`] — comptime type-export intrinsics for defining a managed .NET class from Rust;
//!   backs the `dotnet_macros::dotnet_class` proc-macro and the declarative `dotnet_typedef!`.
//! - [`enums`] — mirrors a C# enum as a `#[repr(..)]` Rust enum via the [`dotnet_enum!`] macro.
//! - [`containers`] — the reusable C#→Rust generic container (`export_rust_containers!`),
//!   backing the shipped C# `RustDotnet.RustVec<T>` / `RustBoxVec<T>`.
//! - [`generic_bridge`] — the lower-level WF-9 generic-interop macros (`dotnet_generic!` /
//!   `dotnet_generic_impl!` / `r#gen!`) that the above idiomatic wrappers are themselves built on.
//! - [`intrinsics`] — very low-level interop primitives. Don't use unless you need to.

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
/// Idiomatic Rust wrappers over the most-used Base Class Library value types and static helpers
/// (`DateTime`, `TimeSpan`, `Guid`, `Uri`, `Regex`, `Random`, `Stopwatch`, `StringBuilder`,
/// `Environment`, `Math`) — used like normal Rust types, no CLR-interop knowledge at the call site.
pub mod bcl;
pub mod class;
/// Ready-to-use, idiomatic wrappers over the common .NET generic collections (`List<T>`,
/// `Dictionary<K, V>`, `HashSet<T>`, `Stack<T>`, `Queue<T>`) — backed by real managed objects, used
/// like `std`. Built on [`generic_bridge`]; no CLR-interop knowledge required at the call site.
pub mod collections;
/// Comptime type-export intrinsics — defining a managed .NET class from Rust (used by the
/// `dotnet_macros::dotnet_class` proc-macro and the declarative `dotnet_typedef!`).
pub mod comptime;
/// The reusable C#→Rust generic container: [`export_rust_containers!`] emits a size-erased byte
/// vector into your `cdylib`, backing the shipped C# `RustDotnet.RustVec<T>` / `RustBoxVec<T>`.
pub mod containers;
/// Delegates & callbacks — wrap a Rust `extern "C" fn` as a managed `Action<..>`/`Func<.., R>`
/// delegate, invoke a held .NET delegate, and pass delegates to .NET APIs / events. Built on the
/// [`intrinsics::rustc_clr_interop_delegate`] magic fn + the WF-9 generic bridge; no CLR-interop
/// knowledge required at the call site.
pub mod delegate;
/// Raw dynamic (late-bound) reflection invoke -- call a `.NET` method whose
/// `(assembly, type, method, args)` isn't known until runtime, via `System.Reflection`. See
/// [`dynamic::invoke_dynamic1`] / [`dynamic::invoke_dynamic1_checked`] (and their 0/2/3/4-arity
/// siblings) and the module docs for the `unsafe` contract.
pub mod dynamic;
/// The enumerator bridge — wrap any .NET `IEnumerator<T>` as a Rust `impl Iterator<Item = T>`. This
/// is what backs by-reference iteration (`for x in &list`) over the [`collections`] wrappers.
pub mod enumerate;
pub mod enumerate_async;
/// `.NET enum` ↔ Rust enum bridge — the [`dotnet_enum!`] macro mirrors a C# enum as a `#[repr(..)]`
/// Rust enum with the boundary conversions (`to_handle`/`from_handle`).
pub mod enums;
/// Idiomatic error/optional-value bridges: managed `null` ↔ [`Option`](core::option::Option) and a
/// thrown .NET exception ↔ [`Result`](core::result::Result) via the interop `try/catch` primitive
/// (`try_managed` / the `.try_()` combinator).
pub mod error;
/// WF-9 generic-interop ergonomics macros (`dotnet_generic!` / `dotnet_generic_impl!` / `r#gen!`),
/// which remove the hand-written `rustc_clr_interop_generic_*` turbofish boilerplate. The macros are
/// `#[macro_export]`ed, so they are also reachable at the crate root (`mycorrhiza::dotnet_generic!`).
pub mod generic_bridge;
/// Very low-level interop stuff. Don't use unless you need to.
pub mod intrinsics;
/// Building `System.Linq.Expressions` trees (the shape EF Core / `IQueryable` consumes). See [`linq`].
pub mod linq;
/// GC-owned `System.Memory<T>` / `ReadOnlyMemory<T>` buffers that can outlive a Rust borrow.
pub mod memory;
/// `System.Nullable<T>` ↔ Rust `Option<T>` bridge (a generic value type). See [`nullable::NullableExt`].
pub mod nullable;
/// One-glance import surface — `use mycorrhiza::prelude::*;` pulls in the collections, the managed
/// `DotNetString`, and the generic-bridge macros so interop code reads like `std`.
pub mod prelude;
/// `System.Span<T>` / `ReadOnlySpan<T>` — zero-copy views over a Rust slice. See [`span::Span`].
pub mod span;
/// Cross-thread / cross-language synchronization primitives — `Semaphore`, `Signal`
/// (`ManualResetEventSlim`), `CountdownEvent`, `Barrier`, and [`sync::SharedLock`] (a mutex-shaped
/// `SemaphoreSlim` meant to be shared by reference with C#). See [`sync`] for the honest safety story
/// on what does and doesn't cross the language boundary.
pub mod sync;
/// Wrappers around types from the `System` namespace
pub mod system;
/// The Task ↔ Future bridge — `.await` a .NET `Task<T>` from Rust (poll `IsCompleted` / read
/// `Result`) and expose a Rust `async fn` as a .NET `Task<T>` (drive to completion into a
/// `TaskCompletionSource<T>`). Built on the WF-9 generic bridge; async coroutine lowering already
/// runs on the dotnet PAL, this is the interop seam. See [`task::await_task`] / [`task::future_to_task`].
pub mod task;
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
managed_safe! {bool,u8,i8,u16,i16,u32,i32,u64,i64,u128,i128,usize,isize,f32,f64}
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
