// DOTNET PAL ARM
//
// `Event`/`Events` + the `event` accessor module required by `crate::event`.
// Modeled on the epoll arm (`Events = Vec<Event>`), but `Event` is a plain
// readiness record (token + readable/writable/error bits) because the dotnet
// Selector produces readiness directly from `Socket.Poll`, with no kernel event
// struct to wrap.
//
// This file is re-exported FLAT from `sys/dotnet/mod.rs`
// (`pub use event_impl::{event, Event, Events}`) so the public `crate::event`
// layer reaches the accessors as `crate::sys::event::token(..)` etc. — exactly
// the path shape the epoll arm provides.

use std::fmt;

use crate::Token;

#[derive(Clone)]
pub struct Event {
    pub(crate) token: usize,
    pub(crate) readable: bool,
    pub(crate) writable: bool,
    pub(crate) error: bool,
    pub(crate) read_closed: bool,
}

impl Event {
    pub(crate) fn new(token: Token) -> Event {
        Event {
            token: usize::from(token),
            readable: false,
            writable: false,
            error: false,
            read_closed: false,
        }
    }
}

pub type Events = Vec<Event>;

pub mod event {
    use std::fmt;

    use crate::sys::Event;
    use crate::Token;

    pub fn token(event: &Event) -> Token {
        Token(event.token)
    }

    pub fn is_readable(event: &Event) -> bool {
        event.readable
    }

    pub fn is_writable(event: &Event) -> bool {
        event.writable
    }

    pub fn is_error(event: &Event) -> bool {
        event.error
    }

    pub fn is_read_closed(event: &Event) -> bool {
        event.read_closed
    }

    pub fn is_write_closed(event: &Event) -> bool {
        // A write-half close surfaces as an error on the dotnet readiness path.
        event.error
    }

    pub fn is_priority(_: &Event) -> bool {
        // Out-of-band / priority data is not surfaced by this arm.
        false
    }

    pub fn is_aio(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub fn is_lio(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub fn debug_details(f: &mut fmt::Formatter<'_>, event: &Event) -> fmt::Result {
        f.debug_struct("dotnet_event")
            .field("token", &event.token)
            .field("readable", &event.readable)
            .field("writable", &event.writable)
            .field("error", &event.error)
            .field("read_closed", &event.read_closed)
            .finish()
    }
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        event::debug_details(f, self)
    }
}
