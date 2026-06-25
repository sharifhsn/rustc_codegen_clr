// survey_smol: exercise smol's executor (`block_on`) over self-contained,
// deterministic async work — no net, no timers, no files, no RNG.
//
// All futures here resolve from fixed inputs, so output is byte-identical
// across runs and (ideally) across native rustc and the .NET backend.

use futures_lite::future;

// A trivially-ready async computation over fixed values.
async fn add(a: u64, b: u64) -> u64 {
    a + b
}

// A future that awaits other futures (composition / state-machine driving),
// still fully deterministic and reactor-free.
async fn fib_async(n: u32) -> u64 {
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    let mut i = 0;
    while i < n {
        let next = add(a, b).await;
        a = b;
        b = next;
        i += 1;
    }
    a
}

// Concurrently drive several ready futures within one block_on and combine.
async fn parallel_sum() -> u64 {
    // future::zip joins two futures; both are ready, no reactor needed.
    let (x, y) = future::zip(add(10, 20), add(3, 4)).await;
    x + y
}

fn main() {
    // 1) block_on a simple ready async block computing over fixed values.
    let basic: u64 = smol::block_on(async { 6 * 7 });
    println!("basic_block_on = {}", basic);

    // 2) block_on an async fn that awaits a nested future.
    let sum: u64 = smol::block_on(add(40, 2));
    println!("async_add = {}", sum);

    // 3) Drive an async state machine (loop with .await) to a fixed result.
    //    fib(10) = 55, fib(20) = 6765 — fully determined.
    let f10: u64 = smol::block_on(fib_async(10));
    let f20: u64 = smol::block_on(fib_async(20));
    println!("fib_async_10 = {}", f10);
    println!("fib_async_20 = {}", f20);

    // 4) Concurrency primitive (zip) over ready futures.
    let combined: u64 = smol::block_on(parallel_sum());
    println!("parallel_sum = {}", combined);

    // 5) future::ready + map-style chaining through the executor.
    let chained: u64 = smol::block_on(async {
        let v = future::ready(100u64).await;
        let w = add(v, 23).await;
        w
    });
    println!("chained = {}", chained);

    // 6) Use smol's local Executor explicitly: spawn ready tasks, run to done.
    //    Deterministic because each task computes a fixed value; we sum them
    //    (order-independent) rather than print spawn/poll order.
    let ex = smol::LocalExecutor::new();
    let total: u64 = smol::block_on(ex.run(async {
        let t1 = ex.spawn(add(1, 2));
        let t2 = ex.spawn(add(3, 4));
        let t3 = ex.spawn(fib_async(7)); // fib(7) = 13
        let a = t1.await;
        let b = t2.await;
        let c = t3.await;
        a + b + c
    }));
    println!("executor_total = {}", total);

    println!("== survey_smol done ==");
}
