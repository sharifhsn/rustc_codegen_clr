// Regression probe for the DST-tailed-struct pointer-metadata bug (gaps-campaign / I2-at-scale).
// `PtrMetadata` dispatched on the pointee's own `TyKind`, so a struct with an unsized tail
// (`UnsafeCell<[i32]>` — kind() == Adt, tail == [i32]) fell through to a synthetic `Void` metadata
// instead of the slice length. `<[i32]>::len()`/slice-eq then read a Void (verifier:
// CantCompareTypes USize/Void; runtime: zeroed length). Fixed by dispatching on the struct tail.
// Surfaced by rust-lang/rust coretests `cells::unsafe_cell_unsized`. Output must match native.
use std::cell::UnsafeCell;

fn main() {
    // unsafe_cell_unsized, the coretests case
    let cell: &UnsafeCell<[i32]> = &UnsafeCell::new([1, 2, 3]);
    {
        let val: &mut [i32] = unsafe { &mut *cell.get() };
        val[0] = 4;
        val[2] = 5;
    }
    let comp: &mut [i32] = &mut [4, 2, 5];
    println!("eq={}", unsafe { &mut *cell.get() } == comp);
    // exercise the metadata (length) directly through the DST-tailed struct
    let s: &[i32] = unsafe { &*cell.get() };
    println!("len={} sum={}", s.len(), s.iter().sum::<i32>());
}
