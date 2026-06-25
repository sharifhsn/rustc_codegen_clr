use futures_lite::future::{self, FutureExt};

fn main() {
    // futures_lite::future::block_on drives a future to completion on the
    // current thread. With only `ready` futures there is no real I/O / timer /
    // reactor involved, so every result here is fully deterministic.

    // 1. ready: resolve immediately to a fixed value.
    let r1: i32 = future::block_on(future::ready(7));
    println!("ready = {}", r1);

    // 2. "then"-style chaining. futures-lite has no `then` combinator, so the
    //    idiomatic way is an async block that awaits the inner ready future and
    //    then computes a continuation — exercising the async/await state machine.
    let r2: i32 = future::block_on(async {
        let v = future::ready(10).await;
        v * 3 + 1
    });
    println!("then = {}", r2);

    // 3. zip: run two futures and collect both results into a tuple (free fn).
    let (a, b): (i32, &str) =
        future::block_on(future::zip(future::ready(42), future::ready("forty-two")));
    println!("zip_left = {}", a);
    println!("zip_right = {}", b);

    // 4. or: race two ready futures; with both already ready the left one wins
    //    deterministically under block_on's single-thread polling (method on FutureExt).
    let r4: i32 = future::block_on(future::ready(100).or(future::ready(200)));
    println!("or = {}", r4);

    // 5. A larger async block doing arithmetic and awaiting nested ready futures,
    //    proving the async/await lowering runs to a fixed result.
    let r5: i32 = future::block_on(async {
        let x = future::ready(3).await;
        let y = future::ready(4).await;
        let z = {
            let w = future::ready(5).await;
            w + 1
        };
        x * 100 + y * 10 + z
    });
    println!("async_block = {}", r5);

    // 6. zip of two async continuations, then sum — combinator + state-machine mix.
    let (p, q): (i32, i32) = future::block_on(future::zip(
        async { future::ready(2).await + 8 },
        async { future::ready(20).await / 2 },
    ));
    println!("composed_sum = {}", p + q);

    // 7. poll_once: poll an already-ready future exactly once; it should yield
    //    Some(value) immediately (no waker/reactor needed). Map to a bool/int so
    //    output stays deterministic regardless of internal representation.
    let polled: Option<i32> = future::block_on(future::poll_once(future::ready(55)));
    let polled_val = match polled {
        Some(v) => v,
        None => -1,
    };
    println!("poll_once = {}", polled_val);

    // 8. Aggregate invariants into a single boolean (no float/pointer/order deps).
    let ok: bool = r1 == 7 && r2 == 31 && a == 42 && r4 == 100 && r5 == 346 && polled_val == 55;
    println!("invariants_ok = {}", ok);

    println!("== survey_futures-lite done ==");
}
