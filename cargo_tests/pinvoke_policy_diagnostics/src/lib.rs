use rust_dotnet_pinvoke::native_api;

#[cfg(feature = "raw")]
#[link(name = "raw_escape_hatch")]
unsafe extern "C" {
    pub fn raw_operation(value: *const u8, length: usize) -> i32;
}

#[cfg(feature = "incomplete-function")]
native_api! {
    pub fn incomplete_function() -> () {
        unsafe_call = 0;
        success = unit;
    }
}

#[cfg(feature = "contradictory-string")]
native_api! {
    pub fn contradictory_string() -> String {
        out result: *mut u16 => result_out;
        unsafe_call = 0;
        status = rust_dotnet_pinvoke::status_zero;
        success = owned_utf8(result, free = free_utf16, null = error);
    }
}

#[cfg(feature = "incomplete-retained")]
native_api! {
    pub retained_callback Registration, StopFailure as callback(i32) -> i32 {
        token = u64;
        quiescence = unregister_waits;
    }
}

#[cfg(feature = "incomplete-scoped")]
native_api! {
    pub scoped_callback CallbackStorage as scoped_trampoline(value: i32) -> i32 {
        fallback = -1;
    }
}

#[cfg(feature = "incomplete-handle")]
native_api! {
    pub handle NativeHandle(core::ffi::c_void) {
        release = missing_close;
    }
}
