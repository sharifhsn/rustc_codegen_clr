// mycorrhiza::sync in action: Semaphore/SemaphorePermit, Signal (ManualResetEventSlim),
// CountdownEvent, Barrier, and SharedLock — each wrapper's basic contract, on the real .NET backend,
// with genuine OS threads (std::thread::spawn) so waits/releases/signals actually cross threads.
//
// Every result is checked in-Rust; `main` prints `pass` then `total` (a `9000000xx` marker flags any
// failing check) and returns non-zero on any mismatch -- the cd_* harness convention.
#![allow(dead_code)]

use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use mycorrhiza::sync::{
    Barrier, CountdownEvent, Semaphore, SharedLock, SharedMutex, SharedRwLock, Signal,
};
use mycorrhiza::system::console::Console;

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    println!("== cd_sync start ==");

    // ---------- 1. Semaphore: blocks until released ----------
    // A binary semaphore starting at 0 permits: the spawned thread's `wait()` MUST block until the
    // main thread releases it. We prove the block actually happened by having the waiter record a
    // sequence number that must land strictly after the release's own sequence number.
    {
        let sem = Semaphore::new(0);
        static SEQ: AtomicU32 = AtomicU32::new(0);
        static RELEASE_SEQ: AtomicU32 = AtomicU32::new(0);
        static WAIT_SEQ: AtomicU32 = AtomicU32::new(0);

        let waiter = thread::spawn(move || {
            sem.wait(); // must block here until the main thread releases
            WAIT_SEQ.store(SEQ.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
        });

        // Give the waiter thread a real chance to reach `wait()` and actually block.
        thread::sleep(Duration::from_millis(50));
        RELEASE_SEQ.store(SEQ.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
        sem.release();

        waiter.join().unwrap();
        // The release must have happened (sequenced) strictly before the post-wait store.
        chk!((RELEASE_SEQ.load(Ordering::SeqCst) < WAIT_SEQ.load(Ordering::SeqCst)), true);
        chk!(sem.current_count(), 0); // the single permit was consumed by wait()
    }

    // ---------- 2. Semaphore::acquire RAII releases on drop ----------
    {
        let sem = Semaphore::new(1);
        {
            let _permit = sem.acquire();
            chk!(sem.current_count(), 0); // permit held
        }
        chk!(sem.current_count(), 1); // released on drop
    }

    // ---------- 3. Semaphore::acquire_async composes with task::block_on ----------
    {
        use mycorrhiza::task::block_on;
        let sem = Semaphore::new(1);
        let count_while_held = block_on(async {
            let _permit = sem.acquire_async().await;
            sem.current_count()
        });
        chk!(count_while_held, 0);
        chk!(sem.current_count(), 1); // released once the guard dropped
    }

    // ---------- 4. Signal: wakes multiple waiters ----------
    // Two waiter threads block on `wait()`; only after `set()` may either proceed. We prove both were
    // actually blocked (not racing ahead) by checking neither incremented the counter before `set()`.
    {
        let signal = Signal::new();
        chk!(signal.is_set(), false);
        static WOKEN: AtomicI32 = AtomicI32::new(0);

        let t1 = thread::spawn(move || {
            signal.wait();
            WOKEN.fetch_add(1, Ordering::SeqCst);
        });
        let t2 = thread::spawn(move || {
            signal.wait();
            WOKEN.fetch_add(1, Ordering::SeqCst);
        });

        thread::sleep(Duration::from_millis(50));
        chk!(WOKEN.load(Ordering::SeqCst), 0); // neither woken yet -- both genuinely blocked

        signal.set();
        t1.join().unwrap();
        t2.join().unwrap();
        chk!(WOKEN.load(Ordering::SeqCst), 2); // both woken by the single set()
        chk!(signal.is_set(), true);

        signal.reset();
        chk!(signal.is_set(), false);
    }

    // ---------- 5. CountdownEvent: releases waiters exactly at zero ----------
    {
        let latch = CountdownEvent::new(3);
        chk!(latch.current_count(), 3);
        chk!(latch.is_set(), false);

        let waiter = {
            let latch = latch;
            thread::spawn(move || {
                latch.wait();
            })
        };

        // Signal twice: must NOT release yet (count 3 -> 2 -> 1).
        chk!(latch.signal(), false);
        chk!(latch.current_count(), 2);
        chk!(latch.signal(), false);
        chk!(latch.current_count(), 1);
        chk!(latch.is_set(), false);

        // The third signal brings it to zero -- THIS call must report true, and only this one.
        chk!(latch.signal(), true);
        chk!(latch.current_count(), 0);
        chk!(latch.is_set(), true);

        waiter.join().unwrap(); // must not hang -- the waiter was released exactly at zero
    }

    // ---------- 6. Barrier: synchronizes N participants ----------
    // 4 participant threads each push a per-phase marker into a shared counter, then
    // `signal_and_wait()`. Every thread's "after barrier" observation of the counter must see ALL 4
    // increments -- i.e. no thread can proceed past the barrier before every participant arrived.
    {
        let barrier = Barrier::new(4);
        static ARRIVED: AtomicI32 = AtomicI32::new(0);
        static SEEN_AT_RELEASE: [AtomicI32; 4] = [
            AtomicI32::new(-1),
            AtomicI32::new(-1),
            AtomicI32::new(-1),
            AtomicI32::new(-1),
        ];
        ARRIVED.store(0, Ordering::SeqCst);

        let handles: Vec<_> = (0..4)
            .map(|i| {
                thread::spawn(move || {
                    // Stagger arrivals so the barrier is genuinely tested, not just a no-op.
                    thread::sleep(Duration::from_millis(10 * i as u64));
                    ARRIVED.fetch_add(1, Ordering::SeqCst);
                    barrier.signal_and_wait();
                    // By the time signal_and_wait() returns, ALL 4 must have arrived.
                    SEEN_AT_RELEASE[i].store(ARRIVED.load(Ordering::SeqCst), Ordering::SeqCst);
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        chk!(ARRIVED.load(Ordering::SeqCst), 4);
        for i in 0..4 {
            chk!(SEEN_AT_RELEASE[i].load(Ordering::SeqCst), 4);
        }
        chk!(barrier.participant_count(), 4);
    }

    // ---------- 7. SharedLock: mutual exclusion under real contention ----------
    // Two threads each increment a shared (non-atomic-protected-by-lock) counter many times, guarded
    // by the SAME SharedLock. If exclusion were fake, interleaved read-modify-write would lose updates
    // and the final count would fall short of the expected total.
    {
        static mut COUNTER: i64 = 0;
        let lock = SharedLock::new();
        const ITERS: i64 = 200_000;

        let l1 = lock;
        let l2 = lock;
        let t1 = thread::spawn(move || {
            for _ in 0..ITERS {
                let _g = l1.lock();
                unsafe {
                    let v = std::ptr::read(std::ptr::addr_of!(COUNTER));
                    std::ptr::write(std::ptr::addr_of_mut!(COUNTER), v + 1);
                }
            }
        });
        let t2 = thread::spawn(move || {
            for _ in 0..ITERS {
                let _g = l2.lock();
                unsafe {
                    let v = std::ptr::read(std::ptr::addr_of!(COUNTER));
                    std::ptr::write(std::ptr::addr_of_mut!(COUNTER), v + 1);
                }
            }
        });
        t1.join().unwrap();
        t2.join().unwrap();
        let final_count = unsafe { std::ptr::read(std::ptr::addr_of!(COUNTER)) };
        chk!(final_count, ITERS * 2);
    }

    // ---------- 8. SharedMutex<T>: the same contention proof, with ZERO unsafe ----------
    // Same shape as check #7 (N threads x M non-atomic-looking increments of a shared counter must
    // land exactly, with no lost updates under real contention) but through `SharedMutex<T>` instead
    // of a bare `SharedLock` + raw pointer: the counter lives inside the mutex's own `UnsafeCell`,
    // reachable only via `SharedMutexGuard`'s `Deref`/`DerefMut`. No `unsafe` appears anywhere in this
    // block -- that is the entire point of the wrapper over `SharedLock` alone.
    {
        let mutex = SharedMutex::new(0i64);
        const ITERS: i64 = 200_000;

        thread::scope(|s| {
            for _ in 0..2 {
                s.spawn(|| {
                    for _ in 0..ITERS {
                        let mut guard = mutex.lock();
                        *guard += 1;
                    }
                });
            }
        });

        chk!(*mutex.lock(), ITERS * 2);

        // get_mut()/into_inner() are lock-free (proven by the type system, not by a runtime check),
        // but confirm they still observe the correct final value.
        let mut mutex = mutex;
        chk!(*mutex.get_mut(), ITERS * 2);
        chk!(mutex.into_inner(), ITERS * 2);
    }

    // ---------- 9. SharedRwLock<T>: writers serialize exactly like SharedMutex ----------
    // Same contention shape as check #8 but through `write()` instead of `lock()` -- proves the
    // exclusive side of the reader/writer lock genuinely excludes every other writer (and, since no
    // reader is active concurrently here, this alone would already catch a broken ReaderWriterLockSlim
    // wiring). Zero unsafe.
    {
        let rwlock = SharedRwLock::new(0i64);
        const ITERS: i64 = 200_000;

        thread::scope(|s| {
            for _ in 0..2 {
                s.spawn(|| {
                    for _ in 0..ITERS {
                        let mut guard = rwlock.write();
                        *guard += 1;
                    }
                });
            }
        });

        chk!(*rwlock.read(), ITERS * 2);
    }

    // ---------- 10. SharedRwLock<T>: readers are genuinely concurrent ----------
    // N threads each take a `read()` guard and hold it across a barrier-style rendezvous: every
    // reader records that it observed the OTHERS' "I'm holding my read guard" flags all set *while its
    // own read guard was still held*. That is only possible if the read locks truly overlap in time --
    // a lock that (incorrectly) serialized readers would deadlock this rendezvous instead (each thread
    // would block forever in `read()` waiting for a reader that is itself waiting to see all N flags).
    {
        const READERS: usize = 4;
        let rwlock = SharedRwLock::new(123i64);
        let holding: Vec<AtomicI32> = (0..READERS).map(|_| AtomicI32::new(0)).collect();
        let all_saw_full_overlap = AtomicI32::new(1);

        thread::scope(|s| {
            for i in 0..READERS {
                let rwlock = &rwlock;
                let holding = &holding;
                let all_saw_full_overlap = &all_saw_full_overlap;
                s.spawn(move || {
                    let guard = rwlock.read();
                    holding[i].store(1, Ordering::SeqCst);

                    // Poll (bounded) until every reader has announced it is holding its guard, or give
                    // up -- a broken (serializing) implementation would never reach "all held" while
                    // this guard is still live, since it would block acquiring THIS read lock until
                    // whichever other "reader" (actually serialized) released first.
                    let mut saw_all = false;
                    for _ in 0..2000 {
                        if holding.iter().all(|h| h.load(Ordering::SeqCst) == 1) {
                            saw_all = true;
                            break;
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                    if !saw_all {
                        all_saw_full_overlap.store(0, Ordering::SeqCst);
                    }
                    // Confirm the data is still readable/correct while N-way concurrent.
                    let v = *guard;
                    if v != 123 {
                        all_saw_full_overlap.store(0, Ordering::SeqCst);
                    }
                });
            }
        });

        chk!(all_saw_full_overlap.load(Ordering::SeqCst), 1);
        chk!(*rwlock.read(), 123);
    }

    // ---------- 11. SharedRwLock<T>: get_mut()/into_inner() are lock-free and correct ----------
    {
        let mut rwlock = SharedRwLock::new(7i64);
        chk!(*rwlock.get_mut(), 7);
        *rwlock.get_mut() += 1;
        chk!(rwlock.into_inner(), 8);
    }

    println!("== cd_sync done ==");
    println!("pass");
    Console::writeln_u64(pass as u64);
    println!("total");
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
