//! Thread-local storage for the .NET ("dotnet") platform.
//!
//! This is a **single-threaded-correct** TLS backend, modelled on the
//! `no_threads` implementation that wasm/zkvm/uefi use: the codegen backend
//! currently produces a single managed thread of execution, so a process-global
//! store *is* the thread-local store. When real .NET threading lands this whole
//! file should be replaced with a `[ThreadStatic]`/`ThreadLocal<T>`-backed key
//! table; until then "one thread" makes the storage trivially correct.
//!
//! Two surfaces are provided:
//!
//! * The **storage** layer (`EagerStorage`, `LazyStorage`, `thread_local_inner`,
//!   `LocalPointer`, `local_pointer`) that the `thread_local!` macro expands to.
//!   This is what `sys::thread_local`'s top cascade re-exports for `os = dotnet`,
//!   and it is a verbatim copy of the `no_threads` storage (plain statics).
//!
//! * The **`guard::enable`** entry point. `thread::current` registers thread
//!   cleanup through `sys::thread_local::guard::enable`; the `dotnet` arm of that
//!   cascade re-exports the [`enable`] below, which (like the wasm/zkvm guards)
//!   is a leak-everything no-op because a single never-exiting thread has no
//!   "thread exit" event to hang destructors on.
//!
//! * The **key** layer (`key::{LazyKey, Key, get, set, create, destroy}`) is a
//!   small fixed-size global table of `*mut u8` slots — correct because there is
//!   exactly one thread. It is currently UNWIRED: for `os = dotnet` the storage
//!   layer re-exports the plain-static `no_threads`-style storage directly (not
//!   `os.rs`), and nothing in std imports `sys::thread_local::key` for this
//!   target, so the key cascade keeps its empty `_` arm. The module is kept
//!   ready for when real .NET threading lands (a `[ThreadStatic]` key table).
//!
//! No `extern`/BCL hooks are needed here: TLS is pure in-process state. The
//! load-bearing `extern "C"` PAL bindings (`rcl_dotnet_alloc`/`_free`/`_write`)
//! live in the `alloc` and `stdio` arms.

#![forbid(unsafe_op_in_unsafe_fn)]

use crate::cell::{Cell, UnsafeCell};
use crate::mem::MaybeUninit;
use crate::ptr;

// ---------------------------------------------------------------------------
// Storage layer (mirrors `sys::thread_local::no_threads`).
// ---------------------------------------------------------------------------

#[doc(hidden)]
#[allow_internal_unstable(thread_local_internals)]
#[allow_internal_unsafe]
#[unstable(feature = "thread_local_internals", issue = "none")]
#[rustc_macro_transparency = "semiopaque"]
pub macro thread_local_inner {
    // used to generate the `LocalKey` value for const-initialized thread locals
    (@key $t:ty, $(#[$align_attr:meta])*, const $init:expr) => {{
        const __RUST_STD_INTERNAL_INIT: $t = $init;

        // NOTE: Please update the shadowing test in `tests/thread.rs` if these types are renamed.
        unsafe {
            $crate::thread::LocalKey::new(|_| {
                $(#[$align_attr])*
                static __RUST_STD_INTERNAL_VAL: $crate::thread::local_impl::EagerStorage<$t> =
                    $crate::thread::local_impl::EagerStorage { value: __RUST_STD_INTERNAL_INIT };
                &__RUST_STD_INTERNAL_VAL.value
            })
        }
    }},

    // used to generate the `LocalKey` value for `thread_local!`
    (@key $t:ty, $(#[$align_attr:meta])*, $init:expr) => {{
        #[inline]
        fn __rust_std_internal_init_fn() -> $t { $init }

        unsafe {
            $crate::thread::LocalKey::new(|__rust_std_internal_init| {
                $(#[$align_attr])*
                static __RUST_STD_INTERNAL_VAL: $crate::thread::local_impl::LazyStorage<$t> = $crate::thread::local_impl::LazyStorage::new();
                __RUST_STD_INTERNAL_VAL.get(__rust_std_internal_init, __rust_std_internal_init_fn)
            })
        }
    }},
}

#[allow(missing_debug_implementations)]
#[repr(transparent)] // Required for correctness of `#[rustc_align_static]`
pub struct EagerStorage<T> {
    pub value: T,
}

// SAFETY: the .NET PAL is currently single-threaded.
unsafe impl<T> Sync for EagerStorage<T> {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    Alive,
    Destroying,
}

#[allow(missing_debug_implementations)]
#[repr(C)]
pub struct LazyStorage<T> {
    // This field must be first, for correctness of `#[rustc_align_static]`
    value: UnsafeCell<MaybeUninit<T>>,
    state: Cell<State>,
}

impl<T> LazyStorage<T> {
    pub const fn new() -> LazyStorage<T> {
        LazyStorage {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            state: Cell::new(State::Initial),
        }
    }

    /// Gets a pointer to the TLS value, potentially initializing it with the
    /// provided parameters.
    ///
    /// The resulting pointer may not be used after reentrant initialization
    /// has occurred.
    #[inline]
    pub fn get(&'static self, i: Option<&mut Option<T>>, f: impl FnOnce() -> T) -> *const T {
        if self.state.get() == State::Alive {
            self.value.get() as *const T
        } else {
            self.initialize(i, f)
        }
    }

    #[cold]
    fn initialize(&'static self, i: Option<&mut Option<T>>, f: impl FnOnce() -> T) -> *const T {
        let value = i.and_then(Option::take).unwrap_or_else(f);

        // Destroy the old value if it is initialized.
        // FIXME(#110897): maybe panic on recursive initialization.
        if self.state.get() == State::Alive {
            self.state.set(State::Destroying);
            // SAFETY: we check for no initialization during drop below.
            unsafe {
                ptr::drop_in_place(self.value.get() as *mut T);
            }
            self.state.set(State::Initial);
        }

        // Guard against initialization during drop.
        if self.state.get() == State::Destroying {
            panic!("Attempted to initialize thread-local while it is being dropped");
        }

        // SAFETY: we have ensured the slot is not alive and not being dropped.
        unsafe {
            self.value.get().write(MaybeUninit::new(value));
        }
        self.state.set(State::Alive);

        self.value.get() as *const T
    }
}

// SAFETY: the .NET PAL is currently single-threaded.
unsafe impl<T> Sync for LazyStorage<T> {}

#[rustc_macro_transparency = "semiopaque"]
pub(crate) macro local_pointer {
    () => {},
    ($vis:vis static $name:ident; $($rest:tt)*) => {
        $vis static $name: $crate::sys::thread_local::LocalPointer = $crate::sys::thread_local::LocalPointer::__new();
        $crate::sys::thread_local::local_pointer! { $($rest)* }
    },
}

pub(crate) struct LocalPointer {
    p: Cell<*mut ()>,
}

impl LocalPointer {
    pub const fn __new() -> LocalPointer {
        LocalPointer { p: Cell::new(ptr::null_mut()) }
    }

    pub fn get(&self) -> *mut () {
        self.p.get()
    }

    pub fn set(&self, p: *mut ()) {
        self.p.set(p)
    }
}

// SAFETY: the .NET PAL is currently single-threaded.
unsafe impl Sync for LocalPointer {}

// ---------------------------------------------------------------------------
// Key layer (the `sys::thread_local::key` contract: LazyKey, Key, get/set,
// create/destroy). Single-threaded: a fixed global table of pointer slots.
//
// This mirrors the API the `os.rs` storage and the `guard` module import via
// `super::key::{Key, LazyKey, get, set}`. It is provided so the key-based TLS
// paths resolve for `os = dotnet`; the macro above uses the cheaper plain-static
// storage, so in practice only `guard::enable` (below) exercises these keys.
// ---------------------------------------------------------------------------

pub mod key {
    use crate::cell::Cell;
    use crate::ptr;
    use crate::sync::atomic::{Atomic, AtomicUsize, Ordering};

    /// A TLS key. `0` is reserved as the "unallocated" sentinel (matching the
    /// `racy` `LazyKey` convention), so real keys are `1..=MAX_KEYS`.
    pub type Key = usize;

    type Dtor = unsafe extern "C" fn(*mut u8);

    /// Maximum number of live TLS keys. `std` itself uses only a handful; a
    /// small fixed table avoids pulling the allocator into TLS setup (which the
    /// `os` path is careful to avoid, since the global allocator may itself use
    /// TLS).
    const MAX_KEYS: usize = 256;

    struct Slot {
        // The stored thread-local pointer for this (single) thread.
        value: Cell<*mut u8>,
        // Destructor to run for this key, if any. Kept for API parity with the
        // key-based runtimes; in this single-threaded PAL it is only invoked
        // through explicit cleanup, never on a separate thread's exit.
        dtor: Cell<Option<Dtor>>,
        used: Cell<bool>,
    }

    impl Slot {
        const fn new() -> Slot {
            Slot {
                value: Cell::new(ptr::null_mut()),
                dtor: Cell::new(None),
                used: Cell::new(false),
            }
        }
    }

    // SAFETY: the .NET PAL is currently single-threaded, so this global table is
    // only ever touched from one thread.
    struct KeyTable {
        slots: [Slot; MAX_KEYS],
    }

    unsafe impl Sync for KeyTable {}

    impl KeyTable {
        const fn new() -> KeyTable {
            KeyTable { slots: [const { Slot::new() }; MAX_KEYS] }
        }
    }

    static KEYS: KeyTable = KeyTable::new();
    /// Next free index (1-based; `0` is the sentinel). Atomic only to keep the
    /// type honest; there is no real contention on a single thread.
    static NEXT: Atomic<usize> = AtomicUsize::new(1);

    /// A `LazyKey` lazily allocates a `Key` on first use.
    ///
    /// This is the same shape as `sys::thread_local::key::racy::LazyKey`: a
    /// const-constructible holder that resolves to a `Key` via [`force`]. We can
    /// allocate keys eagerly here (no OS round-trip), so the logic is simpler
    /// than the racy CAS dance.
    ///
    /// [`force`]: LazyKey::force
    pub struct LazyKey {
        key: Atomic<usize>,
        dtor: Option<Dtor>,
    }

    impl LazyKey {
        pub const fn new(dtor: Option<Dtor>) -> LazyKey {
            LazyKey { key: AtomicUsize::new(0), dtor }
        }

        /// Returns the `Key` for this `LazyKey`, allocating it on first call.
        #[inline]
        pub fn force(&self) -> Key {
            match self.key.load(Ordering::Acquire) {
                0 => {
                    let key = create(self.dtor);
                    self.key.store(key, Ordering::Release);
                    key
                }
                n => n,
            }
        }
    }

    // SAFETY: single-threaded PAL.
    unsafe impl Sync for LazyKey {}

    /// Allocate a fresh TLS key. Aborts if the fixed table is exhausted.
    pub fn create(dtor: Option<Dtor>) -> Key {
        let key = NEXT.fetch_add(1, Ordering::Relaxed);
        if key > MAX_KEYS {
            rtabort!("dotnet PAL: out of thread-local keys");
        }
        let slot = &KEYS.slots[key - 1];
        slot.dtor.set(dtor);
        slot.value.set(ptr::null_mut());
        slot.used.set(true);
        key
    }

    /// Set the current thread's value for `key`.
    ///
    /// # Safety
    /// `key` must be a key returned by [`create`] / [`LazyKey::force`].
    #[inline]
    pub unsafe fn set(key: Key, value: *mut u8) {
        debug_assert!(key >= 1 && key <= MAX_KEYS, "invalid TLS key");
        KEYS.slots[key - 1].value.set(value);
    }

    /// Get the current thread's value for `key` (null if unset).
    ///
    /// # Safety
    /// `key` must be a key returned by [`create`] / [`LazyKey::force`].
    #[inline]
    pub unsafe fn get(key: Key) -> *mut u8 {
        debug_assert!(key >= 1 && key <= MAX_KEYS, "invalid TLS key");
        KEYS.slots[key - 1].value.get()
    }

    /// Release a TLS key.
    ///
    /// # Safety
    /// `key` must be a key returned by [`create`]; it must not be used again.
    pub unsafe fn destroy(key: Key) {
        debug_assert!(key >= 1 && key <= MAX_KEYS, "invalid TLS key");
        let slot = &KEYS.slots[key - 1];
        slot.value.set(ptr::null_mut());
        slot.dtor.set(None);
        slot.used.set(false);
    }
}

// ---------------------------------------------------------------------------
// guard::enable equivalent.
//
// The `sys::thread_local::guard` `_` arm expects `key::{LazyKey, set}` and an
// `enable()`; provide a single-threaded no-op-shaped one. Because this PAL has
// no notion of "thread exit", destructors are leaked (the same trade-off the
// wasm/zkvm guards make explicitly). `thread_cleanup` is referenced so the
// linkage matches the key-based guards.
// ---------------------------------------------------------------------------

pub fn enable() {
    // FIXME: once the .NET PAL grows real threads, this should register a
    // thread-exit callback that walks the key table running destructors and then
    // calls `crate::rt::thread_cleanup`. With one thread there is no exit event,
    // so — exactly like the wasm/zkvm guards — we leak and rely on process
    // teardown.
    #[allow(unused_imports)]
    use crate::rt::thread_cleanup;
}
