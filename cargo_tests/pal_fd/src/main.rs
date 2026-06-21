//! fd-traits PAL probe: do `std::os::fd::{AsRawFd, AsFd, FromRawFd, IntoRawFd}`
//! work on the dotnet std net `TcpStream`? Proves the **unified fd-backed Socket**
//! (Cap-1 of the libc-shim capstone): the net `Socket` is genuinely fd-backed
//! over the int-fd ⇄ GCHandle fd-table, so the real `std::os::fd` traits compile
//! and a stream rebuilt purely from a raw int fd still resolves fd → GCHandle and
//! does I/O. Loopback only. Panic-safe (? inside run(), no unwrap).
//! SUCCESS = "== pal_fd done ==".
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};

fn run() -> std::io::Result<()> {
    // Server: echo one message.
    let listener = TcpListener::bind("127.0.0.1:0")?;
    // The listener itself has a real fd (>= 3, since 0/1/2 are stdio).
    let lfd = listener.as_raw_fd();
    println!("1  listener.as_raw_fd() = {lfd} (expect >= 3)");
    assert!(lfd >= 3, "listener fd should be a real fd-table fd >= 3");

    let addr = listener.local_addr()?;
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let (mut s, _peer) = listener.accept()?;
        let mut buf = [0u8; 64];
        let n = s.read(&mut buf)?;
        s.write_all(&buf[..n])?; // echo
        Ok(())
    });

    // Client: connect, then prove the fd round-trip.
    let client = TcpStream::connect(addr)?;
    let cfd = client.as_raw_fd();
    println!("2  client.as_raw_fd()   = {cfd} (expect >= 3)");
    assert!(cfd >= 3, "client fd should be a real fd-table fd >= 3");

    // into_raw_fd() relinquishes ownership of the fd (no close on drop)...
    let raw = client.into_raw_fd();
    println!("3  into_raw_fd()        = {raw}");
    assert_eq!(raw, cfd, "into_raw_fd must yield the same fd");

    // ...and from_raw_fd() rebuilds a TcpStream purely from the int fd. If the
    // Socket is genuinely fd-backed over the fd-table, the rebuilt stream resolves
    // fd -> GCHandle and can still do I/O.
    // SAFETY: `raw` is an owned, live fd-table SOCKET fd we just relinquished.
    let mut rebuilt = unsafe { TcpStream::from_raw_fd(raw) };
    assert_eq!(rebuilt.as_raw_fd(), raw, "rebuilt stream keeps the same fd");
    rebuilt.write_all(b"ping-fd")?;
    let mut resp = [0u8; 64];
    let n = rebuilt.read(&mut resp)?;
    let echoed = String::from_utf8_lossy(&resp[..n]).into_owned();
    println!("4  echo via rebuilt fd  = {echoed:?} (expect \"ping-fd\")");
    assert_eq!(echoed, "ping-fd", "rebuilt-from-fd stream must round-trip I/O");

    let _ = server.join();
    Ok(())
}

fn main() {
    println!("== pal_fd start ==");
    match run() {
        Ok(()) => println!("== pal_fd done =="),
        Err(e) => {
            println!("!! pal_fd FAILED: {e}");
            std::process::exit(1);
        }
    }
}
