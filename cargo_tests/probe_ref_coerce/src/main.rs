// Regression probe for the struct-unsizing trailing-field-copy bug (gaps-campaign / I2). `unsize`
// read `target_size` from `layout_of(SOURCE)`, so the `target_size != source_size` guard was always
// false and the sized-field copy was dead: coercing `RefMut<[T;N]>`/`Ref<[T;N]>` to the `[T]` form
// never copied the trailing `borrow` guard, so its `Drop` failed to release the `RefCell` borrow ->
// a spurious "already mutably borrowed" panic (and, in the libtest harness, a thread-panic abort).
// Surfaced by rust-lang/rust coretests `cell::refcell_ref_coercion`. Must match native exactly.
use std::cell::{Ref, RefCell, RefMut};
fn main() {
    let cell: RefCell<[i32; 3]> = RefCell::new([1, 2, 3]);
    {
        let mut cellref: RefMut<'_, [i32; 3]> = cell.borrow_mut();
        cellref[0] = 4;
        let mut coerced: RefMut<'_, [i32]> = cellref;
        coerced[2] = 5;
        println!("refmut len={} {:?}", coerced.len(), &*coerced);
    }
    // If the RefMut above did not release its borrow on drop, this borrow() panics.
    {
        let comp: &[i32] = &[4, 2, 5];
        let cellref: Ref<'_, [i32; 3]> = cell.borrow();
        let coerced: Ref<'_, [i32]> = cellref;
        println!("ref len={} eq={}", coerced.len(), &*coerced == comp);
    }
    println!("OK");
}
