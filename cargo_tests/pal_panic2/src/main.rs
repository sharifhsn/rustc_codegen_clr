//! H2 Phase 3 validation: panic=unwind + `catch_unwind` on the REAL dotnet PAL,
//! ISOLATED from the default panic hook.
//!
//! This is the companion to `pal_panic`. Both exercise the same WF-6 throw-bridge
//! (`_Unwind_RaiseException` -> managed `RustException` throw -> `catch_unwind`'s
//! managed try/catch). The difference: `pal_panic` lets the panic run through the
//! DEFAULT hook, whose `std::panic::get_backtrace_style` does a
//! `static Atomic<u8>::compare_exchange` — and that sub-word atomic CAS is lowered
//! via the word-CAS emulation (`emulate_subword_cmp_xchng` in
//! cilly/src/ir/builtins/atomics.rs), which aligns the 1-byte static's address
//! DOWN to its containing 32-bit word and faults (AccessViolation) because the
//! managed static field is neither 4-byte aligned nor 4 bytes wide. That is a
//! PRE-EXISTING sub-word-atomic bug (WF-5), orthogonal to unwinding.
//!
//! `pal_panic2` installs a custom (no-op) panic hook with `set_hook`, which
//! REPLACES `default_hook` entirely, so `get_backtrace_style` is never called and
//! the buggy CAS is never reached. The panic still travels the full unwind path,
//! so a clean run here proves the panic=unwind + catch_unwind machinery itself is
//! correct on the dotnet target, independent of the atomic bug.
//!
//! SUCCESS = prints "b  caught panic: is_err=true", "c  ok path value: Some(42)",
//! "== pal_panic2 done ==".
fn main() {
    println!("== pal_panic2 start ==");
    // Replace the default hook so the panic does NOT touch get_backtrace_style's
    // sub-word atomic CAS (the orthogonal WF-5 bug); see module docs.
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(|| {
        println!("a  inside closure (about to panic)");
        panic!("boom from rust");
    });
    println!("b  caught panic: is_err={}", r.is_err());
    let r2 = std::panic::catch_unwind(|| 6 * 7);
    println!("c  ok path value: {:?}", r2.ok());
    println!("== pal_panic2 done ==");
}
