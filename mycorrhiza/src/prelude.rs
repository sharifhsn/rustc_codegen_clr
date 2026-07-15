//! One-glance import surface: `use mycorrhiza::prelude::*;` brings the everyday interop types and
//! macros into scope so a Rust-targets-.NET program reads like `std`.
//!
//! ```ignore
//! use mycorrhiza::prelude::*;
//!
//! let mut xs = List::<i32>::new();
//! xs.push(1);
//! xs.push(2);
//! let ys: List<i32> = vec![1, 2].into();
//! assert_eq!(xs, ys);                       // element-wise PartialEq
//!
//! let s = DotNetString::from("hi");
//! println!("{s}");                          // Display via the managed string
//! ```
//!
//! It intentionally re-exports only the *idiomatic* surface — the ready-to-use collection wrappers,
//! the managed-string type, and the WF-9 generic-bridge macros — not the low-level
//! [`crate::intrinsics`] magic. Reach into the fuller module paths when you need the raw handles.

// The .NET generic collections, used like `std`.
pub use crate::collections::{
    ConcurrentBag, ConcurrentDictionary, ConcurrentQueue, Dictionary, HashSet, LinkedList, List,
    ListIter, MutableDictionary, MutableList, PriorityQueue, Queue, ReadOnlyList, SortedDictionary,
    SortedSet, Stack,
};

// The idiomatic BCL type wrappers — used like normal Rust types (`DateTime::now()`, `Guid::new_v4()`,
// `Regex::new(..)`, `Math::sqrt(..)`, …). `TimeSpan` is re-exported under its idiomatic name (the
// module type is `DotNetTimeSpan` to avoid colliding with `std::time`-flavoured expectations).
pub use crate::bcl::dateonly::DateOnly;
pub use crate::bcl::datetime::DateTime;
pub use crate::bcl::datetimeoffset::DateTimeOffset;
pub use crate::bcl::decimal::Decimal;
pub use crate::bcl::decimal::DotNetDecimal;
pub use crate::bcl::environment::Environment;
pub use crate::bcl::guid::Guid;
pub use crate::bcl::mathf::Math;
pub use crate::bcl::random::Random;
pub use crate::bcl::regex::Regex;
pub use crate::bcl::stopwatch::Stopwatch;
pub use crate::bcl::stringbuilder::StringBuilder;
pub use crate::bcl::timespan::DotNetTimeSpan;
pub use crate::bcl::uri::Uri;
pub use crate::cancellation::{
    Cancellation, CancellationRegistration, CancellationRequested, CancellationToken,
};

// The enumerator bridge — `for x in &collection` over the reference-type collections, backed by the
// .NET `IEnumerator<T>`. The `Enumerable` trait provides `.iter_enumerator()`; `Enumerator<T>` is the
// resulting `Iterator`.
pub use crate::enumerate::{Enumerable, Enumerator, ManagedEnumerable};
pub use crate::enumerate_async::{
    AsyncEnumerable, AsyncEnumerator, AsyncNextFuture, AsyncStreamClosed, AsyncStreamSendFuture,
    AsyncStreamWriter, IAsyncEnumerable, IAsyncEnumerator,
};

// Delegates & callbacks — hand a Rust `extern "C" fn` to .NET as an `Action`/`Func`, or invoke a held
// .NET delegate. `Action*` wrappers are void-returning; `Func*` wrappers return a value.
pub use crate::delegate::{
    Action1, Action2, Action3, EventHandler, EventSubscription, Func1, Func2, Func3,
};
pub use crate::dispatch::{DispatchRejected, IUiDispatcher, UiDispatcher};

// The Task ↔ Future bridge — `.await` a .NET `Task` (`await_unit`) / `Task<T>` (`await_task`), expose a
// Rust `async fn` as a .NET `Task` (`future_to_task_unit`), and block a Rust future on the PAL
// (`block_on`). `Task` is the non-generic managed Task handle; `TaskT<T>` the result-bearing one.
pub use crate::task::{
    Task, TaskFuture, TaskT, TaskUnitFuture, ValueTask, ValueTaskT, await_task, await_unit,
    await_value_task, block_on, future_to_task, future_to_task_cancelable,
    future_to_task_cancelable_unit, future_to_task_unit, future_to_value_task_unit,
    task_into_value_task, value_task_into_task,
};

// The idiomatic managed `System.String` wrapper (Display / == / Hash) and the raw handle alias.
pub use crate::system::{DotNetString, MString};

// Idiomatic error/optional-value bridges: `null` ↔ `Option` (`Nullable::map_present` / `from_nullable`)
// and a thrown .NET exception ↔ `Result` (`try_managed` / the `.try_()` combinator).
pub use crate::error::{
    ManagedError, ManagedException, ManagedExceptionKind, Nullable, TryManaged, from_nullable,
    try_managed,
};

// `System.Nullable<T>` (a generic *value type*, distinct from managed-reference `null`) ↔ `Option<T>`:
// `.to_option()` on a `.NET`-produced nullable. The `Nullable<T>` type lives at `mycorrhiza::nullable`.
pub use crate::intrinsics::ManagedArray;
pub use crate::nullable::NullableExt;
pub use crate::progress::{Progress, ProgressReporter};

// `System.Span<T>` / `ReadOnlySpan<T>` — zero-copy views over a Rust slice, for handing Rust memory to
// a .NET API (or reading a managed span element-by-element).
pub use crate::span::{ReadOnlySpan, Span};

// GC-owned memory for APIs that retain a buffer or carry it across an async boundary. Unlike Span,
// construction copies a Rust slice into a managed array and therefore has no Rust borrow lifetime.
pub use crate::managed_option::{ManagedOption, ManagedRef};
pub use crate::memory::{Memory, ReadOnlyMemory};

// The single-codepoint managed `char`.
pub use crate::DotNetChar;

// The WF-9 generic-interop macros, for hand-rolling a wrapper over any other generic .NET type.
// (`#[macro_export]`ed, so they also live at the crate root; re-exported here for discoverability.)
pub use crate::{dotnet_generic, dotnet_generic_impl, r#gen};
