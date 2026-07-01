// The END-USER experience of the .NET generic collections: `mycorrhiza::collections` used exactly
// like `std` — `List::new()`, `.push()`, `.get()`, `for x in .iter()`, `Dictionary::insert`, etc. No
// `get_Item`, no `!0` definition-shapes, no `callvirt`, no assembly strings. Compare this file to
// `cd_generic` (the low-level bridge it is built on) to see the ergonomics delta.
//
// Every result is checked in-Rust; `main` prints `pass` then `total` (a `9000000xx` marker flags any
// failing check) and returns non-zero on any mismatch.
#![allow(dead_code)]

use mycorrhiza::collections::{Dictionary, HashSet, List, Queue, Stack};
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

    // ---------- List<i32> ----------
    let mut xs = List::<i32>::new();
    for i in 0..5i32 {
        xs.push(i * 10); // 0,10,20,30,40
    }
    chk!(xs.len(), 5);
    chk!(xs.get(0), Some(0));
    chk!(xs.get(4), Some(40));
    chk!(xs.get(5), None); // out of range → None (bounds-checked)
    chk!(xs.contains(30), true);
    chk!(xs.contains(999), false);
    chk!(xs.index_of(20), 2);
    chk!(xs.set(0, 100), true);
    chk!(xs.get(0), Some(100));
    let mut sum = 0i32;
    for v in xs.iter() {
        sum += v; // 100+10+20+30+40
    }
    chk!(sum, 200);
    chk!(xs.remove_at(0), true);
    chk!(xs.len(), 4);
    chk!(xs.get(0), Some(10));

    // ---------- Dictionary<i32, i64> ----------
    let mut m = Dictionary::<i32, i64>::new();
    m.insert(1, 100);
    m.insert(2, 200);
    m.insert(1, 111); // overwrite (never throws)
    chk!(m.len(), 2);
    chk!(m.get(1), Some(111));
    chk!(m.get(2), Some(200));
    chk!(m.get(99), None); // absent → None (no exception)
    chk!(m.contains_key(2), true);
    chk!(m.remove(2), true);
    chk!(m.contains_key(2), false);
    chk!(m.len(), 1);

    // ---------- HashSet<i32> ----------
    let mut s = HashSet::<i32>::new();
    chk!(s.insert(5), true);
    chk!(s.insert(5), false); // duplicate
    chk!(s.insert(7), true);
    chk!(s.len(), 2);
    chk!(s.contains(5), true);
    chk!(s.remove(5), true);
    chk!(s.contains(5), false);

    // ---------- Stack<i32> (LIFO) ----------
    let mut st = Stack::<i32>::new();
    st.push(1);
    st.push(2);
    st.push(3);
    chk!(st.len(), 3);
    chk!(st.peek(), Some(3));
    chk!(st.pop(), Some(3));
    chk!(st.pop(), Some(2));
    chk!(st.len(), 1);

    // ---------- Queue<i32> (FIFO) ----------
    let mut q = Queue::<i32>::new();
    q.enqueue(10);
    q.enqueue(20);
    q.enqueue(30);
    chk!(q.peek(), Some(10));
    chk!(q.dequeue(), Some(10));
    chk!(q.dequeue(), Some(20));
    chk!(q.len(), 1);
    let mut empty = Queue::<i32>::new();
    chk!(empty.dequeue(), None); // empty → None (no exception)

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
