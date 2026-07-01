// The Task ↔ Future bridge in action: `.await` a real .NET `Task` from Rust, and turn a Rust
// `async fn` back into a .NET `Task`. async coroutine lowering itself already runs on the dotnet PAL
// (see cargo_tests/pal_async); this crate crosses the interop seam to genuine managed Tasks.
//
// Two layout facts shape the API (and this test):
//   * A .NET object reference may NOT live in a coroutine's saved state (an `async fn` state machine
//     is laid out with *overlapping* variant storage, like an enum, and .NET forbids a GC reference in
//     an overlapping field). So a managed `Task` handle must never be held *across* an `.await` inside
//     an `async fn`; it is awaited via a plain `Future` struct driven by `block_on` instead.
//   * `.await` here is *polling* (checks `Task.IsCompleted`, re-arms the waker while pending). On the
//     PAL's spin `block_on` this observes completion as soon as the .NET Task finishes.
//
// Every result is checked in-Rust; `main` prints `pass` then `total` (a `9000000xx` marker flags any
// failing check) and returns non-zero on any mismatch — the cd_* harness convention.
#![allow(dead_code)]

use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicI32, Ordering};
use core::task::{Context, Poll};

use mycorrhiza::prelude::*;
use mycorrhiza::system::console::Console;

// A capture-less `extern "C" fn` scheduled onto the .NET thread pool by `Task.Run`. It bumps a
// `static` atomic so the main thread (spinning in `block_on` awaiting the Task) can observe that the
// callback actually ran on another thread — a genuine asynchronous, cross-thread round trip.
static RAN_ON_POOL: AtomicI32 = AtomicI32::new(0);
extern "C" fn pool_callback() {
    RAN_ON_POOL.store(7, Ordering::SeqCst);
}

// ------- a Rust `async fn` we hand back to .NET as a Task (Future -> Task) -------
//
// A PURE-compute coroutine: it `.await`s nested async blocks but holds NO managed reference across a
// suspend point (only `i32`s), so its state machine has no GC-ref field and lays out fine. Its result
// is written to a `static` (a bare `Task` carries no value); the produced managed Task just signals
// completion. This is the shape a Rust `async fn` takes to become a .NET-awaitable Task.
static COMPUTED: AtomicI32 = AtomicI32::new(0);
async fn rust_async_compute() {
    let mut acc = 1;
    for i in 0..3i32 {
        acc += async move { i * 10 }.await; // await in a loop, nested async block
    }
    COMPUTED.store(acc, Ordering::SeqCst); // 1 + (0+10+20) = 31
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

    println!("== cd_async start ==");

    // ---------- 1. await an ALREADY-COMPLETED .NET Task ----------
    // `Task.CompletedTask` is a genuine managed Task that is already done; awaiting it resolves at once.
    {
        let done: bool = block_on(async {
            await_unit(Task::completed()).await;
            true
        });
        chk!(done, true);
    }

    // ---------- 2. await a DELAYED .NET Task (timer-backed, completes AFTER the await starts) ----------
    // `Task.Delay(ms)` completes on a CLR timer some milliseconds later, so the await genuinely goes
    // Pending -> Ready: `block_on` spins polling `IsCompleted` until the runtime completes the Task.
    {
        let done: bool = block_on(async {
            await_unit(Task::delay(20)).await;
            true
        });
        chk!(done, true);
    }

    // ---------- 3. await a Task.Run(Action) whose body runs a Rust callback on the .NET pool ----------
    // The Task is created by scheduling a capture-less Rust fn onto the thread pool; awaiting it waits
    // for that callback to finish. We then observe the side effect it wrote from the pool thread.
    {
        RAN_ON_POOL.store(0, Ordering::SeqCst);
        let ran: i32 = block_on(async {
            await_unit(Task::run(pool_callback)).await;
            RAN_ON_POOL.load(Ordering::SeqCst)
        });
        chk!(ran, 7);
    }

    // ---------- 4. a Task that is NOT complete at first poll, completing mid-await ----------
    // A plain `Future` struct (NOT an async fn) that returns Pending twice, then resolves once the
    // underlying `Task.Delay` reports completion — exercising the real Pending->Ready path directly.
    {
        struct SpinDelay {
            fut: TaskUnitFuture,
            polls: u32,
        }
        impl Future for SpinDelay {
            type Output = u32;
            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
                self.polls += 1;
                match Pin::new(&mut self.fut).poll(cx) {
                    Poll::Ready(()) => Poll::Ready(self.polls),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
        let polls = block_on(SpinDelay {
            fut: await_unit(Task::delay(15)),
            polls: 0,
        });
        // A 15ms timer cannot complete on the very first poll, so the Task was observed Pending at
        // least once (the real suspend/resume path), i.e. more than one poll happened.
        chk!((polls > 1), true);
    }

    // ---------- 5. Future -> Task: hand a Rust async fn back to .NET as a Task, then await it ----------
    // `future_to_task_unit` drives `rust_async_compute` to completion and packages it into a managed
    // `Task` a .NET caller could `await`. We verify by awaiting it back from Rust and reading the side
    // effect the async fn produced.
    {
        COMPUTED.store(0, Ordering::SeqCst);
        let task: Task = future_to_task_unit(rust_async_compute());
        let done: bool = block_on(async {
            await_unit(task).await;
            true
        });
        chk!(done, true);
        chk!(COMPUTED.load(Ordering::SeqCst), 31);
    }

    // ---------- 6. block_on a pure Rust future on the PAL (executor sanity, no managed Task) ----------
    {
        let v = block_on(async {
            let mut s = 0;
            for i in 0..5i32 {
                s += async move { i }.await;
            }
            s // 0+1+2+3+4
        });
        chk!(v, 10);
    }

    // ---------- 7. RESULT-BEARING Task<T> production (the former wall) ----------
    // `future_to_task` packages an `async fn -> T` into a managed `Task<T>` (via
    // `TaskCompletionSource<T>.get_Task()`, a nested-generic def-shape return). We prove the full
    // round-trip: produce a `Task<i32>` from Rust, then `.await` it back and read the value.
    {
        let t: TaskT<i32> = future_to_task(async { 40 + 2 });
        let got: i32 = block_on(async { await_task(t).await });
        chk!(got, 42);
        // A wider type, to exercise a non-i32 result across the generic Task<T>.
        let t2: TaskT<i64> = future_to_task(async { (1i64 << 40) + 5 });
        let got2: i64 = block_on(async { await_task(t2).await });
        chk!(got2, (1i64 << 40) + 5);
    }

    println!("== cd_async done ==");
    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
