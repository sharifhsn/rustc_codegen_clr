use dotnet_macros::dotnet_export;

#[dotnet_export(name = "LibraryName")]
pub fn library_name() -> &'static str {
    "beta"
}
