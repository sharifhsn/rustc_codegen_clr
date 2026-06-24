//! Differential regression for `UnwindAction::Terminate` (P2-S4 / seam-audit Slice A).
//!
//! A panic that would cross a `nounwind` ABI boundary (here: a panic inside an `extern "C"` fn) MUST
//! abort the process **uncatchably** — `catch_unwind` is required NOT to absorb it. Before P2-S4 the
//! backend mapped the `Terminate` unwind *action* to `None` (no handler), so the panic propagated as
//! an ordinary managed exception and `catch_unwind` wrongly returned `Err` → the program printed
//! `REACHED` and exited 0. The fix routes the `Terminate` action to a synthetic `FailFast` handler
//! (shared with the `UnwindTerminate` terminator), so the process hard-aborts.
//!
//! ASSERTION (apphost-robust): **stdout must be exactly `start\n` — never `REACHED ...`.** Native and
//! the fixed backend both print `start`, then abort (native via libc `abort` → exit 134; backend via
//! `System.Environment.FailFast`, also exit 134 when run with `dotnet <dll>`). The abort *message*
//! (managed FailFast vs Rust's "panic in a function that cannot unwind") and the single-file apphost's
//! exit-code propagation are the known orthogonal abort-fidelity residuals; the load-bearing property
//! — the abort is uncatchable, so `REACHED` never prints — is what this guards.
//!
//! NOTE: the dual case — a panic in a destructor run *during* unwinding (`Terminate(InCleanup)`, a
//! double panic), or a `Drop` unwinding inside a `nounwind` fn — sits on a MIR *cleanup* block, which
//! the backend's single-layer try/catch model does not yet wrap. That tail is a documented residual
//! (needs nested exception regions); see BROKEN_TESTS.md.

extern "C" fn cannot_unwind() {
    panic!("panic crossing an extern \"C\" boundary");
}

fn main() {
    println!("start");
    let r = std::panic::catch_unwind(|| cannot_unwind());
    // Unreachable: the abort must fire before this line.
    println!("REACHED caught_err={}", r.is_err());
}
