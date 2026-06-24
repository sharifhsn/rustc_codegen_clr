static ARR: [u32; 4] = [0x6161_6161, 0x6262_6262, 0x6363_6363, 0x6464_6464];
#[inline(never)]
fn sink(x: u32) { println!("{x:08x}"); }
fn main() {
    let i = std::hint::black_box(2usize);
    sink(ARR[i]);          // dynamic
    sink(ARR[2]);          // const index -> may const-fold to Indirect{offset:8} -> must be 63636363
}
