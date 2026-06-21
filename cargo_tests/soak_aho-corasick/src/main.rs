use aho_corasick::AhoCorasick;

fn main() {
    let patterns = &["apple", "maple", "Snapple", "le"];
    let haystack = "Nobody likes maple in their apple flavored Snapple.";

    // AhoCorasick::new returns a Result; handle it without unwrap/expect.
    let ac = match AhoCorasick::new(patterns) {
        Ok(ac) => ac,
        Err(e) => {
            println!("build error: {e}");
            println!("== soak_aho-corasick done ==");
            return;
        }
    };

    let mut count: usize = 0;
    for mat in ac.find_iter(haystack) {
        // pattern() / start() / end() never panic on a yielded match.
        let pid = mat.pattern().as_usize();
        println!("match pat={} start={} end={}", pid, mat.start(), mat.end());
        count += 1;
    }
    println!("total matches: {count}");

    // replace_all exercises the byte-search + allocation path.
    let replacements = &["APPLE", "MAPLE", "SNAPPLE", "LE"];
    let replaced = ac.replace_all(haystack, replacements);
    println!("replaced len: {}", replaced.len());

    println!("== soak_aho-corasick done ==");
}
