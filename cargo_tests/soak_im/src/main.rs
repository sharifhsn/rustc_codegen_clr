use im::{HashMap, Vector};

fn main() {
    // --- im::Vector: persistent push + structural sharing ---
    let v0: Vector<i32> = Vector::new();
    let v1 = {
        let mut v = v0.clone();
        v.push_back(10);
        v.push_back(20);
        v.push_back(30);
        v
    };
    // Modify a clone of v1; v1 must be unchanged.
    let v2 = {
        let mut v = v1.clone();
        v.push_back(40);
        if let Some(front) = v.get(0).copied() {
            v.push_front(front - 1);
        }
        v
    };

    println!("v0.len = {}", v0.len());
    println!("v1.len = {}", v1.len());
    println!("v2.len = {}", v2.len());

    let v1_sum: i32 = v1.iter().copied().sum();
    let v2_sum: i32 = v2.iter().copied().sum();
    println!("v1.sum = {}", v1_sum);
    println!("v2.sum = {}", v2_sum);

    // structural sharing did not mutate the original
    println!("v0 empty after clone-modify: {}", v0.is_empty());
    println!("v1 unchanged (len 3): {}", v1.len() == 3);

    if let (Some(a), Some(b)) = (v1.get(0), v2.get(0)) {
        println!("v1[0] = {}, v2[0] = {}", a, b);
    }

    // --- im::HashMap: persistent insert + structural sharing ---
    let m0: HashMap<&'static str, i32> = HashMap::new();
    let m1 = {
        let mut m = m0.clone();
        m.insert("a", 1);
        m.insert("b", 2);
        m.insert("c", 3);
        m
    };
    // update("d", 4) returns a NEW map; m1 stays the same.
    let m2 = m1.update("d", 4);
    let m3 = m2.update("a", 100); // override existing key in a fresh map

    println!("m0.len = {}", m0.len());
    println!("m1.len = {}", m1.len());
    println!("m2.len = {}", m2.len());
    println!("m3.len = {}", m3.len());

    println!("m1 has d: {}", m1.contains_key("d"));
    println!("m2 has d: {}", m2.contains_key("d"));

    match (m1.get("a"), m3.get("a")) {
        (Some(a1), Some(a3)) => println!("m1[a] = {}, m3[a] = {}", a1, a3),
        _ => println!("missing key a"),
    }

    // sum the values of m3 deterministically (sort to avoid hash-order noise)
    let mut m3_vals: Vec<i32> = m3.values().copied().collect();
    m3_vals.sort_unstable();
    println!("m3 values sorted = {:?}", m3_vals);

    println!("== soak_im done ==");
}
