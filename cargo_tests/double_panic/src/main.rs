//! Differential regression for `UnwindAction::Terminate(InCleanup)` — a DOUBLE PANIC (the seam-audit
//! Slice A residual, closed by the nested-EH `CILRoot::TerminateRegion` inline abort guard).
//!
//! A destructor that panics *while a first panic is already unwinding* must abort the process
//! UNCATCHABLY — `catch_unwind` is required NOT to absorb it. The `Terminate(InCleanup)` edge sits on
//! the `Drop` call inside a MIR *cleanup* block; P2-S4's synthetic FailFast handler only covered
//! NORMAL-block Terminate edges (cleanup blocks are never run through `resolve_exception_handlers`, and
//! the il_exporter renders only one try/catch layer per block). The fix wraps the cleanup-block drop
//! call in a self-contained `TerminateRegion` that exports as an inner `.try{ <drop> } catch { FailFast }`
//! — uncatchable — without making any BasicBlock carry a nested handler.
//!
//! ASSERTION (apphost-robust): **stdout is exactly `start\n` — never `REACHED`.** Native and the fixed
//! backend both print `start`, then abort (native via libc `abort`; backend via `Environment.FailFast`
//! with the "panic in a destructor during cleanup" message — `dotnet <dll>` exits 134). The abort
//! *message* and the single-file apphost's exit-code are the known abort-fidelity residuals; the
//! load-bearing property — the double panic is uncatchable, so `REACHED` never prints — is what this
//! guards. (A *normal* panic must still be catchable — covered by other tests / the ::stable suite.)

struct Bomb;
impl Drop for Bomb {
    fn drop(&mut self) {
        panic!("panic in destructor");
    }
}

fn main() {
    println!("start");
    let r = std::panic::catch_unwind(|| {
        let _b = Bomb; // dropped while `first panic` unwinds -> second panic -> Terminate(InCleanup)
        panic!("first panic");
    });
    // Unreachable: the double panic must abort before this line.
    println!("REACHED caught_err={}", r.is_err());
}
