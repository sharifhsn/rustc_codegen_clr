//! H2 real-crate SOAK: arrayvec on the dotnet PAL.
//! ArrayVec<i32, 8> push/extend/iter within capacity, then len+sum. Exercises arrayvec's
//! MaybeUninit-backed inline storage / union layout, Deref-to-slice, IntoIter, try_push.
//! Panic-safe: all pushes stay within cap, no unwraps/indexing that could fault.
//! SUCCESS = "== soak_arrayvec done ==" with sane values.
use arrayvec::ArrayVec;

fn main() {
    println!("== soak_arrayvec start ==");

    let mut av: ArrayVec<i32, 8> = ArrayVec::new();
    println!("1  cap={}", av.capacity());

    // push within capacity
    for i in 1..=4 {
        // try_push returns Result; ignore overflow safely (won't happen here)
        let _ = av.try_push(i);
    }
    println!("2  after pushes len={}", av.len());

    // extend within remaining capacity (4 + 4 = 8 == cap)
    av.extend([5, 6, 7, 8]);
    println!("3  after extend len={}", av.len());

    // iterate / sum via Deref-to-slice
    let sum: i32 = av.iter().copied().sum();
    println!("4  sum={}", sum);

    // is_full / remaining_capacity
    println!("5  is_full={} remaining={}", av.is_full(), av.remaining_capacity());

    // pop a couple, observe len + last
    let popped = av.pop();
    println!("6  popped={:?} len={}", popped, av.len());

    // IntoIter consume + recompute sum
    let sum2: i32 = av.into_iter().sum();
    println!("7  consumed sum={}", sum2);

    // try_push past capacity should return Err, not panic
    let mut small: ArrayVec<u8, 2> = ArrayVec::new();
    let _ = small.try_push(10);
    let _ = small.try_push(20);
    let over = small.try_push(30);
    println!("8  overflow handled is_err={}", over.is_err());

    println!("== soak_arrayvec done ==");
}
