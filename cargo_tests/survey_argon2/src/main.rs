use argon2::{Algorithm, Argon2, Params, Version};

// Render bytes as lowercase hex without allocating intermediate Strings per byte
// and without any panic-prone indexing.
fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn run_one(label: &str, algo: Algorithm, m_cost: u32, t_cost: u32, p_cost: u32) {
    // Fixed password + salt => fully deterministic KDF output.
    let password: &[u8] = b"survey-fixed-password";
    let salt: &[u8] = b"survey-fixed-salt"; // 17 bytes, >= 8 required by Argon2.

    // Params::new(m_cost, t_cost, p_cost, output_len). output_len = 32 bytes.
    let params = match Params::new(m_cost, t_cost, p_cost, Some(32)) {
        Ok(p) => p,
        Err(_) => {
            println!("{}_params_error = true", label);
            return;
        }
    };

    let argon2 = Argon2::new(algo, Version::V0x13, params);

    let mut out = [0u8; 32];
    match argon2.hash_password_into(password, salt, &mut out) {
        Ok(()) => {
            println!("{}_hash_hex = {}", label, to_hex(&out));
            // Derive a deterministic integer checksum (XOR fold) as a second signal.
            let mut fold: u8 = 0;
            for &b in out.iter() {
                fold ^= b;
            }
            println!("{}_xor_fold = {}", label, fold);
        }
        Err(_) => {
            println!("{}_hash_error = true", label);
        }
    }
}

fn main() {
    // Exercise the three Argon2 variants. Use modest memory costs so the run is
    // fast and the memory-hard core is still meaningfully driven, while keeping
    // everything deterministic (fixed password/salt/params).
    //
    // m_cost is in KiB. 256 KiB keeps it light but non-trivial.
    run_one("argon2d", Algorithm::Argon2d, 256, 2, 1);
    run_one("argon2i", Algorithm::Argon2i, 256, 2, 1);
    run_one("argon2id", Algorithm::Argon2id, 256, 2, 1);

    // A second-set with a different time cost to exercise the multi-pass path.
    run_one("argon2id_t3", Algorithm::Argon2id, 512, 3, 1);

    // Single-pass, single-lane minimal config (still memory-hard).
    run_one("argon2id_min", Algorithm::Argon2id, 8, 1, 1);

    println!("== survey_argon2 done ==");
}
