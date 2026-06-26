// Regression probe for the sub-word atomic RMW hang (gaps-campaign / I2). AtomicU8/AtomicBool
// fetch_or/and/xor/nand on .NET 8 emulate a CAS loop; it called an unconditional-splice cmpxchng8
// (ignores the comparand → not a real CAS), so the loop oscillated and spun forever on any nonzero
// value. Surfaced by rust-lang/rust coretests `atomic::atomic_access_bool`. Output must match native.
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::SeqCst};
fn main() {
    let a = AtomicU8::new(5);
    println!("u8 start={}", a.load(SeqCst));
    println!("u8 or  old={} now={}", a.fetch_or(2, SeqCst), a.load(SeqCst));   // 5|2=7
    println!("u8 and old={} now={}", a.fetch_and(6, SeqCst), a.load(SeqCst));  // 7&6=6
    println!("u8 xor old={} now={}", a.fetch_xor(3, SeqCst), a.load(SeqCst));  // 6^3=5
    let b = AtomicBool::new(true);
    b.fetch_or(false, SeqCst);
    b.fetch_and(false, SeqCst);
    b.fetch_nand(true, SeqCst);
    b.fetch_xor(true, SeqCst);
    println!("bool={}", b.load(SeqCst));
    println!("DONE");
}
