//! RUSTFLAGS assembly. Ports `_cargo_dotnet_core.sh:640`.

use std::path::Path;

/// The exact RUSTFLAGS the backend needs:
///   -Z codegen-backend=<dylib> -C linker=<linker> -C link-args=--cargo-support
///   --cfg getrandom_backend="custom"
///
/// `--cfg getrandom_backend="custom"` selects getrandom 0.3/0.4's custom backend (our
/// os="dotnet" target has no built-in arm); harmless for crates that don't use it.
pub fn assemble(backend_dylib: &Path, linker: &Path) -> String {
    format!(
        "-Z codegen-backend={dylib} -C linker={linker} -C link-args=--cargo-support --cfg getrandom_backend=\"custom\"",
        dylib = backend_dylib.display(),
        linker = linker.display(),
    )
}
