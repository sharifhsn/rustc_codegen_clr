//! H2 real-crate SOAK: euclid 2D geometry math on the dotnet PAL.
//! Point2D / Vector2D / Transform2D arithmetic + an affine transform applied to a point.
//! Exercises generics over a unit-typed coordinate space (PhantomData), f64 math, and the
//! euclid Transform2D matrix path. Panic-safe: no unwraps/indexing on fallible values.
//! SUCCESS = "== soak_euclid done ==" with sane values.

use euclid::{Point2D, Vector2D, Transform2D};

// A unit/coordinate-space marker type, as euclid expects.
struct Space;

fn main() {
    println!("== soak_euclid start ==");

    let p: Point2D<f64, Space> = Point2D::new(3.0, 4.0);
    let v: Vector2D<f64, Space> = Vector2D::new(1.0, 2.0);

    // Point + Vector
    let p2 = p + v;
    println!("1  p+v = ({}, {})", p2.x, p2.y);

    // Vector length / dot
    println!("2  v.len = {}", v.length());
    println!("3  v.dot(v) = {}", v.dot(v));

    // distance between points
    let origin: Point2D<f64, Space> = Point2D::origin();
    println!("4  dist(p,origin) = {}", p.distance_to(origin));

    // Affine transform: translate then scale, apply to a point.
    let t: Transform2D<f64, Space, Space> =
        Transform2D::translation(10.0, 20.0).then_scale(2.0, 3.0);
    let tp = t.transform_point(p);
    println!("5  transform_point = ({}, {})", tp.x, tp.y);

    // Transform a vector (ignores translation).
    let tv = t.transform_vector(v);
    println!("6  transform_vector = ({}, {})", tv.x, tv.y);

    // lerp between two points
    let mid = p.lerp(p2, 0.5);
    println!("7  lerp(p,p2,0.5) = ({}, {})", mid.x, mid.y);

    println!("== soak_euclid done ==");
}
