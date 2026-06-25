use core::pin::Pin;
use pin_project::pin_project;

// A struct with one structurally-pinned field (`pinned`) and one
// unpinned field (`unpinned`). The `#[pin_project]` proc-macro generates a
// `Projection` type whose `pinned` field is `Pin<&mut i32>` and whose
// `unpinned` field is `&mut u64`.
#[pin_project]
struct Demo {
    #[pin]
    pinned: i32,
    unpinned: u64,
}

impl Demo {
    fn new(pinned: i32, unpinned: u64) -> Self {
        Demo { pinned, unpinned }
    }
}

// Exercise projection through a `Pin<&mut Demo>` derived from a stack value.
// `project()` consumes the `Pin<&mut Self>` and yields field-wise pins/refs.
fn run_projection(mut value: Demo) -> (i32, u64) {
    // Pin to the stack. `Demo: Unpin` (both fields are Unpin), so this is sound
    // and we never move the value afterwards.
    let pinned: Pin<&mut Demo> = Pin::new(&mut value);
    let proj = pinned.project();

    // Read the pinned field through its `Pin<&mut i32>`.
    let pinned_read: i32 = *proj.pinned.as_ref().get_ref();

    // Modify the pinned field in place (i32: Unpin, so get_mut is available).
    let pinned_mut: &mut i32 = proj.pinned.get_mut();
    *pinned_mut = pinned_mut.wrapping_add(7);

    // Modify the unpinned field directly via its `&mut u64`.
    *proj.unpinned = proj.unpinned.wrapping_mul(3);

    let _ = pinned_read;
    (value.pinned, value.unpinned)
}

// A second, generic projection to exercise the macro over a type parameter and
// to cover a Box-pinned (heap) projection path as well.
#[pin_project]
struct Wrapper<T> {
    #[pin]
    inner: T,
    tag: u32,
}

fn run_boxed_projection() -> (i64, u32) {
    let boxed: Pin<Box<Wrapper<i64>>> = Box::pin(Wrapper {
        inner: 100_i64,
        tag: 1,
    });
    let proj = boxed.as_ref().project_ref();
    // Read-only projection (`project_ref`) yields `Pin<&i64>` and `&u32`.
    let inner_read: i64 = *proj.inner.get_ref();
    let tag_read: u32 = *proj.tag;
    (inner_read, tag_read)
}

fn main() {
    // Case 1: stack value, mutable projection of both fields.
    let demo = Demo::new(35, 14);
    let (p, u) = run_projection(demo);
    println!("pinned_after = {}", p); // 35 + 7 = 42
    println!("unpinned_after = {}", u); // 14 * 3 = 42

    // Case 2: a fresh value to confirm projection reads the right field order.
    let demo2 = Demo::new(-5, 1000);
    let (p2, u2) = run_projection(demo2);
    println!("pinned2_after = {}", p2); // -5 + 7 = 2
    println!("unpinned2_after = {}", u2); // 1000 * 3 = 3000

    // Case 3: generic + heap-pinned read-only projection.
    let (inner, tag) = run_boxed_projection();
    println!("boxed_inner = {}", inner); // 100
    println!("boxed_tag = {}", tag); // 1

    // Aggregate a deterministic checksum derived purely from the int results.
    let checksum: i64 = (p as i64) + (u as i64) + (p2 as i64) + (u2 as i64) + inner + (tag as i64);
    println!("checksum = {}", checksum); // 42+42+2+3000+100+1 = 3187

    println!("== survey_pin-project done ==");
}
