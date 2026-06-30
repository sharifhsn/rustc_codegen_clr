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
//Calls
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_call0_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    Ret,
>() -> Ret {
    core::intrinsics::abort();
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
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_call1_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    const IS_STATIC: bool,
    Ret,
    Arg1,
>(
    arg1: Arg1,
) -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_call2_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    const IS_STATIC: bool,
    Ret,
    Arg1,
    Arg2,
>(
    arg1: Arg1,
    arg2: Arg2,
) -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_call3_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    const IS_STATIC: bool,
    Ret,
    Arg1,
    Arg2,
    Arg3,
>(
    arg1: Arg1,
    arg2: Arg2,
    arg3: Arg3,
) -> Ret {
    core::intrinsics::abort();
}
//VCalls
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_call_virt0_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    Ret,
>() -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_call_virt1_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    const IS_STATIC: bool,
    Ret,
    Arg1,
>(
    arg1: Arg1,
) -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_call_virt2_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    const IS_STATIC: bool,
    Ret,
    Arg1,
    Arg2,
>(
    arg1: Arg1,
    arg2: Arg2,
) -> Ret {
    core::intrinsics::abort();
}
//Ctors
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_ctor0_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
>() -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH> {
    core::intrinsics::abort();
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
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_ctor1_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    Arg1,
>(
    arg1: Arg1,
) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH> {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_ctor2_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    Arg1,
    Arg2,
>(
    arg1: Arg1,
    arg2: Arg2,
) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH> {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_managed_ctor3_<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    Arg1,
    Arg2,
    Arg3,
>(
    arg1: Arg1,
    arg2: Arg2,
    arg3: Arg3,
) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH> {
    core::intrinsics::abort();
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
#[derive(Clone, Copy)]
#[repr(C)]
pub struct RustcCLRInteropManagedGeneric<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    ClassGenerics,
> {
    object_ref: usize,
    pd: core::marker::PhantomData<ClassGenerics>,
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
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_generic_call0<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    const KIND: u8,
    ClassGenerics,
    Sig,
    Ret,
>() -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_generic_call1<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    const KIND: u8,
    ClassGenerics,
    Sig,
    Ret,
    Arg1,
>(
    arg1: Arg1,
) -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_generic_call2<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    const KIND: u8,
    ClassGenerics,
    Sig,
    Ret,
    Arg1,
    Arg2,
>(
    arg1: Arg1,
    arg2: Arg2,
) -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_generic_call3<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    const METHOD: &'static str,
    const KIND: u8,
    ClassGenerics,
    Sig,
    Ret,
    Arg1,
    Arg2,
    Arg3,
>(
    arg1: Arg1,
    arg2: Arg2,
    arg3: Arg3,
) -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_generic_ctor0<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    ClassGenerics,
    Sig,
    Ret,
>() -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_generic_ctor1<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    ClassGenerics,
    Sig,
    Ret,
    Arg1,
>(
    arg1: Arg1,
) -> Ret {
    core::intrinsics::abort();
}
#[allow(unused_variables)]
#[inline(never)]
pub fn rustc_clr_interop_generic_ctor2<
    const ASSEMBLY: &'static str,
    const CLASS_PATH: &'static str,
    const IS_VALUETYPE: bool,
    ClassGenerics,
    Sig,
    Ret,
    Arg1,
    Arg2,
>(
    arg1: Arg1,
    arg2: Arg2,
) -> Ret {
    core::intrinsics::abort();
}

impl RustcCLRInteropManagedChar {
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
}
