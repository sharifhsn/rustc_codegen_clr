//! WF-9 generic-interop ergonomics: declarative macros that remove the hand-written turbofish
//! boilerplate around the `rustc_clr_interop_generic_*` magic family.
//!
//! Calling a method on a *generic* .NET instantiation (`List<i32>`, `Dictionary<K, V>`) requires a
//! method reference in the *definition*-shape signature: `List<int32>::Add(!0)`, never `Add(int32)`.
//! The raw intrinsics encode that with an 8-10-element turbofish carrying the assembly, class,
//! `IS_VALUETYPE`, method name, `KIND`, the concrete class-generics tuple, the def-shape `Sig` tuple
//! (using the `RustcCLRInteropTypeGeneric<N>` `!N` markers), the return type and the value-arg types.
//! That is correct but unreadable. These macros generate the *exact same* calls from a concise
//! per-method line.
//!
//! The crux, preserved here: a positional argument or return whose .NET type is the class's `!N`
//! generic must appear in the `Sig` tuple as `RustcCLRInteropTypeGeneric<N>` (its *def* shape), while
//! the runtime value is passed with its ordinary, concrete Rust type. The `gen!(N)` token below
//! captures that half: in a `Sig` position it expands to the `!N` marker, while the caller supplies
//! the concrete Rust value type alongside it. A position whose .NET type is concrete (e.g.
//! `get_Item`'s `int32` index, or `get_Count`'s `int32` return) uses that concrete type in *both*
//! positions — exactly as the hand-written wrappers did.

/// In a method-signature (`Sig`) position, `gen!(N)` denotes the class's `!N` generic parameter:
/// it expands to the def-shape marker `RustcCLRInteropTypeGeneric<N>`. Used only inside the
/// `Sig`-type slots of a [`dotnet_generic_impl!`] method line; the matching runtime value is passed
/// with its concrete Rust type.
#[macro_export]
macro_rules! gen {
    ($n:literal) => { $crate::intrinsics::RustcCLRInteropTypeGeneric<$n> };
}

/// Declares a type alias for a managed *generic* .NET instantiation handle.
///
/// ```ignore
/// dotnet_generic!(RustList<T> = ["System.Private.CoreLib"] "System.Collections.Generic.List" < (T,) >);
/// ```
///
/// expands to
///
/// ```ignore
/// type RustList<T> =
///     RustcCLRInteropManagedGeneric<{ "System.Private.CoreLib" }, { "System.Collections.Generic.List" }, (T,)>;
/// ```
///
/// `ASSEMBLY` must be the *implementation* assembly (a ref assembly forwards the type and throws
/// `TypeLoadException` at JIT); the trailing tuple is the *concrete* class generics in order.
#[macro_export]
macro_rules! dotnet_generic {
    (
        $alias:ident < $($cg:ident),+ $(,)? > = [ $asm:tt ] $class:tt < $cgtuple:ty >
    ) => {
        type $alias< $($cg),+ > =
            $crate::intrinsics::RustcCLRInteropManagedGeneric<{ $asm }, { $class }, $cgtuple>;
    };
}

/// Generates the wrapper free functions for methods/ctors on a managed generic instantiation,
/// replacing the hand-written `rustc_clr_interop_generic_*` turbofish.
///
/// The handle alias is given destructured into its three pieces — `[ASSEMBLY] "Class.Path"
/// <ClassGenerics>` — so the generated fns can spell both the alias (for handle args/returns) and
/// the raw-intrinsic generics.
///
/// Each method line is one of:
///
/// ```ignore
/// // constructor (newobj on the generic instantiation): returns the handle.
/// ctor fn new();
///
/// // instance method: an explicit .NET member name, `recv` (the receiver), then
/// // `name: ValTy as SigTy` value args, then an optional `-> Ret as SigRet`.
/// // `SigTy`/`SigRet` are `gen!(N)` for the class's `!N`, or a concrete .NET type. No `-> …` = void.
/// fn add = "Add"(recv, item: T as gen!(0));
/// fn get = "get_Item"(recv, idx: i32 as i32) -> T as gen!(0);
/// fn count = "get_Count"(recv) -> i32 as i32;
/// ```
///
/// `KIND` is fixed at `2` (callvirt) — every method here is an instance method on a reference-type
/// receiver (`List<T>`/`Dictionary<K,V>` are classes), matching the hand-written wrappers. The
/// ctor uses `rustc_clr_interop_generic_ctor0` with the canonical `((),)` `Sig`.
#[macro_export]
macro_rules! dotnet_generic_impl {
    (
        $alias:ident < $($cg:ident),+ $(,)? > = [ $asm:tt ] $class:tt < $cgtuple:ty > ;
        $( $method:tt )*
    ) => {
        $crate::__dotnet_generic_methods! {
            @alias $alias < $($cg),+ >
            @asm { $asm } @class { $class } @cg { $cgtuple }
            $( $method )*
        }
    };
}

/// Internal muncher: emits one wrapper fn per method line, then recurses on the rest.
///
/// `Sig`-type slots are captured as `:ty` so a `gen!(N)` macro-call (a valid type-position macro)
/// flows straight into the `Sig` tuple and expands there — no re-matching needed.
#[macro_export]
#[doc(hidden)]
macro_rules! __dotnet_generic_methods {
    // ---- base case: nothing left ----
    (
        @alias $alias:ident < $($cg:ident),+ >
        @asm { $asm:expr } @class { $class:expr } @cg { $cgtuple:ty }
    ) => {};

    // ---- ctor (newobj) -> handle ----
    (
        @alias $alias:ident < $($cg:ident),+ >
        @asm { $asm:expr } @class { $class:expr } @cg { $cgtuple:ty }
        ctor fn $fname:ident ();
        $( $rest:tt )*
    ) => {
        fn $fname< $($cg),+ >() -> $alias< $($cg),+ > {
            $crate::intrinsics::rustc_clr_interop_generic_ctor0::<
                { $asm }, { $class }, false, $cgtuple, ((),), $alias< $($cg),+ >,
            >()
        }
        $crate::__dotnet_generic_methods! {
            @alias $alias < $($cg),+ >
            @asm { $asm } @class { $class } @cg { $cgtuple }
            $( $rest )*
        }
    };

    // ---- instance method, 0 value args (receiver only): -> Ret ----
    (
        @alias $alias:ident < $($cg:ident),+ >
        @asm { $asm:expr } @class { $class:expr } @cg { $cgtuple:ty }
        fn $fname:ident = $mname:literal ( $recv:ident ) -> $rty:ty as $rsig:ty ;
        $( $rest:tt )*
    ) => {
        fn $fname< $($cg),+ >($recv: $alias< $($cg),+ >) -> $rty {
            $crate::intrinsics::rustc_clr_interop_generic_call1::<
                { $asm }, { $class }, false, $mname, 2,
                $cgtuple,
                ( $rsig, ),
                $rty,
                $alias< $($cg),+ >,
            >($recv)
        }
        $crate::__dotnet_generic_methods! {
            @alias $alias < $($cg),+ >
            @asm { $asm } @class { $class } @cg { $cgtuple }
            $( $rest )*
        }
    };

    // ---- instance method, 0 value args (receiver only), void return (e.g. `Clear()`) ----
    (
        @alias $alias:ident < $($cg:ident),+ >
        @asm { $asm:expr } @class { $class:expr } @cg { $cgtuple:ty }
        fn $fname:ident = $mname:literal ( $recv:ident );
        $( $rest:tt )*
    ) => {
        fn $fname< $($cg),+ >($recv: $alias< $($cg),+ >) {
            $crate::intrinsics::rustc_clr_interop_generic_call1::<
                { $asm }, { $class }, false, $mname, 2,
                $cgtuple,
                ( (), ),
                (),
                $alias< $($cg),+ >,
            >($recv)
        }
        $crate::__dotnet_generic_methods! {
            @alias $alias < $($cg),+ >
            @asm { $asm } @class { $class } @cg { $cgtuple }
            $( $rest )*
        }
    };

    // ---- instance method, 1 value arg: -> Ret ----
    (
        @alias $alias:ident < $($cg:ident),+ >
        @asm { $asm:expr } @class { $class:expr } @cg { $cgtuple:ty }
        fn $fname:ident = $mname:literal ( $recv:ident , $a1:ident : $a1ty:ty as $a1sig:ty ) -> $rty:ty as $rsig:ty ;
        $( $rest:tt )*
    ) => {
        fn $fname< $($cg),+ >($recv: $alias< $($cg),+ >, $a1: $a1ty) -> $rty {
            $crate::intrinsics::rustc_clr_interop_generic_call2::<
                { $asm }, { $class }, false, $mname, 2,
                $cgtuple,
                ( $rsig, $a1sig ),
                $rty,
                $alias< $($cg),+ >,
                $a1ty,
            >($recv, $a1)
        }
        $crate::__dotnet_generic_methods! {
            @alias $alias < $($cg),+ >
            @asm { $asm } @class { $class } @cg { $cgtuple }
            $( $rest )*
        }
    };

    // ---- instance method, 1 value arg, void return ----
    (
        @alias $alias:ident < $($cg:ident),+ >
        @asm { $asm:expr } @class { $class:expr } @cg { $cgtuple:ty }
        fn $fname:ident = $mname:literal ( $recv:ident , $a1:ident : $a1ty:ty as $a1sig:ty );
        $( $rest:tt )*
    ) => {
        fn $fname< $($cg),+ >($recv: $alias< $($cg),+ >, $a1: $a1ty) {
            $crate::intrinsics::rustc_clr_interop_generic_call2::<
                { $asm }, { $class }, false, $mname, 2,
                $cgtuple,
                ( (), $a1sig ),
                (),
                $alias< $($cg),+ >,
                $a1ty,
            >($recv, $a1)
        }
        $crate::__dotnet_generic_methods! {
            @alias $alias < $($cg),+ >
            @asm { $asm } @class { $class } @cg { $cgtuple }
            $( $rest )*
        }
    };

    // ---- instance method, 2 value args: -> Ret ----
    (
        @alias $alias:ident < $($cg:ident),+ >
        @asm { $asm:expr } @class { $class:expr } @cg { $cgtuple:ty }
        fn $fname:ident = $mname:literal ( $recv:ident , $a1:ident : $a1ty:ty as $a1sig:ty , $a2:ident : $a2ty:ty as $a2sig:ty ) -> $rty:ty as $rsig:ty ;
        $( $rest:tt )*
    ) => {
        fn $fname< $($cg),+ >($recv: $alias< $($cg),+ >, $a1: $a1ty, $a2: $a2ty) -> $rty {
            $crate::intrinsics::rustc_clr_interop_generic_call3::<
                { $asm }, { $class }, false, $mname, 2,
                $cgtuple,
                ( $rsig, $a1sig, $a2sig ),
                $rty,
                $alias< $($cg),+ >,
                $a1ty,
                $a2ty,
            >($recv, $a1, $a2)
        }
        $crate::__dotnet_generic_methods! {
            @alias $alias < $($cg),+ >
            @asm { $asm } @class { $class } @cg { $cgtuple }
            $( $rest )*
        }
    };

    // ---- instance method, 2 value args, void return ----
    (
        @alias $alias:ident < $($cg:ident),+ >
        @asm { $asm:expr } @class { $class:expr } @cg { $cgtuple:ty }
        fn $fname:ident = $mname:literal ( $recv:ident , $a1:ident : $a1ty:ty as $a1sig:ty , $a2:ident : $a2ty:ty as $a2sig:ty );
        $( $rest:tt )*
    ) => {
        fn $fname< $($cg),+ >($recv: $alias< $($cg),+ >, $a1: $a1ty, $a2: $a2ty) {
            $crate::intrinsics::rustc_clr_interop_generic_call3::<
                { $asm }, { $class }, false, $mname, 2,
                $cgtuple,
                ( (), $a1sig, $a2sig ),
                (),
                $alias< $($cg),+ >,
                $a1ty,
                $a2ty,
            >($recv, $a1, $a2)
        }
        $crate::__dotnet_generic_methods! {
            @alias $alias < $($cg),+ >
            @asm { $asm } @class { $class } @cg { $cgtuple }
            $( $rest )*
        }
    };
}
