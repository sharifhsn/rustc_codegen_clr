//! H2 real-crate SOAK: pest + pest_derive (proc-macro parser generator) on the dotnet PAL.
//! A tiny inline PEG grammar parses a comma-separated number list, then we walk the parsed
//! pairs and sum the numbers. Exercises pest_derive's #[derive(Parser)] proc-macro, the
//! generated Rules enum, pest's Pairs/Pair iterators, spans, and core string/iter machinery.
//! Panic-safe: all parse errors are matched, all int parses use unwrap_or, no indexing.
//! SUCCESS = "== soak_pest done ==" with sane parsed values.

use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar_inline = r#"
number = { ASCII_DIGIT+ }
list   = { SOI ~ number ~ ("," ~ number)* ~ EOI }
"#]
struct ListParser;

fn main() {
    println!("== soak_pest start ==");

    let input = "12,34,5,678,9";
    match ListParser::parse(Rule::list, input) {
        Ok(mut pairs) => {
            println!("1  parse ok");
            // The top-level pair is `list`; descend into it.
            let mut count = 0usize;
            let mut sum: u64 = 0;
            if let Some(list_pair) = pairs.next() {
                println!("2  top rule span = {:?}", list_pair.as_str());
                for inner in list_pair.into_inner() {
                    if inner.as_rule() == Rule::number {
                        let text = inner.as_str();
                        let n: u64 = text.parse().unwrap_or(0);
                        count += 1;
                        sum += n;
                    }
                }
            }
            println!("3  parsed {count} numbers", count = count);
            println!("4  sum = {sum}", sum = sum);
        }
        Err(e) => {
            // Print the variant only; full Display is large but still safe.
            println!("parse err: {e}");
        }
    }

    // Negative case: feed an invalid string and ensure the Err path is handled.
    match ListParser::parse(Rule::list, "1,,2") {
        Ok(_) => println!("5  unexpected ok on bad input"),
        Err(_) => println!("5  bad input correctly rejected"),
    }

    println!("== soak_pest done ==");
}
