// The END-USER experience of the .NET generic collections: `mycorrhiza::collections` used exactly
// like `std` — `List::new()`, `.push()`, `.get()`, `for x in .iter()`, `Dictionary::insert`, etc. No
// `get_Item`, no `!0` definition-shapes, no `callvirt`, no assembly strings. Compare this file to
// `cd_generic` (the low-level bridge it is built on) to see the ergonomics delta.
//
// Every result is checked in-Rust; `main` prints `pass` then `total` (a `9000000xx` marker flags any
// failing check) and returns non-zero on any mismatch.
#![allow(dead_code)]

// One-glance import: the prelude brings the collections + `DotNetString` into scope like `std`.
use mycorrhiza::prelude::*;
use mycorrhiza::system::console::Console;

// A tiny helper so the Hash checks read cleanly.
fn hash_of<T: core::hash::Hash>(v: &T) -> u64 {
    use core::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

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

    // ---------- List conveniences: first/last/pop/sort/reverse/to_vec/from_slice ----------
    let mut cv = List::<i32>::from_slice(&[3, 1, 2]);
    chk!(cv.len(), 3);
    chk!(cv.first(), Some(3));
    chk!(cv.last(), Some(2));
    cv.sort(); // ascending -> [1,2,3]
    chk!(cv.to_vec(), std::vec![1, 2, 3]);
    chk!(cv.first(), Some(1));
    chk!(cv.last(), Some(3));
    cv.reverse(); // -> [3,2,1]
    chk!(cv.to_vec(), std::vec![3, 2, 1]);
    chk!(cv.pop(), Some(1)); // pops the last element
    chk!(cv.to_vec(), std::vec![3, 2]);
    let mut ecv = List::<i32>::new();
    chk!(ecv.first(), None); // empty
    chk!(ecv.last(), None);
    chk!(ecv.pop(), None);

    // ---------- List std traits: From<Vec> / FromIterator / Extend / PartialEq / Clone / Hash ----------
    let a: List<i32> = std::vec![1, 2, 3].into(); // From<Vec<T>>
    let b: List<i32> = (1..=3).collect(); // FromIterator
    chk!((a == b), true); // element-wise PartialEq (NOT reference identity)
    chk!(a.to_vec(), std::vec![1, 2, 3]);
    let mut c: List<i32> = std::vec![1, 2].into();
    c.extend(3..=4); // Extend -> [1,2,3,4]
    chk!(c.to_vec(), std::vec![1, 2, 3, 4]);
    let different: List<i32> = std::vec![1, 2, 4].into();
    chk!((a == different), false);
    // Clone is a DEEP copy — mutating the clone must not touch the original.
    let mut cloned = a.clone();
    chk!((cloned == a), true);
    cloned.push(99);
    chk!((cloned == a), false); // independence
    chk!(a.to_vec(), std::vec![1, 2, 3]); // original untouched
    // Hash is consistent with element-wise PartialEq: equal lists -> equal hashes.
    chk!((hash_of(&a) == hash_of(&b)), true);

    // ---------- Dictionary::get_or_default ----------
    let mut dm = Dictionary::<i32, i64>::new();
    dm.insert(1, 100);
    chk!(dm.get_or_default(1, -1), 100); // present
    chk!(dm.get_or_default(2, -1), -1); // absent -> default
    chk!(dm.contains_key(2), false); // get_or_default does NOT insert

    // ---------- DotNetString: Display / Debug / PartialEq / Eq / Hash / round-trip ----------
    let s1 = DotNetString::from("hello");
    let s2 = DotNetString::from("hello");
    let s3 = DotNetString::from("world");
    chk!((s1 == s2), true); // content equality (op_Equality)
    chk!((s1 == s3), false);
    chk!(std::format!("{}", s1).as_str(), "hello"); // Display round-trips the content
    chk!(std::format!("{:?}", s1).as_str(), "\"hello\""); // Debug quotes it
    chk!(s1.to_rust_string().as_str(), "hello"); // explicit marshal-back
    chk!(s1.len_utf16(), 5);
    // GetHashCode is content-based: equal strings -> equal Rust hashes.
    chk!((hash_of(&s1) == hash_of(&s2)), true);
    // A non-ASCII round-trip (multi-byte UTF-8 -> UTF-16 -> back).
    let u = DotNetString::from("héllo");
    chk!(u.to_rust_string().as_str(), "héllo");
    chk!(std::format!("{}", u).as_str(), "héllo");

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
