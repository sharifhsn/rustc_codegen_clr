//! H2 real-crate SOAK: hmac + sha2 doing HMAC-SHA256 on the dotnet PAL.
//! Exercises the RustCrypto Mac/Update traits, generic-array, block-buffer, sha2 compression,
//! key ipad/opad mixing, and a known-answer test (RFC 4231 Test Case 2). Panic-safe (no unwraps
//! on fallible data; new_from_slice handles any key length). SUCCESS = "== soak_hmac done ==".
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('?'));
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('?'));
    }
    s
}

fn mac_hex(key: &[u8], msg: &[u8]) -> String {
    match HmacSha256::new_from_slice(key) {
        Ok(mut mac) => {
            mac.update(msg);
            let out = mac.finalize().into_bytes();
            to_hex(&out)
        }
        Err(e) => format!("key err: {e}"),
    }
}

fn main() {
    println!("== soak_hmac start ==");

    // RFC 4231 Test Case 2: key="Jefe", data="what do ya want for nothing?"
    // expected = 5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843
    let key = b"Jefe";
    let msg = b"what do ya want for nothing?";
    let got = mac_hex(key, msg);
    let expected = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";
    println!("1  hmac-sha256(Jefe) = {got}");
    println!("2  matches RFC4231 TC2 = {}", got == expected);

    // Incremental update should match one-shot.
    let one_shot = mac_hex(b"secret-key", b"hello world");
    let incremental = match HmacSha256::new_from_slice(b"secret-key") {
        Ok(mut mac) => {
            mac.update(b"hello ");
            mac.update(b"world");
            to_hex(&mac.finalize().into_bytes())
        }
        Err(e) => format!("key err: {e}"),
    };
    println!("3  one-shot   = {one_shot}");
    println!("4  incremental= {incremental}");
    println!("5  one-shot == incremental = {}", one_shot == incremental);

    // Long key (> block size, forces key hashing path) and empty message.
    let long_key = [0xaau8; 131];
    let lk = mac_hex(&long_key, b"");
    println!("6  long-key empty-msg len = {}", lk.len());

    // verify() round-trip on a freshly computed tag.
    if let Ok(mut mac) = HmacSha256::new_from_slice(b"verify-key") {
        mac.update(b"payload");
        let tag = mac.finalize().into_bytes();
        if let Ok(mut mac2) = HmacSha256::new_from_slice(b"verify-key") {
            mac2.update(b"payload");
            let verified = mac2.verify_slice(&tag).is_ok();
            println!("7  verify_slice roundtrip ok = {verified}");
        }
    }

    println!("== soak_hmac done ==");
}
