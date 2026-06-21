//! Minimal tokio probe: does tokio's current-thread runtime + scheduler + timers + channels work on
//! the async-codegen foundation, WITHOUT net/mio? Features rt/macros/time/sync only (no net/io -> no
//! mio). Panic-safe. SUCCESS = "== pal_tokio done ==" with sane values.
use std::time::{Duration, Instant};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("== pal_tokio start ==");

    // 1. spawn + join (cooperative tasks on the current thread)
    let h = tokio::spawn(async { 21 * 2 });
    println!("1  spawn/join: {}", h.await.unwrap_or(-1));

    // 2. timer (the time driver + PAL time)
    let t = Instant::now();
    tokio::time::sleep(Duration::from_millis(5)).await;
    println!("2  sleep: elapsed_ms_ok={}", t.elapsed() >= Duration::from_millis(1));

    // 3. mpsc channel between two tasks
    let (tx, mut rx) = tokio::sync::mpsc::channel::<i32>(4);
    tokio::spawn(async move {
        for i in 0..3 {
            let _ = tx.send(i).await;
        }
    });
    let mut sum = 0;
    while let Some(v) = rx.recv().await {
        sum += v;
    }
    println!("3  mpsc sum: {} (expect 3)", sum);

    // 4. join! several futures
    let (a, b, c) = tokio::join!(async { 10 }, async { 20 }, async { 12 });
    println!("4  join!: {} (expect 42)", a + b + c);

    // 5. select! between a ready future and a slow timer
    let pick = tokio::select! {
        v = async { 7 } => v,
        _ = tokio::time::sleep(Duration::from_secs(10)) => -1,
    };
    println!("5  select!: {} (expect 7)", pick);

    println!("== pal_tokio done ==");
}
