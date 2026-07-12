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
pub fn assemble(backend_dylib: &Path, linker: &Path, source_remaps: &[(&Path, &str)]) -> String {
    // `-Z inline-mir-hint-threshold=500`: raise rustc's MIR-inliner budget for `#[inline]`
    // items (iterator combinators, closures, small wrappers — the zero-cost-abstraction
    // surface). rustc inlines these conservatively because the native pipeline lets LLVM
    // finish the job; our backend hands MIR to RyuJIT, which won't inline struct-returning
    // adapter chains, so `(0..n).map(f).filter(g).sum()` would otherwise survive as a
    // per-element `Range::fold` CALL. Collapsing the chain at the MIR level (typed, with
    // real borrow info, battle-tested) gives RyuJIT the same flat loop LLVM gets for native.
    // Inert in debug (mir-opt-level 1 disables the MIR inliner); non-`#[inline]` fns keep the
    // default `threshold` (50).
    let base = format!(
        "-Z codegen-backend={dylib} -C linker={linker} -C link-args=--cargo-support \
         -Z inline-mir-hint-threshold=500",
        dylib = backend_dylib.display(),
        linker = linker.display(),
    );
    let remaps = source_remaps
        .iter()
        .map(|(source, logical)| {
            format!(
                " --remap-path-prefix={}={logical}",
                source.to_string_lossy()
            )
        })
        .collect::<String>();
    let base = format!("{base}{remaps}");
    match backend_content_key(backend_dylib) {
        Some(key) => format!("{base} --cfg cd_backend_{key} --check-cfg=cfg(cd_backend_{key})"),
        None => base,
    }
}

/// 16-hex-char FNV-1a digest of the backend dylib's bytes; `None` if it can't be read
/// (in which case we omit the cfg and fall back to the prior path-keyed behavior).
fn backend_content_key(path: &Path) -> Option<String> {
    let mut bytes = std::fs::read(path).ok()?;
    normalize_producer_binary(path, &mut bytes);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in &bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(format!("{h:016x}"))
}

/// Mach-O's linker-generated UUID changes between otherwise identical builds. It has no effect
/// on code generation, so exclude it from the semantic backend cache key.
pub(crate) fn normalize_producer_binary(path: &Path, bytes: &mut [u8]) {
    if bytes.get(..4) != Some(&[0xcf, 0xfa, 0xed, 0xfe]) || bytes.len() < 32 {
        return;
    }
    let ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let mut offset = 32usize;
    let mut linkedit = None;
    for _ in 0..ncmds {
        if offset.checked_add(8).is_none_or(|end| end > bytes.len()) {
            return;
        }
        let command = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if size < 8 || offset.checked_add(size).is_none_or(|end| end > bytes.len()) {
            return;
        }
        if command == 0x1b && size >= 24 {
            bytes[offset + 8..offset + 24].fill(0);
        }
        if command == 0xd {
            bytes[offset + 8..offset + size].fill(0);
        }
        if command == 0x19 && size >= 72 && &bytes[offset + 8..offset + 18] == b"__LINKEDIT" {
            let file_offset =
                u64::from_le_bytes(bytes[offset + 40..offset + 48].try_into().unwrap()) as usize;
            let file_size =
                u64::from_le_bytes(bytes[offset + 48..offset + 56].try_into().unwrap()) as usize;
            linkedit = Some((file_offset, file_size));
        }
        offset += size;
    }
    if let Some((start, size)) = linkedit
        && let Some(end) = start.checked_add(size)
        && end <= bytes.len()
    {
        bytes[start..end].fill(0);
    }
    // ld64 also records the dylib install path and LTO object paths. Cargo places producers under
    // `<workspace>/target/{profile}`; erase that workspace prefix from the semantic hash.
    if let Some(workspace) = path.parent().and_then(Path::parent).and_then(Path::parent) {
        zero_occurrences(bytes, workspace.as_os_str().as_encoded_bytes());
        let canonical = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_owned());
        zero_occurrences(bytes, canonical.as_os_str().as_encoded_bytes());
        if let Some(alias) = canonical
            .to_str()
            .and_then(|value| value.strip_prefix("/private"))
        {
            zero_occurrences(bytes, alias.as_bytes());
        }
    }
}

fn zero_occurrences(bytes: &mut [u8], needle: &[u8]) {
    if needle.is_empty() || needle.len() > bytes.len() {
        return;
    }
    let mut offset = 0;
    while let Some(relative) = bytes[offset..]
        .windows(needle.len())
        .position(|part| part == needle)
    {
        let start = offset + relative;
        bytes[start..start + needle.len()].fill(0);
        offset = start + needle.len();
    }
}
