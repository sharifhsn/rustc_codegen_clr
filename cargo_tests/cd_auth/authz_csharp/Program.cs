// Angle 2 of the auth investigation: can a Rust-defined type implement
// `Microsoft.AspNetCore.Authorization.IAuthorizationHandler` and get invoked by a REAL ASP.NET
// Core authorization pipeline when a protected minimal-API endpoint is hit?
//
// `RustAuthHandler` (from `cd_auth_authz.dll`, see `authz_rustlib/src/lib.rs`) implements
// `IAuthorizationHandler` directly. It checks the current `ClaimsPrincipal` (populated by a small
// custom middleware below that validates a Bearer JWT via the angle-1 `JwtHelper` facade) for a
// `role=admin` claim and, if present, calls `context.Succeed(RoleRequirement.Instance)` -- the
// SAME singleton object reference wired into the "AdminRole" policy below, so Rust never needs to
// enumerate `context.PendingRequirements`.
//
// HISTORICAL BACKEND GAP hit here (documented at length in the sibling `cd_bgservice/csharp/Program.cs`
// for `IHostedService`, and identical in kind for `IAuthorizationHandler`, now FIXED): the backend's
// `is_bcl_assembly` heuristic used to treat every `Microsoft.*`-named assembly as CoreLib-signed and
// stamp `#[dotnet_class(implements = "[Microsoft.AspNetCore.Authorization]...")]`'s emitted
// `AssemblyRef` with CoreLib's own ECMA public-key token. `Microsoft.AspNetCore.Authorization.dll`
// is part of the shared framework but is actually signed with a DIFFERENT, Microsoft
// "extensions/aspnetcore" token (`adb9793829ddae60`) -- a live reflection probe of the real net8/net9
// ref packs confirms this (see the detailed comment at the registration call below). So this crate
// used to need the exact same non-generic-registration workaround `cd_bgservice` did; both are fixed
// now (`cilly/src/ir/{il_exporter/mod.rs,pe_exporter/tables.rs}::bcl_public_key_token`,
// `docs/RUST_PARITY_ROADMAP.md` Tier-0 item 3) and this file now uses the idiomatic generic
// `AddSingleton<TService, TImplementation>()` registration below.

using System;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Security.Claims;
using System.Text.Encodings.Web;
using System.Threading.Tasks;
using CdAuth;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Builder;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;

int pass = 0, total = 0;
void Check(string name, bool ok)
{
    total++;
    if (ok) pass++;
    Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}");
}

const string Secret = "authz-demo-secret-key-0123456789";
JwtHelper.Configure(Secret);

var builder = WebApplication.CreateBuilder();
builder.Logging.ClearProviders();

// A no-op authentication scheme -- registers `IAuthenticationService` (required by ASP.NET Core's
// `AuthorizationMiddlewareResultHandler` even to call `ForbidAsync`/`ChallengeAsync` on failure,
// independent of who actually authenticates the user) without doing any real authentication work:
// the custom Bearer-JWT middleware below sets `context.User` itself via the angle-1 `JwtHelper`
// facade, entirely outside this scheme's `HandleAuthenticateAsync`.
builder.Services.AddAuthentication("Noop").AddScheme<AuthenticationSchemeOptions, NoopAuthHandler>("Noop", null);

builder.Services.AddAuthorization(options =>
{
    options.AddPolicy("AdminRole", policy => policy.AddRequirements(CdAuthWeb.RoleRequirement.Instance));
});

// FIXED: this used to hit the exact same backend gap `cd_bgservice/csharp/Program.cs` documents at
// length for `IHostedService` -- `#[dotnet_class(implements = "[Microsoft.AspNetCore.Authorization]...")]`
// used to mis-stamp `cd_auth_authz.dll`'s `AssemblyRef` for `Microsoft.AspNetCore.Authorization` with
// CoreLib's ECMA public-key token (`b03f5f7f11d50a3a`) via the backend's old `is_bcl_assembly`
// `name.starts_with("Microsoft")` heuristic; the REAL `Microsoft.AspNetCore.Authorization.dll`
// Roslyn resolves via `Microsoft.NET.Sdk.Web`'s `FrameworkReference` carries a DIFFERENT token
// (`adb9793829ddae60`, confirmed via a live reflection probe of the net8/net9 ref packs), so
// `builder.Services.AddSingleton<IAuthorizationHandler, RustAuthHandler>()` used to fail exactly
// like `AddHostedService<SyncWorker>()` did: `CS0311` (no implicit reference conversion) + `CS0012`
// (type defined in an unreferenced assembly identity). The backend now stamps the real
// `adb9793829ddae60` token for the `Microsoft.AspNetCore.*`/`Microsoft.Extensions.*` family
// (`bcl_public_key_token`, Tier-0 item 3), so the idiomatic generic registration below now compiles
// and dispatches through ASP.NET Core's own DI/reflection code exactly as expected.
builder.Services.AddSingleton<IAuthorizationHandler, RustAuthHandler>();

var app = builder.Build();

app.UseAuthentication();

app.Use(async (context, next) =>
{
    try
    {
        await next();
    }
    catch (Exception ex)
    {
        Console.WriteLine($"  [EXCEPTION] {ex}");
        throw;
    }
});

// A minimal hand-rolled "Bearer JWT -> ClaimsPrincipal" middleware (standing in for
// `AddAuthentication().AddJwtBearer(...)`, which would need an extra NuGet package not otherwise
// needed by this investigation). Reuses the angle-1 `JwtHelper.ValidateAndGetSubject`/`PeekRoleClaim`
// facade -- the SAME real `System.IdentityModel.Tokens.Jwt` validation already proven there.
app.Use(async (context, next) =>
{
    var authHeader = context.Request.Headers.Authorization.ToString();
    if (authHeader.StartsWith("Bearer ", StringComparison.Ordinal))
    {
        var token = authHeader["Bearer ".Length..];
        var subject = JwtHelper.ValidateAndGetSubject(token);
        bool valid = subject is not ("__EXPIRED__" or "__BAD_SIGNATURE__" or "__INVALID__"
            or "__MALFORMED__" or "__NO_SUB__");
        if (valid)
        {
            var role = JwtHelper.PeekRoleClaim(token);
            var identity = new ClaimsIdentity(
                new[] { new Claim("sub", subject), new Claim("role", role) },
                "Bearer");
            context.User = new ClaimsPrincipal(identity);
        }
    }
    await next();
});

app.UseAuthorization();

app.MapGet("/public", () => Results.Ok("no auth needed"));

app.MapGet("/protected", (ClaimsPrincipal user) =>
    Results.Ok(new { message = "secret admin data", subject = user.FindFirst("sub")?.Value }))
    .RequireAuthorization("AdminRole");

app.MapGet("/rustcalls", () => Results.Ok(new
{
    handleCalls = RustAuthHandler.HandleCalls(),
    succeedCalls = RustAuthHandler.SucceedCalls(),
}));

const string BaseUrl = "http://127.0.0.1:5299";
_ = app.RunAsync(BaseUrl);
await Task.Delay(600); // give Kestrel a moment to actually start listening

using var http = new HttpClient();

// 1) No Authorization header at all -> the Rust handler runs (User has no claims), never Succeeds
//    -> 403 Forbidden.
var noTokenResp = await http.GetAsync($"{BaseUrl}/protected");
Console.WriteLine($"  [debug] no-token status: {(int)noTokenResp.StatusCode} {noTokenResp.StatusCode}");
// Correct ASP.NET Core semantics: an entirely UNAUTHENTICATED request gets Challenged (401), not
// Forbidden (403) -- Forbidden means "we know who you are, but the policy said no". The Rust
// handler still runs either way (see the `handleCalls == 3` check below) -- it just never calls
// `Succeed` because `ClaimsPrincipal.HasClaim("role","admin")` is false on an anonymous principal.
Check("no token -> 401 Unauthorized", noTokenResp.StatusCode == System.Net.HttpStatusCode.Unauthorized);

// 2) A validly-signed token, but WITHOUT the admin role -> Rust handler runs, checks the claim,
//    does NOT Succeed -> still 403.
var userToken = JwtHelper.CreateTokenWithRole("carol", "user", 30);
using var userReq = new HttpRequestMessage(HttpMethod.Get, $"{BaseUrl}/protected");
userReq.Headers.Authorization = new AuthenticationHeaderValue("Bearer", userToken);
var userResp = await http.SendAsync(userReq);
Console.WriteLine($"  [debug] non-admin status: {(int)userResp.StatusCode} {userResp.StatusCode}");
Check("valid token, non-admin role -> 403 Forbidden", userResp.StatusCode == System.Net.HttpStatusCode.Forbidden);

// 3) A validly-signed token WITH the admin role -> Rust handler runs, HasClaim("role","admin")
//    is true, calls context.Succeed(RoleRequirement.Instance) -> ASP.NET's real authorization
//    middleware lets the request through -> 200 OK, and the handler body ran inside a genuine
//    ASP.NET Core request pipeline (not a hand-rolled call).
var adminToken = JwtHelper.CreateTokenWithRole("alice", "admin", 30);
using var adminReq = new HttpRequestMessage(HttpMethod.Get, $"{BaseUrl}/protected");
adminReq.Headers.Authorization = new AuthenticationHeaderValue("Bearer", adminToken);
var adminResp = await http.SendAsync(adminReq);
Console.WriteLine($"  [debug] admin status: {(int)adminResp.StatusCode} {adminResp.StatusCode}");
Check("valid token, admin role -> 200 OK", adminResp.StatusCode == System.Net.HttpStatusCode.OK);
var adminBody = await adminResp.Content.ReadAsStringAsync();
Console.WriteLine($"  protected response: {adminBody}");
Check("response body carries the JWT subject (\"alice\")", adminBody.Contains("alice"));

// 4) Public endpoint never touches authorization at all -> always 200, sanity control.
var publicResp = await http.GetAsync($"{BaseUrl}/public");
Check("public endpoint -> 200 OK (no auth involved)", publicResp.StatusCode == System.Net.HttpStatusCode.OK);

// 5) The Rust handler body genuinely executed 3 times (once per /protected request above) and
//    succeeded the requirement exactly once (only the admin-role request) -- read back through the
//    plain static accessors `RustAuthHandler` exposes (NOT part of `IAuthorizationHandler`), proving
//    the counts observed from Rust's own side match what the HTTP responses imply.
var callsResp = await http.GetAsync($"{BaseUrl}/rustcalls");
var callsBody = await callsResp.Content.ReadAsStringAsync();
Console.WriteLine($"  rustcalls: {callsBody}");
Check("HandleAsync ran exactly 3 times (one per /protected request)", callsBody.Contains("\"handleCalls\":3"));
Check("Succeed was called exactly once (only the admin-role request)", callsBody.Contains("\"succeedCalls\":1"));

Console.WriteLine($"cd_auth (angle 2, authz): {pass}/{total} checks passed");
Environment.Exit(pass == total ? 0 : 1);

/// Registers `IAuthenticationService` with zero real authentication behavior -- always reports "no
/// result", leaving `context.User` exactly as the custom Bearer-JWT middleware set it.
public sealed class NoopAuthHandler : AuthenticationHandler<AuthenticationSchemeOptions>
{
    public NoopAuthHandler(IOptionsMonitor<AuthenticationSchemeOptions> options, ILoggerFactory logger, UrlEncoder encoder)
        : base(options, logger, encoder)
    {
    }

    protected override Task<AuthenticateResult> HandleAuthenticateAsync() =>
        Task.FromResult(AuthenticateResult.NoResult());
}

namespace CdAuthWeb
{
    /// A trivial marker requirement -- Rust's `RustAuthHandler.HandleAsync` succeeds THIS singleton
    /// object reference directly (via `RoleRequirementHandle::static0::<"get_Instance", ...>`),
    /// rather than enumerating `context.PendingRequirements` to find "the" pending requirement
    /// (which would need `IEnumerable<T>` interop machinery this investigation doesn't attempt).
    public sealed class RoleRequirement : IAuthorizationRequirement
    {
        private static readonly RoleRequirement _instance = new();
        public static RoleRequirement Instance => _instance;
    }
}
