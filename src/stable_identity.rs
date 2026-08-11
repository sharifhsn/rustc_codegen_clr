//! Versioned, deterministic identities for emitted metadata names.

use cilly::{Assembly, Type};
use sha2::{Digest, Sha256};

/// SHA-256 input builder with domain and length separation.
///
/// Every field is framed as `u64-le length || bytes`; callers therefore cannot create the same
/// digest by moving a byte across a field boundary. The versioned prefix permits deliberate
/// future migrations without silently aliasing old artifacts.
pub struct StableIdentity {
    digest: Sha256,
}

impl StableIdentity {
    #[must_use]
    pub fn new(domain: &str) -> Self {
        let mut this = Self {
            digest: Sha256::new(),
        };
        this.write_bytes(b"rustc_codegen_clr.identity.v1");
        this.write_str(domain);
        this
    }

    pub fn write_bytes(&mut self, value: &[u8]) {
        self.digest.update(
            u64::try_from(value.len())
                .expect("identity field length exceeds u64")
                .to_le_bytes(),
        );
        self.digest.update(value);
    }

    pub fn write_str(&mut self, value: &str) {
        self.write_bytes(value.as_bytes());
    }

    pub fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    #[must_use]
    pub fn finish_hex(self) -> String {
        let bytes = self.digest.finalize();
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use std::fmt::Write as _;
            write!(out, "{byte:02x}").expect("writing into String cannot fail");
        }
        out
    }
}

#[must_use]
pub fn digest_fields<'a>(domain: &str, fields: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut identity = StableIdentity::new(domain);
    for field in fields {
        identity.write_bytes(field);
    }
    identity.finish_hex()
}

/// Writes a structural CIL type identity. Arena indices are deliberately never encoded.
pub fn write_type(identity: &mut StableIdentity, ty: Type, asm: &Assembly) {
    match ty {
        Type::Ptr(inner) => {
            identity.write_str("ptr");
            write_type(identity, asm[inner], asm);
        }
        Type::Ref(inner) => {
            identity.write_str("ref");
            write_type(identity, asm[inner], asm);
        }
        Type::Int(int) => {
            identity.write_str("int");
            identity.write_str(int.name());
        }
        Type::ClassRef(class) => {
            identity.write_str("class");
            let class = &asm[class];
            identity.write_str(class.asm().map_or("", |name| &asm[name]));
            identity.write_str(&asm[class.name()]);
            identity.write_u64(u64::from(class.is_valuetype()));
            identity.write_u64(class.generics().len() as u64);
            for generic in class.generics() {
                write_type(identity, *generic, asm);
            }
        }
        Type::Float(float) => {
            identity.write_str("float");
            identity.write_str(float.name());
        }
        Type::PlatformString => identity.write_str("platform-string"),
        Type::PlatformChar => identity.write_str("platform-char"),
        Type::PlatformGeneric(index, kind) => {
            identity.write_str("platform-generic");
            identity.write_u64(u64::from(index));
            identity.write_str(&format!("{kind:?}"));
        }
        Type::PlatformObject => identity.write_str("platform-object"),
        Type::Bool => identity.write_str("bool"),
        Type::Void => identity.write_str("void"),
        Type::PlatformArray { elem, dims } => {
            identity.write_str("platform-array");
            identity.write_u64(u64::from(dims.get()));
            write_type(identity, asm[elem], asm);
        }
        Type::FnPtr(sig) => {
            identity.write_str("fnptr");
            let sig = &asm[sig];
            identity.write_u64(sig.inputs().len() as u64);
            for input in sig.inputs() {
                write_type(identity, *input, asm);
            }
            write_type(identity, *sig.output(), asm);
        }
        Type::SIMDVector(vector) => {
            identity.write_str("simd");
            identity.write_str(&vector.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StableIdentity, digest_fields};

    #[test]
    fn digest_is_repeatable_and_field_boundaries_are_unambiguous() {
        let first = digest_fields("test", [b"ab".as_slice(), b"c".as_slice()]);
        let repeated = digest_fields("test", [b"ab".as_slice(), b"c".as_slice()]);
        let regrouped = digest_fields("test", [b"a".as_slice(), b"bc".as_slice()]);
        assert_eq!(first, repeated);
        assert_ne!(first, regrouped);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn digest_domains_do_not_alias() {
        let mut symbol = StableIdentity::new("symbol");
        symbol.write_str("same");
        let mut allocation = StableIdentity::new("allocation");
        allocation.write_str("same");
        assert_ne!(symbol.finish_hex(), allocation.finish_hex());
    }
}
