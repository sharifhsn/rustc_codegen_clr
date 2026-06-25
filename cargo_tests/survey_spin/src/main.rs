// survey_spin: exercise spin's no_std spinlock primitives DETERMINISTICALLY.
// Single-threaded driver — we lock/mutate/read on the main thread so the result
// is fully reproducible (no real OS threads, no timing, no addresses).
//
// Surface exercised: spin::Mutex (lock + mutate), spin::Once (call_once + get),
// spin::RwLock (read + write), spin::Lazy (deferred init), spin::Barrier (single
// participant), and the lock_api try_* fast-paths.

use spin::{Barrier, LazyLock, Mutex, Once, RwLock};

fn main() {
    // --- spin::Mutex: lock, mutate under the guard, observe the result. ---
    let counter: Mutex<i64> = Mutex::new(0);
    {
        // Mutating critical section. `lock()` is infallible (spins, never errors).
        let mut g = counter.lock();
        *g += 41;
        *g *= 2;
        *g -= 1; // 41*2 - 1 = 81
    } // guard dropped -> unlocked here
    println!("mutex_value = {}", *counter.lock());

    // try_lock fast-path: succeeds because no guard is held.
    match counter.try_lock() {
        Some(g) => println!("mutex_try_lock = some({})", *g),
        None => println!("mutex_try_lock = none"),
    }
    // try_lock while a guard IS held -> must return None (deterministic contention).
    {
        let _held = counter.lock();
        match counter.try_lock() {
            Some(_) => println!("mutex_try_while_held = some"),
            None => println!("mutex_try_while_held = none"),
        }
    }

    // --- spin::Once: run an initializer exactly once. ---
    let once: Once<u32> = Once::new();
    let first = *once.call_once(|| 7);
    // Second call_once must NOT re-run the closure; value stays 7.
    let second = *once.call_once(|| 999);
    println!("once_first = {}", first);
    println!("once_second = {}", second);
    println!("once_is_completed = {}", once.is_completed());
    match once.get() {
        Some(v) => println!("once_get = some({})", v),
        None => println!("once_get = none"),
    }

    // A fresh Once that was never initialized: get() must be None.
    let empty: Once<u32> = Once::new();
    match empty.get() {
        Some(v) => println!("once_empty_get = some({})", v),
        None => println!("once_empty_get = none"),
    }

    // --- spin::RwLock: many readers / one writer. ---
    let data: RwLock<Vec<i32>> = RwLock::new(vec![10, 20, 30]);
    {
        // Shared read access.
        let r = data.read();
        let sum: i32 = r.iter().sum();
        println!("rwlock_read_len = {}", r.len());
        println!("rwlock_read_sum = {}", sum);
    }
    {
        // Exclusive write access.
        let mut w = data.write();
        w.push(40);
        w[0] = 11;
    }
    {
        let r = data.read();
        let sum: i32 = r.iter().sum();
        println!("rwlock_after_write_len = {}", r.len());
        println!("rwlock_after_write_sum = {}", sum); // 11+20+30+40 = 101
    }
    // try_read / try_write fast paths on an idle lock.
    match data.try_write() {
        Some(mut w) => {
            w.push(50);
            println!("rwlock_try_write = some(len={})", w.len());
        }
        None => println!("rwlock_try_write = none"),
    }
    // try_read while a write guard is held -> None.
    {
        let _w = data.write();
        match data.try_read() {
            Some(_) => println!("rwlock_try_read_while_write = some"),
            None => println!("rwlock_try_read_while_write = none"),
        }
    }

    // --- spin::LazyLock: value initialized on first deref, then cached. ---
    let lazy: LazyLock<i64> = LazyLock::new(|| {
        let mut acc: i64 = 0;
        for i in 1..=10 {
            acc += i;
        }
        acc // 55
    });
    println!("lazy_value = {}", *lazy);
    println!("lazy_value_again = {}", *lazy); // same, no re-init

    // --- spin::Barrier with a single participant: wait() returns immediately. ---
    let barrier = Barrier::new(1);
    let res = barrier.wait();
    println!("barrier_is_leader = {}", res.is_leader());

    println!("== survey_spin done ==");
}
