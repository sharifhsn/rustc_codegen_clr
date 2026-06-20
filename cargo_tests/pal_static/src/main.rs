//! Bisect the TLS AccessViolation: is it a plain interior-mutable `static` write,
//! or specific to the `thread_local!` machinery?
use std::cell::{Cell, UnsafeCell};

struct S {
    v: UnsafeCell<u64>,
}
unsafe impl Sync for S {}
static X: S = S { v: UnsafeCell::new(0) };

thread_local! { static TL: Cell<u64> = Cell::new(7); }

fn main() {
    println!("a static read:        {}", unsafe { *X.v.get() });
    unsafe { *X.v.get() = 42 };
    println!("b static write+read:  {}", unsafe { *X.v.get() });
    TL.with(|c| {
        println!("c tls read:           {}", c.get());
        c.set(99);
        println!("d tls write+read:     {}", c.get());
    });
    println!("done");
}
