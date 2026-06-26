// Subslice get on a fixed array: `[first, rest @ ..]` on `[T; 4]` -> rest is [T;3],
// projection Subslice{from:1, to:4, from_end:false}. Must match native.
use std::hint::black_box;
fn main() {
    let arr = black_box([10i32, 20, 30, 40]);
    let [first, rest @ ..] = arr;
    println!("first={first} rest={:?} sum={}", rest, rest.iter().sum::<i32>());
    // and a tail-fixed variant (from_end) for good measure
    let [head @ .., last] = black_box([1u8, 2, 3, 4, 5]);
    println!("head={:?} last={last}", head);
}
