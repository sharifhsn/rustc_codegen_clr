//! `io::Error` decoding for the .NET ("dotnet") platform.
//!
//! The dotnet PAL does not surface a C-style `errno`: I/O is routed through the
//! managed BCL (e.g. `System.Console` via `rcl_dotnet_write`), which signals
//! failure out-of-band rather than through a thread-local error number. There is
//! therefore nothing meaningful to decode, so this mirrors the shared `generic`
//! arm used by zkvm/wasm: `errno` is always `0`, every code maps to
//! `ErrorKind::Uncategorized`, and `error_string` returns a fixed message.
//!
//! Once the cilly linker grows a way to forward a managed exception's
//! `HResult`/message back into `std`, this can grow real decoding like the unix
//! arm.

pub fn errno() -> i32 {
    0
}

pub fn is_interrupted(_code: i32) -> bool {
    false
}

pub fn decode_error_kind(_code: i32) -> crate::io::ErrorKind {
    crate::io::ErrorKind::Uncategorized
}

pub fn error_string(_errno: i32) -> String {
    "operation successful".to_string()
}
