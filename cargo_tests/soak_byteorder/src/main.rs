//! H2 real-crate SOAK: byteorder (1.x) endian read/write round-trip on the dotnet PAL.
//! Exercises ByteOrder::read_u32/write_u32 (Big- and LittleEndian) into byte buffers,
//! plus ReadBytesExt over a &[u8] cursor. Panic-safe: valid inputs only, all fallible
//! (Read) paths handled via match, no unwrap/expect/indexing-that-can-fail.
//! SUCCESS = "== soak_byteorder done ==".

use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};

fn main() {
    let value: u32 = 0x1234_5678;

    // --- BigEndian write_u32 into a fixed buffer, then read it back ---
    let mut be_buf = [0u8; 4];
    BigEndian::write_u32(&mut be_buf, value);
    println!(
        "be_bytes = {:02x} {:02x} {:02x} {:02x}",
        be_buf[0], be_buf[1], be_buf[2], be_buf[3]
    );
    let be_read = BigEndian::read_u32(&be_buf);
    println!("be_roundtrip = {}", be_read == value);

    // --- LittleEndian write_u32 into a fixed buffer, then read it back ---
    let mut le_buf = [0u8; 4];
    LittleEndian::write_u32(&mut le_buf, value);
    println!(
        "le_bytes = {:02x} {:02x} {:02x} {:02x}",
        le_buf[0], le_buf[1], le_buf[2], le_buf[3]
    );
    let le_read = LittleEndian::read_u32(&le_buf);
    println!("le_roundtrip = {}", le_read == value);

    // BE and LE encodings of the same value should be byte-reversed.
    println!(
        "be_le_reversed = {}",
        be_buf[0] == le_buf[3] && be_buf[1] == le_buf[2]
    );

    // --- ReadBytesExt over a &[u8] cursor (Read trait; returns io::Result) ---
    let src = be_buf; // big-endian bytes
    let mut cursor: &[u8] = &src;
    match cursor.read_u32::<BigEndian>() {
        Ok(v) => println!("cursor_read_be = {} (matches = {})", v, v == value),
        Err(_) => println!("cursor_read_be_error = true"),
    }

    // --- WriteBytesExt into a Vec<u8> (also io::Result) ---
    let mut out: Vec<u8> = Vec::new();
    match out.write_u32::<LittleEndian>(value) {
        Ok(()) => println!("vec_write_len = {}", out.len()),
        Err(_) => println!("vec_write_error = true"),
    }
    if out.len() == 4 {
        println!("vec_write_matches_le = {}", out.as_slice() == &le_buf[..]);
    }

    // --- u16 / u64 paths for breadth ---
    let mut buf16 = [0u8; 2];
    BigEndian::write_u16(&mut buf16, 0xBEEF);
    println!("u16_roundtrip = {}", BigEndian::read_u16(&buf16) == 0xBEEF);

    let mut buf64 = [0u8; 8];
    LittleEndian::write_u64(&mut buf64, 0x0102_0304_0506_0708);
    println!(
        "u64_roundtrip = {}",
        LittleEndian::read_u64(&buf64) == 0x0102_0304_0506_0708
    );

    println!("== soak_byteorder done ==");
}
