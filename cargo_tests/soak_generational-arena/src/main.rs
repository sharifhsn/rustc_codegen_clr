//! H2 real-crate SOAK: generational-arena on the dotnet PAL.
//! Arena insert several -> capture indices -> get/remove -> reinsert (generation bump) -> iterate.
//! Exercises slab-style Vec storage, generational indices, Option/match, iterators, Entry enum.
//! Panic-safe: no unwraps on fallible lookups; removed indices checked via Option.
//! SUCCESS = "== soak_generational-arena done ==" with sane values.
use generational_arena::Arena;

fn main() {
    println!("== soak_generational-arena start ==");

    let mut arena: Arena<String> = Arena::new();

    // insert several
    let i_a = arena.insert("alpha".to_string());
    let i_b = arena.insert("bravo".to_string());
    let i_c = arena.insert("charlie".to_string());
    let i_d = arena.insert("delta".to_string());
    println!("1  inserted 4, len={}", arena.len());

    // get by index
    println!("2  get a={}", arena.get(i_a).map(|s| s.as_str()).unwrap_or("?"));
    println!("3  get c={}", arena.get(i_c).map(|s| s.as_str()).unwrap_or("?"));

    // remove two
    let r_b = arena.remove(i_b);
    let r_d = arena.remove(i_d);
    println!("4  removed b={:?} d={:?}", r_b.as_deref(), r_d.as_deref());
    println!("5  len after removes={}", arena.len());

    // stale-index lookup returns None (generational safety)
    println!("6  stale get b is_none={}", arena.get(i_b).is_none());

    // reinsert -> reuses a slot with bumped generation
    let i_e = arena.insert("echo".to_string());
    println!("7  reinsert e={}", arena.get(i_e).map(|s| s.as_str()).unwrap_or("?"));
    println!("8  old b-index still none={}", arena.get(i_b).is_none());
    println!("9  len after reinsert={}", arena.len());

    // mutate in place
    if let Some(v) = arena.get_mut(i_a) {
        v.push_str("-mut");
    }
    println!("10 get_mut a={}", arena.get(i_a).map(|s| s.as_str()).unwrap_or("?"));

    // iterate remaining values (collect + sort for deterministic output)
    let mut vals: Vec<String> = arena.iter().map(|(_idx, s)| s.clone()).collect();
    vals.sort();
    println!("11 iter values: {vals:?}");

    // capacity growth: insert a batch of integers into a second arena
    let mut nums: Arena<u32> = Arena::with_capacity(2);
    let mut idxs = Vec::new();
    for n in 0..20u32 {
        idxs.push(nums.insert(n * n));
    }
    let sum: u32 = nums.iter().map(|(_i, &v)| v).sum();
    println!("12 nums len={} sum_of_squares={}", nums.len(), sum);

    // remove every other and re-sum
    for (k, idx) in idxs.iter().enumerate() {
        if k % 2 == 0 {
            nums.remove(*idx);
        }
    }
    let sum2: u32 = nums.iter().map(|(_i, &v)| v).sum();
    println!("13 after sparse removes len={} sum={}", nums.len(), sum2);

    // clear
    nums.clear();
    println!("14 cleared len={} is_empty={}", nums.len(), nums.is_empty());

    println!("== soak_generational-arena done ==");
}
