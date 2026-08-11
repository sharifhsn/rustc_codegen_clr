#![feature(adt_const_params, unsized_const_params)]
#![allow(incomplete_features)]

extern crate mycorrhiza;

type Raw = mycorrhiza::intrinsics::RustcCLRInteropManagedClass<"Tests", "Object">;
type RawArray = mycorrhiza::intrinsics::RustcCLRInteropManagedArray<i32, 1>;

#[unsafe(no_mangle)]
pub unsafe fn managed_box_array_new_probe(value: RawArray) -> *mut u8 {
    unsafe { mycorrhiza::managed_option::rustc_clr_interop_managed_box_new(value) }
}

#[unsafe(no_mangle)]
pub unsafe fn managed_box_array_peek_probe(handle: *mut u8) -> RawArray {
    unsafe { mycorrhiza::managed_option::rustc_clr_interop_managed_box_get(handle) }
}

#[unsafe(no_mangle)]
pub unsafe fn managed_box_value_new_probe(value: i32) -> *mut u8 {
    unsafe { mycorrhiza::managed_option::rustc_clr_interop_managed_box_new(value) }
}

#[unsafe(no_mangle)]
pub unsafe fn managed_box_value_peek_probe(handle: *mut u8) -> i32 {
    unsafe { mycorrhiza::managed_option::rustc_clr_interop_managed_box_get(handle) }
}

#[unsafe(no_mangle)]
pub unsafe fn managed_box_peek_probe(handle: *mut u8) -> Raw {
    unsafe { mycorrhiza::managed_option::rustc_clr_interop_managed_box_get(handle) }
}

#[unsafe(no_mangle)]
pub unsafe fn managed_box_take_probe(handle: *mut u8) -> Raw {
    unsafe { mycorrhiza::managed_option::rustc_clr_interop_managed_box_take(handle) }
}

#[unsafe(no_mangle)]
pub unsafe fn managed_box_free_probe(handle: *mut u8) {
    unsafe { mycorrhiza::managed_option::rustc_clr_interop_managed_box_free(handle) }
}
