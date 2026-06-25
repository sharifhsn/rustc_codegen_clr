use crossbeam_channel::{bounded, unbounded, TryRecvError};

fn main() {
    // ------------------------------------------------------------------
    // UNBOUNDED channel: send a fixed set of values, recv them, sum.
    // Single-thread send-then-recv keeps it fully deterministic (FIFO).
    // ------------------------------------------------------------------
    let (utx, urx) = unbounded::<i64>();
    let values: [i64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

    let mut send_ok = 0i64;
    for &v in values.iter() {
        match utx.send(v) {
            Ok(()) => send_ok += 1,
            Err(_) => {}
        }
    }
    // Drop the sender so the receiver side observes a clean disconnect.
    drop(utx);

    // Drain via recv() until the channel is empty + disconnected.
    let mut unbounded_sum = 0i64;
    let mut unbounded_count = 0i64;
    loop {
        match urx.recv() {
            Ok(v) => {
                unbounded_sum += v;
                unbounded_count += 1;
            }
            Err(_) => break, // RecvError == empty + all senders dropped
        }
    }

    println!("unbounded_send_ok = {}", send_ok);
    println!("unbounded_count = {}", unbounded_count);
    println!("unbounded_sum = {}", unbounded_sum);

    // ------------------------------------------------------------------
    // BOUNDED channel (capacity == number of items): send all, then recv.
    // With capacity >= item count, every send succeeds without blocking.
    // ------------------------------------------------------------------
    let cap = values.len();
    let (btx, brx) = bounded::<i64>(cap);

    let mut bounded_send_ok = 0i64;
    for &v in values.iter() {
        match btx.send(v) {
            Ok(()) => bounded_send_ok += 1,
            Err(_) => {}
        }
    }

    println!("bounded_send_ok = {}", bounded_send_ok);
    println!("bounded_capacity = {}", cap);
    println!("bounded_len_after_send = {}", brx.len());
    println!("bounded_is_full = {}", brx.is_full());

    // Drain with try_recv() while sender still alive: deterministic FIFO,
    // stop on Empty (we know exactly how many we put in).
    let mut bounded_sum = 0i64;
    let mut bounded_count = 0i64;
    loop {
        match brx.try_recv() {
            Ok(v) => {
                bounded_sum += v;
                bounded_count += 1;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }

    println!("bounded_count = {}", bounded_count);
    println!("bounded_sum = {}", bounded_sum);
    println!("bounded_empty_after_drain = {}", brx.is_empty());

    // ------------------------------------------------------------------
    // Capacity reflection on the unbounded receiver (None == unbounded).
    // ------------------------------------------------------------------
    let ucap_is_none = urx.capacity().is_none();
    let bcap = brx.capacity().unwrap_or(0);
    println!("unbounded_capacity_is_none = {}", ucap_is_none);
    println!("bounded_capacity_reported = {}", bcap);

    // ------------------------------------------------------------------
    // Cross-check: both channels carried the same fixed multiset, so the
    // two sums must agree, and equal the closed-form sum 1..=8 == 36.
    // ------------------------------------------------------------------
    let expected: i64 = (1..=8).sum();
    println!("sums_agree = {}", unbounded_sum == bounded_sum);
    println!("sum_matches_expected = {}", unbounded_sum == expected);

    println!("== survey_crossbeam-channel done ==");
}
