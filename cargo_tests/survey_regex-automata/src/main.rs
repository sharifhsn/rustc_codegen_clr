// Survey crate for `regex-automata`: exercises the high-level meta::Regex engine
// AND a low-level dense DFA built from static-baked transition tables. Both halves
// are fully deterministic: fixed pattern, fixed haystack, integer/bool/span output.

use regex_automata::{
    dfa::{dense, Automaton},
    meta::Regex as MetaRegex,
    util::primitives::StateID,
    Anchored, HalfMatch, Input, MatchKind,
};

fn main() {
    // ---- Part 1: meta::Regex (the production multi-engine API) ----
    // A pattern with two alternatives + a capture group. Deterministic haystack.
    let pattern = r"(\d{4})-(\d{2})-(\d{2})";
    let haystack = "logs: 2021-01-15 then 1999-12-31 and also 2024-06-24 end.";

    match MetaRegex::new(pattern) {
        Ok(re) => {
            // is_match: cheap boolean over the whole haystack.
            println!("meta_is_match = {}", re.is_match(haystack));

            // find_iter: count every non-overlapping match + record first span.
            let mut count = 0usize;
            let mut first_span = (usize::MAX, usize::MAX);
            let mut last_span = (0usize, 0usize);
            let mut span_sum = 0usize; // checksum of all (start+end) — order-independent-ish but deterministic
            for m in re.find_iter(haystack) {
                if count == 0 {
                    first_span = (m.start(), m.end());
                }
                last_span = (m.start(), m.end());
                span_sum += m.start() + m.end();
                count += 1;
            }
            println!("meta_match_count = {}", count);
            println!("meta_first_span = {}..{}", first_span.0, first_span.1);
            println!("meta_last_span = {}..{}", last_span.0, last_span.1);
            println!("meta_span_sum = {}", span_sum);

            // captures: pull named-by-index groups out of the FIRST match, no panic path.
            let mut caps = re.create_captures();
            re.captures(haystack, &mut caps);
            if caps.is_match() {
                // Group 0 is the whole match; groups 1..=3 are the date parts.
                let part = |i: usize| -> (usize, usize) {
                    match caps.get_group(i) {
                        Some(sp) => (sp.start, sp.end),
                        None => (0, 0),
                    }
                };
                let g0 = part(0);
                let g1 = part(1);
                let g2 = part(2);
                let g3 = part(3);
                // Slice the matched text deterministically (bounds came from the engine).
                let year = &haystack[g1.0..g1.1];
                let month = &haystack[g2.0..g2.1];
                let day = &haystack[g3.0..g3.1];
                println!("meta_caps_whole = {}..{}", g0.0, g0.1);
                println!("meta_caps_year = {}", year);
                println!("meta_caps_month = {}", month);
                println!("meta_caps_day = {}", day);
                println!("meta_group_len = {}", caps.group_len());
            } else {
                println!("meta_caps_whole = <none>");
            }
        }
        Err(_) => {
            println!("meta_compile_error = true");
        }
    }

    // ---- Part 2: dense DFA (the static transition-table engine) ----
    // Build a dense DFA from a simpler pattern; this materializes the big static
    // tables (the "DFA tables — static + codegen probe" the hint calls for).
    let dfa_pattern = r"[a-z]+";
    let dfa_haystack = "  foo bar BAZ qux  ";
    match dense::DFA::new(dfa_pattern) {
        Ok(dfa) => {
            // Drive the DFA by hand over a leftmost search to find the FIRST match end.
            let input = Input::new(dfa_haystack).anchored(Anchored::No);
            match dfa.try_search_fwd(&input) {
                Ok(Some(HalfMatch { .. })) => {
                    // try_search_fwd returns the match END offset; recompute via the
                    // explicit step loop below so we exercise the transition tables.
                    println!("dfa_fwd_search = found");
                }
                Ok(None) => println!("dfa_fwd_search = none"),
                Err(_) => println!("dfa_fwd_search = err"),
            }

            // Explicit byte-by-byte stepping: start state -> next_state per byte.
            // This is the literal table-walk; count how many bytes land in a match state.
            match dfa.start_state_forward(&Input::new(dfa_haystack)) {
                Ok(start) => {
                    let mut state: StateID = start;
                    let mut steps = 0usize;
                    let mut match_states = 0usize;
                    for &b in dfa_haystack.as_bytes() {
                        state = dfa.next_state(state, b);
                        steps += 1;
                        if dfa.is_match_state(state) {
                            match_states += 1;
                        }
                    }
                    // Feed EOI to flush any pending match at end-of-input.
                    let eoi = dfa.next_eoi_state(state);
                    println!("dfa_steps = {}", steps);
                    println!("dfa_match_states = {}", match_states);
                    println!("dfa_eoi_is_match = {}", dfa.is_match_state(eoi));
                    println!("dfa_is_dead = {}", dfa.is_dead_state(state));
                }
                Err(_) => println!("dfa_start = err"),
            }

            // Count whole matches with the DFA's own non-overlapping iteration via
            // repeated anchored-at-position searches (deterministic leftmost-first).
            let mut dfa_count = 0usize;
            let mut at = 0usize;
            let bytes = dfa_haystack.as_bytes();
            while at <= bytes.len() {
                let inp = Input::new(dfa_haystack).range(at..bytes.len());
                match dfa.try_search_fwd(&inp) {
                    Ok(Some(hm)) => {
                        // Find the match START by an anchored reverse-free heuristic:
                        // re-search from `at` forward; the end is hm.offset(). To avoid
                        // infinite loops on empty matches, always advance past the end.
                        let end = hm.offset();
                        dfa_count += 1;
                        at = if end > at { end } else { at + 1 };
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            println!("dfa_match_count = {}", dfa_count);
        }
        Err(_) => {
            println!("dfa_compile_error = true");
        }
    }

    // ---- Part 3: DFA with MatchKind::All + multi-pattern set ----
    // Build a DFA over TWO patterns to exercise multi-pattern match IDs (static
    // table dispatch by PatternID). Deterministic which pattern wins where.
    let multi = dense::DFA::builder()
        .configure(dense::DFA::config().match_kind(MatchKind::LeftmostFirst))
        .build_many(&[r"cat", r"category"]);
    match multi {
        Ok(dfa) => {
            let hay = "the category of cat";
            let inp = Input::new(hay).anchored(Anchored::No);
            match dfa.try_search_fwd(&inp) {
                Ok(Some(hm)) => {
                    println!("multi_first_pattern = {}", hm.pattern().as_usize());
                    println!("multi_first_end = {}", hm.offset());
                }
                Ok(None) => println!("multi_first = none"),
                Err(_) => println!("multi_first = err"),
            }
            println!("multi_pattern_count = {}", dfa.pattern_len());
        }
        Err(_) => println!("multi_compile_error = true"),
    }

    println!("== survey_regex-automata done ==");
}
