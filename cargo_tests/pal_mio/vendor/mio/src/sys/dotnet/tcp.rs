// DOTNET PAL ARM
//
// TCP helpers used by `crate::net::tcp`. Unlike the unix/windows arms (which work
// with raw fds/sockets via `new_for_addr` + `from_raw_*`), the dotnet std::net
// types have no raw-fd constructor, so `TcpListener::bind` / `TcpStream::connect`
// build through `std::net` directly (see the `#[cfg(target_os = "dotnet")]` arms
// in net/tcp/{listener,stream}.rs). Only `accept` is needed here — it just
// delegates to the std listener (like the windows arm).

use std::io;
use std::net::{self, SocketAddr};

pub(crate) fn accept(listener: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    listener.accept()
}
