// DOTNET PAL ARM
//
// The readiness Selector. Holds the registered set {socket-handle -> (Token,
// Interest)} behind a shared Mutex (so `try_clone` shares one registry, as the
// other mio arms do). `select` snapshots the set and asks .NET per-socket whether
// it is read/write/error-ready via `rcl_dotnet_socket_poll` (a thin wrapper over
// `System.Net.Sockets.Socket.Poll(int micros, SelectMode)`), translating ready
// sockets into mio `Event`s.

use std::collections::HashMap;
use std::io;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Event, Events};
use crate::{Interest, Token};

// FIXED extern contract — mapped to the .NET BCL by the cilly linker
// (`cilly/src/ir/builtins/dotnet.rs::insert_dotnet_socket_poll`). Do not rename.
//   rcl_dotnet_socket_poll(handle, micros, mode) -> i32
//     => `s.Poll((int)micros, (SelectMode)mode) ? 1 : 0`
// mode: 0 = SelectRead, 1 = SelectWrite, 2 = SelectError. A negative `micros`
// blocks forever. `handle` is the socket's GCHandle (our std::net handle as a
// pointer).
unsafe extern "C" {
    fn rcl_dotnet_socket_poll(handle: *mut u8, micros: i32, mode: i32) -> i32;
}

const SELECT_READ: i32 = 0;
const SELECT_WRITE: i32 = 1;
const SELECT_ERROR: i32 = 2;

/// Unique id for use as `SelectorId`.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
pub struct Selector {
    #[cfg(debug_assertions)]
    id: usize,
    // Shared so `try_clone` (used by `Registry::try_clone`) observes the same
    // registered set as the original.
    registered: Arc<Mutex<HashMap<u64, (Token, Interest)>>>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        Ok(Selector {
            #[cfg(debug_assertions)]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            registered: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        Ok(Selector {
            // It's the same selector, so we use the same id.
            #[cfg(debug_assertions)]
            id: self.id,
            registered: Arc::clone(&self.registered),
        })
    }

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();

        // Snapshot the registered set; do NOT hold the lock across the Poll loop
        // (a `wake()` from another thread, or a do_io reregister, may want it).
        let snapshot: Vec<(u64, Token, Interest)> = {
            let map = self.registered.lock().unwrap();
            map.iter()
                .map(|(h, (t, i))| (*h, *t, *i))
                .collect()
        };

        if snapshot.is_empty() {
            // Nothing registered: honour the timeout by sleeping, so a poll on an
            // empty set behaves like epoll_wait (block for `timeout`, no events).
            if let Some(d) = timeout {
                if !d.is_zero() {
                    std::thread::sleep(d);
                }
            }
            // `None` timeout on an empty set would block forever in epoll; that is
            // a caller bug (no way to ever wake), so we just return with no events.
            return Ok(());
        }

        // Per-socket Poll budget. We sweep the whole set repeatedly until at least
        // one socket is ready or the deadline passes, so a socket that becomes
        // ready *while we are polling another* is not missed. The first socket of
        // each sweep absorbs the (remaining) timeout; the rest are polled with a
        // zero timeout (just a readiness probe).
        let deadline = timeout.map(|d| Instant::now() + d);

        loop {
            let mut any = false;
            // Budget for the *blocking* probe at the head of this sweep.
            let head_micros = match deadline {
                None => -1, // block forever on the first socket
                Some(dl) => {
                    let now = Instant::now();
                    if now >= dl {
                        0
                    } else {
                        clamp_micros(dl - now)
                    }
                }
            };

            for (idx, (handle, token, interest)) in snapshot.iter().enumerate() {
                let h = *handle as *mut u8;
                // Only the first socket of the sweep gets the real timeout; the
                // rest are instantaneous probes.
                let micros = if idx == 0 { head_micros } else { 0 };

                let mut ev = Event::new(*token);
                let mut ready = false;

                if interest.is_readable() {
                    // SAFETY: `h` is a live socket GCHandle (kept alive by the
                    // registered std type the IoSource owns).
                    if unsafe { rcl_dotnet_socket_poll(h, micros, SELECT_READ) } != 0 {
                        ev.readable = true;
                        ev.read_closed = true; // best-effort; readable may be EOF
                        ready = true;
                    }
                }
                if interest.is_writable() {
                    // Subsequent modes after a (possibly) blocking read probe must
                    // not block again — probe with 0.
                    let m = if idx == 0 && !interest.is_readable() { head_micros } else { 0 };
                    if unsafe { rcl_dotnet_socket_poll(h, m, SELECT_WRITE) } != 0 {
                        ev.writable = true;
                        ready = true;
                    }
                    // A failed non-blocking connect surfaces as error-ready (and is
                    // reported as writable by mio's convention). SelectError catches
                    // it so a connecting TcpStream wakes up.
                    if unsafe { rcl_dotnet_socket_poll(h, 0, SELECT_ERROR) } != 0 {
                        ev.error = true;
                        ev.writable = true;
                        ready = true;
                    }
                }

                if ready {
                    // `read_closed` is only meaningful alongside an error; clear the
                    // speculative flag unless we actually saw an error too.
                    if ev.readable && !ev.error {
                        ev.read_closed = false;
                    }
                    events.push(ev);
                    any = true;
                }
            }

            if any {
                return Ok(());
            }
            match deadline {
                None => {
                    // Blocked forever on the head socket but nothing ready: loop and
                    // block again (head_micros stays -1).
                    continue;
                }
                Some(dl) => {
                    if Instant::now() >= dl {
                        return Ok(()); // timed out, no events
                    }
                    // Avoid a busy spin when the head probe returned immediately
                    // (e.g. a tiny remaining budget rounded to 0).
                    std::thread::yield_now();
                }
            }
        }
    }

    pub fn register(&self, socket: u64, token: Token, interests: Interest) -> io::Result<()> {
        let mut map = self.registered.lock().unwrap();
        map.insert(socket, (token, interests));
        Ok(())
    }

    pub fn reregister(&self, socket: u64, token: Token, interests: Interest) -> io::Result<()> {
        let mut map = self.registered.lock().unwrap();
        map.insert(socket, (token, interests));
        Ok(())
    }

    pub fn deregister(&self, socket: u64) -> io::Result<()> {
        let mut map = self.registered.lock().unwrap();
        map.remove(&socket);
        Ok(())
    }
}

cfg_io_source! {
    impl Selector {
        #[cfg(debug_assertions)]
        pub fn id(&self) -> usize {
            self.id
        }
    }
}

/// Clamp a `Duration` to a non-negative `i32` microsecond count for `Socket.Poll`,
/// rounding sub-microsecond up to 1 (so a tiny non-zero timeout is not turned into
/// a zero/instant probe).
fn clamp_micros(d: Duration) -> i32 {
    let micros = d.as_micros();
    if micros == 0 && !d.is_zero() {
        1
    } else if micros > i32::MAX as u128 {
        i32::MAX
    } else {
        micros as i32
    }
}
