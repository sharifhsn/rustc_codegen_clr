//! `sys::io::is_terminal` for the .NET ("dotnet") platform — Cap-1 foundation arm.
//!
//! Injected as the FIRST arm of the NESTED `cfg_select!` inside
//! `mod is_terminal { … }` in `sys/io/mod.rs`, so the unix `isatty` arm (gated on
//! `target_family="unix"`) never wins at the Cap-2 `families=["unix"]` flip.
//!
//! It MUST be the generic `is_terminal<T>(_: &T) -> bool` form (mirroring
//! `unsupported`), NOT the `isatty` form `is_terminal(fd: &impl AsFd)`: the
//! callers in `io/stdio.rs` invoke it on `Stdin`/`Stdout`/`Stderr`/`File`, none of
//! which implement `AsFd` on os=dotnet, so an `AsFd`-bounded signature would not
//! type-check. (`os/fd/owned.rs`'s `BorrowedFd`/`OwnedFd` callers DO satisfy the
//! generic `&T` too.)
//!
//! With `families` UNSET this is a pure no-op: dotnet already falls to the
//! `unsupported` arm (also `false`). A REAL `Console.Is{Input,Output,Error}Redirected`
//! upgrade would need fd→stream inspection that the generic-`T` signature cannot
//! express, so it is deferred (would require keying on the stdio handle type, not
//! a generic fd). Returning `false` is honest: nothing is reported as a TTY.

pub fn is_terminal<T>(_: &T) -> bool {
    false
}
