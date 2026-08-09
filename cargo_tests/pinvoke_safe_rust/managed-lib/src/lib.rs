//! C#-natural projection over the same safe facade used by managed Rust.

use dotnet_macros::dotnet_export;
use pinvoke_safe_rust_facade::{describe, increment, running_totals, sum_squares};

#[dotnet_export(name = "SumSquares")]
pub fn sum_squares_for_dotnet(values: Vec<i32>) -> i64 {
    sum_squares(&values).expect("safe_rust_native rejected a valid input")
}

#[dotnet_export(name = "Increment")]
pub fn increment_for_dotnet(mut values: Vec<i32>) -> Vec<i32> {
    increment(&mut values).expect("safe_rust_native rejected a valid input");
    values
}

#[dotnet_export(name = "Describe")]
pub fn describe_for_dotnet(label: String, values: Vec<i32>) -> String {
    describe(&label, &values).expect("safe_rust_native rejected a valid UTF-8 input")
}

#[dotnet_export(name = "RunningTotals")]
pub fn running_totals_for_dotnet(values: Vec<i32>) -> Vec<i64> {
    running_totals(&values).expect("safe_rust_native rejected a valid input")
}
