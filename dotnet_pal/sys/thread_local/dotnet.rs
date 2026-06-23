//! Thread-local storage for the .NET ("dotnet") platform — REAL per-thread TLS.
//!
//! Slice 2 of the threading lift. Previously this file was a verbatim copy of the
//! `no_threads` PROCESS-GLOBAL storage (correct only for a single thread). Because
//! the storage was global, the SECOND concurrently-spawned thread aborted in
//! `std::thread::set_current` ("current thread handle already set during thread
//! spawn") — `set_current` uses a `thread_local`, and a global `thread_local` is
//! shared across threads, so the main thread's `CURRENT` looked "already set" to
//! the child. TLS-per-thread was therefore the blocker for ALL concurrency.
//!
//! This implementation is **structurally identical to std's `os.rs`** TLS backend
//! (the library-based "TLS key" path that POSIX targets use), but the underlying
//! key primitive is a managed `System.Threading.ThreadLocal<IntPtr>` instead of a
//! `pthread_key_t`. A `ThreadLocal<T>.Value` is per-thread BY CONSTRUCTION, so we
//! need no `ManagedThreadId` composite key: one `ThreadLocal<IntPtr>` per TLS key,
//! its `.Value` being that key's slot for the calling thread.
//!
//! Surfaces (matching what `sys::thread_local::mod.rs`'s `_`/`os` arm exports, so
//! the dotnet arm can re-export these in place of the old `no_threads`-style
//! `EagerStorage`/`LazyStorage`):
//!
//! * The **storage** layer: `Storage<T, ALIGN>`, `value_align`, and
//!   `thread_local_inner` — a verbatim copy of `os.rs`, heap-boxing each TLS value
//!   (via the System allocator, to avoid recursing through a TLS-using global
//!   allocator) and storing the box pointer under a per-thread key.
//! * `LocalPointer` / `local_pointer!` — the per-thread pointer cell that
//!   `thread::set_current`/`current` rides on (this is the load-bearing one).
//! * The **`key`** layer (`Key`, `LazyKey`, `get`, `set`, `create`, `destroy`) —
//!   the per-thread key table, each key a `GCHandle`-pinned `ThreadLocal<IntPtr>`
//!   reached through the BCL hooks `rcl_dotnet_tls_{create,get,set}` (see
//!   `cilly/src/ir/builtins/dotnet.rs`). This is what makes storage per-thread.
//! * **`guard::enable`** — a leak-on-exit no-op. Real `pthread_key_create`
//!   destructors run at thread exit; this slice has no thread-exit destructor
//!   hook, so TLS values are leaked when a thread ends (acceptable: the same
//!   trade-off the wasm/zkvm guards make, and no test relies on TLS-drop). Because
//!   the destructor never runs, the `os.rs` "destructor running" sentinel (`1`) is
//!   never written, but the storage logic handles it correctly regardless.
//!
//! The BCL hooks are the only `extern`/PAL surface; everything else is the std
//! `os.rs` storage logic unchanged.

#![forbid(unsafe_op_in_unsafe_fn)]

use crate::alloc::{self, GlobalAlloc, Layout, System};
use crate::cell::Cell;
use crate::marker::PhantomData;
use crate::mem::ManuallyDrop;
use crate::ops::Deref;
use crate::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use crate::ptr::{self, NonNull};

use self::key::{Key, LazyKey, get, set};

// ---------------------------------------------------------------------------
// Storage layer (a verbatim copy of `sys::thread_local::os`, backed by the
// per-thread `key` module below).
// ---------------------------------------------------------------------------

#[doc(hidden)]
#[allow_internal_unstable(thread_local_internals)]
#[allow_internal_unsafe]
#[unstable(feature = "thread_local_internals", issue = "none")]
#[rustc_macro_transparency = "semiopaque"]
pub macro thread_local_inner {
    // NOTE: we cannot import `Storage` or `LocalKey` with a `use` because that can shadow user
    // provided type or type alias with a matching name. Please update the shadowing test in
    // `tests/thread.rs` if these types are renamed.

    // used to generate the `LocalKey` value for `thread_local!`.
    (@key $t:ty, $($(#[$($align_attr:tt)*])+)?, $init:expr) => {{
        #[inline]
        fn __rust_std_internal_init_fn() -> $t { $init }

        // NOTE: this cannot import `LocalKey` or `Storage` with a `use` because that can shadow
        // user provided type or type alias with a matching name. Please update the shadowing test
        // in `tests/thread.rs` if these types are renamed.
        unsafe {
            $crate::thread::LocalKey::new(|__rust_std_internal_init| {
                static __RUST_STD_INTERNAL_VAL: $crate::thread::local_impl::Storage<$t, {
                    $({
                        // Ensure that attributes have valid syntax
                        // and that the proper feature gate is enabled
                        $(#[$($align_attr)*])+
                        #[allow(unused)]
                        static DUMMY: () = ();
                    })?

                    #[allow(unused_mut)]
                    let mut final_align = $crate::thread::local_impl::value_align::<$t>();
                    $($($crate::thread::local_impl::thread_local_inner!(@align final_align, $($align_attr)*);)+)?
                    final_align
                }>
                    = $crate::thread::local_impl::Storage::new();
                __RUST_STD_INTERNAL_VAL.get(__rust_std_internal_init, __rust_std_internal_init_fn)
            })
        }
    }},

    // process a single `rustc_align_static` attribute
    (@align $final_align:ident, rustc_align_static($($align:tt)*) $(, $($attr_rest:tt)+)?) => {
        let new_align: $crate::primitive::usize = $($align)*;
        if new_align > $final_align {
            $final_align = new_align;
        }

        $($crate::thread::local_impl::thread_local_inner!(@align $final_align, $($attr_rest)+);)?
    },

    // process a single `cfg_attr` attribute
    // by translating it into a `cfg`ed block and recursing.
    // https://doc.rust-lang.org/reference/conditional-compilation.html#railroad-ConfigurationPredicate

    (@align $final_align:ident, cfg_attr($cfg_pred:expr, $($cfg_rhs:tt)*) $(, $($attr_rest:tt)+)?) => {
        #[cfg($cfg_pred)]
        {
            $crate::thread::local_impl::thread_local_inner!(@align $final_align, $($cfg_rhs)*);
        }

        $($crate::thread::local_impl::thread_local_inner!(@align $final_align, $($attr_rest)+);)?
    },
}

/// Use a regular global static to store this key; the state provided will then be
/// thread-local.
/// INVARIANT: ALIGN must be a valid alignment, and no less than `value_align::<T>`.
#[allow(missing_debug_implementations)]
pub struct Storage<T, const ALIGN: usize> {
    key: LazyKey,
    marker: PhantomData<Cell<T>>,
}

unsafe impl<T, const ALIGN: usize> Sync for Storage<T, ALIGN> {}

#[repr(C)]
struct Value<T: 'static> {
    // This field must be first, for correctness of `#[rustc_align_static]`
    value: T,
    // INVARIANT: if this value is stored under a TLS key, `key` must be that `key`.
    key: Key,
}

pub const fn value_align<T: 'static>() -> usize {
    crate::mem::align_of::<Value<T>>()
}

/// Equivalent to `Box<Value<T>, System>`, but potentially over-aligned.
struct AlignedSystemBox<T: 'static, const ALIGN: usize> {
    ptr: NonNull<Value<T>>,
}

impl<T: 'static, const ALIGN: usize> AlignedSystemBox<T, ALIGN> {
    #[inline]
    fn new(v: Value<T>) -> Self {
        let layout = Layout::new::<Value<T>>().align_to(ALIGN).unwrap();

        // We use the System allocator here to avoid interfering with a potential
        // Global allocator using thread-local storage.
        let ptr: *mut Value<T> = (unsafe { System.alloc(layout) }).cast();
        let Some(ptr) = NonNull::new(ptr) else {
            alloc::handle_alloc_error(layout);
        };
        unsafe { ptr.write(v) };
        Self { ptr }
    }

    #[inline]
    fn into_raw(b: Self) -> *mut Value<T> {
        let md = ManuallyDrop::new(b);
        md.ptr.as_ptr()
    }

    #[inline]
    unsafe fn from_raw(ptr: *mut Value<T>) -> Self {
        Self { ptr: unsafe { NonNull::new_unchecked(ptr) } }
    }
}

impl<T: 'static, const ALIGN: usize> Deref for AlignedSystemBox<T, ALIGN> {
    type Target = Value<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.ptr.as_ptr()) }
    }
}

impl<T: 'static, const ALIGN: usize> Drop for AlignedSystemBox<T, ALIGN> {
    #[inline]
    fn drop(&mut self) {
        let layout = Layout::new::<Value<T>>().align_to(ALIGN).unwrap();

        unsafe {
            let unwind_result = catch_unwind(AssertUnwindSafe(|| self.ptr.drop_in_place()));
            System.dealloc(self.ptr.as_ptr().cast(), layout);
            if let Err(payload) = unwind_result {
                resume_unwind(payload);
            }
        }
    }
}

impl<T: 'static, const ALIGN: usize> Storage<T, ALIGN> {
    pub const fn new() -> Storage<T, ALIGN> {
        Storage { key: LazyKey::new(Some(destroy_value::<T, ALIGN>)), marker: PhantomData }
    }

    /// Gets a pointer to the TLS value, potentially initializing it with the
    /// provided parameters. If the TLS variable has been destroyed, a null
    /// pointer is returned.
    ///
    /// The resulting pointer may not be used after reentrant inialialization
    /// or thread destruction has occurred.
    #[inline]
    pub fn get(&'static self, i: Option<&mut Option<T>>, f: impl FnOnce() -> T) -> *const T {
        let key = self.key.force();
        let ptr = unsafe { get(key) as *mut Value<T> };
        if ptr.addr() > 1 {
            // SAFETY: the check ensured the pointer is safe (its destructor
            // is not running) + it is coming from a trusted source (self).
            unsafe { &(*ptr).value }
        } else {
            // SAFETY: trivially correct.
            unsafe { Self::try_initialize(key, ptr, i, f) }
        }
    }

    /// # Safety
    /// * `key` must be the result of calling `self.key.force()`
    /// * `ptr` must be the current value associated with `key`.
    #[cold]
    unsafe fn try_initialize(
        key: Key,
        ptr: *mut Value<T>,
        i: Option<&mut Option<T>>,
        f: impl FnOnce() -> T,
    ) -> *const T {
        if ptr.addr() == 1 {
            // destructor is running
            return ptr::null();
        }

        let value = AlignedSystemBox::<T, ALIGN>::new(Value {
            value: i.and_then(Option::take).unwrap_or_else(f),
            key,
        });
        let ptr = AlignedSystemBox::into_raw(value);

        // SAFETY:
        // * key came from a `LazyKey` and is thus correct.
        // * `ptr` is a correct pointer that can be destroyed by the key destructor.
        // * the value is stored under the key that it contains.
        let old = unsafe {
            let old = get(key) as *mut Value<T>;
            set(key, ptr as *mut u8);
            old
        };

        if !old.is_null() {
            // If the variable was recursively initialized, drop the old value.
            // SAFETY: We cannot be inside a `LocalKey::with` scope, as the
            // initializer has already returned and the next scope only starts
            // after we return the pointer. Therefore, there can be no references
            // to the old value.
            drop(unsafe { AlignedSystemBox::<T, ALIGN>::from_raw(old) });
        }

        // SAFETY: We just created this value above.
        unsafe { &(*ptr).value }
    }
}

unsafe extern "C" fn destroy_value<T: 'static, const ALIGN: usize>(ptr: *mut u8) {
    // SAFETY:
    //
    // The OS TLS ensures that this key contains a null value when this
    // destructor starts to run. We set it back to a sentinel value of 1 to
    // ensure that any future calls to `get` for this thread will return
    // `None`.
    //
    // Note that to prevent an infinite loop we reset it back to null right
    // before we return from the destructor ourselves.
    //
    // NOTE (dotnet): with the current leak-on-exit `guard::enable` this is never
    // actually invoked — there is no thread-exit hook to register it on — but it
    // is kept verbatim from `os.rs` so the `LazyKey` destructor slot is honest and
    // the storage logic is identical to the proven upstream path.
    abort_on_dtor_unwind(|| {
        let ptr = unsafe { AlignedSystemBox::<T, ALIGN>::from_raw(ptr as *mut Value<T>) };
        let key = ptr.key;
        // SAFETY: `key` is the TLS key `ptr` was stored under.
        unsafe { set(key, ptr::without_provenance_mut(1)) };
        drop(ptr);
        // SAFETY: `key` is the TLS key `ptr` was stored under.
        unsafe { set(key, ptr::null_mut()) };
        // Make sure that the runtime cleanup will be performed
        // after the next round of TLS destruction. `enable` is this module's own
        // (the guard arm re-exports it), so call it directly rather than through
        // the `super::guard` cascade.
        enable();
    });
}

#[rustc_macro_transparency = "semiopaque"]
pub(crate) macro local_pointer {
    () => {},
    ($vis:vis static $name:ident; $($rest:tt)*) => {
        $vis static $name: $crate::sys::thread_local::LocalPointer = $crate::sys::thread_local::LocalPointer::__new();
        $crate::sys::thread_local::local_pointer! { $($rest)* }
    },
}

pub(crate) struct LocalPointer {
    key: LazyKey,
}

impl LocalPointer {
    pub const fn __new() -> LocalPointer {
        LocalPointer { key: LazyKey::new(None) }
    }

    pub fn get(&'static self) -> *mut () {
        unsafe { get(self.key.force()) as *mut () }
    }

    pub fn set(&'static self, p: *mut ()) {
        unsafe { set(self.key.force(), p as *mut u8) }
    }
}

// ---------------------------------------------------------------------------
// `abort_on_dtor_unwind` — copied from `sys::thread_local::mod.rs` (it is a
// private free fn there, not re-exported, so the storage logic above needs its
// own copy). Identical behaviour.
// ---------------------------------------------------------------------------

#[inline]
#[allow(dead_code)]
fn abort_on_dtor_unwind(f: impl FnOnce()) {
    // Using a guard like this is lower cost.
    let guard = DtorUnwindGuard;
    f();
    core::mem::forget(guard);

    struct DtorUnwindGuard;
    impl Drop for DtorUnwindGuard {
        #[inline]
        fn drop(&mut self) {
            // This is not terribly descriptive, but it doesn't need to be as we'll
            // already have printed a panic message at this point.
            rtabort!("thread local panicked on drop");
        }
    }
}

// ---------------------------------------------------------------------------
// Key layer — REAL per-thread TLS keys, each a `GCHandle`-pinned managed
// `System.Threading.ThreadLocal<IntPtr>` reached through the BCL hooks. This is
// what `os.rs`'s storage imports via `super::key::{Key, LazyKey, get, set}`, and
// what the `sys::thread_local::key` `dotnet` arm re-exports for any std code that
// uses the key path directly.
// ---------------------------------------------------------------------------

pub mod key {
    use crate::sync::atomic::{Atomic, AtomicUsize, Ordering};

    unsafe extern "C" {
        /// `new ThreadLocal<nint>()`, `GCHandle`-pinned; returns the handle as the
        /// opaque per-thread TLS key.
        fn rcl_dotnet_tls_create() -> *mut u8;
        /// `((ThreadLocal<nint>)key).Value` — the CALLING thread's slot.
        fn rcl_dotnet_tls_get(key: *mut u8) -> *mut u8;
        /// `((ThreadLocal<nint>)key).Value = (nint)val` — the CALLING thread's slot.
        fn rcl_dotnet_tls_set(key: *mut u8, val: *mut u8);
    }

    /// A TLS key — the opaque `GCHandle` `IntPtr` (as `*mut u8`) of a managed
    /// `ThreadLocal<IntPtr>`. `GCHandle.Alloc` never yields null, so a created key
    /// is always non-null (the `racy`-style `LazyKey` below relies on `0` being a
    /// usable "unallocated" sentinel).
    pub type Key = *mut u8;

    type Dtor = unsafe extern "C" fn(*mut u8);

    /// `0` (null) is the "unallocated" sentinel for the `LazyKey` CAS.
    const KEY_SENTVAL: usize = 0;

    /// Allocate a fresh per-thread TLS key (a managed `ThreadLocal<IntPtr>`).
    ///
    /// The destructor is accepted for API parity with the OS key path but is
    /// NEVER stored/run in this slice (see `guard::enable` — leak on thread exit).
    #[inline]
    pub fn create(_dtor: Option<Dtor>) -> Key {
        // SAFETY: a pure BCL constructor call; always returns a fresh handle.
        unsafe { rcl_dotnet_tls_create() }
    }

    /// Set the CALLING thread's value for `key`.
    ///
    /// # Safety
    /// `key` must be a key returned by [`create`] / [`LazyKey::force`].
    #[inline]
    pub unsafe fn set(key: Key, value: *mut u8) {
        // SAFETY: `key` is a live `ThreadLocal<IntPtr>` handle per the contract.
        unsafe { rcl_dotnet_tls_set(key, value) }
    }

    /// Get the CALLING thread's value for `key` (null if unset on this thread).
    ///
    /// # Safety
    /// `key` must be a key returned by [`create`] / [`LazyKey::force`].
    #[inline]
    pub unsafe fn get(key: Key) -> *mut u8 {
        // SAFETY: `key` is a live `ThreadLocal<IntPtr>` handle per the contract.
        unsafe { rcl_dotnet_tls_get(key) }
    }

    /// Release a TLS key. A no-op in this slice: there is no Free hook for the
    /// `GCHandle`/`ThreadLocal`, so the key (one per live `thread_local!`) leaks
    /// until process exit. `LazyKey::force` only calls this on a lost
    /// initialization race, which is rare and bounded.
    ///
    /// # Safety
    /// `key` must be a key returned by [`create`]; it must not be used again.
    #[inline]
    pub unsafe fn destroy(_key: Key) {}

    /// A `LazyKey` lazily allocates a `Key` on first use. Same shape and racy-CAS
    /// initialization as `sys::thread_local::key::racy::LazyKey`, specialised to
    /// our `*mut u8` key (stored as a `usize` for the atomic).
    pub struct LazyKey {
        /// Inner static TLS key, as a `usize` (the key pointer's address). `0` is
        /// the unallocated sentinel.
        key: Atomic<usize>,
        dtor: Option<Dtor>,
    }

    impl LazyKey {
        pub const fn new(dtor: Option<Dtor>) -> LazyKey {
            LazyKey { key: AtomicUsize::new(KEY_SENTVAL), dtor }
        }

        #[inline]
        pub fn force(&self) -> Key {
            match self.key.load(Ordering::Acquire) {
                KEY_SENTVAL => self.lazy_init(),
                n => crate::ptr::without_provenance_mut(n),
            }
        }

        #[cold]
        fn lazy_init(&self) -> Key {
            // `create` returns a GCHandle, which is never null, so it can never
            // equal `KEY_SENTVAL` (unlike pthread keys, which can be 0); no
            // re-roll dance is needed.
            let key = create(self.dtor);
            let key_addr = key.addr();
            rtassert!(key_addr != KEY_SENTVAL);
            match self.key.compare_exchange(
                KEY_SENTVAL,
                key_addr,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                // The CAS succeeded, so we've created the actual key.
                Ok(_) => key,
                // Someone beat us to it: adopt their key, drop ours.
                Err(n) => {
                    // SAFETY: `key` is freshly created and used nowhere else.
                    unsafe { destroy(key) };
                    crate::ptr::without_provenance_mut(n)
                }
            }
        }
    }

    // SAFETY: the key is just an integer handle; the per-thread-ness lives in the
    // managed `ThreadLocal`, so this static holder is `Sync`.
    unsafe impl Sync for LazyKey {}
}

// ---------------------------------------------------------------------------
// guard::enable — leak-on-exit no-op (no thread-exit destructor hook yet).
// ---------------------------------------------------------------------------

pub fn enable() {
    // FIXME: once the .NET PAL grows a thread-exit callback, this should walk the
    // live keys running destructors and then call `crate::rt::thread_cleanup`.
    // For now — exactly like the wasm/zkvm guards — we leak TLS values on thread
    // exit and rely on process teardown. NOTE: this is a genuine no-op (it
    // introduces NO global state), so it does not reintroduce the global-storage
    // bug this slice fixes.
    #[allow(unused_imports)]
    use crate::rt::thread_cleanup;
}
