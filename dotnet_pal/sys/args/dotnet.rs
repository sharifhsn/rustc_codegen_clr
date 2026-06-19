//! Command-line arguments for the .NET ("dotnet") platform.
//!
//! Minimal implementation: the runtime entry point does not yet thread the
//! process argv into `std` (see `sys::pal::dotnet::init`, which ignores
//! `argc`/`argv`), so the program-args iterator is empty. This mirrors the
//! shared `unsupported` arm — once the cilly entry shim forwards the managed
//! `string[] args`, this can grow a real `args()` like the zkvm arm.

use crate::ffi::OsString;
use crate::fmt;

pub struct Args {}

pub fn args() -> Args {
    Args {}
}

impl fmt::Debug for Args {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().finish()
    }
}

impl Iterator for Args {
    type Item = OsString;

    #[inline]
    fn next(&mut self) -> Option<OsString> {
        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }
}

impl DoubleEndedIterator for Args {
    #[inline]
    fn next_back(&mut self) -> Option<OsString> {
        None
    }
}

impl ExactSizeIterator for Args {
    #[inline]
    fn len(&self) -> usize {
        0
    }
}
