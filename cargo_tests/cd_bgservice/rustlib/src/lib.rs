//! Investigation: can a Rust `#[dotnet_class]` implement `Microsoft.Extensions.Hosting.IHostedService`
//! and be registered/driven by a REAL generic-host lifecycle (`IHostBuilder`/`Host.CreateApplicationBuilder`
//! + `AddHostedService<T>()`)?
//!
//! Two class shapes are proven here:
//!
//! * [`SyncWorker`] — implements `IHostedService` directly (`implements =
//!   "[Microsoft.Extensions.Hosting.Abstractions]Microsoft.Extensions.Hosting.IHostedService"`).
//!   `StartAsync`/`StopAsync` do synchronous work (increment a field, `Console.WriteLine` via a
//!   BCL call) and return an already-completed `System.Threading.Tasks.Task`
//!   (`mycorrhiza::task::Task::completed()`), sidestepping the async ceiling entirely — no `.await`
//!   inside Rust, no coroutine state machine, just synchronous-then-`Task.CompletedTask`.
//!
//! * [`LoopWorker`] — same interface, but `StartAsync` spins a real background OS thread
//!   (`std::thread::spawn`) running a blocking `loop { ... thread::sleep(..) }` poll, and returns
//!   `Task.CompletedTask` immediately (so `StartAsync` itself doesn't block the host). `StopAsync`
//!   flips an atomic flag the loop polls, so the host's shutdown sequence can observe it stopping.
//!   This is the "genuinely long-running, but blocking-not-async" workaround pattern.
//!
//! What is explicitly NOT attempted: subclassing the abstract `Microsoft.Extensions.Hosting.BackgroundService`
//! and overriding its `protected abstract Task ExecuteAsync(CancellationToken)`. Per
//! `mycorrhiza::comptime::rustc_codegen_clr_mark_last_method_override`'s own doc, `.override` of a
//! base-class virtual is proven for exactly one case (`System.Object.ToString()`); general
//! base-class wrapping of a framework type with a non-trivial ctor / protected members is
//! documented as a separate, unaddressed problem. `BackgroundService`'s only public surface is its
//! `protected` ctor and `protected abstract ExecuteAsync` — there is no way to satisfy either
//! through today's `#[dotnet_class(extends = ...)]` + `#[dotnet_override(...)]` machinery without
//! new backend work, so this file does not attempt it.

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use core::time::Duration;

use dotnet_macros::{dotnet_class, dotnet_methods};
use mycorrhiza::bindings::System::Threading::Tasks::Task as RawTaskHandle;
use mycorrhiza::intrinsics::RustcCLRInteropManagedStruct;
use mycorrhiza::task::Task;

const CORELIB: &str = "System.Private.CoreLib";
const CANCELLATION_TOKEN: &str = "System.Threading.CancellationToken";
/// `System.Threading.CancellationToken` — a one-field managed value type. `SIZE` is a Rust-side
/// placeholder the backend never reads (same convention `mycorrhiza::sync` uses internally); the
/// CLR alone knows and uses the real layout. Re-declared here (rather than reusing
/// `mycorrhiza::sync`'s private alias) because that module doesn't export it.
const CANCELLATION_TOKEN_SIZE: usize = core::mem::size_of::<usize>();
type RawCancellationToken =
    RustcCLRInteropManagedStruct<CORELIB, CANCELLATION_TOKEN, CANCELLATION_TOKEN_SIZE>;

// =====================================================================================================
// 1) SyncWorker — IHostedService implemented directly, synchronous body, `Task.CompletedTask` return.
// =====================================================================================================

/// Counters observed from C# after the host runs this service through its lifecycle, proving the
/// Rust-implemented `StartAsync`/`StopAsync` actually executed (not just type-checked).
static START_COUNT: AtomicI32 = AtomicI32::new(0);
static STOP_COUNT: AtomicI32 = AtomicI32::new(0);

/// A Rust-defined managed reference type implementing `Microsoft.Extensions.Hosting.IHostedService`
/// straight from the interface — no `BackgroundService` base class involved. `default_ctor = true`
/// so `AddHostedService<SyncWorker>()`'s DI container can `new SyncWorker()` it (the primary ctor
/// alone would need constructor args DI has no source for).
#[dotnet_class(
    implements = "[Microsoft.Extensions.Hosting.Abstractions]Microsoft.Extensions.Hosting.IHostedService",
    default_ctor = true
)]
pub struct SyncWorker {
    tag: i32,
}

#[dotnet_methods]
impl SyncWorker {
    /// `Task StartAsync(CancellationToken cancellationToken)` — does synchronous work (bump a
    /// static counter, so the C# side can observe it ran), then returns `Task.CompletedTask`. No
    /// `.await`, no coroutine — this is the "async ceiling sidestep" the investigation asked for.
    pub fn StartAsync(_this: SyncWorkerHandle, _ct: RawCancellationToken) -> RawTaskHandle {
        START_COUNT.fetch_add(1, Ordering::SeqCst);
        Task::completed().raw()
    }

    /// `Task StopAsync(CancellationToken cancellationToken)` — same shape as `StartAsync`.
    pub fn StopAsync(_this: SyncWorkerHandle, _ct: RawCancellationToken) -> RawTaskHandle {
        STOP_COUNT.fetch_add(1, Ordering::SeqCst);
        Task::completed().raw()
    }

    /// Not part of `IHostedService` — a plain static accessor so C# can read the counters above
    /// without needing reflection or a second interface.
    pub fn StartCount() -> i32 {
        START_COUNT.load(Ordering::SeqCst)
    }

    pub fn StopCount() -> i32 {
        STOP_COUNT.load(Ordering::SeqCst)
    }
}

// =====================================================================================================
// 2) LoopWorker — same interface, but `StartAsync` spins a real background OS thread running a
//    blocking loop (`thread::sleep`, not `Task.Delay`/`await`), and `StopAsync` signals it to stop.
// =====================================================================================================

static LOOP_RUNNING: AtomicBool = AtomicBool::new(false);
static LOOP_STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
static LOOP_TICKS: AtomicI32 = AtomicI32::new(0);

#[dotnet_class(
    implements = "[Microsoft.Extensions.Hosting.Abstractions]Microsoft.Extensions.Hosting.IHostedService",
    default_ctor = true
)]
pub struct LoopWorker {
    tag: i32,
}

#[dotnet_methods]
impl LoopWorker {
    /// Spins a real OS thread running a blocking poll loop (`thread::sleep`, not `await Task.Delay`),
    /// then returns immediately with `Task.CompletedTask` so the host's own `StartAsync` await point
    /// doesn't block on the loop itself. This is "genuinely long-running" without touching the async
    /// ceiling at all — the loop body is fully synchronous Rust.
    pub fn StartAsync(_this: LoopWorkerHandle, _ct: RawCancellationToken) -> RawTaskHandle {
        LOOP_STOP_REQUESTED.store(false, Ordering::SeqCst);
        LOOP_RUNNING.store(true, Ordering::SeqCst);
        std::thread::spawn(|| {
            while !LOOP_STOP_REQUESTED.load(Ordering::SeqCst) {
                LOOP_TICKS.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
            }
            LOOP_RUNNING.store(false, Ordering::SeqCst);
        });
        Task::completed().raw()
    }

    /// Flips the stop flag the background thread polls, then returns `Task.CompletedTask`. Does
    /// NOT block waiting for the thread to actually exit (a fuller implementation would join with a
    /// timeout) — proving the signal reaches the loop is enough for this investigation.
    pub fn StopAsync(_this: LoopWorkerHandle, _ct: RawCancellationToken) -> RawTaskHandle {
        LOOP_STOP_REQUESTED.store(true, Ordering::SeqCst);
        Task::completed().raw()
    }

    pub fn IsRunning() -> bool {
        LOOP_RUNNING.load(Ordering::SeqCst)
    }

    pub fn Ticks() -> i32 {
        LOOP_TICKS.load(Ordering::SeqCst)
    }
}
