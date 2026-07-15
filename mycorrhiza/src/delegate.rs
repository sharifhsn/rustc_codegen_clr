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
//! capture-less closure written `extern "C"`. `from_closure` accepts a capturing `'static` Rust
//! closure through a boxed trampoline. Its environment is intentionally leaked because the managed
//! delegate may outlive every Rust scope; prefer `from_fn` for capture-less or high-churn callbacks.
//! Argument/return types must each cross the boundary (a .NET primitive, a `#[repr(C)]` value type of
//! such, or a managed handle) — the same rule as [`crate::collections`].
//!
//! **Delegate identity.** Each wrapper is a move-only handle to a managed delegate object; the .NET GC
//! owns it (no `Drop`). `.handle()` exposes the raw
//! [`RustcCLRInteropManagedGeneric`](crate::intrinsics::RustcCLRInteropManagedGeneric) so you can pass
//! the delegate to any method taking this exact delegate type, or hand it to an event `add_*`.
//! Conversely, a wrapper used as a `#[dotnet_export]`/`#[dotnet_methods]` parameter receives a real
//! C# delegate and can invoke it directly; the automatic import surface covers primitives and
//! managed `MString` handles for the wrapper families defined below.
//!
//! Delegates whose signature uses an enclosing class generic are supported too: the collection
//! wrappers use these handles for `List<T>.Sort(Comparison<T>)` and
//! `List<T>.ForEach(Action<T>)`. See `cargo_tests/cd_collections` for the end-to-end proof.

use crate::intrinsics::{
    RustcCLRInteropManagedGeneric, RustcCLRInteropTypeGeneric, rustc_clr_interop_delegate,
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

/// `System.Action` — a void-returning, zero-argument delegate.
///
/// This non-generic sibling is especially useful for cancellation registration and lifecycle
/// callbacks. Capturing closures follow the same process-lifetime rule as the generic wrappers;
/// APIs that can prove unregistration (such as `CancellationRegistration`) own and reclaim the
/// environment separately.
pub struct Action0 {
    h: crate::bindings::System::Action,
}

impl Action0 {
    #[inline]
    pub fn from_fn(f: extern "C" fn()) -> Self {
        let h = rustc_clr_interop_delegate::<
            { CORELIB },
            { ACTION },
            false,
            (),
            ((),),
            extern "C" fn(),
            crate::bindings::System::Action,
        >(f);
        Self { h }
    }

    #[inline]
    pub fn from_closure<Fun: Fn() + 'static>(f: Fun) -> Self {
        type Callback = dyn Fn();
        let boxed: Box<Callback> = Box::new(f);
        let env = Box::into_raw(Box::new(boxed)) as *mut ();
        unsafe { Self::from_owned_env(env, action0_trampoline) }
    }

    /// Build an action over an externally owned closure environment.
    ///
    /// # Safety
    /// `env` must remain live until the delegate can no longer be invoked. `trampoline` must treat
    /// it as the exact environment it was created from.
    #[inline]
    pub(crate) unsafe fn from_owned_env(env: *mut (), trampoline: extern "C" fn(*mut ())) -> Self {
        let h = crate::intrinsics::rustc_clr_interop_delegate_closure::<
            { CORELIB },
            { ACTION },
            false,
            (),
            ((),),
            *mut (),
            extern "C" fn(*mut ()),
            crate::bindings::System::Action,
        >(env, trampoline);
        Self { h }
    }

    #[inline]
    pub fn from_handle(h: crate::bindings::System::Action) -> Self {
        Self { h }
    }

    #[inline]
    pub fn handle(&self) -> crate::bindings::System::Action {
        self.h
    }

    #[inline]
    pub fn invoke(&self) {
        self.h.invoke();
    }
}

extern "C" fn action0_trampoline(env: *mut ()) {
    type Callback = dyn Fn();
    let callback = unsafe { &*(env as *const Box<Callback>) };
    callback();
}

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
                    RustcCLRInteropManagedGeneric<{ CORELIB }, { $class }, ( $($genarg,)+ )>,
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

            /// Build the delegate from a **capturing** Rust closure (`move |x| ...` over local state).
            /// The closure's environment is boxed and **leaked** (kept alive for the process, since the
            /// .NET delegate may outlive this call and the GC won't free Rust memory); the .NET side
            /// invokes it through a trampoline. Use [`from_fn`](Self::from_fn) for a capture-less `fn`
            /// when you don't need to leak.
            #[inline]
            pub fn from_closure<Fun: ::core::ops::Fn( $($iaty),* ) -> $iret + 'static>(f: Fun) -> Self {
                // Box to a `dyn Fn`, then box THAT to get a thin `*mut ()` the shim can hold.
                let boxed: ::std::boxed::Box<dyn ::core::ops::Fn( $($iaty),* ) -> $iret> =
                    ::std::boxed::Box::new(f);
                let env = ::std::boxed::Box::into_raw(::std::boxed::Box::new(boxed)) as *mut ();
                // Monomorphic trampoline (one per delegate signature): reconstruct & call the closure.
                extern "C" fn tramp< $($garg),+ >(
                    env: *mut (),
                    $( $ia : $iaty ),*
                ) -> $iret {
                    let f = unsafe {
                        &*(env as *const ::std::boxed::Box<dyn ::core::ops::Fn( $($iaty),* ) -> $iret>)
                    };
                    f( $($ia),* )
                }
                let h = $crate::intrinsics::rustc_clr_interop_delegate_closure::<
                    { CORELIB }, { $class }, false,
                    ( $($genarg,)+ ),
                    ( $iret, $($iaty,)* ),
                    *mut (),
                    extern "C" fn(*mut (), $($iaty),*) -> $iret,
                    RustcCLRInteropManagedGeneric<{ CORELIB }, { $class }, ( $($genarg,)+ )>,
                >(env, tramp::< $($garg),+ >);
                Self { h }
            }
        }
    };
}

/// `System.EventHandler` — the conventional `(object sender, EventArgs args) -> void` delegate
/// used by many BCL and third-party events. Unlike [`Action2`], this is a non-generic CLR delegate
/// type, so its handle can be passed directly to generated `add_*`/`remove_*` methods that accept
/// `System.EventHandler`.
pub struct EventHandler {
    h: crate::bindings::System::EventHandler,
}

impl EventHandler {
    /// Build an event handler from a capture-less Rust callback.
    #[inline]
    pub fn from_fn(
        f: extern "C" fn(crate::bindings::System::Object, crate::bindings::System::EventArgs),
    ) -> Self {
        let h = rustc_clr_interop_delegate::<
            { CORELIB },
            "System.EventHandler",
            false,
            (),
            (
                (),
                crate::bindings::System::Object,
                crate::bindings::System::EventArgs,
            ),
            extern "C" fn(crate::bindings::System::Object, crate::bindings::System::EventArgs),
            crate::bindings::System::EventHandler,
        >(f);
        Self { h }
    }

    /// Build an event handler from a capturing `'static` Rust closure. The environment follows the
    /// delegate module's existing process-lifetime allocation rule.
    #[inline]
    pub fn from_closure<Fun>(f: Fun) -> Self
    where
        Fun: Fn(crate::bindings::System::Object, crate::bindings::System::EventArgs) + 'static,
    {
        type Callback = dyn Fn(crate::bindings::System::Object, crate::bindings::System::EventArgs);
        let boxed: Box<Callback> = Box::new(f);
        let env = Box::into_raw(Box::new(boxed)) as *mut ();
        extern "C" fn tramp(
            env: *mut (),
            sender: crate::bindings::System::Object,
            args: crate::bindings::System::EventArgs,
        ) {
            let callback = unsafe { &*(env as *const Box<Callback>) };
            callback(sender, args);
        }
        let h = crate::intrinsics::rustc_clr_interop_delegate_closure::<
            { CORELIB },
            "System.EventHandler",
            false,
            (),
            (
                (),
                crate::bindings::System::Object,
                crate::bindings::System::EventArgs,
            ),
            *mut (),
            extern "C" fn(
                *mut (),
                crate::bindings::System::Object,
                crate::bindings::System::EventArgs,
            ),
            crate::bindings::System::EventHandler,
        >(env, tramp);
        Self { h }
    }

    /// The raw handle accepted by reflected/generated event accessors.
    #[inline]
    pub fn handle(&self) -> crate::bindings::System::EventHandler {
        self.h
    }

    /// Invoke the held handler directly.
    #[inline]
    pub fn invoke(
        &self,
        sender: crate::bindings::System::Object,
        args: crate::bindings::System::EventArgs,
    ) {
        self.h.invoke(sender, args);
    }
}

/// An owned event registration that removes the exact same managed delegate on explicit
/// [`unsubscribe`](Self::unsubscribe) or on drop.
///
/// Generated bindings expose event accessors as ordinary functions taking `(owner, delegate)`.
/// Pass those small adapters to [`subscribe`](Self::subscribe); the guard retains both managed
/// handles so removal cannot accidentally use a different delegate instance.
pub struct EventSubscription<Owner: Copy, Delegate: Copy> {
    owner: Owner,
    delegate: Delegate,
    remove: fn(Owner, Delegate),
    active: bool,
}

impl<Owner: Copy, Delegate: Copy> EventSubscription<Owner, Delegate> {
    /// Register `delegate` with `add` and return an active unsubscription guard.
    #[inline]
    pub fn subscribe(
        owner: Owner,
        delegate: Delegate,
        add: fn(Owner, Delegate),
        remove: fn(Owner, Delegate),
    ) -> Self {
        add(owner, delegate);
        Self {
            owner,
            delegate,
            remove,
            active: true,
        }
    }

    /// Whether this guard still owns an active event registration.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Remove the handler now. Drop observes the inactive state and does not remove twice.
    #[inline]
    pub fn unsubscribe(mut self) {
        self.detach();
    }

    fn detach(&mut self) {
        if self.active {
            (self.remove)(self.owner, self.delegate);
            self.active = false;
        }
    }
}

impl<Owner: Copy, Delegate: Copy> Drop for EventSubscription<Owner, Delegate> {
    fn drop(&mut self) {
        self.detach();
    }
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
delegate_wrapper! {
    /// `System.Action<T0, T1, T2>` — a void-returning three-argument delegate.
    Action3<T0, T1, T2> = ACTION, gens = (T0, T1, T2),
    fnptr = extern "C" fn (T0, T1, T2) -> (),
    invoke (a0: T0, a1: T1, a2: T2) -> ()
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
delegate_wrapper! {
    /// `System.Func<T0, T1, T2, R>` — a three-argument delegate returning `R`.
    Func3<T0, T1, T2, R> = FUNC, gens = (T0, T1, T2, R),
    fnptr = extern "C" fn (T0, T1, T2) -> R,
    invoke (a0: T0, a1: T1, a2: T2) -> R
}

// ---- Comparison<T> — the sort/`Comparison<T>` delegate (fixed `(T, T) -> i32` shape, one generic) ----

delegate_wrapper! {
    /// Generic `System.Comparison` over `T` — `(T, T) -> i32`, as taken by `List<T>.Sort` /
    /// `Array.Sort`. Return a
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
            { CORELIB },
            { COMPARISON },
            false,
            { INVOKE },
            2u8,
            (T,),
            (
                i32,
                RustcCLRInteropTypeGeneric<0>,
                RustcCLRInteropTypeGeneric<0>,
            ),
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
// those with the `r#gen!`-style `RustcCLRInteropTypeGeneric<N>` markers, exactly as a hand-written WF-9
// wrapper would. (Kept out of the macro so the `!N` indices read explicitly per arity.)

impl<T0> Action1<T0> {
    /// Invoke the delegate — runs the wrapped callback (`Invoke(!0)`, void).
    #[inline]
    pub fn invoke(&self, a0: T0) {
        crate::intrinsics::rustc_clr_interop_generic_call2::<
            { CORELIB },
            { ACTION },
            false,
            { INVOKE },
            2u8,
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
            { CORELIB },
            { ACTION },
            false,
            { INVOKE },
            2u8,
            (T0, T1),
            (
                (),
                RustcCLRInteropTypeGeneric<0>,
                RustcCLRInteropTypeGeneric<1>,
            ),
            (),
            RustcCLRInteropManagedGeneric<{ CORELIB }, { ACTION }, (T0, T1)>,
            T0,
            T1,
        >(self.h, a0, a1)
    }
}
impl<T0, T1, T2> Action3<T0, T1, T2> {
    /// Invoke the delegate — runs the wrapped callback (`Invoke(!0, !1, !2)`, void).
    #[inline]
    pub fn invoke(&self, a0: T0, a1: T1, a2: T2) {
        crate::intrinsics::rustc_clr_interop_generic_call4::<
            { CORELIB },
            { ACTION },
            false,
            { INVOKE },
            2u8,
            (T0, T1, T2),
            (
                (),
                RustcCLRInteropTypeGeneric<0>,
                RustcCLRInteropTypeGeneric<1>,
                RustcCLRInteropTypeGeneric<2>,
            ),
            (),
            RustcCLRInteropManagedGeneric<{ CORELIB }, { ACTION }, (T0, T1, T2)>,
            T0,
            T1,
            T2,
        >(self.h, a0, a1, a2)
    }
}
impl<T0, R> Func1<T0, R> {
    /// Invoke the delegate, returning its result (`Invoke(!0) -> !1`).
    #[inline]
    pub fn invoke(&self, a0: T0) -> R {
        crate::intrinsics::rustc_clr_interop_generic_call2::<
            { CORELIB },
            { FUNC },
            false,
            { INVOKE },
            2u8,
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
            { CORELIB },
            { FUNC },
            false,
            { INVOKE },
            2u8,
            (T0, T1, R),
            (
                RustcCLRInteropTypeGeneric<2>,
                RustcCLRInteropTypeGeneric<0>,
                RustcCLRInteropTypeGeneric<1>,
            ),
            R,
            RustcCLRInteropManagedGeneric<{ CORELIB }, { FUNC }, (T0, T1, R)>,
            T0,
            T1,
        >(self.h, a0, a1)
    }
}
impl<T0, T1, T2, R> Func3<T0, T1, T2, R> {
    /// Invoke the delegate, returning its result (`Invoke(!0, !1, !2) -> !3`).
    #[inline]
    pub fn invoke(&self, a0: T0, a1: T1, a2: T2) -> R {
        crate::intrinsics::rustc_clr_interop_generic_call4::<
            { CORELIB },
            { FUNC },
            false,
            { INVOKE },
            2u8,
            (T0, T1, T2, R),
            (
                RustcCLRInteropTypeGeneric<3>,
                RustcCLRInteropTypeGeneric<0>,
                RustcCLRInteropTypeGeneric<1>,
                RustcCLRInteropTypeGeneric<2>,
            ),
            R,
            RustcCLRInteropManagedGeneric<{ CORELIB }, { FUNC }, (T0, T1, T2, R)>,
            T0,
            T1,
            T2,
        >(self.h, a0, a1, a2)
    }
}

#[cfg(test)]
mod tests {
    use super::EventSubscription;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ADDS: AtomicUsize = AtomicUsize::new(0);
    static REMOVES: AtomicUsize = AtomicUsize::new(0);

    fn add(owner: u8, delegate: u16) {
        assert_eq!((owner, delegate), (7, 42));
        ADDS.fetch_add(1, Ordering::SeqCst);
    }

    fn remove(owner: u8, delegate: u16) {
        assert_eq!((owner, delegate), (7, 42));
        REMOVES.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn event_subscription_removes_exactly_once_explicitly_or_on_drop() {
        ADDS.store(0, Ordering::SeqCst);
        REMOVES.store(0, Ordering::SeqCst);

        let subscription = EventSubscription::subscribe(7, 42, add, remove);
        assert!(subscription.is_active());
        subscription.unsubscribe();
        assert_eq!(REMOVES.load(Ordering::SeqCst), 1);

        {
            let subscription = EventSubscription::subscribe(7, 42, add, remove);
            assert!(subscription.is_active());
        }

        assert_eq!(ADDS.load(Ordering::SeqCst), 2);
        assert_eq!(REMOVES.load(Ordering::SeqCst), 2);
    }
}
