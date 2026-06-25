use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

// Render bytes as lowercase hex without allocating intermediate Strings per byte
// and without any nondeterminism. Deterministic, allocation-light.
fn hex(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(LUT[(b >> 4) as usize] as char);
        out.push(LUT[(b & 0x0f) as usize] as char);
    }
    out
}

fn main() {
    // FIXED 32-byte seed -> fully deterministic key material + signature.
    // (Ed25519 signing is deterministic given key + message: no RNG involved.)
    let seed: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];

    let signing_key: SigningKey = SigningKey::from_bytes(&seed);
    let verifying_key: VerifyingKey = signing_key.verifying_key();

    println!("seed_hex = {}", hex(&seed));
    println!("verifying_key_hex = {}", hex(verifying_key.as_bytes()));

    let message: &[u8] = b"rustc_codegen_clr survey: ed25519-dalek deterministic test";
    println!("message_len = {}", message.len());

    // Sign. `Signer::sign` is infallible for ed25519-dalek (returns Signature).
    let signature: Signature = signing_key.sign(message);
    let sig_bytes: [u8; 64] = signature.to_bytes();
    println!("signature_hex = {}", hex(&sig_bytes));

    // Verify the correct signature against the correct message.
    let verify_ok = verifying_key.verify(message, &signature).is_ok();
    println!("verify_ok = {}", verify_ok);

    // Negative control: a tampered message must NOT verify (deterministic bool).
    let tampered: &[u8] = b"rustc_codegen_clr survey: ed25519-dalek deterministic TEST";
    let verify_tampered = verifying_key.verify(tampered, &signature).is_ok();
    println!("verify_tampered_ok = {}", verify_tampered);

    // Round-trip the signature and verifying key through their byte encodings
    // and re-verify, to exercise the parse/serialize surface deterministically.
    let vk_roundtrip = match VerifyingKey::from_bytes(verifying_key.as_bytes()) {
        Ok(vk) => vk.verify(message, &signature).is_ok(),
        Err(_) => false,
    };
    println!("vk_roundtrip_verify_ok = {}", vk_roundtrip);

    let sig_roundtrip = Signature::from_bytes(&sig_bytes);
    let sig_roundtrip_ok = verifying_key.verify(message, &sig_roundtrip).is_ok();
    println!("sig_roundtrip_verify_ok = {}", sig_roundtrip_ok);

    println!("== survey_ed25519-dalek done ==");
}
