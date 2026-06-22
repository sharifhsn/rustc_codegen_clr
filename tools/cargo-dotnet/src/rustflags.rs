//! RUSTFLAGS assembly. Ports `_cargo_dotnet_core.sh`.

use std::path::Path;

/// The exact RUSTFLAGS the backend needs:
///   -Z codegen-backend=<dylib> -C linker=<linker> -C link-args=--cargo-support
///
/// getrandom needs NO cfg here: the `dotnet_overlays/getrandom-{0.2,0.3,0.4}` overlays
/// supply a self-contained `target_os="dotnet"` backend arm (the PAL CSPRNG). The old
/// `--cfg getrandom_backend="custom"` is removed — for 0.3/0.4 it is the FIRST branch
/// of getrandom's backend cascade, so it would win over the overlay's dotnet arm and
/// pull custom.rs's now-undefined `__getrandom_v03_custom` extern -> link error.
pub fn assemble(backend_dylib: &Path, linker: &Path) -> String {
    format!(
        "-Z codegen-backend={dylib} -C linker={linker} -C link-args=--cargo-support",
        dylib = backend_dylib.display(),
        linker = linker.display(),
    )
}
