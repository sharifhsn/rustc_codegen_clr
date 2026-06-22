//! `libc` bindings for the .NET ("dotnet") platform — the SINGLE libc face for
//! both std::os::fd AND unmodified upstream mio (Cap-2.5).
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
//! ## Cap-2.5: ALSO the libc face for near-unmodified upstream mio
//! mio's `#[cfg(unix)]` epoll selector (`sys/unix/selector/epoll.rs`) and net
//! glue (`sys/unix/net.rs` + `tcp.rs`/`udp.rs`) call `libc::epoll_*` /
//! `libc::socket`/`bind`/`connect`/`accept`/`setsockopt`/... and reference
//! `libc::epoll_event`, `libc::sockaddr*`, and the `EPOLL*`/`AF_*`/`SOCK_*`/`SO_*`
//! consts. The crate-scoped RUSTC_WRAPPER gives ONLY the mio crate
//! `--cfg unix --cfg target_os="linux"` so it picks that arm — but it does NOT
//! re-cfg libc, because forcing libc's real linux module while `target_os="dotnet"`
//! is ALSO active makes libc's `new/` module tree inconsistent (the `net::route`
//! gnu re-export + the `prelude!()` base-type imports fail under multi-valued
//! `target_os`). So libc stays on THIS dotnet arm for every build — it is the
//! single superset that serves std::os::fd AND mio. The bodies are resolved at
//! link time by the cilly POSIX shim (`posix.rs`/`posix_symbols.rs`/
//! `posix_epoll.rs`) by bare C-ABI symbol name, independent of which libc Rust
//! module is in scope.
//!
//! The functions are bare `extern "C"` declarations: the POSIX shim provides the
//! bodies (`close`/`read`/`write`/`fcntl`/`ioctl`/`socket`/`bind`/`listen`/
//! `connect`/`accept`/`accept4`/`setsockopt`/`epoll_create1`/`epoll_ctl`/
//! `epoll_wait` over the int-fd ⇄ GCHandle fd-table). So `libc::close(fd)` from
//! `OwnedFd::drop` AND `libc::epoll_wait(...)` from mio route through the same
//! fd-table-aware paths — ONE representation.
//!
//! Struct/const LAYOUTS mirror the Linux x86_64 ABI (the shim hardcodes Linux
//! numbering): `epoll_event` is `#[repr(C, packed)]` events:u32@0 / data:u64@4
//! (stride 12); `sockaddr_in` is family@0 / port@2 (network order) / addr@4..8.
//!
//! Injected by `feasibility/dev.sh` as `mod dotnet; pub use dotnet::*;` into the
//! libc crate's empty `else {}` block. os=dotnet-only.
//!
//! `libc` is built `no_core`, so types come from `core::ffi`.

pub type c_int = i32;
pub type c_uint = u32;
pub type c_long = i64;
pub type c_ulong = u64;
pub type c_char = i8;
pub type c_uchar = u8;
pub type c_short = i16;
pub type c_ushort = u16;
pub type size_t = usize;
pub type ssize_t = isize;
pub type socklen_t = u32;
pub type sa_family_t = u16;
pub type in_port_t = u16;
pub type in_addr_t = u32;
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
pub const F_SETFD: c_int = 2;
pub const FD_CLOEXEC: c_int = 1;
pub const O_NONBLOCK: c_int = 0o4000;
pub const O_CLOEXEC: c_int = 0o2000000;

// ioctl request: FIONBIO (set/clear non-blocking).
pub const FIONBIO: c_ulong = 0x5421;

// errno values the POSIX shim sets (Linux x86_64). EINPROGRESS is load-bearing
// for mio's non-blocking connect; EAGAIN for the readiness loop.
pub const EAGAIN: c_int = 11;
pub const EWOULDBLOCK: c_int = 11;
pub const EINPROGRESS: c_int = 115;
pub const EINTR: c_int = 4;

// ---------------------------------------------------------------------------
// Sockets (mio sys/unix/net.rs + tcp.rs + udp.rs).
// ---------------------------------------------------------------------------
pub const AF_INET: c_int = 2;
pub const AF_INET6: c_int = 10;
pub const SOCK_STREAM: c_int = 1;
pub const SOCK_DGRAM: c_int = 2;
pub const SOCK_NONBLOCK: c_int = 0o4000;
pub const SOCK_CLOEXEC: c_int = 0o2000000;
pub const SOL_SOCKET: c_int = 1;
pub const SO_REUSEADDR: c_int = 2;
pub const SO_ERROR: c_int = 4;
// SO_NOSIGPIPE is a BSD/Apple option; Linux has no such const. mio references it
// only under `#[cfg(target_vendor = "apple")]`, never on the linux arm, but the
// glob export is harmless. Give it the Apple numeric value for completeness.
pub const SO_NOSIGPIPE: c_int = 0x1022;
pub const IPPROTO_IPV6: c_int = 41;
pub const IPV6_V6ONLY: c_int = 26;

// PACKAGE A/B — AF_UNIX surface for std::os::unix::net (UnixStream/UnixListener/
// UnixDatagram) to COMPILE under the `target-family=["unix"]` flip. These consts
// + `sockaddr_un` satisfy os/unix/net/{addr,stream,listener,datagram}.rs; the
// genuinely-impossible pieces (abstract namespace, SCM_RIGHTS, ucred) are
// linux/bsd-cfg'd in os/unix/net and DROP for target_os="dotnet" (never compile).
// RUNTIME (AddressFamily.Unix / UnixDomainSocketEndPoint) is Package C; for now
// the dotnet net Socket's AF_UNIX methods are Err(Unsupported) compile-stubs.
pub const AF_UNIX: c_int = 1;
pub const SOMAXCONN: c_int = 128;
pub const SO_RCVTIMEO: c_int = 20;
pub const SO_SNDTIMEO: c_int = 21;
pub const MSG_PEEK: c_int = 2;
pub const MSG_NOSIGNAL: c_int = 0x4000;
pub const SHUT_RD: c_int = 0;
pub const SHUT_WR: c_int = 1;
pub const SHUT_RDWR: c_int = 2;

// PACKAGE A/B — `S_IF*` file-type mask bits (Linux ABI values). os/unix/fs.rs's
// `FileTypeExt` queries `self.as_inner().is(libc::S_IFBLK)` etc. (and the dotnet
// `FileType::is(mode)` stub masks against `S_IFMT`). LEAKY (L3): the dotnet BCL
// models dir-vs-file only, so block/char/fifo/socket all answer `false`.
pub const S_IFMT: c_int = 0o170000;
pub const S_IFSOCK: c_int = 0o140000;
pub const S_IFLNK: c_int = 0o120000;
pub const S_IFREG: c_int = 0o100000;
pub const S_IFBLK: c_int = 0o060000;
pub const S_IFDIR: c_int = 0o040000;
pub const S_IFCHR: c_int = 0o020000;
pub const S_IFIFO: c_int = 0o010000;

// PACKAGE A/B — `SIGKILL` for os::unix::process (Child::kill ->
// `send_process_group_signal(libc::SIGKILL)`). The dotnet `Process` is
// uninhabited (no real spawn), so the signal is never delivered (I6); the const
// only needs to EXIST so the re-export/call resolves.
pub const SIGKILL: c_int = 9;

// PACKAGE A/B — `O_NOFOLLOW` for os::unix::fs::OpenOptionsExt::custom_flags /
// sys::fs::set_permissions_nofollow. The dotnet FileStream model has no raw O_*
// passthrough (L1/I4), so `custom_flags` stores-and-ignores; the const only needs
// to EXIST. (set_permissions_nofollow is separately routed to its unimplemented
// arm for dotnet by feasibility/dev.sh.)
pub const O_NOFOLLOW: c_int = 0o400000;

// uintptr_t — os/unix/io/mod.rs references it for RawFd round-tripping.
pub type uintptr_t = usize;

// sockaddr_un: family@0, then a 108-byte sun_path (Linux ABI). os/unix/net/addr.rs
// uses `mem::offset_of!(sockaddr_un, sun_path)`, `sun_family`, and `sun_path`.
#[repr(C)]
pub struct sockaddr_un {
    pub sun_family: sa_family_t,
    pub sun_path: [c_char; 108],
}

// NOTE: libc is built without the std/derive prelude in this injected `else {}`
// context, so `#[derive(Copy, Clone)]` does not resolve. libc's own modules use
// its `s!` macro which expands to MANUAL `impl Copy`/`impl Clone`; we mirror that
// with the `dotnet_copy_clone!` helper so these mio-facing structs are Copy+Clone
// (the real libc structs are, and mio's `Events = Vec<epoll_event>` + the `Event`
// trait expect it).
macro_rules! dotnet_copy_clone {
    ($($t:ident)*) => ($(
        impl ::core::marker::Copy for $t {}
        impl ::core::clone::Clone for $t {
            fn clone(&self) -> $t { *self }
        }
    )*)
}

#[repr(C)]
pub struct in_addr {
    pub s_addr: in_addr_t,
}

#[repr(C)]
pub struct in6_addr {
    pub s6_addr: [u8; 16],
}

#[repr(C)]
pub struct sockaddr {
    pub sa_family: sa_family_t,
    pub sa_data: [c_char; 14],
}

// sockaddr_in: family@0, port@2 (network order), addr@4..8 — the layout the
// POSIX shim's sockaddr helpers hardcode (posix_symbols.rs).
#[repr(C)]
pub struct sockaddr_in {
    pub sin_family: sa_family_t,
    pub sin_port: in_port_t,
    pub sin_addr: in_addr,
    pub sin_zero: [u8; 8],
}

#[repr(C)]
pub struct sockaddr_in6 {
    pub sin6_family: sa_family_t,
    pub sin6_port: in_port_t,
    pub sin6_flowinfo: u32,
    pub sin6_addr: in6_addr,
    pub sin6_scope_id: u32,
}

#[repr(C)]
pub struct sockaddr_storage {
    pub ss_family: sa_family_t,
    __ss_pad1: [u8; 6],
    __ss_align: u64,
    __ss_pad2: [u8; 112],
}

dotnet_copy_clone! {
    in_addr in6_addr sockaddr sockaddr_in sockaddr_in6 sockaddr_storage epoll_event sockaddr_un
}

// ---------------------------------------------------------------------------
// epoll (mio sys/unix/selector/epoll.rs).
// ---------------------------------------------------------------------------
pub const EPOLL_CLOEXEC: c_int = 0o2000000;
pub const EPOLL_CTL_ADD: c_int = 1;
pub const EPOLL_CTL_DEL: c_int = 2;
pub const EPOLL_CTL_MOD: c_int = 3;

// epoll event flags (Linux x86_64). libc declares these as `c_int` (mio does
// `event.events as libc::c_int & libc::EPOLLIN` and ORs them as c_int), so the
// type MUST be c_int — not u32 — or mio hits `i32 & u32` mismatches.
pub const EPOLLIN: c_int = 0x001;
pub const EPOLLPRI: c_int = 0x002;
pub const EPOLLOUT: c_int = 0x004;
pub const EPOLLERR: c_int = 0x008;
pub const EPOLLHUP: c_int = 0x010;
pub const EPOLLRDNORM: c_int = 0x040;
pub const EPOLLRDBAND: c_int = 0x080;
pub const EPOLLWRNORM: c_int = 0x100;
pub const EPOLLWRBAND: c_int = 0x200;
pub const EPOLLMSG: c_int = 0x400;
pub const EPOLLRDHUP: c_int = 0x2000;
pub const EPOLLEXCLUSIVE: c_int = 1 << 28;
pub const EPOLLWAKEUP: c_int = 1 << 29;
pub const EPOLLONESHOT: c_int = 1 << 30;
// 0x8000_0000 is the sign bit; express via u32 to avoid an i32 literal overflow.
pub const EPOLLET: c_int = 0x8000_0000_u32 as c_int;

// epoll_event: #[repr(C, packed)] events:u32@0, u64:u64@4 (stride 12 on x86_64).
// The shim's posix_epoll.rs reads/writes events@0 (u32) and token@4 (u64) with
// this exact stride; the field name `u64` matches libc's real linux definition
// (mio writes `event.u64 = token`).
#[repr(C, packed)]
pub struct epoll_event {
    pub events: u32,
    pub u64: u64,
}

unsafe extern "C" {
    // fd-generic I/O — bodies provided by the cilly POSIX shim, kind-dispatched
    // (FILE/SOCKET/STD) through the fd-table.
    pub fn close(fd: c_int) -> c_int;
    pub fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t;
    pub fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t;
    pub fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    pub fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;

    // sockets — bodies in posix_symbols.rs over System.Net.Sockets.
    pub fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int;
    pub fn bind(fd: c_int, addr: *const sockaddr, len: socklen_t) -> c_int;
    pub fn listen(fd: c_int, backlog: c_int) -> c_int;
    pub fn connect(fd: c_int, addr: *const sockaddr, len: socklen_t) -> c_int;
    pub fn accept(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t) -> c_int;
    pub fn accept4(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t, flg: c_int) -> c_int;
    pub fn setsockopt(
        fd: c_int,
        level: c_int,
        name: c_int,
        value: *const c_void,
        len: socklen_t,
    ) -> c_int;
    pub fn getsockopt(
        fd: c_int,
        level: c_int,
        name: c_int,
        value: *mut c_void,
        len: *mut socklen_t,
    ) -> c_int;

    // PACKAGE A/B — referenced by os/unix/net + os/unix/io for COMPILE. The shim
    // resolves getsockname/getpeername over the fd-table; recvfrom/sendto/dup2 are
    // AF_UNIX-runtime stubs (Package C wires real AddressFamily.Unix). dup2 is used
    // by os/unix/io's RawFd helpers.
    pub fn getsockname(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t) -> c_int;
    pub fn getpeername(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t) -> c_int;
    pub fn recvfrom(
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t,
    ) -> ssize_t;
    pub fn sendto(
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t,
    ) -> ssize_t;
    pub fn dup2(oldfd: c_int, newfd: c_int) -> c_int;

    // epoll — bodies in posix_epoll.rs (per-fd Socket.Poll sweep).
    pub fn epoll_create1(flags: c_int) -> c_int;
    pub fn epoll_ctl(epfd: c_int, op: c_int, fd: c_int, event: *mut epoll_event) -> c_int;
    pub fn epoll_wait(
        epfd: c_int,
        events: *mut epoll_event,
        maxevents: c_int,
        timeout: c_int,
    ) -> c_int;
}
