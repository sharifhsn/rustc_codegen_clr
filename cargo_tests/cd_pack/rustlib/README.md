# cd_pack

Probe crate for `cargo dotnet pack`'s NuGet package metadata. Not a real published
package — exists only so the pack.rs metadata plumbing has real `Cargo.toml` fields
(description, license, repository, version, readme) to source from and verify against.
