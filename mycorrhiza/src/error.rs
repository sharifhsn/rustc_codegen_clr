//! Idiomatic error/optional-value bridges between the .NET world and Rust.
//!
//! Two everyday .NET habits become native Rust here:
//!
//! * **`null` ↔ [`Option`]** — a managed reference that might be `null` (the most common .NET
//!   "no value" signal) is surfaced as an [`Option`] of a Rust value via [`from_nullable`] / the
//!   [`Nullable`] trait's [`map_present`](Nullable::map_present) (`null` → [`None`], otherwise
//!   `Some(f())`). No more manual `.is_null()` checks at every call site. (It maps to a Rust value
//!   rather than `Option<managed>`, which is a layout wall — see the `null <-> Option` section.)
//! * **thrown exception ↔ [`Result`]** — a possibly-throwing managed call is run inside a CIL
//!   `try/catch` and its outcome surfaced as `Result<T, `[`ManagedException`]`>` via [`try_managed`]
//!   (or the [`TryManaged::try_`] combinator on a closure). This catches *foreign* .NET/BCL
//!   exceptions — the ones a Rust `catch_unwind` deliberately rethrows because they are not a
//!   `RustException`.
//! * **`?` into your own error type** — [`impl_from_managed_exception!`] generates a
//!   `From<ManagedException>` impl for a consumer's own error type (a blanket impl isn't legal Rust
//!   here — orphan rules), so `?` bubbles a [`try_managed`] failure straight into it without a
//!   hand-rolled `From` at every call site.
//!
//! ```ignore
//! use mycorrhiza::prelude::*;
//!
//! // null -> Option (map the live reference to a Rust value)
//! let s = DotNetString::from("hi").handle();
//! let len: Option<i32> = s.map_present(|| DotNetString::from_handle(s).len_utf16());
//! assert_eq!(len, Some(2));
//! assert!(MString::null().present().is_none());
//!
//! // throwing call -> Err, non-throwing call -> Ok
//! let ok  = try_managed(|| 2 + 2);                     // Ok(4)
//! let err = try_managed(|| Guid::parse(bad_handle));   // Err(ManagedException)
//! ```

use core::mem::ManuallyDrop;
use core::mem::MaybeUninit;

use crate::intrinsics::RustcCLRInteropManagedClass;

// ===========================================================================================
// null <-> Option
//
// A subtlety that shapes this API: `Option<managed-ref>` is one of the project's true layout walls
// (docs/TRANSLATION_STATUS.md §7). Rust enums are unions, and a managed reference may not live in an
// overlapping/union field — the .NET GC needs an unambiguous ref/non-ref map per offset, and Rust's
// niche optimization would overlap the managed ref with the `None` slot. So we deliberately never
// construct an `Option<Self>` for a managed `Self`; the bridge maps the live reference to a *Rust*
// value (which has no such restriction) and only *that* goes into the `Option`. `present()` /
// `map_present` express "null → None, else Some(f(ref))" without ever laying out `Option<managed>`.
// ===========================================================================================

/// A managed reference type that has a well-defined `null` value and can be compared against it.
///
/// Every managed-class handle ([`RustcCLRInteropManagedClass`]) implements this, so a possibly-`null`
/// BCL/managed reference can be consumed the Rusty way — [`map_present`](Nullable::map_present) turns
/// it into an `Option` of a plain Rust value (`null` → [`None`], otherwise `Some(f())`).
///
/// It intentionally does **not** offer `-> Option<Self>`: `Option<managed-ref>` cannot be laid out on
/// stock CoreCLR (a managed ref can't sit in an enum's overlapping field — see the module docs), so
/// the mapping form is the honest bridge. The mapping closure takes **no argument** (rather than
/// receiving `Self`): a managed reference passed *as a call argument* through the generic `FnOnce`
/// ABI trips a backend aggregate-sizing limitation, whereas *capturing* the (always-`Copy`) reference
/// is fine — so `x.map_present(|| convert(x))` is the shape that lowers cleanly.
pub trait Nullable: Copy {
    /// The managed `null` reference of this type.
    fn null_ref() -> Self;
    /// Whether `self` is the managed `null` reference (reference identity against `null`).
    fn is_null_ref(self) -> bool;
    /// Whether this reference is present (i.e. *not* `null`).
    #[inline(always)]
    fn is_present(self) -> bool {
        !self.is_null_ref()
    }
    /// `null` → [`None`], otherwise [`Some`]`(f())`. The idiomatic way to consume a possibly-`null`
    /// managed reference: `f` turns the (captured) live reference into a Rust value (e.g. marshal a
    /// `String`, read a field), which is what actually lands in the `Option` — the managed reference
    /// itself never enters the `Option` (that would be an un-layout-able `Option<managed>`).
    ///
    /// Capture the reference in the closure: `s.map_present(|| DotNetString::from_handle(s).len_utf16())`.
    #[inline(always)]
    fn map_present<R>(self, f: impl FnOnce() -> R) -> Option<R> {
        if self.is_null_ref() {
            None
        } else {
            Some(f())
        }
    }
    /// `null` → [`None`], otherwise [`Some(())`] — just the presence bit, when you only need to branch
    /// on "was there a value?" and don't want to move the reference at all.
    #[inline(always)]
    fn present(self) -> Option<()> {
        self.map_present(|| ())
    }
}

impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str> Nullable
    for RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
{
    #[inline(always)]
    fn null_ref() -> Self {
        Self::null()
    }
    #[inline(always)]
    fn is_null_ref(self) -> bool {
        self.is_null()
    }
}

/// Turn a possibly-`null` managed reference into an [`Option`] of a Rust value: `null` → [`None`],
/// otherwise [`Some`]`(f())`. Free-function form of [`Nullable::map_present`], for the
/// `from_nullable(x, || …)` reading. (See the module docs for why the mapping form is required rather
/// than `-> Option<managed>`, and why the closure captures rather than receives the reference.)
#[inline(always)]
pub fn from_nullable<T: Nullable, R>(handle: T, f: impl FnOnce() -> R) -> Option<R> {
    handle.map_present(f)
}

// ===========================================================================================
// thrown exception <-> Result
// ===========================================================================================

/// A caught managed exception.
///
/// The interop `try/catch` primitive catches *any* .NET exception and reports that one occurred, but
/// the underlying built-in does not (yet) hand the exception object back across the seam — a managed
/// reference cannot be smuggled through the `*mut u8` catch callback. So `ManagedException` is, for
/// now, a lightweight marker: it tells you a managed exception was thrown and swallowed, which is the
/// information needed to turn a throwing call into an [`Err`]. (Carrying the exception object itself
/// is a follow-up once a managed-ref catch ABI exists.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedException {
    _priv: (),
}

impl ManagedException {
    #[inline(always)]
    fn new() -> Self {
        ManagedException { _priv: () }
    }
}

impl core::fmt::Display for ManagedException {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("a managed .NET exception was thrown")
    }
}

// The interop try/catch built-in, recognized by the backend by symbol name (see
// `MANAGED_TRY_CATCH` in `src/utilis/mod.rs`). It wraps an indirect `try_fn(data)` in a CIL
// `try/catch` that catches EVERYTHING, running `catch_fn(data)` and returning 1 on a caught
// exception, or 0 on normal completion. The bodies below `abort()`: the backend replaces every call
// with the real `interop_try_catch` IL, so these are never actually executed.
//
// The callbacks are `extern "C-unwind"`, NOT `extern "C"`: the whole purpose is to let a .NET
// exception thrown inside `try_fn` unwind *up to* the surrounding IL `catch`. A plain `extern "C"`
// callback is `nounwind`, so rustc inserts an abort-on-unwind guard into it and the managed exception
// triggers a `FailFast` ("unwinding crossed a nounwind ABI boundary") before the IL catch ever runs.
#[allow(unused_variables)]
#[inline(never)]
fn rustc_clr_interop_try_catch(
    try_fn: fn(*mut u8),
    data: *mut u8,
    catch_fn: fn(*mut u8),
) -> i32 {
    core::intrinsics::abort();
}

// Per-instantiation trampoline state threaded through the `*mut u8` data pointer. Before the call it
// holds the closure; after a successful call it holds the produced value; `caught` is flipped by the
// catch trampoline.
struct TryState<F, T> {
    func: ManuallyDrop<F>,
    result: MaybeUninit<T>,
    caught: bool,
}

// The `try` trampoline: read the closure out of the state and run it, storing the result. It is
// `extern "C-unwind"` so a managed exception thrown by the closure may unwind through it up to the
// IL `catch` in `interop_try_catch` (see the note on `rustc_clr_interop_try_catch`).
fn try_trampoline<F: FnOnce() -> T, T>(data: *mut u8) {
    // SAFETY: `data` is the `TryState<F, T>` set up in `try_managed`, live for the whole call.
    let state = unsafe { &mut *(data as *mut TryState<F, T>) };
    // SAFETY: `func` is initialized exactly once and taken exactly once (here), before any catch.
    let func = unsafe { ManuallyDrop::take(&mut state.func) };
    state.result.write(func());
}

// The `catch` trampoline: a managed exception was caught. Record it; the closure's result was never
// produced, so `result` stays uninitialized and must not be read.
fn catch_trampoline<F, T>(data: *mut u8) {
    // SAFETY: same live `TryState<F, T>` as above.
    let state = unsafe { &mut *(data as *mut TryState<F, T>) };
    state.caught = true;
    // The closure was `ManuallyDrop::take`n only on the success path; if the exception was thrown
    // *before* `func` ran (the common case — the throwing call *is* `func`), `func` was still taken
    // (the throw happens inside it). We conservatively do not drop it here to avoid a double-drop or
    // dropping a moved-out value; `F` is typically a trivial closure with no `Drop` glue. For an `F`
    // that does capture something with real `Drop` glue, this is a genuine leak on the throw path —
    // see the note on `try_managed`.
}

/// Run a possibly-throwing managed call and surface its outcome as a [`Result`].
///
/// `f` is executed inside a CIL `try/catch` that catches **any** .NET exception. If `f` returns
/// normally, you get [`Ok`] with its value; if a managed exception is thrown, you get
/// [`Err`]`(`[`ManagedException`]`)`. This is the *foreign-exception* counterpart to
/// [`std::panic::catch_unwind`] (which rethrows non-Rust exceptions).
///
/// On the throw path, `f`'s captured state is **not** dropped (see `catch_trampoline`) — avoid
/// capturing anything with real `Drop` glue (an owned `Vec`/`String`/`Box`, ...), or it leaks whenever
/// the managed call throws.
///
/// ```ignore
/// let n = try_managed(|| some_bcl_call_that_might_throw())?;
/// ```
#[inline]
pub fn try_managed<T, F: FnOnce() -> T>(f: F) -> Result<T, ManagedException> {
    let mut state = TryState::<F, T> {
        func: ManuallyDrop::new(f),
        result: MaybeUninit::uninit(),
        caught: false,
    };
    let data = (&raw mut state) as *mut u8;
    let caught = rustc_clr_interop_try_catch(try_trampoline::<F, T>, data, catch_trampoline::<F, T>);
    if caught != 0 || state.caught {
        Err(ManagedException::new())
    } else {
        // SAFETY: no exception was caught, so `try_trampoline` ran to completion and initialized
        // `result` exactly once.
        Ok(unsafe { state.result.assume_init() })
    }
}

/// Ergonomic combinator form of [`try_managed`]: `(|| bcl_call()).try_()` reads left-to-right.
pub trait TryManaged<T> {
    /// Run this closure under a managed `try/catch`, yielding `Result<T, `[`ManagedException`]`>`.
    fn try_(self) -> Result<T, ManagedException>;
}

impl<T, F: FnOnce() -> T> TryManaged<T> for F {
    #[inline]
    fn try_(self) -> Result<T, ManagedException> {
        try_managed(self)
    }
}

/// Give a consumer's own error type a `From<ManagedException>` impl, so `?` can bubble a
/// [`try_managed`] (or [`TryManaged::try_`]) failure straight into it without a hand-rolled
/// conversion at every call site.
///
/// A blanket `impl<E> From<ManagedException> for E` is not legal Rust here (orphan rules — neither
/// `ManagedException` nor a downstream `E` is local to this crate), so each consumer's error type
/// needs its own `impl`. This macro generates exactly that `impl`, wrapping the caught exception in
/// whichever variant/constructor you name.
///
/// Syntax: `impl_from_managed_exception!(<ErrorType>, <Variant-or-fn-path>)`, where the second
/// argument is anything callable as `path(ManagedException) -> ErrorType` — typically a tuple-variant
/// path like `MyError::Managed`, but any `fn(ManagedException) -> MyError` works too (e.g. a helper
/// constructor).
///
/// ```ignore
/// use mycorrhiza::prelude::*;
///
/// #[derive(Debug)]
/// enum MyError {
///     Managed(ManagedException),
///     Other(String),
/// }
///
/// impl_from_managed_exception!(MyError, MyError::Managed);
///
/// fn parse_guid(s: MString) -> Result<Guid, MyError> {
///     // `?` now converts a `ManagedException` into `MyError::Managed(..)` automatically.
///     let g = try_managed(|| Guid::parse(s))?;
///     Ok(g)
/// }
/// ```
#[macro_export]
macro_rules! impl_from_managed_exception {
    ($Err:ty, $ctor:expr) => {
        impl ::core::convert::From<$crate::error::ManagedException> for $Err {
            #[inline]
            fn from(e: $crate::error::ManagedException) -> $Err {
                $ctor(e)
            }
        }
    };
}

// ===========================================================================================
// runtime-message managed throw
// ===========================================================================================

/// Raise a genuine, catchable `System.Exception` carrying a message computed **at run time**.
///
/// [`crate::intrinsics::rustc_clr_interop_throw`] (the backend-recognized `MANAGED_THROW` intrinsic)
/// only accepts a **compile-time** message: its `MSG` parameter is a `const` generic, resolved by the
/// backend straight off the callee's generic-argument list before any argument marshalling runs (see
/// `garg_to_string` in the backend's type lowering), so it can never carry a `String`/`&str` value that
/// is only known once the program is running (e.g. a formatted error, or a captured `panic!` payload).
///
/// This function is the *runtime* counterpart, built entirely out of the ordinary generic managed-call
/// surface ([`RustcCLRInteropManagedClass::ctor1`]/[`RustcCLRInteropManagedClass::instance0`]/
/// [`RustcCLRInteropManagedClass::static1`]) — no new backend intrinsic is needed. It:
///
/// 1. marshals `msg` to a real managed `System.String` ([`crate::system::DotNetString`]),
/// 2. constructs a `System.Exception(string)` from it (`Exception::ctor1`),
/// 3. and raises it via `System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(ex).Throw()`
///    — an ordinary managed instance method whose *body* performs the CIL `throw`, so from codegen's
///    point of view this is nothing more than a call returning `!`-shaped control flow; no `throw` IL
///    opcode needs to be emitted directly by the backend.
///
/// The resulting exception is a genuine managed `System.Exception` — a C# `catch (Exception e)` around
/// the call sees `e.Message == msg`, exactly like [`rustc_clr_interop_throw`]'s fixed-message form.
///
/// `ExceptionDispatchInfo.Throw()` preserves the *original* stack trace of `ex` (which, since `ex` was
/// just constructed here, is simply "thrown from here") rather than resetting it the way a bare
/// `throw ex;` would in C# — a harmless, arguably nicer property for this use, not something this
/// function relies on.
///
/// **Caller ABI requirement**: if this is called from an `extern` function that is itself called
/// directly from C# (e.g. a `#[dotnet_export]`-generated shim's `catch_unwind` `Err` arm), that
/// function must be declared `extern "C-unwind"`, **not** plain `extern "C"`. `Throw()` performs a
/// genuine CIL `throw`, which this backend correctly models as an operation that can unwind; a call
/// that can unwind, sitting outside any protecting `catch_unwind` closure inside a plain `nounwind
/// extern "C"` function, is exactly the shape rustc's MIR builder flags as escaping a nounwind ABI
/// boundary — and it lowers to the *same* hard `Environment.FailFast` abort as an actual uncaught
/// panic, silently defeating the point of calling this function at all. This was verified empirically
/// (a minimal repro with `extern "C"` still aborted on the managed throw; switching only the ABI
/// qualifier to `extern "C-unwind"` — no other change — made the exception reach the C# `try`/`catch`
/// correctly). `#[dotnet_export]`'s generated shim already uses `extern "C-unwind"` for this reason.
#[inline]
pub fn throw_msg(msg: &str) -> ! {
    use crate::bindings::System::Exception;
    use crate::bindings::System::Runtime::ExceptionServices::ExceptionDispatchInfo;
    use crate::system::DotNetString;

    let mmsg = DotNetString::from(msg).handle();
    let exc = Exception::ctor1::<crate::system::MString>(mmsg);
    ExceptionDispatchInfo::capture(exc).throw();
    // `Throw()` never returns (it always raises), but its Rust-visible signature is `()`, not `!` —
    // give the compiler the same terminal guarantee `rustc_clr_interop_throw` provides.
    unreachable!("ExceptionDispatchInfo::Throw() does not return")
}
