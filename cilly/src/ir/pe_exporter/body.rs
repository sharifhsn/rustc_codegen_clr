//! Method-body byte assembly (§II.25.4): tiny/fat headers, opcode-byte emission for the ~80
//! instruction forms the backend produces, branch-target layout, `.maxstack` computation, and
//! fat exception-handling clause sections (§II.25.4.6).
//!
//! Semantic oracle: `il_exporter::export_method_imp`/`export_node`/`export_root`
//! (`cilly/src/ir/il_exporter/mod.rs`) — this module must assemble bytes meaning exactly what
//! that textual IL means for the same `MethodDef`. Mirror its block/handler iteration shape
//! exactly (see that function for the `.try { … } catch [System.Runtime]System.Object { … }`
//! nesting this backend emits — always a single flat catch over `System.Object`, plus the nested
//! `TerminateRegion`/`FailFast` shape it documents) rather than re-deriving EH structure from
//! scratch.
//!
//! Design choices fixed for Phase 1a (implementers should not need to revisit these):
//! * **Always emit a fat header** (§II.25.4.3), never tiny (§II.25.4.2). `il_exporter` computes
//!   `.maxstack` per-method and this backend routinely emits `.locals`/EH clauses that the tiny
//!   header format cannot represent (max stack > 8, code > 64 bytes, or any locals/EH) — the plan
//!   doc calls this out explicitly ("fat EH sections (always fat = always valid)"). A uniform fat
//!   header removes a whole size-dependent branch from the writer for a few constant bytes of
//!   overhead per method.
//! * **Long-form branches only** for Phase 1a (`br`/`brtrue`/`beq`/… as 5-byte `i4`-offset forms,
//!   never the 2-byte short forms) — matches the plan doc's "short-form compaction optional
//!   later". This makes branch layout a single forward pass (every instruction has a fixed size
//!   before target offsets are known), at the cost of slightly larger bodies than ilasm's
//!   optimizing assembler produces; a later pass can compact once round-trip correctness is
//!   proven.

use super::tables::{Token, TokenSink};
use crate::ir::{Assembly, MethodDefIdx};

/// The assembled bytes of one method body, ready for the `pe` layout pass to place at a
/// 4-byte-aligned RVA within `.text` (§II.25.4.1: fat-format bodies must start 4-byte aligned;
/// the caller is responsible for padding between consecutive bodies, since only it knows the
/// running offset).
pub struct AssembledBody {
    /// Fat header (§II.25.4.3, 12 bytes: flags/size, `MaxStack`, `CodeSize`, `LocalVarSigTok`) +
    /// IL instruction bytes +, if the method has any handler, a fat EH section (§II.25.4.6,
    /// `Flags = CorILMethod_Sect_EHTable | CorILMethod_Sect_FatFormat`) appended after the code
    /// and 4-byte aligned relative to the body start.
    pub bytes: Vec<u8>,
}

/// Assembles the complete body of `method` (header + IL + EH) into RVA-ready bytes.
///
/// Every metadata reference an instruction operand needs (method/field/type tokens, `ldstr`
/// user-string tokens, `calli`/`.locals` `StandAloneSig` tokens) is resolved through `tokens`
/// rather than touched directly, so this module has zero dependency on `tables.rs`'s row-storage
/// representation — see [`TokenSink`]. `asm` is `&mut` because signature encoding
/// (`sig::encode_type` et al., reached transitively through the token queries) can intern new
/// `Type`/`ClassRef` values (e.g. lowering `i128` to its BCL valuetype `ClassRef` on first use),
/// exactly as `sig::encode_type` already requires.
///
/// `MethodImpl::Extern`/`AliasFor`/`Missing` bodies (see `il_exporter::export_method_imp`'s
/// match) produce **no** body bytes at all (`Extern` — a `pinvokeimpl` method has RVA 0 and no
/// code; `AliasFor` never reaches export, `resolved_implementation` always follows it first;
/// `Missing` mirrors the `il_exporter` fallback of a thrown placeholder exception, which DOES
/// need a real body). Implementers thread that distinction however is cleanest — e.g. returning
/// `None` for `Extern` — but the signature below only commits to the `MethodBody`-shaped case
/// being representable, since that's what every real caller drives today.
pub fn assemble_method(
    asm: &mut Assembly,
    method: MethodDefIdx,
    tokens: &mut dyn TokenSink,
) -> AssembledBody {
    let _ = (asm, method, tokens);
    todo!(
        "walk method.resolved_implementation(asm)'s blocks/roots (mirroring \
         il_exporter::export_method_imp) and emit fat-header + IL + EH bytes"
    )
}

/// Computes `.maxstack` (§II.25.4.3 `MaxStack` field) for one method body, mirroring
/// `il_exporter::emit_one_method`'s per-root `CILIter::new(root, asm).count() + 10` upper-bound
/// heuristic (a deliberately loose over-approximation, not a tight stack-depth analysis — ilasm
/// accepts a `.maxstack` larger than actually needed, so exactness is not a correctness
/// requirement here, only "large enough").
#[must_use]
pub fn compute_maxstack(asm: &Assembly, method: MethodDefIdx) -> u32 {
    let _ = (asm, method);
    todo!("mirror il_exporter's per-root CILIter::new(root, asm).count() + 10 max, over all blocks")
}

/// A resolved token for one instruction operand, as produced while walking a method's roots —
/// the shape `assemble_method`'s inner instruction-encoding step consumes. Kept separate from
/// [`TokenSink`]'s query methods so the byte-emission code can be written as "resolve token, then
/// append `opcode_byte(s) ++ token.0.to_le_bytes()`" uniformly across every operand-carrying
/// instruction, matching how `il_exporter::export_node`/`export_root` uniformly interpolate a
/// rendered operand into each `writeln!`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedOperand(pub Token);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembled_body_is_a_plain_byte_buffer() {
        // Smoke-checks the public shape implementers build against: a bag of bytes the `pe`
        // layout pass can place and 4-align without knowing anything about IL semantics.
        let body = AssembledBody { bytes: vec![0u8; 12] };
        assert_eq!(body.bytes.len(), 12);
    }

    #[test]
    fn resolved_operand_wraps_a_token() {
        let op = ResolvedOperand(Token::new(Token::TABLE_METHOD_DEF, 1));
        assert_eq!(op.0.table(), Token::TABLE_METHOD_DEF);
    }
}
