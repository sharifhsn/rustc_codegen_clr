use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use chacha20::ChaCha20;

// Lowercase hex of a byte slice (deterministic, no allocation surprises).
fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn main() {
    // Fixed 256-bit key and 96-bit nonce (RFC 8439 ChaCha20 uses a 12-byte nonce).
    let key: [u8; 32] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f,
    ];
    let nonce: [u8; 12] = [
        0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b,
    ];

    // Fixed plaintext buffer.
    let plaintext: &[u8] = b"The quick brown fox jumps over the lazy dog. ChaCha20!";
    println!("plaintext_len = {}", plaintext.len());
    println!("plaintext_hex = {}", to_hex(plaintext));

    // --- Encrypt: apply_keystream to a copy of the buffer. ---
    let mut buf = plaintext.to_vec();
    let mut cipher = ChaCha20::new(&key.into(), &nonce.into());
    cipher.apply_keystream(&mut buf);
    println!("ciphertext_hex = {}", to_hex(&buf));
    println!("ciphertext_differs = {}", buf.as_slice() != plaintext);

    // --- Decrypt: re-create the cipher (fresh counter) and apply again. ---
    // ChaCha20 is its own inverse, so a second apply_keystream round-trips.
    let mut cipher2 = ChaCha20::new(&key.into(), &nonce.into());
    cipher2.apply_keystream(&mut buf);
    println!("roundtrip_matches = {}", buf.as_slice() == plaintext);

    // --- Raw keystream (encrypt an all-zero buffer => the keystream itself). ---
    let mut ks = [0u8; 64];
    let mut cipher3 = ChaCha20::new(&key.into(), &nonce.into());
    cipher3.apply_keystream(&mut ks);
    println!("keystream_block0_hex = {}", to_hex(&ks));

    // --- Seeking: skip to byte offset 64 (counter block 1) and read keystream. ---
    let mut ks2 = [0u8; 16];
    let mut cipher4 = ChaCha20::new(&key.into(), &nonce.into());
    cipher4.seek(64u32);
    println!("position_after_seek = {}", cipher4.current_pos::<u32>());
    cipher4.apply_keystream(&mut ks2);
    println!("keystream_at_offset64_hex = {}", to_hex(&ks2));

    // --- Checksum the block-0 keystream as a deterministic integer aggregate. ---
    let sum: u32 = ks.iter().map(|&b| b as u32).sum();
    println!("keystream_block0_bytesum = {}", sum);

    println!("== survey_chacha20 done ==");
}
