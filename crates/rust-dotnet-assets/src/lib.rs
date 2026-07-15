//! Reusable NuGet/native asset resolution, staging, and package projection.
//!
//! This crate owns the platform asset graph independently from the
//! `cargo-dotnet` command-line frontend. It can resolve SDK-selected NuGet
//! assets, stage them atomically for execution, and preserve their logical
//! package paths for redistribution.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedPackageAssetKind {
    Runtime,
    Native,
    Resource,
}

#[derive(Clone, Debug)]
pub struct StagedPackageAsset {
    pub logical_path: String,
    pub source: std::path::PathBuf,
    pub kind: StagedPackageAssetKind,
    pub rid: Option<String>,
}

mod assets;

pub use assets::{
    copy_staged_assets, missing_recorded_roots, package_assets, restore, stage_assets,
    AssetCollision, AssetKind, ResolvedAsset, ResolvedAssets,
};
