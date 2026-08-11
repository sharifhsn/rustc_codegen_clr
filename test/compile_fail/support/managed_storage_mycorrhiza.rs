#![feature(adt_const_params, core_intrinsics, rustc_attrs, unsized_const_params)]
#![allow(incomplete_features, internal_features)]

// The default support crate explicitly opts its raw marker types into the backend ABI and gives
// only the token wrapper native-storage authority. Alternate cfgs deliberately forge malformed or
// safe diagnostic items; the backend must ignore those variants rather than ICE or widen trust.
#[rustc_diagnostic_item = "rustc_codegen_clr_native_storage_safe"]
#[cfg(not(any(forged_capability_item, forged_safe_capability)))]
pub unsafe trait NativeStorageSafe {}

#[rustc_diagnostic_item = "rustc_codegen_clr_native_storage_safe"]
#[cfg(forged_safe_capability)]
pub trait NativeStorageSafe {}

#[rustc_diagnostic_item = "rustc_codegen_clr_native_storage_safe"]
#[cfg(forged_capability_item)]
pub struct NativeStorageSafe;

#[rustc_diagnostic_item = "rustc_codegen_clr_managed_interop_type"]
#[cfg(not(forged_raw_identity))]
pub unsafe trait ManagedInteropType {}

// Crate-name/module/doc provenance plus only a safe capability must remain ordinary Rust. CLR ABI
// interpretation requires an explicit unsafe impl contract.
#[rustc_diagnostic_item = "rustc_codegen_clr_managed_interop_type"]
#[cfg(forged_raw_identity)]
pub trait ManagedInteropType {}

pub mod intrinsics {
    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct RustcCLRInteropManagedClass<
        const ASSEMBLY: &'static str,
        const CLASS_PATH: &'static str,
    > {
        pub size_hint: usize,
    }

    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct RustcCLRInteropManagedStruct<
        const ASSEMBLY: &'static str,
        const CLASS_PATH: &'static str,
        const SIZE: usize,
    > {
        pub size_hint: [u8; SIZE],
    }

    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct RustcCLRInteropManagedArray<T, const DIMENSIONS: usize> {
        pub object_ref: usize,
        pub marker: core::marker::PhantomData<T>,
    }

    impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const SIZE: usize>
        RustcCLRInteropManagedStruct<ASSEMBLY, CLASS_PATH, SIZE>
    {
        pub fn copy_via_ref(&self) -> Self {
            *self
        }
    }
    #[cfg(not(forged_raw_identity))]
    unsafe impl<T, const DIMENSIONS: usize> crate::ManagedInteropType
        for RustcCLRInteropManagedArray<T, DIMENSIONS>
    {
    }
    #[cfg(forged_raw_identity)]
    impl<T, const DIMENSIONS: usize> crate::ManagedInteropType
        for RustcCLRInteropManagedArray<T, DIMENSIONS>
    {
    }

    #[cfg(not(forged_raw_identity))]
    unsafe impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str>
        crate::ManagedInteropType for RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
    {
    }
    #[cfg(forged_raw_identity)]
    impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str> crate::ManagedInteropType
        for RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>
    {
    }
    #[cfg(not(forged_raw_identity))]
    unsafe impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const SIZE: usize>
        crate::ManagedInteropType for RustcCLRInteropManagedStruct<ASSEMBLY, CLASS_PATH, SIZE>
    {
    }
    #[cfg(forged_raw_identity)]
    impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, const SIZE: usize>
        crate::ManagedInteropType for RustcCLRInteropManagedStruct<ASSEMBLY, CLASS_PATH, SIZE>
    {
    }
}

pub mod class {
    // This deliberately stale/forgeable doc marker proves that the backend no longer grants a
    // storage exemption based on crate/name/attribute strings.
    #[doc = "__rustc_codegen_clr_gc_handle_storage_v1"]
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct Class<T> {
        pub handle: usize,
        pub marker: core::marker::PhantomData<fn() -> T>,
    }

    impl<T> Class<T> {
        pub const fn test_handle(handle: usize) -> Self {
            Self {
                handle,
                marker: core::marker::PhantomData,
            }
        }
    }

    #[cfg(not(any(forged_capability_item, forged_safe_capability)))]
    unsafe impl<T> crate::NativeStorageSafe for Class<T> {}

    #[cfg(forged_safe_capability)]
    impl<T> crate::NativeStorageSafe for Class<T> {}

    #[doc = "__rustc_codegen_clr_gc_handle_storage_v1"]
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct ForgedRoot<T> {
        pub value: T,
    }

    #[cfg(forged_safe_capability)]
    impl<T> crate::NativeStorageSafe for ForgedRoot<T> {}

    pub trait ConditionalToken {}
    impl ConditionalToken for usize {}

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct ConditionalRoot<T> {
        pub value: T,
    }

    #[cfg(not(any(forged_capability_item, forged_safe_capability)))]
    unsafe impl<T: ConditionalToken> crate::NativeStorageSafe for ConditionalRoot<T> {}

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct LifetimeRoot<'a, T> {
        pub value: T,
        pub marker: core::marker::PhantomData<&'a ()>,
    }

    // This promise is intentionally lifetime-specific. Codegen erases regions, so the backend
    // rejects capability-bearing ADTs with lifetime parameters instead of applying it to every
    // erased lifetime.
    #[cfg(not(any(forged_capability_item, forged_safe_capability)))]
    unsafe impl<T> crate::NativeStorageSafe for LifetimeRoot<'static, T> {}

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct ReferenceCapabilityLocal<T> {
        pub value: T,
    }

    // These deliberately lifetime-specific promises probe both a non-ADT self type and a named
    // container whose type argument contains a reference. Codegen must not widen either promise
    // after regions are erased.
    #[cfg(not(any(forged_capability_item, forged_safe_capability)))]
    unsafe impl<T> crate::NativeStorageSafe for &'static ReferenceCapabilityLocal<T> {}

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct ReferenceCapabilityContainer<T> {
        pub value: T,
    }

    #[cfg(not(any(forged_capability_item, forged_safe_capability)))]
    unsafe impl<T> crate::NativeStorageSafe
        for ReferenceCapabilityContainer<&'static ReferenceCapabilityLocal<T>>
    {
    }
}

pub mod enums {
    // Exact crate/module/name/marker collision, but intentionally safe. The backend must compile
    // this ordinary function instead of substituting its unsafe stack-transmute magic.
    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    #[inline(never)]
    pub fn rustc_clr_interop_enum_from_repr(value: usize) -> usize {
        value + 1
    }
}

pub mod managed_option {
    /// Root a direct CLR value behind a GCHandle token.
    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    #[inline(never)]
    pub unsafe fn rustc_clr_interop_managed_box_new<T>(_value: T) -> *mut u8 {
        core::intrinsics::abort()
    }

    /// Recover the rooted target without releasing its GCHandle.
    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    #[inline(never)]
    pub unsafe fn rustc_clr_interop_managed_box_get<T>(_handle: *mut u8) -> T {
        core::intrinsics::abort()
    }

    /// Recover the rooted target and release its GCHandle exactly once.
    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    #[inline(never)]
    pub unsafe fn rustc_clr_interop_managed_box_take<T>(_handle: *mut u8) -> T {
        core::intrinsics::abort()
    }

    /// Release the GCHandle without recovering its target.
    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    #[inline(never)]
    pub unsafe fn rustc_clr_interop_managed_box_free(_handle: *mut u8) {
        core::intrinsics::abort()
    }
}

#[cfg(safe_helper_binary)]
fn main() {
    if enums::rustc_clr_interop_enum_from_repr(41) != 42 {
        core::intrinsics::abort();
    }
}
