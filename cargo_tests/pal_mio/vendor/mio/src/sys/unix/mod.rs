/// Helper macro to execute a system call that returns an `io::Result`.
//
// Macro must be defined before any modules that use them.
#[allow(unused_macros)]
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        #[allow(unused_unsafe)]
        let res = unsafe { libc::$fn($($arg, )*) };
        if res < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

cfg_os_poll! {
    // DOTNET PAL ARM (B1 convergence): mio selects its readiness backend by
    // `target_os` — epoll.rs only for {android, illumos, linux, redox}. os=dotnet
    // is none of those, so without help no selector path matches and `mod selector`
    // fails to resolve. Cap-2.5 papered over this with a crate-scoped RUSTC_WRAPPER
    // that layered `--cfg target_os="linux"` onto mio. Now that cfg(unix) is global
    // (the target-family flip), the wrapper is GONE; instead route os=dotnet to
    // epoll.rs explicitly (one `#[path]` arm, symmetric with the waker arm below).
    // The epoll path drives `libc::epoll_*`/`socket`/... whose bodies the cilly
    // POSIX shim resolves by bare C-ABI symbol name (per-fd Socket.Poll loop).
    #[cfg_attr(target_os = "dotnet", path = "selector/epoll.rs")]
    #[cfg_attr(all(
        not(target_os = "dotnet"),
        not(mio_unsupported_force_poll_poll),
        any(
            target_os = "android",
            target_os = "illumos",
            target_os = "linux",
            target_os = "redox",
        )
    ), path = "selector/epoll.rs")]
    #[cfg_attr(all(
        not(mio_unsupported_force_poll_poll),
        any(
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "ios",
            target_os = "macos",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "tvos",
            target_os = "visionos",
            target_os = "watchos",
        )
    ), path = "selector/kqueue.rs")]
    #[cfg_attr(any(
        mio_unsupported_force_poll_poll,
        target_os = "aix",
        target_os = "espidf",
        target_os = "fuchsia",
        target_os = "haiku",
        target_os = "hermit",
        target_os = "hurd",
        target_os = "nto",
        target_os = "solaris",
        target_os = "vita",
        target_os = "cygwin",
        target_os = "wasi",
        target_os = "horizon"
    ), path = "selector/poll.rs")]
    mod selector;
    pub(crate) use self::selector::*;

    // DOTNET PAL ARM (B1 convergence): mio's waker cascade selects `waker/eventfd.rs`
    // for linux, which needs `std::fs::File: FromRawFd` — but the dotnet std
    // `fs::File` is GCHandle/FileStream-backed, not fd-backed (deferred — THIS is
    // the irreducible blocker that keeps mio from being literal zero-patch). The
    // epoll selector re-exports `Waker` RAW (needs `new(selector,token)`+`wake()`,
    // which the File-free single_threaded.rs lacks). So route to a minimal dotnet
    // waker (waker/dotnet.rs) with exactly that surface; pal_mio builds no Waker,
    // so it is never exercised. Gated `target_os = "dotnet"`; the upstream cascade
    // below is gated `not(... "dotnet")` so exactly one `mod waker;` is in scope
    // for os=dotnet (no other target_os is true for our target).
    #[cfg(target_os = "dotnet")]
    #[path = "waker/dotnet.rs"]
    mod waker;
    #[cfg(not(target_os = "dotnet"))]
    #[cfg_attr(all(
        not(mio_unsupported_force_waker_pipe),
        any(
            target_os = "android",
            target_os = "espidf",
            target_os = "fuchsia",
            target_os = "hermit",
            target_os = "illumos",
            target_os = "linux",
        )
    ), path = "waker/eventfd.rs")]
    #[cfg_attr(all(
        not(mio_unsupported_force_waker_pipe),
        not(mio_unsupported_force_poll_poll), // `kqueue(2)` based waker doesn't work with `poll(2)`.
        any(
            target_os = "freebsd",
            target_os = "ios",
            target_os = "macos",
            target_os = "tvos",
            target_os = "visionos",
            target_os = "watchos",
        )
    ), path = "waker/kqueue.rs")]
    #[cfg_attr(any(
        // NOTE: also add to the list for the `pipe` module below.
        mio_unsupported_force_waker_pipe,
        all(
            // `kqueue(2)` based waker doesn't work with `poll(2)`.
            mio_unsupported_force_poll_poll,
            any(
                target_os = "freebsd",
                target_os = "ios",
                target_os = "macos",
                target_os = "tvos",
                target_os = "visionos",
                target_os = "watchos",
            ),
        ),
        target_os = "aix",
        target_os = "dragonfly",
        target_os = "haiku",
        target_os = "hurd",
        target_os = "netbsd",
        target_os = "nto",
        target_os = "openbsd",
        target_os = "redox",
        target_os = "solaris",
        target_os = "vita",
        target_os = "cygwin",
        all(target_os = "wasi", target_env = "p1")
    ), path = "waker/pipe.rs")]
    #[cfg_attr(any(target_os = "horizon", all(target_os = "wasi", not(target_env = "p1"))), path = "waker/single_threaded.rs")]
    mod waker;
    // NOTE: the `Waker` type is expected in the selector module as the
    // `poll(2)` implementation needs to do some special stuff.

    #[cfg(feature = "os-ext")]
    mod sourcefd;
    #[cfg(feature = "os-ext")]
    pub use self::sourcefd::SourceFd;

    cfg_net! {
        mod net;

        pub(crate) mod tcp;
        pub(crate) mod udp;
        // DOTNET PAL ARM (B1 convergence): uds needs std::os::unix::net abstract-
        // namespace AF_UNIX surface (impossible on stock CoreCLR); see net/mod.rs.
        // Gate off for os=dotnet (irreducible remainder).
        #[cfg(not(any(target_os = "hermit", target_os = "wasi", target_os = "dotnet")))]
        pub(crate) mod uds;
    }

    #[cfg(all(
        any(
            // For the public `pipe` module, must match `cfg_os_ext` macro.
            feature = "os-ext",
            // For the `Waker` type based on a pipe.
            mio_unsupported_force_waker_pipe,
            all(
                // `kqueue(2)` based waker doesn't work with `poll(2)`.
                mio_unsupported_force_poll_poll,
                any(
                    target_os = "freebsd",
                    target_os = "ios",
                    target_os = "macos",
                    target_os = "tvos",
                    target_os = "visionos",
                    target_os = "watchos",
                ),
            ),
            // NOTE: also add to the list for the `pipe` module below.
            target_os = "aix",
            target_os = "dragonfly",
            target_os = "haiku",
            target_os = "hurd",
            target_os = "netbsd",
            target_os = "nto",
            target_os = "openbsd",
            target_os = "redox",
            target_os = "solaris",
            target_os = "vita",
            target_os = "cygwin",
        ),
        not(target_os = "hermit"),
        not(target_os = "wasi"),
    ))]
    pub(crate) mod pipe;
}

cfg_not_os_poll! {
    cfg_os_ext! {
        mod sourcefd;
        pub use self::sourcefd::SourceFd;
    }
}
