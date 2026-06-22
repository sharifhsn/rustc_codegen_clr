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

// ---------------------------------------------------------------------------
// socket2 0.6.4 surface (tokio enables socket2 with features=["all"]). socket2's
// `src/sys/unix.rs` is selected under cfg(unix) (the global target-family flip),
// but target_os="dotnet" matches none of its named-OS const lists, so EVERY const
// it references must come from here. Linux x86_64 ABI values (the POSIX shim
// hardcodes Linux numbering). MOST are setsockopt option ints reached only via
// `SockRef` (set_nodelay/set_linger/quickack/...), which are OFF the TCP-echo hot
// path — they must EXIST so the dep type-checks; the shim no-ops the ones it does
// not honour (insert_setsockopt). Bodies are needed only for symbols socket2
// actually CALLS on the echo path (none beyond the already-bodied socket/bind/
// listen/connect/accept/read/write/setsockopt/getsockopt).
pub const AF_UNSPEC: c_int = 0;
pub const SOCK_RAW: c_int = 3;
pub const SOCK_SEQPACKET: c_int = 5;

pub const IPPROTO_IP: c_int = 0;
pub const IPPROTO_ICMP: c_int = 1;
pub const IPPROTO_TCP: c_int = 6;
pub const IPPROTO_UDP: c_int = 17;
pub const IPPROTO_ICMPV6: c_int = 58;

pub const SOL_IP: c_int = 0;
pub const SOL_IPV6: c_int = 41;

pub const SO_TYPE: c_int = 3;
pub const SO_BROADCAST: c_int = 6;
pub const SO_SNDBUF: c_int = 7;
pub const SO_RCVBUF: c_int = 8;
pub const SO_KEEPALIVE: c_int = 9;
pub const SO_OOBINLINE: c_int = 10;
pub const SO_LINGER: c_int = 13;

pub const TCP_NODELAY: c_int = 1;
pub const TCP_KEEPIDLE: c_int = 4;
pub const TCP_KEEPINTVL: c_int = 5;
pub const TCP_KEEPCNT: c_int = 6;

pub const IP_TOS: c_int = 1;
pub const IP_TTL: c_int = 2;
pub const IP_HDRINCL: c_int = 3;
pub const IP_RECVTOS: c_int = 13;
pub const IP_MULTICAST_IF: c_int = 32;
pub const IP_MULTICAST_TTL: c_int = 33;
pub const IP_MULTICAST_LOOP: c_int = 34;
pub const IP_ADD_MEMBERSHIP: c_int = 35;
pub const IP_DROP_MEMBERSHIP: c_int = 36;
pub const IP_ADD_SOURCE_MEMBERSHIP: c_int = 39;
pub const IP_DROP_SOURCE_MEMBERSHIP: c_int = 40;

pub const IPV6_UNICAST_HOPS: c_int = 16;
pub const IPV6_MULTICAST_IF: c_int = 17;
pub const IPV6_MULTICAST_HOPS: c_int = 18;
pub const IPV6_MULTICAST_LOOP: c_int = 19;
pub const IPV6_ADD_MEMBERSHIP: c_int = 20;
pub const IPV6_DROP_MEMBERSHIP: c_int = 21;
pub const IPV6_RECVHOPLIMIT: c_int = 51;
pub const IPV6_RECVTCLASS: c_int = 66;

pub const MSG_OOB: c_int = 1;
pub const MSG_EOR: c_int = 0x80;
pub const MSG_TRUNC: c_int = 0x20;

// Extra socket-type / fcntl / sockopt consts socket2 references on the dotnet
// build (Linux x86_64). All DECLARE-ONLY: reached via SockRef option setters /
// SockType helpers the TCP-echo path never exercises.
pub const SOCK_RDM: c_int = 4;
pub const F_GETFD: c_int = 1;
pub const TCP_MAXSEG: c_int = 2;
pub const SO_REUSEPORT: c_int = 15;

pub const POLLIN: c_short = 0x1;
pub const POLLOUT: c_short = 0x4;
pub const POLLERR: c_short = 0x8;
pub const POLLHUP: c_short = 0x10;

// eventfd flags (the mio Waker primitive). EFD_NONBLOCK is load-bearing for the
// tokio reactor's waker (the read end must not block the epoll sweep).
pub const EFD_CLOEXEC: c_int = 0o2000000;
pub const EFD_NONBLOCK: c_int = 0o4000;

// `nfds_t` for poll(2). socket2 references the type; the shim never calls poll.
pub type nfds_t = c_ulong;

// time types for socket2's SO_RCVTIMEO/SO_SNDTIMEO `timeval` round-trip (DECLARE-
// ONLY — tokio's echo never sets a socket timeout). Linux x86_64: time_t/suseconds_t
// are i64; `timeval` is {tv_sec: time_t, tv_usec: suseconds_t}.
pub type time_t = i64;
pub type suseconds_t = i64;

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

// ---------------------------------------------------------------------------
// socket2 0.6.4 struct surface (Linux x86_64 layout). All DECLARE-ONLY for the
// TCP-echo path: socket2 references them via SockRef option setters / multicast
// joins that tokio's loopback echo never exercises. They only need correct layout
// so socket2's `mem::size_of`/field writes type-check; the shim never reads them.
#[repr(C)]
pub struct linger {
    pub l_onoff: c_int,
    pub l_linger: c_int,
}

#[repr(C)]
pub struct ip_mreq {
    pub imr_multiaddr: in_addr,
    pub imr_interface: in_addr,
}

#[repr(C)]
pub struct ip_mreq_source {
    pub imr_multiaddr: in_addr,
    pub imr_interface: in_addr,
    pub imr_sourceaddr: in_addr,
}

#[repr(C)]
pub struct ipv6_mreq {
    pub ipv6mr_multiaddr: in6_addr,
    pub ipv6mr_interface: c_uint,
}

// ip_mreqn — socket2's interface-by-index multicast join (Linux-only struct).
// DECLARE-ONLY: the dotnet echo path never joins a multicast group.
#[repr(C)]
pub struct ip_mreqn {
    pub imr_multiaddr: in_addr,
    pub imr_address: in_addr,
    pub imr_ifindex: c_int,
}

// timeval — socket2's SO_RCVTIMEO/SO_SNDTIMEO duration round-trip. DECLARE-ONLY.
#[repr(C)]
pub struct timeval {
    pub tv_sec: time_t,
    pub tv_usec: suseconds_t,
}

#[repr(C)]
pub struct pollfd {
    pub fd: c_int,
    pub events: c_short,
    pub revents: c_short,
}

#[repr(C)]
pub struct iovec {
    pub iov_base: *mut c_void,
    pub iov_len: size_t,
}

#[repr(C)]
pub struct msghdr {
    pub msg_name: *mut c_void,
    pub msg_namelen: socklen_t,
    pub msg_iov: *mut iovec,
    pub msg_iovlen: size_t,
    pub msg_control: *mut c_void,
    pub msg_controllen: size_t,
    pub msg_flags: c_int,
}

dotnet_copy_clone! {
    in_addr in6_addr sockaddr sockaddr_in sockaddr_in6 sockaddr_storage epoll_event sockaddr_un
    linger ip_mreq ip_mreq_source ipv6_mreq pollfd iovec msghdr ip_mreqn timeval
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

    // socket2 0.6.4 DECLARE-ONLY surface — referenced for COMPILE under cfg(unix),
    // NEVER called on the tokio TCP-echo path (the echo path is socket/bind/listen/
    // connect/accept/read/write, all bodied). The linker only demands a body for a
    // REFERENCED symbol, so these resolve as missing-but-unused. socketpair is the
    // exception that DOES have a body (the AF_UNIX listener+connect pair, B2).
    pub fn poll(fds: *mut pollfd, nfds: nfds_t, timeout: c_int) -> c_int;
    pub fn socketpair(domain: c_int, ty: c_int, protocol: c_int, sv: *mut c_int) -> c_int;
    pub fn recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t;
    pub fn sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> ssize_t;
    pub fn if_nametoindex(ifname: *const c_char) -> c_uint;
    // socket2's recv/send/shutdown wrappers (DECLARE-ONLY for tokio's echo, which
    // reads/writes via TcpStream -> libc::read/write, not socket2's recv/send).
    // shutdown has a real shim body (rcl_dotnet_net_shutdown) if ever referenced.
    pub fn recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t;
    pub fn send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t;
    pub fn shutdown(fd: c_int, how: c_int) -> c_int;

    // eventfd — the mio Waker primitive. Body in posix_epoll.rs: returns a real
    // FD_KIND_SOCKET fd backed by a self-readable loopback UDP socket, so
    // read/write/epoll_wait dispatch over it work (the 8-byte counter degrades to
    // a readiness edge — any byte makes the read end pollable).
    pub fn eventfd(initval: c_uint, flags: c_int) -> c_int;

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
