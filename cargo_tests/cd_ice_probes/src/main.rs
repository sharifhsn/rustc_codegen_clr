// P3 reachable-ICE regression probes (census §3). Each block previously crashed the backend
// with a cryptic ICE on safe, stable Rust; all must now compile and print deterministically.

// Item 5: extern "system" fn — internal_abi previously hit todo!("Unsuported ABI").
extern "system" fn ext_sys(a: u32, b: u32) -> u32 { a + b }

// Item 1: edition-2024 Drop-temporary in tail position — may emit BackwardIncompatibleDropHint.
struct Guard(u32);
impl Drop for Guard { fn drop(&mut self) { println!("drop {}", self.0); } }
fn tail_temp() -> u32 { (Guard(1), Guard(2)).0.0 }

// Item 4: fn-ptr store through &mut — ptr_set_op previously had no FnPtr arm.
fn fa() -> u32 { 10 }
fn fb() -> u32 { 20 }

fn main() {
    // item 5
    println!("ext_sys={}", ext_sys(2, 3));
    // item 4
    let mut p: fn() -> u32 = fa;
    let q = &mut p;
    *q = fb;
    println!("fnptr={}", p());
    // item 2: assert_mem_uninitialized_valid intrinsic emitted by mem::uninitialized (not read).
    unsafe {
        let x: u32 = std::mem::uninitialized();
        std::hint::black_box(&x);
    }
    println!("uninit_ok");
    // item 1
    println!("tail={}", tail_temp());
}
