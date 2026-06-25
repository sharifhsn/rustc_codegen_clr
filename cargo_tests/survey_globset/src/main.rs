use globset::{Glob, GlobSetBuilder};

// A fixed set of glob patterns, in a fixed order. Building the GlobSet and
// matching is fully deterministic — no HashMap iteration, no RNG, no I/O.
const PATTERNS: &[&str] = &[
    "*.rs",
    "src/**/*.rs",
    "**/*.toml",
    "docs/*.md",
    "*.{png,jpg}",
    "**/test_*.rs",
];

// A fixed set of paths to match against, in a fixed order.
const PATHS: &[&str] = &[
    "main.rs",
    "src/lib.rs",
    "src/deep/nested/mod.rs",
    "Cargo.toml",
    "sub/dir/build.toml",
    "docs/README.md",
    "docs/guide/intro.md",
    "logo.png",
    "photo.jpg",
    "notes.txt",
    "src/test_parser.rs",
    "test_top.rs",
];

fn main() {
    // Build the GlobSet from the fixed patterns. Each Glob::new and the final
    // build() returns a Result; handle without unwrap/expect.
    let mut builder = GlobSetBuilder::new();
    let mut built_ok = 0u32;
    let mut built_err = 0u32;
    for pat in PATTERNS {
        match Glob::new(pat) {
            Ok(g) => {
                builder.add(g);
                built_ok += 1;
            }
            Err(_) => {
                built_err += 1;
            }
        }
    }
    println!("patterns_total = {}", PATTERNS.len());
    println!("globs_built_ok = {}", built_ok);
    println!("globs_built_err = {}", built_err);

    let set = match builder.build() {
        Ok(s) => s,
        Err(_) => {
            println!("globset_build = error");
            println!("== survey_globset done ==");
            return;
        }
    };
    println!("globset_build = ok");
    println!("globset_len = {}", set.len());

    // For each path, record whether it matched ANY glob, plus how many globs
    // matched it (matches() returns a Vec<usize> of indices — deterministic
    // because pattern order is fixed). Print one labeled line per path.
    let mut matched_paths: Vec<&str> = Vec::new();
    let mut total_match_count = 0usize;
    for path in PATHS {
        let is_match = set.is_match(path);
        let which = set.matches(path); // Vec<usize>, ascending pattern indices
        total_match_count += which.len();
        if is_match {
            matched_paths.push(path);
        }
        // which is already in ascending index order from globset.
        println!(
            "path[{}] matched={} count={} indices={:?}",
            path,
            is_match,
            which.len(),
            which
        );
    }

    // Aggregate, sorted for stable output.
    matched_paths.sort();
    println!("matched_count = {}", matched_paths.len());
    println!("matched_paths_sorted = {:?}", matched_paths);
    println!("total_glob_hits = {}", total_match_count);

    // Exercise a single-Glob compiled matcher (the GlobMatcher surface) on a
    // couple of fixed inputs to label more of the core API deterministically.
    match Glob::new("src/**/*.rs") {
        Ok(g) => {
            let m = g.compile_matcher();
            println!("single_glob = {}", g.glob());
            println!("single_match_src_a_b = {}", m.is_match("src/a/b.rs"));
            println!("single_match_top = {}", m.is_match("top.rs"));
            println!("single_match_dir = {}", m.is_match("src/x.txt"));
        }
        Err(_) => {
            println!("single_glob = error");
        }
    }

    // A deliberately invalid pattern to exercise the error path deterministically.
    match Glob::new("a[") {
        Ok(_) => println!("invalid_pattern = unexpectedly_ok"),
        Err(_) => println!("invalid_pattern = rejected"),
    }

    println!("== survey_globset done ==");
}
