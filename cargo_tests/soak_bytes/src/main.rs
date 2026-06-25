//! H2 real-crate SOAK: bytes (the tokio bytes crate) on the dotnet PAL.
//! Exercises BytesMut growable buffer, put_u32 (big-endian byte writes), put_slice,
//! freeze() -> Bytes (Arc-backed shared buffer), get_u32 (BE reads consuming the cursor),
//! and split_to (zero-copy slice/refcount). This pulls in Vec growth, Arc atomics, and
//! pointer/length bookkeeping. Panic-safe: all reads are bounded by the bytes we wrote,
//! no .unwrap()/indexing that can fail; we check remaining() before every get.
//! SUCCESS = "== soak_bytes done =="
use bytes::{Buf, BufMut, Bytes, BytesMut};

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('?'));
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('?'));
    }
    s
}

fn main() {
    println!("== soak_bytes start ==");

    // 1: build a BytesMut by writing a u32 (BE) then a byte slice.
    let mut buf = BytesMut::with_capacity(64);
    buf.put_u32(0xDEADBEEF);
    buf.put_slice(b"hello");
    println!("1  len after writes   = {}", buf.len());
    println!("1  bytes              = {}", to_hex(&buf));
    println!("1  expect             = deadbeef68656c6c6f");

    // 2: freeze -> Bytes (shared, Arc-backed).
    let frozen: Bytes = buf.freeze();
    println!("2  frozen len         = {}", frozen.len());

    // 3: read the u32 back (BE) via a Buf cursor.
    let mut cur = frozen.clone();
    if cur.remaining() >= 4 {
        let v = cur.get_u32();
        println!("3  get_u32            = {:08x}", v);
        println!("3  matches 0xDEADBEEF = {}", v == 0xDEADBEEF);
    } else {
        println!("3  not enough bytes");
    }
    println!("3  remaining after u32 = {}", cur.remaining());

    // 4: split_to on the original frozen Bytes (zero-copy refcount split).
    let mut tail = frozen.clone();
    if tail.len() >= 4 {
        let head = tail.split_to(4);
        println!("4  head               = {}", to_hex(&head));
        println!("4  tail               = {}", to_hex(&tail));
        // tail should be the ascii "hello"
        let s = core::str::from_utf8(&tail).unwrap_or("<bad utf8>");
        println!("4  tail as str        = {}", s);
        println!("4  tail == hello      = {}", s == "hello");
    } else {
        println!("4  too short to split");
    }

    // 5: copy_to_bytes consuming the cursor remainder ("hello").
    if cur.remaining() > 0 {
        let rest = cur.copy_to_bytes(cur.remaining());
        let s = core::str::from_utf8(&rest).unwrap_or("<bad utf8>");
        println!("5  cursor remainder   = {}", s);
    } else {
        println!("5  cursor empty");
    }

    println!("== soak_bytes done ==");
}
