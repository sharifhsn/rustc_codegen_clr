use strum::{AsRefStr, Display, EnumIter, EnumString, IntoEnumIterator};

// An enum exercising the four headline strum derives:
//   EnumIter   -> Planet::iter() yields variants in declaration order (deterministic).
//   Display    -> to_string() (custom serialize names via #[strum(serialize = ..)]).
//   EnumString -> parse from &str via FromStr.
//   AsRefStr   -> as_ref() borrows the variant's string name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, Display, EnumString, AsRefStr)]
enum Planet {
    Mercury,
    Venus,
    Earth,
    Mars,
    // Give one variant explicit, distinct serialize/parse names to prove the
    // attribute path is wired (Display + EnumString both honor it).
    #[strum(serialize = "the-red-giant", to_string = "Jupiter")]
    Jupiter,
}

fn main() {
    // 1. EnumIter: iterate variants in declaration order, collect Display strings.
    //    Order is fixed by the derive (no HashMap), so this is deterministic.
    let names: Vec<String> = Planet::iter().map(|p| p.to_string()).collect();
    println!("variant_count = {}", names.len());
    println!("display_sequence = {}", names.join(","));

    // 2. AsRefStr: the derive generates `impl AsRef<str>` returning a &'static
    //    str. Exercise it per variant; copy into owned Strings to keep the join
    //    simple (the AsRef call itself is the surface under test).
    let as_ref_seq: Vec<String> = Planet::iter()
        .map(|p| {
            let s: &str = p.as_ref();
            s.to_string()
        })
        .collect();
    println!("as_ref_sequence = {}", as_ref_seq.join(","));

    // 3. EnumString: parse a known-good name back to a variant. Match the Result
    //    (no unwrap) and confirm the round-trip via Display.
    match "Earth".parse::<Planet>() {
        Ok(p) => {
            println!("parsed_earth = {}", p);
            println!("parsed_earth_is_earth = {}", p == Planet::Earth);
        }
        Err(_) => println!("parsed_earth = <err>"),
    }

    // 4. EnumString on the custom-serialized variant: its parse name is the
    //    `serialize` attr ("the-red-giant"), and its Display is the `to_string`
    //    attr ("Jupiter"). Exercise both directions deterministically.
    match "the-red-giant".parse::<Planet>() {
        Ok(p) => {
            println!("parsed_custom = {}", p); // Display -> "Jupiter"
            println!("parsed_custom_is_jupiter = {}", p == Planet::Jupiter);
        }
        Err(_) => println!("parsed_custom = <err>"),
    }

    // 5. EnumString error path: an unknown string must NOT parse. Confirm the
    //    failure as a bool marker (no panic).
    let bad_ok = "Pluto".parse::<Planet>().is_ok();
    println!("pluto_parses = {}", bad_ok);

    // 6. Full round-trip over every variant: Display name -> parse -> equal.
    //    For Jupiter, Display yields "Jupiter" but its parse name is the
    //    serialize alias, so Display!=parse-name there; count exact round-trips
    //    that DO recover via the Display string.
    let mut display_roundtrips = 0u32;
    for p in Planet::iter() {
        let s = p.to_string();
        if let Ok(back) = s.parse::<Planet>() {
            if back == p {
                display_roundtrips += 1;
            }
        }
    }
    println!("display_roundtrips = {}", display_roundtrips);

    println!("== survey_strum done ==");
}
