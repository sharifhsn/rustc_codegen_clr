//! Minimal `libc` bindings for the .NET ("dotnet") platform — Cap-1 foundation.
//!
//! The upstream `libc` crate (0.2) has NO module for `target_os = "dotnet"`: its
//! top-level `cfg_if!` falls through to an empty `else {}` ("non-supported
//! targets: empty"). But `libc` IS linked into dotnet std (`std/Cargo.toml` gates
//! the dep on `cfg(not(all(windows, msvc)))`, which includes dotnet) and std's
//! own `std::os::fd` files (`os/fd/raw.rs`, `os/fd/owned.rs`) reference a small
//! fixed set of `libc::` symbols. With `os::fd` enabled for dotnet (the unified
//! fd-backed net `Socket` capstone, LIBC_SHIM_SCOPE §4.2), those references must
//! resolve. This module supplies exactly that set.
//!
//! The functions are bare `extern "C"` declarations: the cilly linker's POSIX
//! shim (`cilly/src/ir/builtins/posix.rs`, `posix_symbols.rs`) provides the
//! bodies (`close`/`read`/`write`/`fcntl`/`ioctl` over the int-fd ⇄ GCHandle
//! fd-table). So `libc::close(fd)` from `OwnedFd::drop` routes through the same
//! fd-table-aware close path as everything else — ONE representation.
//!
//! Injected by `feasibility/dev.sh` as `mod dotnet; pub use dotnet::*;` into the
//! libc crate's empty `else {}` block. os=dotnet-only; cannot affect any other
//! target (the `else` only fires for unsupported OSes, of which dotnet is one).
//!
//! `libc` is built `no_core`, so types come from `core::ffi`.

pub type c_int = i32;
pub type c_uint = u32;
pub type c_long = i64;
pub type c_ulong = u64;
pub type size_t = usize;
pub type ssize_t = isize;
// NOTE: `c_void` is already re-exported at the libc crate root (`pub use
// core::ffi::c_void`), so we must NOT re-export it here (a glob `pub use
// crate::dotnet::*` would make it ambiguous). Reference it via `core::ffi`.
use core::ffi::c_void;

// Standard stream fds (pre-seeded in the fd-table as STD sentinels).
pub const STDIN_FILENO: c_int = 0;
pub const STDOUT_FILENO: c_int = 1;
pub const STDERR_FILENO: c_int = 2;

// fcntl commands (Linux x86_64 ABI — the shim hardcodes Linux numbering).
pub const F_DUPFD: c_int = 0;
pub const F_DUPFD_CLOEXEC: c_int = 1030;
pub const F_GETFL: c_int = 3;
pub const F_SETFL: c_int = 4;
pub const O_NONBLOCK: c_int = 0o4000;

// ioctl request: FIONBIO (set/clear non-blocking).
pub const FIONBIO: c_ulong = 0x5421;

unsafe extern "C" {
    // fd-generic I/O — bodies provided by the cilly POSIX shim, kind-dispatched
    // (FILE/SOCKET/STD) through the fd-table.
    pub fn close(fd: c_int) -> c_int;
    pub fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t;
    pub fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t;
    pub fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    pub fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
}
