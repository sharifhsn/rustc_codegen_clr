use aes::Aes128;
use cipher::generic_array::GenericArray;
use cipher::{BlockDecrypt, BlockEncrypt, KeyInit};

// Render bytes as lowercase hex without allocating a Vec or risking a panic.
fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(TABLE[(b >> 4) as usize] as char);
        out.push(TABLE[(b & 0x0f) as usize] as char);
    }
    out
}

fn main() {
    // Fixed 16-byte key and a fixed 16-byte plaintext block. Both deterministic.
    // This key/plaintext pair is the FIPS-197 AES-128 test vector, so the
    // ciphertext is well-known: 69c4e0d86a7b0430d8cdb78070b4c55a.
    let key_bytes: [u8; 16] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f,
    ];
    let plaintext: [u8; 16] = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff,
    ];

    println!("key_hex = {}", hex(&key_bytes));
    println!("plaintext_hex = {}", hex(&plaintext));

    let key = GenericArray::from_slice(&key_bytes);
    let cipher = Aes128::new(key);

    // Encrypt a single block in place.
    let mut block = GenericArray::clone_from_slice(&plaintext);
    cipher.encrypt_block(&mut block);
    let ciphertext = block;
    println!("ciphertext_hex = {}", hex(ciphertext.as_slice()));

    // Compare against the known FIPS-197 expected ciphertext (derive a bool).
    let expected_ct: [u8; 16] = [
        0x69, 0xc4, 0xe0, 0xd8, 0x6a, 0x7b, 0x04, 0x30, 0xd8, 0xcd, 0xb7, 0x80, 0x70, 0xb4, 0xc5,
        0x5a,
    ];
    let ct_matches_fips = ciphertext.as_slice() == expected_ct.as_slice();
    println!("ciphertext_matches_fips197 = {}", ct_matches_fips);

    // Decrypt back and verify the round-trip restored the original plaintext.
    let mut dec = ciphertext;
    cipher.decrypt_block(&mut dec);
    println!("decrypted_hex = {}", hex(dec.as_slice()));
    let roundtrip_ok = dec.as_slice() == plaintext.as_slice();
    println!("decrypt_roundtrip_ok = {}", roundtrip_ok);

    // Exercise a second, all-zero block so the survey covers more of the surface
    // with another deterministic vector.
    let zero_pt = GenericArray::clone_from_slice(&[0u8; 16]);
    let mut zb = zero_pt;
    cipher.encrypt_block(&mut zb);
    println!("zero_block_ciphertext_hex = {}", hex(zb.as_slice()));
    cipher.decrypt_block(&mut zb);
    let zero_roundtrip_ok = zb.as_slice() == [0u8; 16].as_slice();
    println!("zero_block_roundtrip_ok = {}", zero_roundtrip_ok);

    println!("== survey_aes done ==");
}
