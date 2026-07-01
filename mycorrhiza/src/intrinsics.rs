use std::ptr::null;

use crate::ManagedSafe;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct RustcCLRInteropManagedClass<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str>
{
    size_hint: usize,
}
#[derive(Clone, Copy)]
#[repr(C)]
pub struct RustcCLRInteropManagedStruct<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const SIZE: usize,
> {
    size_hint: [u8; SIZE],
}

impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str>
    RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
{
    #[inline(always)]
    pub fn ctor0() -> Self {
        rustc_clr_interop_managed_ctor0_::<ASSEMBLY, CLASS_PATH, false>()
    }
    #[inline(always)]
    pub fn ctor1<Arg1>(arg1: Arg1) -> Self {
        rustc_clr_interop_managed_ctor1_::<ASSEMBLY, CLASS_PATH, false, Arg1>(arg1)
    }
    #[inline(always)]
    pub fn ctor2<Arg1, Arg2>(arg1: Arg1, arg2: Arg2) -> Self {
        rustc_clr_interop_managed_ctor2_::<ASSEMBLY, CLASS_PATH, false, Arg1, Arg2>(arg1, arg2)
    }
    #[inline(always)]
    pub fn ctor3<Arg1, Arg2, Arg3>(arg1: Arg1, arg2: Arg2, arg3: Arg3) -> Self {
        rustc_clr_interop_managed_ctor3_::<ASSEMBLY, CLASS_PATH, false, Arg1, Arg2, Arg3>(
            arg1, arg2, arg3,
        )
    }
    #[inline(always)]
    pub fn static0<const METHOD: &'static str, Ret>() -> Ret {
        rustc_clr_interop_managed_call0_::<ASSEMBLY, CLASS_PATH, false, METHOD, Ret>()
    }
    #[inline(always)]
    pub fn instance0<const METHOD: &'static str, Ret>(self) -> Ret {
        rustc_clr_interop_managed_call1_::<ASSEMBLY, CLASS_PATH, false, METHOD, false, Ret, Self>(
            self,
        )
    }
    #[inline(always)]
    pub fn virt0<const METHOD: &'static str, Ret>(self) -> Ret {
        rustc_clr_interop_managed_call_virt1_::<ASSEMBLY, CLASS_PATH, false, METHOD, false, Ret, Self>(
            self,
        )
    }
    #[inline(always)]
    pub fn static1<const METHOD: &'static str, Arg1, Ret>(arg1: Arg1) -> Ret {
        rustc_clr_interop_managed_call1_::<ASSEMBLY, CLASS_PATH, false, METHOD, true, Ret, Arg1>(
            arg1,
        )
    }
    #[inline(always)]
    pub fn static2<const METHOD: &'static str, Arg1, Arg2, Ret>(arg1: Arg1, arg2: Arg2) -> Ret {
        rustc_clr_interop_managed_call2_::<ASSEMBLY, CLASS_PATH, false, METHOD, true, Ret, Arg1, Arg2>(
            arg1, arg2,
        )
    }
    #[inline(always)]
    pub fn instance1<const METHOD: &'static str, Arg1, Ret>(self, arg1: Arg1) -> Ret {
        rustc_clr_interop_managed_call2_::<
            ASSEMBLY,
            CLASS_PATH,
            false,
            METHOD,
            false,
            Ret,
            Self,
            Arg1,
        >(self, arg1)
    }
    #[inline(always)]
    pub fn instance2<const METHOD: &'static str, Arg1, Arg2, Ret>(
        self,
        arg1: Arg1,
        arg2: Arg2,
    ) -> Ret {
        rustc_clr_interop_managed_call3_::<
            ASSEMBLY,
            CLASS_PATH,
            false,
            METHOD,
            false,
            Ret,
            Self,
            Arg1,
            Arg2,
        >(self, arg1, arg2)
    }
    #[inline(always)]
    pub fn to_mstring(self) -> crate::system::MString {
        self.instance0::<"ToString", crate::system::MString>()
    }
    #[inline(always)]
    pub fn equality(self, other: Self) -> bool {
        Self::static2::<"op_Equality", Self, Self, bool>(self, other)
    }
    #[inline(always)]
    pub fn null() -> Self {
        rustc_clr_interop_managed_ld_null::<Self>()
    }
    pub fn is_null(self) -> bool {
        self.equality(Self::null())
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct RustcCLRInteropManagedChar {
    utf16_char: u16,
}
#[derive(Clone, Copy)]
#[repr(C)]
pub struct RustcCLRInteropManagedArray<T, const DIMENSIONS: usize> {
    object_ref: usize,
    pd: core::marker::PhantomData<T>,
}
/// Arity-ladder generator for the interop "magic" function declarations.
///
/// Every member of the `rustc_clr_interop_managed_call*` / `_call_virt*` / `_ctor*` and WF-9
/// `rustc_clr_interop_generic_call*` / `_generic_ctor*` families is the same shape: a `pub fn` whose
/// body is `core::intrinsics::abort();` and which differs *only* by how many `ArgN` type params /
/// `argN` value params it declares. They are never actually run — the codegen backend recognizes them
/// by *name* (see `is_function_magic` / the call-site dispatch in `src/terminator/call.rs`) and lowers
/// the call directly to a managed `call`/`callvirt`/`newobj`/etc.
///
/// **The function name is load-bearing and must be byte-identical.** The backend matches on name
/// substrings (`rustc_clr_interop_managed_call`, …) and parses the arity digit + the trailing `_`
/// (`argc_from_fn_name`). So each invocation spells the *literal* `ident` name — including the arity
/// digit and any trailing underscore — rather than building it from `concat!`/`paste!`; the macro only
/// factors out the repeated `#[allow]/#[inline(never)]` attributes, the common generic-param prefix,
/// and the `arg`-ladder body.
macro_rules! interop_magic_fn {
    (
        $name:ident
        [ $($prefix:tt)* ]
        ( $($arg:ident : $argty:ident),* $(,)? )
        -> $ret:ty
    ) => {
        #[allow(unused_variables)]
        #[inline(never)]
        pub fn $name< $($prefix)* $(, $argty)* >( $($arg : $argty),* ) -> $ret {
            core::intrinsics::abort();
        }
    };
}

//Calls
interop_magic_fn! {
    rustc_clr_interop_managed_call0_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, Ret]
    () -> Ret
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_ld_len<T>(arr: RustcCLRInteropManagedArray<T, 1>) -> i32 {
    core::intrinsics::abort();
}
/// Allocates a new managed (.NET) 1-D array of `T` with `len` elements (`newarr`). The element type
/// `T` must be a primitive that maps to a .NET primitive (e.g. `i32`/`i64`/`f64`).
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_new_arr<T>(len: i32) -> RustcCLRInteropManagedArray<T, 1> {
    core::intrinsics::abort();
}
/// Stores `val` into the managed array `arr` at `idx` (`stelem`).
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_set_elem<T>(
    arr: RustcCLRInteropManagedArray<T, 1>,
    idx: i32,
    val: T,
) {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_ld_elem_ref<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
>(
    arr: RustcCLRInteropManagedArray<RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>, 1>,
    idx: i32,
) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH> {
    core::intrinsics::abort();
}
interop_magic_fn! {
    rustc_clr_interop_managed_call1_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const IS_STATIC: bool, Ret]
    (arg1: Arg1) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_managed_call2_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const IS_STATIC: bool, Ret]
    (arg1: Arg1, arg2: Arg2) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_managed_call3_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const IS_STATIC: bool, Ret]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3) -> Ret
}
//VCalls
interop_magic_fn! {
    rustc_clr_interop_managed_call_virt0_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, Ret]
    () -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_managed_call_virt1_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const IS_STATIC: bool, Ret]
    (arg1: Arg1) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_managed_call_virt2_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const IS_STATIC: bool, Ret]
    (arg1: Arg1, arg2: Arg2) -> Ret
}
//Ctors
interop_magic_fn! {
    rustc_clr_interop_managed_ctor0_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool]
    () -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_ld_null<T>() -> T {
    core::intrinsics::abort();
}
/// Raises a managed `System.Exception(MSG)` directly (a `throw` IL op), so a .NET caller can `catch`
/// it. This is the C#-catchable error direction — unlike a Rust `panic!`, which goes through the
/// unwinder and does not propagate cleanly out to a managed frame.
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_throw<const MSG: &'static str>() -> ! {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_checked_cast<DST, SRC>(src: SRC) -> DST {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_is_inst<DST, SRC>(src: SRC) -> bool {
    core::intrinsics::abort();
}
interop_magic_fn! {
    rustc_clr_interop_managed_ctor1_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool]
    (arg1: Arg1) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
}
interop_magic_fn! {
    rustc_clr_interop_managed_ctor2_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool]
    (arg1: Arg1, arg2: Arg2) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
}
interop_magic_fn! {
    rustc_clr_interop_managed_ctor3_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
}
impl From<u16> for RustcCLRInteropManagedChar {
    fn from(utf16_char: u16) -> RustcCLRInteropManagedChar {
        unsafe {
            core::mem::transmute::<u16, RustcCLRInteropManagedChar>(core::intrinsics::black_box(
                utf16_char,
            ))
        }
    }
}

// ===========================================================================================
// WF-9 — generic interop bridge (Rust → generic .NET): calling methods on generic .NET
// instantiations such as `List<i32>` / `Dictionary<K, V>`.
//
// A method reference on a generic instantiation must use the *definition* signature shape
// (`!0` for the class's first generic, `!!0` for the method's first generic), NOT the
// instantiated one — `List<int32>::Add(!0)`, never `Add(int32)`. So each call carries two extra
// pieces of type information beyond the existing managed-call family:
//   * `ClassGenerics`: a tuple of the *concrete* .NET type arguments for the class instantiation
//     (e.g. `(i32,)` for `List<i32>`), threaded onto the `ClassRef`.
//   * `Sig`: a tuple `(Output, In0, In1, …)` describing the method in *definition* shape, using
//     the `DotnetTypeGeneric<N>` / `DotnetMethodGeneric<N>` markers for `!N` / `!!N` and concrete
//     types elsewhere. (Receiver excluded — it is implied by `instance`/`callvirt`.)
// The runtime argument values are passed normally (concrete); the JIT binds `!N` to the class
// instantiation.
// ===========================================================================================

/// A handle to a managed object of a *generic* .NET instantiation, e.g. `List<i32>`.
/// `ASSEMBLY`/`CLASS_PATH` name the open generic type and `ClassGenerics` is a tuple of the
/// concrete .NET type arguments. Lowers to a `ClassRef` carrying those generics.
#[repr(C)]
pub struct RustcCLRInteropManagedGeneric<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    ClassGenerics,
> {
    object_ref: usize,
    pd: core::marker::PhantomData<ClassGenerics>,
}
// `Clone`/`Copy` are UNCONDITIONAL: the handle is just a managed object reference (a pointer-sized
// `object_ref` + a zero-sized `PhantomData`), so it is always bit-copyable regardless of whether the
// element type is `Copy`. `#[derive(Copy)]` would wrongly require `ClassGenerics: Copy`, which forces
// a spurious `T: Copy` bound on every wrapper built over the handle (e.g. `collections::List<T>`).
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, ClassGenerics> Clone
    for RustcCLRInteropManagedGeneric<ASSEMBLY, CLASS_PATH, ClassGenerics>
{
    fn clone(&self) -> Self {
        *self
    }
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, ClassGenerics> Copy
    for RustcCLRInteropManagedGeneric<ASSEMBLY, CLASS_PATH, ClassGenerics>
{
}

/// A managed **value type** of a *generic* .NET instantiation, e.g. `Nullable<JsonNodeOptions>` or
/// `Span<i32>`. This is the value-type counterpart to [`RustcCLRInteropManagedGeneric`] (a reference
/// type) and the generic counterpart to [`RustcCLRInteropManagedStruct`] (a non-generic value type):
/// it lowers to a `ClassRef` that is **both** a value type **and** carries concrete generic
/// arguments — the combination neither of the other two markers can express.
///
/// `ASSEMBLY`/`CLASS_PATH` name the open generic value type, `SIZE` is its byte size (used only for
/// Rust-side layout — the CLR knows the real size), and `ClassGenerics` is a tuple of the concrete
/// .NET type arguments (e.g. `(JsonNodeOptions,)`).
#[repr(C)]
pub struct RustcCLRInteropManagedGenericStruct<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const SIZE: usize,
    ClassGenerics,
> {
    size_hint: [u8; SIZE],
    pd: core::marker::PhantomData<ClassGenerics>,
}
// `Clone`/`Copy` are UNCONDITIONAL (as for `RustcCLRInteropManagedGeneric`): the handle is a plain
// byte buffer + a zero-sized `PhantomData`, always bit-copyable regardless of whether `ClassGenerics`
// is `Copy`. A `#[derive(Copy)]` would wrongly demand `ClassGenerics: Copy`.
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const SIZE: usize, ClassGenerics>
    Clone for RustcCLRInteropManagedGenericStruct<ASSEMBLY, CLASS_PATH, SIZE, ClassGenerics>
{
    fn clone(&self) -> Self {
        *self
    }
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const SIZE: usize, ClassGenerics>
    Copy for RustcCLRInteropManagedGenericStruct<ASSEMBLY, CLASS_PATH, SIZE, ClassGenerics>
{
}
unsafe impl<
        const ASSEMBLY: &'static str,
        const CLASS_PATH: &'static str,
        const SIZE: usize,
        ClassGenerics,
    > ManagedSafe
    for RustcCLRInteropManagedGenericStruct<ASSEMBLY, CLASS_PATH, SIZE, ClassGenerics>
{
}

/// Method-definition-signature marker: lowers to the .NET *class* generic parameter `!N`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct RustcCLRInteropTypeGeneric<const N: usize>;

/// Method-definition-signature marker: lowers to the .NET *method* generic parameter `!!N`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct RustcCLRInteropMethodGeneric<const N: usize>;

unsafe impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, ClassGenerics> ManagedSafe
    for RustcCLRInteropManagedGeneric<ASSEMBLY, CLASS_PATH, ClassGenerics>
{
}

// `KIND` for `rustc_clr_interop_generic_call*`: 0 = static, 1 = instance (`call instance`),
// 2 = virtual (`callvirt`).
//
// NOTE: the generic family names carry NO trailing `_` (unlike the `managed_*` family). The backend
// dispatches on the `rustc_clr_interop_generic_call` / `_generic_ctor` substring and reads the arity
// from the call's argument count rather than from the name, but the spelled name must still be exact.
interop_magic_fn! {
    rustc_clr_interop_generic_call0
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, Sig, Ret]
    () -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_call1
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, Sig, Ret]
    (arg1: Arg1) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_call2
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, Sig, Ret]
    (arg1: Arg1, arg2: Arg2) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_call3
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, Sig, Ret]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_ctor0
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, ClassGenerics, Sig, Ret]
    () -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_ctor1
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, ClassGenerics, Sig, Ret]
    (arg1: Arg1) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_ctor2
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, ClassGenerics, Sig, Ret]
    (arg1: Arg1, arg2: Arg2) -> Ret
}

// ===========================================================================================
// Delegates & callbacks (Rust → .NET): wrap a Rust `extern` fn pointer into a managed delegate
// instance (`Action<..>` / `Func<.., R>`), so a Rust callback can be handed to any .NET method
// that takes a delegate (a sort comparator, `List.ForEach`, a LINQ predicate, an event `add_*`).
//
// The pointer arrives as a plain native `FnPtr` (a capture-less closure / `fn` item is coerced to
// one before the call), so the backend synthesises a small managed *shim* class holding the pointer
// whose `Invoke` `calli`s it, then `newobj`s the real generic delegate over `ldftn shim::Invoke`.
// The generic-argument layout mirrors the WF-9 generic family:
//   * `ClassGenerics`: a tuple of the *concrete* .NET type args of the delegate instantiation
//     (e.g. `(i32, bool)` for `Func<i32, bool>`; `(i32,)` for `Action<i32>`).
//   * `Sig`: a tuple `(Ret, In0, In1, …)` giving the *concrete* signature the pointer is invoked
//     with — this is the shim's `calli` signature and equals the delegate's instantiated `Invoke`.
//     For an `Action` (void), the return slot is `()`; a parameterless delegate is `((),)`.
// ===========================================================================================

/// Wraps the native fn pointer `f` into a managed delegate of the instantiation
/// `{ASSEMBLY}{CLASS_PATH}<ClassGenerics..>` (e.g. `System.Func`2<i32,bool>`). Returns the delegate
/// as a [`RustcCLRInteropManagedGeneric`] handle (a managed object reference), which can be passed to
/// any .NET method taking that delegate type, or invoked via the generic bridge's `Invoke` wrapper.
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_delegate<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    ClassGenerics,
    Sig,
    FnPtrTy,
>(
    f: FnPtrTy,
) -> RustcCLRInteropManagedGeneric<ASSEMBLY, CLASS_PATH, ClassGenerics> {
    core::intrinsics::abort();
}

impl RustcCLRInteropManagedChar {
    /// The raw UTF-16 code unit (`System.Char` is a 16-bit value). Inverse of the `From<u16>` impl.
    #[inline(always)]
    pub fn as_u16(self) -> u16 {
        unsafe {
            core::mem::transmute::<RustcCLRInteropManagedChar, u16>(core::intrinsics::black_box(self))
        }
    }
    pub fn single_codepoint_unchecked(value: char) -> Self {
        let byte1 = (value as u64) & 0xFF;
        if (byte1 & 0x80) == 0x00 {
            //1 byte long char
            let utf16 = (byte1 & 0x7F) as u16;
            utf16.into()
        } else if (byte1 & 0xE0) == 0xC0 {
            //2 byte long char
            let byte2 = ((value as u64) & 0x00FF) >> 8;
            let utf16 = (((byte1 & 0x1F) << 6) | (byte2 & 0x3F)) as u16;
            utf16.into()
        } else if (byte1 & 0xF0) == 0xE0 {
            //3 byte long char
            let byte2 = ((value as u64) & 0x00FF) >> 8;
            let byte3 = ((value as u64) & 0x0000FF) >> 16;
            let utf16 = (((byte1 & 0x0F) << 12) | ((byte2 & 0x3F) << 6) | (byte3 & 0x3F)) as u16;
            utf16.into()
        } else if (byte1 & 0xF8) == 0xF0 {
            //4 byte long char
            0xFFFD.into()
        } else {
            //Invalid utf8.
            0xFFFD.into()
        }
    }
}
impl<T> RustcCLRInteropManagedArray<T, 1> {
    /// Gets the length of this managed array
    pub fn len(self) -> i32 {
        rustc_clr_interop_managed_ld_len(self)
    }
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str>
    RustcCLRInteropManagedArray<RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>, 1>
{
    pub fn index(self, index: i32) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH> {
        rustc_clr_interop_managed_ld_elem_ref(self, index)
    }
}
unsafe impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str> ManagedSafe
    for RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
{
}
unsafe impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const SIZE: usize>
    ManagedSafe for RustcCLRInteropManagedStruct<ASSEMBLY, CLASS_PATH, SIZE>
{
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const SIZE: usize>
    RustcCLRInteropManagedStruct<ASSEMBLY, CLASS_PATH, SIZE>
{
    #[inline(always)]
    pub fn instance0<const METHOD: &'static str, Ret>(self) -> Ret {
        rustc_clr_interop_managed_call1_::<ASSEMBLY, CLASS_PATH, false, METHOD, false, Ret, &Self>(
            &self,
        )
    }
    #[inline(always)]
    pub fn static1<const METHOD: &'static str, Arg1, Ret>(arg1: Arg1) -> Ret {
        rustc_clr_interop_managed_call1_::<ASSEMBLY, CLASS_PATH, false, METHOD, true, Ret, Arg1>(
            arg1,
        )
    }

    // ---- value-type-correct instance calls (`call instance` on a `valuetype` receiver) -------------
    //
    // A .NET **value type**'s instance method is a non-virtual slot and MUST be reached with
    // `call instance` on the unboxed `valuetype` receiver — emitting `callvirt` (or referencing the
    // receiver as a reference-type `class`) is invalid for a value type and makes the CLR reject the
    // type with a `TypeLoadException: ... due to value type mismatch`. The plain `instance0` above
    // passes `IS_VALUETYPE = false` (it predates the value-type BCL wrappers and is kept for the
    // GCHandle path), which lowers to `callvirt`/`class`. These `vt_*` variants pass
    // `IS_VALUETYPE = true`, so the backend emits `call instance` against the `valuetype` — the
    // correct dispatch for `DateTime`/`TimeSpan`/`Guid` and every other real BCL value type. The
    // receiver is passed by managed reference (`&self`), which is how a `call instance` on a value
    // type takes its `this`.

    /// A zero-explicit-arg value-type instance call (a getter/`ToString`/etc.) — `call instance`.
    #[inline(always)]
    pub fn vt_instance0<const METHOD: &'static str, Ret>(self) -> Ret {
        rustc_clr_interop_managed_call1_::<ASSEMBLY, CLASS_PATH, true, METHOD, false, Ret, &Self>(
            &self,
        )
    }

    /// A one-explicit-arg value-type instance call — `call instance`, receiver by `&self`.
    #[inline(always)]
    pub fn vt_instance1<const METHOD: &'static str, Arg1, Ret>(self, arg1: Arg1) -> Ret {
        rustc_clr_interop_managed_call2_::<
            ASSEMBLY,
            CLASS_PATH,
            true,
            METHOD,
            false,
            Ret,
            &Self,
            Arg1,
        >(&self, arg1)
    }

    /// A zero-arg value-type **static** call/factory (e.g. `TimeSpan.FromTicks` is `static1`; a
    /// static *property* getter like `get_Zero`/`get_MinValue` is this) — `call` on the `valuetype`.
    #[inline(always)]
    pub fn vt_static0<const METHOD: &'static str, Ret>() -> Ret {
        rustc_clr_interop_managed_call0_::<ASSEMBLY, CLASS_PATH, true, METHOD, Ret>()
    }

    /// A one-arg value-type **static** factory (e.g. `TimeSpan.FromTicks`/`Guid.Parse`) — `call` on
    /// the `valuetype`, returning a fresh value.
    #[inline(always)]
    pub fn vt_static1<const METHOD: &'static str, Arg1, Ret>(arg1: Arg1) -> Ret {
        rustc_clr_interop_managed_call1_::<ASSEMBLY, CLASS_PATH, true, METHOD, true, Ret, Arg1>(
            arg1,
        )
    }
}
