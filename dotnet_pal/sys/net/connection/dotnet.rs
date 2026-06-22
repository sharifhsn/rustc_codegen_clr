//! Networking for the .NET ("dotnet") platform — **fd-backed** (Cap-1 unified
//! design).
//!
//! Backs `std::net` (TcpStream / TcpListener / UdpSocket / SocketAddr) with the
//! .NET BCL (`System.Net.Sockets.Socket` + `System.Net.IPAddress` /
//! `IPEndPoint`) through a set of `extern "C"` hooks the cilly linker maps to BCL
//! calls — the same `MissingMethodPatcher` mechanism the alloc / stdio / env /
//! thread / fs arms use. See `cilly/src/ir/builtins/dotnet.rs::insert_dotnet_net`.
//!
//! ## The unified fd-backed Socket (Cap-1 of the libc-shim capstone)
//! The canonical identity of an open socket is now its **integer fd**. A
//! `Socket` is a `FileDesc` (an `OwnedFd`); the managed `System.Net.Sockets.Socket`
//! GCHandle lives in the process-global **fd-table** entry
//! (`cilly/src/ir/builtins/posix.rs`: `rcl_fd_table` int fd → `RclFdEntry`
//! {handle, kind, flags}). Socket ops resolve `fd → GCHandle` through the table
//! (`rcl_fdtable_handle`) and then call the UNCHANGED `rcl_dotnet_net_*` BCL
//! hooks — ONE representation shared by the std net PAL and the POSIX shim. This
//! is what lets `std::os::fd` (`AsRawFd`/`AsFd`/`FromRawFd`/`IntoRawFd`) and
//! `os/fd/net.rs` (`Socket::from_inner(OwnedFd…)`, `as_inner().socket()`) compile
//! against the dotnet net types — the prerequisite for unmodified `mio`/`socket2`
//! when `families=["unix"]` is flipped at Cap-2.
//!
//! Create paths (`tcp_connect`/`bind`/`accept`/`socket`) call the create hook,
//! which returns the opaque `GCHandle` `IntPtr`, then register it into the
//! fd-table (`rcl_fdtable_insert(handle, FD_KIND_SOCKET, 0)`), receive the int
//! fd, and wrap it as `Socket(FileDesc(OwnedFd::from_raw_fd(fd)))`. Drop routes
//! teardown through the fd-table-aware close (one path).
//!
//! SocketAddr ABI (unchanged): a `&SocketAddr` is decomposed std-side into
//! `(family: i32, ip_ptr, ip_len, port: u16)` and rebuilt BCL-side; out-addrs are
//! written into caller out-buffers. See `addr_parts`/`addr_from_parts`.
//!
//! FIXED extern contract (the names must match EXACTLY on the linker side):
//!
//! * `rcl_dotnet_net_tcp_connect(family, ip_ptr, ip_len, port) -> *mut u8`
//! * `rcl_dotnet_net_bind(family, ip_ptr, ip_len, port, sock_type, backlog) -> *mut u8`
//! * `rcl_dotnet_net_accept(handle, out_family, out_ip, out_port) -> *mut u8`
//! * `rcl_dotnet_net_recv(handle, buf_ptr, len) -> isize`
//! * `rcl_dotnet_net_send(handle, buf_ptr, len) -> isize`
//! * `rcl_dotnet_net_recv_from(handle, buf_ptr, len, out_family, out_ip, out_port) -> isize`
//! * `rcl_dotnet_net_send_to(handle, buf_ptr, len, family, ip_ptr, ip_len, port) -> isize`
//! * `rcl_dotnet_net_local_addr(handle, out_family, out_ip, out_port) -> i32`
//! * `rcl_dotnet_net_peer_addr(handle, out_family, out_ip, out_port) -> i32`
//! * `rcl_dotnet_net_udp_connect(handle, family, ip_ptr, ip_len, port) -> i32`
//! * `rcl_dotnet_net_shutdown(handle, how) -> i32`
//! * `rcl_dotnet_net_set_nonblocking(handle, nonblocking) -> i32`
//! * `rcl_dotnet_net_set_nodelay(handle, on) -> i32`
//! * `rcl_dotnet_net_nodelay(handle) -> i32`
//! * `rcl_dotnet_net_close(handle)`
//!
//! fd-table builtins (cilly/src/ir/builtins/posix.rs):
//! * `rcl_fdtable_insert(handle: isize, kind: i32, flags: i32) -> i32`
//! * `rcl_fdtable_handle(fd: i32) -> *mut u8`
//!
//! REAL (BCL-backed): `TcpStream::{connect, read, write, peer_addr, socket_addr,
//! shutdown, set_nodelay, nodelay, set_nonblocking}`; `TcpListener::{bind, accept,
//! socket_addr, set_nonblocking}`; `UdpSocket::{bind, send_to, recv_from, recv,
//! send, peer_addr, socket_addr, connect, set_nonblocking}`. The `*_vectored` /
//! `read_buf` methods delegate to the `crate::io::default_*` adapters.
//!
//! STUBBED to `Err(Unsupported)` / sensible default (os=dotnet-only): timeouts,
//! `peek`/`peek_from`, `duplicate`, linger/keepalive/ttl/only_v6/broadcast/
//! multicast, `take_error`, `lookup_host`.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::fmt;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs};
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::sys::cvt;
use crate::sys::fd::FileDesc;
use crate::sys::unsupported;
use crate::sys::{AsInner, FromInner, IntoInner};
use crate::time::Duration;

// FIXED extern contract — mapped to the .NET BCL by the cilly linker. Do not
// rename: the linker keys on these exact symbols.
unsafe extern "C" {
    fn rcl_dotnet_net_tcp_connect(
        family: i32,
        ip_ptr: *const u8,
        ip_len: usize,
        port: u16,
    ) -> *mut u8;
    fn rcl_dotnet_net_bind(
        family: i32,
        ip_ptr: *const u8,
        ip_len: usize,
        port: u16,
        sock_type: i32,
        backlog: i32,
    ) -> *mut u8;
    fn rcl_dotnet_net_accept(
        handle: *mut u8,
        out_family: *mut i32,
        out_ip: *mut u8,
        out_port: *mut u16,
    ) -> *mut u8;
    fn rcl_dotnet_net_recv(handle: *mut u8, buf_ptr: *mut u8, len: usize) -> isize;
    fn rcl_dotnet_net_send(handle: *mut u8, buf_ptr: *const u8, len: usize) -> isize;
    fn rcl_dotnet_net_recv_from(
        handle: *mut u8,
        buf_ptr: *mut u8,
        len: usize,
        out_family: *mut i32,
        out_ip: *mut u8,
        out_port: *mut u16,
    ) -> isize;
    fn rcl_dotnet_net_send_to(
        handle: *mut u8,
        buf_ptr: *const u8,
        len: usize,
        family: i32,
        ip_ptr: *const u8,
        ip_len: usize,
        port: u16,
    ) -> isize;
    fn rcl_dotnet_net_local_addr(
        handle: *mut u8,
        out_family: *mut i32,
        out_ip: *mut u8,
        out_port: *mut u16,
    ) -> i32;
    fn rcl_dotnet_net_peer_addr(
        handle: *mut u8,
        out_family: *mut i32,
        out_ip: *mut u8,
        out_port: *mut u16,
    ) -> i32;
    fn rcl_dotnet_net_udp_connect(
        handle: *mut u8,
        family: i32,
        ip_ptr: *const u8,
        ip_len: usize,
        port: u16,
    ) -> i32;
    fn rcl_dotnet_net_shutdown(handle: *mut u8, how: i32) -> i32;
    fn rcl_dotnet_net_set_nonblocking(handle: *mut u8, nonblocking: i32) -> i32;
    fn rcl_dotnet_net_set_nodelay(handle: *mut u8, on: i32) -> i32;
    fn rcl_dotnet_net_nodelay(handle: *mut u8) -> i32;
    fn rcl_dotnet_net_close(handle: *mut u8);

    // fd-table builtins (cilly/src/ir/builtins/posix.rs). insert takes the GCHandle
    // as an `isize` (the IntPtr), kind = FD_KIND_SOCKET (2), flags = 0; returns the
    // int fd. handle resolves an int fd back to its GCHandle.
    fn rcl_fdtable_insert(handle: isize, kind: i32, flags: i32) -> i32;
    fn rcl_fdtable_handle(fd: i32) -> *mut u8;
}

// fd-table kind tag for a socket entry (matches FD_KIND_SOCKET in posix.rs).
const FD_KIND_SOCKET: i32 = 2;

// Our own ABI address-family ints (NOT libc AF_*): the hook maps these to the
// BCL `AddressFamily` enum (InterNetwork / InterNetworkV6).
const FAMILY_V4: i32 = 4;
const FAMILY_V6: i32 = 6;
const SOCK_STREAM: i32 = 1;
const SOCK_DGRAM: i32 = 2;
const NO_LISTEN: i32 = -1;
const TCP_BACKLOG: i32 = 128;
const SHUT_READ: i32 = 0;
const SHUT_WRITE: i32 = 1;
const SHUT_BOTH: i32 = 2;

// ===========================================================================
// The unified fd-backed Socket
// ===========================================================================

/// An open socket, identified by its integer fd. The managed
/// `System.Net.Sockets.Socket` GCHandle lives in the fd-table entry; this type IS
/// an `OwnedFd` (via `FileDesc`). The single source of truth shared by the std
/// net PAL and the POSIX shim.
pub struct Socket(FileDesc);

impl Socket {
    /// Register a managed `Socket` GCHandle (returned by a create hook) into the
    /// fd-table and wrap the resulting int fd as a `Socket`. Returns `None` if the
    /// hook handed back a null handle (the BCL failed / threw).
    fn from_handle(handle: *mut u8) -> Option<Socket> {
        if handle.is_null() {
            return None;
        }
        // SAFETY: `handle` is a non-null opaque GCHandle IntPtr; registering it
        // hands back a fresh int fd owning that entry. We then take ownership via
        // OwnedFd::from_raw_fd, so the fd-table slot is freed exactly once on Drop.
        let fd = unsafe { rcl_fdtable_insert(handle as isize, FD_KIND_SOCKET, 0) };
        let owned = unsafe { OwnedFd::from_raw_fd(fd as RawFd) };
        Some(Socket(FileDesc::from_inner(owned)))
    }

    /// Resolve this socket's int fd back to its managed `Socket` GCHandle through
    /// the fd-table — the seam every data-plane op crosses.
    #[inline]
    fn handle(&self) -> *mut u8 {
        // SAFETY: `self` owns a live fd-table SOCKET entry; the builtin reads its
        // GCHandle field.
        unsafe { rcl_fdtable_handle(self.as_raw_fd()) }
    }

    // =======================================================================
    // PACKAGE A/B — AF_UNIX compile-stubs.
    //
    // Under the `target-family=["unix"]` flip, `std::os::unix::net`
    // (UnixStream/UnixListener/UnixDatagram) is `Socket(crate::sys::net::Socket)`
    // and calls these inherent methods on it. The genuinely-impossible AF_UNIX
    // pieces (abstract namespace, SCM_RIGHTS, ucred) are linux/bsd-cfg'd in
    // os/unix/net and DROP for dotnet, so only this stable subset must EXIST.
    //
    // .NET *does* model unix-domain sockets (AddressFamily.Unix +
    // UnixDomainSocketEndPoint), so these are NOT impossible — they are
    // deliberately deferred to Package C (runtime). For Package A/B they are
    // Err(Unsupported)/no-op compile-stubs so std + os::unix COMPILE. They are
    // additive (new method names) and do NOT touch the working TcpStream/
    // TcpListener/UdpSocket data-plane methods, so the net PAL is unregressed.
    // =======================================================================

    /// `Socket::new(domain, ty)` — create an AF_UNIX socket. STUB (Package C
    /// wires `new System.Net.Sockets.Socket(AddressFamily.Unix, ...)`).
    pub fn new(_domain: i32, _ty: i32) -> io::Result<Socket> {
        unsupported()
    }

    /// `Socket::new_pair(domain, ty)` — `socketpair(2)`. STUB (Package C emulates
    /// via a bound-listener+connect pair — there is no kernel socketpair on CLR).
    pub fn new_pair(_domain: i32, _ty: i32) -> io::Result<(Socket, Socket)> {
        unsupported()
    }

    pub fn accept(
        &self,
        _storage: *mut crate::ffi::c_void,
        _len: *mut u32,
    ) -> io::Result<Socket> {
        unsupported()
    }

    pub fn duplicate(&self) -> io::Result<Socket> {
        unsupported()
    }

    /// `set_timeout(dur, kind)` where `kind` is `SO_RCVTIMEO`/`SO_SNDTIMEO`. STUB.
    pub fn set_timeout(&self, _dur: Option<Duration>, _kind: i32) -> io::Result<()> {
        unsupported()
    }

    pub fn timeout(&self, _kind: i32) -> io::Result<Option<Duration>> {
        Ok(None)
    }

    /// SO_MARK (Linux-only socket option). STUB (no managed equivalent).
    pub fn set_mark(&self, _mark: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        Ok(None)
    }

    pub fn shutdown(&self, _how: Shutdown) -> io::Result<()> {
        unsupported()
    }

    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        // No-op for the AF_UNIX stub (the fd is never live at runtime here).
        Ok(())
    }

    pub fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> {
        unsupported()
    }

    pub fn read(&self, _buf: &mut [u8]) -> io::Result<usize> {
        unsupported()
    }

    pub fn read_buf(&self, _cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        unsupported()
    }

    pub fn read_vectored(&self, _bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        unsupported()
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn write(&self, _buf: &[u8]) -> io::Result<usize> {
        unsupported()
    }

    pub fn write_vectored(&self, _bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        unsupported()
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }

    /// `send_with_flags(buf, MSG_NOSIGNAL)` from os::unix::net::UnixStream::write. STUB.
    pub fn send_with_flags(&self, _buf: &[u8], _flags: i32) -> io::Result<usize> {
        unsupported()
    }
}

impl AsInner<FileDesc> for Socket {
    #[inline]
    fn as_inner(&self) -> &FileDesc {
        &self.0
    }
}

impl IntoInner<FileDesc> for Socket {
    #[inline]
    fn into_inner(self) -> FileDesc {
        self.0
    }
}

impl FromInner<FileDesc> for Socket {
    #[inline]
    fn from_inner(file_desc: FileDesc) -> Socket {
        Socket(file_desc)
    }
}

impl AsFd for Socket {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl AsRawFd for Socket {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl IntoRawFd for Socket {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.0.into_raw_fd()
    }
}

impl FromRawFd for Socket {
    #[inline]
    unsafe fn from_raw_fd(raw_fd: RawFd) -> Self {
        // SAFETY: caller guarantees `raw_fd` is an owned, live fd-table SOCKET fd.
        Socket(unsafe { FileDesc::from_raw_fd(raw_fd) })
    }
}

impl fmt::Debug for Socket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Socket").field("fd", &self.as_raw_fd()).finish()
    }
}

// ===========================================================================
// SocketAddr <-> hook ABI
// ===========================================================================

/// Decompose a `&SocketAddr` into the `(family, ip octets, ip_len, port)` the
/// hooks expect. Octets are network order (what `IPAddress(ReadOnlySpan<byte>)`
/// wants), so no byte-swap.
fn addr_parts(addr: &SocketAddr) -> (i32, [u8; 16], usize, u16) {
    let mut ip = [0u8; 16];
    match addr {
        SocketAddr::V4(a) => {
            let oct = a.ip().octets();
            ip[..4].copy_from_slice(&oct);
            (FAMILY_V4, ip, 4, a.port())
        }
        SocketAddr::V6(a) => {
            ip = a.ip().octets();
            (FAMILY_V6, ip, 16, a.port())
        }
    }
}

/// Rebuild a `SocketAddr` from the out-buffer triple a hook filled. The hook
/// writes the IP byte-length (4/16) into `out_family`.
fn addr_from_parts(ip_len: i32, ip: &[u8; 16], port: u16) -> io::Result<SocketAddr> {
    match ip_len {
        4 => {
            let oct: [u8; 4] = [ip[0], ip[1], ip[2], ip[3]];
            Ok(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(oct), port)))
        }
        16 => Ok(SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::from(*ip), port, 0, 0))),
        _ => Err(io::const_error!(io::ErrorKind::Uncategorized, "unknown address family")),
    }
}

/// Resolve the first concrete `SocketAddr` from a generic `ToSocketAddrs`.
fn first_addr<A: ToSocketAddrs>(addr: A) -> io::Result<SocketAddr> {
    addr.to_socket_addrs()?
        .next()
        .ok_or_else(|| io::const_error!(io::ErrorKind::InvalidInput, "no socket addresses resolved"))
}

/// Read an out-addr triple from a hook into a `SocketAddr` (local/peer addr).
fn read_out_addr(
    fill: impl FnOnce(*mut i32, *mut u8, *mut u16) -> i32,
) -> io::Result<SocketAddr> {
    let mut family: i32 = 0;
    let mut ip = [0u8; 16];
    let mut port: u16 = 0;
    let rc = fill(&mut family as *mut i32, ip.as_mut_ptr(), &mut port as *mut u16);
    if rc != 0 {
        return Err(io::const_error!(io::ErrorKind::Other, "address query failed"));
    }
    addr_from_parts(family, &ip, port)
}

/// Map a `0 => Ok(()) / nonzero => Err` integer return code from a net hook.
fn rc(code: i32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::const_error!(io::ErrorKind::Other, "dotnet net operation failed"))
    }
}

// ===========================================================================
// TcpStream
// ===========================================================================

pub struct TcpStream {
    inner: Socket,
}

impl TcpStream {
    /// The unified `Socket` backing this stream (for `os/fd/net.rs`).
    #[inline]
    pub fn socket(&self) -> &Socket {
        &self.inner
    }

    /// Consume into the backing `Socket` (for `os/fd/net.rs` `into_raw_fd`).
    #[inline]
    pub fn into_socket(self) -> Socket {
        self.inner
    }

    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        let addr = first_addr(addr)?;
        let (family, ip, ip_len, port) = addr_parts(&addr);
        // SAFETY: `(ip.as_ptr(), ip_len)` is a readable octet buffer; the hook
        // returns an opaque handle (or null on failure).
        let handle = unsafe { rcl_dotnet_net_tcp_connect(family, ip.as_ptr(), ip_len, port) };
        let inner = Socket::from_handle(handle)
            .ok_or_else(|| io::const_error!(io::ErrorKind::Other, "tcp connect failed"))?;
        Ok(TcpStream { inner })
    }

    pub fn connect_timeout(_: &SocketAddr, _: Duration) -> io::Result<TcpStream> {
        unsupported()
    }

    pub fn set_read_timeout(&self, _: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn set_write_timeout(&self, _: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        Ok(None)
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        Ok(None)
    }

    pub fn peek(&self, _: &mut [u8]) -> io::Result<usize> {
        unsupported()
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: writable region; the handle is resolved fresh from the fd-table.
        let n = unsafe { rcl_dotnet_net_recv(self.inner.handle(), buf.as_mut_ptr(), buf.len()) };
        // WouldBlock fix: the shim now returns -1/errno on a non-blocking recv
        // race (errno=EAGAIN → ErrorKind::WouldBlock), so surface the real errno
        // rather than a flat ErrorKind::Other — mio re-polls on WouldBlock.
        let n = cvt(n)?;
        Ok(n as usize)
    }

    pub fn read_buf(&self, cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        crate::io::default_read_buf(|buf| self.read(buf), cursor)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        crate::io::default_read_vectored(|b| self.read(b), bufs)
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: readable region the hook only reads.
        let n = unsafe { rcl_dotnet_net_send(self.inner.handle(), buf.as_ptr(), buf.len()) };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "send failed"));
        }
        Ok(n as usize)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        crate::io::default_write_vectored(|b| self.write(b), bufs)
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        read_out_addr(|f, ip, p|
            // SAFETY: live handle; valid out-pointers.
            unsafe { rcl_dotnet_net_peer_addr(self.inner.handle(), f, ip, p) })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        read_out_addr(|f, ip, p|
            // SAFETY: see `peer_addr`.
            unsafe { rcl_dotnet_net_local_addr(self.inner.handle(), f, ip, p) })
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Read => SHUT_READ,
            Shutdown::Write => SHUT_WRITE,
            Shutdown::Both => SHUT_BOTH,
        };
        // SAFETY: live handle.
        rc(unsafe { rcl_dotnet_net_shutdown(self.inner.handle(), how) })
    }

    pub fn duplicate(&self) -> io::Result<TcpStream> {
        unsupported()
    }

    pub fn set_linger(&self, _: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn linger(&self) -> io::Result<Option<Duration>> {
        Ok(None)
    }

    pub fn set_keepalive(&self, _: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn keepalive(&self) -> io::Result<bool> {
        Ok(false)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        // SAFETY: live handle.
        rc(unsafe { rcl_dotnet_net_set_nodelay(self.inner.handle(), nodelay as i32) })
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        // SAFETY: live handle.
        let r = unsafe { rcl_dotnet_net_nodelay(self.inner.handle()) };
        if r < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "get nodelay failed"));
        }
        Ok(r != 0)
    }

    pub fn set_ttl(&self, _: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        unsupported()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        Ok(None)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        // SAFETY: live handle.
        rc(unsafe { rcl_dotnet_net_set_nonblocking(self.inner.handle(), nonblocking as i32) })
    }

    /// The opaque managed `GCHandle` `IntPtr` backing this socket, resolved fresh
    /// through the fd-table. Bespoke accessor for the dotnet `mio` PAL arm (the
    /// readiness Selector keys sockets by this handle, passing it to
    /// `rcl_dotnet_socket_poll`).
    pub fn dotnet_raw_handle(&self) -> *mut u8 {
        self.inner.handle()
    }
}

impl AsInner<Socket> for TcpStream {
    #[inline]
    fn as_inner(&self) -> &Socket {
        &self.inner
    }
}

impl FromInner<Socket> for TcpStream {
    #[inline]
    fn from_inner(inner: Socket) -> TcpStream {
        TcpStream { inner }
    }
}

impl IntoInner<Socket> for TcpStream {
    #[inline]
    fn into_inner(self) -> Socket {
        self.inner
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpStream").field("fd", &self.inner.as_raw_fd()).finish()
    }
}

// ===========================================================================
// TcpListener
// ===========================================================================

pub struct TcpListener {
    inner: Socket,
}

impl TcpListener {
    #[inline]
    pub fn socket(&self) -> &Socket {
        &self.inner
    }

    #[inline]
    pub fn into_socket(self) -> Socket {
        self.inner
    }

    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<TcpListener> {
        let addr = first_addr(addr)?;
        let (family, ip, ip_len, port) = addr_parts(&addr);
        // SAFETY: readable octet buffer; binds + listens (TCP_BACKLOG > 0).
        let handle = unsafe {
            rcl_dotnet_net_bind(family, ip.as_ptr(), ip_len, port, SOCK_STREAM, TCP_BACKLOG)
        };
        let inner = Socket::from_handle(handle)
            .ok_or_else(|| io::const_error!(io::ErrorKind::Other, "tcp bind failed"))?;
        Ok(TcpListener { inner })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        read_out_addr(|f, ip, p|
            // SAFETY: live handle; valid out-pointers.
            unsafe { rcl_dotnet_net_local_addr(self.inner.handle(), f, ip, p) })
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let mut family: i32 = 0;
        let mut ip = [0u8; 16];
        let mut port: u16 = 0;
        // SAFETY: live listening handle; valid out-pointers; the hook returns the
        // accepted connection's GCHandle and fills the peer address out-buffers.
        let conn = unsafe {
            rcl_dotnet_net_accept(
                self.inner.handle(),
                &mut family as *mut i32,
                ip.as_mut_ptr(),
                &mut port as *mut u16,
            )
        };
        let inner = Socket::from_handle(conn)
            .ok_or_else(|| io::const_error!(io::ErrorKind::Other, "accept failed"))?;
        let peer = addr_from_parts(family, &ip, port)?;
        Ok((TcpStream { inner }, peer))
    }

    pub fn duplicate(&self) -> io::Result<TcpListener> {
        unsupported()
    }

    pub fn set_ttl(&self, _: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        unsupported()
    }

    pub fn set_only_v6(&self, _: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn only_v6(&self) -> io::Result<bool> {
        unsupported()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        Ok(None)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        // SAFETY: live handle.
        rc(unsafe { rcl_dotnet_net_set_nonblocking(self.inner.handle(), nonblocking as i32) })
    }

    /// See `TcpStream::dotnet_raw_handle` — used by the dotnet `mio` PAL arm.
    pub fn dotnet_raw_handle(&self) -> *mut u8 {
        self.inner.handle()
    }
}

impl AsInner<Socket> for TcpListener {
    #[inline]
    fn as_inner(&self) -> &Socket {
        &self.inner
    }
}

impl FromInner<Socket> for TcpListener {
    #[inline]
    fn from_inner(inner: Socket) -> TcpListener {
        TcpListener { inner }
    }
}

impl IntoInner<Socket> for TcpListener {
    #[inline]
    fn into_inner(self) -> Socket {
        self.inner
    }
}

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpListener").field("fd", &self.inner.as_raw_fd()).finish()
    }
}

// ===========================================================================
// UdpSocket
// ===========================================================================

pub struct UdpSocket {
    inner: Socket,
}

impl UdpSocket {
    #[inline]
    pub fn socket(&self) -> &Socket {
        &self.inner
    }

    #[inline]
    pub fn into_socket(self) -> Socket {
        self.inner
    }

    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<UdpSocket> {
        let addr = first_addr(addr)?;
        let (family, ip, ip_len, port) = addr_parts(&addr);
        // SAFETY: readable octet buffer; NO_LISTEN tells the hook to skip Listen.
        let handle = unsafe {
            rcl_dotnet_net_bind(family, ip.as_ptr(), ip_len, port, SOCK_DGRAM, NO_LISTEN)
        };
        let inner = Socket::from_handle(handle)
            .ok_or_else(|| io::const_error!(io::ErrorKind::Other, "udp bind failed"))?;
        Ok(UdpSocket { inner })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        read_out_addr(|f, ip, p|
            // SAFETY: live handle; valid out-pointers.
            unsafe { rcl_dotnet_net_peer_addr(self.inner.handle(), f, ip, p) })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        read_out_addr(|f, ip, p|
            // SAFETY: live handle; valid out-pointers.
            unsafe { rcl_dotnet_net_local_addr(self.inner.handle(), f, ip, p) })
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let mut family: i32 = 0;
        let mut ip = [0u8; 16];
        let mut port: u16 = 0;
        // SAFETY: writable region; valid out-pointers.
        let n = unsafe {
            rcl_dotnet_net_recv_from(
                self.inner.handle(),
                buf.as_mut_ptr(),
                buf.len(),
                &mut family as *mut i32,
                ip.as_mut_ptr(),
                &mut port as *mut u16,
            )
        };
        // WouldBlock fix: surface the real errno (EAGAIN → WouldBlock) on a
        // non-blocking recvfrom race; keep the explicit form because the addr
        // decode below runs only on success.
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        let from = addr_from_parts(family, &ip, port)?;
        Ok((n as usize, from))
    }

    pub fn peek_from(&self, _: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        unsupported()
    }

    pub fn send_to(&self, buf: &[u8], addr: &SocketAddr) -> io::Result<usize> {
        let (family, ip, ip_len, port) = addr_parts(addr);
        // SAFETY: both `(buf.as_ptr(), buf.len())` and `(ip.as_ptr(), ip_len)` are
        // readable regions the hook only reads.
        let n = unsafe {
            rcl_dotnet_net_send_to(
                self.inner.handle(),
                buf.as_ptr(),
                buf.len(),
                family,
                ip.as_ptr(),
                ip_len,
                port,
            )
        };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "send_to failed"));
        }
        Ok(n as usize)
    }

    pub fn duplicate(&self) -> io::Result<UdpSocket> {
        unsupported()
    }

    pub fn set_read_timeout(&self, _: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn set_write_timeout(&self, _: Option<Duration>) -> io::Result<()> {
        unsupported()
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        Ok(None)
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        Ok(None)
    }

    pub fn set_broadcast(&self, _: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn broadcast(&self) -> io::Result<bool> {
        Ok(false)
    }

    pub fn set_multicast_loop_v4(&self, _: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        Ok(false)
    }

    pub fn set_multicast_ttl_v4(&self, _: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        unsupported()
    }

    pub fn set_multicast_loop_v6(&self, _: bool) -> io::Result<()> {
        unsupported()
    }

    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        Ok(false)
    }

    pub fn join_multicast_v4(&self, _: &Ipv4Addr, _: &Ipv4Addr) -> io::Result<()> {
        unsupported()
    }

    pub fn join_multicast_v6(&self, _: &Ipv6Addr, _: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn leave_multicast_v4(&self, _: &Ipv4Addr, _: &Ipv4Addr) -> io::Result<()> {
        unsupported()
    }

    pub fn leave_multicast_v6(&self, _: &Ipv6Addr, _: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn set_ttl(&self, _: u32) -> io::Result<()> {
        unsupported()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        unsupported()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        Ok(None)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        // SAFETY: live handle.
        rc(unsafe { rcl_dotnet_net_set_nonblocking(self.inner.handle(), nonblocking as i32) })
    }

    /// See `TcpStream::dotnet_raw_handle` — used by the dotnet `mio` PAL arm (the
    /// loopback waker socket is a `UdpSocket` registered for readiness).
    pub fn dotnet_raw_handle(&self) -> *mut u8 {
        self.inner.handle()
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: writable region.
        let n = unsafe { rcl_dotnet_net_recv(self.inner.handle(), buf.as_mut_ptr(), buf.len()) };
        // WouldBlock fix: surface the real errno (EAGAIN → WouldBlock) on a
        // non-blocking recv race instead of a flat ErrorKind::Other.
        let n = cvt(n)?;
        Ok(n as usize)
    }

    pub fn peek(&self, _: &mut [u8]) -> io::Result<usize> {
        unsupported()
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: readable region. Requires a prior `connect`.
        let n = unsafe { rcl_dotnet_net_send(self.inner.handle(), buf.as_ptr(), buf.len()) };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "send failed"));
        }
        Ok(n as usize)
    }

    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        let addr = first_addr(addr)?;
        let (family, ip, ip_len, port) = addr_parts(&addr);
        // SAFETY: readable octet buffer; sets the socket's default peer.
        rc(unsafe { rcl_dotnet_net_udp_connect(self.inner.handle(), family, ip.as_ptr(), ip_len, port) })
    }
}

impl AsInner<Socket> for UdpSocket {
    #[inline]
    fn as_inner(&self) -> &Socket {
        &self.inner
    }
}

impl FromInner<Socket> for UdpSocket {
    #[inline]
    fn from_inner(inner: Socket) -> UdpSocket {
        UdpSocket { inner }
    }
}

impl IntoInner<Socket> for UdpSocket {
    #[inline]
    fn into_inner(self) -> Socket {
        self.inner
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpSocket").field("fd", &self.inner.as_raw_fd()).finish()
    }
}

// ===========================================================================
// DNS (stubbed — numeric/loopback addrs are parsed std-side and never reach here)
// ===========================================================================

pub struct LookupHost(());

impl Iterator for LookupHost {
    type Item = SocketAddr;
    fn next(&mut self) -> Option<SocketAddr> {
        None
    }
}

pub fn lookup_host(_host: &str, _port: u16) -> io::Result<LookupHost> {
    unsupported()
}
