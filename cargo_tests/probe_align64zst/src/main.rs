#![feature(thin_box)]
use core::fmt::Debug;
use std::boxed::ThinBox;
use core::sync::atomic::{AtomicIsize, Ordering};
#[track_caller]
fn verify_aligned<T>(ptr: *const T) {
    let ptr = std::hint::black_box(ptr);
    assert!(ptr.is_aligned() && !ptr.is_null(),
        "misaligned: ptr={:p} align={}", ptr, align_of::<T>());
}
#[track_caller]
fn check_thin_sized<T: Debug + PartialEq + Clone>(make: impl FnOnce() -> T) {
    let value = make(); let boxed = ThinBox::new(value.clone());
    verify_aligned(&*boxed as *const T); assert_eq!(&*boxed, &value);
}
#[track_caller]
fn check_thin_dyn<T: Debug + PartialEq + Clone>(make: impl FnOnce() -> T) {
    let value = make(); let wanted = format!("{value:?}");
    let boxed: ThinBox<dyn Debug> = ThinBox::new_unsize(value.clone());
    verify_aligned(&*boxed as *const dyn Debug as *const T);
    assert_eq!(wanted, format!("{:?}", &*boxed));
}
#[repr(align(64))] #[derive(Debug, PartialEq)] struct Align64Zst { _priv: () }
#[repr(align(128))] #[derive(Debug, PartialEq, Clone)] struct Align128Small { _p: u8 }
static COUNTER: AtomicIsize = AtomicIsize::new(0);
impl Align64Zst { fn new() -> Self { COUNTER.fetch_add(1, Ordering::Relaxed); Self { _priv: () } } }
impl Clone for Align64Zst { fn clone(&self) -> Self { verify_aligned(self); Self::new() } }
impl Drop for Align64Zst { fn drop(&mut self) { verify_aligned(self); COUNTER.fetch_add(-1, Ordering::Relaxed); } }
fn main() {
    check_thin_sized(|| Align64Zst::new());
    check_thin_dyn(|| Align64Zst::new());
    assert_eq!(COUNTER.load(Ordering::Relaxed), 0, "leak");
    // also a non-zst over-aligned (128) to exercise the runtime align-up beyond 64
    check_thin_dyn(|| Align128Small { _p: 7 });
    check_thin_sized(|| Align128Small { _p: 7 });
    println!("align64zst ok");
}
