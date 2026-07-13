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
//! This is the consumer half of async streams. Producing a CLR `IAsyncEnumerable<T>` from a
//! Rust `async fn` remains a separate coroutine-layout problem.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::class::GenericClass;
use crate::intrinsics::{
    RustcCLRInteropManagedClass, RustcCLRInteropManagedGeneric,
    RustcCLRInteropManagedGenericStruct, RustcCLRInteropManagedStruct, RustcCLRInteropTypeGeneric,
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2,
};
use crate::task::{Task, TaskFuture, TaskT, await_task, await_unit, block_on};

const CORELIB: &str = "System.Private.CoreLib";
const ASYNC_ENUMERABLE: &str = "System.Collections.Generic.IAsyncEnumerable";
const ASYNC_ENUMERATOR: &str = "System.Collections.Generic.IAsyncEnumerator";
const VALUE_TASK: &str = "System.Threading.Tasks.ValueTask";
const TASK: &str = "System.Threading.Tasks.Task";
const CANCELLATION_TOKEN: &str = "System.Threading.CancellationToken";
const VALUE_TYPE_SIZE: usize = core::mem::size_of::<usize>() * 2;

/// Raw managed `IAsyncEnumerable<T>` interface handle.
pub type IAsyncEnumerable<T> =
    RustcCLRInteropManagedGeneric<{ CORELIB }, { ASYNC_ENUMERABLE }, (T,)>;
/// Raw managed `IAsyncEnumerator<T>` interface handle.
pub type IAsyncEnumerator<T> =
    RustcCLRInteropManagedGeneric<{ CORELIB }, { ASYNC_ENUMERATOR }, (T,)>;

type RootAsyncEnumerable<T> = GenericClass<{ CORELIB }, { ASYNC_ENUMERABLE }, (T,)>;
type RootAsyncEnumerator<T> = GenericClass<{ CORELIB }, { ASYNC_ENUMERATOR }, (T,)>;
type RawCancellationToken = RustcCLRInteropManagedStruct<
    { CORELIB },
    { CANCELLATION_TOKEN },
    { core::mem::size_of::<usize>() },
>;
type RawValueTaskBool =
    RustcCLRInteropManagedGenericStruct<{ CORELIB }, { VALUE_TASK }, { VALUE_TYPE_SIZE }, (bool,)>;
type RawValueTask = RustcCLRInteropManagedStruct<{ CORELIB }, { VALUE_TASK }, { VALUE_TYPE_SIZE }>;
type RawTask = RustcCLRInteropManagedClass<{ CORELIB }, { TASK }>;
type IAsyncDisposable = RustcCLRInteropManagedClass<{ CORELIB }, "System.IAsyncDisposable">;
type ValueTaskBoolDef =
    RustcCLRInteropManagedGenericStruct<{ CORELIB }, { VALUE_TASK }, { VALUE_TYPE_SIZE }, (bool,)>;

#[inline]
fn no_cancellation() -> RawCancellationToken {
    RawCancellationToken::vt_static0::<"get_None", RawCancellationToken>()
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
