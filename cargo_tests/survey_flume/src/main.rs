// survey_flume — exercise the flume channel core surface DETERMINISTICALLY.
//
// Single-thread "send-then-recv": we push a fixed set of values into a channel
// from the current thread, then drain them on the SAME thread and compute
// derived totals. No real OS threads, no async reactor, no timing — so the
// output is byte-stable across native rustc and the .NET backend.

fn main() {
    // ----- 1. unbounded channel: send a fixed set, then recv-sum -----------
    let (tx, rx) = flume::unbounded::<i64>();

    // A fixed, deterministic data set.
    let data: [i64; 8] = [3, 1, 4, 1, 5, 9, 2, 6];

    let mut sent = 0i64;
    for &v in data.iter() {
        match tx.send(v) {
            Ok(()) => sent += 1,
            Err(_) => println!("send_error = unexpected"),
        }
    }
    // Drop the sender so the channel is closed and recv terminates cleanly.
    drop(tx);

    println!("unbounded_sent = {}", sent);
    println!("unbounded_len_before_drain = {}", rx.len());
    println!("unbounded_is_empty_before = {}", rx.is_empty());

    // Drain with recv() until disconnected; sum + count.
    let mut recv_count = 0i64;
    let mut recv_sum = 0i64;
    loop {
        match rx.recv() {
            Ok(v) => {
                recv_count += 1;
                recv_sum += v;
            }
            Err(_) => break, // RecvError::Disconnected once empty + all senders dropped
        }
    }
    println!("unbounded_recv_count = {}", recv_count);
    println!("unbounded_recv_sum = {}", recv_sum);
    println!("unbounded_is_empty_after = {}", rx.is_empty());

    // ----- 2. try_recv on a drained, disconnected channel ------------------
    let try_after = match rx.try_recv() {
        Ok(_) => "unexpected_value",
        Err(flume::TryRecvError::Empty) => "empty",
        Err(flume::TryRecvError::Disconnected) => "disconnected",
    };
    println!("unbounded_try_recv_after = {}", try_after);

    // ----- 3. bounded channel: capacity, send up to cap, drain via iter ----
    let (btx, brx) = flume::bounded::<u32>(4);
    let mut bsent = 0u32;
    // Send exactly capacity items so no blocking on a single thread.
    for v in 1u32..=4 {
        match btx.send(v) {
            Ok(()) => bsent += 1,
            Err(_) => println!("bounded_send_error = unexpected"),
        }
    }
    println!("bounded_capacity = {}", brx.capacity().unwrap_or(0));
    println!("bounded_sent = {}", bsent);
    println!("bounded_len = {}", brx.len());

    // try_send into a now-full channel should report Full (deterministic).
    let full_marker = match btx.try_send(99) {
        Ok(()) => "unexpected_ok",
        Err(flume::TrySendError::Full(_)) => "full",
        Err(flume::TrySendError::Disconnected(_)) => "disconnected",
    };
    println!("bounded_try_send_when_full = {}", full_marker);

    drop(btx);

    // Drain the bounded channel via the blocking Iterator (drain()/iter()).
    let mut bsum = 0u32;
    let mut bprod = 1u32;
    let mut bcount = 0u32;
    for v in brx.drain() {
        bsum += v;
        bprod = bprod.wrapping_mul(v);
        bcount += 1;
    }
    println!("bounded_drain_count = {}", bcount);
    println!("bounded_drain_sum = {}", bsum);
    println!("bounded_drain_product = {}", bprod);

    // ----- 4. rendezvous channel (capacity 0): try_send with no receiver ---
    // On a single thread with no waiting receiver, try_send must report Full.
    let (rtx, rrx) = flume::bounded::<u8>(0);
    let rdz = match rtx.try_send(7) {
        Ok(()) => "unexpected_ok",
        Err(flume::TrySendError::Full(_)) => "full",
        Err(flume::TrySendError::Disconnected(_)) => "disconnected",
    };
    println!("rendezvous_try_send = {}", rdz);
    // try_recv on an empty rendezvous channel reports Empty.
    let rdz_recv = match rrx.try_recv() {
        Ok(_) => "unexpected_value",
        Err(flume::TryRecvError::Empty) => "empty",
        Err(flume::TryRecvError::Disconnected) => "disconnected",
    };
    println!("rendezvous_try_recv = {}", rdz_recv);

    // ----- 5. sender/receiver cloning + multi-sender disconnect semantics --
    let (ctx, crx) = flume::unbounded::<i32>();
    let ctx2 = ctx.clone();
    let _ = ctx.send(10);
    let _ = ctx2.send(20);
    let _ = ctx.send(30);
    // Drop ONE sender; channel still connected via the clone, recv works.
    drop(ctx);
    let mut clone_sum = 0i32;
    let mut clone_count = 0i32;
    // Pull exactly the 3 we know are buffered, using try_recv to stay non-blocking.
    loop {
        match crx.try_recv() {
            Ok(v) => {
                clone_sum += v;
                clone_count += 1;
            }
            Err(_) => break,
        }
    }
    drop(ctx2);
    println!("clone_recv_count = {}", clone_count);
    println!("clone_recv_sum = {}", clone_sum);
    let clone_after = match crx.recv() {
        Ok(_) => "unexpected_value",
        Err(_) => "disconnected",
    };
    println!("clone_recv_after_all_dropped = {}", clone_after);

    // ----- 6. derived grand total (single int that folds everything) -------
    let grand_total =
        recv_sum + sent + i64::from(bsum) + i64::from(bsum * 0 + bsent) + i64::from(clone_sum);
    println!("grand_total = {}", grand_total);

    println!("== survey_flume done ==");
}
