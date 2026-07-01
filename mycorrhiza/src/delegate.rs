//! Delegates & callbacks — hand a Rust callback to .NET as a managed `Action<..>` / `Func<.., R>`
//! delegate, invoke a held delegate from Rust, and (composing the two) subscribe to a .NET event.
//!
//! A .NET delegate is a first-class managed object wrapping a target + method pointer. C# builds one
//! with `new Action<int>(SomeMethod)`; the CLR requires the pointer to name a *managed* method whose
//! signature matches the delegate's `Invoke`. A Rust callback is a native function, so the backend
//! synthesises a tiny managed **shim** per signature (holding the native pointer, `calli`-ing it from
//! its `Invoke`) and builds the real generic delegate over `ldftn shim::Invoke`. All of that is behind
//! [`crate::intrinsics::rustc_clr_interop_delegate`]; this module is the ergonomic face.
//!
//! ```ignore
//! use mycorrhiza::delegate::{Action1, Func2};
//!
//! extern "C" fn shout(x: i32) { /* … */ }
//! let a = Action1::<i32>::from_fn(shout);
//! a.invoke(7);                    // runs `shout(7)` on the .NET side
//!
//! extern "C" fn add(a: i32, b: i32) -> i32 { a + b }
//! let f = Func2::<i32, i32, i32>::from_fn(add);
//! assert_eq!(f.invoke(2, 3), 5);   // `callvirt Func`2::Invoke` → the Rust callback
//! ```
//!
//! **What `invoke` really is.** Each `invoke` lowers to `callvirt Action`/`Func`/`Comparison`
//! `::Invoke` on the managed delegate object — i.e. the .NET runtime dispatching *through a real
//! first-class delegate* into your Rust function. That is the load-bearing capability: any .NET code
//! holding one of these delegates (an event, a `List<T>.ForEach`, a LINQ pipeline) invokes your Rust
//! callback exactly the same way.
//!
//! **Callback shape.** `from_fn` takes an `extern "C" fn(..) -> ..` — a plain top-level fn or a
//! capture-less closure written `extern "C"`. **Captures are not yet supported** (a closure
//! environment has no place to live on the managed side without a boxing trampoline); pass state via
//! arguments or a `'static` for now. Argument/return types must each cross the boundary (a .NET
//! primitive, a `#[repr(C)]` value type of such, or a managed handle) — the same rule as
//! [`crate::collections`].
//!
//! **Delegate identity.** Each wrapper is a move-only handle to a managed delegate object; the .NET GC
//! owns it (no `Drop`). `.handle()` exposes the raw
//! [`RustcCLRInteropManagedGeneric`](crate::intrinsics::RustcCLRInteropManagedGeneric) so you can pass
//! the delegate to any method taking this exact delegate type, or hand it to an event `add_*`.
//!
//! **Not yet:** handing a delegate to a *generic* method whose delegate parameter is itself
//! parameterised by the class generic (`List<T>.Sort(Comparison<T>)`, `List<T>.ForEach(Action<T>)`)
//! needs the CIL type-verifier to model nested generic-parameter binding (`Comparison`1<!0>` param
//! vs a `Comparison`1<int32>` argument). That is a follow-up; a delegate over a *concrete* signature
//! (everything here) is fully supported.

use crate::intrinsics::{
    rustc_clr_interop_delegate, RustcCLRInteropManagedGeneric, RustcCLRInteropTypeGeneric,
};

// Delegates live in the core implementation assembly (a ref assembly forwards + throws at JIT).
const CORELIB: &str = "System.Private.CoreLib";
// `Action`/`Func`/`Comparison` are namespaced directly under `System`.
const ACTION: &str = "System.Action";
const FUNC: &str = "System.Func";
/// `System.Comparison<T>` — the delegate `List<T>.Sort` / `Array.Sort` take: `(T, T) -> int`.
const COMPARISON: &str = "System.Comparison";

/// The delegate `Invoke` method name (every `Action`/`Func` exposes it).
const INVOKE: &str = "Invoke";

/// Emits one `Action<..>` / `Func<..>` wrapper: a move-only handle over the managed delegate, a
/// `from_fn` that lowers a native fn pointer to the delegate (via `rustc_clr_interop_delegate`), an
/// `invoke` that calls the delegate's own `Invoke` (via the WF-9 generic bridge — this is also how a
/// *held* .NET delegate is invoked), and `.handle()` for passing the delegate to a .NET API.
///
/// Per line: the wrapper name, the delegate class path, the tuple of generic *type params* (the
/// arg types then — for `Func` — the return type, i.e. the `Action`/`Func` type-argument order), the
/// `extern "C"` fn-pointer type, the `invoke` value params, and the `invoke` return type.
macro_rules! delegate_wrapper {
    (
        $(#[$meta:meta])*
        $name:ident < $($garg:ident),+ >
            = $class:expr, gens = ( $($genarg:ident),+ $(,)? ),
            fnptr = extern "C" fn ( $($fa:ident),* ) -> $fret:ty,
            invoke ( $($ia:ident : $iaty:ident),* ) -> $iret:ty
    ) => {
        $(#[$meta])*
        pub struct $name< $($garg),+ > {
            h: RustcCLRInteropManagedGeneric<{ CORELIB }, { $class }, ( $($genarg,)+ )>,
        }

        impl< $($garg),+ > $name< $($garg),+ > {
            /// Build the delegate from a native Rust callback (`extern "C" fn`). A capture-less
            /// closure coerces to this pointer type automatically.
            #[inline]
            pub fn from_fn(f: extern "C" fn ( $($fa),* ) -> $fret) -> Self {
                // `ClassGenerics` = the delegate's concrete type args (arg types [+ ret for `Func`]);
                // `Sig` = the *concrete* pointer signature `(Ret, In0, …)` the shim `calli`s.
                let h = rustc_clr_interop_delegate::<
                    { CORELIB }, { $class }, false,
                    ( $($genarg,)+ ),
                    ( $fret, $($fa,)* ),
                    extern "C" fn ( $($fa),* ) -> $fret,
                >(f);
                Self { h }
            }

            /// The raw managed-delegate handle — pass it to any .NET method taking this delegate type
            /// (e.g. an event `add_*`, `List<T>.ForEach`/`Sort`).
            #[inline]
            pub fn handle(
                &self,
            ) -> RustcCLRInteropManagedGeneric<{ CORELIB }, { $class }, ( $($genarg,)+ )> {
                self.h
            }

            /// Wrap an existing managed-delegate handle (e.g. one returned from a .NET call) so it can
            /// be `invoke`d from Rust. This is the "hold and invoke a .NET delegate" direction.
            #[inline]
            pub fn from_handle(
                h: RustcCLRInteropManagedGeneric<{ CORELIB }, { $class }, ( $($genarg,)+ )>,
            ) -> Self {
                Self { h }
            }
        }
    };
}

// ---- Action (void-returning) delegates ----

delegate_wrapper! {
    /// `System.Action<T0>` — a void-returning one-argument delegate.
    Action1<T0> = ACTION, gens = (T0,),
    fnptr = extern "C" fn (T0) -> (),
    invoke (a0: T0) -> ()
}
delegate_wrapper! {
    /// `System.Action<T0, T1>` — a void-returning two-argument delegate.
    Action2<T0, T1> = ACTION, gens = (T0, T1),
    fnptr = extern "C" fn (T0, T1) -> (),
    invoke (a0: T0, a1: T1) -> ()
}

// ---- Func (value-returning) delegates ----

delegate_wrapper! {
    /// `System.Func<T0, R>` — a one-argument delegate returning `R`.
    Func1<T0, R> = FUNC, gens = (T0, R),
    fnptr = extern "C" fn (T0) -> R,
    invoke (a0: T0) -> R
}
delegate_wrapper! {
    /// `System.Func<T0, T1, R>` — a two-argument delegate returning `R`.
    Func2<T0, T1, R> = FUNC, gens = (T0, T1, R),
    fnptr = extern "C" fn (T0, T1) -> R,
    invoke (a0: T0, a1: T1) -> R
}

// ---- Comparison<T> — the sort/`Comparison<T>` delegate (fixed `(T, T) -> i32` shape, one generic) ----

delegate_wrapper! {
    /// `System.Comparison<T>` — `(T, T) -> i32`, as taken by `List<T>.Sort` / `Array.Sort`. Return a
    /// negative / zero / positive `i32` for less / equal / greater (the .NET `IComparer` convention).
    Comparison<T> = COMPARISON, gens = (T,),
    fnptr = extern "C" fn (T, T) -> i32,
    invoke (a0: T, a1: T) -> i32
}

impl<T> Comparison<T> {
    /// Invoke the comparison delegate directly (`Invoke(!0, !0) -> int`).
    #[inline]
    pub fn invoke(&self, a0: T, a1: T) -> i32 {
        crate::intrinsics::rustc_clr_interop_generic_call3::<
            { CORELIB }, { COMPARISON }, false, { INVOKE }, 2u8,
            (T,),
            (i32, RustcCLRInteropTypeGeneric<0>, RustcCLRInteropTypeGeneric<0>),
            i32,
            RustcCLRInteropManagedGeneric<{ CORELIB }, { COMPARISON }, (T,)>,
            T,
            T,
        >(self.h, a0, a1)
    }
}

// The `invoke` methods call the delegate's own `Invoke` through the WF-9 generic bridge. `Invoke`'s
// definition-shape signature uses the delegate's class generics: an `Action<T..>`'s `Invoke(!0, !1, …)`
// returns void; a `Func<T.., R>`'s `Invoke(!0, …)` returns the LAST class generic (`!N`). We spell
// those with the `gen!`-style `RustcCLRInteropTypeGeneric<N>` markers, exactly as a hand-written WF-9
// wrapper would. (Kept out of the macro so the `!N` indices read explicitly per arity.)

impl<T0> Action1<T0> {
    /// Invoke the delegate — runs the wrapped callback (`Invoke(!0)`, void).
    #[inline]
    pub fn invoke(&self, a0: T0) {
        crate::intrinsics::rustc_clr_interop_generic_call2::<
            { CORELIB }, { ACTION }, false, { INVOKE }, 2u8,
            (T0,),
            ((), RustcCLRInteropTypeGeneric<0>),
            (),
            RustcCLRInteropManagedGeneric<{ CORELIB }, { ACTION }, (T0,)>,
            T0,
        >(self.h, a0)
    }
}
impl<T0, T1> Action2<T0, T1> {
    /// Invoke the delegate — runs the wrapped callback (`Invoke(!0, !1)`, void).
    #[inline]
    pub fn invoke(&self, a0: T0, a1: T1) {
        crate::intrinsics::rustc_clr_interop_generic_call3::<
            { CORELIB }, { ACTION }, false, { INVOKE }, 2u8,
            (T0, T1),
            ((), RustcCLRInteropTypeGeneric<0>, RustcCLRInteropTypeGeneric<1>),
            (),
            RustcCLRInteropManagedGeneric<{ CORELIB }, { ACTION }, (T0, T1)>,
            T0,
            T1,
        >(self.h, a0, a1)
    }
}
impl<T0, R> Func1<T0, R> {
    /// Invoke the delegate, returning its result (`Invoke(!0) -> !1`).
    #[inline]
    pub fn invoke(&self, a0: T0) -> R {
        crate::intrinsics::rustc_clr_interop_generic_call2::<
            { CORELIB }, { FUNC }, false, { INVOKE }, 2u8,
            (T0, R),
            (RustcCLRInteropTypeGeneric<1>, RustcCLRInteropTypeGeneric<0>),
            R,
            RustcCLRInteropManagedGeneric<{ CORELIB }, { FUNC }, (T0, R)>,
            T0,
        >(self.h, a0)
    }
}
impl<T0, T1, R> Func2<T0, T1, R> {
    /// Invoke the delegate, returning its result (`Invoke(!0, !1) -> !2`).
    #[inline]
    pub fn invoke(&self, a0: T0, a1: T1) -> R {
        crate::intrinsics::rustc_clr_interop_generic_call3::<
            { CORELIB }, { FUNC }, false, { INVOKE }, 2u8,
            (T0, T1, R),
            (RustcCLRInteropTypeGeneric<2>, RustcCLRInteropTypeGeneric<0>, RustcCLRInteropTypeGeneric<1>),
            R,
            RustcCLRInteropManagedGeneric<{ CORELIB }, { FUNC }, (T0, T1, R)>,
            T0,
            T1,
        >(self.h, a0, a1)
    }
}
