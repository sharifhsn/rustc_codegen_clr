//! Native test library that deliberately retains a callback and invokes it from a worker thread.

use std::ffi::{c_int, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

type Callback = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;

pub struct Registration {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    fail_first_unregister: bool,
    unregister_attempts: usize,
}

static LIVE_WORKERS: AtomicI32 = AtomicI32::new(0);

#[unsafe(no_mangle)]
/// Starts a worker that retains `callback` and `context` until `ac_unregister` joins it.
///
/// # Safety
///
/// `out_registration` must be writable. The callback and context must remain valid until a
/// successful `ac_unregister` returns.
pub unsafe extern "C" fn ac_register(
    callback: Option<Callback>,
    context: *mut c_void,
    fail_first_unregister: c_int,
    out_registration: *mut *mut Registration,
) -> c_int {
    let Some(callback) = callback else {
        return -1;
    };
    if out_registration.is_null() {
        return -1;
    }

    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let context = context as usize;
    let worker = std::thread::spawn(move || {
        LIVE_WORKERS.fetch_add(1, Ordering::AcqRel);
        // Make the retained nature deterministic: registration has returned before the first call.
        std::thread::sleep(Duration::from_millis(10));
        let mut value = 1;
        while !worker_stop.load(Ordering::Acquire) {
            let result = unsafe { callback(context as *mut c_void, value) };
            if result != 0 {
                break;
            }
            value += 1;
            std::thread::sleep(Duration::from_millis(1));
        }
        LIVE_WORKERS.fetch_sub(1, Ordering::AcqRel);
    });
    let registration = Box::new(Registration {
        stop,
        worker: Some(worker),
        fail_first_unregister: fail_first_unregister != 0,
        unregister_attempts: 0,
    });
    unsafe { out_registration.write(Box::into_raw(registration)) };
    0
}

#[unsafe(no_mangle)]
/// Stops and joins the registered worker.
///
/// # Safety
///
/// `registration` must be a live token returned by `ac_register` and must not be used after this
/// function succeeds.
pub unsafe extern "C" fn ac_unregister(registration: *mut Registration) -> c_int {
    let Some(registration) = (unsafe { registration.as_mut() }) else {
        return -1;
    };
    registration.unregister_attempts += 1;
    if registration.fail_first_unregister && registration.unregister_attempts == 1 {
        return 1;
    }

    registration.stop.store(true, Ordering::Release);
    if let Some(worker) = registration.worker.take()
        && worker.join().is_err()
    {
        return 2;
    }
    unsafe { drop(Box::from_raw(registration)) };
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn ac_live_workers() -> c_int {
    LIVE_WORKERS.load(Ordering::Acquire)
}

#[unsafe(no_mangle)]
/// Copies a borrowed NUL-terminated UTF-16 string into a native-owned allocation.
///
/// # Safety
///
/// `input` must be NUL-terminated and readable. `output` must be writable. A successful output
/// must be released exactly once with `ac_free_utf16`.
pub unsafe extern "C" fn ac_copy_utf16(input: *const u16, output: *mut *mut u16) -> c_int {
    if input.is_null() || output.is_null() {
        return -1;
    }
    let mut len = 0usize;
    while unsafe { input.add(len).read() } != 0 {
        len += 1;
    }
    let input = unsafe { std::slice::from_raw_parts(input, len) };
    let owned: Box<[u16]> = input
        .iter()
        .copied()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    unsafe { output.write(Box::into_raw(owned).cast::<u16>()) };
    0
}

#[unsafe(no_mangle)]
/// Releases a string returned by `ac_copy_utf16`.
///
/// # Safety
///
/// `value` must be a live pointer returned by `ac_copy_utf16` and not previously freed.
pub unsafe extern "C" fn ac_free_utf16(value: *mut u16) {
    if value.is_null() {
        return;
    }
    let mut len = 0usize;
    while unsafe { value.add(len).read() } != 0 {
        len += 1;
    }
    let allocation = core::ptr::slice_from_raw_parts_mut(value, len + 1);
    unsafe { drop(Box::from_raw(allocation)) };
}
