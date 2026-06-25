use num_complex::Complex;

fn main() {
    let a = Complex::<f64>::new(3.0, 4.0);
    let b = Complex::<f64>::new(1.0, 2.0);

    let sum = a + b;
    let prod = a * b;
    let norm = a.norm();
    let e = a.exp();

    println!("a    = {} + {}i", a.re, a.im);
    println!("b    = {} + {}i", b.re, b.im);
    println!("sum  = {} + {}i", sum.re, sum.im);
    println!("prod = {} + {}i", prod.re, prod.im);
    println!("norm = {}", norm);
    println!("exp  = {} + {}i", e.re, e.im);

    println!("== soak_num-complex done ==");
}
