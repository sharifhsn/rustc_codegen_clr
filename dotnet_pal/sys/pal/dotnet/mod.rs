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

// ===========================================================================
// `cvt` — the *-1 means error is in `errno`* convention helper.
//
// `crate::sys::cvt` resolves to `sys::pal::*::cvt` (`sys/mod.rs` re-exports the
// PAL with `pub use pal::*`). For os=dotnet the PAL IS this module (injected as
// `sys/pal/mod.rs`'s first cfg_select! arm by dev.sh), so `os/fd/owned.rs`'s
// `use crate::sys::cvt;` (compiled in for not(wasm/sgx/hermit/trusty/motor),
// which includes dotnet) needs `cvt` to live here. Mirrors the canonical unix
// PAL `cvt`; `io::Error::last_os_error()` reads the thread-local `errno` via the
// shim's `__errno_location` (cilly/src/ir/builtins/posix.rs). This is what makes
// `std::os::fd` (`OwnedFd`/`BorrowedFd`) compile on the dotnet target — the
// prerequisite for the unified fd-backed net `Socket` (Cap-1 of the libc-shim
// capstone, LIBC_SHIM_SCOPE §4.2).
// ===========================================================================

#[doc(hidden)]
pub trait IsMinusOne {
    fn is_minus_one(&self) -> bool;
}

macro_rules! impl_is_minus_one {
    ($($t:ident)*) => ($(impl IsMinusOne for $t {
        fn is_minus_one(&self) -> bool {
            *self == -1
        }
    })*)
}

impl_is_minus_one! { i8 i16 i32 i64 isize }

/// Converts native return values to `Result` using the *-1 means error is in
/// `errno`* convention. Non-error values are `Ok`-wrapped.
pub fn cvt<T: IsMinusOne>(t: T) -> std_io::Result<T> {
    if t.is_minus_one() { Err(std_io::Error::last_os_error()) } else { Ok(t) }
}

/// `-1` → look at `errno` → retry on `EINTR` (which never fires on dotnet, so
/// this never loops). Otherwise `Ok()`-wrap the closure return value.
#[allow(dead_code)] // Not used on all paths.
pub fn cvt_r<T, F>(mut f: F) -> std_io::Result<T>
where
    T: IsMinusOne,
    F: FnMut() -> T,
{
    loop {
        match cvt(f()) {
            Err(ref e) if e.is_interrupted() => {}
            other => return other,
        }
    }
}
