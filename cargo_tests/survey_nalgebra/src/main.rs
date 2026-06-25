use nalgebra::{Matrix3, Vector3};

// Deterministic exercise of nalgebra's dense f64 linear-algebra core:
// Vector3 dot/cross/norm and Matrix3 mul-vector/determinant/transpose.
// All inputs are fixed literals and every printed float uses {:.6}, so the
// output is byte-stable across runs and between native rustc and the .NET
// backend. No RNG, no clocks, no hashing, no I/O beyond stdout.
fn main() {
    // --- Fixed-input vectors -------------------------------------------------
    let a: Vector3<f64> = Vector3::new(1.0, 2.0, 3.0);
    let b: Vector3<f64> = Vector3::new(4.0, 5.0, 6.0);

    // Dot product (scalar).
    let dot = a.dot(&b);
    println!("dot = {:.6}", dot);

    // Cross product (vector).
    let cross = a.cross(&b);
    println!("cross_x = {:.6}", cross.x);
    println!("cross_y = {:.6}", cross.y);
    println!("cross_z = {:.6}", cross.z);

    // Euclidean norms (sqrt-based, deterministic for these inputs).
    println!("norm_a = {:.6}", a.norm());
    println!("norm_b = {:.6}", b.norm());
    println!("norm_squared_a = {:.6}", a.norm_squared());

    // Normalized vector (unit length). Components printed at fixed precision.
    let unit_a = a.normalize();
    println!("unit_a_x = {:.6}", unit_a.x);
    println!("unit_a_y = {:.6}", unit_a.y);
    println!("unit_a_z = {:.6}", unit_a.z);
    println!("unit_a_norm = {:.6}", unit_a.norm());

    // Vector arithmetic: linear combination.
    let lc = a * 2.0 + b * 0.5;
    println!("lc_x = {:.6}", lc.x);
    println!("lc_y = {:.6}", lc.y);
    println!("lc_z = {:.6}", lc.z);

    // --- Fixed-input 3x3 matrix ---------------------------------------------
    // A non-singular matrix with an exact, well-conditioned determinant.
    let m: Matrix3<f64> = Matrix3::new(
        2.0, -1.0, 0.0,
        -1.0, 2.0, -1.0,
        0.0, -1.0, 2.0,
    );

    // Matrix * vector.
    let mv = m * a;
    println!("mv_x = {:.6}", mv.x);
    println!("mv_y = {:.6}", mv.y);
    println!("mv_z = {:.6}", mv.z);

    // Determinant (exact integer-valued result for this matrix: 4).
    println!("det = {:.6}", m.determinant());

    // Trace (sum of diagonal).
    println!("trace = {:.6}", m.trace());

    // Transpose: symmetric matrix, so transpose == original; verify via a
    // derived boolean rather than printing the whole matrix.
    let mt = m.transpose();
    println!("transpose_symmetric = {}", mt == m);

    // Matrix * matrix (square the matrix), then read a few fixed entries.
    let m2 = m * m;
    println!("m2_00 = {:.6}", m2[(0, 0)]);
    println!("m2_11 = {:.6}", m2[(1, 1)]);
    println!("m2_22 = {:.6}", m2[(2, 2)]);
    println!("m2_det = {:.6}", m2.determinant());

    // Identity / scaling sanity check.
    let id: Matrix3<f64> = Matrix3::identity();
    let scaled = id * 3.0;
    println!("scaled_trace = {:.6}", scaled.trace());

    // Matrix inverse (this matrix is invertible). Result is Option; handle it.
    match m.try_inverse() {
        Some(inv) => {
            // m * inv should be the identity; check via a derived scalar
            // (the trace, which must be 3.0 for I) at fixed precision.
            let prod = m * inv;
            println!("inverse_ok = true");
            println!("inverse_trace = {:.6}", prod.trace());
            println!("inv_00 = {:.6}", inv[(0, 0)]);
        }
        None => {
            println!("inverse_ok = false");
        }
    }

    // Component-wise min / max across a vector (deterministic reductions).
    println!("min_a = {:.6}", a.min());
    println!("max_b = {:.6}", b.max());
    println!("sum_a = {:.6}", a.sum());

    println!("== survey_nalgebra done ==");
}
