//! FLOOR proof for the POSIX/libc-over-.NET shim (the proof slice).
//!
//! A RAW `extern "C"` probe: it declares the bare POSIX symbols (socket/bind/
//! listen/connect/accept/send/recv/close/epoll_*/__errno_location/open) and drives
//! a LOOPBACK TCP echo + an `epoll_wait` readiness wait + an ENOENT errno
//! round-trip, with NO mio and NO std::net. This proves the fd-table + the POSIX
//! symbol wrappers + the thread-local errno end-to-end (LIBC_SHIM_SCOPE §5 Phase 0
//! probe). The symbols are resolved by the cilly linker's `insert_posix_shim`
//! patcher overrides (cilly/src/ir/builtins/posix.rs).
//!
//! Single-threaded: the loopback `connect` to a listening socket completes
//! synchronously on .NET, so `accept` then returns the pending connection without
//! a second thread. Readiness is observed via `epoll_wait` on the accepted fd.
//!
//! SUCCESS = "== pal_libc done ==" after the echo and the errno assertion.

#![allow(non_camel_case_types)]

use core::mem::{size_of, zeroed};

// ---- POSIX C-ABI declarations (resolved by the shim patcher) -----------------
unsafe extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn bind(fd: i32, addr: *const u8, len: u32) -> i32;
    fn listen(fd: i32, backlog: i32) -> i32;
    fn connect(fd: i32, addr: *const u8, len: u32) -> i32;
    fn accept(fd: i32, addr: *mut u8, len: *mut u32) -> i32;
    fn getsockname(fd: i32, addr: *mut u8, len: *mut u32) -> i32;
    fn send(fd: i32, buf: *const u8, len: usize, flags: i32) -> isize;
    fn recv(fd: i32, buf: *mut u8, len: usize, flags: i32) -> isize;
    fn close(fd: i32) -> i32;

    fn epoll_create1(flags: i32) -> i32;
    fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut epoll_event) -> i32;
    fn epoll_wait(epfd: i32, events: *mut epoll_event, maxevents: i32, timeout: i32) -> i32;

    fn open(path: *const u8, flags: i32, mode: i32) -> i32;
    fn __errno_location() -> *mut i32;
}

// ---- Linux ABI structs/constants (the shim hardcodes the Linux layout) -------
const AF_INET: i32 = 2;
const SOCK_STREAM: i32 = 1;
const IPPROTO_TCP: i32 = 6;

const EPOLL_CTL_ADD: i32 = 1;
const EPOLLIN: u32 = 0x001;

const ENOENT: i32 = 2;

// struct sockaddr_in { u16 sin_family; u16 sin_port(BE); u32 sin_addr(BE); u8 _pad[8]; }
#[repr(C)]
#[derive(Clone, Copy)]
struct sockaddr_in {
    sin_family: u16,
    sin_port: u16,   // network (big-endian) order
    sin_addr: u32,   // network (big-endian) order
    _pad: [u8; 8],
}

// struct epoll_event { u32 events; u64 data; } — #[repr(packed)] on x86_64 Linux.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct epoll_event {
    events: u32,
    data: u64,
}

fn sockaddr_loopback(port_host: u16) -> sockaddr_in {
    sockaddr_in {
        sin_family: AF_INET as u16,
        sin_port: port_host.to_be(),
        // 127.0.0.1 = 0x7f000001, stored network order (big-endian).
        sin_addr: 0x7f000001u32.to_be(),
        _pad: [0; 8],
    }
}

fn errno() -> i32 {
    unsafe { *__errno_location() }
}

unsafe fn run() -> Result<String, &'static str> {
    let sa_len = size_of::<sockaddr_in>() as u32;

    // 1. listener: socket + bind(127.0.0.1:0) + listen.
    let listener = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if listener < 0 {
        return Err("socket(listener)");
    }
    let bind_addr = sockaddr_loopback(0);
    if bind(listener, &bind_addr as *const _ as *const u8, sa_len) < 0 {
        return Err("bind");
    }
    if listen(listener, 16) < 0 {
        return Err("listen");
    }

    // discover the assigned port via getsockname.
    let mut local: sockaddr_in = zeroed();
    let mut local_len = sa_len;
    if getsockname(listener, &mut local as *mut _ as *mut u8, &mut local_len) < 0 {
        return Err("getsockname");
    }
    let port = u16::from_be(local.sin_port);
    if port == 0 {
        return Err("port==0");
    }

    // 2. client: socket + connect to the listener's port (loopback connect is
    //    synchronous on .NET, so accept then succeeds in this single thread).
    let client = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if client < 0 {
        return Err("socket(client)");
    }
    let connect_addr = sockaddr_loopback(port);
    if connect(client, &connect_addr as *const _ as *const u8, sa_len) < 0 {
        return Err("connect");
    }

    // 3. accept the pending connection.
    let server = accept(listener, core::ptr::null_mut(), core::ptr::null_mut());
    if server < 0 {
        return Err("accept");
    }

    // 4. client -> server: send "ping-libc".
    let msg = b"ping-libc";
    let n = send(client, msg.as_ptr(), msg.len(), 0);
    if n < 0 {
        return Err("send(client)");
    }

    // 5. epoll readiness: register the server fd READABLE, wait for it.
    let epfd = epoll_create1(0);
    if epfd < 0 {
        return Err("epoll_create1");
    }
    let mut ev = epoll_event { events: EPOLLIN, data: 0x1234 };
    if epoll_ctl(epfd, EPOLL_CTL_ADD, server, &mut ev) < 0 {
        return Err("epoll_ctl");
    }
    let mut out: [epoll_event; 4] = [epoll_event { events: 0, data: 0 }; 4];
    let ready = epoll_wait(epfd, out.as_mut_ptr(), 4, 1000);
    if ready < 1 {
        return Err("epoll_wait(no readiness)");
    }
    let tok = out[0].data;
    if tok != 0x1234 {
        return Err("epoll_wait(wrong token)");
    }

    // 6. server recv + echo back; client recv the echo.
    let mut buf = [0u8; 64];
    let r = recv(server, buf.as_mut_ptr(), buf.len(), 0);
    if r <= 0 {
        return Err("recv(server)");
    }
    let r = r as usize;
    if send(server, buf.as_ptr(), r, 0) < 0 {
        return Err("send(server echo)");
    }
    let mut resp = [0u8; 64];
    let rr = recv(client, resp.as_mut_ptr(), resp.len(), 0);
    if rr <= 0 {
        return Err("recv(client)");
    }
    let rr = rr as usize;
    let got = String::from_utf8_lossy(&resp[..rr]).into_owned();

    close(server);
    close(client);
    close(listener);
    close(epfd);

    Ok(got)
}

unsafe fn errno_roundtrip() -> Result<(), &'static str> {
    // Force an ENOENT: open a path that does not exist; assert -1 + errno==ENOENT.
    let path = b"/nonexistent/pal_libc/definitely_missing\0";
    let fd = open(path.as_ptr(), 0, 0);
    if fd != -1 {
        return Err("open(missing) did not return -1");
    }
    let e = errno();
    if e != ENOENT {
        return Err("errno != ENOENT");
    }
    Ok(())
}

fn main() {
    println!("== pal_libc start ==");
    unsafe {
        match run() {
            Ok(s) => println!("1  libc loopback echo: {:?} (expect \"ping-libc\")", s),
            Err(e) => println!("1  libc loopback ERR: {}", e),
        }
        match errno_roundtrip() {
            Ok(()) => println!("2  errno round-trip: open(missing) -> -1 + errno==ENOENT OK"),
            Err(e) => println!("2  errno round-trip ERR: {}", e),
        }
    }
    println!("== pal_libc done ==");
}
