//! Shared safe managed-Rust facade over the native Rust library.
//!
//! This crate intentionally contains no raw pointers, ABI lengths, or `unsafe` blocks.
//! `native_import!` generates and confines those implementation details.

use rust_dotnet_pinvoke::native_import;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeError(pub i32);

impl NativeError {
    pub const fn code(self) -> i32 {
        self.0
    }
}

impl core::fmt::Display for NativeError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "safe_rust_native failed with status {}", self.0)
    }
}

impl std::error::Error for NativeError {}

mod abi {
    use super::native_import;

    native_import! {
        library = "safe_rust_native";
        pub fn sum_squares(values: &[i32]) -> Result<i64, i32>;
    }

    native_import! {
        library = "safe_rust_native";
        pub fn increment(values: &mut [i32]) -> Result<(), i32>;
    }

    native_import! {
        library = "safe_rust_native";
        pub fn describe(label: &str, values: &[i32]) -> Result<String, i32>;
    }

    native_import! {
        library = "safe_rust_native";
        pub fn running_totals(values: &[i32]) -> Result<Vec<i64>, i32>;
    }
}

pub fn sum_squares(values: &[i32]) -> Result<i64, NativeError> {
    abi::sum_squares(values).map_err(NativeError)
}

pub fn increment(values: &mut [i32]) -> Result<(), NativeError> {
    abi::increment(values).map_err(NativeError)
}

pub fn describe(label: &str, values: &[i32]) -> Result<String, NativeError> {
    abi::describe(label, values).map_err(NativeError)
}

pub fn running_totals(values: &[i32]) -> Result<Vec<i64>, NativeError> {
    abi::running_totals(values).map_err(NativeError)
}
