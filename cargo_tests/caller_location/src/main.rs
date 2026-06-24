//! Differential regression for `#[track_caller]` / `Location::caller()` text.
//!
//! Before the P2-S3 fix the backend materialized the *intrinsic's own* span, so every
//! `Location::caller()` reported `core::panic::Location::caller`'s body
//! (`library/core/src/panic/location.rs:181:9`) instead of the real user call site, and the
//! propagation up a `#[track_caller]` chain was lost. We print only line/column (never the file
//! path) so the result is independent of the build working directory.

use std::panic::Location;

// Depth-1: a single `#[track_caller]` hop must report *our* call site, not its own body.
#[track_caller]
fn depth1() -> &'static Location<'static> {
    Location::caller()
}

// Depth-2: the location must propagate through two `#[track_caller]` hops to the user site.
#[track_caller]
fn depth2_inner() -> &'static Location<'static> {
    Location::caller()
}
#[track_caller]
fn depth2_outer() -> &'static Location<'static> {
    depth2_inner()
}

// A `#[track_caller]` fn that uses the location locally (formatting), not just returns it.
#[track_caller]
fn describe() -> (u32, u32) {
    let l = Location::caller();
    (l.line(), l.column())
}

// A `#[track_caller]` fn that *takes arguments* — the implicit `&Location` is then the trailing CIL
// arg at index `arg_count` (here `LdArg(2)`), exactly the shape of `core::panicking::panic_bounds_check`.
// This guards the non-zero-arg arm of the forwarding logic.
#[track_caller]
fn with_args(_a: i32, _b: i32) -> &'static Location<'static> {
    Location::caller()
}
// Two-arg track_caller forwarding through another two-arg track_caller hop.
#[track_caller]
fn with_args_outer(a: i32, b: i32) -> &'static Location<'static> {
    with_args(a, b)
}

fn main() {
    let a = depth1();                                   // call site A
    println!("depth1 {} {}", a.line(), a.column());

    let b = depth2_outer();                             // call site B
    println!("depth2 {} {}", b.line(), b.column());

    let (cl, cc) = describe();                          // call site C
    println!("describe {} {}", cl, cc);

    let e = with_args(1, 2);                            // call site E (arg_count=2 forwarding)
    println!("with_args {} {}", e.line(), e.column());

    let f = with_args_outer(3, 4);                      // call site F (2-arg -> 2-arg chain)
    println!("with_args_outer {} {}", f.line(), f.column());

    // Non-track_caller context: `Location::caller()` reports *this* statement's location.
    let here = Location::caller();                      // call site D
    println!("here {} {}", here.line(), here.column());
}
