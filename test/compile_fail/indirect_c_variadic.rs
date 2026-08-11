#![no_std]

type Variadic = unsafe extern "C" fn(*const core::ffi::c_char, ...) -> core::ffi::c_int;

#[unsafe(no_mangle)]
pub unsafe fn call_indirect_c_variadic(function: Variadic) -> core::ffi::c_int {
    let format = b"%d\0";
    unsafe { function(format.as_ptr().cast(), 17_i32) }
}
