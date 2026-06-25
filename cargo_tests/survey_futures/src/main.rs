// survey_futures: exercise the pure executor + combinator surface of the
// `futures` crate DETERMINISTICALLY. No reactor, no timers, no threads, no I/O —
// just `futures::executor::block_on` driving ready/map/join/FuturesUnordered
// over fixed values, then printing the resolved RESULTS (never order/timing).

use futures::executor::block_on;
use futures::future::{self, FutureExt};
use futures::stream::{FuturesUnordered, StreamExt};

fn main() {
    // 1. The simplest leaf: a future that is immediately Ready with a fixed value.
    //    block_on resolves it without any reactor.
    let ready_val: i32 = block_on(future::ready(7));
    println!("ready = {}", ready_val);

    // 2. `.map` combinator: transform a ready value through a pure closure.
    let mapped: i32 = block_on(future::ready(20).map(|x| x + 22));
    println!("mapped = {}", mapped);

    // 3. Chained combinators: ready -> map -> then (sequence two futures).
    //    `.then` returns a new future; we keep everything ready so it is pure.
    let chained: i32 = block_on(future::ready(3).map(|x| x * 4).then(|y| future::ready(y + 1)));
    println!("chained = {}", chained);

    // 4. `join`: drive two independent futures to completion, collect both
    //    results as a tuple. Deterministic because both are ready leaves.
    let (a, b): (i32, &str) = block_on(future::join(future::ready(100), future::ready("ok")));
    println!("join_a = {}", a);
    println!("join_b = {}", b);

    // 5. `join3`: same idea, three futures.
    let (x, y, z): (u8, u8, u8) =
        block_on(future::join3(future::ready(1u8), future::ready(2u8), future::ready(3u8)));
    println!("join3_sum = {}", x as u32 + y as u32 + z as u32);

    // 6. FuturesUnordered: completion ORDER is nondeterministic in general, so we
    //    collect into a Vec and then SUM/SORT to derive a deterministic result.
    //    Each member is a ready future yielding a fixed square.
    let unordered: FuturesUnordered<_> =
        (1u64..=5).map(|n| future::ready(n * n)).collect();
    let mut collected: Vec<u64> = block_on(unordered.collect());
    let sum: u64 = collected.iter().copied().sum();
    collected.sort_unstable(); // make the printed sequence order-independent
    println!("unordered_count = {}", collected.len());
    println!("unordered_sum = {}", sum);
    println!("unordered_sorted = {:?}", collected);

    // 7. select over two ready branches but resolve to a deterministic total:
    //    use `join` of an iterator of futures via `future::join_all`, which
    //    preserves INPUT order (deterministic) regardless of completion order.
    let all: Vec<i32> = block_on(future::join_all((0..4).map(|i| future::ready(i * 10))));
    println!("join_all = {:?}", all);

    // 8. A small async block driven by block_on — exercises the async/await
    //    state-machine codegen path against a fixed computation.
    let computed: i32 = block_on(async {
        let p = future::ready(6).await;
        let q = future::ready(7).await;
        p * q
    });
    println!("async_block = {}", computed);

    println!("== survey_futures done ==");
}
