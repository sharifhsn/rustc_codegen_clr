# Threading / Sync / Async on .NET — what the PAL can provide (Class-D research)

Research into how the Rust threading/sync/async primitives that the compat-survey "Class D" crates
(parking_lot, dashmap, rayon, smol, crossbeam/flume) need can map onto .NET. Grounded in the *current*
dotnet PAL + the full `System.Threading` / `System.Threading.Tasks` / `System.Collections.Concurrent`
surface (the backend can name any BCL type via `ClassRef`).

## Headline

**Far more already works than the survey implied, and the remaining gap is small + cleanly mappable.**
Real preemptive OS threads, `Mutex`, and per-thread TLS are *done and contention-tested*
(`cargo_tests/pal_threads`: 4 threads × 100k shared-`Mutex<u64>` increments = 400000, no lost updates).
The gaps are **four stubbed sync primitives** (`Parker`, `Once`, `Condvar`, `RwLock`, today routed to
`no_threads.rs`/`unsupported.rs`), each with a clean BCL mapping — and there is a **keystone**: rayon's
only failure is the stubbed `Once`, which itself just needs a real **`Parker`**. So *one* ~30-line PAL
arm (a `ManualResetEventSlim`/`SemaphoreSlim`-backed Parker) lets std's **generic** queue-based
`Once`/`Condvar`/`RwLock` arms — the same code the Linux/futex target runs — be used unmodified, and
unblocks rayon. **Do not build a futex**; map the abstraction one level up at the sync-primitive layer
(the existing `SemaphoreSlim` `Mutex` already proves this works).

## 1. Already works (done + verified)

| Rust primitive | .NET mechanism | where |
|---|---|---|
| `thread::spawn` / `JoinHandle::join` | `System.Threading.Thread` + a native-fnptr `UnmanagedThreadStart` trampoline; `GCHandle` as the handle; `Thread.Join()` | `cilly/.../thread.rs:251`, `dotnet.rs:554/662` |
| `std::sync::Mutex` | `SemaphoreSlim(1,1)` Wait/Release/Wait(0) — **not** `Monitor` (Monitor is reentrant; std Mutex must not be) | `dotnet_pal/sys/sync/mutex/`, `dotnet.rs:797` |
| `thread_local!` + `#[thread_local]` | `ThreadLocal<nint>` per key; native `[ThreadStatic]` for the unstable attribute | `dotnet.rs:939`, `ClassRef::thread_local` |
| atomics (32/64/usize, CAS, swap, fetch_*) | `System.Threading.Interlocked.*`; fences → `Thread.MemoryBarrier`; loads/stores → `volatile.ldind/stind` | `cilly/.../atomics.rs` |
| `available_parallelism`, `yield_now`, `sleep` | `Environment.ProcessorCount`, `Thread.Yield()`, `Thread.Sleep` | `dotnet.rs:1058/735/763` |

*Stale comments to fix:* several arms still say "TLS is process-global / `[ThreadStatic]` deferred" — no
longer true.

## 2. The gaps + their mappings

| Rust primitive | .NET mechanism | feasibility |
|---|---|---|
| **`Parker` (park/unpark)** — the keystone | per-thread `ManualResetEventSlim` (park = `Wait`/`Wait(ms)`, unpark = `Set`); or `SemaphoreSlim(0,1)` (token-not-lost for free) | moderate |
| `std::sync::Once` / `OnceLock` | std's **generic `queue` Once** (pure Parker+atomics) once Parker is real; or `LazyInitializer.EnsureInitialized` / `Lazy<T>` | moderate (free after Parker) |
| `std::sync::Condvar` | std's generic queue Condvar over Parker; or directly `Monitor.Wait/Pulse/PulseAll`; or `ManualResetEventSlim` | moderate (free after Parker) |
| `std::sync::RwLock` | std's generic queue RwLock over Parker; or directly `ReaderWriterLockSlim` (EnterReadLock/EnterWriteLock) | moderate (free after Parker) |
| `std::sync::Barrier` | `System.Threading.Barrier` (`SignalAndWait`) | trivial |
| thread id / name | `Thread.CurrentThread.ManagedThreadId` / `.Name` (one hook; diagnostics-only) | trivial |

## 3. The keystone: the Parker-first cascade

`rayon` builds and computes the *correct* parallel result, then aborts in its lazy global-pool init with
`OnceLock: one-time initialization may not be performed recursively`. Root cause: the `no_threads` `Once`
panics on observing `State::Running` — it cannot represent "another thread is initializing; block until
Complete." rayon's worker threads legitimately re-enter `Registry::current()` during init, which the
single-thread stub misreads as illegal same-thread recursion. **Not a codegen bug — the stubbed `Once`.**

The fix is *not* four bespoke arms. Build **one** `Parker` (a `ManualResetEventSlim` per thread, with the
EMPTY/PARKED/NOTIFIED state machine in an `AtomicI8`), then repoint `palinject.rs` so `sys::sync::{once,
condvar,rwlock}` use std's **generic `queue` implementations** — which are written purely against `Parker`
+ atomics and are the exact code Linux runs. One PAL arm + ~3 BCL hooks collapses three stubs and
unblocks rayon (and `parking_lot`, whose generic `ThreadParker` fallback also rides on std `Mutex`+`Parker`).

## 4. Don't build a futex — map one level up

A Linux `futex(addr, WAIT/WAKE)` is only ever an *implementation detail* of Mutex/Condvar/RwLock/Once/
Parker. The dotnet target already proves you can route std's sync arms straight to BCL primitives and skip
the futex arm entirely (the `SemaphoreSlim` Mutex is the proof). So the deliverable is the BCL-backed sync
arms, not a futex emulator. *(If a crate ever needs a raw `core::sync::atomic::AtomicU32::wait` futex, the
creative polyfill is a `static ConcurrentDictionary<nint_addr, ManualResetEventSlim>`: `wait(addr,exp)` =
if `*addr==exp` get-or-add the event and `Wait`; `wake(addr,n)` = `Set` — but no Class-D crate needs it.)*

## 5. Overlays: drop-in BCL types for whole crates

Some Class-D crates have a **semantically-equivalent BCL type** and can be handled by an *overlay* (the
same `dotnet_overlays/` mechanism used for mio/socket2), no primitive needed:

- `dashmap::DashMap` → `System.Collections.Concurrent.ConcurrentDictionary` (sharded concurrent map).
- `crossbeam-channel` / `flume` → `System.Threading.Channels.Channel<T>` (bounded/unbounded MPMC).
- `arc-swap::ArcSwap` → native, just needs `AtomicPtr` CAS (already have Interlocked) — no overlay.

This is an explicit *overlay-vs-build-the-primitive* choice per crate: build the primitive when the crate
reimplements sync (parking_lot, once_cell), overlay when a BCL type is a clean drop-in (dashmap, channels).

## 6. Async (smol / async-io)

`tokio-net` already works via the Rust reactor backed by the epoll/libc shim + eventfd-as-self-connected-
socket. **smol maps the same way (ROUTE a):** back its `async-io`/`polling` reactor with the existing
epoll shim. Staged:
1. **(trivial)** the actual compile blocker is just two missing libc symbols — add `strlen` to the posix
   shim and `ERANGE`=34. Self-contained.
2. **(moderate)** force `polling`/`rustix` onto the libc backend (RUSTFLAGS `--cfg=rustix_use_libc`), so
   `Poller`→ the epoll shim and `Async<T>` works automatically.
3. **(moderate)** `async-io` `Timer` needs `timerfd` — **creative:** `timerfd` = a loopback socket armed
   readable by a `System.Threading.Timer` callback that self-sends 1 byte at the deadline. Generalizes the
   eventfd-as-socket trick; the reactor sees it as just another readable fd, zero new reactor concepts.

(Route b — bridging Rust futures to .NET `Task`/`ValueTask` — is the path for *exposing* a Rust async
entry point to C#, a separate interop feature, not needed to run smol internally.)

## 7. Atomics residuals

- **Sub-word static atoms (WF-5):** a `static AtomicU8`/`U16` can fault because the masked-32-bit-CAS
  emulation aligns the address *down* and can read off the page. Fix in static emission: pad/align any
  static carrying a sub-word atomic to a 4-byte word. (Unblocks the default panic hook — `pal_panic`.)
- **128-bit atomics:** no native `Interlocked` for 128-bit; emulate with a `SpinLock`/`Monitor`-guarded
  critical section, or the address-keyed lock. Rare.

## 8. Roadmap (this is "WF-F Slice 2", sequenced by dependency)

1. **Parker** (`ManualResetEventSlim` per thread; `rcl_dotnet_park/unpark/park_timeout` hooks + a
   `ClassRef::manual_reset_event_slim`, CoreLib-named). **Keystone.**
2. Repoint `Once`/`Condvar`/`RwLock` `palinject` arms off `no_threads`/`unsupported` to std's generic
   `queue` impls → **rayon runs; parking_lot-via-std works.**
3. **Overlays:** dashmap → `ConcurrentDictionary`, crossbeam/flume → `Channel<T>`.
4. **smol stage 1–2** (`strlen`+`ERANGE`, then `rustix_use_libc`); stage 3 (timerfd) if timers are needed.
5. TLS-drop-on-thread-exit (wrap `UnmanagedThreadStart.Start` in a try/finally → `rt::thread_cleanup`) +
   the WF-5 static-atomic padding + thread-id/name hooks (all small, independent).

**Bottom line:** there is **no fundamental .NET wall** for Class D. Real threads/Mutex/TLS/atomics are
done; the rest is a single keystone primitive (Parker) + routing std's generic arms + a few BCL overlays
— a focused, well-understood PAL slice, not a research risk.
