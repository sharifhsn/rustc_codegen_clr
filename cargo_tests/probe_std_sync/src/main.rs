// Exercises the std GENERIC sync primitives that now ride the dotnet Parker:
//   * Once / OnceLock  -> sys::sync::once::queue (pure Parker + atomics)
//   * RwLock           -> sys::sync::rwlock::queue (pure Parker + atomics)
//   * Condvar          -> the dotnet SemaphoreSlim-counter arm
// All contended across spawned threads to actually drive park/unpark.
use std::sync::{Arc, Condvar, Mutex, Once, OnceLock, RwLock};
use std::thread;

static INIT: Once = Once::new();
static CELL: OnceLock<u64> = OnceLock::new();

fn main() {
    // --- Once: many threads race call_once; the body runs EXACTLY once. ---
    let counter = Arc::new(Mutex::new(0u64));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let c = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            INIT.call_once(|| {
                *c.lock().unwrap() += 1;
            });
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("once_body_runs = {}", *counter.lock().unwrap());

    // --- OnceLock: concurrent get_or_init returns the SAME first value. ---
    let mut hs = Vec::new();
    for i in 0..8u64 {
        hs.push(thread::spawn(move || *CELL.get_or_init(|| 40 + i % 1)));
    }
    let mut all_same = true;
    let first = CELL.get_or_init(|| 99);
    for h in hs {
        if h.join().unwrap() != *first {
            all_same = false;
        }
    }
    println!("oncelock_value = {}", first);
    println!("oncelock_all_same = {}", all_same);

    // --- RwLock: contended writers, deterministic final count. ---
    let data = Arc::new(RwLock::new(0u64));
    let mut rs = Vec::new();
    for _ in 0..4 {
        let d = Arc::clone(&data);
        rs.push(thread::spawn(move || {
            for _ in 0..10_000 {
                *d.write().unwrap() += 1;
            }
        }));
    }
    for h in rs {
        h.join().unwrap();
    }
    println!("rwlock_total = {}", *data.read().unwrap());

    // --- Condvar: a worker waits until the main thread signals readiness. ---
    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    let pair2 = Arc::clone(&pair);
    let worker = thread::spawn(move || {
        let (lock, cvar) = &*pair2;
        let mut ready = lock.lock().unwrap();
        while !*ready {
            ready = cvar.wait(ready).unwrap();
        }
        true
    });
    {
        let (lock, cvar) = &*pair;
        let mut ready = lock.lock().unwrap();
        *ready = true;
        cvar.notify_one();
    }
    let woke = worker.join().unwrap();
    println!("condvar_woke = {}", woke);

    println!("== probe_std_sync done ==");
}
