
#[link(name = "async_callback")]
unsafe extern "C" {
    fn ac_live_workers() -> i32;
}

/// Calls a vendored native Rust library through the managed Rust assembly. The attached C# host
/// never handles a native path or copies a binary itself.
#[dotnet_export(name = "NativeAssetProbe")]
pub fn native_asset_probe() -> i32 {
    unsafe { ac_live_workers() }
}
