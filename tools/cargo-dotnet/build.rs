use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_DOTNET_BUILD_ID");
    let build_id = env::var("CARGO_DOTNET_BUILD_ID")
        .unwrap_or_else(|_| format!("source-sha256:{}", "0".repeat(64)));
    assert!(
        !build_id.is_empty()
            && build_id.len() <= 256
            && build_id
                .bytes()
                .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\'')),
        "CARGO_DOTNET_BUILD_ID must be a non-empty printable token of at most 256 bytes"
    );
    let receipt = format!(
        "CARGO_DOTNET_BUILD_ID_RECEIPT_V1_BEGIN:{build_id}:CARGO_DOTNET_BUILD_ID_RECEIPT_V1_END"
    );
    let generated = format!(
        "pub(crate) const EMBEDDED_DRIVER_BUILD_ID: &str = {build_id:?};\n\
         #[used]\n\
         pub(crate) static CARGO_DOTNET_BUILD_ID_BINARY_RECEIPT: [u8; {}] = *b{receipt:?};\n",
        receipt.len()
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo did not set OUT_DIR"))
        .join("cargo_dotnet_build_identity.rs");
    fs::write(output, generated).expect("writing cargo-dotnet build identity receipt");
    println!("cargo:rustc-env=CARGO_DOTNET_EMBEDDED_BUILD_ID={build_id}");
}
