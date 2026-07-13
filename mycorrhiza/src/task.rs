//! The **Task ↔ Future bridge** — `.await` a .NET `Task` / `Task<T>` from Rust, and expose a Rust
//! `async fn` as a .NET `Task`.
//!
//! .NET's asynchronous surface (`HttpClient`, EF, ASP.NET, `File.ReadAllTextAsync`, `Task.Delay`, …) is
//! expressed as `System.Threading.Tasks.Task` / `Task<TResult>`: a first-class managed object
//! representing an in-flight or completed operation. Rust's asynchronous surface is
//! [`Future`](core::future::Future). This module is the *interop adapter* between the two — the async
//! coroutine lowering itself already runs on the dotnet PAL (see `cargo_tests/pal_async`); what was
//! missing was the seam that lets a `.NET Task` participate in a Rust `.await` and vice-versa.
//!
//! ```ignore
//! use mycorrhiza::task::{await_unit, block_on, Task};
//!
//! // block on (await) a real .NET Task — here a timer-backed delay:
//! block_on(async { await_unit(Task::delay(5)).await });   // completes after ~5ms
//! ```
//!
//! ## `.await`-ing a .NET Task (Task → Future)
//!
//! [`await_unit`](crate::task::await_unit) wraps a non-generic [`Task`](crate::task::Task) (result
//! `()`), [`await_task`](crate::task::await_task) a result-bearing [`TaskT`](crate::task::TaskT),
//! each in a Rust [`Future`] ([`TaskUnitFuture`](crate::task::TaskUnitFuture) /
//! [`TaskFuture`](crate::task::TaskFuture)) whose `poll`
//! inspects the Task's `IsCompleted`:
//!
//! * completed → resolve (`()`, resp. read `Result` = `!0` = `T`);
//! * still running → re-arm the waker (`wake_by_ref`) and return [`Poll::Pending`](core::task::Poll::Pending).
//!
//! This is a **polling** adapter: it works with any executor whose `wake` re-polls (a spin
//! [`block_on`](crate::task::block_on), tokio's `current_thread`, …). It does *not* register a .NET continuation on the Task —
//! that would need a managed callback re-entering an arbitrary Rust `Waker`, i.e. a *capturing*
//! delegate, which the delegate bridge does not yet support (only capture-less `extern "C" fn`). For a
//! `block_on`-style executor (what async programs on this PAL use) polling is exactly right.
//!
//! **Do not `.await` a RAW managed Task handle *inside* an `async fn`** — i.e. never hold a bare
//! `TaskT`/`Task`/`RustcCLRInteropManagedClass<..>` in a local across a suspend point. A managed
//! object reference may not live in a coroutine's saved state (an `async fn` state machine is laid
//! out with *overlapping* variant storage, like an enum, and .NET forbids a GC reference in an
//! overlapping field — `cilly::ir::class`'s `layout_check`, `ManagedRefInOverlapingField`). So a raw
//! handle must be awaited via a *plain* [`Future`] struct (as [`TaskFuture`](crate::task::TaskFuture)
//! itself does) driven by [`block_on`](crate::task::block_on), never captured directly in a
//! suspending `async fn` body. See [`TaskFuture`](crate::task::TaskFuture).
//!
//! **This IS solvable, though, for the common case of holding a managed reference across an
//! `.await`** — not by weakening `layout_check` (never done), but by never putting a gcref in the
//! coroutine state in the first place. [`crate::class::Class`] wraps the target object in a
//! `System.Runtime.InteropServices.GCHandle` (`GCHandle.Alloc`/`.Free`), and `GCHandle` is itself a
//! .NET *value type* over a plain `IntPtr` — not a gcref. A `Class<ASSEMBLY, CLASS_PATH>` value
//! therefore has no gcref field at all, so `layout_check` never rejects it, and it is legal to
//! declare one *before* an `.await` and use it again *after* — including across more than one
//! suspend point in the same `async fn` body. `cargo_tests/cd_persisted_async` proves this end to
//! end: a `Class<..>`-wrapped `System.Text.StringBuilder` is built, appended to, held across TWO
//! `.await`s, appended to again after each, and read back correctly. (Fixed as part of this proof:
//! `Class::get_naked_ref` was calling `GCHandle.get_Target()` with the concrete handle type as the
//! declared return, but `get_Target` is declared `object Target { get; }` — its real signature
//! return is always `System.Object`; the fix reads it as `System.Object` then `castclass`es down,
//! matching `from_naked_ref`'s upcast in reverse.) This newtype needed **no backend change** — it is
//! a pure `mycorrhiza`-level pattern, reusable for any class-typed value a coroutine needs to keep
//! live across a suspend point.
//!
//! ## Exposing a Rust `async fn` as a .NET `Task` (Future → Task)
//!
//! [`future_to_task_unit`](crate::task::future_to_task_unit) drives a Rust `async fn`
//! (`Future<Output = ()>`) to completion with a self-contained spin executor and packages it into a
//! **completed** non-generic managed [`Task`](crate::task::Task) a
//! .NET caller can `await` (it returns synchronously, the work being done). Combined with
//! [`#[dotnet_export]`](../../dotnet_macros) this is how a Rust `async fn` becomes a C#-awaitable
//! method.
//!
//! ## Result-bearing `Task<T>` — both directions supported
//!
//! Awaiting a `Task<T>` ([`await_task`](crate::task::await_task)) and **producing** one
//! ([`future_to_task`](crate::task::future_to_task), via
//! `TaskCompletionSource<T>.get_Task()`) both work. `get_Task()`'s def-shape nested-generic return
//! `Task<!0>` binds against the concrete `Task<T>` local — the CIL verifier accepts it (same open
//! generic, `!0` pairwise-assignable) and codegen proves `!0` == `T` via the recursive WF-9 marker
//! guard. So `async fn -> T` ⇒ `Task<T>` is symmetric with the non-generic `Task` direction.
//! (`Task.FromResult<T>` remains unused — it needs generic-*method* `!!N` argument support the backend
//! doesn't emit; the `TaskCompletionSource<T>` route is fully sufficient.)

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crate::intrinsics::{
    RustcCLRInteropManagedClass, RustcCLRInteropManagedGeneric,
    RustcCLRInteropManagedGenericStruct, RustcCLRInteropTypeGeneric,
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2,
    rustc_clr_interop_generic_ctor0,
};

/// `Task` / `Task<T>` / `TaskCompletionSource<T>` all live in the core implementation assembly
/// (`System.Private.CoreLib`); a reference assembly forwards them and throws at JIT, so method-body
/// refs must name the impl assembly — the same rule the rest of the interop surface follows.
const CORELIB: &str = "System.Private.CoreLib";

const TASK_GEN: &str = "System.Threading.Tasks.Task";

/// A handle to a managed `System.Threading.Tasks.Task<T>` — the generic (result-bearing) Task. Named
/// `TaskT` (not `Task`) so it doesn't collide with the generated non-generic Task binding. `T` must
/// be a boundary-crossing .NET type (a primitive, a `#[repr(C)]` value type, or a managed handle),
/// exactly as for [`crate::collections`].
pub type TaskT<T> = RustcCLRInteropManagedGeneric<{ CORELIB }, { TASK_GEN }, (T,)>;

const VALUE_TASK_GEN: &str = "System.Threading.Tasks.ValueTask";

/// The raw managed value-type handle for `System.Threading.Tasks.ValueTask<T>`.
///
/// Generated NuGet bindings use this type for closed `ValueTask<T>` returns. Convert it directly
/// into an idiomatic Rust [`Future`] with [`await_value_task`], or into a managed [`TaskT`] with
/// [`value_task_into_task`]. The size is only a Rust-side transport buffer; the CLR owns the real
/// generic value-type layout.
pub type ValueTaskT<T> = RustcCLRInteropManagedGenericStruct<
    { CORELIB },
    { VALUE_TASK_GEN },
    { core::mem::size_of::<usize>() * 2 },
    (T,),
>;

/// Convert a returned managed `ValueTask<T>` into `Task<T>` via `ValueTask<T>.AsTask()`.
///
/// This is useful when a caller needs to retain or pass the managed task. For ordinary Rust async
/// code, [`await_value_task`] is the shorter entry point.
#[inline]
pub fn value_task_into_task<T>(value_task: ValueTaskT<T>) -> TaskT<T> {
    rustc_clr_interop_generic_call1::<
        { CORELIB },
        { VALUE_TASK_GEN },
        true,
        "AsTask",
        1,
        (T,),
        (
            RustcCLRInteropManagedGeneric<
                { CORELIB },
                { TASK_GEN },
                (RustcCLRInteropTypeGeneric<0>,),
            >,
        ),
        TaskT<T>,
        &ValueTaskT<T>,
    >(&value_task)
}

/// The raw non-generic managed `System.Threading.Tasks.Task` handle (same underlying type as
/// [`crate::bindings::Task`]). Wrapped by [`Task`] so this module can carry its own inherent methods
/// without colliding with the generated bindings' inherent impl on the identical alias.
type RawTask = RustcCLRInteropManagedClass<{ CORELIB }, { TASK_GEN }>;
/// A managed `System.Action` (parameterless) delegate handle — the argument type of `Task.Run(Action)`.
type ActionHandle = RustcCLRInteropManagedGeneric<{ CORELIB }, "System.Action", ()>;

// ---- Task<T> members reached through the WF-9 generic bridge -----------------------------------
//
// `IsCompleted` / `IsFaulted` / `IsCanceled` are inherited from the non-generic `Task` base, but a
// methodref on the generic instantiation resolves them fine (the CLR JIT walks the base chain); their
// return is a concrete `bool`, so no `!N` marker is involved. `get_Result` is declared on `Task<T>`
// itself and returns `!0` (= `T`) — the one member that uses the class generic. All are `callvirt`
// (`KIND = 2`) on a reference type, matching how C# accesses them.

/// `Task<T>.IsCompleted` — `true` once the task has finished (successfully, faulted, or canceled).
#[inline]
fn task_is_completed<T>(t: TaskT<T>) -> bool {
    rustc_clr_interop_generic_call1::<
        { CORELIB },
        { TASK_GEN },
        false,
        "get_IsCompleted",
        2u8,
        (T,),
        (bool,),
        bool,
        TaskT<T>,
    >(t)
}

/// `Task<T>.IsFaulted` — `true` if the task ended by throwing.
#[inline]
fn task_is_faulted<T>(t: TaskT<T>) -> bool {
    rustc_clr_interop_generic_call1::<
        { CORELIB },
        { TASK_GEN },
        false,
        "get_IsFaulted",
        2u8,
        (T,),
        (bool,),
        bool,
        TaskT<T>,
    >(t)
}

/// `Task<T>.IsCanceled` — `true` if the task ended by cancellation.
#[inline]
fn task_is_canceled<T>(t: TaskT<T>) -> bool {
    rustc_clr_interop_generic_call1::<
        { CORELIB },
        { TASK_GEN },
        false,
        "get_IsCanceled",
        2u8,
        (T,),
        (bool,),
        bool,
        TaskT<T>,
    >(t)
}

/// `Task<T>.Result` — the produced value (`!0` = `T`). Blocks the calling thread until the task
/// completes when read on an incomplete task; the bridge only reads it *after* `IsCompleted`, so it
/// returns immediately. The return marker `!0` is accepted against the concrete `T` by the WF-9 rule.
#[inline]
fn task_result<T>(t: TaskT<T>) -> T {
    rustc_clr_interop_generic_call1::<
        { CORELIB },
        { TASK_GEN },
        false,
        "get_Result",
        2u8,
        (T,),
        (RustcCLRInteropTypeGeneric<0>,),
        T,
        TaskT<T>,
    >(t)
}

// ---- Why `future_to_task_unit` is non-generic (not a producer wall — see `tcs_get_task` below) --
//
// `future_to_task_unit` goes through the non-generic `TaskCompletionSource` (`new()`/`SetResult()`/
// `get_Task()`, no def-shape return involved) rather than `TaskCompletionSource<T>.get_Task()`.
// Producing a result-bearing `Task<T>` — once thought to need the def-shape `Task`1<!0>` return to
// bind against a concrete `Task<T>` local, which the WF-9 nested-generic-binding fix later made
// possible — is implemented further down; see `tcs_get_task`/`future_to_task`.

// ---- Task → Future: `.await` a .NET Task<T> ----------------------------------------------------

/// A Rust [`Future`] over a managed `Task<T>` — the Task→Future half of the bridge.
///
/// Each `poll` checks the Task's `IsCompleted`. When it becomes `true` the future reads `Result` and
/// resolves; until then it re-arms the waker and returns [`Poll::Pending`], yielding back to the
/// executor. On a spin `block_on` (what async programs on this PAL use) the executor immediately
/// re-polls, so the value is observed as soon as the .NET Task finishes; on a smarter executor the
/// `wake` simply reschedules the task. It holds only the managed `Task<T>` handle (a reference), so
/// `T` need not be `Copy`.
///
/// **Faulted / canceled Tasks.** If the Task ends by throwing or cancellation, reading `Result` would
/// raise the managed exception through the interop boundary. `poll` therefore only reports `Ready`
/// for a *successful* completion; a faulted/canceled Task resolves to a panic via `task_result`'s
/// managed throw — matching how `.await` on a faulted Task surfaces in C# (an exception at the await).
pub struct TaskFuture<T> {
    // Held via `GenericClass` (a `GCHandle`-backed value type — see the module docs above), NOT the
    // raw `TaskT<T>` handle directly: `TaskFuture` is a plain struct (not a coroutine), so a raw gcref
    // field here is fine on its own, but `TaskFuture` values themselves get boxed and suspended inside
    // OTHER coroutines' saved state (e.g. `RecvFuture` in `crate::sync`, or any `async fn` that awaits
    // through `await_task`) — see `docs/…` / the `layout_check` note above. Wrapping the handle here
    // once means every caller gets the safety for free instead of having to know to re-wrap it.
    task: crate::class::GenericClass<{ CORELIB }, { TASK_GEN }, (T,)>,
}

impl<T> Future for TaskFuture<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        // SAFETY: the naked ref is used transiently within this call only (passed straight to the
        // two call wrappers below), never stored — satisfies `get_naked_ref`'s contract.
        let task = unsafe { self.task.get_naked_ref() };
        if task_is_completed(task) {
            Poll::Ready(task_result(task))
        } else {
            // Not done yet — ask to be polled again. On the PAL's spin executor this is an immediate
            // re-poll; on any waker-driven executor it reschedules us.
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl<T> TaskFuture<T> {
    /// Wrap a managed `Task<T>` handle directly.
    #[inline]
    pub fn new(task: TaskT<T>) -> Self {
        Self {
            task: crate::class::GenericClass::from_naked_ref(task),
        }
    }

    /// `true` if the wrapped Task faulted (ended by throwing). Useful to check before `.await` if you
    /// want to avoid the managed exception surfacing.
    #[inline]
    pub fn is_faulted(&self) -> bool {
        // SAFETY: transient use only, see `poll` above.
        task_is_faulted(unsafe { self.task.get_naked_ref() })
    }

    /// `true` if the wrapped Task was canceled.
    #[inline]
    pub fn is_canceled(&self) -> bool {
        // SAFETY: transient use only, see `poll` above.
        task_is_canceled(unsafe { self.task.get_naked_ref() })
    }
}

/// `.await`-adapt a managed `Task<T>`: `await_task(t).await` yields the Task's result. This is the
/// idiomatic entry point for the Task→Future direction — hand it any `Task<T>` a .NET API returned.
#[inline]
pub fn await_task<T>(task: TaskT<T>) -> TaskFuture<T> {
    TaskFuture::new(task)
}

/// `.await`-adapt a managed `ValueTask<T>` returned by a generated binding.
///
/// ```ignore
/// let answer = mycorrhiza::task::await_value_task(client.get_answer_async()).await;
/// ```
#[inline]
pub fn await_value_task<T>(value_task: ValueTaskT<T>) -> TaskFuture<T> {
    await_task(value_task_into_task(value_task))
}

// ---- Task → Future for the NON-GENERIC `Task` (result `()`) ------------------------------------
//
// `Task` (no result) covers `Task.Delay`, `Task.Run(Action)`, `Stream.FlushAsync`, etc. Its
// `IsCompleted` is a plain `bool` property (no generic return), so this path is unconditionally
// supported. `get_IsCompleted`/`get_IsFaulted` are `virtual` (properties on `Task`), reached with
// `callvirt`.

/// An idiomatic handle to the **non-generic** `System.Threading.Tasks.Task` (no result — the async
/// equivalent of a Rust `Future<Output = ()>`). Covers the enormous non-result async surface:
/// `Task.Delay`, `Task.Run(Action)`, `Stream.FlushAsync/WriteAsync`, `Task.WhenAll`, and every `async`
/// method returning a bare `Task`. Awaiting it needs no generic-return handling, so it is fully
/// supported. A move-only wrapper around a managed reference; the .NET GC owns the object.
#[derive(Clone, Copy)]
pub struct Task {
    h: RawTask,
}

impl Task {
    /// Wrap a raw managed `Task` handle (e.g. one returned by a `bindings::Task`-typed API such as
    /// `stream.FlushAsync()`), so it can be `.await`ed via [`await_unit`].
    #[inline]
    pub fn from_raw(h: RawTask) -> Self {
        Self { h }
    }

    /// The raw managed handle, for passing the Task to a .NET API expecting `System.Threading.Tasks.Task`.
    #[inline]
    pub fn raw(self) -> RawTask {
        self.h
    }

    /// `Task.IsCompleted` — `true` once the task has finished (successfully, faulted, or canceled).
    #[inline]
    pub fn is_completed(self) -> bool {
        self.h.virt0::<"get_IsCompleted", bool>()
    }

    /// `Task.IsFaulted` — `true` if the task ended by throwing.
    #[inline]
    pub fn is_faulted(self) -> bool {
        self.h.virt0::<"get_IsFaulted", bool>()
    }

    /// `Task.Delay(ms)` — a Task that completes after `ms` milliseconds (a real timer-backed delay, so
    /// awaiting it genuinely goes `Pending → Ready`). The canonical asynchronous non-result Task.
    #[inline]
    pub fn delay(ms: i32) -> Task {
        Self::from_raw(RawTask::static1::<"Delay", i32, RawTask>(ms))
    }

    /// `Task.CompletedTask` — a Task that is already completed. Awaiting it resolves immediately.
    #[inline]
    pub fn completed() -> Task {
        Self::from_raw(RawTask::static0::<"get_CompletedTask", RawTask>())
    }

    /// `Task.Run(Action)` — schedule a capture-less Rust callback (`extern "C" fn()`) onto the .NET
    /// thread pool, returning the Task that represents its execution. The callback is wrapped into a
    /// managed `System.Action` via the delegate bridge (a per-signature shim `calli`s the pointer).
    /// Captures aren't supported (delegate-bridge limitation) — pass state via a `static`.
    #[inline]
    pub fn run(f: extern "C" fn()) -> Task {
        let action: ActionHandle = crate::intrinsics::rustc_clr_interop_delegate::<
            { CORELIB },
            "System.Action",
            false,
            (),    // Action has no generic args
            ((),), // shim `calli` sig: () -> void
            extern "C" fn(),
            ActionHandle,
        >(f);
        Self::from_raw(RawTask::static1::<"Run", ActionHandle, RawTask>(action))
    }
}

/// A Rust [`Future`] over the **non-generic** managed [`Task`] — resolves to `()` when the Task
/// completes. Polls `IsCompleted`, re-arming the waker while the Task is still running (the same
/// polling model as [`TaskFuture`]). A plain struct (not a coroutine), so holding the managed `Task`
/// handle here is fine.
pub struct TaskUnitFuture {
    // Held via `Class` (a `GCHandle`-backed value type), not a raw `Task`/`RawTask` — same rationale
    // as `TaskFuture<T>` above: `TaskUnitFuture` values get boxed and suspended inside other
    // coroutines' saved state (any `async fn` awaiting through `await_unit`), so the raw managed
    // reference must not live in this struct.
    task: crate::class::Class<{ CORELIB }, { TASK_GEN }>,
}

impl Future for TaskUnitFuture {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY: the naked ref is used transiently within this call only, via `Task::from_raw` +
        // `is_completed`, never stored — satisfies `get_naked_ref`'s contract.
        let task = Task::from_raw(unsafe { self.task.get_naked_ref() });
        if task.is_completed() {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl TaskUnitFuture {
    /// Wrap a non-generic managed [`Task`].
    #[inline]
    pub fn new(task: Task) -> Self {
        Self {
            task: crate::class::Class::from_naked_ref(task.raw()),
        }
    }
}

/// `.await`-adapt a non-generic managed [`Task`]: `await_unit(t).await` completes when the Task does
/// (result `()`). Hand it any bare `Task` a .NET API returned (`Task.Delay`, `FlushAsync`, …); wrap a
/// raw `bindings::Task` with [`Task::from_raw`] first.
#[inline]
pub fn await_unit(task: Task) -> TaskUnitFuture {
    TaskUnitFuture::new(task)
}

// ---- Future → Task: expose a Rust async fn as a .NET Task<T> -----------------------------------

/// Minimal no-op waker for the self-contained spin executor below. The future is driven to completion
/// synchronously, so `wake` need do nothing (the loop re-polls unconditionally on `Pending`).
fn spin_waker() -> Waker {
    fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(core::ptr::null(), &VTABLE)
    }
    fn noop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    // SAFETY: the vtable's fns are all valid for the null data pointer (none dereference it).
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
}

/// Drive a Rust [`Future`] to completion with a self-contained spin executor and return its result.
/// This is the executor half of [`future_to_task_unit`]; exposed on its own because "block on a Rust
/// future, on the .NET PAL, without pulling in tokio" is itself useful (it's `pal_async`'s `block_on`,
/// packaged). Any `.NET Task` `.await`ed *inside* `fut` (via [`await_unit`]/[`await_task`]) is polled
/// by this loop.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    let waker = spin_waker();
    let mut cx = Context::from_waker(&waker);
    let mut fut = Box::pin(fut);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => {} // re-poll; TaskFuture re-arms the waker, other futures make progress
        }
    }
}

/// Run a Rust [`Future`] to completion and package its output into a **completed** managed `Task<T>` —
/// the Future→Task half of the bridge. A .NET caller can `await` the returned Task (it completes
/// synchronously, since the work is already done). This is the seam a Rust `async fn` crosses to become
/// a C#-awaitable method (pair it with [`#[dotnet_export]`](../../dotnet_macros)).
///
/// Uses the **non-generic** `System.Threading.Tasks.TaskCompletionSource` (whose `Task` property
/// returns a non-generic `Task`), so it involves no generic-return handling and is fully supported.
/// The result-bearing counterpart (`async fn -> T` ⇒ `Task<T>`) is [`future_to_task`], below.
pub fn future_to_task_unit<F>(fut: F) -> Task
where
    F: Future<Output = ()>,
{
    block_on(fut);
    // Non-generic TaskCompletionSource: `new()`, `SetResult()` (no arg), `get_Task() -> Task` — all
    // non-generic, so no def-shape return is involved. Its `get_Task` returns a raw `bindings::Task`
    // (= `RawTask`), which we wrap into our idiomatic `Task`.
    let tcs = crate::System::Threading::Tasks::TaskCompletionSource::new();
    tcs.set_result();
    Task::from_raw(tcs.get_task())
}

// ---- Result-bearing `Task<T>` PRODUCTION (the former wall — now unblocked) ----------------------
//
// `TaskCompletionSource<T>.get_Task()` returns the def-shape nested generic `Task`1<!0>`. That is now
// bindable: the CIL type-verifier accepts a `Task<!0>` return against the concrete `Task<T>` local
// (same open generic, `!0` pairwise-assignable), and codegen proves `!0` == `T` via the recursive
// WF-9 marker guard. So `async fn -> T` ⇒ `Task<T>` works — the symmetric counterpart to
// `future_to_task_unit`.

const TCS_GEN: &str = "System.Threading.Tasks.TaskCompletionSource";
/// A managed `TaskCompletionSource<T>` handle — produces a `Task<T>` whose result we set from Rust.
type TaskCompletionSourceT<T> = RustcCLRInteropManagedGeneric<{ CORELIB }, { TCS_GEN }, (T,)>;

/// `new TaskCompletionSource<T>()`.
#[inline]
fn tcs_new<T>() -> TaskCompletionSourceT<T> {
    rustc_clr_interop_generic_ctor0::<
        { CORELIB },
        { TCS_GEN },
        false,
        (T,),
        ((),),
        TaskCompletionSourceT<T>,
    >()
}
/// `TaskCompletionSource<T>.SetResult(!0)` — completes the task with `value`.
#[inline]
fn tcs_set_result<T>(tcs: TaskCompletionSourceT<T>, value: T) {
    rustc_clr_interop_generic_call2::<
        { CORELIB },
        { TCS_GEN },
        false,
        "SetResult",
        2u8,
        (T,),
        ((), RustcCLRInteropTypeGeneric<0>),
        (),
        TaskCompletionSourceT<T>,
        T,
    >(tcs, value)
}
/// `TaskCompletionSource<T>.get_Task()` — the produced `Task<T>`. The def-shape return `Task<!0>`
/// binds against the concrete `TaskT<T>` local (nested-generic binding).
#[inline]
fn tcs_get_task<T>(tcs: TaskCompletionSourceT<T>) -> TaskT<T> {
    rustc_clr_interop_generic_call1::<
        { CORELIB },
        { TCS_GEN },
        false,
        "get_Task",
        2u8,
        (T,),
        (TaskT<RustcCLRInteropTypeGeneric<0>>,), // Sig return: `Task<!0>` (def shape)
        TaskT<T>,
        TaskCompletionSourceT<T>,
    >(tcs)
}

/// Run a Rust [`Future`] to completion and package its output into a **completed** managed `Task<T>` —
/// the result-bearing Future→Task half of the bridge (the counterpart to [`future_to_task_unit`]). A
/// .NET caller can `await` the returned `Task<T>` and receive the value; because the work is already
/// done, it completes synchronously. Pair a Rust `async fn -> T` with this and `#[dotnet_export]` to
/// hand it to C# as an awaitable `Task<T>`-returning method.
pub fn future_to_task<T, F>(fut: F) -> TaskT<T>
where
    F: Future<Output = T>,
{
    let value = block_on(fut);
    let tcs = tcs_new::<T>();
    tcs_set_result::<T>(tcs, value);
    tcs_get_task::<T>(tcs)
}
