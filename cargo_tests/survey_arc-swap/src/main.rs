use arc_swap::ArcSwap;
use std::sync::Arc;

fn main() {
    // Build an ArcSwap holding an Arc<i32>. All operations below run on the
    // current thread in a fixed sequence, so the output is fully deterministic
    // (no threads, no timing, no addresses — only the integer values).
    let swap = ArcSwap::from(Arc::new(10_i32));

    // load: read the current value via a temporary Guard, deref to the i32.
    let loaded0 = **swap.load();
    println!("load_initial = {}", loaded0);

    // load_full: get an owned Arc<i32> clone of the current value.
    let full0 = swap.load_full();
    println!("load_full = {}", *full0);

    // store: replace the contents with a new Arc, then read it back.
    swap.store(Arc::new(20));
    let loaded1 = **swap.load();
    println!("store_then_load = {}", loaded1);

    // swap: atomically exchange, returning the PREVIOUS Arc.
    let previous = swap.swap(Arc::new(30));
    println!("swap_returned_previous = {}", *previous);
    let loaded2 = **swap.load();
    println!("swap_then_load = {}", loaded2);

    // rcu: read-copy-update. Apply a pure function repeatedly; arc-swap retries
    // the closure on contention, but single-threaded it runs exactly once each.
    // Add 5 three times: 30 -> 35 -> 40 -> 45.
    for _ in 0..3 {
        swap.rcu(|cur| Arc::new(**cur + 5));
    }
    let loaded3 = **swap.load();
    println!("rcu_after_three = {}", loaded3);

    // rcu can also branch on the current value deterministically.
    // Double it once: 45 -> 90.
    let prev_rcu = swap.rcu(|cur| Arc::new(**cur * 2));
    println!("rcu_doubled_prev = {}", *prev_rcu);
    println!("rcu_doubled_now = {}", **swap.load());

    // compare_and_swap: only succeeds if the current Arc matches the expected
    // one. Use the owned Arc we just loaded as the expected value so it matches.
    // The call returns the previous contents (as a Guard); deref to the i32.
    let expected = swap.load_full();
    let cas_prev = swap.compare_and_swap(&expected, Arc::new(100));
    println!("cas_prev_value = {}", **cas_prev);
    // A matching expected swaps in the new value: now reads 100.
    println!("cas_then_load = {}", **swap.load());

    // A failing compare_and_swap: expected is now stale (we just stored 100),
    // so this must NOT swap. Construct a clearly-wrong expected value.
    let stale = Arc::new(-1_i32);
    let _ = swap.compare_and_swap(&stale, Arc::new(999));
    println!("cas_failed_keeps_value = {}", **swap.load());

    println!("== survey_arc-swap done ==");
}
