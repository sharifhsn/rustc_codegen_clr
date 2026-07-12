//! Trivial exported surface — this crate exists to exercise `cargo dotnet pack`'s NuGet
//! metadata plumbing (see `tools/cargo-dotnet/src/pack.rs`), not to test codegen.

#[unsafe(no_mangle)]
pub extern "C" fn cd_pack_answer() -> i32 {
    42
}
