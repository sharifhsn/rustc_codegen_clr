//! Environment variables for the .NET ("dotnet") platform.
//!
//! Reads and writes go to the real process environment through three `extern`
//! hooks that the cilly linker maps to BCL calls (see
//! `cilly/src/ir/builtins/dotnet.rs::insert_dotnet_env`):
//!
//! * `rcl_dotnet_getenv(key_ptr, key_len) -> *mut u8`
//!   => `var s = Environment.GetEnvironmentVariable(Encoding.UTF8.GetString(key_ptr, key_len));`
//!      `return s == null ? null : Marshal.StringToCoTaskMemUTF8(s);`
//!      — a freshly-allocated, NUL-terminated UTF-8 C string the caller frees
//!      with `rcl_dotnet_cotaskmem_free`, or null when the variable is unset.
//! * `rcl_dotnet_setenv(key_ptr, key_len, val_ptr, val_len)`
//!   => `Environment.SetEnvironmentVariable(<key>, <val>)`.
//! * `rcl_dotnet_unsetenv(key_ptr, key_len)`
//!   => `Environment.SetEnvironmentVariable(<key>, null)` (deletes the var).
//!
//! This replaces the earlier stub (read returned `None`, writes reported
//! `Unsupported`). The full-environment iterator (`Env`, `env()`) is also real now:
//! a fourth hook `rcl_dotnet_environ()` enumerates `GetEnvironmentVariables()` into a
//! `KEY=VALUE\n` block that `env()` parses, so `std::env::vars()`/`vars_os()` work
//! instead of panicking via the shared `unsupported` arm.
//!
//! All `OsString`s are built from the returned bytes with the platform-agnostic
//! `os_str::Buf` + `FromInner` (no unix-only `OsStringExt`). The free-buffer hook is
//! shared with the args arm.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::ffi::{CStr, OsStr, OsString};
use crate::sys::os_str::Buf;
use crate::sys::FromInner;
use crate::vec;
use crate::{fmt, io};

// FIXED extern contract — the names must match EXACTLY the symbols the cilly
// linker patches in (`cilly/src/ir/builtins/dotnet.rs`). Do not rename these.
unsafe extern "C" {
    /// `Environment.GetEnvironmentVariable(key)` -> NUL-terminated UTF-8 C
    /// string (caller frees), or null if the variable is unset.
    fn rcl_dotnet_getenv(key_ptr: *const u8, key_len: usize) -> *mut u8;
    /// `Environment.SetEnvironmentVariable(key, val)`.
    fn rcl_dotnet_setenv(key_ptr: *const u8, key_len: usize, val_ptr: *const u8, val_len: usize);
    /// `Environment.SetEnvironmentVariable(key, null)` — unset the variable.
    fn rcl_dotnet_unsetenv(key_ptr: *const u8, key_len: usize);
    /// `Marshal.FreeCoTaskMem((IntPtr)ptr)` — release a buffer returned by
    /// `rcl_dotnet_getenv` (shared with the args arm's `rcl_dotnet_arg`).
    fn rcl_dotnet_cotaskmem_free(ptr: *mut u8);
    /// Enumerate `Environment.GetEnvironmentVariables()` into a freshly-allocated,
    /// NUL-terminated UTF-8 buffer of `KEY=VALUE\n` lines (caller frees with
    /// `rcl_dotnet_cotaskmem_free`).
    fn rcl_dotnet_environ() -> *mut u8;
}

/// Iterator over the process environment (`std::env::vars()`/`vars_os()`).
pub struct Env {
    iter: vec::IntoIter<(OsString, OsString)>,
}

impl Iterator for Env {
    type Item = (OsString, OsString);
    fn next(&mut self) -> Option<(OsString, OsString)> {
        self.iter.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl fmt::Debug for Env {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter.as_slice().iter()).finish()
    }
}

/// Snapshot the whole environment by parsing the `KEY=VALUE\n` block produced by
/// `rcl_dotnet_environ`. Each line is split on its FIRST `=` (env var names cannot
/// contain `=`); empty lines are skipped. Mirrors how the unix arm reads `environ`,
/// but the block is built once on the .NET side rather than walking a `char**`.
pub fn env() -> Env {
    let mut entries: Vec<(OsString, OsString)> = Vec::new();
    // SAFETY: the hook returns either null or a freshly-allocated NUL-terminated
    // UTF-8 buffer that we own and free below.
    let ptr = unsafe { rcl_dotnet_environ() };
    if !ptr.is_null() {
        // SAFETY: `ptr` is a valid NUL-terminated C string until we free it.
        let bytes = unsafe { CStr::from_ptr(ptr.cast()) }.to_bytes().to_vec();
        // SAFETY: `ptr` came from `rcl_dotnet_environ` and has not been freed; the
        // bytes were copied out, so releasing it now is sound.
        unsafe { rcl_dotnet_cotaskmem_free(ptr) };
        for line in bytes.split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }
            if let Some(eq) = line.iter().position(|&b| b == b'=') {
                let key = OsString::from_inner(Buf { inner: line[..eq].to_vec() });
                let val = OsString::from_inner(Buf { inner: line[eq + 1..].to_vec() });
                entries.push((key, val));
            }
        }
    }
    Env { iter: entries.into_iter() }
}

pub fn getenv(key: &OsStr) -> Option<OsString> {
    let key = key.as_encoded_bytes();
    // SAFETY: `(key.as_ptr(), key.len())` describes a readable region of exactly
    // `key.len()` UTF-8 bytes for the duration of the call; the hook only reads
    // it. It returns either null (variable unset) or a freshly-allocated,
    // NUL-terminated UTF-8 buffer that we own.
    let ptr = unsafe { rcl_dotnet_getenv(key.as_ptr(), key.len()) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `ptr` is a valid NUL-terminated C string until we free it below.
    let bytes = unsafe { CStr::from_ptr(ptr.cast()) }.to_bytes().to_vec();
    // SAFETY: `ptr` came from `rcl_dotnet_getenv` and has not been freed; the
    // bytes were copied out, so releasing it now is sound.
    unsafe { rcl_dotnet_cotaskmem_free(ptr) };
    Some(OsString::from_inner(Buf { inner: bytes }))
}

pub unsafe fn setenv(key: &OsStr, value: &OsStr) -> io::Result<()> {
    let key = key.as_encoded_bytes();
    let value = value.as_encoded_bytes();
    // SAFETY: both `(ptr, len)` pairs describe readable byte regions for the
    // duration of the call; the hook only reads them.
    unsafe { rcl_dotnet_setenv(key.as_ptr(), key.len(), value.as_ptr(), value.len()) };
    Ok(())
}

pub unsafe fn unsetenv(key: &OsStr) -> io::Result<()> {
    let key = key.as_encoded_bytes();
    // SAFETY: `(key.as_ptr(), key.len())` describes a readable byte region for
    // the duration of the call; the hook only reads it.
    unsafe { rcl_dotnet_unsetenv(key.as_ptr(), key.len()) };
    Ok(())
}
