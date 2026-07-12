//! Proves `cargo dotnet pack`'s add-nuget transparency fix: this crate depends on CsvHelper via
//! `add-nuget`. When packed and consumed by a fresh C# console app that never runs `cargo dotnet`
//! at all (plain `dotnet add package` + `<PackageReference>`), NuGet's own restore must pull in
//! CsvHelper transitively via the real `<dependency>` entry `pack` now emits — not a raw bundled
//! dll `pack` used to silently omit.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, non_snake_case)]

pub mod nuget;

use nuget::csvhelper::CsvHelper::TypeConversion::{TypeConverterCache, TypeConverterCache_Methods};

#[unsafe(no_mangle)]
pub extern "C" fn csv_smoke() -> i32 {
    // Trivial use of the add-nuget-sourced type: prove the assembly actually loads and its type
    // is usable, not just that the reference compiles.
    let cache = TypeConverterCache::new();
    let _ = cache;
    42
}
