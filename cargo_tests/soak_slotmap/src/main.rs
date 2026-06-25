//! H2 real-crate SOAK: slotmap (a generational-index arena/map) on the dotnet PAL.
//! Exercises: SlotMap insert (returns generational keys), get by key, remove, iteration over
//! (key, value) pairs, len tracking, and key invalidation after removal. slotmap leans heavily
//! on Vec growth, generational u32 packing, MaybeUninit slot storage, and Option<&T> returns.
//! Panic-safe: keys come straight from insert (always valid); all gets are matched as Option,
//! no .unwrap()/indexing on fallible data; iteration uses for-loops.
//! SUCCESS = "== soak_slotmap done ==".
use slotmap::SlotMap;

fn main() {
    println!("== soak_slotmap start ==");

    // 1: build a SlotMap, insert several values, keep the keys
    let mut sm: SlotMap<_, i32> = SlotMap::new();
    let k0 = sm.insert(10);
    let k1 = sm.insert(20);
    let k2 = sm.insert(30);
    let k3 = sm.insert(40);
    println!("1  len after 4 inserts = {}", sm.len());

    // 2: get by key (Option, no unwrap)
    match sm.get(k1) {
        Some(v) => println!("2  get(k1)             = {}", v),
        None => println!("2  get(k1)             = <none>"),
    }
    println!("2  contains_key(k2)    = {}", sm.contains_key(k2));

    // 3: remove one element; its key must become invalid, others stay valid
    let removed = sm.remove(k1);
    match removed {
        Some(v) => println!("3  removed k1 value    = {}", v),
        None => println!("3  removed k1 value    = <none>"),
    }
    println!("3  len after remove    = {}", sm.len());
    println!("3  contains_key(k1)?   = {} (expect false)", sm.contains_key(k1));
    println!("3  contains_key(k0)?   = {} (expect true)", sm.contains_key(k0));

    // 4: mutate a value through get_mut
    if let Some(v) = sm.get_mut(k3) {
        *v += 5;
    }
    match sm.get(k3) {
        Some(v) => println!("4  get(k3) after +5    = {}", v),
        None => println!("4  get(k3) after +5    = <none>"),
    }

    // 5: insert again — reuses the freed slot with a bumped generation
    let k4 = sm.insert(99);
    println!("5  len after re-insert = {}", sm.len());
    println!("5  k4 != k1 (gen bump) = {}", k4 != k1);

    // 6: iterate over all (key, value) pairs, summing values deterministically
    let mut sum: i64 = 0;
    let mut count = 0usize;
    for (_key, val) in sm.iter() {
        sum += *val as i64;
        count += 1;
    }
    println!("6  iter count          = {}", count);
    println!("6  iter value sum      = {}", sum);

    // 7: values()/keys() iterators
    let max = sm.values().copied().max();
    match max {
        Some(m) => println!("7  max value           = {}", m),
        None => println!("7  max value           = <none>"),
    }

    // 8: clear and confirm empty
    sm.clear();
    println!("8  len after clear     = {}", sm.len());
    println!("8  is_empty            = {}", sm.is_empty());

    println!("== soak_slotmap done ==");
}
