//! Raw interop representation of the managed `System.DateOnly` value type.

use crate::intrinsics::RustcCLRInteropManagedStruct;

/// Raw inline managed value for `System.DateOnly`.
///
/// `DateOnly` contains one 32-bit day-number field in .NET 8. The backend identifies this type by
/// its managed identity, so DTO fields and method signatures lower to `System.DateOnly`, not to an
/// imitation Rust struct.
pub type DateOnly = RustcCLRInteropManagedStruct<"System.Private.CoreLib", "System.DateOnly", 4>;
