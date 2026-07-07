//! Probe crate for `cargo dotnet test` libtest-parity: `#[should_panic]`, `#[ignore]`,
//! and test-name filtering, run through the real `.NET` backend (see
//! docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md section 6).
//!
//! Exercises: plain pass, #[should_panic] (bare + `expected =`), #[ignore], and name
//! filtering. The default run (below) is all-green — 6 passed, 1 ignored. To exercise
//! failure/exit-code propagation and `--ignored` manually:
//!
//!   cargo dotnet test                      -- runs everything except #[ignore] (6 ok, 1 ignored)
//!   cargo dotnet test -- --ignored         -- runs only the ignored test (fails on purpose)
//!   cargo dotnet test -- filter_me         -- runs only tests whose name contains "filter_me"

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_pass() {
        assert_eq!(add(2, 2), 4);
    }

    #[test]
    #[should_panic]
    fn panics_bare() {
        panic!("boom");
    }

    #[test]
    #[should_panic(expected = "specific message")]
    fn panics_with_expected_message() {
        panic!("this is the specific message");
    }

    #[test]
    #[ignore]
    fn ignored_test() {
        panic!("should never run unless --ignored is passed");
    }

    #[test]
    fn filter_me_one() {
        assert_eq!(add(1, 1), 2);
    }

    #[test]
    fn filter_me_two() {
        assert_eq!(add(3, 3), 6);
    }

    #[test]
    fn not_matched_by_filter() {
        assert_eq!(add(5, 5), 10);
    }
}
