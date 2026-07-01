// The END-USER experience of .NET delegates & callbacks from Rust: `mycorrhiza::delegate` — wrap a
// Rust `extern "C" fn` as a managed `Action`/`Func`/`Comparison` delegate and invoke it (each `invoke`
// is `callvirt Delegate::Invoke`, i.e. the .NET runtime dispatching into the Rust callback through a
// real first-class delegate object), and re-hold a delegate handle. No `ldftn`, no `newobj`, no shim
// classes at the call site — that machinery is behind `rustc_clr_interop_delegate`.
//
// Every result is checked in-Rust; `main` prints `pass` then `total` (a `9000000xx` marker flags any
// failing check) and returns non-zero on any mismatch.
#![allow(dead_code)]

use core::sync::atomic::{AtomicI32, Ordering};
use mycorrhiza::delegate::{Action1, Action2, Comparison, Func1, Func2};
use mycorrhiza::system::console::Console;

// --- Callbacks the delegates wrap. Plain top-level `extern "C" fn`s; state crosses via a static. ---

static LAST_SEEN: AtomicI32 = AtomicI32::new(0);
static SUM_SEEN: AtomicI32 = AtomicI32::new(0);

extern "C" fn remember(x: i32) {
    LAST_SEEN.store(x, Ordering::SeqCst);
}
extern "C" fn add_both(a: i32, b: i32) {
    SUM_SEEN.store(a + b, Ordering::SeqCst);
}
extern "C" fn double_it(x: i32) -> i32 {
    x * 2
}
extern "C" fn add(a: i32, b: i32) -> i32 {
    a + b
}
// Descending comparison: positive when a<b, negative when a>b (reverse of the default order).
extern "C" fn desc(a: i32, b: i32) -> i32 {
    b - a
}

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    // ---------- Action1<i32>: build from a Rust fn, invoke it, observe the side effect ----------
    let a1 = Action1::<i32>::from_fn(remember);
    a1.invoke(42);
    chk!(LAST_SEEN.load(Ordering::SeqCst), 42);
    a1.invoke(-7);
    chk!(LAST_SEEN.load(Ordering::SeqCst), -7);

    // ---------- Action2<i32, i32> ----------
    let a2 = Action2::<i32, i32>::from_fn(add_both);
    a2.invoke(10, 20);
    chk!(SUM_SEEN.load(Ordering::SeqCst), 30);

    // ---------- Func1<i32, i32>: return value flows back from the callback ----------
    let f1 = Func1::<i32, i32>::from_fn(double_it);
    chk!(f1.invoke(21), 42);
    chk!(f1.invoke(0), 0);
    chk!(f1.invoke(-5), -10);

    // ---------- Func2<i32, i32, i32> ----------
    let f2 = Func2::<i32, i32, i32>::from_fn(add);
    chk!(f2.invoke(2, 3), 5);
    chk!(f2.invoke(100, -1), 99);

    // ---------- Comparison<i32>: invoke directly ----------
    let cmp = Comparison::<i32>::from_fn(desc);
    // desc(1, 2) = 2 - 1 = 1  (a<b ⇒ positive ⇒ "a after b" ⇒ descending order)
    chk!(cmp.invoke(1, 2), 1);
    chk!(cmp.invoke(5, 5), 0);
    chk!(cmp.invoke(9, 4), -5);

    // ---------- "Hold and invoke a .NET delegate": rewrap a handle, invoke it ----------
    // The handle round-trips through a raw `RustcCLRInteropManagedGeneric`, mimicking a delegate
    // returned from a .NET call — `from_handle` is the "hold a .NET delegate" entry point. Every
    // `invoke` above emits `callvirt Func`1<int32,int32>::Invoke(!0)` (or the `Action`/`Comparison`
    // equivalent): that is the .NET runtime dispatching through a real managed delegate object into
    // the Rust callback — i.e. .NET code invoking a Rust function via a first-class .NET delegate.
    let held = Func1::<i32, i32>::from_handle(f1.handle());
    chk!(held.invoke(7), 14);
    chk!(held.invoke(100), 200);
    // Re-invoke a rewrapped Action too (the same managed delegate object, freshly held).
    let held_a = Action1::<i32>::from_handle(a1.handle());
    held_a.invoke(1234);
    chk!(LAST_SEEN.load(Ordering::SeqCst), 1234);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
