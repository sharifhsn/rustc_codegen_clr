// mycorrhiza::sync::{channel, bounded_channel, Sender, Receiver} in action: a real mpsc/mpmc
// producer/consumer proof over System.Threading.Channels, run on genuine OS threads
// (std::thread::spawn) -- not just single-threaded sanity.
//
// Every result is checked in-Rust; `main` prints `pass` then `total` (a `9000000xx` marker flags any
// failing check) and returns non-zero on any mismatch -- the cd_* harness convention.
#![allow(dead_code)]

use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use mycorrhiza::sync::{bounded_channel, channel};
use mycorrhiza::system::console::Console;

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    println!("== cd_channel start ==");

    // ---------- 1. Unbounded: try_send / try_recv basic single-threaded contract ----------
    {
        let (tx, rx) = channel::<i32>();
        chk!(rx.try_recv(), None); // empty
        chk!(tx.try_send(10), Ok(()));
        chk!(tx.try_send(20), Ok(()));
        chk!(rx.try_recv(), Some(10)); // FIFO order
        chk!(rx.try_recv(), Some(20));
        chk!(rx.try_recv(), None); // drained again
    }

    // ---------- 2. Sender/Receiver are cheaply Clone -- both halves usable from many places ----------
    {
        let (tx, rx) = channel::<i32>();
        let tx2 = tx.clone(); // each clone roots the same managed writer
        tx.try_send(1).unwrap();
        tx2.try_send(2).unwrap();
        chk!(rx.try_recv(), Some(1));
        chk!(rx.try_recv(), Some(2));
    }

    // ---------- 3. Real cross-thread blocking receive: recv_blocking() actually blocks ----------
    // The receiver calls recv_blocking() on an empty channel; it must NOT return until the sender
    // (on a different OS thread, after a real sleep) sends. Proven the same way cd_sync proves
    // Semaphore::wait blocks: a sequence-number race that must land in a fixed order.
    {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        static SEND_SEQ: AtomicU32 = AtomicU32::new(0);
        static RECV_SEQ: AtomicU32 = AtomicU32::new(0);

        let (tx, rx) = channel::<i32>();
        let receiver = thread::spawn(move || {
            let v = rx.recv_blocking(); // must block here until the sender thread sends
            RECV_SEQ.store(SEQ.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
            v
        });

        thread::sleep(Duration::from_millis(50)); // give the receiver a real chance to block
        SEND_SEQ.store(SEQ.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
        tx.send_blocking(42);

        let got = receiver.join().unwrap();
        chk!(got, Some(42));
        // The send must have happened (sequenced) strictly before the post-recv store.
        chk!((SEND_SEQ.load(Ordering::SeqCst) < RECV_SEQ.load(Ordering::SeqCst)), true);
    }

    // ---------- 4. Multi-producer: several OS threads sending into ONE unbounded channel ----------
    // Every rooted clone targets the same managed writer, and every item from every producer is
    // observed by the single consumer exactly once.
    {
        const PRODUCERS: i64 = 8;
        const PER_PRODUCER: i64 = 500;
        let (tx, rx) = channel::<i64>();

        let producers: Vec<_> = (0..PRODUCERS)
            .map(|p| {
                let tx = tx.clone();
                thread::spawn(move || {
                    for i in 0..PER_PRODUCER {
                        // Encode (producer, index) into one i64 so the consumer can verify FIFO
                        // ordering PER PRODUCER even though producers interleave.
                        tx.send_blocking(p * 1_000_000 + i);
                    }
                })
            })
            .collect();

        let mut last_seen = [-1i64; PRODUCERS as usize];
        let mut received: i64 = 0;
        while received < PRODUCERS * PER_PRODUCER {
            if let Some(v) = rx.recv_blocking() {
                let p = (v / 1_000_000) as usize;
                let i = v % 1_000_000;
                // Per-producer ordering must be preserved (each producer's own sends are a queue).
                if i != last_seen[p] + 1 {
                    Console::writeln_u64(900_000_100); // distinct marker for an ordering violation
                }
                last_seen[p] = i;
                received += 1;
            }
        }
        for p in producers {
            p.join().unwrap();
        }
        chk!(received, PRODUCERS * PER_PRODUCER);
        chk!(last_seen.iter().all(|&last| last == PER_PRODUCER - 1), true);
    }

    // ---------- 5. Multi-consumer: several OS threads competing for items from ONE channel ----------
    // Each item must go to EXACTLY ONE consumer -- proven by summing a shared atomic counter of
    // total items actually received across every consumer thread.
    {
        const CONSUMERS: usize = 4;
        const TOTAL_ITEMS: i64 = 2000;
        let (tx, rx) = channel::<i64>();
        static RECEIVED_COUNT: AtomicI64 = AtomicI64::new(0);
        RECEIVED_COUNT.store(0, Ordering::SeqCst);
        static SUM: AtomicI64 = AtomicI64::new(0);
        SUM.store(0, Ordering::SeqCst);

        for i in 0..TOTAL_ITEMS {
            tx.try_send(i).unwrap();
        }
        tx.close(); // no more items are coming -- lets consumers observe definite drain

        let consumers: Vec<_> = (0..CONSUMERS)
            .map(|_| {
                let rx = rx.clone();
                thread::spawn(move || loop {
                    match rx.recv_blocking() {
                        Some(v) => {
                            RECEIVED_COUNT.fetch_add(1, Ordering::SeqCst);
                            SUM.fetch_add(v, Ordering::SeqCst);
                        }
                        None => break, // channel closed and drained
                    }
                })
            })
            .collect();
        for c in consumers {
            c.join().unwrap();
        }

        chk!(RECEIVED_COUNT.load(Ordering::SeqCst), TOTAL_ITEMS);
        // Each item observed EXACTLY once (no duplication, no loss): sum must match 0+1+..+(N-1).
        chk!(SUM.load(Ordering::SeqCst), (0..TOTAL_ITEMS).sum::<i64>());
    }

    // ---------- 6. Bounded channel: backpressure -- try_send fails once full ----------
    {
        let (tx, rx) = bounded_channel::<i32>(2);
        chk!(tx.try_send(1), Ok(()));
        chk!(tx.try_send(2), Ok(()));
        chk!(tx.try_send(3), Err(3)); // full -- capacity 2, item handed back
        chk!(rx.try_recv(), Some(1)); // free a slot
        chk!(tx.try_send(3), Ok(())); // now it fits
        chk!(rx.try_recv(), Some(2));
        chk!(rx.try_recv(), Some(3));
    }

    // ---------- 7. Bounded channel: send_blocking on a full channel really blocks for room ----------
    {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        static FULL_SEND_SEQ: AtomicU32 = AtomicU32::new(0);
        static RECV_SEQ: AtomicU32 = AtomicU32::new(0);

        let (tx, rx) = bounded_channel::<i32>(1);
        tx.try_send(1).unwrap(); // fill capacity

        let sender = thread::spawn(move || {
            tx.send_blocking(2); // must block until the main thread makes room
            FULL_SEND_SEQ.store(SEQ.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
        });

        thread::sleep(Duration::from_millis(50)); // let the sender thread really block
        RECV_SEQ.store(SEQ.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
        let first = rx.recv_blocking(); // frees the slot -> unblocks the sender
        sender.join().unwrap();

        chk!(first, Some(1));
        chk!(rx.recv_blocking(), Some(2));
        // The freeing recv must have happened strictly before the blocked send's post-send store.
        chk!((RECV_SEQ.load(Ordering::SeqCst) < FULL_SEND_SEQ.load(Ordering::SeqCst)), true);
    }

    // ---------- 8. Async send/recv via task::block_on, and mixed blocking<->async on either end ----------
    {
        use mycorrhiza::task::block_on;
        let (tx, rx) = channel::<i32>();

        block_on(async {
            tx.send_async(7).await;
            tx.send_async(8).await;
        });
        let a = block_on(rx.recv_async());
        let b = block_on(rx.recv_async());
        chk!(a, Some(7));
        chk!(b, Some(8));

        // Cross-thread: producer thread uses the blocking API, this thread awaits asynchronously.
        let tx2 = tx;
        let producer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            tx2.send_blocking(99);
        });
        let got = block_on(rx.recv_async());
        producer.join().unwrap();
        chk!(got, Some(99));
    }

    // ---------- 9. close()/recv semantics: draining then None, not an exception ----------
    {
        let (tx, rx) = channel::<i32>();
        tx.try_send(1).unwrap();
        chk!(tx.close(), true); // first close succeeds
        chk!(tx.close(), false); // second close is a no-op (already completed)
        // Buffered item is still readable after close -- close only stops NEW writes.
        chk!(rx.try_recv(), Some(1));
        chk!(rx.recv_blocking(), None); // now drained AND closed -> None, no throw
        chk!(rx.try_recv(), None);
    }

    // ---------- 10. raw() round-trips to the SAME underlying managed object (cross-language proof) ----------
    // Hand out the raw ChannelWriter<T>/ChannelReader<T> handles and drive the channel purely through
    // them -- this is exactly the shape a #[dotnet_export] would expose to a genuine C# caller, so
    // proving it here proves the handle is the real, shared, two-way-usable object.
    {
        let (tx, rx) = channel::<i32>();
        let raw_w = tx.raw();
        let raw_r = rx.raw();
        let tx_from_raw = mycorrhiza::sync::Sender::from_raw(raw_w);
        let rx_from_raw = mycorrhiza::sync::Receiver::from_raw(raw_r);
        tx_from_raw.try_send(123).unwrap();
        chk!(rx_from_raw.try_recv(), Some(123));
        // And the original handles still observe the SAME object (not a copy of state).
        tx.try_send(456).unwrap();
        chk!(rx_from_raw.try_recv(), Some(456));
    }

    println!("== cd_channel done ==");
    println!("pass");
    Console::writeln_u64(pass as u64);
    println!("total");
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
