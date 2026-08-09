//! Native Rust implementation for the safe managed-Rust P/Invoke acceptance.
//!
//! These are ordinary safe Rust functions. The attribute generates the small,
//! audited C ABI shim (pointer validation, slice reconstruction, out values,
//! panic containment, and status conversion) outside application code.

use rust_dotnet_pinvoke::native_export;

#[native_export]
pub fn sum_squares(values: &[i32]) -> Result<i64, i32> {
    Ok(values
        .iter()
        .map(|&value| i64::from(value) * i64::from(value))
        .sum())
}

#[native_export]
pub fn increment(values: &mut [i32]) -> Result<(), i32> {
    for value in values {
        *value = value.checked_add(1).ok_or(1)?;
    }
    Ok(())
}

#[native_export]
pub fn describe(label: &str, values: &[i32]) -> Result<String, i32> {
    let total: i64 = values.iter().map(|&value| i64::from(value)).sum();
    Ok(format!("{label}: count={}, sum={total}", values.len()))
}

#[native_export]
pub fn running_totals(values: &[i32]) -> Result<Vec<i64>, i32> {
    let mut total = 0_i64;
    values
        .iter()
        .map(|&value| {
            total = total.checked_add(i64::from(value)).ok_or(2)?;
            Ok(total)
        })
        .collect()
}
