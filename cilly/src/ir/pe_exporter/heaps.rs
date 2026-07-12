//! The four ECMA-335 metadata heaps (§II.24.2.2–II.24.2.4): `#Strings`, `#Blob`, `#GUID`, `#US`.
//!
//! Each heap interns its items (byte-identical duplicates share one entry) and hands back the
//! index the metadata tables store. Note the two indexing conventions the spec mixes:
//! * `#Strings` / `#Blob` / `#US` indices are **byte offsets** into the heap.
//! * `#GUID` indices are **1-based ordinals** (index 1 = first 16-byte GUID).
//!
//! Whether a table column stores a 2- or 4-byte heap index is a *container-level* decision (the
//! `HeapSizes` bits of the `#~` stream header, §II.24.2.6) made after all heaps are final — the
//! heaps themselves always deal in `u32` and expose their final byte size for that computation.

use std::collections::HashMap;

/// Writes an ECMA-335 *compressed unsigned integer* (§II.23.2) — the length prefix used by the
/// `#Blob`/`#US` heaps and throughout signature blobs.
///
/// Encodable range is `0..=0x1FFF_FFFF`; the metadata format has no representation for larger
/// values, so passing one is a caller bug (a blob that large is unconstructible anyway).
pub fn write_compressed_u32(out: &mut Vec<u8>, value: u32) {
    match value {
        0..=0x7F => out.push(value as u8),
        0x80..=0x3FFF => out.extend_from_slice(&[(0x80 | (value >> 8)) as u8, value as u8]),
        0x4000..=0x1FFF_FFFF => out.extend_from_slice(&[
            (0xC0 | (value >> 24)) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ]),
        _ => panic!("value {value:#x} exceeds the ECMA-335 compressed-u32 range"),
    }
}

/// The `#Strings` heap: null-terminated UTF-8 names (§II.24.2.3). Offset 0 is always the empty
/// string (the heap begins with a single `\0`).
pub struct StringsHeap {
    data: Vec<u8>,
    interned: HashMap<Box<str>, u32>,
}

impl Default for StringsHeap {
    fn default() -> Self {
        Self {
            data: vec![0],
            interned: HashMap::new(),
        }
    }
}

impl StringsHeap {
    /// Interns `s`, returning its byte offset. The empty string is always offset 0.
    pub fn intern(&mut self, s: &str) -> u32 {
        if s.is_empty() {
            return 0;
        }
        assert!(
            !s.bytes().any(|b| b == 0),
            "metadata name contains an interior NUL: {s:?}"
        );
        if let Some(&off) = self.interned.get(s) {
            return off;
        }
        let off = u32::try_from(self.data.len()).expect("#Strings heap exceeded 4 GiB");
        self.data.extend_from_slice(s.as_bytes());
        self.data.push(0);
        self.interned.insert(s.into(), off);
        off
    }

    /// Final heap bytes (unpadded; the container pads streams to 4-byte alignment).
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// The `#Blob` heap: length-prefixed byte blobs (§II.24.2.4). Offset 0 is always the empty blob
/// (the heap begins with a single `0x00` length byte).
pub struct BlobHeap {
    data: Vec<u8>,
    interned: HashMap<Box<[u8]>, u32>,
}

impl Default for BlobHeap {
    fn default() -> Self {
        Self {
            data: vec![0],
            interned: HashMap::new(),
        }
    }
}

impl BlobHeap {
    /// Interns `blob`, returning its byte offset (of the length prefix). The empty blob is
    /// always offset 0.
    pub fn intern(&mut self, blob: &[u8]) -> u32 {
        if blob.is_empty() {
            return 0;
        }
        if let Some(&off) = self.interned.get(blob) {
            return off;
        }
        let off = u32::try_from(self.data.len()).expect("#Blob heap exceeded 4 GiB");
        write_compressed_u32(
            &mut self.data,
            u32::try_from(blob.len()).expect("blob exceeds u32"),
        );
        self.data.extend_from_slice(blob);
        self.interned.insert(blob.into(), off);
        off
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// The `#GUID` heap: raw 16-byte GUIDs, indexed by **1-based ordinal** (§II.24.2.5).
#[derive(Default)]
pub struct GuidHeap {
    data: Vec<u8>,
}

impl GuidHeap {
    /// Appends a GUID and returns its 1-based index. GUIDs are not deduped — in practice a module
    /// has exactly one (the MVID), so interning machinery would be dead weight.
    pub fn push(&mut self, guid: [u8; 16]) -> u32 {
        self.data.extend_from_slice(&guid);
        u32::try_from(self.data.len() / 16).expect("#GUID heap exceeded u32 ordinals")
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// The `#US` (user-string) heap backing `ldstr` (§II.24.2.4): length-prefixed UTF-16LE strings,
/// each followed by the spec's one-byte "contains special characters" flag. Offset 0 is the empty
/// entry.
pub struct UserStringHeap {
    data: Vec<u8>,
    interned: HashMap<Box<str>, u32>,
}

impl Default for UserStringHeap {
    fn default() -> Self {
        Self {
            data: vec![0],
            interned: HashMap::new(),
        }
    }
}

impl UserStringHeap {
    /// Interns `s`, returning the byte offset an `ldstr` token's low 24 bits carry.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&off) = self.interned.get(s) {
            return off;
        }
        let utf16: Vec<u16> = s.encode_utf16().collect();
        // §II.24.2.4: the trailing byte is 1 iff any UTF-16 unit needs "special handling"
        // (>= 0x100, or one of the control/format code points listed by the spec).
        let special = utf16
            .iter()
            .any(|&u| u >= 0x100 || matches!(u, 0x01..=0x08 | 0x0E..=0x1F | 0x27 | 0x2D | 0x7F));
        let byte_len = u32::try_from(utf16.len() * 2 + 1).expect("user string exceeds u32");
        let off = u32::try_from(self.data.len()).expect("#US heap exceeded 4 GiB");
        write_compressed_u32(&mut self.data, byte_len);
        for unit in utf16 {
            self.data.extend_from_slice(&unit.to_le_bytes());
        }
        self.data.push(u8::from(special));
        self.interned.insert(s.into(), off);
        off
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compressed(value: u32) -> Vec<u8> {
        let mut out = Vec::new();
        write_compressed_u32(&mut out, value);
        out
    }

    #[test]
    fn compressed_u32_spec_examples() {
        // The worked examples from ECMA-335 §II.23.2.
        assert_eq!(compressed(0x03), [0x03]);
        assert_eq!(compressed(0x7F), [0x7F]);
        assert_eq!(compressed(0x80), [0x80, 0x80]);
        assert_eq!(compressed(0x2E57), [0xAE, 0x57]);
        assert_eq!(compressed(0x3FFF), [0xBF, 0xFF]);
        assert_eq!(compressed(0x4000), [0xC0, 0x00, 0x40, 0x00]);
        assert_eq!(compressed(0x1FFF_FFFF), [0xDF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    #[should_panic(expected = "compressed-u32 range")]
    fn compressed_u32_overflow_panics() {
        compressed(0x2000_0000);
    }

    #[test]
    fn strings_heap_interning() {
        let mut heap = StringsHeap::default();
        assert_eq!(heap.intern(""), 0);
        let a = heap.intern("Main");
        let b = heap.intern("Other");
        assert_eq!(heap.intern("Main"), a, "duplicate must reuse the offset");
        assert_ne!(a, b);
        // layout: \0 M a i n \0 O t h e r \0
        assert_eq!(heap.as_bytes(), b"\0Main\0Other\0");
        assert_eq!(a, 1);
        assert_eq!(b, 6);
    }

    #[test]
    fn blob_heap_interning_and_prefix() {
        let mut heap = BlobHeap::default();
        assert_eq!(heap.intern(&[]), 0);
        let a = heap.intern(&[1, 2, 3]);
        assert_eq!(heap.intern(&[1, 2, 3]), a);
        assert_eq!(heap.as_bytes(), &[0, 3, 1, 2, 3]);
        // A blob long enough to need a 2-byte length prefix.
        let long = vec![0xAB; 0x80];
        let b = heap.intern(&long);
        let bytes = heap.as_bytes();
        assert_eq!(&bytes[b as usize..b as usize + 2], &[0x80, 0x80]);
    }

    #[test]
    fn guid_heap_is_one_based() {
        let mut heap = GuidHeap::default();
        assert_eq!(heap.push([0xAA; 16]), 1);
        assert_eq!(heap.push([0xBB; 16]), 2);
        assert_eq!(heap.as_bytes().len(), 32);
    }

    #[test]
    fn user_string_heap_encoding() {
        let mut heap = UserStringHeap::default();
        let a = heap.intern("Hi");
        // 2 UTF-16 units * 2 + 1 flag byte = 5; ASCII-only => flag 0.
        assert_eq!(
            &heap.as_bytes()[a as usize..],
            &[5, b'H', 0, b'i', 0, 0][..]
        );
        // Non-ASCII flips the special flag.
        let b = heap.intern("π");
        let bytes = &heap.as_bytes()[b as usize..];
        assert_eq!(bytes[0], 3);
        assert_eq!(*bytes.last().unwrap(), 1);
        // The apostrophe is on the spec's special list despite being ASCII.
        let c = heap.intern("'");
        assert_eq!(*heap.as_bytes()[c as usize..].last().unwrap(), 1);
        assert_eq!(heap.intern("Hi"), a);
    }
}
