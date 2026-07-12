use dotnet_macros::dotnet_export;

/// Return a marker through an assembly whose CLR name differs from its Cargo package.
#[dotnet_export(name = "LibraryName")]
pub fn library_name() -> &'static str {
    "custom"
}
