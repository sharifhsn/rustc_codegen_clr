// DOTNET PAL ARM
//
// UDP helpers used by `crate::net::UdpSocket`. The dotnet std::net::UdpSocket has
// no raw-fd path, so `bind` builds through std and leaves the socket blocking by
// default (mio sets non-blocking via `from_std` callers / IoSource); the waker
// sets non-blocking explicitly. `only_v6` is not surfaced by the dotnet std arm,
// so it reports `false` (IPv4 loopback is all pal_mio uses).

use std::io;
use std::net::{self, SocketAddr};

pub(crate) fn bind(addr: SocketAddr) -> io::Result<net::UdpSocket> {
    let socket = net::UdpSocket::bind(addr)?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

pub(crate) fn only_v6(_socket: &net::UdpSocket) -> io::Result<bool> {
    // The dotnet std::net arm does not expose IPV6_V6ONLY; default to false.
    Ok(false)
}
