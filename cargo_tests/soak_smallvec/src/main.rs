use smallvec::SmallVec;

fn main() {
    // Inline capacity of 4; push within capacity first.
    let mut v: SmallVec<[i32; 4]> = SmallVec::new();
    for i in 1..=4 {
        v.push(i);
    }
    println!("after inline pushes: len={} spilled={}", v.len(), v.spilled());

    // Spill to the heap by pushing past inline capacity.
    for i in 5..=10 {
        v.push(i);
    }
    println!("after spill: len={} spilled={}", v.len(), v.spilled());

    // Sum without any panicking operations.
    let mut sum: i64 = 0;
    for &x in v.iter() {
        sum += x as i64;
    }
    println!("sum={}", sum);

    // First / last via non-panicking accessors.
    if let Some(first) = v.first() {
        println!("first={}", first);
    }
    if let Some(last) = v.last() {
        println!("last={}", last);
    }

    println!("== soak_smallvec done ==");
}
