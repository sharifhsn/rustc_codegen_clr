// DOTNET PAL ARM: a REAL cross-call Waker (B-tokio-net upgrade of the former
// AtomicBool self-wake stub). This is the ONLY net-new mio source FILE; the rest
// of the vendored tree is byte-identical to crates.io mio 1.2.1 except a handful
// of `target_os = "dotnet"` cfg arms (selector + waker + net SOCK_NONBLOCK + tcp
// accept4 + uds gate-off) and ZERO Cargo edits.
//
// WHY A SEPARATE FILE (cannot use mio's stock waker/eventfd.rs):
// the stock eventfd waker is hardwired to `std::fs::File` (`File::from_raw_fd(fd)`
// then `(&File).write/read` the 8-byte counter). The dotnet std `fs::File` is
// FileStream/GCHandle-backed, NOT fd-table-backed (Cap-1 deferred; feasibility/
// dev.sh defers `impl FromRawFd for fs::File` for dotnet), so the stock waker
// cannot compile here. Fully fd-backing fs::File (Option B) would let us drop this
// file and use stock eventfd.rs — that is an explicitly-DEFERRED follow-up
// (LIBC_SHIM_SCOPE §2.2 / cap2-outcome). What IS enabled for dotnet is
// `std::os::fd::OwnedFd`/`FromRawFd` and the net os/fd impls (dev.sh keeps them
// ON), so this waker holds an `OwnedFd` and read/writes the counter via
// `libc::{read,write}` — mirroring stock eventfd.rs line-for-line, swapping
// File -> OwnedFd. The cilly POSIX shim's `eventfd()` returns a real
// FD_KIND_SOCKET fd (a self-readable loopback UDP socket), so:
//   * register(fd, READABLE) on the epoll selector works (Socket.Poll sweep);
//   * wake()  -> libc::write(fd, &1u64) -> rcl_dotnet_net_send -> the socket
//     becomes readable -> the next epoll_wait reports the Waker token;
//   * reset() -> libc::read(fd, &mut [0;8]) -> rcl_dotnet_net_recv drains it.
// The 8-byte counter degrades to a readiness EDGE (mio reads readiness, not the
// count), which is exactly what the tokio reactor needs.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use crate::sys::Selector;
use crate::{Interest, Token};

#[derive(Debug)]
pub(crate) struct Waker {
    fd: OwnedFd,
}

impl Waker {
    pub(crate) fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        let waker = Waker::new_unregistered()?;
        selector.register(waker.fd.as_raw_fd(), token, Interest::READABLE)?;
        Ok(waker)
    }

    pub(crate) fn new_unregistered() -> io::Result<Waker> {
        // The dotnet shim's eventfd() ignores initval/flags and creates the
        // backing socket non-blocking (== EFD_NONBLOCK). A negative fd is failure.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` is a fresh fd-table SOCKET entry we now own exactly once.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        Ok(Waker { fd })
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        let buf: [u8; 8] = 1u64.to_ne_bytes();
        let n = unsafe {
            libc::write(self.fd.as_raw_fd(), buf.as_ptr() as *const _, buf.len())
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            // A would-block on write means the backing socket's buffer is full —
            // it is ALREADY signalled, so the wake is effectively delivered. Drain
            // once and retry to keep the counter from saturating.
            if err.kind() == io::ErrorKind::WouldBlock {
                self.reset()?;
                return self.wake();
            }
            return Err(err);
        }
        Ok(())
    }

    #[allow(dead_code)] // Only used by the `poll(2)` implementation.
    pub(crate) fn ack_and_reset(&self) {
        let _ = self.reset();
    }

    #[allow(dead_code)] // Only used by the `poll(2)` implementation.
    pub(crate) fn fd(&self) -> Option<RawFd> {
        Some(self.fd.as_raw_fd())
    }

    /// Only ever `true` for the `single_threaded.rs` implementation.
    #[allow(dead_code)] // Only used by the `poll(2)` implementation.
    pub(crate) fn woken(&self) -> bool {
        false
    }

    /// Drain the readiness signal (the 8-byte counter). A would-block means there
    /// was nothing buffered, which is fine.
    fn reset(&self) -> io::Result<()> {
        let mut buf: [u8; 8] = 0u64.to_ne_bytes();
        let n = unsafe {
            libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len())
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                return Ok(());
            }
            return Err(err);
        }
        Ok(())
    }
}

impl AsRawFd for Waker {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
