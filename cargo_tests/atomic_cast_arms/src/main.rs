use std::sync::atomic::{AtomicI32, Ordering, compiler_fence};
fn main() {
    let a = AtomicI32::new(5);
    a.fetch_max(8, Ordering::SeqCst);   // 8
    a.fetch_min(2, Ordering::SeqCst);   // 2  (signed min/max)
    compiler_fence(Ordering::SeqCst);
    let p = &a as *const AtomicI32;
    let narrow = p as u32;              // narrow PointerExposeProvenance
    println!("{} {}", a.load(Ordering::SeqCst), narrow != 0);
}
