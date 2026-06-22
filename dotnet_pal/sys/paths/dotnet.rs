//! `sys::paths` for the .NET ("dotnet") platform — PACKAGE A.
//!
//! The `target-family="unix"` flip switches `sys/paths/mod.rs`'s cascade onto its
//! `target_family="unix"` arm (`mod unix; use unix as imp;`), which pulls
//! `libc::getcwd`/`chdir`, `getpwuid_r`/`passwd`/`getuid`, `sysconf`, and the
//! apple/bsd `current_exe` sysctl path — none of which exist on .NET. The dotnet
//! arm-0 (injected ahead of it by `feasibility/dev.sh`) routes here instead.
//!
//! REAL (BCL-backed) via 4 hooks the cilly linker maps to managed equivalents:
//!   * `getcwd`      -> `System.IO.Directory.GetCurrentDirectory()`
//!   * `current_exe` -> `System.Environment.ProcessPath`
//!   * `chdir`       -> `System.IO.Directory.SetCurrentDirectory(path)`
//!   * `temp_dir`    -> `System.IO.Path.GetTempPath()`
//!
//! PURE (copied byte-logic, no libc, no os::unix `OsStrExt`):
//!   * `split_paths` / `join_paths` / `SplitPaths` / `JoinPathsError` — a plain
//!     byte split/join on `:` (PATH_SEPARATOR). The unix arm uses
//!     `OsStr::from_bytes` / `OsStringExt::from_vec`; we use the platform-agnostic
//!     `as_encoded_bytes()` + `Buf`+`FromInner` convention (mirroring the dotnet
//!     env/fs arms) so this arm never depends on os::unix.
//!
//! LEAKY (L5): `home_dir` drops the `getpwuid_r` passwd fallback the unix arm has;
//! it returns the `HOME` environment variable or `None`. Fine on .NET (no passwd
//! db).
//!
//! Handle model: the `getcwd`/`current_exe`/`temp_dir` hooks return a freshly
//! allocated NUL-terminated UTF-8 C string (`Marshal.StringToCoTaskMemUTF8`,
//! shared with args/env), which we copy out and free via `rcl_dotnet_cotaskmem_free`.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::ffi::{CStr, OsStr, OsString};
use crate::path::{self, PathBuf};
use crate::sys::os_str::Buf;
use crate::sys::FromInner;
use crate::{fmt, io, iter, slice};

const PATH_SEPARATOR: u8 = b':';

// FIXED extern contract — mapped to the .NET BCL by the cilly linker. The string
// returns are CoTaskMem UTF-8 NUL-terminated buffers (shared free with args/env).
unsafe extern "C" {
    /// `Marshal.StringToCoTaskMemUTF8(Directory.GetCurrentDirectory())`.
    fn rcl_dotnet_paths_getcwd() -> *mut u8;
    /// `Marshal.StringToCoTaskMemUTF8(Environment.ProcessPath)` (null if unknown).
    fn rcl_dotnet_paths_current_exe() -> *mut u8;
    /// `Directory.SetCurrentDirectory((ptr,len) decoded UTF-8)`. Returns 0 / nonzero.
    fn rcl_dotnet_paths_chdir(ptr: *const u8, len: usize) -> i32;
    /// `Marshal.StringToCoTaskMemUTF8(Path.GetTempPath())`.
    fn rcl_dotnet_paths_temp_dir() -> *mut u8;
    /// Shared with args/env/fs: `Marshal.FreeCoTaskMem`.
    fn rcl_dotnet_cotaskmem_free(ptr: *mut u8);
}

/// Copy a CoTaskMem UTF-8 NUL-terminated buffer out into an owned `Vec<u8>`,
/// freeing the buffer. Returns `None` if the hook returned null.
fn take_cstr(ptr: *mut u8) -> Option<Vec<u8>> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `ptr` is a valid NUL-terminated C string from the hook until freed.
    let bytes = unsafe { CStr::from_ptr(ptr.cast()) }.to_bytes().to_vec();
    // SAFETY: `ptr` came from the hook and has not been freed; bytes copied out.
    unsafe { rcl_dotnet_cotaskmem_free(ptr) };
    Some(bytes)
}

fn pathbuf_from_bytes(bytes: Vec<u8>) -> PathBuf {
    // Platform-agnostic OsString construction (no os::unix `OsStringExt`).
    PathBuf::from(OsString::from_inner(Buf { inner: bytes }))
}

pub fn getcwd() -> io::Result<PathBuf> {
    // SAFETY: the hook returns a fresh CoTaskMem UTF-8 string or null.
    match take_cstr(unsafe { rcl_dotnet_paths_getcwd() }) {
        Some(b) => Ok(pathbuf_from_bytes(b)),
        None => Err(io::const_error!(io::ErrorKind::Other, "getcwd failed")),
    }
}

pub fn current_exe() -> io::Result<PathBuf> {
    // SAFETY: the hook returns a fresh CoTaskMem UTF-8 string or null.
    match take_cstr(unsafe { rcl_dotnet_paths_current_exe() }) {
        Some(b) => Ok(pathbuf_from_bytes(b)),
        None => Err(io::const_error!(io::ErrorKind::Uncategorized, "current_exe unavailable")),
    }
}

pub fn chdir(p: &path::Path) -> io::Result<()> {
    let bytes = p.as_os_str().as_encoded_bytes();
    // SAFETY: `(ptr,len)` is a readable UTF-8 region; the hook only reads it.
    let rc = unsafe { rcl_dotnet_paths_chdir(bytes.as_ptr(), bytes.len()) };
    if rc == 0 { Ok(()) } else { Err(io::const_error!(io::ErrorKind::Other, "chdir failed")) }
}

pub fn temp_dir() -> PathBuf {
    // SAFETY: the hook returns a fresh CoTaskMem UTF-8 string (never null —
    // Path.GetTempPath always yields a value).
    match take_cstr(unsafe { rcl_dotnet_paths_temp_dir() }) {
        Some(b) => pathbuf_from_bytes(b),
        None => PathBuf::from("/tmp"),
    }
}

pub fn home_dir() -> Option<PathBuf> {
    // LEAKY (L5): no getpwuid_r passwd fallback — HOME env only.
    crate::env::var_os("HOME").filter(|s| !s.is_empty()).map(PathBuf::from)
}

// --- pure byte split/join (no libc, no os::unix OsStrExt) -------------------

// `SplitPaths` mirrors the unix arm's opaque iterator type, built on
// `as_encoded_bytes()` (platform-agnostic) instead of `OsStr::from_bytes`.
pub type SplitPaths<'a> = iter::Map<
    slice::Split<'a, u8, impl FnMut(&u8) -> bool + 'static>,
    impl FnMut(&[u8]) -> PathBuf + 'static,
>;

#[define_opaque(SplitPaths)]
pub fn split_paths(unparsed: &OsStr) -> SplitPaths<'_> {
    fn is_separator(&b: &u8) -> bool {
        b == PATH_SEPARATOR
    }

    fn into_pathbuf(part: &[u8]) -> PathBuf {
        // The parts are valid UTF-8 sub-slices of a `:`-split path string.
        PathBuf::from(OsString::from_inner(Buf { inner: part.to_vec() }))
    }

    unparsed.as_encoded_bytes().split(is_separator).map(into_pathbuf)
}

#[derive(Debug)]
pub struct JoinPathsError;

pub fn join_paths<I, T>(paths: I) -> Result<OsString, JoinPathsError>
where
    I: Iterator<Item = T>,
    T: AsRef<OsStr>,
{
    let mut joined = Vec::new();

    for (i, path) in paths.enumerate() {
        let path = path.as_ref().as_encoded_bytes();
        if i > 0 {
            joined.push(PATH_SEPARATOR)
        }
        if path.contains(&PATH_SEPARATOR) {
            return Err(JoinPathsError);
        }
        joined.extend_from_slice(path);
    }
    Ok(OsString::from_inner(Buf { inner: joined }))
}

impl fmt::Display for JoinPathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "path segment contains separator `{}`", char::from(PATH_SEPARATOR))
    }
}

impl crate::error::Error for JoinPathsError {}
