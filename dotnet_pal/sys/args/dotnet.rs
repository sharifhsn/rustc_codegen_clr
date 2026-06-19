//! Command-line arguments for the .NET ("dotnet") platform.
//!
//! Backed by the real process command line through three `extern` hooks that the
//! cilly linker maps to BCL calls (see
//! `cilly/src/ir/builtins/dotnet.rs::insert_dotnet_args`):
//!
//! * `rcl_dotnet_args_count() -> usize`
//!   => `System.Environment.GetCommandLineArgs().Length`
//! * `rcl_dotnet_arg(idx: usize) -> *mut u8`
//!   => `Marshal.StringToCoTaskMemUTF8(Environment.GetCommandLineArgs()[idx])`
//!      — a freshly-allocated, NUL-terminated UTF-8 C string (or null if `idx`
//!      is out of range). The caller owns it and must release it with
//!      `rcl_dotnet_cotaskmem_free`.
//! * `rcl_dotnet_cotaskmem_free(ptr: *mut u8)`
//!   => `Marshal.FreeCoTaskMem((IntPtr)ptr)` — frees a buffer returned by
//!      `rcl_dotnet_arg` (and is shared with the env arm).
//!
//! This replaces the earlier always-empty stub: `std::env::args()` now reports
//! the actual managed `string[] args` (argv[0] is the host/exe path, exactly as
//! on unix). The argv is not threaded through `sys::pal::dotnet::init` (the
//! dotnet entry shim does not forward argc/argv), so it is fetched on demand
//! from the BCL here — the same on-demand strategy the `zkvm` arm uses.
//!
//! Modeled on `sys/args/zkvm.rs` (count-then-fetch-each), but each arg comes
//! back as a NUL-terminated UTF-8 string read with `CStr` rather than a
//! length-prefixed word buffer. The bytes are copied into an owned `OsString`
//! before the managed buffer is freed, so the returned `Args` borrows nothing.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::ffi::{CStr, OsString};
use crate::fmt;
use crate::sys::os_str::Buf;
use crate::sys::FromInner;
use crate::vec;

// FIXED extern contract — the names must match EXACTLY the symbols the cilly
// linker patches in (`cilly/src/ir/builtins/dotnet.rs`). Do not rename these.
unsafe extern "C" {
    /// `Environment.GetCommandLineArgs().Length`.
    fn rcl_dotnet_args_count() -> usize;
    /// `Marshal.StringToCoTaskMemUTF8(Environment.GetCommandLineArgs()[idx])`:
    /// a NUL-terminated UTF-8 C string the caller must free, or null if `idx`
    /// is out of range.
    fn rcl_dotnet_arg(idx: usize) -> *mut u8;
    /// `Marshal.FreeCoTaskMem((IntPtr)ptr)` — release a buffer from
    /// `rcl_dotnet_arg`.
    fn rcl_dotnet_cotaskmem_free(ptr: *mut u8);
}

/// Snapshot the process arguments from the BCL into owned `OsString`s.
fn collect_args() -> Vec<OsString> {
    // SAFETY: `rcl_dotnet_args_count` takes no arguments and only reads the
    // managed command line; it is always safe to call.
    let argc = unsafe { rcl_dotnet_args_count() };
    let mut args = Vec::with_capacity(argc);
    let mut i = 0;
    while i < argc {
        // SAFETY: `i < argc`, so the index is in range and the hook returns a
        // freshly-allocated, NUL-terminated UTF-8 buffer (never null here).
        let ptr = unsafe { rcl_dotnet_arg(i) };
        i += 1;
        if ptr.is_null() {
            // Defensive: a null would only appear on an out-of-range index,
            // which the bound above rules out — skip rather than deref.
            continue;
        }
        // SAFETY: `ptr` is a valid NUL-terminated C string for the duration of
        // this block (until we hand it back to `rcl_dotnet_cotaskmem_free`).
        let bytes = unsafe { CStr::from_ptr(ptr.cast()) }.to_bytes().to_vec();
        // SAFETY: `ptr` came from `rcl_dotnet_arg` and has not been freed yet;
        // the bytes above were copied out, so freeing it now is sound.
        unsafe { rcl_dotnet_cotaskmem_free(ptr) };
        // Platform-agnostic OsString construction (matches the `zkvm`/`sgx`
        // arms): the UTF-8 bytes are valid os-str content.
        args.push(OsString::from_inner(Buf { inner: bytes }));
    }
    args
}

pub struct Args {
    iter: vec::IntoIter<OsString>,
}

pub fn args() -> Args {
    Args { iter: collect_args().into_iter() }
}

impl fmt::Debug for Args {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.iter.as_slice().fmt(f)
    }
}

impl Iterator for Args {
    type Item = OsString;

    #[inline]
    fn next(&mut self) -> Option<OsString> {
        self.iter.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for Args {
    #[inline]
    fn next_back(&mut self) -> Option<OsString> {
        self.iter.next_back()
    }
}

impl ExactSizeIterator for Args {
    #[inline]
    fn len(&self) -> usize {
        self.iter.len()
    }
}
