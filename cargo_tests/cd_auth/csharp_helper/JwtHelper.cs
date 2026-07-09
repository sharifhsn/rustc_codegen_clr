// Thin C# facade over System.IdentityModel.Tokens.Jwt / Microsoft.IdentityModel.Tokens, exposed to
// Rust as a flat, low-arity (<=2 args), string/int-only static API.
//
// Why a hand-written facade instead of `cargo dotnet add-nuget System.IdentityModel.Tokens.Jwt`:
// the library's real entry points (`JwtSecurityTokenHandler.ValidateToken(string, TokenValidationParameters,
// out SecurityToken)`, `SecurityTokenDescriptor` property-bag construction, `ClaimsPrincipal`/`ClaimsIdentity`
// traversal) use an `out` parameter and non-trivial object graphs that spinacz's reflection bindgen
// explicitly refuses to emit (see cargo_tests/spinacz/src/reflect.rs: "no ref/out params" in the doc,
// mirrored by cargo_tests/cd_efcore's own hand-written-handle precedent for EF Core's similarly complex
// surface). Hiding all of that behind a small facade with only `string`/`int` parameters/returns keeps
// the Rust side to plain `RustcCLRInteropManagedClass::staticN` calls -- no ref/out, no generics, no
// object-graph walking needed from Rust.
using System;
using System.IdentityModel.Tokens.Jwt;
using System.Security.Claims;
using System.Text;
using Microsoft.IdentityModel.Tokens;

namespace CdAuth;

public static class JwtHelper
{
    // A fixed issuer/audience for this proof (a real app would parameterize these too, but two more
    // string args would exceed the static2 arity `mycorrhiza`'s intrinsics currently expose).
    private const string Issuer = "cd_auth_issuer";
    private const string Audience = "cd_auth_audience";

    // Configured once via `Configure(secret)` before Create/Validate calls -- keeps every other
    // entry point to <=2 args instead of threading the key through each call.
    private static string _secret = string.Empty;

    public static void Configure(string secret) => _secret = secret;

    private static SymmetricSecurityKey Key() =>
        new SymmetricSecurityKey(Encoding.UTF8.GetBytes(_secret));

    /// Construct + sign a real JWT (HMAC-SHA256) carrying a `sub` claim and a custom `role` claim.
    /// `expiresMinutes` may be negative to mint an already-expired token (used to prove expiry
    /// validation actually runs, not just signature checking).
    public static string CreateToken(string subject, int expiresMinutes)
    {
        var claims = new[]
        {
            new Claim(JwtRegisteredClaimNames.Sub, subject),
            new Claim("role", "admin"),
        };
        var creds = new SigningCredentials(Key(), SecurityAlgorithms.HmacSha256);
        var token = new JwtSecurityToken(
            issuer: Issuer,
            audience: Audience,
            claims: claims,
            expires: DateTime.UtcNow.AddMinutes(expiresMinutes),
            signingCredentials: creds);
        return new JwtSecurityTokenHandler().WriteToken(token);
    }

    /// Validate signature + issuer/audience/lifetime. Returns the `sub` claim on success, or one of
    /// a small set of sentinel strings on failure -- distinguishing failure *reasons* is exactly the
    /// kind of claims-graph inspection (`SecurityTokenException` subtype dispatch) that's easiest to
    /// do here in C# and hand back to Rust as a plain string rather than exposing exception types.
    public static string ValidateAndGetSubject(string token)
    {
        var handler = new JwtSecurityTokenHandler();
        // Without this, the handler silently remaps short claim names ("sub") to long legacy
        // XML-SOAP claim-type URIs (its default `DefaultInboundClaimTypeMap`) before handing back
        // the `ClaimsPrincipal` -- so `FindFirst(JwtRegisteredClaimNames.Sub)` (which looks for the
        // literal short name) would silently miss even a successfully validated token. This is a
        // well-known .NET gotcha, not a rustc_codegen_clr limitation.
        handler.MapInboundClaims = false;
        var parameters = new TokenValidationParameters
        {
            ValidateIssuer = true,
            ValidIssuer = Issuer,
            ValidateAudience = true,
            ValidAudience = Audience,
            ValidateLifetime = true,
            ClockSkew = TimeSpan.Zero,
            IssuerSigningKey = Key(),
        };
        try
        {
            // `out SecurityToken` -- the very "out param" spinacz's bindgen would refuse to emit --
            // stays entirely inside this C# method; Rust never sees it.
            ClaimsPrincipal principal = handler.ValidateToken(token, parameters, out _);
            return principal.FindFirst(JwtRegisteredClaimNames.Sub)?.Value ?? "__NO_SUB__";
        }
        catch (SecurityTokenExpiredException)
        {
            return "__EXPIRED__";
        }
        catch (SecurityTokenInvalidSignatureException)
        {
            return "__BAD_SIGNATURE__";
        }
        catch (SecurityTokenException)
        {
            return "__INVALID__";
        }
        catch (Exception)
        {
            return "__MALFORMED__";
        }
    }

    /// Same as <see cref="CreateToken"/> but with an explicit `role` claim value instead of the
    /// hardcoded `"admin"` -- used by the angle-2 (ASP.NET Core `[Authorize]` pipeline) proof to
    /// mint both admin and non-admin tokens. A plain C#-to-C# overload (not called from Rust, so
    /// it isn't constrained to the <=2-arg `staticN` intrinsics the angle-1 Rust proof uses).
    public static string CreateTokenWithRole(string subject, string role, int expiresMinutes)
    {
        var claims = new[]
        {
            new Claim(JwtRegisteredClaimNames.Sub, subject),
            new Claim("role", role),
        };
        var creds = new SigningCredentials(Key(), SecurityAlgorithms.HmacSha256);
        var token = new JwtSecurityToken(
            issuer: Issuer,
            audience: Audience,
            claims: claims,
            expires: DateTime.UtcNow.AddMinutes(expiresMinutes),
            signingCredentials: creds);
        return new JwtSecurityTokenHandler().WriteToken(token);
    }

    /// Read back the custom `role` claim WITHOUT re-validating the signature (mirrors
    /// `JwtSecurityTokenHandler.ReadJwtToken`, the no-signature-check "peek" API) -- proves claims
    /// round-trip through the payload, not just the `sub` claim `ValidateAndGetSubject` already covers.
    public static string PeekRoleClaim(string token)
    {
        var jwt = new JwtSecurityTokenHandler().ReadJwtToken(token);
        return jwt.Claims.FirstOrDefaultRole();
    }

    private static string FirstOrDefaultRole(this System.Collections.Generic.IEnumerable<Claim> claims)
    {
        foreach (var c in claims)
        {
            if (c.Type == "role") return c.Value;
        }
        return "__NO_ROLE__";
    }
}
