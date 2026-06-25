// parking_lot rides std `Mutex` + `Parker` (its generic ThreadParker fallback).
// Exercises a contended Mutex and an RwLock across spawned threads.
use parking_lot::{Mutex, RwLock};
use std::sync::Arc;
use std::thread;

fn main() {
    // --- Contended Mutex: 4 threads x 25_000 increments = 100_000. ---
    let counter = Arc::new(Mutex::new(0u64));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..25_000 {
                let mut g = c.lock();
                *g += 1;
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("mutex_total = {}", *counter.lock());

    // --- RwLock: many readers + a few writers, final value deterministic. ---
    let data = Arc::new(RwLock::new(0u64));
    let mut hs = Vec::new();
    for _ in 0..4 {
        let d = Arc::clone(&data);
        hs.push(thread::spawn(move || {
            for _ in 0..10_000 {
                let mut w = d.write();
                *w += 1;
            }
        }));
    }
    // Reader threads that just observe (must not corrupt the count).
    for _ in 0..2 {
        let d = Arc::clone(&data);
        hs.push(thread::spawn(move || {
            let mut last = 0u64;
            for _ in 0..5_000 {
                let r = d.read();
                // Monotonic non-decreasing observation.
                assert!(*r >= last);
                last = *r;
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    println!("rwlock_total = {}", *data.read());

    println!("== probe_parking_lot done ==");
}
