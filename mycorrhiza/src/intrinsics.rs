use crate::ManagedSafe;

/// A handle to a managed reference type, identified *only* by its `(ASSEMBLY, CLASS_PATH)` const
/// generic parameters — there is no other field that distinguishes one instantiation from another.
///
/// **This means type identity is structural, not nominal.** Two `pub type` aliases in *different*
/// files — one hand-written (e.g. [`crate::system::MString`]), one `spinacz`-generated (e.g.
/// `bindings::System::String`) — are the *same Rust type* whenever they name the same
/// `(ASSEMBLY, CLASS_PATH)` pair, even though nothing textually links the two definitions. Before
/// writing a manual cast between two handle types, check whether their alias definitions already
/// resolve to identical const-generic arguments; if they do, they're interchangeable with **no**
/// conversion at all (not even `.into()` — a value of one *is* a value of the other), and any
/// `impl` on one (traits, inherent methods, `From`) applies to both.
///
/// **Prefer `.into()` over [`rustc_clr_interop_managed_checked_cast`]** for upcasts. `spinacz`
/// generates an `impl From<Derived> for Base` for every reflected type with a resolvable .NET base
/// class (see `bindings.rs`), so casting a bound/reflected managed value up its inheritance chain is
/// almost always already a `From` impl — grep `bindings.rs` for `impl From<` before reaching for the
/// low-level intrinsic. Reserve `rustc_clr_interop_managed_checked_cast` for downcasts (checked
/// `castclass`) or cross-hierarchy casts that have no generated `From` impl.
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

/// Reads a named field from a managed class or value type. The backend replaces this body with a
/// typed `ldfld`; the declaration exists only to carry the owner identity, field name, and Rust
/// return type through MIR.
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_get_field<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const FIELD: &'static str,
    Ret,
    Owner,
>(
    owner: Owner,
) -> Ret {
    core::intrinsics::abort();
}

impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str>
    RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
{
    #[inline(always)]
    pub fn ctor0() -> Self {
        rustc_clr_interop_managed_ctor0_::<ASSEMBLY, CLASS_PATH, false, Self>()
    }
    #[inline(always)]
    pub fn ctor1<Arg1>(arg1: Arg1) -> Self {
        rustc_clr_interop_managed_ctor1_::<ASSEMBLY, CLASS_PATH, false, Self, Arg1>(arg1)
    }
    #[inline(always)]
    pub fn ctor2<Arg1, Arg2>(arg1: Arg1, arg2: Arg2) -> Self {
        rustc_clr_interop_managed_ctor2_::<ASSEMBLY, CLASS_PATH, false, Self, Arg1, Arg2>(
            arg1, arg2,
        )
    }
    #[inline(always)]
    pub fn ctor3<Arg1, Arg2, Arg3>(arg1: Arg1, arg2: Arg2, arg3: Arg3) -> Self {
        rustc_clr_interop_managed_ctor3_::<ASSEMBLY, CLASS_PATH, false, Self, Arg1, Arg2, Arg3>(
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
    pub fn static3<const METHOD: &'static str, Arg1, Arg2, Arg3, Ret>(
        arg1: Arg1,
        arg2: Arg2,
        arg3: Arg3,
    ) -> Ret {
        rustc_clr_interop_managed_call3_::<
            ASSEMBLY,
            CLASS_PATH,
            false,
            METHOD,
            true,
            Ret,
            Arg1,
            Arg2,
            Arg3,
        >(arg1, arg2, arg3)
    }
    /// A four-explicit-arg **static** call. Added specifically for
    /// the dynamic invocation helpers (`Mycorrhiza.Reflection.DynamicInvoker.InvokeStatic` takes
    /// `(assemblyName, typeName, methodName, args)`) -- nothing else in the tree currently needs a
    /// static call past three explicit args, but the underlying `call4_` magic-fn family and the
    /// backend's `call_managed` dispatch are fully generic over arity (`argc_from_fn_name` reads the
    /// digit out of the name and the arg list is threaded straight from the actual call), so this is a
    /// mechanical extra rung, not a special case.
    #[inline(always)]
    pub fn static4<const METHOD: &'static str, Arg1, Arg2, Arg3, Arg4, Ret>(
        arg1: Arg1,
        arg2: Arg2,
        arg3: Arg3,
        arg4: Arg4,
    ) -> Ret {
        rustc_clr_interop_managed_call4_::<
            ASSEMBLY,
            CLASS_PATH,
            false,
            METHOD,
            true,
            Ret,
            Arg1,
            Arg2,
            Arg3,
            Arg4,
        >(arg1, arg2, arg3, arg4)
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
    pub fn virt1<const METHOD: &'static str, Arg1, Ret>(self, arg1: Arg1) -> Ret {
        rustc_clr_interop_managed_call_virt2_::<
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
    pub fn virt2<const METHOD: &'static str, Arg1, Arg2, Ret>(self, arg1: Arg1, arg2: Arg2) -> Ret {
        rustc_clr_interop_managed_call_virt3_::<
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

/// Idiomatic public spelling for a GC-owned one-dimensional CLR `T[]`.
///
/// Unlike `Vec<T>`, this remains managed storage and may safely contain managed references. Keep it
/// transient in synchronous Rust code; use `Memory<T>`/`ReadOnlyMemory<T>` when the buffer must be
/// retained or cross an async suspension.
pub type ManagedArray<T> = RustcCLRInteropManagedArray<T, 1>;
/// Arity-ladder generator for the interop "magic" function declarations.
///
/// Every member of the `rustc_clr_interop_managed_call*` / `_call_virt*` / `_ctor*` and WF-9
/// `rustc_clr_interop_generic_call*` / `_generic_ctor*` families is the same shape: a `pub fn` whose
/// body is `core::intrinsics::abort();` and which differs *only* by how many `ArgN` type params /
/// `argN` value params it declares. They are never actually run — the codegen backend recognizes them
/// by *name* (see `is_magic_fn` / the call-site dispatch in `src/terminator/call.rs`) and lowers
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
/// Loads `arr[idx]` as `T` (`ldelem T`). Unlike `ldelem.ref`, this supports primitive and managed
/// value-type arrays without pretending their elements are object references.
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_get_elem<T>(
    arr: RustcCLRInteropManagedArray<T, 1>,
    idx: i32,
) -> T {
    core::intrinsics::abort()
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
interop_magic_fn! {
    rustc_clr_interop_managed_call4_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const IS_STATIC: bool, Ret]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3, arg4: Arg4) -> Ret
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
interop_magic_fn! {
    rustc_clr_interop_managed_call_virt3_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const IS_STATIC: bool, Ret]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3) -> Ret
}
//Ctors
interop_magic_fn! {
    rustc_clr_interop_managed_ctor0_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, Ret]
    () -> Ret
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_ld_null<T>() -> T {
    core::intrinsics::abort();
}
/// Returns whether a managed reference is null. The backend emits a direct reference comparison;
/// no type-specific `op_Equality` member is required.
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_is_null<T>(value: T) -> bool {
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
/// Low-level checked cast (a CIL `castclass`) between two managed handle types — throws
/// `InvalidCastException` at runtime if `src` isn't actually an instance of `DST`.
///
/// Before calling this directly: if `DST` is a *base* of `SRC`'s reflected/bound type, `spinacz`
/// has almost certainly already generated `impl From<SRC> for DST` (see `bindings.rs`) — use
/// `src.into()` instead, it's the same cast with no turbofish required. See the type-identity note
/// on [`RustcCLRInteropManagedClass`] for when two differently-named handle types need no cast at
/// all. Reach for this function directly only for downcasts or hierarchy jumps with no `From` impl.
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
/// Boxes the value-type `val` into a managed `System.Object` (the .NET `box <T>` instruction). `T`
/// must be a value type (an integer/float/bool primitive or a value-type managed struct); boxing a
/// reference type is rejected by the CIL typechecker.
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_box<T>(
    val: T,
) -> RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Object"> {
    core::intrinsics::abort();
}
interop_magic_fn! {
    rustc_clr_interop_managed_ctor1_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, Ret]
    (arg1: Arg1) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_managed_ctor2_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, Ret]
    (arg1: Arg1, arg2: Arg2) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_managed_ctor3_
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, Ret]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3) -> Ret
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
> ManagedSafe for RustcCLRInteropManagedGenericStruct<ASSEMBLY, CLASS_PATH, SIZE, ClassGenerics>
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

/// Signature-shape marker: lowers to a managed byref `Inner&` (`Type::Ref`). Use it in a `Sig` return
/// slot for a byref-returning member — e.g. `Span<T>.get_Item(int) -> ref T` is
/// `RustcCLRInteropByRef<RustcCLRInteropTypeGeneric<0>>` (`!0&`). The matching runtime value is a raw
/// pointer (`*mut Inner`), read/written through directly.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct RustcCLRInteropByRef<Inner> {
    _pd: core::marker::PhantomData<Inner>,
}

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
    rustc_clr_interop_generic_call4
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, Sig, Ret]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3, arg4: Arg4) -> Ret
}

// Generic-METHOD calls (`!!N`): a method that itself takes type arguments (e.g.
// `Activator.CreateInstance<T>()`, `Deserialize<T>(s)`, `GetService<T>()`). Identical to the
// `generic_call` family plus a `MethodGenerics` tuple (the method's concrete type args) after
// `ClassGenerics`. `!N` in `Sig` still refers to the CLASS generics; `!!N` refers to these method
// generics. Name carries no `_` after `call` (the backend reads arity from the call's arg count).
interop_magic_fn! {
    rustc_clr_interop_generic_method_call0
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, MethodGenerics, Sig, Ret]
    () -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_method_call1
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, MethodGenerics, Sig, Ret]
    (arg1: Arg1) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_method_call2
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, MethodGenerics, Sig, Ret]
    (arg1: Arg1, arg2: Arg2) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_method_call3
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, MethodGenerics, Sig, Ret]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3) -> Ret
}
interop_magic_fn! {
    rustc_clr_interop_generic_method_call4
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, MethodGenerics, Sig, Ret]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3, arg4: Arg4) -> Ret
}
// `Queryable.Join<TOuter,TInner,TKey,TResult>(outer, inner, outerKeySelector, innerKeySelector,
// resultSelector)` is the motivating case for this arity rung — the first WF-9 generic-method call
// site with more than 2 runtime arguments (`mycorrhiza::linq::join`). `call_gmethod`
// (`src/terminator/call.rs`) already reads argument count from the actual call args / `Sig` tuple
// length rather than from this fn name's digit, so no backend change was needed — only this
// mechanical arity-ladder rung.
interop_magic_fn! {
    rustc_clr_interop_generic_method_call5
    [const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const IS_VALUETYPE: bool, const METHOD: &'static str, const KIND: u8, ClassGenerics, MethodGenerics, Sig, Ret]
    (arg1: Arg1, arg2: Arg2, arg3: Arg3, arg4: Arg4, arg5: Arg5) -> Ret
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
/// `{ASSEMBLY}{CLASS_PATH}<ClassGenerics..>` (e.g. `System.Func<i32, bool>`). `Ret` is the Rust-side
/// managed-handle marker for that exact delegate: normally [`RustcCLRInteropManagedGeneric`], or a
/// generated concrete handle for a non-generic delegate such as `System.EventHandler`. The backend
/// returns the actual managed delegate reference, which can be passed to a .NET method or invoked.
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_delegate<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    ClassGenerics,
    Sig,
    FnPtrTy,
    Ret,
>(
    f: FnPtrTy,
) -> Ret {
    core::intrinsics::abort();
}

/// Wraps a **capturing** closure into a managed delegate: `env` is a boxed-environment pointer and
/// `trampoline` an `extern "C" fn(env, In..) -> CallbackRet` that reconstructs and calls it. The
/// backend synthesises a shim holding both, whose `Invoke` prepends `env` before the `calli`, so the
/// captured state rides along on the .NET side. Generic layout mirrors [`rustc_clr_interop_delegate`]
/// with an extra `EnvTy` before the fn-ptr type; its `Ret` is the exact managed-delegate handle
/// marker described there.
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_delegate_closure<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    ClassGenerics,
    Sig,
    EnvTy,
    FnPtrTy,
    Ret,
>(
    env: EnvTy,
    trampoline: FnPtrTy,
) -> Ret {
    core::intrinsics::abort();
}

impl RustcCLRInteropManagedChar {
    /// The raw UTF-16 code unit (`System.Char` is a 16-bit value). Inverse of the `From<u16>` impl.
    #[inline(always)]
    pub fn as_u16(self) -> u16 {
        unsafe {
            core::mem::transmute::<RustcCLRInteropManagedChar, u16>(core::intrinsics::black_box(
                self,
            ))
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

    /// Allocate a GC-owned one-dimensional managed array and copy `slice` into it.
    ///
    /// This is the ordinary boundary helper for generated .NET APIs that accept `T[]`.
    /// `T` must be a boundary-safe primitive, value type, or managed-reference handle.
    pub fn from_slice(slice: &[T]) -> Self
    where
        T: Copy + ManagedSafe,
    {
        let len = i32::try_from(slice.len()).expect("managed array length exceeds i32");
        let array = rustc_clr_interop_managed_new_arr::<T>(len);
        for (idx, value) in slice.iter().copied().enumerate() {
            rustc_clr_interop_managed_set_elem(array, idx as i32, value);
        }
        array
    }

    /// Replace one element of this one-dimensional managed array.
    pub fn set(self, index: i32, value: T)
    where
        T: ManagedSafe,
    {
        rustc_clr_interop_managed_set_elem(self, index, value);
    }

    /// Copy one value from this one-dimensional managed array.
    pub fn get(self, index: i32) -> T
    where
        T: Copy + ManagedSafe,
    {
        rustc_clr_interop_managed_get_elem(self, index)
    }
}

impl RustcCLRInteropManagedArray<u8, 1> {
    /// Encode UTF-8 bytes into a GC-owned `System.Byte[]`.
    pub fn from_utf8(value: &str) -> Self {
        Self::from_slice(value.as_bytes())
    }

    /// Decode this `System.Byte[]` with `System.Text.Encoding.UTF8`.
    pub fn to_utf8_string(self) -> String {
        let encoding = crate::bindings::System::Text::Encoding::get_utf8();
        let managed = encoding.instance1::<"GetString", Self, crate::system::MString>(self);
        String::from(crate::system::DotNetString::from_handle(managed))
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
    /// Construct a managed value type through its parameterless `.ctor`.
    #[inline(always)]
    pub fn ctor0() -> Self {
        rustc_clr_interop_managed_ctor0_::<ASSEMBLY, CLASS_PATH, true, Self>()
    }

    /// Construct a managed value type through a one-argument `.ctor`.
    #[inline(always)]
    pub fn ctor1<Arg1>(arg1: Arg1) -> Self {
        rustc_clr_interop_managed_ctor1_::<ASSEMBLY, CLASS_PATH, true, Self, Arg1>(arg1)
    }

    /// Construct a managed value type through a two-argument `.ctor`.
    #[inline(always)]
    pub fn ctor2<Arg1, Arg2>(arg1: Arg1, arg2: Arg2) -> Self {
        rustc_clr_interop_managed_ctor2_::<ASSEMBLY, CLASS_PATH, true, Self, Arg1, Arg2>(arg1, arg2)
    }

    /// Construct a managed value type through a three-argument `.ctor`.
    #[inline(always)]
    pub fn ctor3<Arg1, Arg2, Arg3>(arg1: Arg1, arg2: Arg2, arg3: Arg3) -> Self {
        rustc_clr_interop_managed_ctor3_::<ASSEMBLY, CLASS_PATH, true, Self, Arg1, Arg2, Arg3>(
            arg1, arg2, arg3,
        )
    }

    /// Reads one field directly from an unboxed managed value type.
    #[inline(always)]
    pub fn vt_field<const FIELD: &'static str, Ret>(self) -> Ret {
        rustc_clr_interop_managed_get_field::<ASSEMBLY, CLASS_PATH, true, FIELD, Ret, &Self>(&self)
    }

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

    /// A two-arg value-type **static** call — used for the operator methods of arithmetic value types
    /// (`Decimal.op_Addition(Decimal, Decimal)`, `Decimal.Compare(Decimal, Decimal)`, …).
    #[inline(always)]
    pub fn vt_static2<const METHOD: &'static str, Arg1, Arg2, Ret>(arg1: Arg1, arg2: Arg2) -> Ret {
        rustc_clr_interop_managed_call2_::<ASSEMBLY, CLASS_PATH, true, METHOD, true, Ret, Arg1, Arg2>(
            arg1, arg2,
        )
    }
}
