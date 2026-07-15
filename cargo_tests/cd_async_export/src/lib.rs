#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::dotnet_export;
use mycorrhiza::cancellation::{Cancellation, CancellationRequested, CancellationToken};
use mycorrhiza::collections::{MutableDictionary, MutableList};
use mycorrhiza::dispatch::UiDispatcher;
use mycorrhiza::enumerate::ManagedEnumerable;
use mycorrhiza::enumerate_async::{AsyncEnumerable, AsyncStreamWriter};
use mycorrhiza::memory::{Memory, ReadOnlyMemory};
use mycorrhiza::managed_option::ManagedOption;
use mycorrhiza::progress::{Progress, ProgressReporter};
use mycorrhiza::system::MString;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

static ACTIVE_STREAM_PRODUCERS: AtomicI32 = AtomicI32::new(0);
static STREAM_EMITTED: AtomicI32 = AtomicI32::new(0);
static ACTIVE_UI_DISPATCHES: AtomicI32 = AtomicI32::new(0);
static UI_DISPATCH_MARKER: AtomicI32 = AtomicI32::new(0);
static UI_DISPATCH_THREAD: AtomicI32 = AtomicI32::new(0);

struct ActiveStreamProducer;

impl ActiveStreamProducer {
    fn enter() -> Self {
        ACTIVE_STREAM_PRODUCERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for ActiveStreamProducer {
    fn drop(&mut self) {
        ACTIVE_STREAM_PRODUCERS.fetch_sub(1, Ordering::SeqCst);
    }
}

struct ActiveUiDispatch;

impl ActiveUiDispatch {
    fn enter() -> Self {
        ACTIVE_UI_DISPATCHES.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for ActiveUiDispatch {
    fn drop(&mut self) {
        ACTIVE_UI_DISPATCHES.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn yield_once() {
    let mut first_poll = true;
    std::future::poll_fn(move |context| {
        if first_poll {
            first_poll = false;
            context.waker().wake_by_ref();
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    })
    .await;
}

/// Computes a score after a genuine suspension point and returns a C#-awaitable Task<int>.
#[dotnet_export(name = "ComputeScoreAsync")]
pub async fn compute_score_async(base: i32) -> i32 {
    yield_once().await;
    base * 2
}

/// Unit-returning async exports become ordinary non-generic Task values.
#[dotnet_export(name = "WarmUpAsync")]
pub async fn warm_up_async() {}

/// Synchronous exports accept framework-native cancellation and progress contracts directly.
#[dotnet_export(name = "ReportAndObserve")]
pub fn report_and_observe(token: CancellationToken, progress: Progress<i32>) -> bool {
    progress.report(50);
    progress.report(100);
    token.is_cancellation_requested()
}

/// Rooted projections remain safe in a Rust coroutine across a genuine suspension point.
#[dotnet_export(name = "ReportAndObserveAsync")]
pub async fn report_and_observe_async(
    mut cancellation: Cancellation,
    progress: ProgressReporter<i32>,
) -> bool {
    progress.report(25);
    yield_once().await;
    progress.report(100);
    cancellation.is_cancellation_requested()
}

/// An explicit cancellation policy maps the Rust error branch to TaskStatus.Canceled rather than
/// a faulted task or a successful boolean sentinel.
#[dotnet_export(name = "CancelableScoreAsync", cancellation = "task")]
pub async fn cancelable_score_async(
    mut cancellation: Cancellation,
    value: i32,
) -> Result<i32, CancellationRequested> {
    yield_once().await;
    cancellation.ensure_not_canceled()?;
    Ok(value * 2)
}

/// The same policy supports a non-generic Task for Result<(), E>.
#[dotnet_export(name = "CancelableUnitAsync", cancellation = "task")]
pub async fn cancelable_unit_async(
    mut cancellation: Cancellation,
) -> Result<(), CancellationRequested> {
    yield_once().await;
    cancellation.ensure_not_canceled()
}

/// A retained read-only buffer is rooted before the Rust future is created, so a genuine
/// suspension point never places its embedded CLR reference in overlapping coroutine state.
#[dotnet_export(name = "SumReadOnlyMemoryAsync")]
pub async fn sum_readonly_memory_async(values: ReadOnlyMemory<i32>) -> i32 {
    yield_once().await;
    let mut copy = Memory::from_slice(&vec![0; values.len() as usize]);
    values.copy_to(&mut copy);
    copy.to_vec().into_iter().sum()
}

/// Mutable `Memory<T>` retains the caller's backing storage across suspension and writes through
/// the original sliced view.
#[dotnet_export(name = "FillMemoryAsync")]
pub async fn fill_memory_async(mut values: Memory<i32>, fill: i32) -> i32 {
    yield_once().await;
    values.fill(fill);
    values.len()
}

/// Familiar mutable collection interfaces are GC-rooted before the future is created.
#[dotnet_export(name = "UpdateCollectionsAsync")]
pub async fn update_collections_async(
    mut list: MutableList<i32>,
    mut dictionary: MutableDictionary<i32, i32>,
) -> i32 {
    yield_once().await;
    list.push(8);
    dictionary.insert(8, list.len());
    dictionary.get(8).unwrap_or_default()
}

/// The producer interface itself remains rooted across suspension; iteration starts afterward.
#[dotnet_export(name = "SumSequenceAsync")]
pub async fn sum_sequence_async(values: ManagedEnumerable<i32>) -> i32 {
    yield_once().await;
    values.iter().sum()
}

fn is_non_null_string(value: MString) -> bool {
    !mycorrhiza::intrinsics::rustc_clr_interop_managed_is_null(value)
}

/// Nullable managed references use a pointer-only rooted option in coroutine state.
#[dotnet_export(name = "HasTextAsync")]
pub async fn has_text_async(value: ManagedOption<MString>) -> bool {
    yield_once().await;
    match value.as_ref() {
        Some(text) => text.with_raw(is_non_null_string),
        None => false,
    }
}

/// Produce a real single-consumer IAsyncEnumerable<int> with one-item backpressure.
#[dotnet_export(name = "ScoresAsync")]
pub fn scores_async(count: i32, delay_ms: i32) -> AsyncEnumerable<i32> {
    STREAM_EMITTED.store(0, Ordering::SeqCst);
    AsyncEnumerable::spawn(move |writer: AsyncStreamWriter<i32>| async move {
        let _active = ActiveStreamProducer::enter();
        for value in 1..=count {
            std::thread::sleep(Duration::from_millis(delay_ms.max(0) as u64));
            if writer.send(value * 10).await.is_err() {
                break;
            }
            STREAM_EMITTED.fetch_add(1, Ordering::SeqCst);
        }
    })
}

/// A producer error faults async enumeration instead of looking like graceful completion.
#[dotnet_export(name = "FaultingScoresAsync")]
pub fn faulting_scores_async() -> AsyncEnumerable<i32> {
    AsyncEnumerable::try_spawn(move |writer: AsyncStreamWriter<i32>| async move {
        let _active = ActiveStreamProducer::enter();
        writer
            .send(7)
            .await
            .map_err(|_| "managed consumer stopped")?;
        Err::<(), _>("stream boom from Rust")
    })
}

#[dotnet_export(name = "ActiveStreamProducers")]
pub fn active_stream_producers() -> i32 {
    ACTIVE_STREAM_PRODUCERS.load(Ordering::SeqCst)
}

#[dotnet_export(name = "StreamEmittedCount")]
pub fn stream_emitted_count() -> i32 {
    STREAM_EMITTED.load(Ordering::SeqCst)
}

/// Start on a Rust worker and transfer one owned closure to the managed UI dispatcher.
#[dotnet_export(name = "StartUiDispatch")]
pub fn start_ui_dispatch(dispatcher: UiDispatcher, marker: i32, panic_after_run: bool) -> bool {
    UI_DISPATCH_MARKER.store(0, Ordering::SeqCst);
    UI_DISPATCH_THREAD.store(0, Ordering::SeqCst);
    let active = ActiveUiDispatch::enter();
    std::thread::Builder::new()
        .name("rust-dotnet-ui-dispatch".to_string())
        .spawn(move || {
            let _ = dispatcher.try_dispatch(move || {
                let _active = active;
                let thread =
                    mycorrhiza::bindings::System::Threading::Thread::get_current_thread();
                UI_DISPATCH_THREAD.store(thread.get_managed_thread_id(), Ordering::SeqCst);
                UI_DISPATCH_MARKER.store(marker, Ordering::SeqCst);
                if panic_after_run {
                    panic!("intentional UI-dispatch callback panic");
                }
            });
        })
        .is_ok()
}

#[dotnet_export(name = "ActiveUiDispatches")]
pub fn active_ui_dispatches() -> i32 {
    ACTIVE_UI_DISPATCHES.load(Ordering::SeqCst)
}

#[dotnet_export(name = "UiDispatchMarker")]
pub fn ui_dispatch_marker() -> i32 {
    UI_DISPATCH_MARKER.load(Ordering::SeqCst)
}

#[dotnet_export(name = "UiDispatchThread")]
pub fn ui_dispatch_thread() -> i32 {
    UI_DISPATCH_THREAD.load(Ordering::SeqCst)
}
