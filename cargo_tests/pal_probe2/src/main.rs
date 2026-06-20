//! H2 codegen frontier probe #2 — richer std surface real crates hit (trait objects, Rc/RefCell,
//! error handling, more collections, closures, iterator chains), beyond pal_probe's core set.
//! Panic-abort-safe: no deliberate panics. Each line that prints is a working subsystem; the first
//! missing line (or a hard crash) localizes the next codegen gap.
use std::cell::RefCell;
use std::collections::{BTreeSet, BinaryHeap, HashSet, VecDeque};
use std::fmt;
use std::rc::Rc;

trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> &str;
}
struct Circle(f64);
struct Square(f64);
impl Shape for Circle {
    fn area(&self) -> f64 { std::f64::consts::PI * self.0 * self.0 }
    fn name(&self) -> &str { "circle" }
}
impl Shape for Square {
    fn area(&self) -> f64 { self.0 * self.0 }
    fn name(&self) -> &str { "square" }
}

#[derive(Debug)]
struct MyErr(String);
impl fmt::Display for MyErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "MyErr: {}", self.0) }
}
impl std::error::Error for MyErr {}

fn parse_sum(items: &[&str]) -> Result<i32, Box<dyn std::error::Error>> {
    let mut s = 0;
    for it in items {
        s += it.parse::<i32>()?;
    }
    Ok(s)
}

fn main() {
    println!("== pal_probe2 start ==");

    // 1 trait objects / dynamic dispatch
    let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Circle(1.0)), Box::new(Square(2.0))];
    let total: f64 = shapes.iter().map(|s| s.area()).sum();
    println!("1  dyn dispatch:     n={} total_area={:.3} first={}", shapes.len(), total, shapes[0].name());

    // 2 Rc + RefCell (shared mutable, ref counting)
    let shared = Rc::new(RefCell::new(vec![1, 2, 3]));
    let clone = Rc::clone(&shared);
    clone.borrow_mut().push(4);
    println!("2  rc/refcell:       sum={} rc_count={}", shared.borrow().iter().sum::<i32>(), Rc::strong_count(&shared));

    // 3 error handling: ? operator, Box<dyn Error>, custom Display
    println!("3a parse_sum ok:     {:?}", parse_sum(&["1", "2", "3"]));
    println!("3b parse_sum err:    {}", parse_sum(&["1", "x"]).map_err(|e| e.to_string()).unwrap_err());
    let custom: Result<(), MyErr> = Err(MyErr("boom".into()));
    println!("3c custom display:   {}", custom.unwrap_err());

    // 4 more collections
    let mut vd: VecDeque<i32> = VecDeque::new();
    vd.push_back(1);
    vd.push_front(0);
    let mut heap = BinaryHeap::new();
    for x in [3, 1, 2] { heap.push(x); }
    let hs: HashSet<i32> = [1, 2, 2, 3].into_iter().collect();
    let bs: BTreeSet<i32> = [3, 1, 2, 1].into_iter().collect();
    println!("4  collections:      vd={:?} heap_max={:?} hashset_len={} btreeset={:?}", vd, heap.peek(), hs.len(), bs);

    // 5 closures: env capture, FnMut, boxed Fn
    let mut counter = 0;
    let mut inc = || { counter += 1; counter };
    let _ = inc();
    let c = inc();
    let adder: Box<dyn Fn(i32) -> i32> = { let base = 10; Box::new(move |x| x + base) };
    println!("5  closures:         fnmut={} boxed_fn={}", c, adder(5));

    // 6 iterator chains
    let evens_sq: Vec<i32> = (1..=10).filter(|x| x % 2 == 0).map(|x| x * x).collect();
    let folded = (1..=5).fold(0, |a, b| a + b);
    let zipped: Vec<(i32, char)> = (1..=3).zip(['a', 'b', 'c']).collect();
    let flat: Vec<i32> = vec![vec![1, 2], vec![3, 4]].into_iter().flatten().collect();
    println!("6  iterators:        evens_sq={:?} fold={} zip={:?} flat={:?}", evens_sq, folded, zipped, flat);

    // 7 string ops
    let words: Vec<&str> = "the quick brown fox".split(' ').collect();
    let upper = "hello".to_uppercase();
    let csv_sum: i32 = "1,2,3,4".split(',').map(|s| s.parse::<i32>().unwrap()).sum();
    println!("7  strings:          words={} upper={} csv_sum={}", words.len(), upper, csv_sum);

    // 8 sorting with closures
    let mut v = vec![(3, "c"), (1, "a"), (2, "b")];
    v.sort_by_key(|&(k, _)| k);
    v.sort_by(|a, b| b.0.cmp(&a.0));
    println!("8  sort:             {:?}", v);

    println!("== pal_probe2 done ==");
}
