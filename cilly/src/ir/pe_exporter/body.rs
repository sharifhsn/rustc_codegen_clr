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
//!
//! # Two-pass branch layout
//!
//! Every instruction is appended to a flat `Vec<u8>` in the same order `export_method_imp`/
//! `export_root`/`export_node` would visit the method's blocks/handlers/roots/nodes (block N's
//! own roots, then — inline, immediately after, exactly like the `.try{…}catch{…}` nesting in the
//! oracle — its handler sub-blocks, then block N+1, …). Each *label* (`bb{id}`, `h{id}_{sub}`,
//! `jp{id}_{sub}`, `tr_done_{n}`) records the byte offset (within the method body's code stream,
//! i.e. NOT counting the fat header) at which it is defined; each branch instruction records the
//! byte offset of the *start of its i4 operand* plus which label it targets. After the whole body
//! is emitted, every recorded branch operand is patched with `target_offset - (operand_offset +
//! 4)` (§II.25.4.4: the offset is relative to the instruction immediately *following* the branch,
//! i.e. measured from the end of the 5-byte long-form instruction).

use super::pdb::SequencePoint;
use super::tables::{Token, TokenSink};
use crate::ir::basic_block::BasicBlock;
use crate::ir::cilnode::ExtendKind;
use crate::ir::cilnode::UnOp;
use crate::ir::cilroot::{BranchCond, CmpKind};
use crate::ir::method::{LocalDef, MethodImpl};
use crate::ir::{
    Assembly, CILNode, CILRoot, ClassRef, Const, Float, Int, Interned, MethodDefIdx, Type,
};
use std::collections::HashMap;

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
    /// Every `CILRoot::SourceFileInfo` root visited while linearizing this body (same order
    /// `il_exporter`'s `.line` directives appear in, per this module's parity-bar doc), each
    /// tagged with the IL byte offset (within [`bytes`](Self::bytes)'s code stream, i.e. counting
    /// past the fat header — `pdb::PdbBuilder`/`export.rs`'s wiring must subtract the header
    /// length if it needs an offset relative to `code` alone) it was recorded at. Empty for a
    /// method with no source spans (`MethodImpl::Extern`/`Missing`, or a body whose MIR carried no
    /// spans) — [`pdb::PdbBuilder::add_method`] treats that as "no debug info", matching the spec's
    /// empty-blob convention for such rows.
    pub sequence_points: Vec<SequencePoint>,
    /// The `StandAloneSig` token of this method's `.locals`, if any — mirrors the fat header's own
    /// `LocalVarSigTok` field (`finish_body`'s `locals_tok` parameter) so `pdb.rs`'s
    /// `MethodSequencePoints::local_signature` never needs to re-derive it.
    pub locals_signature: Option<Token>,
    /// This method's locals, in `LocalVarSig` declaration order (i.e. index == the local's slot
    /// index), resolved to owned `Option<String>` names (`None` for an unnamed/compiler-generated
    /// temporary) — the source `pdb.rs`'s `LocalScope`/`LocalVariable` (0x32/0x33) row emission
    /// reads from. Resolved to an owned `String` HERE (not left as a raw `Interned<IString>`)
    /// for the same reason [`SequencePoint::document_path`] is already an owned `String`: this
    /// struct must not carry a handle that assumes a specific `Assembly` outlives it, and `asm`
    /// is in scope right here in `assemble_method_body` (via `emitter.asm`) but not in `pdb.rs`,
    /// which builds its own independent heaps (see that module's doc). Empty for a method with no
    /// locals at all (`Extern`/`Missing`, or a body whose MIR declared zero locals).
    pub locals: Vec<Option<String>>,
    /// The method body's pure IL code length in bytes — i.e. `bytes.len()` MINUS the fat header
    /// (and minus any trailing EH section) — matching the fat header's own `CodeSize` field
    /// (§II.25.4.3) and [`SequencePoint::il_offset`]'s "offset from start of code, header not
    /// counted" convention. `pdb.rs`'s `LocalScope.Length` (0x32) uses this to cover the whole
    /// method body as one flat scope. `0` for a body with no code (`Extern`).
    pub code_len: u32,
}

// §II.25.4.3 fat-header flag bits (the low nibble of the Flags/Size u16).
const COR_IL_METHOD_FAT_FORMAT: u16 = 0x3;
const COR_IL_METHOD_INIT_LOCALS: u16 = 0x10;
const COR_IL_METHOD_MORE_SECTS: u16 = 0x8;
/// `Size` field of the fat header: header length in 4-byte words — always 3 (12 bytes).
const FAT_HEADER_SIZE_WORDS: u16 = 3;

// §II.25.4.6 fat EH-section flags.
const COR_IL_METHOD_SECT_EHTABLE: u8 = 0x1;
const COR_IL_METHOD_SECT_FAT_FORMAT: u8 = 0x40;

/// A label identity used while linearizing a method's blocks/handlers into one instruction
/// stream, matching `branch_cond_to_name`'s four label shapes (`cilly/src/ir/mod.rs`), plus a
/// `Marker` kind for anonymous EH-region boundary points that have no named-label counterpart in
/// the textual exporter (it uses raw code position implicitly via `.try{}`/`}` nesting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Label {
    /// `bb{id}` — a plain basic-block entry.
    Block(u32),
    /// `h{block}_{sub}` — a `leave` target inside a handler, back to block `sub`.
    HandlerLeave(u32, u32),
    /// `jp{block}_{sub}` — a `leave` target inside a protected (has-handler) block, to block `sub`.
    ProtectedLeave(u32, u32),
    /// `tr_done_{n}` — the `TerminateRegion` inner-protected-region `leave` target.
    TerminateDone(u64),
    /// An anonymous marker at the current write position (try/handler region boundaries).
    Marker(u64),
}

/// One handler-clause record, gathered while linearizing blocks, consumed after the two-pass
/// layout resolves label offsets into the fat EH section (§II.25.4.6).
struct PendingClause {
    try_start: Label,
    try_end: Label,
    handler_start: Label,
    handler_end: Label,
}

/// Emission state threaded through the linearization pass: the output byte buffer, pending label
/// definitions/references, and gathered EH clauses (both real `BasicBlock` handlers and
/// `TerminateRegion` inner regions).
struct Emitter<'a> {
    asm: &'a mut Assembly,
    tokens: &'a mut dyn TokenSink,
    out: Vec<u8>,
    /// Label -> byte offset, once defined.
    label_offsets: HashMap<Label, u32>,
    /// (offset of the branch's i4 operand, target label) to patch after the whole body is emitted.
    branch_fixups: Vec<(u32, Label)>,
    clauses: Vec<PendingClause>,
    marker_counter: u64,
    terminate_region_counter: u64,
    /// Collected in the same visitation order `emit_root` runs (see this struct's doc and
    /// `AssembledBody::sequence_points`), one entry per `CILRoot::SourceFileInfo` root — the
    /// side-collector this module's doc describes as "the natural seam for Phase 2".
    sequence_points: Vec<SequencePoint>,
}

impl<'a> Emitter<'a> {
    fn define_label(&mut self, label: Label) {
        let off = u32::try_from(self.out.len()).expect("method body exceeds 4 GiB");
        self.label_offsets.insert(label, off);
    }

    /// A fresh anonymous marker bound to the *current* write position.
    fn here(&mut self) -> Label {
        let lbl = Label::Marker(self.marker_counter);
        self.marker_counter += 1;
        self.define_label(lbl);
        lbl
    }

    fn push_u8(&mut self, b: u8) {
        self.out.push(b);
    }
    fn push_i32(&mut self, v: i32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn push_u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn push_i64(&mut self, v: i64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn push_token(&mut self, tok: Token) {
        self.push_u32(tok.0);
    }
    /// Two-byte (`0xFE xx`) opcode (§III.1.3 — the extended opcode space).
    fn push_ext(&mut self, sub: u8) {
        self.push_u8(0xFE);
        self.push_u8(sub);
    }
    /// The `volatile.` prefix, §III.2.6 — a two-byte opcode (`0xFE 0x13`) followed by the
    /// instruction it modifies.
    fn push_volatile_prefix(&mut self) {
        self.push_ext(0x13);
    }

    /// Emits a long-form branch: 1-byte opcode + 4-byte relative-offset placeholder, recorded for
    /// second-pass patching. Always long-form per the Phase 1a module-doc decision.
    fn push_branch(&mut self, opcode: u8, target: Label) {
        self.push_u8(opcode);
        let operand_off = u32::try_from(self.out.len()).unwrap();
        self.push_i32(0); // placeholder
        self.branch_fixups.push((operand_off, target));
    }

    /// Patches every recorded branch operand now that all labels are defined. §II.25.4.4: the
    /// offset is relative to the byte immediately following the 4-byte operand.
    fn patch_branches(&mut self) {
        let fixups = std::mem::take(&mut self.branch_fixups);
        for (operand_off, target) in fixups {
            let target_off = *self.label_offsets.get(&target).unwrap_or_else(|| {
                panic!("branch target label {target:?} was never defined in this method body")
            });
            let rel = i64::from(target_off) - (i64::from(operand_off) + 4);
            let rel = i32::try_from(rel).expect("branch offset overflowed i32");
            self.out[operand_off as usize..operand_off as usize + 4]
                .copy_from_slice(&rel.to_le_bytes());
        }
    }

    fn resolve(&self, label: Label) -> u32 {
        *self
            .label_offsets
            .get(&label)
            .unwrap_or_else(|| panic!("label {label:?} was never defined"))
    }

    // ---- node (value-producing) emission ----------------------------------------------------

    fn emit_node(&mut self, node: Interned<CILNode>) {
        let node = self.asm[node].clone();
        match node {
            CILNode::Const(cst) => self.emit_const(cst.as_ref()),
            CILNode::BinOp(lhs, rhs, op) => {
                self.emit_node(lhs);
                self.emit_node(rhs);
                use crate::ir::BinOp;
                // §III.3 two-byte comparison opcodes: ceq=0xFE01, cgt=0xFE02, cgt.un=0xFE03,
                // clt=0xFE04, clt.un=0xFE05.
                match op {
                    BinOp::Add => self.push_u8(0x58),
                    BinOp::Eq => self.push_ext(0x01), // ceq
                    BinOp::Sub => self.push_u8(0x59),
                    BinOp::Mul => self.push_u8(0x5A),
                    BinOp::Gt => self.push_ext(0x02),   // cgt
                    BinOp::GtUn => self.push_ext(0x03), // cgt.un
                    BinOp::Lt => self.push_ext(0x04),   // clt
                    BinOp::LtUn => self.push_ext(0x05), // clt.un
                    BinOp::Or => self.push_u8(0x60),
                    BinOp::XOr => self.push_u8(0x61),
                    BinOp::And => self.push_u8(0x5F),
                    BinOp::Rem => self.push_u8(0x5D),
                    BinOp::RemUn => self.push_u8(0x5E),
                    BinOp::Shl => self.push_u8(0x62),
                    BinOp::Shr => self.push_u8(0x63),
                    BinOp::ShrUn => self.push_u8(0x64),
                    BinOp::DivUn => self.push_u8(0x5C),
                    BinOp::Div => self.push_u8(0x5B),
                }
            }
            CILNode::UnOp(arg, un) => {
                self.emit_node(arg);
                match un {
                    UnOp::Not => self.push_u8(0x66),
                    UnOp::Neg => self.push_u8(0x65),
                }
            }
            CILNode::LdLoc(loc) => self.emit_ldloc(loc),
            CILNode::LdLocA(loc) => self.emit_ldloca(loc),
            CILNode::LdArg(arg) => self.emit_ldarg(arg),
            CILNode::LdArgA(arg) => self.emit_ldarga(arg),
            CILNode::Call(call) => {
                let (mref, args, _is_pure) = *call;
                for arg in args.iter() {
                    self.emit_node(*arg);
                }
                self.emit_call_like(mref, false);
            }
            CILNode::IntCast {
                input,
                target,
                extend,
            } => {
                self.emit_node(input);
                self.emit_int_cast(target, extend);
            }
            CILNode::FloatCast {
                input,
                target,
                is_signed,
            } => {
                self.emit_node(input);
                self.emit_float_cast(target, is_signed);
            }
            CILNode::RefToPtr(inner) => {
                self.emit_node(inner);
                self.push_u8(0xE0); // conv.u
            }
            CILNode::PtrCast(val, _) => self.emit_node(val),
            CILNode::LdFieldAddress { addr, field } => {
                self.emit_node(addr);
                let tok = self.tokens.field_token(self.asm, field);
                self.push_u8(0x7C); // ldflda
                self.push_token(tok);
            }
            CILNode::LdField { addr, field } => {
                self.emit_node(addr);
                let tok = self.tokens.field_token(self.asm, field);
                self.push_u8(0x7B); // ldfld
                self.push_token(tok);
            }
            CILNode::LdInd {
                addr,
                tpe,
                volatile,
            } => {
                self.emit_node(addr);
                let tpe_val = self.asm[tpe];
                self.emit_ldind(tpe_val, volatile);
            }
            CILNode::SizeOf(tpe) => {
                let tpe_val = self.asm[tpe];
                if tpe_val == Type::Void {
                    // Mirrors il_exporter's WARNING + ldc.i4.0 fallback.
                    self.push_u8(0x16); // ldc.i4.0
                } else {
                    let tok = self.tokens.type_token(self.asm, tpe_val);
                    self.push_ext(0x1C); // sizeof
                    self.push_token(tok);
                }
            }
            CILNode::GetException => {
                // The exception object is already on the eval stack at the start of a handler
                // (§I.12.4.2.5); this node just names it, it emits no bytes.
            }
            CILNode::IsInst(val, tpe) => {
                self.emit_node(val);
                let tpe_val = self.asm[tpe];
                let tok = self.tokens.type_token(self.asm, tpe_val);
                self.push_u8(0x75); // isinst
                self.push_token(tok);
            }
            CILNode::CheckedCast(val, tpe) => {
                self.emit_node(val);
                let tpe_val = self.asm[tpe];
                let tok = self.tokens.type_token(self.asm, tpe_val);
                self.push_u8(0x74); // castclass
                self.push_token(tok);
            }
            CILNode::CallI(calli) => {
                let (fn_ptr, fn_sig, args) = *calli;
                for arg in args.iter() {
                    self.emit_node(*arg);
                }
                self.emit_node(fn_ptr);
                let tok = self.calli_sig_token(fn_sig);
                self.push_u8(0x29); // calli
                self.push_token(tok);
            }
            CILNode::LocAlloc { size } => {
                self.emit_node(size);
                self.push_ext(0x0F); // localloc
            }
            CILNode::LdStaticField(sfld) => {
                let tok = self.tokens.static_field_token(self.asm, sfld);
                self.push_u8(0x7E); // ldsfld
                self.push_token(tok);
            }
            CILNode::LdStaticFieldAddress(sfld) => {
                let tok = self.tokens.static_field_token(self.asm, sfld);
                self.push_u8(0x7F); // ldsflda
                self.push_token(tok);
            }
            CILNode::LdFtn(ftn) => {
                let tok = self.method_token_for_mref(ftn);
                self.push_ext(0x06); // ldftn
                self.push_token(tok);
            }
            CILNode::LdTypeToken(tok_ty) => {
                let tpe_val = self.asm[tok_ty];
                let tok = self.tokens.type_token(self.asm, tpe_val);
                self.push_u8(0xD0); // ldtoken
                self.push_token(tok);
            }
            CILNode::LdLen(array) => {
                self.emit_node(array);
                self.push_u8(0x8E); // ldlen
            }
            CILNode::LocAllocAlgined { tpe, align } => self.emit_locaalloc_aligned(tpe, align),
            CILNode::LdElelemRef { array, index } => {
                self.emit_node(array);
                self.emit_node(index);
                self.push_u8(0x9A); // ldelem.ref
            }
            CILNode::UnboxAny { object, tpe } => {
                self.emit_node(object);
                let tpe_val = self.asm[tpe];
                let tok = self.tokens.type_token(self.asm, tpe_val);
                self.push_u8(0xA5); // unbox.any
                self.push_token(tok);
            }
            CILNode::Box { value, tpe } => {
                self.emit_node(value);
                let tpe_val = self.asm[tpe];
                let tok = self.tokens.type_token(self.asm, tpe_val);
                self.push_u8(0x8C); // box
                self.push_token(tok);
            }
            CILNode::NewArr { elem, len } => {
                self.emit_node(len);
                let tpe_val = self.asm[elem];
                let tok = self.tokens.type_token(self.asm, tpe_val);
                self.push_u8(0x8D); // newarr
                self.push_token(tok);
            }
        }
    }

    fn emit_ldloc(&mut self, loc: u32) {
        match loc {
            0 => self.push_u8(0x06),
            1 => self.push_u8(0x07),
            2 => self.push_u8(0x08),
            3 => self.push_u8(0x09),
            4..=255 => {
                self.push_u8(0x11); // ldloc.s
                self.push_u8(loc as u8);
            }
            _ => {
                self.push_ext(0x0C); // ldloc
                self.push_u32(loc);
            }
        }
    }
    fn emit_ldloca(&mut self, loc: u32) {
        if loc <= 255 {
            self.push_u8(0x12); // ldloca.s
            self.push_u8(loc as u8);
        } else {
            self.push_ext(0x0D); // ldloca
            self.push_u32(loc);
        }
    }
    fn emit_ldarg(&mut self, arg: u32) {
        match arg {
            0 => self.push_u8(0x02),
            1 => self.push_u8(0x03),
            2 => self.push_u8(0x04),
            3 => self.push_u8(0x05),
            4..=255 => {
                self.push_u8(0x0E); // ldarg.s
                self.push_u8(arg as u8);
            }
            _ => {
                self.push_ext(0x09); // ldarg
                self.push_u32(arg);
            }
        }
    }
    fn emit_ldarga(&mut self, arg: u32) {
        if arg <= 255 {
            self.push_u8(0x0F); // ldarga.s
            self.push_u8(arg as u8);
        } else {
            self.push_ext(0x0A); // ldarga
            self.push_u32(arg);
        }
    }
    fn emit_stloc(&mut self, loc: u32) {
        match loc {
            0 => self.push_u8(0x0A),
            1 => self.push_u8(0x0B),
            2 => self.push_u8(0x0C),
            3 => self.push_u8(0x0D),
            4..=255 => {
                self.push_u8(0x13); // stloc.s
                self.push_u8(loc as u8);
            }
            _ => {
                self.push_ext(0x0E); // stloc
                self.push_u32(loc);
            }
        }
    }
    fn emit_starg(&mut self, arg: u32) {
        if arg <= 255 {
            self.push_u8(0x10); // starg.s
            self.push_u8(arg as u8);
        } else {
            self.push_ext(0x0B); // starg
            self.push_u32(arg);
        }
    }

    /// `ldc.i4` short-form ladder + the widening constant forms (§III.3.39/III.3.40), mirroring
    /// every `il_exporter::export_node`'s `Const` arm width-by-width (same numeric bands, same
    /// trailing `conv.*`).
    fn emit_const(&mut self, cst: &Const) {
        match cst {
            Const::ByteBuffer { data, tpe: _ } => {
                let tok = self.const_blob_field_token(*data);
                self.push_u8(0x7F); // ldsflda
                self.push_token(tok);
            }
            Const::Null(_) => self.push_u8(0x14), // ldnull
            // `I8` is special: every value fits an `i8`, so `il_exporter` never needs the full
            // `ldc.i4` form — everything outside `-1`/`0..=8` uses `.s` (mirrors its own arm,
            // which has no `9..=127`/full split, just a single `_ => ldc.i4.s` catch-all).
            Const::I8(v) => self.emit_ldc_i4_i8_band(i64::from(*v)),
            Const::I16(v) => self.emit_ldc_i4_signed(i64::from(*v)),
            Const::I32(v) => self.emit_ldc_i4_signed(i64::from(*v)),
            Const::I64(v) => self.emit_ldc_i8_via_i4(*v),
            Const::ISize(v) => {
                // Same `ldc.i4.*`/`ldc.i8` width ladder as `I64`, but followed by `conv.i` instead
                // of `conv.i8` (mirrors `il_exporter`'s `ISize` arm exactly).
                self.emit_ldc_i8_via_i4_with_conv(*v, 0xD3);
            }
            Const::U8(v) => self.emit_ldc_i4_signed(i64::from(*v)),
            Const::U16(v) => self.emit_ldc_i4_signed(i64::from(*v)),
            Const::U32(v) => self.emit_ldc_i4_unsigned(i64::from(*v)),
            Const::U64(v) => self.emit_ldc_u64(*v),
            Const::USize(v) => self.emit_ldc_usize(*v),
            Const::I128(v) => self.emit_i128(*v),
            Const::U128(v) => self.emit_u128(*v),
            Const::PlatformString(s) => {
                let s = self.asm[*s].to_string();
                let tok = self.tokens.user_string_token(&s);
                self.push_u8(0x72); // ldstr
                self.push_token(tok);
            }
            Const::Bool(v) => self.push_u8(if *v { 0x17 } else { 0x16 }),
            Const::F32(f) => {
                self.push_u8(0x22); // ldc.r4
                self.out.extend_from_slice(&f.0.to_le_bytes());
            }
            Const::F64(f) => {
                self.push_u8(0x23); // ldc.r8
                self.out.extend_from_slice(&f.0.to_le_bytes());
            }
        }
    }

    /// `ldc.i4` ladder for signed-band constants: `-1` -> `.m1`, `0..=8` -> `.N`, `9..=127` ->
    /// `.s`, else full `ldc.i4`. Matches `il_exporter`'s `I16`/`I32` arms exactly (note: `I8` has
    /// a DIFFERENT band — see [`Self::emit_ldc_i4_i8_band`] — since every `i8` value fits `.s`).
    fn emit_ldc_i4_signed(&mut self, val: i64) {
        match val {
            -1 => self.push_u8(0x15), // ldc.i4.m1
            0..=8 => self.push_short_form_ldc(val as u8),
            9..=127 => {
                self.push_u8(0x1F); // ldc.i4.s
                self.push_u8(val as i8 as u8);
            }
            _ => {
                self.push_u8(0x20); // ldc.i4
                self.push_i32(val as i32);
            }
        }
    }
    /// `ldc.i4` ladder for `Const::I8` specifically: `-1` -> `.m1`, `0..=8` -> `.N`, else `.s` —
    /// no full-width fallback, since every `i8` value (down to `-128`) fits the `.s` sbyte operand
    /// (mirrors `il_exporter`'s `I8` arm, which has no `9..=127`/full split).
    fn emit_ldc_i4_i8_band(&mut self, val: i64) {
        match val {
            -1 => self.push_u8(0x15), // ldc.i4.m1
            0..=8 => self.push_short_form_ldc(val as u8),
            _ => {
                self.push_u8(0x1F); // ldc.i4.s
                self.push_u8(val as i8 as u8);
            }
        }
    }
    /// The `ldc.i4.0`..`ldc.i4.8` one-byte opcode ladder shared by every constant width's
    /// `0..=8` band.
    fn push_short_form_ldc(&mut self, val: u8) {
        self.push_u8(0x16 + val); // ldc.i4.0(=0x16) + val
    }
    /// U32 band: same ladder, `0..=8` -> `.N`, `9..=127` -> `.s`, else full — no `-1` case since
    /// unsigned constants never lower to a `-1` literal.
    fn emit_ldc_i4_unsigned(&mut self, val: i64) {
        match val {
            0..=8 => self.emit_ldc_i4_signed(val),
            9..=127 => {
                self.push_u8(0x1F);
                self.push_u8(val as u8);
            }
            _ => {
                self.push_u8(0x20);
                self.push_i32(val as u32 as i32);
            }
        }
    }
    /// I64: `ldc.i4.*` + `conv.i8` for values fitting i32, else `ldc.i8` directly.
    fn emit_ldc_i8_via_i4(&mut self, val: i64) {
        self.emit_ldc_i8_via_i4_with_conv(val, 0x6A); // conv.i8
    }
    /// Shared shape for `I64`/`ISize`: `ldc.i4.*` (for values fitting an i32) or `ldc.i8` (for
    /// values that don't), followed by `trailing_conv` (`conv.i8` for `I64`, `conv.i` for
    /// `ISize` — mirrors `il_exporter`'s two near-identical `match` arms).
    fn emit_ldc_i8_via_i4_with_conv(&mut self, val: i64, trailing_conv: u8) {
        if (i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&val) {
            self.emit_ldc_i4_signed(val);
        } else {
            self.push_u8(0x21); // ldc.i8
            self.push_i64(val);
        }
        self.push_u8(trailing_conv);
    }
    fn emit_ldc_u64(&mut self, val: u64) {
        match val {
            0..=8 => {
                self.emit_ldc_i4_signed(val as i64);
                self.push_u8(0x6E); // conv.u8
            }
            9..=127 => {
                self.push_u8(0x1F);
                self.push_u8(val as u8);
                self.push_u8(0x6E);
            }
            128..=4_294_967_295 => {
                self.push_u8(0x20);
                self.push_i32(val as u32 as i32);
                self.push_u8(0x6E);
            }
            _ => {
                self.push_u8(0x21); // ldc.i8
                self.push_i64(val as i64);
            }
        }
    }
    fn emit_ldc_usize(&mut self, val: u64) {
        match val {
            0..=8 => {
                self.emit_ldc_i4_signed(val as i64);
                self.push_u8(0xE0); // conv.u
            }
            9..=127 => {
                self.push_u8(0x1F);
                self.push_u8(val as u8);
                self.push_u8(0xE0);
            }
            128..=2_147_483_647 => {
                self.push_u8(0x20);
                self.push_i32(val as u32 as i32);
                self.push_u8(0xE0);
            }
            _ => {
                self.push_u8(0x21);
                self.push_i64(val as i64);
                self.push_u8(0xE0);
            }
        }
    }
    /// `System.Int128`: `ldc.i4.*`/`ldc.i8` + `Int128::op_Implicit`, or the 2-`ldc.i8` + `.ctor`
    /// form for values that don't fit an i64 — mirrors `il_exporter`'s exact numeric bands.
    fn emit_i128(&mut self, val: i128) {
        let cref = self.bcl_valuetype_ref("System.Int128");
        // Band structure mirrors `il_exporter`'s `I128` arm exactly (NOT the `I8`/`I16`/`I32`
        // ladder — this one has its own 5-way split: `-1`, `0..=8`, `9..=127` -> `.s`,
        // `[-2^31,0) | [128,2^31)` -> full `ldc.i4` (negatives here do NOT get `.s`, unlike
        // `I128`'s sibling scalar-const arms), `[-2^63,-2^31) | [2^31,2^63)` -> `ldc.i8`, else the
        // two-`ldc.i8` + `.ctor` fallback.
        match val {
            -1 => {
                self.push_u8(0x15); // ldc.i4.m1
                self.emit_int128_op_implicit(cref, Type::Int(Int::I32));
            }
            0..=8 => {
                self.push_short_form_ldc(val as u8);
                self.emit_int128_op_implicit(cref, Type::Int(Int::I32));
            }
            9..=127 => {
                self.push_u8(0x1F); // ldc.i4.s
                self.push_u8(val as i8 as u8);
                self.emit_int128_op_implicit(cref, Type::Int(Int::I32));
            }
            -2_147_483_648..0 | 128..=2_147_483_647 => {
                self.push_u8(0x20); // ldc.i4
                self.push_i32(val as i32);
                self.emit_int128_op_implicit(cref, Type::Int(Int::I32));
            }
            -9_223_372_036_854_775_808..-2_147_483_648
            | 2_147_483_648..=9_223_372_036_854_775_807 => {
                self.push_u8(0x21); // ldc.i8
                self.push_i64(val as i64);
                self.emit_int128_op_implicit(cref, Type::Int(Int::I64));
            }
            _ => {
                let low = (val as u128 & u128::from(u64::MAX)) as u64;
                let high = ((val as u128) >> 64) as u64;
                self.push_u8(0x21); // ldc.i8 <high>
                self.push_i64(high as i64);
                self.push_u8(0x21); // ldc.i8 <low>
                self.push_i64(low as i64);
                self.emit_int128_ctor(cref);
            }
        }
    }
    fn emit_u128(&mut self, val: u128) {
        let cref = self.bcl_valuetype_ref("System.UInt128");
        match val {
            0..=8 => {
                self.emit_ldc_i4_signed(val as i64);
                self.emit_int128_op_implicit(cref, Type::Int(Int::U32));
            }
            9..=127 => {
                self.push_u8(0x1F);
                self.push_u8(val as u8);
                self.emit_int128_op_implicit(cref, Type::Int(Int::U32));
            }
            128..=4_294_967_295 => {
                self.push_u8(0x20);
                self.push_i32(val as u32 as i32);
                self.emit_int128_op_implicit(cref, Type::Int(Int::U32));
            }
            4_294_967_296..=18_446_744_073_709_551_615 => {
                self.push_u8(0x21);
                self.push_i64(val as u64 as i64);
                self.emit_int128_op_implicit(cref, Type::Int(Int::U64));
            }
            _ => {
                let low = (val & u128::from(u64::MAX)) as u64;
                let high = (val >> 64) as u64;
                self.push_u8(0x21);
                self.push_i64(high as i64);
                self.push_u8(0x21);
                self.push_i64(low as i64);
                self.emit_int128_ctor(cref);
            }
        }
    }

    /// Resolves the `[System.Runtime]System.{Int128,UInt128,Half,Object,Exception,Environment}`
    /// value/reference-type `ClassRef` this backend needs for its fixed BCL call sites (mirrors
    /// the same hard-coded names `il_exporter` interpolates verbatim).
    fn bcl_class_ref(&mut self, name: &'static str, is_valuetype: bool) -> Interned<ClassRef> {
        let asm_name = self.asm.alloc_string("System.Runtime");
        let type_name = self.asm.alloc_string(name);
        self.asm.alloc_class_ref(ClassRef::new(
            type_name,
            Some(asm_name),
            is_valuetype,
            [].into(),
        ))
    }
    fn bcl_valuetype_ref(&mut self, name: &'static str) -> Interned<ClassRef> {
        self.bcl_class_ref(name, true)
    }

    fn emit_int128_op_implicit(&mut self, cref: Interned<ClassRef>, from: Type) {
        let out_ty = Type::ClassRef(cref);
        let sig = self.asm.sig([from], out_ty);
        let mref = self.asm.new_methodref(
            cref,
            "op_Implicit",
            sig,
            crate::ir::cilnode::MethodKind::Static,
            vec![],
        );
        let tok = self.method_token_for_mref(mref);
        self.push_u8(0x28); // call
        self.push_token(tok);
    }
    fn emit_int128_ctor(&mut self, cref: Interned<ClassRef>) {
        // Every `Constructor`-kind `MethodRef`'s stored `FnSig` carries the owning type as an
        // IMPLICIT receiver at `inputs()[0]` — the same convention the backend's call lowering
        // `MethodRef::new(... MethodKind::Constructor ...)` call sites always follow (`this` is
        // prepended before the explicit ctor args), and which `TokenSink::method_token`
        // (`tables.rs`) / `il_exporter`'s own MemberRef-rendering (`mod.rs` `&sig.inputs()[1..]`)
        // both unconditionally strip back off before encoding. Omitting it here made this
        // writer's `method_token` strip a REAL constructor argument instead of a placeholder,
        // encoding `UInt128::.ctor(uint64,uint64)` as a bogus 1-arg `.ctor(uint64)` MemberRef —
        // a real regression caught by `dotnet-ilverify` on `cargo_tests/cd_collections`
        // (164 `MissingMethod: Void System.UInt128..ctor(UInt64)` errors, 0 for the ilasm A/B
        // twin) that manifested at runtime as a `FileLoadException` the moment any JITted method
        // referencing this bogus MemberRef got resolved.
        let this_ty = Type::ClassRef(cref);
        let sig = self.asm.sig(
            [this_ty, Type::Int(Int::U64), Type::Int(Int::U64)],
            Type::Void,
        );
        let mref = self.asm.new_methodref(
            cref,
            ".ctor",
            sig,
            crate::ir::cilnode::MethodKind::Constructor,
            vec![],
        );
        let tok = self.method_token_for_mref(mref);
        self.push_u8(0x73); // newobj
        self.push_token(tok);
    }

    /// Token for the synthetic blob-sized FieldRVA static field a const-data buffer lowers to
    /// (mirrors `il_exporter`'s `c_N`/`__rcl_const_blob_N` pair, `mod.rs` lines ~107-127). The
    /// `tables.rs` populate pass is expected to have already registered the matching
    /// `StaticFieldDesc` (same owner/name/type triple) before body assembly runs — body.rs only
    /// re-derives the *lookup key*, not the row itself, and hands it to the sink like any other
    /// static-field reference.
    fn const_blob_field_token(&mut self, data: Interned<Box<[u8]>>) -> Token {
        let main_module = self.asm.main_module();
        let n = self.asm.const_data[data].len().max(1);
        let field_name = self
            .asm
            .alloc_string(format!("c_{}", crate::utilis::encode(data.inner() as u64)));
        let blob_ty_name = self.asm.alloc_string(format!("__rcl_const_blob_{n}"));
        let blob_cref =
            self.asm
                .alloc_class_ref(ClassRef::new(blob_ty_name, None, true, [].into()));
        let sfld = crate::ir::field::StaticFieldDesc::new(
            *main_module,
            field_name,
            Type::ClassRef(blob_cref),
        );
        let sfld = self.asm.alloc_sfld(sfld);
        self.tokens.static_field_token(self.asm, sfld)
    }

    fn emit_int_cast(&mut self, target: Int, extend: ExtendKind) {
        match (target, extend) {
            (Int::U8 | Int::I8, ExtendKind::ZeroExtend) => self.push_u8(0xD2), // conv.u1
            (Int::U8 | Int::I8, ExtendKind::SignExtend) => self.push_u8(0x67), // conv.i1
            (Int::U16 | Int::I16, ExtendKind::ZeroExtend) => self.push_u8(0xD1), // conv.u2
            (Int::U16 | Int::I16, ExtendKind::SignExtend) => self.push_u8(0x68), // conv.i2
            (Int::U32 | Int::I32, ExtendKind::ZeroExtend) => self.push_u8(0x6D), // conv.u4
            (Int::U32 | Int::I32, ExtendKind::SignExtend) => self.push_u8(0x69), // conv.i4
            (Int::U64 | Int::I64, ExtendKind::ZeroExtend) => self.push_u8(0x6E), // conv.u8
            (Int::U64 | Int::I64, ExtendKind::SignExtend) => self.push_u8(0x6A), // conv.i8
            (Int::USize | Int::ISize, ExtendKind::SignExtend) => self.push_u8(0xD3), // conv.i
            (Int::USize | Int::ISize, ExtendKind::ZeroExtend) => self.push_u8(0xE0), // conv.u
            (Int::U128, ExtendKind::ZeroExtend)
            | (Int::U128, ExtendKind::SignExtend)
            | (Int::I128, ExtendKind::ZeroExtend)
            | (Int::I128, ExtendKind::SignExtend) => {
                todo!(
                    "§III IntCast to/from a 128-bit int — il_exporter itself has no encoding for this arm (its own todo!())"
                )
            }
        }
    }
    fn emit_float_cast(&mut self, target: Float, is_signed: bool) {
        match (target, is_signed) {
            (Float::F16, _) => {
                todo!(
                    "§III FloatCast to f16 — il_exporter itself has no encoding for this arm (its own todo!())"
                )
            }
            (Float::F32, true) => self.push_u8(0x6B), // conv.r4
            (Float::F32, false) => {
                self.push_u8(0x76); // conv.r.un
                self.push_u8(0x6B); // conv.r4
            }
            (Float::F64, true) => self.push_u8(0x6C), // conv.r8
            (Float::F64, false) => {
                self.push_u8(0x76);
                self.push_u8(0x6C);
            }
            (Float::F128, _) => {
                todo!(
                    "§III FloatCast to f128 — il_exporter itself has no encoding for this arm (its own todo!())"
                )
            }
        }
    }

    fn emit_ldind(&mut self, tpe: Type, volatile: bool) {
        if volatile {
            self.push_volatile_prefix();
        }
        match tpe {
            Type::Ptr(_) => self.push_u8(0x4D), // ldind.i
            Type::Ref(_) => {
                todo!("§III ldind of a Ref — il_exporter itself has no encoding for this arm")
            }
            Type::Int(int) => match int {
                Int::U8 => self.push_u8(0x47),  // ldind.u1
                Int::U16 => self.push_u8(0x49), // ldind.u2
                Int::U32 => self.push_u8(0x4B), // ldind.u4
                Int::U64 => self.push_u8(0x4C), // ldind.u8 (alias of ldind.i8)
                Int::U128 => {
                    let cref = self.bcl_valuetype_ref("System.UInt128");
                    let tok = self.tokens.type_token(self.asm, Type::ClassRef(cref));
                    self.push_u8(0x71); // ldobj
                    self.push_token(tok);
                }
                Int::USize => self.push_u8(0x4D), // ldind.i
                Int::I8 => self.push_u8(0x46),    // ldind.i1
                Int::I16 => self.push_u8(0x48),   // ldind.i2
                Int::I32 => self.push_u8(0x4A),   // ldind.i4
                Int::I64 => self.push_u8(0x4C),   // ldind.i8
                Int::I128 => {
                    let cref = self.bcl_valuetype_ref("System.Int128");
                    let tok = self.tokens.type_token(self.asm, Type::ClassRef(cref));
                    self.push_u8(0x71); // ldobj
                    self.push_token(tok);
                }
                Int::ISize => self.push_u8(0x4D), // ldind.i
            },
            Type::ClassRef(_) => {
                let tok = self.tokens.type_token(self.asm, tpe);
                self.push_u8(0x71); // ldobj
                self.push_token(tok);
            }
            Type::Float(float) => match float {
                Float::F16 => {
                    let cref = self.bcl_valuetype_ref("System.Half");
                    let tok = self.tokens.type_token(self.asm, Type::ClassRef(cref));
                    self.push_u8(0x71);
                    self.push_token(tok);
                }
                Float::F32 => self.push_u8(0x4E), // ldind.r4
                Float::F64 => self.push_u8(0x4F), // ldind.r8
                Float::F128 => {
                    let tok = self.tokens.type_token(self.asm, tpe);
                    self.push_u8(0x71);
                    self.push_token(tok);
                }
            },
            Type::PlatformString | Type::PlatformObject => self.push_u8(0x50), // ldind.ref
            Type::PlatformChar => self.push_u8(0x49),                          // ldind.u2
            Type::PlatformGeneric(_, _) => {
                todo!(
                    "§III ldind of a generic-parameter type — il_exporter itself has no encoding for this arm"
                )
            }
            Type::Bool => self.push_u8(0x46), // ldind.i1
            Type::Void => panic!("Void can't be dereferenced!"),
            Type::PlatformArray { .. } => self.push_u8(0x50), // ldind.ref
            Type::FnPtr(_) => self.push_u8(0x4D),             // ldind.i
            Type::SIMDVector(_) => {
                let tok = self.tokens.type_token(self.asm, tpe);
                self.push_u8(0x71);
                self.push_token(tok);
            }
        }
    }

    /// Mirrors `il_exporter`'s `LocAllocAlgined` arithmetic exactly:
    /// `sizeof <tpe> ; ldc.i8 <align> conv.i ; add ; localloc ; dup ;
    ///  ldc.i8 <align> add ; ldc.i8 <align> rem ; sub ; ldc.i8 <align> add ; conv.u`.
    fn emit_locaalloc_aligned(&mut self, tpe: Interned<Type>, align: u64) {
        let tpe_val = self.asm[tpe];
        let tok = self.tokens.type_token(self.asm, tpe_val);
        self.push_ext(0x1C); // sizeof
        self.push_token(tok);
        self.push_u8(0x21); // ldc.i8 <align>
        self.push_i64(align as i64);
        self.push_u8(0xD3); // conv.i
        self.push_u8(0x58); // add
        self.push_ext(0x0F); // localloc
        self.push_u8(0x25); // dup
        self.push_u8(0x21); // ldc.i8 <align>
        self.push_i64(align as i64);
        self.push_u8(0x58); // add
        self.push_u8(0x21); // ldc.i8 <align>
        self.push_i64(align as i64);
        self.push_u8(0x5D); // rem
        self.push_u8(0x59); // sub
        self.push_u8(0x21); // ldc.i8 <align>
        self.push_i64(align as i64);
        self.push_u8(0x58); // add
        self.push_u8(0xE0); // conv.u
    }

    /// Resolves the `call`/`callvirt`/`newobj` opcode + token for a `MethodRef`. `is_root_call`
    /// mirrors `il_exporter`'s "A constructor can't be a CIL root" panic for the (invalid)
    /// `CILRoot::Call` on a `Constructor`-kind method.
    fn emit_call_like(&mut self, mref: Interned<crate::ir::MethodRef>, is_root_call: bool) {
        let kind = self.asm[mref].kind();
        if is_root_call && kind == crate::ir::cilnode::MethodKind::Constructor {
            panic!("A constructor can't be a CIL root");
        }
        let tok = self.method_token_for_mref(mref);
        match kind {
            crate::ir::cilnode::MethodKind::Static => self.push_u8(0x28), // call
            crate::ir::cilnode::MethodKind::Instance => self.push_u8(0x28), // call instance
            crate::ir::cilnode::MethodKind::Virtual => self.push_u8(0x6F), // callvirt
            crate::ir::cilnode::MethodKind::Constructor => self.push_u8(0x73), // newobj
        }
        self.push_token(tok);
    }

    fn method_token_for_mref(&mut self, mref: Interned<crate::ir::MethodRef>) -> Token {
        let generics = self.asm[mref].generics().to_vec();
        self.tokens
            .method_token(self.asm, MethodDefIdx::from_raw(mref), &generics)
    }

    fn calli_sig_token(&mut self, fn_sig: Interned<crate::ir::FnSig>) -> Token {
        let sig_val = self.asm[fn_sig].clone();
        let semantic_key = format!(
            "({})->{}",
            sig_val
                .inputs()
                .iter()
                .map(|input| input.mangle(self.asm))
                .collect::<Vec<_>>()
                .join(","),
            sig_val.output().mangle(self.asm)
        );
        let mut blob = Vec::new();
        // Reuse `self` as the resolver: `TokenSink::type_token` already knows how to resolve a
        // `ClassRef` to a `TypeDef`/`TypeRef`/`TypeSpec` token, which is exactly the shape a
        // `TypeDefOrRef` coded index needs (tag 0/1/2 for those three tables respectively).
        struct Resolver<'x, 'y>(&'x mut &'y mut dyn TokenSink);
        impl super::sig::TypeDefOrRefResolver for Resolver<'_, '_> {
            fn type_def_or_ref(&mut self, cref: Interned<ClassRef>, asm: &mut Assembly) -> u32 {
                let tok = self.0.type_token(asm, Type::ClassRef(cref));
                let tag = match tok.table() {
                    Token::TABLE_TYPE_DEF => 0,
                    Token::TABLE_TYPE_REF => 1,
                    Token::TABLE_TYPE_SPEC => 2,
                    other => {
                        panic!("type_token returned table id {other:#x}, not a TypeDefOrRef member")
                    }
                };
                (tok.rid() << 2) | tag
            }
        }
        let mut resolver = Resolver(&mut self.tokens);
        super::sig::encode_method_sig(
            super::sig::SIG_DEFAULT,
            0,
            &sig_val,
            self.asm,
            &mut resolver,
            &mut blob,
        );
        self.tokens.calli_sig_token(self.asm, &semantic_key, &blob)
    }

    // ---- root (statement) emission ---------------------------------------------------------

    fn emit_root(&mut self, root: Interned<CILRoot>, is_handler: bool, has_handler: bool) {
        let root = self.asm[root].clone();
        match root {
            CILRoot::StLoc(loc, val) => {
                self.emit_node(val);
                self.emit_stloc(loc);
            }
            CILRoot::StArg(loc, val) => {
                self.emit_node(val);
                self.emit_starg(loc);
            }
            CILRoot::Ret(val) => {
                self.emit_node(val);
                self.push_u8(0x2A); // ret
            }
            CILRoot::Pop(val) => {
                self.emit_node(val);
                self.push_u8(0x26); // pop
            }
            CILRoot::Throw(val) => {
                self.emit_node(val);
                self.push_u8(0x7A); // throw
            }
            CILRoot::VoidRet => self.push_u8(0x2A), // ret
            CILRoot::Break => self.push_u8(0x01),   // break
            CILRoot::Nop => self.push_u8(0x00),     // nop
            CILRoot::Branch(branch) => self.emit_branch(*branch, is_handler, has_handler),
            CILRoot::SourceFileInfo {
                line_start,
                line_len,
                col_start,
                col_len,
                file,
            } => {
                // Debug-info only — emits no IL bytes (mirrors `il_exporter`'s `.line` directive,
                // which is likewise not an instruction). The sequence point is attributed to
                // WHATEVER instruction comes next in the code stream, i.e. `self.out.len()` at
                // this exact point — parity with `il_exporter`, whose `.line` directive textually
                // precedes (and thus, per ilasm's own PDB writer, applies to) the very next
                // emitted instruction.
                let il_offset = u32::try_from(self.out.len()).expect("method body exceeds 4 GiB");
                let file_path = self.asm[file].to_string();
                let line = line_start;
                let end_line = line_start + u32::from(line_len);
                let col = u32::from(col_start);
                let end_col = col + u32::from(col_len);
                self.sequence_points.push(SequencePoint {
                    il_offset,
                    document_path: file_path,
                    line,
                    col,
                    end_line,
                    end_col,
                    is_hidden: false,
                });
            }
            CILRoot::SetField(flds) => {
                let (field, addr, val) = *flds;
                self.emit_node(addr);
                self.emit_node(val);
                let tok = self.tokens.field_token(self.asm, field);
                self.push_u8(0x7D); // stfld
                self.push_token(tok);
            }
            CILRoot::Call(call) => {
                let (mref, args, _is_pure) = *call;
                for arg in args.iter() {
                    self.emit_node(*arg);
                }
                self.emit_call_like(mref, true);
            }
            CILRoot::CpObj { src, dst, tpe } => {
                self.emit_node(src);
                self.emit_node(dst);
                let tpe_val = self.asm[tpe];
                let tok = self.tokens.type_token(self.asm, tpe_val);
                self.push_u8(0x70); // cpobj
                self.push_token(tok);
            }
            CILRoot::InitObj(addr, tpe) => {
                self.emit_node(addr);
                let tpe_val = self.asm[tpe];
                let tok = self.tokens.type_token(self.asm, tpe_val);
                self.push_ext(0x15); // initobj
                self.push_token(tok);
            }
            CILRoot::StInd(stind) => {
                let (addr, val, tpe, volatile) = *stind;
                self.emit_node(addr);
                self.emit_node(val);
                self.emit_stind(tpe, volatile);
            }
            CILRoot::InitBlk(blk) => {
                let (dst, val, count) = *blk;
                self.emit_node(dst);
                self.emit_node(val);
                self.emit_node(count);
                self.push_ext(0x18); // initblk
            }
            CILRoot::CpBlk(cpblk) => {
                let (dst, src, len) = *cpblk;
                self.emit_node(dst);
                self.emit_node(src);
                self.emit_node(len);
                self.push_ext(0x17); // cpblk
            }
            CILRoot::CallI(calli) => {
                let (fn_ptr, fn_sig, args) = *calli;
                for arg in args.iter() {
                    self.emit_node(*arg);
                }
                self.emit_node(fn_ptr);
                let tok = self.calli_sig_token(fn_sig);
                self.push_u8(0x29); // calli
                self.push_token(tok);
            }
            CILRoot::TerminateRegion { protected, reason } => {
                self.emit_terminate_region(protected, reason, is_handler, has_handler);
            }
            CILRoot::ExitSpecialRegion { target, source } => {
                if is_handler {
                    self.define_label(Label::HandlerLeave(source, target));
                    self.push_branch(0xDD, Label::Block(target)); // leave (long form)
                } else if has_handler {
                    self.define_label(Label::ProtectedLeave(source, target));
                    self.push_branch(0xDD, Label::Block(target)); // leave
                }
                // else: no bytes, mirrors il_exporter's `Ok(())` fall-through arm.
            }
            CILRoot::ReThrow => self.push_ext(0x1A), // rethrow
            CILRoot::SetStaticField { field, val } => {
                self.emit_node(val);
                let tok = self.tokens.static_field_token(self.asm, field);
                self.push_u8(0x80); // stsfld
                self.push_token(tok);
            }
            CILRoot::Unreachable(msg) => {
                let s = self.asm[msg].to_string();
                self.emit_throw_new_exception(&s);
            }
            CILRoot::StElem {
                array,
                index,
                value,
                elem,
            } => {
                self.emit_node(array);
                self.emit_node(index);
                self.emit_node(value);
                let elem_val = self.asm[elem];
                match elem_val {
                    Type::Int(Int::I8 | Int::U8) => self.push_u8(0x9C), // stelem.i1
                    Type::Int(Int::I16 | Int::U16) => self.push_u8(0x9D), // stelem.i2
                    Type::Int(Int::I32 | Int::U32) => self.push_u8(0x9E), // stelem.i4
                    Type::Int(Int::I64 | Int::U64) => self.push_u8(0x9F), // stelem.i8
                    Type::Int(Int::ISize | Int::USize) => self.push_u8(0x9B), // stelem.i
                    Type::Bool => self.push_u8(0x9C),                   // stelem.i1
                    Type::Float(Float::F32) => self.push_u8(0xA0),      // stelem.r4
                    Type::Float(Float::F64) => self.push_u8(0xA1),      // stelem.r8
                    _ => {
                        let tok = self.tokens.type_token(self.asm, elem_val);
                        self.push_u8(0xA4); // stelem <type>
                        self.push_token(tok);
                    }
                }
            }
        }
    }

    fn emit_branch(
        &mut self,
        branch: (u32, u32, Option<BranchCond>),
        is_handler: bool,
        has_handler: bool,
    ) {
        let (target, sub_target, cond) = branch;
        let label = branch_label(target, sub_target, has_handler, is_handler);
        match cond {
            Some(BranchCond::Eq(a, b)) => {
                self.emit_node(a);
                self.emit_node(b);
                self.push_branch(0x3B, label); // beq
            }
            Some(BranchCond::Ne(a, b)) => {
                self.emit_node(a);
                self.emit_node(b);
                self.push_branch(0x40, label); // bne.un
            }
            Some(BranchCond::Lt(a, b, kind)) => {
                self.emit_node(a);
                self.emit_node(b);
                let op = match kind {
                    CmpKind::Ordered | CmpKind::Signed => 0x3F,     // blt
                    CmpKind::Unordered | CmpKind::Unsigned => 0x44, // blt.un
                };
                self.push_branch(op, label);
            }
            Some(BranchCond::Gt(a, b, kind)) => {
                self.emit_node(a);
                self.emit_node(b);
                let op = match kind {
                    CmpKind::Ordered | CmpKind::Signed => 0x3D,     // bgt
                    CmpKind::Unordered | CmpKind::Unsigned => 0x42, // bgt.un
                };
                self.push_branch(op, label);
            }
            Some(BranchCond::Le(a, b, kind)) => {
                self.emit_node(a);
                self.emit_node(b);
                let op = match kind {
                    CmpKind::Ordered | CmpKind::Signed => 0x3E,     // ble
                    CmpKind::Unordered | CmpKind::Unsigned => 0x43, // ble.un
                };
                self.push_branch(op, label);
            }
            Some(BranchCond::Ge(a, b, kind)) => {
                self.emit_node(a);
                self.emit_node(b);
                let op = match kind {
                    CmpKind::Ordered | CmpKind::Signed => 0x3C,     // bge
                    CmpKind::Unordered | CmpKind::Unsigned => 0x41, // bge.un
                };
                self.push_branch(op, label);
            }
            Some(BranchCond::True(c)) => {
                self.emit_node(c);
                self.push_branch(0x3A, label); // brtrue
            }
            Some(BranchCond::False(c)) => {
                self.emit_node(c);
                self.push_branch(0x39, label); // brfalse
            }
            None => self.push_branch(0x38, label), // br
        }
    }

    fn emit_stind(&mut self, tpe: Type, volatile: bool) {
        if volatile {
            self.push_volatile_prefix();
        }
        match tpe {
            Type::Ptr(_) => self.push_u8(0xDF), // stind.i
            Type::Ref(_) => {
                todo!("§III stind of a Ref — il_exporter itself has no encoding for this arm")
            }
            Type::Int(int) => match int {
                Int::U8 | Int::I8 => self.push_u8(0x52),   // stind.i1
                Int::U16 | Int::I16 => self.push_u8(0x53), // stind.i2
                Int::U32 | Int::I32 => self.push_u8(0x54), // stind.i4
                Int::U64 | Int::I64 => self.push_u8(0x55), // stind.i8
                Int::U128 => {
                    let cref = self.bcl_valuetype_ref("System.UInt128");
                    let tok = self.tokens.type_token(self.asm, Type::ClassRef(cref));
                    self.push_u8(0x81); // stobj
                    self.push_token(tok);
                }
                Int::I128 => {
                    let cref = self.bcl_valuetype_ref("System.Int128");
                    let tok = self.tokens.type_token(self.asm, Type::ClassRef(cref));
                    self.push_u8(0x81);
                    self.push_token(tok);
                }
                Int::USize | Int::ISize => self.push_u8(0xDF), // stind.i
            },
            Type::ClassRef(cref_idx) => {
                let is_valuetype = self.asm[cref_idx].is_valuetype();
                if is_valuetype {
                    let tok = self.tokens.type_token(self.asm, tpe);
                    self.push_u8(0x81); // stobj
                    self.push_token(tok);
                } else {
                    self.push_u8(0x51); // stind.ref
                }
            }
            Type::Float(float) => match float {
                Float::F16 => {
                    let cref = self.bcl_valuetype_ref("System.Half");
                    let tok = self.tokens.type_token(self.asm, Type::ClassRef(cref));
                    self.push_u8(0x81);
                    self.push_token(tok);
                }
                Float::F32 => self.push_u8(0x56), // stind.r4
                Float::F64 => self.push_u8(0x57), // stind.r8
                Float::F128 => {
                    let tok = self.tokens.type_token(self.asm, tpe);
                    self.push_u8(0x81);
                    self.push_token(tok);
                }
            },
            Type::PlatformString | Type::PlatformObject => self.push_u8(0x51), // stind.ref
            Type::PlatformChar => self.push_u8(0x53),                          // stind.i2
            Type::PlatformGeneric(_, _) => {
                todo!(
                    "§III stind of a generic-parameter type — il_exporter itself has no encoding for this arm"
                )
            }
            Type::Bool => self.push_u8(0x52), // stind.i1
            Type::Void => {
                // Mirrors il_exporter's `pop pop ldstr … newobj … throw` guard for this
                // never-valid-at-runtime case.
                self.push_u8(0x26); // pop
                self.push_u8(0x26); // pop
                self.emit_throw_new_exception("Attempted to wrtie to a zero-sized type(void).");
            }
            Type::PlatformArray { .. } => self.push_u8(0x51), // stind.ref
            Type::FnPtr(_) => self.push_u8(0xDF),             // stind.i
            Type::SIMDVector(_) => {
                let tok = self.tokens.type_token(self.asm, tpe);
                self.push_u8(0x81);
                self.push_token(tok);
            }
        }
    }

    /// `ldstr <msg> newobj instance void [System.Runtime]System.Exception::.ctor(string) throw` —
    /// the shared shape behind `CILRoot::Unreachable`, `MethodImpl::Missing`, and the `StInd`
    /// void-type guard in `il_exporter`.
    fn emit_throw_new_exception(&mut self, msg: &str) {
        let str_tok = self.tokens.user_string_token(msg);
        self.push_u8(0x72); // ldstr
        self.push_token(str_tok);
        let ctor_tok = self.system_exception_ctor();
        self.push_u8(0x73); // newobj
        self.push_token(ctor_tok);
        self.push_u8(0x7A); // throw
    }

    fn system_exception_ctor(&mut self) -> Token {
        let cref = self.bcl_class_ref("System.Exception", false);
        // Same "implicit receiver at `inputs()[0]`" convention as `emit_int128_ctor` above — see
        // its doc comment. Without the leading `this` type here, `method_token` strips the real
        // (only) `string` argument, encoding a bogus zero-arg `System.Exception::.ctor()`.
        let this_ty = Type::ClassRef(cref);
        let sig = self.asm.sig([this_ty, Type::PlatformString], Type::Void);
        let mref = self.asm.new_methodref(
            cref,
            ".ctor",
            sig,
            crate::ir::cilnode::MethodKind::Constructor,
            vec![],
        );
        self.method_token_for_mref(mref)
    }

    /// Renders the `.try{ <protected>; leave tr_done_N } catch System.Object{ pop; ldstr <msg>;
    /// call Environment::FailFast; rethrow } tr_done_N: nop` shape (§II.25.4.6 fat clause,
    /// mirrors `il_exporter`'s `TerminateRegion` arm exactly, byte offsets instead of labels).
    fn emit_terminate_region(
        &mut self,
        protected: Interned<CILRoot>,
        reason: u8,
        is_handler: bool,
        has_handler: bool,
    ) {
        let lbl_id = self.terminate_region_counter;
        self.terminate_region_counter += 1;
        let done = Label::TerminateDone(lbl_id);

        let try_start = self.here();
        // `is_handler`/`has_handler` are propagated unchanged: the protected op is lexically
        // inside whatever enclosing region this root already lives in.
        self.emit_root(protected, is_handler, has_handler);
        self.push_branch(0xDD, done); // leave tr_done_N
        let try_end = self.here();

        let handler_start = self.here();
        self.push_u8(0x26); // pop (the caught exception)
        let msg = if reason == 1 {
            "Rust panicked while running a destructor during unwinding (panic in a destructor during cleanup); aborted."
        } else {
            "Rust unwinding crossed a `nounwind` ABI boundary (panic in a function that cannot unwind); aborted."
        };
        let str_tok = self.tokens.user_string_token(msg);
        self.push_u8(0x72); // ldstr
        self.push_token(str_tok);
        let failfast_tok = self.environment_failfast();
        self.push_u8(0x28); // call
        self.push_token(failfast_tok);
        self.push_ext(0x1A); // rethrow (FailFast never returns; keeps the catch well-formed)
        let handler_end = self.here();

        self.define_label(done);
        self.push_u8(0x00); // nop

        self.clauses.push(PendingClause {
            try_start,
            try_end,
            handler_start,
            handler_end,
        });
    }

    fn environment_failfast(&mut self) -> Token {
        let cref = self.bcl_class_ref("System.Environment", false);
        let sig = self.asm.sig([Type::PlatformString], Type::Void);
        let mref = self.asm.new_methodref(
            cref,
            "FailFast",
            sig,
            crate::ir::cilnode::MethodKind::Static,
            vec![],
        );
        self.method_token_for_mref(mref)
    }

    fn system_object_token(&mut self) -> Token {
        let cref = self.bcl_class_ref("System.Object", false);
        self.tokens.type_token(self.asm, Type::ClassRef(cref))
    }
}

fn branch_label(target: u32, sub_target: u32, has_handler: bool, is_handler: bool) -> Label {
    if sub_target == 0 {
        Label::Block(target)
    } else if is_handler {
        Label::HandlerLeave(target, sub_target)
    } else if has_handler {
        Label::ProtectedLeave(target, sub_target)
    } else {
        Label::Block(sub_target)
    }
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
    let mimpl = asm[method].resolved_implementation(asm).clone();
    match &mimpl {
        MethodImpl::MethodBody { .. } | MethodImpl::RegionBody { .. } => {
            let (blocks, locals) = mimpl
                .materialize_legacy_body(asm)
                .expect("managed method body must materialize");
            assemble_method_body(asm, tokens, &blocks, &locals)
        }
        MethodImpl::Extern { .. } => AssembledBody {
            bytes: Vec::new(),
            sequence_points: Vec::new(),
            locals_signature: None,
            locals: Vec::new(),
            code_len: 0,
        },
        MethodImpl::AliasFor(_) => panic!("resolved_implementation returned `AliasFor`"),
        MethodImpl::Missing => {
            let name = asm[asm[method].name()].to_string();
            let mut emitter = new_emitter(asm, tokens);
            let msg = format!("missing methiod {name}");
            emitter.emit_throw_new_exception(&msg);
            finish_body(emitter.out, 3, None, &[], Token::new(0, 0), Vec::new())
        }
    }
}

fn new_emitter<'a>(asm: &'a mut Assembly, tokens: &'a mut dyn TokenSink) -> Emitter<'a> {
    Emitter {
        asm,
        tokens,
        out: Vec::new(),
        label_offsets: HashMap::new(),
        branch_fixups: Vec::new(),
        clauses: Vec::new(),
        marker_counter: 0,
        terminate_region_counter: 0,
        sequence_points: Vec::new(),
    }
}

fn assemble_method_body(
    asm: &mut Assembly,
    tokens: &mut dyn TokenSink,
    blocks: &[BasicBlock],
    locals: &[LocalDef],
) -> AssembledBody {
    let mut emitter = new_emitter(asm, tokens);

    let mut blocks_iter = blocks.iter().peekable();
    while let Some(block) = blocks_iter.next() {
        let try_start = if block.handler().is_some() {
            Some(emitter.here())
        } else {
            None
        };
        emitter.define_label(Label::Block(block.block_id()));
        for root in block.roots() {
            emitter.emit_root(*root, false, block.handler().is_some());
        }
        if let Some(handler) = block.handler() {
            let try_end = emitter.here();
            let handler_start = emitter.here();
            // §I.12.4.2.5: the CLR pushes the caught exception object onto the (otherwise empty)
            // eval stack at the start of every catch handler. `CILNode::GetException` is the only
            // way this IR "names" that object (it emits zero bytes — see `emit_node`'s arm for
            // it) — a handler that never uses it never consumes the object, so it must be popped
            // here or every instruction after it runs with the stack one deeper than the JIT's
            // verifier expects (a corrupted stack depth that surfaces as `InvalidProgramException`
            // at JIT time, not at write time — the bug has no signal until the method actually
            // runs). Mirrors `il_exporter`'s identical conditional-`pop` (mod.rs, `export_method_imp`):
            // "Check for the GetException intrinsic. If it is not used, put a pop here."
            let uses_get_exception = handler.iter().flat_map(BasicBlock::roots).any(|root| {
                crate::ir::CILIter::new(emitter.asm[*root].clone(), emitter.asm)
                    .any(|elem| matches!(elem, crate::ir::CILIterElem::Node(CILNode::GetException)))
            });
            if !uses_get_exception {
                emitter.push_u8(0x26); // pop
            }
            for hblock in handler {
                emitter.define_label(Label::HandlerLeave(block.block_id(), hblock.block_id()));
                for root in hblock.roots() {
                    emitter.emit_root(*root, true, false);
                }
            }
            let handler_end = emitter.here();
            emitter.clauses.push(PendingClause {
                try_start: try_start.unwrap(),
                try_end,
                handler_start,
                handler_end,
            });
        }
    }
    let _ = blocks_iter.peek(); // parity with il_exporter's peekable() usage; no lookahead needed here

    emitter.patch_branches();

    let maxstack = compute_maxstack_from_body(blocks, emitter.asm);
    let locals_types: Vec<Type> = locals.iter().map(|(_, t)| emitter.asm[*t]).collect();
    let locals_tok = if locals_types.is_empty() {
        None
    } else {
        Some(emitter.tokens.locals_sig_token(emitter.asm, &locals_types))
    };

    let has_eh = !emitter.clauses.is_empty();
    let clauses_resolved: Vec<(u32, u32, u32, u32)> = emitter
        .clauses
        .iter()
        .map(|c| {
            (
                emitter.resolve(c.try_start),
                emitter.resolve(c.try_end),
                emitter.resolve(c.handler_start),
                emitter.resolve(c.handler_end),
            )
        })
        .collect();
    let catch_type = if has_eh {
        emitter.system_object_token()
    } else {
        Token::new(0, 0)
    };

    let local_names: Vec<Option<String>> = locals
        .iter()
        .map(|(name, _)| name.map(|n| emitter.asm[n].to_string()))
        .collect();

    let sequence_points = emitter.sequence_points;
    let mut body = finish_body(
        emitter.out,
        maxstack,
        locals_tok,
        &clauses_resolved,
        catch_type,
        sequence_points,
    );
    body.locals_signature = locals_tok;
    body.locals = local_names;
    body
}

/// Writes the fat header (§II.25.4.3) + code + optional fat EH section (§II.25.4.6) into the
/// final buffer. Phase 1a always uses the fat header per the module doc.
fn finish_body(
    code: Vec<u8>,
    maxstack: u32,
    locals_tok: Option<Token>,
    clauses: &[(u32, u32, u32, u32)],
    catch_type: Token,
    sequence_points: Vec<SequencePoint>,
) -> AssembledBody {
    let has_eh = !clauses.is_empty();
    let mut bytes = Vec::new();

    // Fat header, §II.25.4.3: 12 bytes — 2 bytes Flags/Size, 2 bytes MaxStack, 4 bytes CodeSize,
    // 4 bytes LocalVarSigTok. `Size` (header length in 4-byte words, always 3) occupies the top
    // nibble of the first u16; `Flags` occupies the low 12 bits (of which we only ever set
    // FatFormat(0x3)/InitLocals(0x10)/MoreSects(0x8)).
    let mut flags = COR_IL_METHOD_FAT_FORMAT | COR_IL_METHOD_INIT_LOCALS;
    if has_eh {
        flags |= COR_IL_METHOD_MORE_SECTS;
    }
    let flags_and_size = flags | (FAT_HEADER_SIZE_WORDS << 12);
    bytes.extend_from_slice(&flags_and_size.to_le_bytes());
    let maxstack_u16 = u16::try_from(maxstack.min(u32::from(u16::MAX))).unwrap();
    bytes.extend_from_slice(&maxstack_u16.to_le_bytes());
    let code_size = u32::try_from(code.len()).expect("method body code exceeds 4 GiB");
    bytes.extend_from_slice(&code_size.to_le_bytes());
    bytes.extend_from_slice(&locals_tok.map_or(0u32, |t| t.0).to_le_bytes());

    bytes.extend_from_slice(&code);

    if has_eh {
        // §II.25.4.1: the EH section must start on a 4-byte boundary relative to the method body.
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        let clause_count = clauses.len();
        let data_size = 4 + clause_count * 24; // 4-byte section header + 24 bytes/fat clause
        bytes.push(COR_IL_METHOD_SECT_EHTABLE | COR_IL_METHOD_SECT_FAT_FORMAT);
        let data_size_u32 = u32::try_from(data_size).unwrap();
        bytes.push((data_size_u32 & 0xFF) as u8);
        bytes.push(((data_size_u32 >> 8) & 0xFF) as u8);
        bytes.push(((data_size_u32 >> 16) & 0xFF) as u8);
        for (try_start, try_end, handler_start, handler_end) in clauses {
            // Flags = 0 (COR_ILEXCEPTION_CLAUSE_NONE — a plain typed `catch`, §II.25.4.6).
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&try_start.to_le_bytes());
            bytes.extend_from_slice(&(try_end - try_start).to_le_bytes());
            bytes.extend_from_slice(&handler_start.to_le_bytes());
            bytes.extend_from_slice(&(handler_end - handler_start).to_le_bytes());
            bytes.extend_from_slice(&catch_type.0.to_le_bytes());
        }
    }

    // `sequence_points`' `il_offset`s were recorded against `code` (the raw instruction stream,
    // before the fat header was prepended above) — exactly what the Portable PDB spec wants
    // (`MethodDebugInformation`'s sequence points are offsets from the start of the method body's
    // IL, not counting the header; §II.25.4.3's `CodeSize` field has the same "header doesn't
    // count" convention). No offset adjustment needed here.
    AssembledBody {
        bytes,
        sequence_points,
        locals_signature: None,
        locals: Vec::new(),
        code_len: code_size,
    }
}

/// Computes `.maxstack` (§II.25.4.3 `MaxStack` field) for one method body, mirroring
/// `il_exporter::emit_one_method`'s per-root `CILIter::new(root, asm).count() + 10` upper-bound
/// heuristic (a deliberately loose over-approximation, not a tight stack-depth analysis — ilasm
/// accepts a `.maxstack` larger than actually needed, so exactness is not a correctness
/// requirement here, only "large enough").
#[must_use]
pub fn compute_maxstack(asm: &Assembly, method: MethodDefIdx) -> u32 {
    match asm[method].resolved_implementation(asm) {
        MethodImpl::MethodBody { blocks, .. } => compute_maxstack_from_body(blocks, asm),
        MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            ..
        } => u32::try_from(
            blocks
                .iter()
                .chain(cleanup_blocks)
                .flat_map(|block| block.roots())
                .map(|root| crate::CILIter::new(asm[*root].clone(), asm).count() + 10)
                .max()
                .unwrap_or(0),
        )
        .unwrap(),
        MethodImpl::Extern { .. } => 0,
        MethodImpl::AliasFor(_) => panic!("resolved_implementation returned `AliasFor`"),
        MethodImpl::Missing => 3,
    }
}

fn compute_maxstack_from_body(blocks: &[BasicBlock], asm: &Assembly) -> u32 {
    blocks
        .iter()
        .flat_map(BasicBlock::iter_roots)
        .map(|root| crate::ir::CILIter::new(asm[root].clone(), asm).count() as u32 + 10)
        .max()
        .unwrap_or(0)
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
    use crate::ir::cilnode::MethodKind;
    use crate::ir::method::MethodDef;
    use crate::ir::pe_exporter::tables::MetadataBuilder;
    use crate::ir::{Access, BasicBlock as BB, BinOp, ExceptionRegion, FnSig, MethodRef};

    /// A [`TokenSink`] stub for tests: hands out small monotonically increasing tokens per
    /// category and records every request so tests can assert on what was resolved.
    #[derive(Default)]
    struct StubSink {
        next_method_rid: u32,
        next_field_rid: u32,
        next_type_rid: u32,
        requested_methods: Vec<MethodDefIdx>,
        requested_strings: Vec<String>,
    }
    impl TokenSink for StubSink {
        fn method_token(
            &mut self,
            _asm: &mut Assembly,
            method: MethodDefIdx,
            _generics: &[Type],
        ) -> Token {
            self.requested_methods.push(method);
            self.next_method_rid += 1;
            Token::new(Token::TABLE_METHOD_DEF, self.next_method_rid)
        }
        fn field_token(
            &mut self,
            _asm: &mut Assembly,
            _field: Interned<crate::ir::FieldDesc>,
        ) -> Token {
            self.next_field_rid += 1;
            Token::new(Token::TABLE_FIELD, self.next_field_rid)
        }
        fn static_field_token(
            &mut self,
            _asm: &mut Assembly,
            _field: Interned<crate::ir::StaticFieldDesc>,
        ) -> Token {
            self.next_field_rid += 1;
            Token::new(Token::TABLE_FIELD, self.next_field_rid)
        }
        fn user_string_token(&mut self, s: &str) -> Token {
            self.requested_strings.push(s.to_string());
            Token::new(0x70, self.requested_strings.len() as u32)
        }
        fn calli_sig_token(
            &mut self,
            _asm: &mut Assembly,
            _semantic_key: &str,
            _sig_blob: &[u8],
        ) -> Token {
            Token::new(Token::TABLE_STAND_ALONE_SIG, 1)
        }
        fn locals_sig_token(&mut self, _asm: &mut Assembly, _locals: &[Type]) -> Token {
            Token::new(Token::TABLE_STAND_ALONE_SIG, 2)
        }
        fn type_token(&mut self, _asm: &mut Assembly, _tpe: Type) -> Token {
            self.next_type_rid += 1;
            Token::new(Token::TABLE_TYPE_REF, self.next_type_rid)
        }
    }

    fn make_static_method(
        asm: &mut Assembly,
        sig: Interned<FnSig>,
        blocks: Vec<BB>,
        locals: Vec<LocalDef>,
    ) -> MethodDefIdx {
        let main = asm.main_module();
        let name = asm.alloc_string("test_method");
        let def = MethodDef::new(
            Access::Private,
            main,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody { blocks, locals },
            vec![],
        );
        asm.new_method(def)
    }

    #[test]
    fn assembled_body_is_a_plain_byte_buffer() {
        let body = AssembledBody {
            bytes: vec![0u8; 12],
            sequence_points: Vec::new(),
            locals_signature: None,
            locals: Vec::new(),
            code_len: 0,
        };
        assert_eq!(body.bytes.len(), 12);
    }

    #[test]
    fn resolved_operand_wraps_a_token() {
        let op = ResolvedOperand(Token::new(Token::TABLE_METHOD_DEF, 1));
        assert_eq!(op.0.table(), Token::TABLE_METHOD_DEF);
    }

    /// Emits `ldc.i4 <val>` (or a shorter form) for a single constant and returns just the
    /// constant's bytes (fat header + trailing `pop`/`ret` stripped off).
    fn const_bytes(cst: Const) -> Vec<u8> {
        let mut asm = Assembly::default();
        let sig = asm.sig([], Type::Void);
        let node = asm.alloc_node(CILNode::Const(Box::new(cst)));
        let pop = asm.alloc_root(CILRoot::Pop(node));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BB::new(vec![pop, ret], 0, None);
        let method = make_static_method(&mut asm, sig, vec![block], vec![]);
        let mut sink = StubSink::default();
        let body = assemble_method(&mut asm, method, &mut sink);
        // Strip the fat header (12 bytes) and the trailing `pop ret` (2 bytes).
        let code = &body.bytes[12..];
        code[..code.len() - 2].to_vec()
    }

    /// `I8` has NO full-width `ldc.i4` band (every `i8` fits `.s`) — a value `I16`/`I32` would
    /// need the full form for (e.g. `-100`) must still use `.s` when it's an `I8`.
    #[test]
    fn i8_never_uses_the_full_width_form() {
        assert_eq!(
            const_bytes(Const::I8(-100)),
            [0x1F, (-100i8) as u8],
            "ldc.i4.s -100"
        );
        assert_eq!(const_bytes(Const::I8(-1)), [0x15], "ldc.i4.m1");
        assert_eq!(const_bytes(Const::I8(8)), [0x1E], "ldc.i4.8");
    }

    /// `I16`/`I32`, unlike `I8`, DO fall back to the full-width `ldc.i4` form for values outside
    /// `-1`/`0..=8`/`9..=127` — including negatives below `-1` (mirrors `il_exporter`'s `I16`/
    /// `I32` arms, which have no `.s`-for-negatives band the way `I8` does).
    #[test]
    fn i16_and_i32_use_the_full_width_form_for_negatives_below_m1() {
        let expected = {
            let mut v = vec![0x20u8];
            v.extend_from_slice(&(-100i32).to_le_bytes());
            v
        };
        assert_eq!(
            const_bytes(Const::I16(-100)),
            expected,
            "ldc.i4 -100 (full form, not .s)"
        );
        assert_eq!(
            const_bytes(Const::I32(-100)),
            expected,
            "ldc.i4 -100 (full form, not .s)"
        );
    }

    /// `I128` in the `[-2^31, 0) | [128, 2^31)` band uses the FULL `ldc.i4` form even for small
    /// negatives (its own arm shape, distinct from `I8`'s all-`.s` band) followed by
    /// `Int128::op_Implicit(int32)`.
    #[test]
    fn i128_small_negative_uses_full_width_ldc_i4_not_s() {
        let code = const_bytes(Const::I128(-100));
        assert_eq!(code[0], 0x20, "ldc.i4 (full form)");
        let operand = i32::from_le_bytes([code[1], code[2], code[3], code[4]]);
        assert_eq!(operand, -100);
        assert_eq!(code[5], 0x28, "call op_Implicit");
    }

    /// A 128-bit constant too large for any `ldc` + `op_Implicit` band falls back to
    /// `ldc.i8 <high> ldc.i8 <low> newobj instance void UInt128::.ctor(uint64,uint64)`. This is a
    /// regression test for a real bug caught wiring `DIRECT_PE=1` into the linker on
    /// `cargo_tests/cd_collections`: `emit_int128_ctor` built the `.ctor` signature as EXACTLY
    /// `[uint64, uint64] -> void`, but `MetadataBuilder::method_token` (the real `TokenSink`, NOT
    /// `StubSink`) unconditionally treats `inputs()[0]` as an implicit `this` placeholder for any
    /// non-static `MethodKind` (matching the backend call lowering's own `MethodRef::new(...,
    /// MethodKind::Constructor, ...)` call sites, which always prepend the owning class as
    /// `inputs()[0]` — see `src/terminator/call.rs`) and strips it before encoding the
    /// `MethodRefSig` blob. Without a leading `this`-type slot, that strip ate a REAL argument,
    /// encoding a bogus 1-param `.ctor(uint64)` MemberRef — caught at runtime as 164
    /// `dotnet-ilverify` `MissingMethod: Void System.UInt128..ctor(UInt64)` errors and a
    /// `FileLoadException` the moment any JITted method referenced the bogus MemberRef.
    #[test]
    fn u128_ctor_fallback_signature_has_two_params_not_one_after_this_stripping() {
        let mut asm = Assembly::default();
        let sig = asm.sig([], Type::Void);
        // Outside every `ldc` + `op_Implicit` band (needs the full 128 bits): forces the
        // `emit_u128` `_ =>` arm, i.e. the `.ctor(uint64,uint64)` fallback.
        let huge = u128::MAX / 3;
        let node = asm.alloc_node(CILNode::Const(Box::new(Const::U128(huge))));
        let pop = asm.alloc_root(CILRoot::Pop(node));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BB::new(vec![pop, ret], 0, None);
        let method = make_static_method(&mut asm, sig, vec![block], vec![]);

        let mut mb = MetadataBuilder::default();
        let body = assemble_method(&mut asm, method, &mut mb);
        // Fat header (12 bytes) + `ldc.i8 <high>`(9) + `ldc.i8 <low>`(9) + `newobj`(1) + token(4).
        let code = &body.bytes[12..];
        assert_eq!(code[0], 0x21, "ldc.i8 <high>");
        assert_eq!(code[9], 0x21, "ldc.i8 <low>");
        assert_eq!(code[18], 0x73, "newobj");
        let token_bytes = [code[19], code[20], code[21], code[22]];
        let token = Token(u32::from_le_bytes(token_bytes));
        assert_eq!(
            token.table(),
            Token::TABLE_MEMBER_REF,
            "external BCL ctor resolves as a MemberRef"
        );

        // Decode the interned `MethodRefSig` blob (§II.23.2.2) at the row's `Signature` column and
        // assert its declared param count is 2 — the two `uint64`s, NOT 1 (the bug's symptom).
        let row = &mb_member_ref_row(&mb, token);
        let blob = read_blob_at(mb.blobs.as_bytes(), row.signature);
        // byte0 = calling convention (HASTHIS), byte1 = compressed param count.
        assert_eq!(
            blob[0] & 0x20,
            0x20,
            "HASTHIS bit set for an instance/ctor MemberRef"
        );
        assert_eq!(
            blob[1], 2,
            "MethodRefSig ParamCount must be 2 (uint64,uint64), not 1"
        );
    }

    /// Test-only accessor: `MetadataBuilder::member_ref` rows are private to `tables.rs`, but a
    /// `MemberRef` token's row-derived fields are exactly what this regression test needs to
    /// inspect. Re-derives the row by re-running the SAME dedup lookup `member_ref()` uses,
    /// which is safe here because `emit_int128_ctor` is the only call site that could have
    /// produced this exact (class, ".ctor", signature) key in this test's tiny assembly.
    fn mb_member_ref_row(mb: &MetadataBuilder, token: Token) -> TestMemberRefRow {
        assert_eq!(token.table(), Token::TABLE_MEMBER_REF);
        let sig_off = mb.member_ref_signature_for_test(token);
        TestMemberRefRow { signature: sig_off }
    }
    struct TestMemberRefRow {
        signature: u32,
    }

    /// Reads a length-prefixed `#Blob` heap entry (§II.24.2.4) at `offset`, returning just the
    /// blob's payload bytes (the compressed length prefix is consumed, not included).
    fn read_blob_at(heap: &[u8], offset: u32) -> Vec<u8> {
        let start = offset as usize;
        let (len, header_len) = read_compressed_u32(&heap[start..]);
        heap[start + header_len..start + header_len + len as usize].to_vec()
    }
    fn read_compressed_u32(bytes: &[u8]) -> (u32, usize) {
        let b0 = bytes[0];
        if b0 & 0x80 == 0 {
            (u32::from(b0), 1)
        } else if b0 & 0xC0 == 0x80 {
            (u32::from(b0 & 0x3F) << 8 | u32::from(bytes[1]), 2)
        } else {
            (
                u32::from(b0 & 0x1F) << 24
                    | u32::from(bytes[1]) << 16
                    | u32::from(bytes[2]) << 8
                    | u32::from(bytes[3]),
                4,
            )
        }
    }

    /// A `CILRoot::SourceFileInfo` root immediately before `ret` is collected as a `SequencePoint`
    /// at the IL offset of `ret` itself (`SourceFileInfo` emits zero bytes — this is the exact
    /// "seam" this module's doc describes: the source root marks whatever comes next). Also
    /// covers a *second* `SourceFileInfo` between two more instructions, proving offsets track the
    /// running code length (not e.g. always 0) and multiple points from ONE method collect in
    /// visitation order.
    #[test]
    fn source_file_info_roots_become_sequence_points_at_the_next_instructions_offset() {
        let mut asm = Assembly::default();
        let sig = asm.sig([], Type::Int(Int::I32));
        let file = asm.alloc_string("src/main.rs");
        let span1 = asm.alloc_root(CILRoot::SourceFileInfo {
            line_start: 10,
            line_len: 1,
            col_start: 4,
            col_len: 3,
            file,
        });
        let three = asm.alloc_node(CILNode::Const(Box::new(Const::I32(3))));
        let pop = asm.alloc_root(CILRoot::Pop(three));
        let span2 = asm.alloc_root(CILRoot::SourceFileInfo {
            line_start: 11,
            line_len: 2,
            col_start: 0,
            col_len: 5,
            file,
        });
        let four = asm.alloc_node(CILNode::Const(Box::new(Const::I32(4))));
        let ret = asm.alloc_root(CILRoot::Ret(four));
        let block = BB::new(vec![span1, pop, span2, ret], 0, None);
        let method = make_static_method(&mut asm, sig, vec![block], vec![]);

        let mut sink = StubSink::default();
        let body = assemble_method(&mut asm, method, &mut sink);

        assert_eq!(
            body.sequence_points.len(),
            2,
            "one SequencePoint per SourceFileInfo root"
        );
        let p1 = &body.sequence_points[0];
        assert_eq!(
            p1.il_offset, 0,
            "first SourceFileInfo precedes the very first instruction (`ldc.i4.3`)"
        );
        assert_eq!(p1.document_path, "src/main.rs");
        assert_eq!(p1.line, 10);
        assert_eq!(p1.end_line, 11, "line_start + line_len");
        assert_eq!(p1.col, 4);
        assert_eq!(p1.end_col, 7, "col_start + col_len");
        assert!(!p1.is_hidden);

        let p2 = &body.sequence_points[1];
        // `ldc.i4.3`(1) + `pop`(1) = 2 bytes precede the second SourceFileInfo root.
        assert_eq!(
            p2.il_offset, 2,
            "second SourceFileInfo's offset tracks the running code length"
        );
        assert_eq!(p2.line, 11);
        assert_eq!(p2.end_line, 13);
        assert!(
            p2.il_offset > p1.il_offset,
            "offsets strictly increase in visitation order"
        );
    }

    /// `ldc.i4.3 ldc.i4.s 4 add ret` — straight-line arithmetic, fat header, tiny code, no locals.
    #[test]
    fn straight_line_arithmetic_and_ret() {
        let mut asm = Assembly::default();
        let sig = asm.sig([], Type::Int(Int::I32));
        let three = asm.alloc_node(CILNode::Const(Box::new(Const::I32(3))));
        let four = asm.alloc_node(CILNode::Const(Box::new(Const::I32(4))));
        let sum = asm.alloc_node(CILNode::BinOp(three, four, BinOp::Add));
        let ret = asm.alloc_root(CILRoot::Ret(sum));
        let block = BB::new(vec![ret], 0, None);
        let method = make_static_method(&mut asm, sig, vec![block], vec![]);

        let mut sink = StubSink::default();
        let body = assemble_method(&mut asm, method, &mut sink);

        // Fat header (12 bytes) + `ldc.i4.3 ldc.i4.4 add ret` (4 bytes) = 16 bytes, no EH. Both `3`
        // and `4` fall in the `0..=8` short-form band (§III.3.39), so each is a single opcode byte.
        assert_eq!(body.bytes.len(), 12 + 4);
        let flags_and_size = u16::from_le_bytes([body.bytes[0], body.bytes[1]]);
        assert_eq!(
            flags_and_size & 0xFFF,
            COR_IL_METHOD_FAT_FORMAT | COR_IL_METHOD_INIT_LOCALS
        );
        assert_eq!(flags_and_size >> 12, FAT_HEADER_SIZE_WORDS);
        assert_eq!(
            flags_and_size & COR_IL_METHOD_MORE_SECTS,
            0,
            "no EH -> no MoreSects flag"
        );
        let code_size =
            u32::from_le_bytes([body.bytes[4], body.bytes[5], body.bytes[6], body.bytes[7]]);
        assert_eq!(code_size, 4);
        let code = &body.bytes[12..];
        assert_eq!(
            code,
            &[0x19, 0x1A, 0x58, 0x2A],
            "ldc.i4.3(=0x19) ldc.i4.4(=0x1A) add ret"
        );
    }

    /// `bb0: brtrue(false)->bb1 ; br bb0` / `bb1: ret` — verifies the two-pass long-form offset
    /// patch (§II.25.4.4: relative to the byte after the operand).
    #[test]
    fn branch_offsets_are_patched_relative_to_instruction_end() {
        let mut asm = Assembly::default();
        let sig = asm.sig([], Type::Void);
        let cond = asm.alloc_node(CILNode::Const(Box::new(Const::Bool(false))));
        let branch_to_1 = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::True(cond)),
        ))));
        let branch_to_0 = asm.alloc_root(CILRoot::Branch(Box::new((0, 0, None))));
        let bb0 = BB::new(vec![branch_to_1, branch_to_0], 0, None);
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let bb1 = BB::new(vec![ret], 1, None);
        let method = make_static_method(&mut asm, sig, vec![bb0, bb1], vec![]);

        let mut sink = StubSink::default();
        let body = assemble_method(&mut asm, method, &mut sink);
        let code = &body.bytes[12..];

        // ldc.i4.0 (1 byte) ; brtrue (1 + 4 bytes) ; br (1 + 4 bytes) ; ret (1 byte) = 12 bytes.
        assert_eq!(code.len(), 1 + 5 + 5 + 1);
        assert_eq!(code[0], 0x16, "ldc.i4.0");
        assert_eq!(code[1], 0x3A, "brtrue long form");
        let brtrue_operand = i32::from_le_bytes([code[2], code[3], code[4], code[5]]);
        // brtrue's operand ends at byte 6; bb1 starts at byte 11 (1+5+5).
        assert_eq!(brtrue_operand, 11 - 6);
        assert_eq!(code[6], 0x38, "br long form");
        let br_operand = i32::from_le_bytes([code[7], code[8], code[9], code[10]]);
        // br's operand ends at byte 11; bb0's label is defined at block entry, byte 0 (BEFORE its
        // first root's bytes — the leading `ldc.i4.0` for the `brtrue` condition).
        assert_eq!(br_operand, 0 - 11);
    }

    /// `ldstr "hi" call void Foo::Bar(string)` — verifies user-string + method tokens are pulled
    /// through the stub [`TokenSink`] in the right order/shape.
    #[test]
    fn ldstr_and_call_resolve_through_the_token_sink() {
        let mut asm = Assembly::default();
        let target_sig = asm.sig([Type::PlatformString], Type::Void);
        let main = asm.main_module();
        let target_name = asm.alloc_string("Bar");
        let target_def = MethodDef::new(
            Access::Private,
            main,
            target_name,
            target_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![BB::new(vec![asm.alloc_root(CILRoot::VoidRet)], 0, None)],
                locals: vec![],
            },
            vec![None],
        );
        let target = asm.new_method(target_def);
        let target_mref: Interned<MethodRef> = *target;

        let msg = asm.alloc_string("hi");
        let ldstr = asm.alloc_node(CILNode::Const(Box::new(Const::PlatformString(msg))));
        let call = asm.alloc_root(CILRoot::Call(Box::new((
            target_mref,
            vec![ldstr].into(),
            crate::ir::cilnode::IsPure(false),
        ))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let block = BB::new(vec![call, ret], 0, None);
        let caller_sig = asm.sig([], Type::Void);
        let method = make_static_method(&mut asm, caller_sig, vec![block], vec![]);

        let mut sink = StubSink::default();
        let body = assemble_method(&mut asm, method, &mut sink);

        assert_eq!(sink.requested_strings, vec!["hi".to_string()]);
        assert_eq!(sink.requested_methods, vec![target]);
        let code = &body.bytes[12..];
        assert_eq!(code[0], 0x72, "ldstr");
        let str_tok = u32::from_le_bytes([code[1], code[2], code[3], code[4]]);
        assert_eq!(Token(str_tok).table(), 0x70);
        assert_eq!(code[5], 0x28, "call");
        let call_tok = u32::from_le_bytes([code[6], code[7], code[8], code[9]]);
        assert_eq!(Token(call_tok).table(), Token::TABLE_METHOD_DEF);
        assert_eq!(code[10], 0x2A, "ret");
    }

    /// `bb0 { nop } catch System.Object { pop; leave bb1 } / bb1 { ret }` — verifies the fat EH
    /// clause records correct try/handler byte offsets and the `MoreSects` header flag is set.
    #[test]
    fn try_catch_produces_a_fat_eh_clause_with_correct_offsets() {
        let mut asm = Assembly::default();
        let sig = asm.sig([], Type::Void);

        let nop = asm.alloc_root(CILRoot::Nop);
        let get_exc = asm.alloc_node(CILNode::GetException);
        let pop_root = asm.alloc_root(CILRoot::Pop(get_exc));
        let leave = asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 1,
            source: 0,
        });
        let handler_block = BB::new(vec![pop_root, leave], 0, None);
        let bb0 = BB::new(vec![nop], 0, Some(vec![handler_block]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let bb1 = BB::new(vec![ret], 1, None);

        let method = make_static_method(&mut asm, sig, vec![bb0, bb1], vec![]);
        let mut sink = StubSink::default();
        let body = assemble_method(&mut asm, method, &mut sink);

        let flags_and_size = u16::from_le_bytes([body.bytes[0], body.bytes[1]]);
        assert_ne!(
            flags_and_size & COR_IL_METHOD_MORE_SECTS,
            0,
            "a handler must set CorILMethod_MoreSects"
        );

        let code_size =
            u32::from_le_bytes([body.bytes[4], body.bytes[5], body.bytes[6], body.bytes[7]])
                as usize;
        let mut eh_off = 12 + code_size;
        while eh_off % 4 != 0 {
            eh_off += 1;
        }
        assert_eq!(
            body.bytes[eh_off] & (COR_IL_METHOD_SECT_EHTABLE | COR_IL_METHOD_SECT_FAT_FORMAT),
            COR_IL_METHOD_SECT_EHTABLE | COR_IL_METHOD_SECT_FAT_FORMAT
        );
        let data_size = u32::from_le_bytes([
            body.bytes[eh_off + 1],
            body.bytes[eh_off + 2],
            body.bytes[eh_off + 3],
            0,
        ]);
        assert_eq!(
            data_size,
            4 + 24,
            "one clause: 4-byte section header + 24-byte fat clause"
        );

        let clause_off = eh_off + 4;
        let clause_flags =
            u32::from_le_bytes(body.bytes[clause_off..clause_off + 4].try_into().unwrap());
        assert_eq!(clause_flags, 0, "a plain typed catch has flags = 0");
        let try_off = u32::from_le_bytes(
            body.bytes[clause_off + 4..clause_off + 8]
                .try_into()
                .unwrap(),
        );
        let try_len = u32::from_le_bytes(
            body.bytes[clause_off + 8..clause_off + 12]
                .try_into()
                .unwrap(),
        );
        let handler_off = u32::from_le_bytes(
            body.bytes[clause_off + 12..clause_off + 16]
                .try_into()
                .unwrap(),
        );
        let handler_len = u32::from_le_bytes(
            body.bytes[clause_off + 16..clause_off + 20]
                .try_into()
                .unwrap(),
        );
        // Protected region: one `nop` (1 byte) at code offset 0.
        assert_eq!(try_off, 0);
        assert_eq!(try_len, 1);
        // Handler starts right after the try region.
        assert_eq!(handler_off, try_len);
        assert!(
            handler_len > 0,
            "handler emits pop + leave, so its length must be non-zero"
        );
        let catch_tok = u32::from_le_bytes(
            body.bytes[clause_off + 20..clause_off + 24]
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            Token(catch_tok).table(),
            Token::TABLE_TYPE_REF,
            "System.Object via the stub sink"
        );
    }

    #[test]
    fn region_body_assembles_to_the_exact_legacy_pe_method_body() {
        let mut asm = Assembly::default();
        let sig = asm.sig([], Type::Void);
        let to_next = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let rethrow = asm.alloc_root(CILRoot::ReThrow);
        let cleanup = vec![BB::new(vec![rethrow], 10, None)];

        let mut legacy_protected = BB::new_raw(vec![to_next], 0, Some(10));
        legacy_protected.resolve_exception_handlers(&cleanup, &mut asm);
        let owner = asm.main_module();
        let legacy_name = asm.alloc_string("legacy_region_compat");
        let legacy = asm.new_method(MethodDef::new(
            Access::Private,
            owner,
            legacy_name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![legacy_protected, BB::new(vec![ret], 1, None)],
                locals: vec![],
            },
            vec![],
        ));
        let canonical_name = asm.alloc_string("canonical_region_compat");
        let canonical = asm.new_method(MethodDef::new(
            Access::Private,
            owner,
            canonical_name,
            sig,
            MethodKind::Static,
            MethodImpl::RegionBody {
                blocks: vec![BB::new(vec![to_next], 0, None), BB::new(vec![ret], 1, None)],
                cleanup_blocks: cleanup,
                exception_regions: vec![ExceptionRegion::new(0, 10)],
                locals: vec![],
            },
            vec![],
        ));

        let mut legacy_sink = StubSink::default();
        let legacy_body = assemble_method(&mut asm, legacy, &mut legacy_sink);
        let mut canonical_sink = StubSink::default();
        let canonical_body = assemble_method(&mut asm, canonical, &mut canonical_sink);
        assert_eq!(canonical_body.bytes, legacy_body.bytes);
        assert_eq!(canonical_body.code_len, legacy_body.code_len);
    }

    /// Regression for the `InvalidProgramException` on `main()` bisected from cd_collections
    /// down to a minimal `Vec::push` + `println!` + `if` program: a catch handler that never
    /// references `CILNode::GetException` (the common case — most Rust panic/unwind cleanup
    /// handlers just `leave` without inspecting the exception object) must still have the CLR's
    /// implicitly-pushed exception object popped off the stack (§I.12.4.2.5), or every
    /// instruction after the handler runs with the eval stack one slot deeper than the JIT's
    /// verifier expects — surfacing as `InvalidProgramException` at JIT time (not at write time,
    /// so it has no signal until the method actually runs). `il_exporter`'s
    /// `export_method_imp` has the identical conditional (mod.rs: "Check for the GetException
    /// intrinsic. If it is not used, put a pop here."); `assemble_method_body`'s handler-emission
    /// loop originally omitted it unconditionally. `bb0 { nop } catch System.Object { leave bb1 }`
    /// (no `GetException`/`Pop` in the handler at all) must assemble a leading `pop` opcode.
    #[test]
    fn handler_without_get_exception_gets_an_implicit_pop() {
        let mut asm = Assembly::default();
        let sig = asm.sig([], Type::Void);

        let nop = asm.alloc_root(CILRoot::Nop);
        let leave = asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 1,
            source: 0,
        });
        // No `GetException`/`Pop` anywhere in the handler — this is the common shape (a cleanup
        // handler that just unwinds further, ignoring the caught exception's value).
        let handler_block = BB::new(vec![leave], 0, None);
        let bb0 = BB::new(vec![nop], 0, Some(vec![handler_block]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let bb1 = BB::new(vec![ret], 1, None);

        let method = make_static_method(&mut asm, sig, vec![bb0, bb1], vec![]);
        let mut sink = StubSink::default();
        let body = assemble_method(&mut asm, method, &mut sink);

        let code_size =
            u32::from_le_bytes([body.bytes[4], body.bytes[5], body.bytes[6], body.bytes[7]])
                as usize;
        let code = &body.bytes[12..12 + code_size];
        // Protected region: one `nop` (1 byte) at code offset 0. The handler starts at offset 1
        // and — since the handler never names the caught exception — must open with `pop` (0x26)
        // before its own `leave` (0xDD).
        assert_eq!(code[0], 0x00, "nop");
        assert_eq!(
            code[1], 0x26,
            "a catch handler that never uses GetException must open with an implicit pop \
             (§I.12.4.2.5), or the eval stack is left one slot too deep for the rest of the method"
        );
        assert_eq!(code[2], 0xDD, "leave");
    }

    #[test]
    fn handler_exception_is_spilled_across_nested_leave_in_direct_pe() {
        let mut asm = Assembly::default();
        let sig = asm.sig([], Type::Void);
        let protected_nop = asm.alloc_root(CILRoot::Nop);
        let inner_nop = asm.alloc_root(CILRoot::Nop);
        let terminate = asm.alloc_root(CILRoot::TerminateRegion {
            protected: inner_nop,
            reason: 1,
        });
        let to_use = asm.alloc_root(CILRoot::Branch(Box::new((0, 2, None))));
        let get_exception = asm.alloc_node(CILNode::GetException);
        let consume = asm.alloc_root(CILRoot::Pop(get_exception));
        let leave = asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 1,
            source: 0,
        });
        let handler = vec![
            BB::new(vec![terminate, to_use], 3, None),
            BB::new(vec![consume, leave], 2, None),
        ];
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let method = make_static_method(
            &mut asm,
            sig,
            vec![
                BB::new(vec![protected_nop], 0, Some(handler)),
                BB::new(vec![ret], 1, None),
            ],
            vec![],
        );

        let mut sink = StubSink::default();
        let body = assemble_method(&mut asm, method, &mut sink);
        assert_eq!(
            body.locals.len(),
            1,
            "the hidden exception local must be emitted"
        );

        let code_size = u32::from_le_bytes(body.bytes[4..8].try_into().unwrap()) as usize;
        let code = &body.bytes[12..12 + code_size];
        assert_eq!(code[0], 0x00, "protected nop");
        assert_eq!(
            code[1], 0x0A,
            "outer handler must immediately stloc.0 the implicit exception"
        );
        let nested_leave = code[2..]
            .iter()
            .position(|byte| *byte == 0xDD)
            .map(|offset| offset + 2)
            .expect("TerminateRegion must emit a nested leave");
        let reload = code
            .windows(3)
            .position(|window| window == [0x06, 0x26, 0xDD])
            .expect("later handler block must ldloc.0, pop, then leave");
        assert!(
            nested_leave < reload,
            "the exception reload must occur after the nested leave emptied the eval stack"
        );
    }
}
