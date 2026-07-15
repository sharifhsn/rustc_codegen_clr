//! Consume a managed `IAsyncEnumerable<T>` from Rust with a typed, backpressure-preserving bridge.
//!
//! [`AsyncEnumerable::get_async_enumerator`] obtains the real managed enumerator. Each
//! [`AsyncEnumerator::next`] call invokes `MoveNextAsync`, converts its `ValueTask<bool>` to a
//! `Task<bool>`, and returns a hand-written Rust [`Future`]. No sequence is
//! pre-buffered: the next item is requested only when Rust asks for it.
//!
//! The wrappers root managed handles through [`crate::class::GenericClass`]. Consequently, an
//! [`AsyncNextFuture`] stores only GCHandle-backed value types and ordinary Rust references across
//! suspension; it never places a raw CLR object reference in overlapping Rust coroutine storage.
//! [`AsyncEnumerable::spawn`] and [`AsyncEnumerable::try_spawn`] provide the producer half. They
//! run a Rust future on an owned worker and feed a one-item managed channel. The CLR's own
//! `ChannelReader<T>.ReadAllAsync` supplies the real `ValueTask<bool>` state machine, while a small
//! bundled managed adapter notifies Rust exactly once on normal completion, cancellation, early
//! `await foreach` disposal, or abandonment before enumeration.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cancellation::CancellationToken;
use crate::class::GenericClass;
use crate::delegate::Action1;
use crate::intrinsics::{
    RustcCLRInteropManagedClass, RustcCLRInteropManagedGeneric,
    RustcCLRInteropManagedGenericStruct, RustcCLRInteropManagedStruct, RustcCLRInteropTypeGeneric,
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2,
    rustc_clr_interop_generic_ctor2, rustc_clr_interop_managed_checked_cast,
};
use crate::sync::{Sender, bounded_channel};
use crate::task::{Task, TaskFuture, TaskT, await_task, await_unit, block_on};

const CORELIB: &str = "System.Private.CoreLib";
const ASYNC_ENUMERABLE: &str = "System.Collections.Generic.IAsyncEnumerable";
const ASYNC_ENUMERATOR: &str = "System.Collections.Generic.IAsyncEnumerator";
const VALUE_TASK: &str = "System.Threading.Tasks.ValueTask";
const TASK: &str = "System.Threading.Tasks.Task";
const VALUE_TYPE_SIZE: usize = core::mem::size_of::<usize>() * 2;

/// Raw managed `IAsyncEnumerable<T>` interface handle.
pub type IAsyncEnumerable<T> =
    RustcCLRInteropManagedGeneric<{ CORELIB }, { ASYNC_ENUMERABLE }, (T,)>;
/// Raw managed `IAsyncEnumerator<T>` interface handle.
pub type IAsyncEnumerator<T> =
    RustcCLRInteropManagedGeneric<{ CORELIB }, { ASYNC_ENUMERATOR }, (T,)>;

type RootAsyncEnumerable<T> = GenericClass<{ CORELIB }, { ASYNC_ENUMERABLE }, (T,)>;
type RootAsyncEnumerator<T> = GenericClass<{ CORELIB }, { ASYNC_ENUMERATOR }, (T,)>;
type RawCancellationToken = CancellationToken;
type RawValueTaskBool =
    RustcCLRInteropManagedGenericStruct<{ CORELIB }, { VALUE_TASK }, { VALUE_TYPE_SIZE }, (bool,)>;
type RawValueTask = RustcCLRInteropManagedStruct<{ CORELIB }, { VALUE_TASK }, { VALUE_TYPE_SIZE }>;
type RawTask = RustcCLRInteropManagedClass<{ CORELIB }, { TASK }>;
type IAsyncDisposable = RustcCLRInteropManagedClass<{ CORELIB }, "System.IAsyncDisposable">;
type ValueTaskBoolDef =
    RustcCLRInteropManagedGenericStruct<{ CORELIB }, { VALUE_TASK }, { VALUE_TYPE_SIZE }, (bool,)>;

const HELPERS_ASSEMBLY: &str = "Mycorrhiza.Interop.Helpers";
const RUST_STREAM_LEASE: &str = "Mycorrhiza.Interop.Helpers.RustStreamLease";
const RUST_ASYNC_ENUMERABLE: &str = "Mycorrhiza.Interop.Helpers.RustAsyncEnumerable";

type RustStreamLease = RustcCLRInteropManagedClass<{ HELPERS_ASSEMBLY }, { RUST_STREAM_LEASE }>;
type RawRustAsyncEnumerable<T> =
    RustcCLRInteropManagedGeneric<{ HELPERS_ASSEMBLY }, { RUST_ASYNC_ENUMERABLE }, (T,)>;

extern "C" fn release_rust_stream(id: i64) {
    assert_ne!(id, 0, "Rust async-stream lease received a null identifier");
    // SAFETY: `wrap_rust_stream` allocates exactly one `Box<Arc<AtomicBool>>` for this identifier.
    // RustStreamLease exchanges its callback with null before invoking it, so every managed
    // completion/disposal/finalizer path converges on exactly one reconstruction and drop.
    let stopped = unsafe { Box::from_raw(id as usize as *mut Arc<AtomicBool>) };
    stopped.store(true, Ordering::Release);
}

fn wrap_rust_stream<T>(
    source: IAsyncEnumerable<T>,
    stopped: Arc<AtomicBool>,
) -> IAsyncEnumerable<T> {
    type AsyncEnumerableDef = RustcCLRInteropManagedGeneric<
        { CORELIB },
        { ASYNC_ENUMERABLE },
        (RustcCLRInteropTypeGeneric<0>,),
    >;
    let id = Box::into_raw(Box::new(stopped)) as usize as i64;
    let release = Action1::<i64>::from_fn(release_rust_stream);
    let lease = RustStreamLease::ctor2(id, release.handle());
    let wrapper = rustc_clr_interop_generic_ctor2::<
        { HELPERS_ASSEMBLY },
        { RUST_ASYNC_ENUMERABLE },
        false,
        (T,),
        ((), AsyncEnumerableDef, RustStreamLease),
        RawRustAsyncEnumerable<T>,
        IAsyncEnumerable<T>,
        RustStreamLease,
    >(source, lease);
    rustc_clr_interop_managed_checked_cast::<IAsyncEnumerable<T>, RawRustAsyncEnumerable<T>>(
        wrapper,
    )
}

#[inline]
fn no_cancellation() -> RawCancellationToken {
    CancellationToken::none()
}

#[inline]
fn get_async_enumerator<T>(source: IAsyncEnumerable<T>) -> IAsyncEnumerator<T> {
    type EnumeratorDef = RustcCLRInteropManagedGeneric<
        { CORELIB },
        { ASYNC_ENUMERATOR },
        (RustcCLRInteropTypeGeneric<0>,),
    >;
    rustc_clr_interop_generic_call2::<
        { CORELIB },
        { ASYNC_ENUMERABLE },
        false,
        "GetAsyncEnumerator",
        2,
        (T,),
        (EnumeratorDef, RawCancellationToken),
        IAsyncEnumerator<T>,
        IAsyncEnumerable<T>,
        RawCancellationToken,
    >(source, no_cancellation())
}

#[inline]
fn move_next_task<T>(enumerator: IAsyncEnumerator<T>) -> TaskT<bool> {
    let value_task: RawValueTaskBool = rustc_clr_interop_generic_call1::<
        { CORELIB },
        { ASYNC_ENUMERATOR },
        false,
        "MoveNextAsync",
        2,
        (T,),
        (ValueTaskBoolDef,),
        RawValueTaskBool,
        IAsyncEnumerator<T>,
    >(enumerator);
    rustc_clr_interop_generic_call1::<
        { CORELIB },
        { VALUE_TASK },
        true,
        "AsTask",
        1,
        (bool,),
        (RustcCLRInteropManagedGeneric<{ CORELIB }, { TASK }, (RustcCLRInteropTypeGeneric<0>,)>,),
        TaskT<bool>,
        &RawValueTaskBool,
    >(&value_task)
}

#[inline]
fn current<T>(enumerator: IAsyncEnumerator<T>) -> T {
    rustc_clr_interop_generic_call1::<
        { CORELIB },
        { ASYNC_ENUMERATOR },
        false,
        "get_Current",
        2,
        (T,),
        (RustcCLRInteropTypeGeneric<0>,),
        T,
        IAsyncEnumerator<T>,
    >(enumerator)
}

#[inline]
fn dispose_task<T>(enumerator: IAsyncEnumerator<T>) -> Task {
    let disposable = crate::intrinsics::rustc_clr_interop_managed_checked_cast::<
        IAsyncDisposable,
        IAsyncEnumerator<T>,
    >(enumerator);
    let value_task = disposable.virt0::<"DisposeAsync", RawValueTask>();
    Task::from_raw(value_task.vt_instance0::<"AsTask", RawTask>())
}

/// A rooted managed async sequence.
pub struct AsyncEnumerable<T> {
    handle: RootAsyncEnumerable<T>,
}

impl<T> AsyncEnumerable<T> {
    /// Wrap an `IAsyncEnumerable<T>` returned by a .NET API.
    #[inline]
    pub fn from_handle(handle: IAsyncEnumerable<T>) -> Self {
        Self {
            handle: RootAsyncEnumerable::from_naked_ref(handle),
        }
    }

    /// Obtain a new managed enumerator using `CancellationToken.None`.
    #[inline]
    pub fn get_async_enumerator(&self) -> AsyncEnumerator<T> {
        let source = unsafe { self.handle.get_naked_ref() };
        AsyncEnumerator::from_handle(get_async_enumerator(source))
    }

    /// Expose the raw interface handle for another managed call. Use it only transiently.
    #[inline]
    pub fn raw(&self) -> IAsyncEnumerable<T> {
        unsafe { self.handle.get_naked_ref() }
    }

    /// Consume the Rust root and return the real managed interface at an immediate call boundary.
    /// This is primarily used by `#[dotnet_export]`; managed code roots the returned object.
    #[inline]
    pub fn into_handle(self) -> IAsyncEnumerable<T> {
        self.raw()
    }

    /// Produce a single-consumer CLR `IAsyncEnumerable<T>` from a Rust future.
    ///
    /// The producer receives an [`AsyncStreamWriter`] backed by a one-item channel. Awaiting
    /// [`AsyncStreamWriter::send`] therefore preserves backpressure: Rust cannot advance more than
    /// one item ahead of C#. Normal completion closes the stream. Cancellation or early C# loop
    /// exit marks the writer stopped, causing the pending/next `send` to return its item in
    /// [`AsyncStreamClosed`]. A producer doing lengthy work between sends should also poll
    /// [`AsyncStreamWriter::is_cancellation_requested`].
    pub fn spawn<F, Fut>(producer: F) -> Self
    where
        T: Copy + Send + 'static,
        F: FnOnce(AsyncStreamWriter<T>) -> Fut + Send + 'static,
        Fut: Future<Output = ()>,
    {
        let (sender, receiver) = bounded_channel::<T>(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let writer = AsyncStreamWriter::new(sender, Arc::clone(&stopped));
        let completion = writer.clone();
        let source = receiver.read_all_async().into_handle();
        let managed = wrap_rust_stream(source, Arc::clone(&stopped));

        std::thread::Builder::new()
            .name("rust-dotnet-async-stream".to_string())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    block_on(producer(writer));
                }));
                match outcome {
                    Ok(()) => {
                        completion.close();
                    }
                    Err(_) => {
                        completion.close_with_error("Rust async-stream producer panicked");
                    }
                }
            })
            .expect("failed to spawn Rust async-stream producer");

        Self::from_handle(managed)
    }

    /// The fallible sibling of [`Self::spawn`]. An `Err` faults the managed channel, so C# observes
    /// the error from `await foreach` rather than mistaking it for graceful end-of-stream.
    pub fn try_spawn<F, Fut, E>(producer: F) -> Self
    where
        T: Copy + Send + 'static,
        F: FnOnce(AsyncStreamWriter<T>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), E>>,
        E: fmt::Display,
    {
        let (sender, receiver) = bounded_channel::<T>(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let writer = AsyncStreamWriter::new(sender, Arc::clone(&stopped));
        let completion = writer.clone();
        let source = receiver.read_all_async().into_handle();
        let managed = wrap_rust_stream(source, Arc::clone(&stopped));

        std::thread::Builder::new()
            .name("rust-dotnet-async-stream".to_string())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    block_on(producer(writer))
                }));
                match outcome {
                    Ok(Ok(())) => {
                        completion.close();
                    }
                    Ok(Err(error)) => {
                        completion.close_with_error(&error.to_string());
                    }
                    Err(_) => {
                        completion.close_with_error("Rust async-stream producer panicked");
                    }
                }
            })
            .expect("failed to spawn Rust async-stream producer");

        Self::from_handle(managed)
    }
}

/// The producer end of a Rust-owned async stream.
pub struct AsyncStreamWriter<T> {
    sender: Sender<T>,
    stopped: Arc<AtomicBool>,
}

impl<T> AsyncStreamWriter<T> {
    fn new(sender: Sender<T>, stopped: Arc<AtomicBool>) -> Self {
        Self { sender, stopped }
    }

    /// `true` after managed enumeration completed, was canceled, was disposed early, or was
    /// abandoned and finalized.
    #[inline]
    pub fn is_cancellation_requested(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    /// Close successfully. This is automatic when the producer future returns, but useful when a
    /// producer wants to terminate before returning from surrounding cleanup code.
    #[inline]
    pub fn close(&self) -> bool {
        self.sender.close()
    }

    /// Fault the stream with a normal managed exception.
    #[inline]
    pub fn close_with_error(&self, message: &str) -> bool {
        self.sender.close_with_error(message)
    }
}

impl<T: Copy> AsyncStreamWriter<T> {
    /// Wait until C# has room for one item, or return the unsent item if enumeration stopped.
    #[inline]
    pub fn send(&self, item: T) -> AsyncStreamSendFuture<'_, T> {
        AsyncStreamSendFuture { writer: self, item }
    }
}

impl<T> Clone for AsyncStreamWriter<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            stopped: Arc::clone(&self.stopped),
        }
    }
}

/// An item that could not be emitted because managed enumeration ended.
pub struct AsyncStreamClosed<T> {
    item: T,
}

impl<T> AsyncStreamClosed<T> {
    /// Recover the item that was not sent.
    pub fn into_inner(self) -> T {
        self.item
    }
}

impl<T> fmt::Debug for AsyncStreamClosed<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncStreamClosed(..)")
    }
}

impl<T> fmt::Display for AsyncStreamClosed<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("managed async-stream enumeration has ended")
    }
}

impl<T> std::error::Error for AsyncStreamClosed<T> {}

/// Future returned by [`AsyncStreamWriter::send`]. It keeps the item in Rust until the one-slot
/// managed channel accepts it, so cancellation never loses ownership ambiguously.
pub struct AsyncStreamSendFuture<'a, T: Copy> {
    writer: &'a AsyncStreamWriter<T>,
    item: T,
}

impl<T: Copy> Future for AsyncStreamSendFuture<'_, T> {
    type Output = Result<(), AsyncStreamClosed<T>>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: the future never exposes a pinned projection and neither field is
        // address-sensitive. The item remains in place until this poll returns.
        let this = unsafe { self.get_unchecked_mut() };
        if this.writer.is_cancellation_requested() {
            return Poll::Ready(Err(AsyncStreamClosed { item: this.item }));
        }
        match this.writer.sender.try_send(this.item) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(_) => {
                std::thread::yield_now();
                context.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

/// A rooted managed `IAsyncEnumerator<T>`.
pub struct AsyncEnumerator<T> {
    handle: RootAsyncEnumerator<T>,
    disposed: bool,
}

impl<T> AsyncEnumerator<T> {
    /// Wrap an `IAsyncEnumerator<T>` returned by a .NET API.
    #[inline]
    pub fn from_handle(handle: IAsyncEnumerator<T>) -> Self {
        Self {
            handle: RootAsyncEnumerator::from_naked_ref(handle),
            disposed: false,
        }
    }

    /// Request exactly one item. The returned future resolves to `None` when the managed sequence
    /// completes and preserves the managed producer's asynchronous backpressure.
    #[inline]
    pub fn next(&mut self) -> AsyncNextFuture<'_, T> {
        assert!(!self.disposed, "cannot advance a disposed async enumerator");
        let handle = unsafe { self.handle.get_naked_ref() };
        AsyncNextFuture {
            enumerator: self,
            move_next: await_task(move_next_task(handle)),
        }
    }

    /// Blocking convenience over [`Self::next`], useful for synchronous Rust entry points.
    #[inline]
    pub fn next_blocking(&mut self) -> Option<T> {
        block_on(self.next())
    }

    /// Drain the sequence in order, requesting each item only after the previous one completed.
    pub fn collect_blocking(mut self) -> Vec<T> {
        let mut values = Vec::new();
        while let Some(value) = self.next_blocking() {
            values.push(value);
        }
        self.dispose_blocking();
        values
    }

    /// Run the managed `IAsyncDisposable.DisposeAsync` contract to completion. Call this when
    /// stopping before the sequence ends; [`Self::collect_blocking`] does it automatically.
    pub fn dispose_blocking(&mut self) {
        if !self.disposed {
            let handle = unsafe { self.handle.get_naked_ref() };
            block_on(await_unit(dispose_task(handle)));
            self.disposed = true;
        }
    }

    /// Expose the raw interface handle for another managed call. Use it only transiently.
    #[inline]
    pub fn raw(&self) -> IAsyncEnumerator<T> {
        unsafe { self.handle.get_naked_ref() }
    }
}

/// Future returned by [`AsyncEnumerator::next`].
pub struct AsyncNextFuture<'a, T> {
    enumerator: &'a mut AsyncEnumerator<T>,
    move_next: TaskFuture<bool>,
}

impl<T> Future for AsyncNextFuture<'_, T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: neither field is address-sensitive. We never move either field after projection.
        let this = unsafe { self.get_unchecked_mut() };
        match Pin::new(&mut this.move_next).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(false) => Poll::Ready(None),
            Poll::Ready(true) => Poll::Ready(Some(current(unsafe {
                this.enumerator.handle.get_naked_ref()
            }))),
        }
    }
}
