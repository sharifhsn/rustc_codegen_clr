use phf::phf_map;

// Compile-time static map: str -> int. The macro generates a large static
// perfect-hash table baked into the binary (static-reloc paths).
static COLORS: phf::Map<&'static str, u32> = phf_map! {
    "red"    => 0xFF0000,
    "green"  => 0x00FF00,
    "blue"   => 0x0000FF,
    "white"  => 0xFFFFFF,
    "black"  => 0x000000,
    "cyan"   => 0x00FFFF,
    "magenta"=> 0xFF00FF,
    "yellow" => 0xFFFF00,
};

// Compile-time static map: str -> str.
static CAPITALS: phf::Map<&'static str, &'static str> = phf_map! {
    "france"  => "paris",
    "japan"   => "tokyo",
    "germany" => "berlin",
    "italy"   => "rome",
    "spain"   => "madrid",
    "canada"  => "ottawa",
};

fn main() {
    // --- str -> int lookups (hits + a miss), printed in a fixed order. ---
    let int_keys = ["red", "blue", "yellow", "black", "purple"]; // "purple" is a miss
    for key in int_keys {
        match COLORS.get(key) {
            Some(v) => println!("color {} = {:06X}", key, v),
            None => println!("color {} = <miss>", key),
        }
    }

    // --- str -> str lookups (hits + a miss). ---
    let str_keys = ["france", "japan", "spain", "atlantis", "canada"]; // "atlantis" miss
    for key in str_keys {
        match CAPITALS.get(key) {
            Some(v) => println!("capital {} = {}", key, *v),
            None => println!("capital {} = <miss>", key),
        }
    }

    // --- contains_key (bool, deterministic). ---
    println!("colors_contains_green = {}", COLORS.contains_key("green"));
    println!("colors_contains_pink = {}", COLORS.contains_key("pink"));
    println!("capitals_contains_italy = {}", CAPITALS.contains_key("italy"));

    // --- table sizes (compile-time constants, deterministic). ---
    println!("colors_len = {}", COLORS.len());
    println!("capitals_len = {}", CAPITALS.len());

    // --- Iterate entries deterministically: collect then sort by key. ---
    // (phf::Map iteration order is build-stable but we sort to be safe.)
    let mut color_entries: Vec<(&&str, &u32)> = COLORS.entries().collect();
    color_entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut color_sum: u64 = 0;
    for (k, v) in &color_entries {
        color_sum = color_sum.wrapping_add(u64::from(**v));
        println!("entry color {} -> {:06X}", k, v);
    }
    println!("color_value_sum = {:08X}", color_sum);

    let mut cap_entries: Vec<(&&str, &&str)> = CAPITALS.entries().collect();
    cap_entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut cap_key_charsum: u64 = 0;
    for (k, v) in &cap_entries {
        for b in k.bytes() {
            cap_key_charsum = cap_key_charsum.wrapping_add(u64::from(b));
        }
        println!("entry capital {} -> {}", k, v);
    }
    println!("capital_key_charsum = {}", cap_key_charsum);

    // --- Derive a deterministic bool from a lookup chain. ---
    let chain_ok = COLORS.get("red") == Some(&0xFF0000u32)
        && CAPITALS.get("japan") == Some(&"tokyo")
        && COLORS.get("nope").is_none();
    println!("lookup_chain_ok = {}", chain_ok);

    println!("== survey_phf done ==");
}
