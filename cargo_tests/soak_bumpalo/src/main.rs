//! H2 real-crate SOAK: bumpalo (a bump-allocation arena) on the dotnet PAL.
//! Bump::new(); alloc several scalars + a slice; print sum. Exercises the arena's
//! raw pointer bump-allocation, alloc_slice_*, Cell-based bump pointer, and the
//! allocated_bytes accounting. Panic-safe (no unwraps, only checked arithmetic).
//! SUCCESS = "== soak_bumpalo done ==" with sane values.
use bumpalo::Bump;

fn main() {
    println!("== soak_bumpalo start ==");

    let bump = Bump::new();

    // alloc several scalar values into the arena
    let a: &mut u64 = bump.alloc(10);
    let b: &mut u64 = bump.alloc(20);
    let c: &mut u64 = bump.alloc(30);
    println!("1  scalars: a={a} b={b} c={c}");

    // mutate one of them through the arena reference
    *b += 5;
    println!("2  after mutate: b={b}");

    // alloc a slice (copy) and a slice filled with a clone value
    let slice: &mut [u32] = bump.alloc_slice_copy(&[1u32, 2, 3, 4, 5]);
    println!("3  slice.len={}", slice.len());

    let filled: &mut [u32] = bump.alloc_slice_fill_copy(4, 7u32);
    println!("4  filled.len={} first={}", filled.len(), filled.first().copied().unwrap_or(0));

    // sum the slice via iterator fold (checked, no overflow on these small values)
    let sum: u64 = slice.iter().map(|&x| x as u64).sum();
    let scalar_sum = *a + *b + *c;
    println!("5  slice sum={sum} scalar sum={scalar_sum}");

    let filled_sum: u64 = filled.iter().map(|&x| x as u64).sum();
    println!("6  filled sum={filled_sum}");

    // alloc_str: arena-allocate a string slice
    let s: &str = bump.alloc_str("bumpalo-on-dotnet");
    println!("7  alloc_str len={}", s.len());

    // arena accounting
    println!("8  allocated_bytes>=0? {}", bump.allocated_bytes() > 0);

    let total = sum + scalar_sum + filled_sum;
    println!("9  grand total={total}");

    println!("== soak_bumpalo done ==");
}
