//! RUSTFLAGS assembly. Ports `_cargo_dotnet_core.sh`.

use std::path::Path;

/// The RUSTFLAGS the backend needs:
///   -Z codegen-backend=<dylib> -C linker=<linker> -C link-args=--cargo-support
///   --cfg cd_backend_<hash> --check-cfg=cfg(cd_backend_<hash>)
///
/// getrandom needs NO cfg here: the `dotnet_overlays/getrandom-{0.2,0.3,0.4}` overlays
/// supply a self-contained `target_os="dotnet"` backend arm (the PAL CSPRNG). The old
/// `--cfg getrandom_backend="custom"` is removed — for 0.3/0.4 it is the FIRST branch
/// of getrandom's backend cascade, so it would win over the overlay's dotnet arm and
/// pull custom.rs's now-undefined `__getrandom_v03_custom` extern -> link error.
///
/// The trailing `cd_backend_<hash>` cfg is the **backend-content cache key**: cargo's
/// build-std fingerprint hashes the RUSTFLAGS *string* (the `-Zcodegen-backend` PATH, not
/// the dylib's bytes), so a rebuilt backend at the same path would silently reuse STALE
/// `core`/`std`/`alloc` codegen — out-of-line items (panic_bounds_check, Location::caller,
/// …) keep their old behavior while only the user crate picks up the new backend. Folding
/// the dylib's content hash in busts the fingerprint EXACTLY when the backend changes (and
/// only then — an unchanged backend keeps the same key, so normal caching applies). The cfg
/// is inert (nothing reads it) and `--check-cfg`-declared (no `unexpected_cfgs` warning).
/// Matches the shell logic in `_cargo_dotnet_core.sh`.
pub fn assemble(backend_dylib: &Path, linker: &Path) -> String {
    let base = format!(
        "-Z codegen-backend={dylib} -C linker={linker} -C link-args=--cargo-support",
        dylib = backend_dylib.display(),
        linker = linker.display(),
    );
    match backend_content_key(backend_dylib) {
        Some(key) => format!("{base} --cfg cd_backend_{key} --check-cfg=cfg(cd_backend_{key})"),
        None => base,
    }
}

/// 16-hex-char FNV-1a digest of the backend dylib's bytes; `None` if it can't be read
/// (in which case we omit the cfg and fall back to the prior path-keyed behavior).
fn backend_content_key(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in &bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(format!("{h:016x}"))
}
