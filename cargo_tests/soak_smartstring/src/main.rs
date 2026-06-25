//! H2 real-crate SOAK: smartstring on the dotnet PAL.
//! Exercises SmartString inline (short, stack) storage + spill to heap (long), push/concat,
//! comparison, formatting, and conversion to/from std String. Panic-safe (no unwraps that can fail).
//! SUCCESS = "== soak_smartstring done ==" with sane values.
use smartstring::alias::String as SmartString;

fn main() {
    println!("== soak_smartstring start ==");

    // 1. inline (short) string — should live inline (no heap spill)
    let mut s: SmartString = SmartString::new();
    s.push_str("hi");
    s.push('!');
    println!("1  inline: {s} len={}", s.len());

    // 2. spill: grow past the inline boundary (>23 bytes) to force a heap allocation
    let mut big: SmartString = SmartString::new();
    for _ in 0..10 {
        big.push_str("abcdef");
    }
    println!("2  spilled: len={} starts_ab={}", big.len(), big.starts_with("ab"));

    // 3. concat via + / push_str
    let mut joined: SmartString = SmartString::from("foo");
    joined.push_str("bar");
    joined.push_str("baz");
    println!("3  concat: {joined} len={}", joined.len());

    // 4. comparison + ordering
    let a: SmartString = SmartString::from("apple");
    let b: SmartString = SmartString::from("banana");
    println!("4  cmp: a<b={} eq={}", a < b, a == b);

    // 5. round-trip to/from std String
    let std_owned: std::string::String = big.clone().into();
    let back: SmartString = SmartString::from(std_owned.as_str());
    println!("5  roundtrip: eq={} stdlen={}", back == big, std_owned.len());

    // 6. formatting + uppercase via chars
    let up: std::string::String = a.chars().map(|c| c.to_ascii_uppercase()).collect();
    println!("6  fmt: {a:>8} upper={up}");

    // 7. collect smartstrings into a Vec, sort, print
    let mut v: Vec<SmartString> = vec![
        SmartString::from("zeta"),
        SmartString::from("alpha"),
        SmartString::from("mu"),
    ];
    v.sort();
    let names: Vec<&str> = v.iter().map(|x| x.as_str()).collect();
    println!("7  sorted: {names:?}");

    println!("== soak_smartstring done ==");
}
