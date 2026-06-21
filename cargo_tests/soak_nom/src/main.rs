use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::character::complete::{alpha1, alphanumeric1, char, digit1};
use nom::combinator::{map, map_res, recognize};
use nom::multi::{many0, separated_list0};
use nom::sequence::{pair, separated_pair};
use nom::IResult;

// Parse an identifier: an alpha char/underscore-ish key made of alphanumerics.
// Exercises `recognize` + `pair` + closures.
fn identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(alpha1, many0(alphanumeric1)))(input)
}

// Parse a value as either a number (mapped to a decimal string) or an identifier.
// Exercises `alt`, `map`, `map_res` (closures + generics-heavy combinators).
fn value(input: &str) -> IResult<&str, String> {
    alt((
        map_res(digit1, |s: &str| s.parse::<i64>().map(|n| n.to_string())),
        map(identifier, |s: &str| s.to_string()),
    ))(input)
}

// Parse a single "key=value" pair.
fn kv_pair(input: &str) -> IResult<&str, (&str, String)> {
    separated_pair(identifier, char('='), value)(input)
}

// Parse a ';'-separated list of pairs.
fn kv_list(input: &str) -> IResult<&str, Vec<(&str, String)>> {
    separated_list0(tag(";"), kv_pair)(input)
}

fn main() {
    let input = "alpha=1;beta=hello;gamma=42;delta=world";

    match kv_list(input) {
        Ok((rest, pairs)) => {
            println!("remaining = {:?}", rest);
            println!("pair_count = {}", pairs.len());
            for (k, v) in &pairs {
                println!("{} -> {}", k, v);
            }
            // A small numeric aggregation over the parsed values that look numeric.
            let sum: i64 = pairs
                .iter()
                .filter_map(|(_, v)| v.parse::<i64>().ok())
                .sum();
            println!("numeric_sum = {}", sum);
        }
        Err(_) => {
            println!("parse_error = true");
        }
    }

    // Also parse a plain comma-separated number list to exercise digit1 + closures.
    let nums_input = "10,20,30,40";
    let num: IResult<&str, Vec<i64>> = separated_list0(
        char(','),
        map_res(digit1, |s: &str| s.parse::<i64>()),
    )(nums_input);
    match num {
        Ok((_, list)) => {
            let total: i64 = list.iter().copied().sum();
            println!("num_list_len = {}", list.len());
            println!("num_list_total = {}", total);
        }
        Err(_) => {
            println!("num_parse_error = true");
        }
    }

    println!("== soak_nom done ==");
}
