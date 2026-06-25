// Multi-threaded dashmap contention probe: 8 threads each do 8000 entry-increments
// spread over 64 keys, all hammering the SAME shared map -> real shard-lock
// contention -> exercises parking_lot's parker (the dotnet `generic.rs` spin parker
// routed by the parking_lot_core overlay). Final state is fully deterministic.
use dashmap::DashMap;
use std::sync::Arc;
use std::thread;

const THREADS: usize = 8;
const ITERS: usize = 8000;
const KEYS: usize = 64;

fn main() {
    let map: Arc<DashMap<usize, u64>> = Arc::new(DashMap::new());
    for k in 0..KEYS {
        map.insert(k, 0);
    }
    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let m = Arc::clone(&map);
            thread::spawn(move || {
                for i in 0..ITERS {
                    *m.entry(i % KEYS).or_insert(0) += 1;
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // Each of KEYS keys incremented THREADS*ITERS/KEYS times.
    let mut total: u64 = 0;
    let mut per_key_ok = true;
    let expect_per_key = (THREADS * ITERS / KEYS) as u64;
    for k in 0..KEYS {
        let v = *map.get(&k).unwrap();
        total += v;
        if v != expect_per_key {
            per_key_ok = false;
        }
    }
    println!("threads = {THREADS}");
    println!("len = {}", map.len());
    println!("total_increments = {total}");
    println!("expected_total = {}", (THREADS * ITERS) as u64);
    println!("every_key_exact = {per_key_ok}");
}
