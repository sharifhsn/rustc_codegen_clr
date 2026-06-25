// survey_wide: direct SIMD-lowering probe for the `wide` crate.
// Exercises f32x4 and i32x4 fixed-lane SIMD: add/mul/sqrt/min/max/horizontal-sum.
// All output is deterministic: floats printed with {:.6}, ints printed exactly,
// no RNG / no time / no hashing / no addresses, no panic paths.

use wide::{f32x4, i32x4};

// Print the 4 lanes of an f32x4 with fixed precision under a label.
fn print_f32x4(label: &str, v: f32x4) {
    let a = v.to_array();
    println!(
        "{} = [{:.6}, {:.6}, {:.6}, {:.6}]",
        label, a[0], a[1], a[2], a[3]
    );
}

// Print the 4 lanes of an i32x4 exactly under a label.
fn print_i32x4(label: &str, v: i32x4) {
    let a = v.to_array();
    println!("{} = [{}, {}, {}, {}]", label, a[0], a[1], a[2], a[3]);
}

fn main() {
    // ---- f32x4 lane arithmetic ----------------------------------------
    let fa = f32x4::from([1.0_f32, 2.0, 3.0, 4.0]);
    let fb = f32x4::from([10.0_f32, 20.0, 30.0, 40.0]);

    print_f32x4("f32_a", fa);
    print_f32x4("f32_b", fb);

    // Elementwise add and mul.
    print_f32x4("f32_add", fa + fb);
    print_f32x4("f32_mul", fa * fb);

    // sqrt over a known set of perfect-ish squares for stable digits.
    let fsq = f32x4::from([4.0_f32, 9.0, 16.0, 25.0]);
    print_f32x4("f32_sqrt", fsq.sqrt());

    // Lanewise min / max between two vectors.
    let fx = f32x4::from([5.0_f32, 1.0, 8.0, 2.0]);
    let fy = f32x4::from([3.0_f32, 6.0, 7.0, 9.0]);
    print_f32x4("f32_min", fx.min(fy));
    print_f32x4("f32_max", fx.max(fy));

    // Horizontal (reduce) sum across the 4 lanes -> scalar.
    let fsum: f32 = (fa + fb).reduce_add();
    println!("f32_hsum = {:.6}", fsum);

    // ---- i32x4 lane arithmetic ----------------------------------------
    let ia = i32x4::from([1_i32, 2, 3, 4]);
    let ib = i32x4::from([10_i32, 20, 30, 40]);

    print_i32x4("i32_a", ia);
    print_i32x4("i32_b", ib);

    // Elementwise add and mul.
    print_i32x4("i32_add", ia + ib);
    print_i32x4("i32_mul", ia * ib);

    // Lanewise min / max between two vectors.
    let ix = i32x4::from([5_i32, 1, 8, 2]);
    let iy = i32x4::from([3_i32, 6, 7, 9]);
    print_i32x4("i32_min", ix.min(iy));
    print_i32x4("i32_max", ix.max(iy));

    // Horizontal (reduce) sum across the 4 lanes -> scalar.
    let isum: i32 = (ia + ib).reduce_add();
    println!("i32_hsum = {}", isum);

    // ---- a small fused combination to chain several ops ----------------
    // ((a + b) * 2.0).min(scalar-broadcast 50.0), then reduce.
    let two = f32x4::splat(2.0);
    let cap = f32x4::splat(50.0);
    let fused = ((fa + fb) * two).min(cap);
    print_f32x4("f32_fused", fused);
    println!("f32_fused_hsum = {:.6}", fused.reduce_add());

    println!("== survey_wide done ==");
}
