//! Safe facade for a native API that retains callbacks and invokes them from another thread.

use std::os::raw::c_int;

use rust_dotnet_pinvoke::{native_api, status_zero};

use crate::native;

native_api! {
    /// Retained native worker registration with retryable, quiescent stop.
    pub retained_callback Registration, StopFailure as callback_trampoline(
        value: c_int,
    ) -> c_int {
        start(fail_first_unregister: bool);
        token = *mut native::ac_registration;
        register(context, out_registration) = native::ac_register(
            Some(callback_trampoline),
            context,
            i32::from(fail_first_unregister),
            out_registration,
        );
        unregister(registration) = native::ac_unregister(*registration);
        status = status_zero;
        on_panic = 77;
        quiescence = unregister_waits;
        threading = send;
    }

    /// Round-trips a borrowed Rust string through native-owned UTF-16 storage.
    pub fn copy_utf16(value: &str) -> String {
        utf16 value => value_pointer;
        out copied: *mut u16 => copied_pointer;
        unsafe_call = native::ac_copy_utf16(value_pointer, copied_pointer);
        status = status_zero;
        success = owned_utf16(copied, free = free_utf16, null = error);
    }
}

unsafe fn free_utf16(pointer: *mut core::ffi::c_void) {
    unsafe { native::ac_free_utf16(pointer.cast()) };
}

pub fn live_workers() -> i32 {
    unsafe { native::ac_live_workers() }
}
