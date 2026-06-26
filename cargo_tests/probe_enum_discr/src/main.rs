//! Regression test for the `get_discr` 128-bit-niche miscompile that root-caused the regex AV.
//!
//! `Result<Big, u64>` where `Big(NonZeroU128, u8)` makes rustc niche-encode the Result on the
//! 16-byte `NonZeroU128` => a **U128 tag**. The niche variant is `Err` (variant index 1), while
//! `niche_start == 0`. The buggy decoder compared the tag against the variant *index* (1) instead
//! of the niche *value* (0), so `Err` was read as `Ok`. (For `Option`, niche_start==index==0, which
//! is why it was invisible.) Any regression here flips an assert below. See src/utilis/adt.rs
//! get_discr and rustc_codegen_clr_type's DUMP_LAYOUT introspection.
use std::hint::black_box;
use std::num::NonZeroU128;

struct Big(#[allow(dead_code)] NonZeroU128, u8);

#[inline(never)] fn mk_err() -> Result<Big, u64> { black_box(Err(42)) }
#[inline(never)] fn mk_ok()  -> Result<Big, u64> { black_box(Ok(Big(NonZeroU128::new(7).unwrap(), 9))) }

fn main() {
    // The exact bug: U128-niche Result, niche variant (Err) is NOT variant 0.
    match mk_err() {
        Err(42) => {}
        Err(x) => panic!("wrong Err payload {x}"),
        Ok(_) => panic!("U128-niche Result misread Err as Ok (get_discr niche_start bug)"),
    }
    match mk_ok() {
        Ok(b) => assert_eq!(b.1, 9),
        Err(_) => panic!("misread Ok as Err"),
    }
    // Sibling shapes that must keep working: plain Option niche, nested Result.
    let n: Option<NonZeroU128> = black_box(None);
    assert!(n.is_none());
    let nn: Result<Result<Big, u64>, u8> = black_box(Ok(Err(5)));
    assert!(matches!(nn, Ok(Err(5))));
    println!("enum_discr: all checks passed");
}
