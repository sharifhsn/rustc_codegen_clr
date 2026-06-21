//! `io::Error` decoding for the .NET ("dotnet") platform.
//!
//! Historically the dotnet PAL had no C-style `errno`: I/O went through the
//! managed BCL, which signals failure by **throwing**, so this arm hardcoded
//! `errno()==0` / `ErrorKind::Uncategorized`. The libc-shim tier
//! (`cilly/src/ir/builtins/posix.rs`) added a real thread-local `errno` cell +
//! an exception→errno translation in the bare POSIX wrappers; this arm now reads
//! that cell via `__errno_location` and decodes the curated errno set the shim
//! produces (mirroring the unix arm for the codes that map cleanly).
//!
//! **Leak (LIBC_SHIM_SCOPE §3.2):** the exception→errno map is lossy — ~20
//! `SocketError` codes map cleanly (notably `WouldBlock`→`EAGAIN`), the long
//! `IOException`/HResult tail collapses to `EIO`, and `EINTR` never fires (fine:
//! `is_interrupted` stays false). Codes outside the curated set decode to
//! `Uncategorized`.

unsafe extern "C" {
    fn __errno_location() -> *mut i32;
}

pub fn errno() -> i32 {
    // SAFETY: `__errno_location` is provided by the cilly linker's POSIX shim and
    // returns the address of a `[ThreadStatic]` i32 cell; reading it is sound.
    unsafe { *__errno_location() }
}

pub fn is_interrupted(code: i32) -> bool {
    // EINTR never fires on the .NET PAL (the BCL has no signal-interruptible I/O),
    // so this is always false. Kept for parity with the unix arm.
    code == 4 // EINTR
}

pub fn decode_error_kind(code: i32) -> crate::io::ErrorKind {
    use crate::io::ErrorKind;
    // Linux x86_64 errno numbering (the shim hardcodes the Linux ABI). Only the
    // codes the shim's exception→errno table actually produces are mapped; the
    // rest decode to `Uncategorized` (the documented leak tail).
    match code {
        1 => ErrorKind::PermissionDenied,        // EPERM
        2 => ErrorKind::NotFound,                // ENOENT
        13 => ErrorKind::PermissionDenied,       // EACCES
        11 => ErrorKind::WouldBlock,             // EAGAIN/EWOULDBLOCK (load-bearing)
        98 => ErrorKind::AddrInUse,              // EADDRINUSE
        104 => ErrorKind::ConnectionReset,       // ECONNRESET
        110 => ErrorKind::TimedOut,              // ETIMEDOUT
        111 => ErrorKind::ConnectionRefused,     // ECONNREFUSED
        32 => ErrorKind::BrokenPipe,             // EPIPE
        _ => ErrorKind::Uncategorized,
    }
}

pub fn error_string(errno: i32) -> String {
    // No managed `strerror`; a terse numeric description is enough for the shim's
    // coarse error reporting (the BCL exception message is the richer source, not
    // wired back through std).
    match errno {
        0 => "success".to_string(),
        2 => "no such file or directory".to_string(),
        11 => "resource temporarily unavailable".to_string(),
        13 => "permission denied".to_string(),
        98 => "address already in use".to_string(),
        104 => "connection reset by peer".to_string(),
        110 => "connection timed out".to_string(),
        111 => "connection refused".to_string(),
        _ => format!("error {errno}"),
    }
}
