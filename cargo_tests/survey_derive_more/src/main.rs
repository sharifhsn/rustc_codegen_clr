// Survey of derive_more derive-macro codegen breadth.
// Exercises: Add, Display, From, Into, Constructor on small structs/enums.
// All output is deterministic (fixed integer inputs, no float shortest-repr,
// no HashMap iteration, no addresses/timestamps/RNG).

use derive_more::{Add, Constructor, Display, From, Into};

// --- Add on a tuple struct (field-wise +) -----------------------------------
#[derive(Add, Display, Clone, Copy)]
#[display("MyInt({_0})")]
struct MyInt(i64);

// --- Add on a multi-field struct (component-wise +) -------------------------
#[derive(Add, Constructor, Display, Clone, Copy)]
#[display("Point({x}, {y})")]
struct Point {
    x: i64,
    y: i64,
}

// --- From / Into on a newtype wrapper ---------------------------------------
// From<i64> generated; Into<i64> generated (derive_more `Into` makes the
// wrapped value extractable).
#[derive(From, Into, Display, Clone, Copy)]
#[display("Wrapper({_0})")]
struct Wrapper(i64);

// --- Display + From on an enum ----------------------------------------------
// `From` on an enum generates From for each variant's payload type.
#[derive(Display, From, Clone, Copy)]
enum Shape {
    #[display("Circle(r={_0})")]
    Circle(u32),
    #[display("Square(s={_0})")]
    Square(u64),
}

fn main() {
    // Add on a tuple struct: 10 + 32 = 42.
    let a = MyInt(10);
    let b = MyInt(32);
    let sum = a + b;
    println!("myint_add = {}", sum);

    // Add on a multi-field struct: component-wise.
    let p1 = Point::new(1, 2);
    let p2 = Point::new(30, 40);
    let p3 = p1 + p2;
    println!("point_ctor = {}", p1);
    println!("point_add = {}", p3);

    // From: build a Wrapper from a raw i64 via From/Into.
    let w: Wrapper = Wrapper::from(7);
    let w2: Wrapper = 100i64.into();
    println!("wrapper_from = {}", w);
    println!("wrapper_into = {}", w2);

    // Into: extract the inner i64 back out.
    let inner: i64 = w.into();
    let inner2: i64 = w2.into();
    println!("wrapper_inner = {}", inner);
    println!("wrapper_inner2 = {}", inner2);

    // Display on an enum (both variants).
    let c = Shape::Circle(5);
    let s = Shape::Square(9);
    println!("shape_circle = {}", c);
    println!("shape_square = {}", s);

    // From on an enum: u32 -> Circle, u64 -> Square (distinct payload types).
    let from_u32: Shape = Shape::from(11u32);
    let from_u64: Shape = Shape::from(13u64);
    println!("shape_from_u32 = {}", from_u32);
    println!("shape_from_u64 = {}", from_u64);

    // A short deterministic accumulation using Add to stress the derived op.
    let mut acc = MyInt(0);
    let mut i = 1i64;
    while i <= 5 {
        acc = acc + MyInt(i);
        i += 1;
    }
    println!("myint_accumulate = {}", acc); // 1+2+3+4+5 = 15

    println!("== survey_derive_more done ==");
}
