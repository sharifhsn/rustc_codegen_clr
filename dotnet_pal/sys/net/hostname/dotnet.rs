//! `sys::net::hostname` for the .NET ("dotnet") platform — Cap-1 foundation arm.
//!
//! Injected as the FIRST `cfg_select!` arm of `sys/net/hostname/mod.rs`,
//! replacing the `_ => unsupported` catch that currently serves os=dotnet, so the
//! unix arm (gated on `target_family="unix"`) never wins at the Cap-2
//! `families=["unix"]` flip.
//!
//! REAL: `hostname()` → `System.Environment.MachineName` via the
//! `rcl_dotnet_hostname` hook (cilly/src/ir/builtins/dotnet.rs), which returns a
//! freshly-allocated NUL-terminated UTF-8 C string on the COM-task-memory heap
//! (`Marshal.StringToCoTaskMemUTF8`); we copy it into an `OsString` and free it
//! with `rcl_dotnet_cotaskmem_free`, mirroring the args/env marshalling pattern.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::ffi::{CStr, OsString};
use crate::io;

unsafe extern "C" {
    fn rcl_dotnet_hostname() -> *mut u8;
    fn rcl_dotnet_cotaskmem_free(ptr: *mut u8);
}

pub fn hostname() -> io::Result<OsString> {
    // SAFETY: the hook returns either null (BCL failure) or a non-null pointer to
    // a NUL-terminated UTF-8 buffer it allocated; we own it and free it below.
    let ptr = unsafe { rcl_dotnet_hostname() };
    if ptr.is_null() {
        return Err(io::const_error!(io::ErrorKind::Other, "hostname query failed"));
    }
    // SAFETY: `ptr` points at a NUL-terminated C string from the hook.
    let cstr = unsafe { CStr::from_ptr(ptr as *const _) };
    let owned = OsString::from(String::from_utf8_lossy(cstr.to_bytes()).into_owned());
    // SAFETY: free the COM-task-memory buffer the hook allocated.
    unsafe { rcl_dotnet_cotaskmem_free(ptr) };
    Ok(owned)
}
