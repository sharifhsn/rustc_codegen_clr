# Interop

Interop works in both directions:

- Export Rust functions, DTOs, classes, and containers to managed consumers.
- Call .NET methods, collections, tasks, delegates, LINQ, and selected BCL APIs from Rust through
  `mycorrhiza`.
- Call C ABI native libraries with ordinary Rust `#[link]` declarations lowered to CLR P/Invoke.

Prefer typed projection macros and the idiomatic `mycorrhiza` wrappers. Raw handles and intrinsic
calls remain available for unsupported surfaces, but they move ownership and ABI checks into your
code.

For asynchronous cancellation, return `Result<T, E>` from Rust and opt in with
`#[dotnet_export(cancellation = "task")]` only when every `Err` means cancellation. The exported
method then returns a genuinely canceled `Task<T>`/`Task`, so normal C# `await` throws
`OperationCanceledException`; unannotated async results remain rejected rather than guessed.

To expose a sequence, return `AsyncEnumerable<T>` from a synchronous exported factory and build it
with `AsyncEnumerable::spawn` or `try_spawn`. The producer body is an ordinary Rust future that
awaits `writer.send(value)`. C# receives `IAsyncEnumerable<T>` and uses normal `await foreach`,
including cancellation and automatic cleanup after `break`.

For UI work, accept `UiDispatcher` in the exported Rust API and call
`dispatcher.try_dispatch(move || { ... })` from a Rust worker. C# passes the bundled
`IRustUiDispatcher`: use `SynchronizationContextUiDispatcher.CaptureCurrent()` on Unity's main
thread, or `DelegateUiDispatcher` to adapt WinUI's `HasThreadAccess`/`TryEnqueue` and MAUI's
`IsDispatchRequired`/`Dispatch`. A managed lease releases the Rust closure exactly once if it runs,
is rejected, throws, or is abandoned during host shutdown.

These are separate paths. Use managed interop for .NET APIs and P/Invoke only when the dependency
is a native C ABI library.
