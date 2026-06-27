// Regression for alloctests sort::*::correct_i32_random_z{1_03,2}: the abort was NOT a sort bug but a
// missing `expm1` float intrinsic (the Zipf rejection sampler's helper2 calls f64::exp_m1 for non-1.0
// exponents; z1=exp 1.0 took the Taylor branch and passed). Now expm1/log1p have portable Math-based
// bodies. Asserts intrinsic accuracy + the faithful zipf-generate-then-sort scenario.
#![allow(non_snake_case)]
fn approx(a: f64, b: f64) -> bool { (a - b).abs() <= 1e-12 * b.abs().max(1.0) }
struct Zipf { num_elements: f64, exponent: f64, h_integral_x1: f64, h_integral_num_elements: f64, s: f64 }
impl Zipf {
    fn new(n: usize, exponent: f64) -> Zipf {
        Zipf { num_elements: n as f64, exponent,
            h_integral_x1: Zipf::h_integral(1.5, exponent) - 1.0,
            h_integral_num_elements: Zipf::h_integral(n as f64 + 0.5, exponent),
            s: 2.0 - Zipf::h_integral_inv(Zipf::h_integral(2.5, exponent) - Zipf::h(2.0, exponent), exponent) }
    }
    fn next(&self, rf: &mut impl FnMut() -> f64) -> usize {
        let hnum = self.h_integral_num_elements;
        loop {
            let u = hnum + rf() * (self.h_integral_x1 - hnum);
            let x = Zipf::h_integral_inv(u, self.exponent);
            let k64 = x.max(1.0).min(self.num_elements);
            let k = std::cmp::max(1, (k64 + 0.5) as usize);
            if k64 - x <= self.s || u >= Zipf::h_integral(k64 + 0.5, self.exponent) - Zipf::h(k64, self.exponent) { return k; }
        }
    }
    fn h_integral(x: f64, e: f64) -> f64 { let l = x.ln(); helper2((1.0 - e) * l) * l }
    fn h_integral_inv(x: f64, e: f64) -> f64 { let mut t = x * (1.0 - e); if t < -1.0 { t = -1.0; } (helper1(t) * x).exp() }
    fn h(x: f64, e: f64) -> f64 { (-e * x.ln()).exp() }
}
fn helper1(x: f64) -> f64 { if x.abs() > 1e-8 { x.ln_1p() / x } else { 1.0 - x*(0.5 - x*(1.0/3.0 - 0.25*x)) } }
fn helper2(x: f64) -> f64 { if x.abs() > 1e-8 { x.exp_m1()/x } else { 1.0 + x*0.5*(1.0 + x*1.0/3.0*(1.0 + 0.25*x)) } }
struct Rng(u64);
impl Rng { fn f(&mut self) -> f64 { self.0 ^= self.0<<13; self.0 ^= self.0>>7; self.0 ^= self.0<<17; (self.0>>11) as f64 / (1u64<<53) as f64 } }

fn main() {
    // intrinsic accuracy (vs known values)
    assert!(approx(1.0f64.exp_m1(), std::f64::consts::E - 1.0), "expm1(1)");
    assert!(approx(0.001f64.exp_m1(), 0.0010005001667083846), "expm1(0.001)");
    assert!(approx(1.0f64.ln_1p(), std::f64::consts::LN_2), "log1p(1)");
    assert!(approx(0.001f64.ln_1p(), 0.0009995003330835332), "log1p(0.001)");
    assert!((0.5f32.exp_m1() - 0.64872116).abs() < 1e-6, "expm1f");
    // faithful zipf-generate-then-sort, checking sortedness (the alloctests scenario)
    let mut fails = 0;
    for exp in [1.03f64, 2.0] {
        for len in [2usize,3,7,20,50,171,300,1000] {
            let mut rng = Rng(0x9E3779B97F4A7C15 ^ len as u64 ^ exp.to_bits());
            let d = Zipf::new(len, exp);
            let data: Vec<i32> = (0..len).map(|_| d.next(&mut || rng.f()) as i32).collect();
            let mut s = data.clone(); s.sort();
            let mut u = data; u.sort_unstable();
            if s.windows(2).any(|w| w[0] > w[1]) || u.windows(2).any(|w| w[0] > w[1]) { fails += 1; }
        }
    }
    assert_eq!(fails, 0, "zipf sort failures");
    println!("zipfsort ok");
}
