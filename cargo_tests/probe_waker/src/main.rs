// Regression probe for the ZST-field-address AccessViolation (alloctests task::test_waker_will_wake_*).
// `Arc::as_ptr` = `&raw (*inner).data`; for a ZST `data` field the backend used to return a dangling
// `0x1` (ZST fields are elided from .NET structs), so `Arc::from_raw`/`increment_strong_count` (which
// subtract the field offset) AccessViolated. Now a ZST field's address is the real `base + offset`.
// NOTE: the full `will_wake` assert additionally needs const-static dedup (`ptr::eq` of the promoted
// `RawWakerVTable`) which is a separate bug — not asserted here.
use std::sync::Arc;
struct Zst;
struct S { a: u64, b: u64, z: Zst }
struct NoopWaker;
impl std::task::Wake for NoopWaker { fn wake(self: Arc<Self>) {} }
fn main() {
    let s = S { a: 1, b: 2, z: Zst };
    assert_eq!((&s.z as *const Zst as usize).wrapping_sub(&s as *const S as usize), 16, "&s.z offset");
    // Arc<ZST> raw-pointer round-trip + strong-count (was an AccessViolation)
    let a = Arc::new(Zst);
    assert_ne!(Arc::as_ptr(&a) as usize, 1, "Arc<ZST>::as_ptr must be a real address, not dangling 0x1");
    let raw = Arc::into_raw(a);
    unsafe { Arc::increment_strong_count(raw); }
    let back = unsafe { Arc::from_raw(raw) };
    assert_eq!(Arc::strong_count(&back), 2);
    unsafe { Arc::decrement_strong_count(raw); }
    // Waker::from(Arc<W>) + clone(): used to AccessViolate in clone_waker -> increment_strong_count.
    let waker = std::task::Waker::from(Arc::new(NoopWaker));
    let _clone = waker.clone();
    println!("zst-field-address ok");
}
