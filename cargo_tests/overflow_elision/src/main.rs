use std::ops::Add;
fn main() {
    let x = std::hint::black_box(100u8);
    let y = std::hint::black_box(200u8);
    // <u8 as Add>::add is #[rustc_inherit_overflow_checks]; inlined into a release crate
    // (overflow-checks off) it must WRAP -> 300 % 256 = 44, not panic.
    println!("{}", Add::add(x, y));
}
