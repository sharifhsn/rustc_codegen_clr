//! H2 real-crate SOAK: the `uuid` crate on the dotnet PAL.
//! Deterministic Uuid::from_bytes -> hyphenated string formatting (exercises fmt, byte->hex).
//! Then Uuid::new_v4() (exercises the RNG path / getrandom on the PAL) and check it's v4.
//! Panic-safe: fixed bytes, no unwraps on fallible ops. SUCCESS = "== soak_uuid done ==".
use uuid::Uuid;

// getrandom 0.4 custom backend -> dotnet PAL CSPRNG. uuid's "v4" feature pulls
// getrandom 0.4, which rejects os="dotnet" unless a custom backend is provided.
// 0.4 uses the identical `__getrandom_v03_custom` symbol + `getrandom_backend`
// cfg as 0.3 (set in feasibility/dev.sh pal-build).
#[no_mangle]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    getrandom_dotnet::fill(unsafe { core::slice::from_raw_parts_mut(dest, len) });
    Ok(())
}

fn main() {
    println!("== soak_uuid start ==");

    // Deterministic UUID from fixed bytes.
    let bytes: [u8; 16] = [
        0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4, 0xa7, 0x16, 0x44, 0x66, 0x55, 0x44, 0x00,
        0x00,
    ];
    let u = Uuid::from_bytes(bytes);
    println!("1  from_bytes hyphenated: {}", u.hyphenated());
    println!("2  version_num: {}", u.get_version_num());
    println!("3  is_nil: {}", u.is_nil());

    // Round-trip: bytes back out should match.
    let out = u.as_bytes();
    println!("4  bytes_match: {}", out == &bytes);

    // Nil UUID (deterministic).
    let nil = Uuid::nil();
    println!("5  nil hyphenated: {}", nil.hyphenated());

    // Random v4 UUID -> uses the RNG / getrandom path on the PAL.
    let v4 = Uuid::new_v4();
    println!("6  v4 is_version_4: {}", v4.get_version_num() == 4);
    // A v4 is overwhelmingly unlikely to be nil; report it as a sanity signal (no assert).
    println!("7  v4 not_nil: {}", !v4.is_nil());

    println!("== soak_uuid done ==");
}
