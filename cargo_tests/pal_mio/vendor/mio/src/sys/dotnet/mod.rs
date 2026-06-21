// DOTNET PAL ARM
//
// mio's system arm for the `os = "dotnet"` target (rustc_codegen_clr's .NET
// backend). os=dotnet has no target_family, so the unix (epoll/kqueue), windows
// (IOCP) and wasi arms are all compiled out — this module supplies the required
// `sys` items (Event/Events/Selector/IoSourceState/Waker + tcp/udp) instead.
//
// The model mirrors mio's UNIX selector (a readiness/`select()` multiplexer, NOT
// completion-based) rather than the windows IOCP arm: each registered socket is
// keyed by its `*mut u8` GCHandle (widened to u64 for the registry map), and
// readiness is queried per-socket via `System.Net.Sockets.Socket.Poll(int
// micros, SelectMode)` exposed by the `rcl_dotnet_socket_poll` backend hook.
// `Socket.Select` would need three managed `IList`s, which the cilly IR cannot
// construct — `Socket.Poll` gives the identical answer one socket at a time.

// `event_impl` holds the `Event`/`Events` types AND the inner `event` accessor
// module; re-exporting all three FLAT here makes the accessors reachable at
// `crate::sys::event::*` (the path `crate::event` expects), matching the epoll arm.
mod event_impl;
pub use event_impl::{event, Event, Events};

mod selector;
pub use selector::Selector;

mod waker;
pub(crate) use waker::Waker;

cfg_net! {
    pub(crate) mod tcp;
    pub(crate) mod udp;
}

mod net;
pub use net::DotnetRawHandle;

cfg_io_source! {
    use std::io;

    use crate::{Interest, Registry, Token};

    /// State for an `IoSource`. Like the windows arm, the readiness state lives
    /// in the (shared) Selector; this only remembers the socket's raw handle so
    /// reregister/deregister can address it, and tracks whether it is currently
    /// registered.
    pub struct IoSourceState {
        // u64 GCHandle of the registered socket; `None` until registered.
        socket: Option<u64>,
    }

    impl IoSourceState {
        pub fn new() -> IoSourceState {
            IoSourceState { socket: None }
        }

        pub fn do_io<T, F, R>(&self, f: F, io: &T) -> io::Result<R>
        where
            F: FnOnce(&T) -> io::Result<R>,
        {
            // The dotnet Selector re-polls its whole registered set on each
            // `select`, so a socket that returns WouldBlock here will simply be
            // re-reported as ready on the next poll once .NET says it is ready.
            // No completion-port re-arming is required (cf. the windows arm),
            // so we just run the op.
            f(io)
        }

        pub fn register(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
            socket: u64,
        ) -> io::Result<()> {
            if self.socket.is_some() {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            registry.selector().register(socket, token, interests)?;
            self.socket = Some(socket);
            Ok(())
        }

        pub fn reregister(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
            socket: u64,
        ) -> io::Result<()> {
            match self.socket {
                Some(_) => registry.selector().reregister(socket, token, interests),
                None => Err(io::ErrorKind::NotFound.into()),
            }
        }

        pub fn deregister(&mut self, registry: &Registry, socket: u64) -> io::Result<()> {
            match self.socket.take() {
                Some(_) => registry.selector().deregister(socket),
                None => Err(io::ErrorKind::NotFound.into()),
            }
        }
    }
}
