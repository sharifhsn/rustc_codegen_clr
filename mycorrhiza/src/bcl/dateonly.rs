//! Raw interop representation of the managed `System.DateOnly` value type.

use crate::NativeStorageSafe;
use crate::intrinsics::RustcCLRInteropManagedStruct;

/// Raw inline managed value for `System.DateOnly`.
///
/// `DateOnly` contains one 32-bit day-number field in .NET 8. The backend identifies this type by
/// its managed identity, so DTO fields and method signatures lower to `System.DateOnly`, not to an
/// imitation Rust struct.
pub type DateOnly = RustcCLRInteropManagedStruct<"System.Private.CoreLib", "System.DateOnly", 4>;
// SAFETY: `System.DateOnly` is one 32-bit day number and contains no GC references.
unsafe impl NativeStorageSafe for DateOnly {}
