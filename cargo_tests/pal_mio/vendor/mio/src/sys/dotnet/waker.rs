// DOTNET PAL ARM
//
// Waker: lets a thread break another thread's blocked `select()`. mio's portable
// trick is a loopback socket pair added to the read set; `wake()` writes a byte so
// the select reports the waker socket readable. On dotnet we use a self-connected
// `std::net::UdpSocket` (bind 127.0.0.1:0, connect to its own local addr) and
// register its handle READABLE in the Selector. `wake()` sends one byte.
//
// pal_mio itself is single-threaded so the waker is not exercised there; it is
// needed by tokio's runtime (a later milestone). Threads work on the dotnet PAL.

use std::io;
use std::net::UdpSocket;

use super::Selector;
use crate::{Interest, Token};

#[derive(Debug)]
pub(crate) struct Waker {
    socket: UdpSocket,
}

impl Waker {
    pub(crate) fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let local = socket.local_addr()?;
        // Self-connect so `send` has a default peer (itself) and the byte loops
        // back, making the socket read-ready.
        socket.connect(local)?;
        socket.set_nonblocking(true)?;
        selector.register(socket.dotnet_raw_handle() as u64, token, Interest::READABLE)?;
        Ok(Waker { socket })
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        match self.socket.send(&[1]) {
            Ok(_) => Ok(()),
            // The datagram buffer is full: a wake is already pending, which is all
            // we need. Drain handled by the Selector after the read-ready report.
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => self.wake(),
            Err(e) => Err(e),
        }
    }
}
