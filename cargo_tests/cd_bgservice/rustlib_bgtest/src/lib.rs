//! THROWAWAY probe (not part of the shipped `cd_bgservice` proof crate): does
//! `#[dotnet_class(extends = "...BackgroundService")]` + `#[dotnet_override("...BackgroundService")]`
//! on `ExecuteAsync` even get past the Rust/backend compile step? Kept in its own crate so failure
//! here doesn't disturb the working `cd_bgservice` demo.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::{dotnet_class, dotnet_methods};
use mycorrhiza::bindings::System::Threading::Tasks::Task as RawTaskHandle;
use mycorrhiza::intrinsics::RustcCLRInteropManagedStruct;
use mycorrhiza::task::Task;

const CORELIB: &str = "System.Private.CoreLib";
type RawCancellationToken =
    RustcCLRInteropManagedStruct<CORELIB, "System.Threading.CancellationToken", { core::mem::size_of::<usize>() }>;

#[dotnet_class(
    extends = "[Microsoft.Extensions.Hosting.Abstractions]Microsoft.Extensions.Hosting.BackgroundService",
    default_ctor = true
)]
pub struct RustBgService {
    tag: i32,
}

#[dotnet_methods]
impl RustBgService {
    #[dotnet_override("[Microsoft.Extensions.Hosting.Abstractions]Microsoft.Extensions.Hosting.BackgroundService")]
    pub fn ExecuteAsync(_this: RustBgServiceHandle, _ct: RawCancellationToken) -> RawTaskHandle {
        Task::completed().raw()
    }
}
