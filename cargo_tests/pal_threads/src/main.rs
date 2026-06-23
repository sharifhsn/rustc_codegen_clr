//! Combined threading probe for the dotnet PAL: Mutex mutual-exclusion (Slice 1)
//! + thread-local-storage isolation (Slice 2).
//!
//! Part A — Mutex shared counter. N=4 threads each lock a shared
//! `Arc<Mutex<u64>>` and do `*g += 1` K=100_000 times. `std::sync::Mutex` wraps
//! `sys::sync::Mutex` (our SemaphoreSlim-backed arm). A broken/no_threads Mutex
//! either PANICS under contention or silently no-ops (LOST UPDATES, final < N*K).
//! Only a REAL mutually-exclusive lock makes the final count exactly N*K.
//!
//! Part B — TLS isolation. Each spawned thread writes a UNIQUE value into a
//! `thread_local!` cell, then loops K2 times asserting the cell still reads back
//! its OWN value (proving no other thread clobbered it — which a process-global
//! TLS store would allow). The main thread plants its own sentinel before spawn
//! and re-checks it after join (proving the children never touched the main
//! thread's slot). We also assert `thread::current().id()` differs across all
//! threads.
//!
//! Why Part B matters: with the OLD process-global TLS, the SECOND concurrently
//! spawned thread aborted in `thread::set_current` ("current thread handle
//! already set during thread spawn"), because `set_current`'s `thread_local`
//! looked already-set (the main thread's value). So merely *spawning 2 threads*
//! exercises per-thread TLS; the explicit cell test then proves isolation.

use std::cell::Cell;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;

const N: u64 = 4;
const K: u64 = 100_000;
const K2: u64 = 50_000;

thread_local! {
    static TID: Cell<u64> = const { Cell::new(0) };
}

fn main() {
    // ---- main thread plants its TLS sentinel BEFORE spawning ----
    let main_sentinel: u64 = 0xDEAD_BEEF;
    TID.with(|c| c.set(main_sentinel));

    let counter = Arc::new(Mutex::new(0u64));
    // Collect each child's reported ThreadId (as u64) under the shared mutex.
    let ids = Arc::new(Mutex::new(Vec::<thread::ThreadId>::new()));

    let mut handles = Vec::new();
    for t in 0..N {
        let counter = Arc::clone(&counter);
        let ids = Arc::clone(&ids);
        // Each thread gets a unique, non-zero, non-sentinel TLS value.
        let my_val = 1000 + t;
        handles.push(thread::spawn(move || {
            // Part B: this thread's TLS slot must start at the const default (0),
            // NOT the main thread's planted sentinel — proves a fresh per-thread
            // slot rather than a shared global.
            let initial = TID.with(Cell::get);
            assert_eq!(initial, 0, "thread {t}: TLS not fresh (saw {initial:#x})");

            TID.with(|c| c.set(my_val));

            // Part A + B interleaved: bump the shared counter while repeatedly
            // re-reading our own TLS. If any other thread's `set` bled into our
            // slot (global storage), this assert trips.
            for _ in 0..K {
                let mut g = counter.lock().unwrap();
                *g += 1;
            }
            for _ in 0..K2 {
                let got = TID.with(Cell::get);
                assert_eq!(got, my_val, "thread {t}: TLS bled (got {got}, want {my_val})");
                thread::yield_now();
            }

            // Record this thread's identity for the cross-thread-distinct check.
            ids.lock().unwrap().push(thread::current().id());
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // ---- Part A assert: no lost updates ----
    let total = *counter.lock().unwrap();
    assert_eq!(total, N * K, "lost updates: got {total}, expected {}", N * K);

    // ---- Part B assert: main thread's sentinel survived untouched ----
    let main_after = TID.with(Cell::get);
    assert_eq!(
        main_after, main_sentinel,
        "main TLS clobbered: got {main_after:#x}, want {main_sentinel:#x}"
    );

    // ---- ThreadId distinctness across all N children + main ----
    let child_ids = ids.lock().unwrap();
    assert_eq!(child_ids.len() as u64, N, "expected {N} child ids");
    let mut set: HashSet<thread::ThreadId> = HashSet::new();
    for id in child_ids.iter() {
        assert!(set.insert(*id), "duplicate child ThreadId {id:?}");
    }
    let main_id = thread::current().id();
    assert!(!set.contains(&main_id), "a child shares the main ThreadId {main_id:?}");

    println!(
        "pal_threads OK (counter={total}, expected={}, distinct_ids={}, main_tls={main_after:#x})",
        N * K,
        child_ids.len()
    );
}
