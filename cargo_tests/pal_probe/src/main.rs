//! H2 gap probe: exercise real `std` on the dotnet PAL and print how far each subsystem gets.
//!
//! Panic-abort-safe (no deliberate panics — the dotnet target is `panic-strategy: abort`, so a panic
//! would kill the probe before later lines print). Each numbered line that prints is a subsystem that
//! works; the first MISSING line (or a hard crash) localizes the gap. `fs`/`net`/`process` are expected
//! to be `unsupported` (no PAL arm yet) — we print their `Err` rather than unwrap, to keep going.

use std::collections::{BTreeMap, HashMap};

fn main() {
    println!("== pal_probe start ==");

    // 1. Vec + iterators + sort (alloc + core)
    let mut v: Vec<i32> = (1..=10).rev().collect();
    v.sort();
    let sum: i32 = v.iter().sum();
    println!("1  vec/iter/sort:   sum={sum} min={}", v[0]);

    // 2. String + format! (alloc + fmt)
    let s = format!("{}|{:?}|{:.2}", "hi", [1, 2, 3], 3.14159_f64);
    println!("2  string/format:   {s}");

    // 3. HashMap (RandomState -> PAL random)
    let mut m = HashMap::new();
    for i in 0..5 {
        m.insert(i, i * i);
    }
    println!("3  hashmap:         len={} m[3]={}", m.len(), m[&3]);

    // 4. BTreeMap (ordered, no RNG)
    let mut bt = BTreeMap::new();
    bt.insert("b", 2);
    bt.insert("a", 1);
    bt.insert("c", 3);
    let keys: Vec<_> = bt.keys().copied().collect();
    println!("4  btreemap:        keys={keys:?}");

    // 5. time (PAL time -> Stopwatch/DateTime)
    let now = std::time::Instant::now();
    let spun: u64 = (0..1000).map(|x| x as u64).sum();
    let dt = now.elapsed();
    println!("5  time::Instant:   spun={spun} elapsed_ns_ok={}", dt.as_nanos() < u128::MAX);

    let sys = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() > 1_600_000_000)
        .unwrap_or(false);
    println!("5b SystemTime:      after_2020={sys}");

    // 6. env (PAL env)
    println!("6  env::var(PATH):  ok={}", std::env::var("PATH").is_ok());

    // 7. args (PAL args)
    println!("7  env::args:       count={}", std::env::args().count());

    // 8. threads (PAL thread spawn/join)
    let h = std::thread::spawn(|| (0..21).sum::<i32>() * 2);
    match h.join() {
        Ok(r) => println!("8  thread join:     r={r}"),
        Err(_) => println!("8  thread join:     ERR (panicked)"),
    }

    // 9. fs (NO PAL arm yet -> expect Err unsupported; printed, not unwrapped)
    let w = std::fs::write("/tmp/pal_probe.txt", b"hello pal");
    println!("9  fs::write:       {w:?}");
    let r = std::fs::read_to_string("/tmp/pal_probe.txt");
    println!("9b fs::read:        {r:?}");
    println!("9c fs::metadata:    {:?}", std::fs::metadata(".").map(|m| m.is_dir()));

    // 10. process (NO PAL arm yet -> expect Err)
    let cmd = std::process::Command::new("echo").arg("hi").output();
    println!("10 process::cmd:    ok={}", cmd.is_ok());

    // 11. net (NO PAL arm yet -> expect Err; TcpStream connect to a dead port)
    let net = std::net::TcpStream::connect("127.0.0.1:9");
    println!("11 net::tcp:        {:?}", net.map(|_| ()).map_err(|e| e.kind()));

    println!("== pal_probe done ==");
}
