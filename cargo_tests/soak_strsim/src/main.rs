use strsim::{
    damerau_levenshtein, hamming, jaro, jaro_winkler, levenshtein, normalized_levenshtein,
};

fn main() {
    // Fixed, known string pairs. All deterministic — no RNG, no I/O, no time.
    // Integer-valued metrics print as integers; float metrics print at fixed
    // precision ({:.6}) so the shortest-repr can never diverge between runtimes.

    // --- Levenshtein (edit distance, usize) ---
    println!("lev_kitten_sitting = {}", levenshtein("kitten", "sitting"));
    println!("lev_flaw_lawn = {}", levenshtein("flaw", "lawn"));
    println!("lev_empty_abc = {}", levenshtein("", "abc"));
    println!("lev_same = {}", levenshtein("rust", "rust"));

    // --- Damerau-Levenshtein (adds transposition, usize) ---
    println!("dlev_ca_abc = {}", damerau_levenshtein("ca", "abc"));
    println!(
        "dlev_specs_spec = {}",
        damerau_levenshtein("specter", "spectre")
    );

    // --- Hamming (equal-length only; returns Result). Handle without unwrap. ---
    match hamming("karolin", "kathrin") {
        Ok(d) => println!("ham_karolin_kathrin = {}", d),
        Err(_) => println!("ham_karolin_kathrin = <len-mismatch>"),
    }
    match hamming("1011101", "1001001") {
        Ok(d) => println!("ham_bits = {}", d),
        Err(_) => println!("ham_bits = <len-mismatch>"),
    }
    // Intentional length mismatch -> deterministic error marker (no panic).
    match hamming("abc", "abcd") {
        Ok(d) => println!("ham_mismatch = {}", d),
        Err(_) => println!("ham_mismatch = <len-mismatch>"),
    }

    // --- Jaro / Jaro-Winkler (f64 in [0,1]); print at fixed precision ---
    println!("jaro_martha_marhta = {:.6}", jaro("martha", "marhta"));
    println!("jaro_dixon_dicksonx = {:.6}", jaro("dixon", "dicksonx"));
    println!(
        "jw_martha_marhta = {:.6}",
        jaro_winkler("martha", "marhta")
    );
    println!(
        "jw_dwayne_duane = {:.6}",
        jaro_winkler("dwayne", "duane")
    );

    // --- Normalized Levenshtein (f64 in [0,1]); fixed precision ---
    println!(
        "nlev_kitten_sitting = {:.6}",
        normalized_levenshtein("kitten", "sitting")
    );
    println!("nlev_same = {:.6}", normalized_levenshtein("rust", "rust"));
    println!(
        "nlev_empty_empty = {:.6}",
        normalized_levenshtein("", "")
    );

    println!("== soak_strsim done ==");
}
