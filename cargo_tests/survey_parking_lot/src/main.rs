use parking_lot::{Mutex, ReentrantMutex, RwLock};
use std::cell::Cell;

fn main() {
    // --- Mutex: lock, mutate a counter a fixed number of times, read back. ---
    // Single-threaded, deterministic: no real OS threads, no timing.
    let counter = Mutex::new(0u64);
    let iters: u64 = 1000;
    for _ in 0..iters {
        let mut guard = counter.lock();
        *guard += 1;
    }
    let mutex_final = *counter.lock();
    println!("mutex_iters = {}", iters);
    println!("mutex_final = {}", mutex_final);

    // try_lock while no contention -> Some; while a guard is held -> None.
    let try_when_free = counter.try_lock().is_some();
    println!("mutex_try_lock_when_free = {}", try_when_free);
    let held = counter.lock();
    let try_when_held = counter.try_lock().is_none();
    println!("mutex_try_lock_when_held = {}", try_when_held);
    drop(held);

    // --- RwLock: many readers (sequential), then a writer mutating a counter. ---
    let rw = RwLock::new(100u64);
    // Take several read guards in sequence and sum the observed value.
    let mut read_sum: u64 = 0;
    let reads: u64 = 50;
    for _ in 0..reads {
        let r = rw.read();
        read_sum += *r;
    }
    println!("rwlock_reads = {}", reads);
    println!("rwlock_read_sum = {}", read_sum);

    // Multiple concurrent read guards are allowed; hold two at once.
    let r1 = rw.read();
    let r2 = rw.read();
    let two_readers_value = *r1 + *r2;
    println!("rwlock_two_readers_value = {}", two_readers_value);
    drop(r1);
    drop(r2);

    // Writer path: mutate the counter a fixed number of times.
    let writes: u64 = 200;
    for _ in 0..writes {
        let mut w = rw.write();
        *w += 1;
    }
    let rwlock_final = *rw.read();
    println!("rwlock_writes = {}", writes);
    println!("rwlock_final = {}", rwlock_final);

    // try_write should fail while a read guard is held.
    let held_read = rw.read();
    let try_write_blocked = rw.try_write().is_none();
    println!("rwlock_try_write_blocked = {}", try_write_blocked);
    drop(held_read);
    let try_write_free = rw.try_write().is_some();
    println!("rwlock_try_write_when_free = {}", try_write_free);

    // --- ReentrantMutex: same thread can lock multiple times (deterministic). ---
    let re = ReentrantMutex::new(Cell::new(0i64));
    {
        let g1 = re.lock();
        g1.set(g1.get() + 5);
        {
            // Re-lock on the same thread; would deadlock with a plain Mutex.
            let g2 = re.lock();
            g2.set(g2.get() + 7);
        }
        g1.set(g1.get() + 1);
    }
    let reentrant_final = re.lock().get();
    println!("reentrant_final = {}", reentrant_final);

    // --- const constructors are available (compile-time check, runtime use). ---
    static GLOBAL: Mutex<u32> = Mutex::new(42);
    let global_val = *GLOBAL.lock();
    println!("const_mutex_value = {}", global_val);

    println!("== survey_parking_lot done ==");
}
