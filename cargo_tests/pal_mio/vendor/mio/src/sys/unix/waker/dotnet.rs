// DOTNET PAL ARM (B1 convergence): the ONLY net-new mio source FILE. Everything
// else in the vendored tree is byte-identical to crates.io mio 1.2.1 except a
// handful of `target_os = "dotnet"` cfg arms (selector + waker + net SOCK_NONBLOCK
// + tcp accept4 + uds gate-off) and ZERO Cargo edits. This file is the irreducible
// remainder: it exists ONLY because the dotnet std `fs::File` is not yet fd-backed,
// so mio's stock eventfd waker cannot be used.
//
// mio's epoll selector re-exports `crate::sys::unix::waker::Waker` RAW (it does
// not wrap it like the poll selector does), so that `Waker` must expose
// `new(selector, token)` + `wake()` directly (waker.rs::Waker::new calls
// `sys::Waker::new(registry.selector(), token)`). The stock File-free waker
// (single_threaded.rs) only has `new_unregistered()` — it is the poll selector's
// internal waker, NOT a drop-in for the epoll selector. The stock eventfd waker
// has the right shape but needs `std::fs::File: FromRawFd`, which the dotnet std
// fs::File (GCHandle/FileStream) is not (deferred).
//
// So this minimal waker satisfies the epoll selector's API surface with the
// signatures it actually uses. pal_mio builds NO Waker, so `wake()` is never
// exercised at runtime; it is here so mio's public `Waker` type-checks. A real
// cross-thread waker (loopback-socket eventfd, registered readable) is the
// documented follow-up for tokio's reactor (which DOES construct one).
use crate::sys::Selector;
use crate::Token;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

#[derive(Debug)]
pub(crate) struct Waker {
    woken: AtomicBool,
}

impl Waker {
    pub(crate) fn new(_selector: &Selector, _token: Token) -> io::Result<Waker> {
        Ok(Waker {
            woken: AtomicBool::new(false),
        })
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        // Single managed thread: record the request. The epoll_wait sweep
        // (Socket.Poll over the interest set) re-checks readiness each pass, so a
        // self-wake is a no-op here — pal_mio never crosses threads.
        self.woken.store(true, Relaxed);
        Ok(())
    }
}
