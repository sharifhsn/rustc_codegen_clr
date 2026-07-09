//! Angle 2 of the auth investigation: can a Rust-defined type implement
//! `Microsoft.AspNetCore.Authorization.IAuthorizationHandler` and get invoked by a REAL ASP.NET
//! Core authorization pipeline (`WebApplication` + `.RequireAuthorization()`) when a protected
//! minimal-API endpoint is hit?
//!
//! Reuses the exact technique `cargo_tests/cd_bgservice` already proved for
//! `Microsoft.Extensions.Hosting.IHostedService`: `#[dotnet_class(implements = "[[Assembly]]Type")]`
//! declares the interface, and the method body stays fully synchronous, returning
//! `mycorrhiza::task::Task::completed()` (`Task.CompletedTask`) instead of touching `.await` — so
//! there's no coroutine state machine holding a managed reference across a suspension point (the
//! documented async ceiling in `mycorrhiza::task`'s own doc comments).
//!
//! `HandleAsync` reads `AuthorizationHandlerContext.User` (a real `ClaimsPrincipal` -- populated by
//! ASP.NET's standard JWT-bearer authentication middleware from a token minted/validated the same
//! way as `cd_auth`'s angle-1 proof), checks for a `"role"="admin"` claim, and calls
//! `context.Succeed(requirement)` only if present. `requirement` is a well-known singleton
//! (`CdAuthWeb.RoleRequirement.Instance`, a static property on a class defined in the C# host
//! project `authz_csharp/`) -- Rust never needs to enumerate `context.PendingRequirements` (which
//! would need `IEnumerable<T>` binding machinery this investigation doesn't attempt); it just
//! succeeds the SAME object reference the host wired into the policy's requirement list.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::{dotnet_class, dotnet_methods};
use mycorrhiza::intrinsics::{rustc_clr_interop_managed_checked_cast as checked_cast, RustcCLRInteropManagedClass};
use mycorrhiza::system::MString;
use mycorrhiza::task::Task;

const ASPNETCORE_AUTHZ: &str = "Microsoft.AspNetCore.Authorization";
const CLAIMS_NS: &str = "System.Security.Claims";
// The C# host project's own assembly name (see `authz_csharp/authz_cs.csproj`'s `AssemblyName`) --
// resolved at RUNTIME only (same "arbitrary AssemblyRef by name" mechanism `implements=` itself
// uses), never at Rust build time, so there's no build-order cycle even though the host also
// references this crate's `.dll`.
const HOST_ASM: &str = "authz_cs";

type ContextHandle =
    RustcCLRInteropManagedClass<ASPNETCORE_AUTHZ, "Microsoft.AspNetCore.Authorization.AuthorizationHandlerContext">;
type RequirementIfaceHandle =
    RustcCLRInteropManagedClass<ASPNETCORE_AUTHZ, "Microsoft.AspNetCore.Authorization.IAuthorizationRequirement">;
type ClaimsPrincipalHandle = RustcCLRInteropManagedClass<CLAIMS_NS, "System.Security.Claims.ClaimsPrincipal">;
type RoleRequirementHandle = RustcCLRInteropManagedClass<HOST_ASM, "CdAuthWeb.RoleRequirement">;
type RawTaskHandle = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Threading.Tasks.Task">;

/// Observed by the C# host after a request completes, so it can assert the Rust handler actually
/// ran (not just type-checked) without needing any output-capture machinery.
static HANDLE_CALLS: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
static SUCCEED_CALLS: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

#[dotnet_class(
    implements = "[Microsoft.AspNetCore.Authorization]Microsoft.AspNetCore.Authorization.IAuthorizationHandler",
    default_ctor = true
)]
pub struct RustAuthHandler {
    tag: i32,
}

#[dotnet_methods]
impl RustAuthHandler {
    /// `Task HandleAsync(AuthorizationHandlerContext context)` -- the sole `IAuthorizationHandler`
    /// member. Fully synchronous body; returns an already-completed `Task` (no `.await` inside
    /// Rust at all).
    pub fn HandleAsync(_this: RustAuthHandlerHandle, context: ContextHandle) -> RawTaskHandle {
        HANDLE_CALLS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);

        let user: ClaimsPrincipalHandle = context.instance0::<"get_User", ClaimsPrincipalHandle>();
        let has_admin_role: bool = user.instance2::<"HasClaim", MString, MString, bool>(
            MString::from("role"),
            MString::from("admin"),
        );

        if has_admin_role {
            let req: RoleRequirementHandle =
                RoleRequirementHandle::static0::<"get_Instance", RoleRequirementHandle>();
            let req_iface: RequirementIfaceHandle = checked_cast::<RequirementIfaceHandle, RoleRequirementHandle>(req);
            context.instance1::<"Succeed", RequirementIfaceHandle, ()>(req_iface);
            SUCCEED_CALLS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }

        Task::completed().raw()
    }

    /// Not part of `IAuthorizationHandler` -- plain static accessors the C# host reads to assert
    /// the Rust handler body actually executed (and how many times it succeeded the requirement).
    pub fn HandleCalls() -> i32 {
        HANDLE_CALLS.load(core::sync::atomic::Ordering::SeqCst)
    }

    pub fn SucceedCalls() -> i32 {
        SUCCEED_CALLS.load(core::sync::atomic::Ordering::SeqCst)
    }
}
