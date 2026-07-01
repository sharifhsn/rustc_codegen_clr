// The ENUMERATOR BRIDGE (Theme-1 keystone): iterate any .NET collection through its
// `IEnumerator<T>` as a Rust `impl Iterator` — `for x in &list`, `.collect()`, `.extend()`.
//
// This exercises by-REFERENCE iteration (`&List`/`&HashSet`/`&Stack`/`&Queue`) which drives the
// managed `GetEnumerator`/`MoveNext`/`get_Current` loop on the interface path, plus `FromIterator`
// (`collect`) and `Extend` for `List` and `HashSet`.
//
// Every result is checked in-Rust; `main` prints `pass` then `total` (a `9000000xx` marker flags any
// failing check) and returns non-zero on any mismatch.
#![allow(dead_code)]

use mycorrhiza::prelude::*;
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

    // ---------- List<i32>: for x in &list (enumerator-backed) ----------
    let xs: List<i32> = std::vec![10, 20, 30, 40].into();
    let mut sum = 0i32;
    let mut count = 0i32;
    for x in &xs {
        sum += x;
        count += 1;
    }
    chk!(sum, 100);
    chk!(count, 4);
    // Iterate twice — a fresh enumerator each time (no consumption of the collection).
    let mut sum2 = 0i32;
    for x in &xs {
        sum2 += x;
    }
    chk!(sum2, 100);
    chk!(xs.len(), 4); // the list itself is untouched by iteration
    // Compose with Iterator adapters over &List.
    let doubled: std::vec::Vec<i32> = (&xs).into_iter().map(|v| v * 2).collect();
    chk!(doubled, std::vec![20, 40, 60, 80]);
    let max = (&xs).into_iter().max();
    chk!(max, Some(40));

    // Empty list: the enumerator yields nothing.
    let empty: List<i32> = std::vec![].into();
    let mut ecount = 0i32;
    for _ in &empty {
        ecount += 1;
    }
    chk!(ecount, 0);

    // ---------- List: collect (FromIterator) and extend ----------
    let l: List<i32> = (0..5).collect(); // 0,1,2,3,4
    chk!(l.len(), 5);
    chk!(l.to_vec(), std::vec![0, 1, 2, 3, 4]);
    let via_enum: std::vec::Vec<i32> = (&l).into_iter().collect();
    chk!(via_enum, std::vec![0, 1, 2, 3, 4]);
    let mut l2: List<i32> = std::vec![1, 2].into();
    l2.extend(&xs); // extend from another List's by-ref iterator -> [1,2,10,20,30,40]
    chk!(l2.to_vec(), std::vec![1, 2, 10, 20, 30, 40]);

    // ---------- HashSet<i32>: iteration + collect + extend ----------
    let hs: HashSet<i32> = std::vec![1, 2, 3, 3, 2].into_iter().collect(); // dedup -> {1,2,3}
    chk!(hs.len(), 3);
    // Enumerate and sum (order is the set's internal order; the sum is order-independent).
    let mut hsum = 0i32;
    let mut hcount = 0i32;
    for x in &hs {
        hsum += x;
        hcount += 1;
    }
    chk!(hsum, 6);
    chk!(hcount, 3);
    // Every enumerated element is actually a member.
    let mut all_members = true;
    for x in &hs {
        if !hs.contains(x) {
            all_members = false;
        }
    }
    chk!(all_members, true);
    let mut hs2: HashSet<i32> = HashSet::new();
    hs2.extend(0..4); // {0,1,2,3}
    chk!(hs2.len(), 4);
    chk!(hs2.contains(3), true);

    // ---------- Stack<i32>: LIFO enumeration (top first) ----------
    let mut st = Stack::<i32>::new();
    st.push(1);
    st.push(2);
    st.push(3);
    let stack_order: std::vec::Vec<i32> = (&st).into_iter().collect();
    chk!(stack_order, std::vec![3, 2, 1]); // LIFO
    chk!(st.len(), 3); // untouched

    // ---------- Queue<i32>: FIFO enumeration (front first) ----------
    let mut q = Queue::<i32>::new();
    q.enqueue(10);
    q.enqueue(20);
    q.enqueue(30);
    let queue_order: std::vec::Vec<i32> = (&q).into_iter().collect();
    chk!(queue_order, std::vec![10, 20, 30]); // FIFO
    chk!(q.len(), 3); // untouched

    // ---------- element type i64 (a wider primitive through the enumerator) ----------
    let wide: List<i64> = std::vec![1_000_000_000_000i64, 2_000_000_000_000i64].into();
    let mut wsum = 0i64;
    for x in &wide {
        wsum += x;
    }
    chk!(wsum, 3_000_000_000_000i64);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
