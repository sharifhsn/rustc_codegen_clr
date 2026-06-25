//! H2 real-crate SOAK: winnow parser combinators on the dotnet PAL.
//! Parses comma-separated decimal digits "12,34,5,678" into a Vec<u32>, then sums them.
//! Exercises winnow combinators (separated, dec_uint), Parser trait/generics, &str streams,
//! Result handling. Panic-safe: parse error is matched, no unwraps on user data.
//! SUCCESS = "== soak_winnow done ==" with sane values.
use winnow::ascii::dec_uint;
use winnow::combinator::separated;
use winnow::{PResult, Parser};

fn csv_numbers(input: &mut &str) -> PResult<Vec<u32>> {
    separated(1.., dec_uint::<_, u32, _>, ',').parse_next(input)
}

fn main() {
    println!("== soak_winnow start ==");
    let input = "12,34,5,678,9";

    let mut data = input;
    match csv_numbers(&mut data) {
        Ok(nums) => {
            println!("1  parsed count={}", nums.len());
            println!("2  values={:?}", nums);
            let sum: u32 = nums.iter().copied().sum();
            println!("3  sum={}", sum);
            let max = nums.iter().copied().max().unwrap_or(0);
            println!("4  max={}", max);
            println!("5  remaining=\"{}\"", data);
        }
        Err(e) => println!("parse err: {e}"),
    }

    // A second parse: single number, then check error path on bad input.
    let mut single = "42";
    match dec_uint::<_, u32, winnow::error::ContextError>(&mut single) {
        Ok(n) => println!("6  single={}", n),
        Err(e) => println!("6  single err: {e}"),
    }

    let mut bad = "abc";
    match dec_uint::<_, u32, winnow::error::ContextError>(&mut bad) {
        Ok(n) => println!("7  unexpected ok={}", n),
        Err(_) => println!("7  bad input rejected (expected)"),
    }

    println!("== soak_winnow done ==");
}
