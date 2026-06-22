//! Networking primitives.
//!
//! The types provided in this module are non-blocking by default and are
//! designed to be portable across all supported Mio platforms. As long as the
//! [portability guidelines] are followed, the behavior should be identical no
//! matter the target platform.
//!
//! [portability guidelines]: ../struct.Poll.html#portability
//!
//! # Notes
//!
//! When using a datagram based socket, i.e. [`UdpSocket`] or [`UnixDatagram`],
//! it's only possible to receive a packet once. This means that if you provide a
//! buffer that is too small you won't be able to receive the data anymore. How
//! OSs deal with this situation is different for each OS:
//!  * Unixes, such as Linux, FreeBSD and macOS, will simply fill the buffer and
//!    return the amount of bytes written. This means that if the returned value
//!    is equal to the size of the buffer it may have only written a part of the
//!    packet (or the packet has the same size as the buffer).
//!  * Windows returns an `WSAEMSGSIZE` error.
//!
//! Mio does not change the value (either ok or error) returned by the OS, it's
//! up to the user to handle this. How to deal with these differences is still up
//! for debate, specifically in
//! <https://github.com/rust-lang/rust/issues/55794>. The best advice we can
//! give is to always call receive with a large enough buffer.

mod tcp;
pub use self::tcp::{TcpListener, TcpStream};

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
mod udp;
#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub use self::udp::UdpSocket;

// DOTNET PAL ARM (B1 convergence): cfg(unix) is now global (the target-family
// flip), so mio's TCP/UDP epoll path activates from --target. But its unix-DOMAIN-
// socket module needs `std::os::unix::net::SocketAddr::{from_pathname,
// from_abstract_name}` — the abstract-namespace AF_UNIX surface the dotnet PAL
// only compile-stubs as Err(Unsupported) / drops by linux-bsd cfg (impossible on
// stock CoreCLR). Gate uds off for os=dotnet; pal_mio uses only TCP/UDP. This is
// part of the irreducible remainder (see docs/LIBC_SHIM_SCOPE.md §4.5).
#[cfg(all(unix, not(target_os = "dotnet")))]
mod uds;
#[cfg(all(unix, not(target_os = "dotnet")))]
pub use self::uds::{UnixDatagram, UnixListener, UnixStream};
