//! The single Rust layout contract representable by the current CIL/PE runtime.
//!
//! Direct PE emission, native-width CIL integers, runtime helpers, and serialized allocation
//! decoding currently share one contract: 64-bit pointers and little-endian bytes. `TargetLayout`
//! is therefore a zero-sized proof that the rustc target was checked against that contract at
//! `codegen_crate` entry, rather than a bag of facts that downstream code might accidentally treat
//! as broader support.

use rustc_abi::{Endian, TargetDataLayout};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetLayout(());

impl TargetLayout {
    pub const SUPPORTED: Self = Self(());
    pub const POINTER_BYTES: usize = 8;
    pub const POINTER_BITS: u32 = 64;

    pub fn from_data_layout(layout: &TargetDataLayout) -> Result<Self, String> {
        Self::from_facts(layout.pointer_size().bytes(), layout.endian)
    }

    pub fn from_facts(pointer_bytes: u64, endian: Endian) -> Result<Self, String> {
        if pointer_bytes == Self::POINTER_BYTES as u64 && endian == Endian::Little {
            return Ok(Self::SUPPORTED);
        }
        let endian = match endian {
            Endian::Little => "little-endian",
            Endian::Big => "big-endian",
        };
        Err(format!(
            "the current .NET PE/runtime contract requires a 64-bit little-endian Rust target; \
             this target uses {}-bit {endian} data",
            pointer_bytes.saturating_mul(8)
        ))
    }

    #[must_use]
    pub const fn pointer_bytes(self) -> usize {
        Self::POINTER_BYTES
    }

    #[must_use]
    pub const fn pointer_bits(self) -> u32 {
        Self::POINTER_BITS
    }

    /// Decode target-layout bytes. Construction of this proof guarantees little-endian order.
    #[must_use]
    pub fn decode_uint(self, bytes: &[u8]) -> u128 {
        assert!(bytes.len() <= 16, "target scalar exceeds 128 bits");
        bytes
            .iter()
            .rev()
            .fold(0_u128, |value, byte| (value << 8) | u128::from(*byte))
    }

    /// Decode exactly one target pointer-sized inline relocation addend.
    #[must_use]
    pub fn decode_pointer(self, bytes: &[u8]) -> u64 {
        assert_eq!(
            bytes.len(),
            Self::POINTER_BYTES,
            "pointer addend byte width disagrees with TargetLayout"
        );
        u64::try_from(self.decode_uint(bytes)).expect("64-bit target pointer exceeds u64")
    }

    #[must_use]
    pub const fn signed_pointer_min(self) -> i128 {
        i64::MIN as i128
    }

    #[must_use]
    pub const fn signed_pointer_max(self) -> i128 {
        i64::MAX as i128
    }

    #[must_use]
    pub const fn unsigned_pointer_max(self) -> u128 {
        u64::MAX as u128
    }
}

#[cfg(test)]
mod tests {
    use super::TargetLayout;
    use rustc_abi::Endian;

    #[test]
    fn rejects_32_bit_and_big_endian_target_facts() {
        let narrow = TargetLayout::from_facts(4, Endian::Little).unwrap_err();
        assert!(narrow.contains("requires a 64-bit little-endian Rust target"));
        assert!(narrow.contains("32-bit little-endian"));

        let big = TargetLayout::from_facts(8, Endian::Big).unwrap_err();
        assert!(big.contains("requires a 64-bit little-endian Rust target"));
        assert!(big.contains("64-bit big-endian"));
    }

    #[test]
    fn supported_proof_has_constant_pointer_and_byte_contract() {
        let layout = TargetLayout::from_facts(8, Endian::Little).unwrap();
        assert_eq!(layout.pointer_bytes(), 8);
        assert_eq!(layout.pointer_bits(), 64);
        assert_eq!(layout.decode_uint(&[0x78, 0x56, 0x34, 0x12]), 0x1234_5678);
        assert_eq!(
            layout.decode_pointer(&[1, 2, 3, 4, 5, 6, 7, 8]),
            0x0807_0605_0403_0201
        );
        assert_eq!(layout.signed_pointer_min(), i128::from(i64::MIN));
        assert_eq!(layout.signed_pointer_max(), i128::from(i64::MAX));
        assert_eq!(layout.unsigned_pointer_max(), u128::from(u64::MAX));
    }
}
