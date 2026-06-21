//! Minimal repro for the regex crash: building a smart pointer over an unsized [u8] slice
//! (Arc<[u8]>/Rc<[u8]>/Box<[u8]> from &[u8] — DST unsized coercion + fat-pointer alloc + copy).
//! Panic-safe (valid indices only). SUCCESS = "== pal_arc done ==".
use std::rc::Rc;
use std::sync::Arc;

fn main() {
    println!("== pal_arc start ==");
    let s: &[u8] = b"hello world";

    let a: Arc<[u8]> = Arc::from(s);
    println!("1  Arc<[u8]>::from:  len={} first={} last={}", a.len(), a[0], a[a.len() - 1]);

    let a2: Arc<[u8]> = s.to_vec().into();
    println!("2  Vec->Arc<[u8]>:   len={} sum={}", a2.len(), a2.iter().map(|&b| b as u32).sum::<u32>());

    let rc: Rc<[u8]> = Rc::from(s);
    println!("3  Rc<[u8]>::from:   len={} first={}", rc.len(), rc[0]);

    let boxed: Box<[u8]> = Box::from(s);
    let blen = boxed.len();
    println!("4a Box<[u8]>::from:  len={}", blen);
    println!("4  Box<[u8]>::from:  first={} last={}", boxed[0], boxed[blen - 1]);

    // Arc<str> too (str is also a DST)
    let astr: Arc<str> = Arc::from("a string");
    println!("5  Arc<str>::from:   len={} starts_a={}", astr.len(), astr.starts_with('a'));

    println!("== pal_arc done ==");
}
