//! H2 real-crate SOAK: tinyvec on the dotnet PAL.
//! Exercises TinyVec inline storage (Array2 backed) then spill-to-heap when it
//! outgrows the inline array; push, extend, iterate, sum, sort, drain. Also
//! covers ArrayVec (pure-inline, no heap). Panic-safe: no unwraps, valid inputs,
//! bounded indices via .get(), Result/Option handled.
//! SUCCESS = "== soak_tinyvec done ==" with sane values.
use tinyvec::{array_vec, tiny_vec, ArrayVec, TinyVec};

fn main() {
    println!("== soak_tinyvec start ==");

    // 1. TinyVec starting inline (capacity 4), staying inline.
    let mut tv: TinyVec<[i32; 4]> = tiny_vec![1, 2, 3];
    println!("1  inline len={} cap-array=4 is_heap={}", tv.len(), tv.is_heap());

    // 2. Push past the inline capacity -> spill to heap.
    for n in 4..=10 {
        tv.push(n);
    }
    println!("2  after spill len={} is_heap={}", tv.len(), tv.is_heap());

    // 3. Iterate + sum (exercises Iterator impl over the spilled backing).
    let sum: i32 = tv.iter().copied().sum();
    println!("3  sum={}", sum);

    // 4. Indexed read via get (panic-safe).
    match tv.get(7) {
        Some(v) => println!("4  tv[7]={}", v),
        None => println!("4  tv[7]=<none>"),
    }

    // 5. Sort in reverse, then re-read first few.
    tv.sort_unstable_by(|a, b| b.cmp(a));
    let head: Vec<i32> = tv.iter().take(3).copied().collect();
    println!("5  sorted-desc head={:?}", head);

    // 6. Drain a range back into a plain Vec.
    let drained: Vec<i32> = tv.drain(0..3).collect();
    println!("6  drained={:?} remaining_len={}", drained, tv.len());

    // 7. ArrayVec: pure inline, fill to capacity then try_push past it.
    let mut av: ArrayVec<[u8; 4]> = array_vec![10, 20, 30];
    av.push(40);
    let overflow = av.try_push(50); // should return Some(50) (full), no panic
    println!(
        "7  arrayvec len={} cap={} overflow_back={:?}",
        av.len(),
        av.capacity(),
        overflow
    );

    // 8. extend_from_slice on a fresh TinyVec of a non-Copy-ish small type.
    let mut sv: TinyVec<[u16; 2]> = TinyVec::new();
    sv.extend_from_slice(&[100, 200, 300, 400]); // forces spill
    let prod: u32 = sv.iter().map(|&x| x as u32).product();
    println!("8  extend len={} is_heap={} product={}", sv.len(), sv.is_heap(), prod);

    println!("== soak_tinyvec done ==");
}
