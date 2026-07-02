// Independent quick cross-check litmus probe: Message-Passing (MP) with Acquire/Release, and
// Store-Buffering (SB) with SeqCst, plus Relaxed-ordering sensitivity controls for both.
// This is a fast, focused, standalone probe (not the full campaign harness) used to sanity-check
// the atomic_load/atomic_store fix independently. Uses PERSISTENT threads (spawned once) looping
// over rounds via a reusable Barrier, to avoid the cost of spawning an OS thread per iteration
// (expensive on the .NET backend) while still resetting shared state every round.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

const ITERS: usize = 300_000;

// MP: thread A writes data then sets flag (release); thread B reads flag (acquire) then data.
// If flag observed set, data MUST be observed set. A violation = flag=1 but data=0.
fn run_mp(iters: usize, store_ord: Ordering, load_ord: Ordering) -> (u64, u64) {
    let data = Arc::new(AtomicUsize::new(0));
    let flag = Arc::new(AtomicUsize::new(0));
    let flag_seen = Arc::new(AtomicUsize::new(0));
    let violations = Arc::new(AtomicUsize::new(0));
    let start_barrier = Arc::new(Barrier::new(3));
    let done_barrier = Arc::new(Barrier::new(3));

    let writer = {
        let data = Arc::clone(&data);
        let flag = Arc::clone(&flag);
        let start_barrier = Arc::clone(&start_barrier);
        let done_barrier = Arc::clone(&done_barrier);
        thread::spawn(move || {
            for _ in 0..iters {
                start_barrier.wait();
                data.store(42, store_ord);
                flag.store(1, store_ord);
                done_barrier.wait();
            }
        })
    };
    let reader = {
        let data = Arc::clone(&data);
        let flag = Arc::clone(&flag);
        let flag_seen = Arc::clone(&flag_seen);
        let violations = Arc::clone(&violations);
        let start_barrier = Arc::clone(&start_barrier);
        let done_barrier = Arc::clone(&done_barrier);
        thread::spawn(move || {
            for _ in 0..iters {
                start_barrier.wait();
                // Busy-poll briefly to maximize the chance of racing the writer (rather than
                // always observing after the writer has already finished, which would tell us
                // nothing about ordering).
                let mut spins = 0;
                while flag.load(load_ord) == 0 && spins < 500 {
                    spins += 1;
                }
                let fval = flag.load(load_ord);
                let dval = data.load(load_ord);
                if fval == 1 {
                    flag_seen.fetch_add(1, Ordering::Relaxed);
                    if dval != 42 {
                        violations.fetch_add(1, Ordering::Relaxed);
                    }
                }
                done_barrier.wait();
            }
        })
    };

    for _ in 0..iters {
        data.store(0, Ordering::SeqCst);
        flag.store(0, Ordering::SeqCst);
        start_barrier.wait();
        done_barrier.wait();
    }
    writer.join().unwrap();
    reader.join().unwrap();
    (
        flag_seen.load(Ordering::SeqCst) as u64,
        violations.load(Ordering::SeqCst) as u64,
    )
}

// SB: two threads each store to their own var then load the OTHER var. With SeqCst both r1==0 &&
// r2==0 is forbidden (StoreLoad reordering). With Relaxed it's allowed (control). r1/r2 are
// published through shared atomics (not returned via join, since these are persistent threads).
fn run_sb(iters: usize, ord: Ordering) -> u64 {
    let x = Arc::new(AtomicUsize::new(0));
    let y = Arc::new(AtomicUsize::new(0));
    let r1_slot = Arc::new(AtomicUsize::new(0));
    let r2_slot = Arc::new(AtomicUsize::new(0));
    let both_zero = Arc::new(AtomicUsize::new(0));
    let start_barrier = Arc::new(Barrier::new(3));
    let done_barrier = Arc::new(Barrier::new(3));

    let h1 = {
        let x = Arc::clone(&x);
        let y = Arc::clone(&y);
        let r1_slot = Arc::clone(&r1_slot);
        let start_barrier = Arc::clone(&start_barrier);
        let done_barrier = Arc::clone(&done_barrier);
        thread::spawn(move || {
            for _ in 0..iters {
                start_barrier.wait();
                x.store(1, ord);
                let r1 = y.load(ord);
                r1_slot.store(r1, Ordering::SeqCst);
                done_barrier.wait();
            }
        })
    };
    let h2 = {
        let x = Arc::clone(&x);
        let y = Arc::clone(&y);
        let r2_slot = Arc::clone(&r2_slot);
        let start_barrier = Arc::clone(&start_barrier);
        let done_barrier = Arc::clone(&done_barrier);
        thread::spawn(move || {
            for _ in 0..iters {
                start_barrier.wait();
                y.store(1, ord);
                let r2 = x.load(ord);
                r2_slot.store(r2, Ordering::SeqCst);
                done_barrier.wait();
            }
        })
    };

    for _ in 0..iters {
        x.store(0, Ordering::SeqCst);
        y.store(0, Ordering::SeqCst);
        start_barrier.wait();
        done_barrier.wait();
        if r1_slot.load(Ordering::SeqCst) == 0 && r2_slot.load(Ordering::SeqCst) == 0 {
            both_zero.fetch_add(1, Ordering::Relaxed);
        }
    }
    h1.join().unwrap();
    h2.join().unwrap();
    both_zero.load(Ordering::SeqCst) as u64
}

fn main() {
    let iters = std::env::var("ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(ITERS);

    println!("== MP Relaxed (control, reorderings ALLOWED) ==");
    let (seen, viol) = run_mp(iters, Ordering::Relaxed, Ordering::Relaxed);
    println!("flag_seen={seen} violations={viol}");

    println!("== MP Release/Acquire (violations FORBIDDEN) ==");
    let (seen, viol) = run_mp(iters, Ordering::Release, Ordering::Acquire);
    println!("flag_seen={seen} violations={viol}");

    println!("== SB Relaxed (control, both-zero ALLOWED) ==");
    let bz = run_sb(iters, Ordering::Relaxed);
    println!("both_zero={bz}");

    println!("== SB SeqCst (both-zero FORBIDDEN) ==");
    let bz = run_sb(iters, Ordering::SeqCst);
    println!("both_zero={bz}");

    println!("DONE");
}
