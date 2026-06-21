// Exercises PlaceElem::ConstantIndex{from_end} and Subslice{from_end} in BOTH
// read and write/set context. Slice tail-patterns (matched against &[T]/&mut [T])
// lower to from_end projections; fixed arrays use static indices, so we force a
// slice via `&a[..]` / `&mut a[..]`.

fn read_last(s: &[i32]) -> i32 {
    // ConstantIndex{from_end} in READ context.
    if let [.., last] = s {
        *last
    } else {
        -1
    }
}

fn read_first_last(s: &[i32]) -> (i32, i32) {
    // Two ConstantIndex{from_end} (and a Subslice in the middle binding) in READ.
    if let [first, .., last] = s {
        (*first, *last)
    } else {
        (-1, -1)
    }
}

fn read_subslice(s: &[i32]) -> i32 {
    // Subslice{from_end}: `mid` is `s[1 .. len-1]`.
    if let [_, mid @ .., _] = s {
        mid.iter().sum()
    } else {
        -1
    }
}

fn write_last(s: &mut [i32]) {
    // ConstantIndex{from_end} in WRITE/set context.
    if let [.., last] = s {
        *last = 99;
    }
}

fn write_first_last(s: &mut [i32]) {
    // Two ConstantIndex{from_end} in WRITE context.
    if let [first, .., last] = s {
        *first = 7;
        *last = 8;
    }
}

fn main() {
    let mut a = [10, 20, 30, 40, 50];

    // READ from_end.
    let last = read_last(&a[..]);
    let (first, last2) = read_first_last(&a[..]);
    let mid_sum = read_subslice(&a[..]);
    println!("read_last={last}");
    println!("read_first={first} read_last2={last2}");
    println!("mid_sum={mid_sum}");

    // WRITE from_end.
    write_last(&mut a[..]);
    println!("after write_last: {a:?}");
    write_first_last(&mut a[..]);
    println!("after write_first_last: {a:?}");

    // Combined check: tail-pattern read after writes.
    let final_last = read_last(&a[..]);
    println!("final_last={final_last}");

    println!("== pal_fromend done ==");
}
