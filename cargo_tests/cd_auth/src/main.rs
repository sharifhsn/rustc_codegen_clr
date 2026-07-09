//! Angle 1 of the auth exploration: validate/produce a real JWT from Rust, calling into
//! `System.IdentityModel.Tokens.Jwt` / `Microsoft.IdentityModel.Tokens` through a thin C# facade
//! (`csharp_helper/JwtHelper.cs`) rather than `cargo dotnet add-nuget`-generated reflection bindings
//! -- the library's real entry points use an `out` parameter (`ValidateToken(string,
//! TokenValidationParameters, out SecurityToken)`) and multi-property object graphs
//! (`SecurityTokenDescriptor`, `ClaimsPrincipal`) that spinacz's bindgen explicitly won't emit (see
//! `cargo_tests/spinacz/src/reflect.rs`'s doc, and `cargo_tests/cd_efcore` for the same precedent
//! applied to EF Core). The facade collapses everything to `string`/`int`-only, <=2-arg static
//! methods, wired in exactly like `cd_efcore`'s `EfHelper.dll`: built with plain `dotnet build`,
//! copied (with its own transitive deps) into `.cargo-dotnet-nuget-assets/`, resolved at runtime via
//! a real `AssemblyRef` the PE writer emits for "JwtHelper".
#![allow(dead_code)]

use mycorrhiza::intrinsics::RustcCLRInteropManagedClass;
use mycorrhiza::system::console::Console;
use mycorrhiza::system::{DotNetString, MString};

type JwtHelperHandle = RustcCLRInteropManagedClass<"JwtHelper", "CdAuth.JwtHelper">;

fn mstr_to_rust(s: MString) -> String {
    DotNetString::from_handle(s).to_rust_string()
}

fn rstr_to_mstr(s: &str) -> MString {
    DotNetString::from(s).handle()
}

fn say(label: &str, s: &str) {
    Console::writeln_string(DotNetString::from(format!("{label}: {s}").as_str()).handle());
}

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr, $label:expr) => {{
            total += 1;
            let got = $got;
            let want = $want;
            if got == want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
                say(&format!("FAIL[{}] got", $label), &format!("{:?}", got));
                say(&format!("FAIL[{}] want", $label), &format!("{:?}", want));
            }
        }};
    }

    // Configure a shared secret once (kept out of the per-call arg lists to stay within the
    // <=2-arg `staticN` intrinsics `mycorrhiza::intrinsics::RustcCLRInteropManagedClass` exposes).
    JwtHelperHandle::static1::<"Configure", MString, ()>(rstr_to_mstr("super-secret-signing-key-01234567"));

    // 1) Construct a real signed JWT from Rust (HMAC-SHA256, via JwtSecurityToken +
    //    JwtSecurityTokenHandler.WriteToken under the hood).
    let token: MString =
        JwtHelperHandle::static2::<"CreateToken", MString, i32, MString>(rstr_to_mstr("alice"), 30);
    let token_str = mstr_to_rust(token);
    say("token", &token_str);
    chk!(token_str.matches('.').count(), 2, "token has 3 dot-separated JWS parts");
    chk!(token_str.starts_with("eyJ"), true, "token starts with base64url JSON header");

    // 2) Validate FROM Rust: signature + issuer/audience/lifetime checked entirely inside
    //    JwtSecurityTokenHandler.ValidateToken (the out-param call is hidden in the C# facade); the
    //    `sub` claim comes back correctly as "alice".
    let subject: MString = JwtHelperHandle::static1::<"ValidateAndGetSubject", MString, MString>(
        rstr_to_mstr(&token_str),
    );
    let subject_str = mstr_to_rust(subject);
    chk!(subject_str, "alice".to_string(), "validated subject claim round-trips");

    // 3) A custom claim ("role") also round-trips through the payload (peeked without
    //    re-validating the signature, mirroring JwtSecurityTokenHandler.ReadJwtToken).
    let role: MString =
        JwtHelperHandle::static1::<"PeekRoleClaim", MString, MString>(rstr_to_mstr(&token_str));
    chk!(mstr_to_rust(role), "admin".to_string(), "custom role claim round-trips");

    // 4) Tampering is caught: flip one character in the payload segment -> signature check fails.
    let tampered = tamper(&token_str);
    let tampered_result: MString = JwtHelperHandle::static1::<"ValidateAndGetSubject", MString, MString>(
        rstr_to_mstr(&tampered),
    );
    // The tamper flips a byte inside the base64url payload segment; depending on which byte, the
    // handler either rejects it as a signature mismatch (payload still decodes to *some* JSON) or
    // as malformed (the flipped byte breaks JSON parsing before signature verification runs) --
    // either way it must NOT come back as the original "alice" subject.
    chk!(
        mstr_to_rust(tampered_result) != "alice",
        true,
        "tampered token rejected (not accepted as alice)"
    );

    // 5) Expiry is enforced: mint a token that already expired 5 minutes ago -> rejected.
    let expired_token: MString =
        JwtHelperHandle::static2::<"CreateToken", MString, i32, MString>(rstr_to_mstr("bob"), -5);
    let expired_result: MString = JwtHelperHandle::static1::<"ValidateAndGetSubject", MString, MString>(
        rstr_to_mstr(&mstr_to_rust(expired_token)),
    );
    chk!(
        mstr_to_rust(expired_result),
        "__EXPIRED__".to_string(),
        "expired token rejected"
    );

    // 6) A completely different secret can't validate a token signed with the first one -- proves
    //    the signature check is real (not just format validation), independent of check #4's
    //    string-mangling tamper.
    JwtHelperHandle::static1::<"Configure", MString, ()>(rstr_to_mstr("a-totally-different-key-9999999"));
    let wrong_key_result: MString = JwtHelperHandle::static1::<"ValidateAndGetSubject", MString, MString>(
        rstr_to_mstr(&token_str),
    );
    chk!(
        mstr_to_rust(wrong_key_result),
        "__BAD_SIGNATURE__".to_string(),
        "wrong signing key rejected"
    );

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

/// Flip one character inside the JWT's payload (second dot-separated) segment, keeping the string
/// well-formed base64url-ish so the tamper is a signature-mismatch, not a parse failure.
fn tamper(token: &str) -> String {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return token.to_string();
    }
    let mut payload: Vec<u8> = parts[1].as_bytes().to_vec();
    if let Some(b) = payload.first_mut() {
        *b = if *b == b'A' { b'B' } else { b'A' };
    }
    format!("{}.{}.{}", parts[0], String::from_utf8_lossy(&payload), parts[2])
}
