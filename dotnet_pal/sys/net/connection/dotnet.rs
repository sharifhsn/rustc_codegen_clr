//! Networking for the .NET ("dotnet") platform.
//!
//! Backs `std::net` (TcpStream / TcpListener / UdpSocket / SocketAddr) with the
//! .NET BCL (`System.Net.Sockets.Socket` + `System.Net.IPAddress` /
//! `IPEndPoint`) through a set of `extern "C"` hooks the cilly linker maps to BCL
//! calls — the same `MissingMethodPatcher` mechanism the alloc / stdio / env /
//! thread / fs arms use. See `cilly/src/ir/builtins/dotnet.rs::insert_dotnet_net`.
//!
//! This arm mirrors the `connection/unsupported.rs` interface shape (the exact
//! item set `std::net` consumes), NOT the libc-coupled `connection/socket`. It is
//! `target_os = "dotnet"`-only and so cannot affect the surrogate target or the
//! `::stable` suite.
//!
//! Handle model: an open socket is a `*mut u8` opaque `GCHandle` `IntPtr` pinning
//! a managed `System.Net.Sockets.Socket`. No managed object (`Socket` /
//! `IPEndPoint` / `IPAddress`) is ever passed through a Rust signature — only
//! opaque handles, `(ptr, len)` byte buffers, and a decomposed `SocketAddr`.
//!
//! SocketAddr ABI: a `&SocketAddr` is decomposed std-side (using the public
//! `core::net` accessors — no DNS, the resolution already happened in
//! `to_socket_addrs`) into `(family: i32, ip_ptr: *const u8, ip_len: usize,
//! port: u16)` where `family` is `4` for IPv4 / `6` for IPv6 and the ip bytes are
//! network-order octets (`Ipv4Addr::octets` / `Ipv6Addr::octets`). The hook
//! rebuilds an `IPEndPoint` BCL-side via `new IPAddress(ReadOnlySpan<byte>)` (no
//! byte-swap: that ctor takes network-order bytes) + `new IPEndPoint(IPAddress,
//! int port)` (port is host-order on `IPEndPoint`, so it is passed as-is — unlike
//! the sockaddr path the libc arms use). Out addresses (accept peer, recv_from
//! sender, local/peer addr) are written back into caller-provided out-buffers
//! `(out_family: *mut i32, out_ip: *mut u8 [16], out_port: *mut u16)`, which the
//! std side reassembles into a `SocketAddr` with `SocketAddrV4`/`V6::new`.
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
//! REAL (BCL-backed): `TcpStream::{connect, read, write, peer_addr, socket_addr,
//! shutdown, set_nodelay, nodelay, set_nonblocking}`; `TcpListener::{bind, accept,
//! socket_addr, set_nonblocking}`; `UdpSocket::{bind, send_to, recv_from, recv,
//! send, peer_addr, socket_addr, connect, set_nonblocking}`. The `*_vectored` /
//! `read_buf` methods delegate to the `crate::io::default_*` adapters over the
//! real `read`/`write` (mirroring the fs `File` arm).
//!
//! STUBBED to `Err(Unsupported)` / sensible default (cfg-gated os=dotnet-only;
//! none are exercised by the Phase-C net probe; being os=dotnet-only they cannot
//! affect the surrogate target or `::stable`):
//!   * timeouts (`connect_timeout`, `set/read/write_timeout`) — could later map to
//!     `Socket.ReceiveTimeout` / `SendTimeout`; not wired.
//!   * `peek` / `peek_from` — `Socket.Receive(.., SocketFlags.Peek)`; follow-up.
//!   * `duplicate` — `Socket.DuplicateAndClose` needs a target process id; not wired.
//!   * `set/get linger`, `set/get keepalive`, `set/get ttl`, `set/get only_v6`,
//!     `set/get broadcast`, all `*_multicast_*` — real via `Socket.SetSocketOption`;
//!     follow-up.
//!   * `take_error` -> `Ok(None)` (no pending-error model on this arm).
//!   * `lookup_host` / `LookupHost` -> `Err(Unsupported)`. The probe (and most
//!     loopback/numeric callers) never hit DNS: `ToSocketAddrs for str` parses a
//!     numeric `SocketAddr` first and only falls back to `lookup_host` on a
//!     parse failure. A real impl = `System.Net.Dns.GetHostAddresses`.
//!   * `hostname()` is NOT provided here — the separate `sys/net/hostname` cascade
//!     has an `_ =>` unsupported arm that already catches os=dotnet, so no dotnet
//!     hostname arm is injected.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::fmt;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs};
use crate::sys::unsupported;
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
}

// Our own ABI address-family ints (NOT libc AF_*): the hook maps these to the
// BCL `AddressFamily` enum (InterNetwork / InterNetworkV6).
const FAMILY_V4: i32 = 4;
const FAMILY_V6: i32 = 6;
// `bind` socket-type selector mapped BCL-side to (SocketType, ProtocolType):
// stream -> (Stream, Tcp), dgram -> (Dgram, Udp).
const SOCK_STREAM: i32 = 1;
const SOCK_DGRAM: i32 = 2;
// A negative backlog tells `bind` to skip `Socket.Listen` (UDP sockets bind but
// never listen). TcpListener passes a real backlog.
const NO_LISTEN: i32 = -1;
const TCP_BACKLOG: i32 = 128;
// `System.Net.Sockets.SocketShutdown`: Receive=0, Send=1, Both=2.
const SHUT_READ: i32 = 0;
const SHUT_WRITE: i32 = 1;
const SHUT_BOTH: i32 = 2;

/// Decompose a `&SocketAddr` into the `(family, ip octets, ip_len, port)` the
/// hooks expect. The `[u8; 16]` always has capacity for IPv6; for IPv4 only the
/// first 4 bytes are meaningful and `ip_len` is 4. The octets are network order
/// (what `IPAddress(ReadOnlySpan<byte>)` wants), so no byte-swap.
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
/// writes the IP byte-length (4 for IPv4 / 16 for IPv6 — what
/// `IPAddress.GetAddressBytes().Length` yields) into `out_family`, so this maps
/// length -> address family; `ip` holds network-order octets. IPv6 scope-id /
/// flowinfo are not surfaced by this arm (always 0) — adequate for loopback and
/// the common case; a richer impl would pass them through extra out-params.
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

/// Resolve the first concrete `SocketAddr` from a generic `ToSocketAddrs`. DNS
/// resolution (if any) happens here, std-side, before the addr reaches a hook —
/// mirroring the `motor` arm's `to_socket_addrs()?.next()` pattern (the
/// `each_addr` helper in `connection/mod.rs` is only used by the libc `socket`
/// arm, and is `#[allow(dead_code)]` for us).
fn first_addr<A: ToSocketAddrs>(addr: A) -> io::Result<SocketAddr> {
    addr.to_socket_addrs()?
        .next()
        .ok_or_else(|| io::const_error!(io::ErrorKind::InvalidInput, "no socket addresses resolved"))
}

/// Read an out-addr triple from a hook into a `SocketAddr`. Shared by
/// local_addr / peer_addr. The `fill` closure invokes a hook with the three
/// valid out-pointers and returns its int rc; the SAFETY obligation (live
/// handle, valid out-pointers) is discharged by the caller's own SAFETY note.
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

pub struct TcpStream {
    /// Opaque managed `GCHandle` `IntPtr` pinning a `System.Net.Sockets.Socket`.
    handle: *mut u8,
}

// SAFETY: the handle is an opaque managed `GCHandle` `IntPtr`; moving it between
// threads is sound (it identifies a managed `Socket`, not thread-affine native
// state). Mirrors the `File` / `Thread` arms.
unsafe impl Send for TcpStream {}
unsafe impl Sync for TcpStream {}

impl TcpStream {
    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        let addr = first_addr(addr)?;
        let (family, ip, ip_len, port) = addr_parts(&addr);
        // SAFETY: `(ip.as_ptr(), ip_len)` is a readable octet buffer the hook only
        // reads; it returns an opaque non-null handle (a managed exception unwinds
        // on connect failure).
        let handle = unsafe { rcl_dotnet_net_tcp_connect(family, ip.as_ptr(), ip_len, port) };
        if handle.is_null() {
            return Err(io::const_error!(io::ErrorKind::Other, "tcp connect failed"));
        }
        Ok(TcpStream { handle })
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
        // SAFETY: `(buf.as_mut_ptr(), buf.len())` is an exclusively-borrowed
        // writable region the hook writes at most `len` bytes into. A 0 return is
        // an orderly shutdown (EOF), naturally mapped to `Ok(0)`.
        let n = unsafe { rcl_dotnet_net_recv(self.handle, buf.as_mut_ptr(), buf.len()) };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "recv failed"));
        }
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
        // SAFETY: `(buf.as_ptr(), buf.len())` is a readable region the hook only
        // reads, sending up to `len` bytes.
        let n = unsafe { rcl_dotnet_net_send(self.handle, buf.as_ptr(), buf.len()) };
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
            // SAFETY: `self.handle` is a live Socket handle; the out-pointers are
            // valid for the call and written through only on success.
            unsafe { rcl_dotnet_net_peer_addr(self.handle, f, ip, p) })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        read_out_addr(|f, ip, p|
            // SAFETY: see `peer_addr`.
            unsafe { rcl_dotnet_net_local_addr(self.handle, f, ip, p) })
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Read => SHUT_READ,
            Shutdown::Write => SHUT_WRITE,
            Shutdown::Both => SHUT_BOTH,
        };
        // SAFETY: `self.handle` is a live Socket handle.
        rc(unsafe { rcl_dotnet_net_shutdown(self.handle, how) })
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
        // SAFETY: `self.handle` is a live Socket handle.
        rc(unsafe { rcl_dotnet_net_set_nodelay(self.handle, nodelay as i32) })
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        // SAFETY: `self.handle` is a live Socket handle.
        let r = unsafe { rcl_dotnet_net_nodelay(self.handle) };
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
        // SAFETY: `self.handle` is a live Socket handle.
        rc(unsafe { rcl_dotnet_net_set_nonblocking(self.handle, nonblocking as i32) })
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: `self.handle` is a live Socket handle; the hook disposes the
            // socket and frees the GCHandle exactly once (Drop).
            unsafe { rcl_dotnet_net_close(self.handle) };
        }
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpStream").field("handle", &self.handle).finish()
    }
}

pub struct TcpListener {
    handle: *mut u8,
}

// SAFETY: see `TcpStream`.
unsafe impl Send for TcpListener {}
unsafe impl Sync for TcpListener {}

impl TcpListener {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<TcpListener> {
        let addr = first_addr(addr)?;
        let (family, ip, ip_len, port) = addr_parts(&addr);
        // SAFETY: `(ip.as_ptr(), ip_len)` is a readable octet buffer; the hook
        // binds + listens (TCP_BACKLOG > 0) and returns an opaque non-null handle.
        let handle = unsafe {
            rcl_dotnet_net_bind(family, ip.as_ptr(), ip_len, port, SOCK_STREAM, TCP_BACKLOG)
        };
        if handle.is_null() {
            return Err(io::const_error!(io::ErrorKind::Other, "tcp bind failed"));
        }
        Ok(TcpListener { handle })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        read_out_addr(|f, ip, p|
            // SAFETY: `self.handle` is a live Socket handle; valid out-pointers.
            unsafe { rcl_dotnet_net_local_addr(self.handle, f, ip, p) })
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let mut family: i32 = 0;
        let mut ip = [0u8; 16];
        let mut port: u16 = 0;
        // SAFETY: `self.handle` is a live listening Socket; the out-pointers are
        // valid for the duration of the call; the hook returns the accepted
        // connection's handle and fills the peer address out-buffers.
        let conn = unsafe {
            rcl_dotnet_net_accept(
                self.handle,
                &mut family as *mut i32,
                ip.as_mut_ptr(),
                &mut port as *mut u16,
            )
        };
        if conn.is_null() {
            return Err(io::const_error!(io::ErrorKind::Other, "accept failed"));
        }
        let peer = addr_from_parts(family, &ip, port)?;
        Ok((TcpStream { handle: conn }, peer))
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
        // SAFETY: `self.handle` is a live Socket handle.
        rc(unsafe { rcl_dotnet_net_set_nonblocking(self.handle, nonblocking as i32) })
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: see `TcpStream::drop`.
            unsafe { rcl_dotnet_net_close(self.handle) };
        }
    }
}

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpListener").field("handle", &self.handle).finish()
    }
}

pub struct UdpSocket {
    handle: *mut u8,
}

// SAFETY: see `TcpStream`.
unsafe impl Send for UdpSocket {}
unsafe impl Sync for UdpSocket {}

impl UdpSocket {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<UdpSocket> {
        let addr = first_addr(addr)?;
        let (family, ip, ip_len, port) = addr_parts(&addr);
        // SAFETY: readable octet buffer; NO_LISTEN tells the hook to skip Listen
        // (a UDP socket binds but does not listen). Returns an opaque non-null handle.
        let handle = unsafe {
            rcl_dotnet_net_bind(family, ip.as_ptr(), ip_len, port, SOCK_DGRAM, NO_LISTEN)
        };
        if handle.is_null() {
            return Err(io::const_error!(io::ErrorKind::Other, "udp bind failed"));
        }
        Ok(UdpSocket { handle })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        read_out_addr(|f, ip, p|
            // SAFETY: `self.handle` is a live Socket handle; valid out-pointers.
            unsafe { rcl_dotnet_net_peer_addr(self.handle, f, ip, p) })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        read_out_addr(|f, ip, p|
            // SAFETY: `self.handle` is a live Socket handle; valid out-pointers.
            unsafe { rcl_dotnet_net_local_addr(self.handle, f, ip, p) })
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let mut family: i32 = 0;
        let mut ip = [0u8; 16];
        let mut port: u16 = 0;
        // SAFETY: `(buf.as_mut_ptr(), buf.len())` is a writable region; the
        // out-pointers are valid for the call. The hook returns the byte count
        // received and fills the sender address out-buffers.
        let n = unsafe {
            rcl_dotnet_net_recv_from(
                self.handle,
                buf.as_mut_ptr(),
                buf.len(),
                &mut family as *mut i32,
                ip.as_mut_ptr(),
                &mut port as *mut u16,
            )
        };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "recv_from failed"));
        }
        let from = addr_from_parts(family, &ip, port)?;
        Ok((n as usize, from))
    }

    pub fn peek_from(&self, _: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        unsupported()
    }

    pub fn send_to(&self, buf: &[u8], addr: &SocketAddr) -> io::Result<usize> {
        let (family, ip, ip_len, port) = addr_parts(addr);
        // SAFETY: `(buf.as_ptr(), buf.len())` and `(ip.as_ptr(), ip_len)` are both
        // readable regions the hook only reads.
        let n = unsafe {
            rcl_dotnet_net_send_to(
                self.handle,
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
        // SAFETY: `self.handle` is a live Socket handle.
        rc(unsafe { rcl_dotnet_net_set_nonblocking(self.handle, nonblocking as i32) })
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: writable region; the hook writes at most `len` bytes.
        let n = unsafe { rcl_dotnet_net_recv(self.handle, buf.as_mut_ptr(), buf.len()) };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "recv failed"));
        }
        Ok(n as usize)
    }

    pub fn peek(&self, _: &mut [u8]) -> io::Result<usize> {
        unsupported()
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // SAFETY: readable region the hook only reads. Requires a prior `connect`.
        let n = unsafe { rcl_dotnet_net_send(self.handle, buf.as_ptr(), buf.len()) };
        if n < 0 {
            return Err(io::const_error!(io::ErrorKind::Other, "send failed"));
        }
        Ok(n as usize)
    }

    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        let addr = first_addr(addr)?;
        let (family, ip, ip_len, port) = addr_parts(&addr);
        // SAFETY: readable octet buffer; sets the socket's default peer.
        rc(unsafe { rcl_dotnet_net_udp_connect(self.handle, family, ip.as_ptr(), ip_len, port) })
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: see `TcpStream::drop`.
            unsafe { rcl_dotnet_net_close(self.handle) };
        }
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpSocket").field("handle", &self.handle).finish()
    }
}

/// DNS resolution result. STUBBED: this arm never produces results (see the
/// module-doc `lookup_host` note). The `!`-free shape (vs `unsupported.rs`'s
/// `LookupHost(!)`) keeps the `Iterator` impl total without `feature(never_type)`.
pub struct LookupHost(());

impl Iterator for LookupHost {
    type Item = SocketAddr;
    fn next(&mut self) -> Option<SocketAddr> {
        None
    }
}

pub fn lookup_host(_host: &str, _port: u16) -> io::Result<LookupHost> {
    // Loopback / numeric addresses are parsed std-side and never reach here; a
    // real impl would call System.Net.Dns.GetHostAddresses. See module doc.
    unsupported()
}

/// Map a `0 => Ok(()) / nonzero => Err` integer return code from a net hook.
fn rc(code: i32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::const_error!(io::ErrorKind::Other, "dotnet net operation failed"))
    }
}
