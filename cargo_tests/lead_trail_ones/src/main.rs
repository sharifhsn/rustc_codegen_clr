// Sub-word leading_ones / trailing_ones / leading_zeros / trailing_zeros codegen repro.
// Mirrors upstream coretests num::{u8,i8,u16,i16,u32,i32}::test_leading_trailing_ones,
// but forced through the RUNTIME path with black_box so the codegen lowering runs
// (not just const-eval). leading_ones == (!self).leading_zeros(): a sub-word !self
// that gets widened/sign-extended on the CIL stack would make leading_zeros count
// 32/64 zeros instead of the type's BITS -> a class-level sub-word bug.
use std::hint::black_box;

macro_rules! ae {
    ($label:expr, $got:expr, $exp:expr) => {{
        let g = $got;
        let e = $exp;
        println!("{}: {}|{}", $label, g, e);
        assert_eq!(g, e, "MISMATCH {}", $label);
    }};
}

macro_rules! suite {
    ($name:literal, $T:ty) => {{
        let bits = <$T>::BITS;
        let a: $T = black_box(0b0101_1111 as $T);
        let zero: $T = black_box(0 as $T);
        let ones: $T = black_box(!(0 as $T)); // _1 = !0 (all-ones)
        let max: $T = black_box(<$T>::MAX);
        let x: $T = black_box(0b0010_1100 as $T);

        ae!(concat!($name, " a.trailing_ones"), a.trailing_ones(), 5);
        ae!(concat!($name, " (!a).leading_ones"), (!a).leading_ones(), bits - 7);
        ae!(concat!($name, " a.reverse_bits.leading_ones"), a.reverse_bits().leading_ones(), 5);

        ae!(concat!($name, " ones.leading_ones"), ones.leading_ones(), bits);
        ae!(concat!($name, " ones.trailing_ones"), ones.trailing_ones(), bits);

        ae!(concat!($name, " (ones<<1).trailing_ones"), (ones << 1).trailing_ones(), 0);

        ae!(concat!($name, " (ones<<1).leading_ones"), (ones << 1).leading_ones(), bits - 1);

        ae!(concat!($name, " zero.leading_ones"), zero.leading_ones(), 0);
        ae!(concat!($name, " zero.trailing_ones"), zero.trailing_ones(), 0);

        ae!(concat!($name, " x.leading_ones"), x.leading_ones(), 0);
        ae!(concat!($name, " x.trailing_ones"), x.trailing_ones(), 0);

        // Cross-check the underlying primitives directly.
        ae!(concat!($name, " zero.leading_zeros"), zero.leading_zeros(), bits);
        ae!(concat!($name, " zero.trailing_zeros"), zero.trailing_zeros(), bits);
        ae!(concat!($name, " ones.leading_zeros"), ones.leading_zeros(), 0);
        ae!(concat!($name, " ones.trailing_zeros"), ones.trailing_zeros(), 0);
        ae!(concat!($name, " a.count_ones"), a.count_ones(), 6);
        ae!(concat!($name, " a.count_zeros"), a.count_zeros(), bits - 6);
        ae!(concat!($name, " max.leading_ones"), max.leading_ones(),
            if <$T>::MIN == (0 as $T) { bits } else { 0 });
    }};
}

fn main() {
    // unsigned
    suite!("u8", u8);
    suite!("u16", u16);
    suite!("u32", u32);
    suite!("u64", u64);
    // signed
    suite!("i8", i8);
    suite!("i16", i16);
    suite!("i32", i32);
    suite!("i64", i64);
    println!("lead_trail_ones OK");
}
