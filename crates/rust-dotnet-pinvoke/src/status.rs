//! Native status-code policies shared by the safe P/Invoke facade helpers.

/// A native API status code retained without lossy conversion.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeStatusError(pub i32);

impl NativeStatusError {
    /// Returns the original native status code.
    pub const fn code(self) -> i32 {
        self.0
    }
}

impl core::fmt::Display for NativeStatusError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "native call failed with status {}", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NativeStatusError {}

/// Interprets zero as success and preserves any non-zero native status code.
pub const fn status_zero(code: i32) -> Result<(), NativeStatusError> {
    if code == 0 {
        Ok(())
    } else {
        Err(NativeStatusError(code))
    }
}

/// Interprets non-negative values as success and preserves a negative native status code.
pub const fn status_nonnegative(code: i32) -> Result<i32, NativeStatusError> {
    if code >= 0 {
        Ok(code)
    } else {
        Err(NativeStatusError(code))
    }
}
