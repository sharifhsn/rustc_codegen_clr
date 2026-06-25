// survey_cgmath: deterministic exercise of cgmath's f64 vector/matrix core surface.
// All output is fixed-precision ({:.6}) so it can be byte-compared between native rustc
// and the .NET backend. No RNG, no time, no hashing, no I/O beyond stdout.

use cgmath::{InnerSpace, Matrix, Matrix4, Rad, SquareMatrix, Vector3, Vector4};

fn main() {
    // --- Fixed input vectors (deterministic constants) ---
    let a: Vector3<f64> = Vector3::new(1.0, 2.0, 3.0);
    let b: Vector3<f64> = Vector3::new(4.0, 5.0, 6.0);

    // --- Dot product ---
    let dot = a.dot(b);
    println!("dot = {:.6}", dot);

    // --- Cross product ---
    let cross = a.cross(b);
    println!("cross_x = {:.6}", cross.x);
    println!("cross_y = {:.6}", cross.y);
    println!("cross_z = {:.6}", cross.z);

    // --- Magnitude (length) and squared magnitude ---
    println!("magnitude_a = {:.6}", a.magnitude());
    println!("magnitude2_a = {:.6}", a.magnitude2());

    // --- Normalization (unit vector); magnitude of normalized should be ~1 ---
    let a_norm = a.normalize();
    println!("normalized_x = {:.6}", a_norm.x);
    println!("normalized_y = {:.6}", a_norm.y);
    println!("normalized_z = {:.6}", a_norm.z);
    println!("normalized_magnitude = {:.6}", a_norm.magnitude());

    // --- Vector arithmetic ---
    let sum = a + b;
    let scaled = a * 2.5;
    println!("sum_x = {:.6}", sum.x);
    println!("sum_y = {:.6}", sum.y);
    println!("sum_z = {:.6}", sum.z);
    println!("scaled_x = {:.6}", scaled.x);
    println!("scaled_y = {:.6}", scaled.y);
    println!("scaled_z = {:.6}", scaled.z);

    // --- Matrix4 construction & multiplication ---
    // Two explicit column-major 4x4 matrices.
    let m1: Matrix4<f64> = Matrix4::new(
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
    );
    let m2: Matrix4<f64> = Matrix4::new(
        16.0, 15.0, 14.0, 13.0, 12.0, 11.0, 10.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0,
    );
    let prod = m1 * m2;
    // Print the full 4x4 product deterministically (column-major access via index).
    for col in 0..4 {
        for row in 0..4 {
            println!("prod_c{}_r{} = {:.6}", col, row, prod[col][row]);
        }
    }

    // --- Identity matrix & determinant ---
    let ident: Matrix4<f64> = Matrix4::identity();
    println!("ident_det = {:.6}", ident.determinant());
    println!("m1_det = {:.6}", m1.determinant());

    // --- Rotation: from_angle about Z by a fixed angle (radians) ---
    // Use a fixed angle (pi/4) so the trig results are deterministic.
    let angle = Rad(std::f64::consts::FRAC_PI_4);
    let rot: Matrix4<f64> = Matrix4::from_angle_z(angle);
    // Apply the rotation to a known point (the x-axis unit vector, homogeneous).
    let p: Vector4<f64> = Vector4::new(1.0, 0.0, 0.0, 1.0);
    let rotated = rot * p;
    println!("rot_point_x = {:.6}", rotated.x);
    println!("rot_point_y = {:.6}", rotated.y);
    println!("rot_point_z = {:.6}", rotated.z);
    println!("rot_point_w = {:.6}", rotated.w);

    // Rotation about X and Y too, applied to a fixed point, for broader coverage.
    let rot_x: Matrix4<f64> = Matrix4::from_angle_x(Rad(std::f64::consts::FRAC_PI_6));
    let rot_y: Matrix4<f64> = Matrix4::from_angle_y(Rad(std::f64::consts::FRAC_PI_3));
    let py: Vector4<f64> = Vector4::new(0.0, 1.0, 0.0, 1.0);
    let pz: Vector4<f64> = Vector4::new(0.0, 0.0, 1.0, 1.0);
    let rx_py = rot_x * py;
    let ry_pz = rot_y * pz;
    println!("rotx_py_y = {:.6}", rx_py.y);
    println!("rotx_py_z = {:.6}", rx_py.z);
    println!("roty_pz_x = {:.6}", ry_pz.x);
    println!("roty_pz_z = {:.6}", ry_pz.z);

    // --- Matrix * vector (transform a vector by m1's upper-left, via Vector4) ---
    let v4: Vector4<f64> = Vector4::new(1.0, 1.0, 1.0, 1.0);
    let mv = m1 * v4;
    println!("mv_x = {:.6}", mv.x);
    println!("mv_y = {:.6}", mv.y);
    println!("mv_z = {:.6}", mv.z);
    println!("mv_w = {:.6}", mv.w);

    // --- Transpose ---
    let mt = m1.transpose();
    println!("transpose_c0_r1 = {:.6}", mt[0][1]);
    println!("transpose_c1_r0 = {:.6}", mt[1][0]);

    println!("== survey_cgmath done ==");
}
