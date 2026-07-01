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

// Callbacks for `List::sort_by` / `for_each` (delegate-as-generic-method-arg).
extern "C" fn cmp_i32(a: i32, b: i32) -> i32 {
    a - b
}
static mut FE_ACC: i32 = 0;
extern "C" fn fe_add(x: i32) {
    unsafe { FE_ACC += x }
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

    // ---------- SortedDictionary<i32, i64> (key-ordered) ----------
    let mut sd = SortedDictionary::<i32, i64>::new();
    sd.insert(3, 300);
    sd.insert(1, 100);
    sd.insert(2, 200);
    sd.insert(1, 111); // overwrite
    chk!(sd.len(), 3);
    chk!(sd.get(1), Some(111));
    chk!(sd.get(2), Some(200));
    chk!(sd.get(99), None);
    chk!(sd.contains_key(3), true);
    chk!(sd.get_or_default(2, -1), 200);
    chk!(sd.get_or_default(9, -1), -1);
    chk!(sd.remove(2), true);
    chk!(sd.remove(2), false); // already gone
    chk!(sd.contains_key(2), false);
    chk!(sd.len(), 2);

    // ---------- SortedSet<i32> (ordered; iterates ascending) ----------
    let mut ss = SortedSet::<i32>::new();
    chk!(ss.insert(30), true);
    chk!(ss.insert(10), true);
    chk!(ss.insert(20), true);
    chk!(ss.insert(10), false); // duplicate
    chk!(ss.len(), 3);
    chk!(ss.contains(20), true);
    chk!(ss.contains(999), false);
    // Enumeration yields ascending order (10,20,30).
    let mut sorted = std::vec::Vec::new();
    for v in &ss {
        sorted.push(v);
    }
    chk!(sorted, std::vec![10, 20, 30]);
    chk!(ss.remove(20), true);
    chk!(ss.contains(20), false);
    // FromIterator / Extend
    let mut ss2: SortedSet<i32> = std::vec![5, 1, 3].into_iter().collect();
    ss2.extend(std::vec![2, 4]);
    let mut ss2v = std::vec::Vec::new();
    for v in &ss2 {
        ss2v.push(v);
    }
    chk!(ss2v, std::vec![1, 2, 3, 4, 5]);

    // ---------- LinkedList<i32> ----------
    let mut ll = LinkedList::<i32>::new();
    ll.push_back(1);
    ll.push_back(2);
    ll.push_back(3);
    chk!(ll.len(), 3);
    chk!(ll.contains(2), true);
    chk!(ll.contains(99), false);
    let mut llv = std::vec::Vec::new();
    for v in &ll {
        llv.push(v); // front-to-back
    }
    chk!(llv, std::vec![1, 2, 3]);
    chk!(ll.remove(2), true);
    chk!(ll.remove(2), false);
    let mut llv2 = std::vec::Vec::new();
    for v in &ll {
        llv2.push(v);
    }
    chk!(llv2, std::vec![1, 3]);
    // FromIterator
    let ll2: LinkedList<i32> = std::vec![7, 8, 9].into_iter().collect();
    chk!(ll2.len(), 3);

    // ---------- PriorityQueue<i32, i32> (min-priority) ----------
    let mut pq = PriorityQueue::<i32, i32>::new();
    pq.enqueue(100, 5); // element 100 with priority 5
    pq.enqueue(200, 1); // priority 1 -> dequeues first
    pq.enqueue(300, 3);
    chk!(pq.len(), 3);
    chk!(pq.peek(), Some(200)); // lowest priority (1)
    chk!(pq.dequeue(), Some(200));
    chk!(pq.dequeue(), Some(300)); // priority 3 next
    chk!(pq.dequeue(), Some(100)); // priority 5 last
    chk!(pq.dequeue(), None); // empty -> None
    chk!(pq.peek(), None);
    chk!(pq.is_empty(), true);

    // ---------- ConcurrentDictionary<i32, i64> ----------
    let mut cd = ConcurrentDictionary::<i32, i64>::new();
    chk!(cd.is_empty(), true);
    cd.insert(1, 100);
    chk!(cd.try_add(1, 999), false); // key exists -> not added, not overwritten
    chk!(cd.get(1), Some(100)); // still 100
    chk!(cd.try_add(2, 200), true); // new key -> added
    chk!(cd.get(2), Some(200));
    chk!(cd.get(99), None);
    chk!(cd.contains_key(1), true);
    chk!(cd.len(), 2);
    chk!(cd.is_empty(), false);
    chk!(cd.get_or_default(2, -1), 200);
    chk!(cd.get_or_default(9, -1), -1);
    cd.clear();
    chk!(cd.is_empty(), true);

    // ---------- ConcurrentQueue<i32> (produce then drain-by-iteration) ----------
    let mut cq = ConcurrentQueue::<i32>::new();
    chk!(cq.is_empty(), true);
    cq.enqueue(10);
    cq.enqueue(20);
    cq.enqueue(30);
    chk!(cq.len(), 3);
    chk!(cq.is_empty(), false);
    let mut cqv = std::vec::Vec::new();
    for v in &cq {
        cqv.push(v); // FIFO snapshot
    }
    chk!(cqv, std::vec![10, 20, 30]);

    // ---------- ConcurrentBag<i32> (add then drain-by-iteration) ----------
    let mut cb = ConcurrentBag::<i32>::new();
    chk!(cb.is_empty(), true);
    cb.add(1);
    cb.add(2);
    cb.add(3);
    chk!(cb.len(), 3);
    chk!(cb.is_empty(), false);
    // Unordered, so check via a sum (order not guaranteed).
    let mut bagsum = 0i32;
    for v in &cb {
        bagsum += v;
    }
    chk!(bagsum, 6);

    // ===== Dictionary entry iteration (`for (k, v) in &dict`) — value-type-generic KeyValuePair =====
    let mut dm: Dictionary<i32, i64> = Dictionary::new();
    dm.insert(1, 100);
    dm.insert(2, 200);
    dm.insert(3, 300);
    let (mut ksum, mut vsum, mut n) = (0i32, 0i64, 0i32);
    for (k, v) in &dm {
        ksum += k;
        vsum += v;
        n += 1;
    }
    chk!(n, 3);
    chk!(ksum, 6); // 1+2+3
    chk!(vsum, 600i64); // 100+200+300
    // `.iter()` yields the same and composes with Iterator adapters.
    chk!(dm.iter().map(|(_, v)| v).sum::<i64>(), 600i64);
    chk!(dm.iter().find(|&(k, _)| k == 2).map(|(_, v)| v), Some(200i64));

    // ===== List.sort_by / for_each — delegate as a generic-method argument (Comparison/Action<T>) =====
    let mut sl: List<i32> = List::new();
    sl.push(30);
    sl.push(10);
    sl.push(20);
    sl.sort_by(cmp_i32); // ascending
    chk!(sl.get(0), Some(10));
    chk!(sl.get(1), Some(20));
    chk!(sl.get(2), Some(30));
    unsafe { FE_ACC = 0 };
    sl.for_each(fe_add); // .NET ForEach drives the Rust callback
    chk!(unsafe { FE_ACC }, 60); // 10+20+30

    // ===== Dictionary.keys() / values() (over the working entry iteration) =====
    chk!(dm.keys().sum::<i32>(), 6); // 1+2+3
    chk!(dm.values().sum::<i64>(), 600i64); // 100+200+300

    // ===== LinkedList.push_front (AddFirst -> LinkedListNode<T>, nested-generic return) =====
    let mut ll: LinkedList<i32> = LinkedList::new();
    ll.push_back(2);
    ll.push_front(1); // [1, 2]
    ll.push_back(3); // [1, 2, 3]
    chk!(ll.len(), 3);
    let order: std::vec::Vec<i32> = (&ll).into_iter().collect();
    chk!(order, std::vec![1, 2, 3]);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
