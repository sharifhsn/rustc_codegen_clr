// MINIMAL MRE (1 crate) for the regex/globset AccessViolation, reduced by systematic
// feature bisection from the full `regex` crate:
//   regex -> regex_automata::meta + hybrid (lazy DFA) is the minimal crashing combo.
//   meta alone = OK; hybrid alone = OK; meta+hybrid = AV.  perf-literal is INCIDENTAL.
// The backend AVs in `core::ptr::drop_glue::<Core>` -> the `Core.pre: Option<Prefilter>`
// field (which is None here) is READ AS Some(wild Arc<dyn PrefilterI>) and dropped.
// Cause: a wrong field offset / niche read for `pre` inside the multi-field `Core` once
// the (large/over-aligned) hybrid lazy-DFA field is present. Native is correct.
use regex_automata::meta::Regex;
fn main() {
    let re = Regex::new("[a-o]").unwrap();
    println!("is_match(hello) = {}", re.is_match(b"hello")); // expect true
    println!("OK");
}
