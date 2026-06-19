//! System bindings for the .NET ("dotnet") platform.
//!
//! The platform-abstraction-layer facade for running Rust `std` on .NET (CoreCLR)
//! via the rustc_codegen_clr backend. Most facilities fall back to the shared
//! `unsupported` stubs; the load-bearing ones (alloc, stdio) are backed by the
//! .NET BCL through `extern` hooks that the cilly linker maps to BCL calls
//! (NativeMemory, System.Console) — the same MissingMethodPatcher mechanism the
//! surrogate already uses. This keeps the PAL clean Rust with the .NET binding
//! concentrated in the linker.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::io as std_io;

// SAFETY: must be called only once during runtime initialization.
// NOTE: this is not guaranteed to run, for example when Rust code is called externally.
pub unsafe fn init(_argc: isize, _argv: *const *const u8, _sigpipe: u8) {}

// SAFETY: must be called only once during runtime cleanup.
// NOTE: this is not guaranteed to run, for example when the program aborts.
pub unsafe fn cleanup() {}

pub fn unsupported<T>() -> std_io::Result<T> {
    Err(unsupported_err())
}

pub fn unsupported_err() -> std_io::Error {
    std_io::Error::UNSUPPORTED_PLATFORM
}

pub fn abort_internal() -> ! {
    core::intrinsics::abort();
}
