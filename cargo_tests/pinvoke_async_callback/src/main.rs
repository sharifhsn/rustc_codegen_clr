use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pinvoke_async_callback::{Registration, copy_utf16, live_workers};

struct Dropped(Arc<AtomicBool>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn wait_until(mut condition: impl FnMut() -> bool, description: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn retry_scenario() {
    let registered = Arc::new(AtomicBool::new(false));
    let callback_before_return = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let callback_registered = Arc::clone(&registered);
    let callback_before_return_flag = Arc::clone(&callback_before_return);
    let callback_calls = Arc::clone(&calls);
    let drop_sentinel = Dropped(Arc::clone(&dropped));
    let registration = Registration::start(
        move |_| {
            let _keep_alive = &drop_sentinel;
            if !callback_registered.load(Ordering::Acquire) {
                callback_before_return_flag.store(true, Ordering::Release);
            }
            callback_calls.fetch_add(1, Ordering::Relaxed);
            0
        },
        true,
    )
    .expect("native callback registration failed");
    registered.store(true, Ordering::Release);
    wait_until(
        || calls.load(Ordering::Relaxed) > 0,
        "asynchronous callback",
    );
    assert!(!callback_before_return.load(Ordering::Acquire));

    let failure = registration
        .stop()
        .expect_err("first unregister should be busy");
    assert_eq!(failure.error().code(), 1);
    assert!(!dropped.load(Ordering::Acquire));
    failure
        .into_registration()
        .stop()
        .expect("unregister retry should join the worker");
    assert!(dropped.load(Ordering::Acquire));
    assert_eq!(live_workers(), 0);
    let stopped_at = calls.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(calls.load(Ordering::Relaxed), stopped_at);
    println!("Async callback retry/quiescence OK: calls={stopped_at}");
}

fn drop_scenario() {
    let dropped = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::clone(&calls);
    let drop_sentinel = Dropped(Arc::clone(&dropped));
    let registration = Registration::start(
        move |_| {
            let _keep_alive = &drop_sentinel;
            callback_calls.fetch_add(1, Ordering::Relaxed);
            0
        },
        false,
    )
    .expect("drop-path callback registration failed");
    wait_until(|| calls.load(Ordering::Relaxed) > 0, "drop-path callback");
    drop(registration);
    assert!(dropped.load(Ordering::Acquire));
    assert_eq!(live_workers(), 0);
    println!("Async callback Drop/unregister OK");
}

fn panic_scenario() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::clone(&calls);
    let registration = Registration::start(
        move |_| {
            callback_calls.fetch_add(1, Ordering::Relaxed);
            panic!("callback panic must become native status 77");
        },
        false,
    )
    .expect("panic callback registration failed");
    wait_until(
        || calls.load(Ordering::Relaxed) > 0,
        "panic-contained callback",
    );
    registration
        .stop()
        .expect("panic callback worker did not join");
    std::panic::set_hook(previous_hook);
    assert_eq!(live_workers(), 0);
    println!("Async callback panic containment OK");
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("all") => {
            let text = "managed Rust ⇄ native UTF-16: λ世界";
            assert_eq!(copy_utf16(text).unwrap(), text);
            println!("Native owned UTF-16 round-trip OK");
            retry_scenario();
            drop_scenario();
            panic_scenario();
            println!("Async retained P/Invoke callback acceptance OK");
        }
        Some("retry") => retry_scenario(),
        Some("drop") => drop_scenario(),
        Some("panic") => panic_scenario(),
        scenario => panic!("expected all, retry, drop, or panic scenario; got {scenario:?}"),
    }
}
