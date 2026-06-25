// survey_glam — exercises glam's f32 Vec3 / Mat4 core surface deterministically.
//
// glam selects a SIMD backend (SSE2 / NEON / wasm-simd) under the hood for its
// f32 vector/matrix types, so this doubles as a SIMD-codegen probe for the .NET
// backend. All outputs are printed with a fixed `{:.5}` precision so the bytes are
// stable across runs and comparable between native rustc and the backend.

use glam::{Mat4, Quat, Vec3, Vec4};

fn main() {
    // ---- Vec3 construction + componentwise ops -----------------------------
    let a = Vec3::new(1.0, 2.0, 2.0); // length 3, a clean magnitude
    let b = Vec3::new(4.0, -1.0, 0.5);

    println!("a = ({:.5}, {:.5}, {:.5})", a.x, a.y, a.z);
    println!("b = ({:.5}, {:.5}, {:.5})", b.x, b.y, b.z);

    let sum = a + b;
    println!("a_plus_b = ({:.5}, {:.5}, {:.5})", sum.x, sum.y, sum.z);

    // ---- dot / cross -------------------------------------------------------
    let dot = a.dot(b);
    println!("dot_ab = {:.5}", dot);

    let cross = a.cross(b);
    println!("cross_ab = ({:.5}, {:.5}, {:.5})", cross.x, cross.y, cross.z);

    // ---- length / normalize ------------------------------------------------
    // a has integer length 3.0 exactly; normalize gives (1/3, 2/3, 2/3).
    println!("len_a = {:.5}", a.length());
    println!("len_sq_a = {:.5}", a.length_squared());

    let na = a.normalize();
    println!("norm_a = ({:.5}, {:.5}, {:.5})", na.x, na.y, na.z);
    println!("norm_a_len = {:.5}", na.length());

    // normalize_or_zero on a zero vector must not panic / NaN.
    let nz = Vec3::ZERO.normalize_or_zero();
    println!("norm_zero = ({:.5}, {:.5}, {:.5})", nz.x, nz.y, nz.z);

    // distance between two points.
    println!("dist_ab = {:.5}", a.distance(b));

    // ---- Mat4 construction + multiply --------------------------------------
    // A translation matrix times a scale matrix; the product is deterministic.
    let t = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
    let s = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let m = t * s;

    // Dump the 16 elements (column-major) at fixed precision.
    let c = m.to_cols_array();
    println!(
        "mat_ts_row0 = [{:.5}, {:.5}, {:.5}, {:.5}]",
        c[0], c[4], c[8], c[12]
    );
    println!(
        "mat_ts_row1 = [{:.5}, {:.5}, {:.5}, {:.5}]",
        c[1], c[5], c[9], c[13]
    );
    println!(
        "mat_ts_row2 = [{:.5}, {:.5}, {:.5}, {:.5}]",
        c[2], c[6], c[10], c[14]
    );
    println!(
        "mat_ts_row3 = [{:.5}, {:.5}, {:.5}, {:.5}]",
        c[3], c[7], c[11], c[15]
    );

    // determinant of the scale*translation product = product of the scales.
    println!("mat_ts_det = {:.5}", m.determinant());

    // ---- transform_point3 / transform_vector3 ------------------------------
    let p = Vec3::new(1.0, 1.0, 1.0);
    let tp = m.transform_point3(p); // scaled then translated
    println!("transform_point = ({:.5}, {:.5}, {:.5})", tp.x, tp.y, tp.z);

    let tv = m.transform_vector3(p); // scaled only, no translation
    println!("transform_vector = ({:.5}, {:.5}, {:.5})", tv.x, tv.y, tv.z);

    // ---- rotation matrix from a quaternion ---------------------------------
    // 90-degree rotation about Z: maps +X -> +Y. Use FRAC_PI_2 for an exact turn.
    let angle = core::f32::consts::FRAC_PI_2;
    let q = Quat::from_rotation_z(angle);
    let rot = Mat4::from_quat(q);
    let rx = rot.transform_vector3(Vec3::X);
    println!("rotX_about_z = ({:.5}, {:.5}, {:.5})", rx.x, rx.y, rx.z);

    // ---- inverse round-trip ------------------------------------------------
    // m * m^-1 should be identity; check a transformed-then-untransformed point.
    let inv = m.inverse();
    let back = inv.transform_point3(tp);
    println!("inverse_roundtrip = ({:.5}, {:.5}, {:.5})", back.x, back.y, back.z);

    // ---- Vec4 (full 128-bit SIMD lane) -------------------------------------
    let v4 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let w4 = Vec4::new(0.5, 0.5, 0.5, 0.5);
    println!("vec4_dot = {:.5}", v4.dot(w4));
    let mv = m * v4; // matrix-vector multiply (homogeneous point at w=4)
    println!("mat_vec4 = ({:.5}, {:.5}, {:.5}, {:.5})", mv.x, mv.y, mv.z, mv.w);

    // min/max/abs componentwise (SIMD lane ops).
    let mn = a.min(b);
    let mx = a.max(b);
    println!("min_ab = ({:.5}, {:.5}, {:.5})", mn.x, mn.y, mn.z);
    println!("max_ab = ({:.5}, {:.5}, {:.5})", mx.x, mx.y, mx.z);

    println!("== survey_glam done ==");
}
