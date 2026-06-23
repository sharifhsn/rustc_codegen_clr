// i128 saturating_abs / saturating_neg codegen repro.
// WF-A wired saturating_add/sub for i128; abs/neg build on those.
use std::hint::black_box;

fn chk(label: &str, got: i128, expected: i128) {
    println!("{label}: {got}|{expected}");
    assert_eq!(got, expected, "MISMATCH {label}");
}

fn main() {
    // saturating_abs
    chk("abs_pos", black_box(5i128).saturating_abs(), 5);
    chk("abs_neg", black_box(-5i128).saturating_abs(), 5);
    chk("abs_zero", black_box(0i128).saturating_abs(), 0);
    chk("abs_min", black_box(i128::MIN).saturating_abs(), i128::MAX);
    chk("abs_minp1", black_box(i128::MIN + 1).saturating_abs(), i128::MAX);
    chk("abs_max", black_box(i128::MAX).saturating_abs(), i128::MAX);

    // saturating_neg
    chk("neg_pos", black_box(5i128).saturating_neg(), -5);
    chk("neg_neg", black_box(-5i128).saturating_neg(), 5);
    chk("neg_zero", black_box(0i128).saturating_neg(), 0);
    chk("neg_min", black_box(i128::MIN).saturating_neg(), i128::MAX);
    chk("neg_max", black_box(i128::MAX).saturating_neg(), i128::MIN + 1);

    // u128 saturating for good measure
    let u_ok: u128 = black_box(u128::MAX).saturating_sub(1);
    println!("u_sat_sub: {u_ok}|{}", u128::MAX - 1);
    assert_eq!(u_ok, u128::MAX - 1);

    println!("i128_sat OK");
}
