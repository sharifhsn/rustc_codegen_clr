use crate::{
    BasicBlock, BinOp, CILNode, CILRoot, ClassRef, Const, Int, MethodImpl, MethodRef, Type,
    asm::MissingMethodPatcher,
    bimap::Interned,
    cilnode::{ExtendKind, MethodKind},
    cilroot::BranchCond,
};

use super::{
    super::Assembly,
    math::{int_max, int_min},
};
/// Emits a sub-word (`u8`/`i8`/`u16`/`i16`) atomic exchange, named `atomic_xchng{8,16}_correct`, as a
/// masked 32-bit `Interlocked.CompareExchange` loop that unconditionally splices the new sub-word and
/// retries until the full word swaps. Unlike a plain volatile load/store, this is genuinely atomic
/// against concurrent writers to the SAME word. Returns the old sub-word.
///
/// Signature: `int_ty atomic_xchng{8,16}_correct(int_ty& addr, int_ty new)`.
/// Same LE-only + page-boundary caveats as [`emulate_subword_cmp_xchng`].
pub fn emulate_subword_xchng(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, width: u8) {
    debug_assert!(
        width == 1 || width == 2,
        "sub-word xchg width must be 1 or 2"
    );
    let name = asm.alloc_string(format!("atomic_xchng{}_correct", width * 8));
    let full_mask: u32 = if width == 1 { 0xFF } else { 0xFFFF };
    let generator = move |method: Interned<MethodRef>, asm: &mut Assembly| {
        let signature = asm[method].sig();
        let return_int = match *asm[signature].output() {
            Type::Int(int) if int.size() == Some(width.into()) => int,
            ref output => panic!(
                "atomic_xchng{}_correct has incompatible return type {output:?}",
                width * 8
            ),
        };
        // locals: 0 = word_addr (i32*), 1 = shift (i32), 2 = observed_word (i32), 3 = prev (i32)
        let i32_t = asm.alloc_type(Type::Int(Int::I32));
        // Loc 0 is the `int32&` argument of `Interlocked.CompareExchange` — declare it as a
        // pointer (`int32*`), not `int32`, or the JIT rejects the call (`InvalidProgramException`,
        // StackUnexpected). See the matching note in `emulate_subword_cmp_xchng`.
        let i32_ptr_t = asm.alloc_type(Type::Ptr(i32_t));
        // --- bb0: containing-word address + sub-word bit shift. ---
        let addr_ref = asm.alloc_node(CILNode::LdArg(0));
        let addr_ptr = asm.alloc_node(CILNode::RefToPtr(addr_ref));
        let addr_int = asm.alloc_node(CILNode::PtrCast(
            addr_ptr,
            Box::new(crate::cilnode::PtrCastRes::USize),
        ));
        let three = asm.alloc_node(Const::USize(3));
        let not_three = asm.alloc_node(Const::USize(!3u64));
        let word_addr_int = asm.alloc_node(CILNode::BinOp(addr_int, not_three, BinOp::And));
        let word_addr = asm.alloc_node(CILNode::PtrCast(
            word_addr_int,
            Box::new(crate::cilnode::PtrCastRes::Ptr(i32_t)),
        ));
        let byte_off = asm.alloc_node(CILNode::BinOp(addr_int, three, BinOp::And));
        let eight = asm.alloc_node(Const::USize(8));
        let shift_usize = asm.alloc_node(CILNode::BinOp(byte_off, eight, BinOp::Mul));
        let shift = asm.alloc_node(CILNode::IntCast {
            input: shift_usize,
            target: Int::I32,
            extend: ExtendKind::ZeroExtend,
        });
        let bb0 = vec![
            asm.alloc_root(CILRoot::StLoc(0, word_addr)),
            asm.alloc_root(CILRoot::StLoc(1, shift)),
            asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None)))),
        ];
        // --- bb1: read word, splice new sub-word, CAS, retry on contention. ---
        let ld_word_addr = asm.alloc_node(CILNode::LdLoc(0));
        let observed_word = asm.alloc_node(CILNode::LdInd {
            addr: ld_word_addr,
            tpe: i32_t,
            volatile: true,
        });
        let ld_shift = asm.alloc_node(CILNode::LdLoc(1));
        let mask_node = asm.alloc_node(Const::I32(full_mask as i32));
        let mask_at_shift = asm.alloc_node(CILNode::BinOp(mask_node, ld_shift, BinOp::Shl));
        let neg_one = asm.alloc_node(Const::I32(-1));
        let clear_mask = asm.alloc_node(CILNode::BinOp(mask_at_shift, neg_one, BinOp::XOr));
        let ld_observed_word = asm.alloc_node(CILNode::LdLoc(2));
        let cleared = asm.alloc_node(CILNode::BinOp(ld_observed_word, clear_mask, BinOp::And));
        let ld_new = asm.alloc_node(CILNode::LdArg(1));
        let new_i32 = asm.alloc_node(CILNode::IntCast {
            input: ld_new,
            target: Int::I32,
            extend: ExtendKind::ZeroExtend,
        });
        let new_masked = asm.alloc_node(CILNode::BinOp(new_i32, mask_node, BinOp::And));
        let ld_shift2 = asm.alloc_node(CILNode::LdLoc(1));
        let new_at_shift = asm.alloc_node(CILNode::BinOp(new_masked, ld_shift2, BinOp::Shl));
        let new_word = asm.alloc_node(CILNode::BinOp(cleared, new_at_shift, BinOp::Or));
        let ld_word_addr2 = asm.alloc_node(CILNode::LdLoc(0));
        let ld_observed_word2 = asm.alloc_node(CILNode::LdLoc(2));
        let cmpxchng = asm.alloc_string("CompareExchange");
        let i32_ref = asm.nref(Type::Int(Int::I32));
        let cmpxchng_sig = asm.sig(
            [i32_ref, Type::Int(Int::I32), Type::Int(Int::I32)],
            Type::Int(Int::I32),
        );
        let interlocked = ClassRef::interlocked(asm);
        let cmpxchng = asm.alloc_methodref(MethodRef::new(
            interlocked,
            cmpxchng,
            cmpxchng_sig,
            MethodKind::Static,
            vec![].into(),
        ));
        let prev = asm.alloc_node(CILNode::call(
            cmpxchng,
            [ld_word_addr2, new_word, ld_observed_word2],
        ));
        let ld_prev = asm.alloc_node(CILNode::LdLoc(3));
        let ld_observed_word3 = asm.alloc_node(CILNode::LdLoc(2));
        let bb1 = vec![
            asm.alloc_root(CILRoot::StLoc(2, observed_word)),
            asm.alloc_root(CILRoot::StLoc(3, prev)),
            // if CAS observed a different word, some byte changed under us -> retry bb1.
            asm.alloc_root(CILRoot::Branch(Box::new((
                0,
                1,
                Some(BranchCond::Ne(ld_prev, ld_observed_word3)),
            )))),
            asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None)))),
        ];
        // --- bb2: return the old sub-word, extracted from the word we swapped out. ---
        let ld_old_word = asm.alloc_node(CILNode::LdLoc(3));
        let ld_shift3 = asm.alloc_node(CILNode::LdLoc(1));
        let mask_node2 = asm.alloc_node(Const::I32(full_mask as i32));
        let old_shifted = asm.alloc_node(CILNode::BinOp(ld_old_word, ld_shift3, BinOp::ShrUn));
        let old_sub = asm.alloc_node(CILNode::BinOp(old_shifted, mask_node2, BinOp::And));
        let old_sub = asm.alloc_node(CILNode::IntCast {
            input: old_sub,
            target: return_int,
            extend: ExtendKind::ZeroExtend,
        });
        let bb2 = vec![asm.alloc_root(CILRoot::Ret(old_sub))];
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(bb0, 0, None),
                BasicBlock::new(bb1, 1, None),
                BasicBlock::new(bb2, 2, None),
            ],
            locals: vec![
                (None, i32_ptr_t),
                (None, i32_t),
                (None, i32_t),
                (None, i32_t),
            ],
        }
    };
    patcher.insert(name, Box::new(generator));
}
/// Emits a CORRECT sub-word (`u8`/`i8`/`u16`/`i16`) atomic compare-exchange, named
/// `atomic_cmpxchng{8,16}_correct`, by emulating it with a masked 32-bit `Interlocked.CompareExchange`
/// loop. Unlike the loop-internal `cmpxchng{8,16}` builtins (which unconditionally splice the new
/// sub-word and so write on mismatch — fine inside a re-reading RMW loop, but WRONG as Rust's
/// `compare_exchange`), this checks the observed sub-word against the comparand *before* writing:
///   * if the observed sub-word != comparand, it returns the observed sub-word WITHOUT writing
///     (Rust's no-write-on-failure contract);
///   * otherwise it splices the new sub-word into the containing word and CASes the full word,
///     retrying only on contention from the OTHER bytes of that word.
/// In all cases it returns the genuine old sub-word, so the caller's `old == expected` check is exact.
///
/// Signature: `int_ty atomic_cmpxchng{8,16}_correct(int_ty& addr, int_ty comparand, int_ty new)`.
///
/// CAVEATS (inherent to the word-CAS strategy, and matching the existing emulation):
/// * Little-endian only (the legacy Unity ABI is supported only on the project's 64-bit LE hosts):
///   the sub-word byte lives in the LOW bits of the containing word at `(addr & 3) * 8`.
/// * Page-boundary hazard: the address is aligned DOWN to its containing 32-bit word, so up to 3
///   bytes before the target byte are touched. A naturally-aligned `u8`/`u16` atomic is always
///   contained within one word (Rust requires natural alignment), so the aligned-down word stays in
///   the same allocation in practice — but the general caveat stands. Public .NET 10 artifacts do
///   not register this isolated legacy fallback.
pub fn emulate_subword_cmp_xchng(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    width: u8,
) {
    debug_assert!(
        width == 1 || width == 2,
        "sub-word CAS width must be 1 or 2"
    );
    let name = asm.alloc_string(format!("atomic_cmpxchng{}_correct", width * 8));
    let full_mask: u32 = if width == 1 { 0xFF } else { 0xFFFF };
    let generator = move |method: Interned<MethodRef>, asm: &mut Assembly| {
        let signature = asm[method].sig();
        let return_int = match *asm[signature].output() {
            Type::Int(int) if int.size() == Some(width.into()) => int,
            ref output => panic!(
                "atomic_cmpxchng{}_correct has incompatible return type {output:?}",
                width * 8
            ),
        };
        // locals: 0 = word_addr (i32*), 1 = shift (i32), 2 = observed_word (i32), 3 = observed_sub (i32)
        let i32_t = asm.alloc_type(Type::Int(Int::I32));
        // Loc 0 holds the containing-word ADDRESS and is passed as the `int32&` argument of
        // `Interlocked.CompareExchange(int32&,int32,int32)`. It MUST be declared as a pointer
        // (`int32*`), not `int32`: a plain-`int32` local loaded onto the stack is NOT a
        // managed/unmanaged pointer, so the JIT rejects the call with
        // `InvalidProgramException` (StackUnexpected: int32 where int32& expected). With the
        // local typed `int32*`, `ldloc.0` yields a pointer the runtime accepts for `int32&`.
        let i32_ptr_t = asm.alloc_type(Type::Ptr(i32_t));
        // --- bb0: compute the containing-word address and the sub-word bit shift. ---
        let addr_ref = asm.alloc_node(CILNode::LdArg(0));
        let addr_ptr = asm.alloc_node(CILNode::RefToPtr(addr_ref));
        let addr_int = asm.alloc_node(CILNode::PtrCast(
            addr_ptr,
            Box::new(crate::cilnode::PtrCastRes::USize),
        ));
        // word_addr = (i32*)(addr & ~3)
        let three = asm.alloc_node(Const::USize(3));
        let not_three = asm.alloc_node(Const::USize(!3u64));
        let word_addr_int = asm.alloc_node(CILNode::BinOp(addr_int, not_three, BinOp::And));
        let word_addr = asm.alloc_node(CILNode::PtrCast(
            word_addr_int,
            Box::new(crate::cilnode::PtrCastRes::Ptr(i32_t)),
        ));
        // shift = (i32)((addr & 3) * 8)
        let byte_off = asm.alloc_node(CILNode::BinOp(addr_int, three, BinOp::And));
        let eight = asm.alloc_node(Const::USize(8));
        let shift_usize = asm.alloc_node(CILNode::BinOp(byte_off, eight, BinOp::Mul));
        let shift = asm.alloc_node(CILNode::IntCast {
            input: shift_usize,
            target: Int::I32,
            extend: ExtendKind::ZeroExtend,
        });
        let bb0 = vec![
            asm.alloc_root(CILRoot::StLoc(0, word_addr)),
            asm.alloc_root(CILRoot::StLoc(1, shift)),
            asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None)))),
        ];
        // --- bb1: read the word, extract the observed sub-word, bail to bb3 if it != comparand. ---
        let ld_word_addr = asm.alloc_node(CILNode::LdLoc(0));
        let observed_word = asm.alloc_node(CILNode::LdInd {
            addr: ld_word_addr,
            tpe: i32_t,
            volatile: true,
        });
        let ld_shift = asm.alloc_node(CILNode::LdLoc(1));
        // observed_sub = (word >> shift) & full_mask, then zero-extended into the int_ty value space
        let shifted = asm.alloc_node(CILNode::BinOp(observed_word, ld_shift, BinOp::ShrUn));
        let mask_node = asm.alloc_node(Const::I32(full_mask as i32));
        let observed_sub = asm.alloc_node(CILNode::BinOp(shifted, mask_node, BinOp::And));
        // comparand, masked to the sub-word width so a sign-extended negative arg compares correctly.
        let ld_comparand = asm.alloc_node(CILNode::LdArg(1));
        let comparand_i32 = asm.alloc_node(CILNode::IntCast {
            input: ld_comparand,
            target: Int::I32,
            extend: ExtendKind::ZeroExtend,
        });
        let comparand_sub = asm.alloc_node(CILNode::BinOp(comparand_i32, mask_node, BinOp::And));
        let ld_observed_sub = asm.alloc_node(CILNode::LdLoc(3));
        let bb1 = vec![
            asm.alloc_root(CILRoot::StLoc(2, observed_word)),
            asm.alloc_root(CILRoot::StLoc(3, observed_sub)),
            // if observed_sub != comparand -> bb3 (return observed, NO write)
            asm.alloc_root(CILRoot::Branch(Box::new((
                0,
                3,
                Some(BranchCond::Ne(ld_observed_sub, comparand_sub)),
            )))),
            // else fall through to bb2 (attempt the CAS)
            asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None)))),
        ];
        // --- bb2: splice the new sub-word into the word and CAS; retry bb1 only on other-byte contention. ---
        let ld_word_addr2 = asm.alloc_node(CILNode::LdLoc(0));
        let ld_observed_word = asm.alloc_node(CILNode::LdLoc(2));
        let ld_shift2 = asm.alloc_node(CILNode::LdLoc(1));
        // clear the target sub-word: word & ~(full_mask << shift)
        let mask_node2 = asm.alloc_node(Const::I32(full_mask as i32));
        let mask_at_shift = asm.alloc_node(CILNode::BinOp(mask_node2, ld_shift2, BinOp::Shl));
        let neg_one = asm.alloc_node(Const::I32(-1));
        let clear_mask = asm.alloc_node(CILNode::BinOp(mask_at_shift, neg_one, BinOp::XOr));
        let cleared = asm.alloc_node(CILNode::BinOp(ld_observed_word, clear_mask, BinOp::And));
        // place the new sub-word: (new & full_mask) << shift
        let ld_new = asm.alloc_node(CILNode::LdArg(2));
        let new_i32 = asm.alloc_node(CILNode::IntCast {
            input: ld_new,
            target: Int::I32,
            extend: ExtendKind::ZeroExtend,
        });
        let new_masked = asm.alloc_node(CILNode::BinOp(new_i32, mask_node2, BinOp::And));
        let ld_shift3 = asm.alloc_node(CILNode::LdLoc(1));
        let new_at_shift = asm.alloc_node(CILNode::BinOp(new_masked, ld_shift3, BinOp::Shl));
        let new_word = asm.alloc_node(CILNode::BinOp(cleared, new_at_shift, BinOp::Or));
        // prev = Interlocked.CompareExchange(word_addr, new_word, observed_word)
        let cmpxchng = asm.alloc_string("CompareExchange");
        let i32_ref = asm.nref(Type::Int(Int::I32));
        let cmpxchng_sig = asm.sig(
            [i32_ref, Type::Int(Int::I32), Type::Int(Int::I32)],
            Type::Int(Int::I32),
        );
        let interlocked = ClassRef::interlocked(asm);
        let cmpxchng = asm.alloc_methodref(MethodRef::new(
            interlocked,
            cmpxchng,
            cmpxchng_sig,
            MethodKind::Static,
            vec![].into(),
        ));
        let prev = asm.alloc_node(CILNode::call(
            cmpxchng,
            [ld_word_addr2, new_word, ld_observed_word],
        ));
        // Store the CAS result once into loc 4; referencing the call node twice would re-emit the
        // (side-effecting) CompareExchange.
        let ld_prev = asm.alloc_node(CILNode::LdLoc(4));
        let ld_observed_word2 = asm.alloc_node(CILNode::LdLoc(2));
        let bb2 = vec![
            asm.alloc_root(CILRoot::StLoc(4, prev)),
            // CompareExchange returns the value it observed; if it differs from the word we read,
            // some OTHER byte changed under us -> retry the whole load/compare from bb1.
            asm.alloc_root(CILRoot::Branch(Box::new((
                0,
                1,
                Some(BranchCond::Ne(ld_prev, ld_observed_word2)),
            )))),
            // success: the target sub-word == comparand and the word was swapped -> bb3.
            asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None)))),
        ];
        // --- bb3: return the genuine old sub-word (==comparand on success, observed on failure). ---
        let ret_sub = asm.alloc_node(CILNode::LdLoc(3));
        let ret_sub = asm.alloc_node(CILNode::IntCast {
            input: ret_sub,
            target: return_int,
            extend: ExtendKind::ZeroExtend,
        });
        let bb3 = vec![asm.alloc_root(CILRoot::Ret(ret_sub))];
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(bb0, 0, None),
                BasicBlock::new(bb1, 1, None),
                BasicBlock::new(bb2, 2, None),
                BasicBlock::new(bb3, 3, None),
            ],
            locals: vec![
                (None, i32_ptr_t),
                (None, i32_t),
                (None, i32_t),
                (None, i32_t),
                (None, i32_t),
            ],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn compare_exchange(
    asm: &mut Assembly,
    int: Int,
    addr: Interned<CILNode>,
    value: Interned<CILNode>,
    comaprand: Interned<CILNode>,
    native_subword: bool,
) -> Interned<CILNode> {
    match int.size().unwrap_or(8) {
        1 | 2 if native_subword => {
            let compare_exchange = asm.alloc_string("CompareExchange");
            let tpe = Type::Int(int);
            let tref = asm.nref(tpe);
            let cmpxchng_sig = asm.sig([tref, tpe, tpe], tpe);
            let interlocked = ClassRef::interlocked(asm);
            let mref = asm.alloc_methodref(MethodRef::new(
                interlocked,
                compare_exchange,
                cmpxchng_sig,
                MethodKind::Static,
                vec![].into(),
            ));
            asm.alloc_node(CILNode::call(mref, [addr, value, comaprand]))
        }
        // Sub-word (u8/i8/u16/i16) CAS via the COMPARAND-CHECKED `_correct` builtin. The old path
        // called `atomic_cmpxchng{8,16}_i32`, which splices the new sub-word UNCONDITIONALLY — it
        // never reads the comparand, so it is an atomic *exchange*, not a CAS. As the inner step of
        // the re-reading RMW loop in `generate_atomic` an unconditional exchange writes every
        // iteration, so the value oscillates and `loc0 != loc1` never clears: e.g.
        // `AtomicBool::fetch_or(false)` / `AtomicU8::fetch_*` on any nonzero atom spins FOREVER
        // (the coretests `atomic::atomic_access_bool` hang). `_correct` only writes when the
        // observed sub-word equals the comparand (returning the genuine old value otherwise), so
        // the loop converges. Same builtin + signature + (addr, comparand, new) arg order that
        // `intrinsics::atomic::cxchg` already uses for Rust's `compare_exchange`. `addr` stays the
        // `int_ty&` argument as-is — the builtin word-aligns and masks internally (no `int32*`
        // pre-cast); `value`/`comparand` are already `int_ty`. Also fixes the long-standing u16 TODO.
        1 | 2 => {
            let cmpxchng = asm.alloc_string(format!(
                "atomic_cmpxchng{}_correct",
                int.size().unwrap_or(8) * 8
            ));
            let int_ty = Type::Int(int);
            let int_ref = asm.nref(int_ty);
            let cmpxchng_sig = asm.sig([int_ref, int_ty, int_ty], int_ty);
            let main_mod = asm.main_module();
            let mref = asm.alloc_methodref(MethodRef::new(
                *main_mod,
                cmpxchng,
                cmpxchng_sig,
                MethodKind::Static,
                vec![].into(),
            ));
            asm.alloc_node(CILNode::call(mref, [addr, comaprand, value]))
        }
        4..=8 => {
            let compare_exchange = asm.alloc_string("CompareExchange");

            let tpe = Type::Int(int);
            let tref = asm.nref(tpe);
            let cmpxchng_sig = asm.sig([tref, tpe, tpe], tpe);
            let interlocked = ClassRef::interlocked(asm);
            let mref = asm.alloc_methodref(MethodRef::new(
                interlocked,
                compare_exchange,
                cmpxchng_sig,
                MethodKind::Static,
                vec![].into(),
            ));

            asm.alloc_node(CILNode::call(mref, [addr, value, comaprand]))
        }
        _ => todo!("Can't cmpxchng {int:?}"),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtomicRmwOp {
    Add,
    Sub,
    Or,
    Xor,
    And,
    Nand,
    Min,
    Max,
}

impl AtomicRmwOp {
    const ALL: [Self; 8] = [
        Self::Add,
        Self::Sub,
        Self::Or,
        Self::Xor,
        Self::And,
        Self::Nand,
        Self::Min,
        Self::Max,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::And => "and",
            Self::Nand => "nand",
            Self::Min => "min",
            Self::Max => "max",
        }
    }

    fn apply(
        self,
        asm: &mut Assembly,
        lhs: Interned<CILNode>,
        rhs: Interned<CILNode>,
        int: Int,
    ) -> Interned<CILNode> {
        match self {
            Self::Add => asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::Add)),
            Self::Sub => asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::Sub)),
            Self::Or => asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::Or)),
            Self::Xor => asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::XOr)),
            Self::And => asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::And)),
            Self::Nand => {
                let and = asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::And));
                asm.alloc_node(CILNode::UnOp(and, crate::cilnode::UnOp::Not))
            }
            Self::Min => int_min(asm, lhs, rhs, int),
            Self::Max => int_max(asm, lhs, rhs, int),
        }
    }
}

/// Integer widths supported by every generated atomic RMW operation.
const ATOMIC_INTS: [Int; 10] = [
    Int::U8,
    Int::I8,
    Int::U16,
    Int::I16,
    Int::U32,
    Int::I32,
    Int::U64,
    Int::I64,
    Int::USize,
    Int::ISize,
];

type AsmGen = dyn Fn(&mut Assembly, Interned<CILNode>, Interned<CILNode>, Int) -> Interned<CILNode>;

fn generate_atomic_impl(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    op_name: &str,
    op: Box<AsmGen>,
    int: Int,
    native_subword: bool,
) {
    let name = asm.alloc_string(format!("atomic_{op_name}_{int}", int = int.name()));
    let generator = move |_, asm: &mut Assembly| {
        // Common ops
        let ldloc_0 = asm.alloc_node(CILNode::LdLoc(0));
        let ldloc_1 = asm.alloc_node(CILNode::LdLoc(1));
        let ldarg_0 = asm.alloc_node(CILNode::LdArg(0));
        let ldarg_1 = asm.alloc_node(CILNode::LdArg(1));
        // Types for which this atomic is implemented

        // The OP of this atomic
        let value = op(asm, ldloc_0, ldarg_1, int);
        let call = compare_exchange(asm, int, ldarg_0, value, ldloc_0, native_subword);

        let tpe = Type::Int(int);
        let zero = asm.alloc_node(int.zero());
        let entry_block = vec![
            asm.alloc_root(CILRoot::StLoc(1, zero)),
            asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None)))),
        ];
        let loop_block = vec![
            asm.alloc_root(CILRoot::StLoc(0, ldloc_1)),
            asm.alloc_root(CILRoot::StLoc(1, call)),
            asm.alloc_root(CILRoot::Branch(Box::new((
                0,
                1,
                Some(BranchCond::Ne(ldloc_0, ldloc_1)),
            )))),
            asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None)))),
        ];
        let exit_block = vec![asm.alloc_root(CILRoot::Ret(ldloc_0))];
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(entry_block, 0, None),
                BasicBlock::new(loop_block, 1, None),
                BasicBlock::new(exit_block, 2, None),
            ],
            locals: vec![(None, asm.alloc_type(tpe)), (None, asm.alloc_type(tpe))],
        }
    };
    patcher.insert(name, Box::new(generator));
}

fn generate_atomic(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    op: AtomicRmwOp,
    int: Int,
    native_subword: bool,
) {
    generate_atomic_impl(
        asm,
        patcher,
        op.name(),
        Box::new(move |asm, lhs, rhs, int| op.apply(asm, lhs, rhs, int)),
        int,
        native_subword,
    );
}

/// Registers the fallback-only atomics required by the legacy Unity artifact ABI. Public .NET 10
/// codegen uses native sub-word `Interlocked.Exchange`/`CompareExchange` overloads and therefore
/// never references these helpers.
fn generate_legacy_subword_fallbacks(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    emulate_subword_cmp_xchng(asm, patcher, 1);
    emulate_subword_cmp_xchng(asm, patcher, 2);
    emulate_subword_xchng(asm, patcher, 1);
    emulate_subword_xchng(asm, patcher, 2);
}

/// Adds every builtin atomic function to the patcher.
pub fn generate_all_atomics(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    native_subword: bool,
) {
    for op in AtomicRmwOp::ALL {
        for int in ATOMIC_INTS {
            generate_atomic(asm, patcher, op, int, native_subword);
        }
    }
    if !native_subword {
        generate_legacy_subword_fallbacks(asm, patcher);
    }
}
/*
  .method public hidebysig static
        uint32 atomic_xor (
            uint32& addr,
            uint32 xorand
        ) cil managed
    {
        // Method begins at RVA 0x2050
        // Code size 25 (0x19)
        .maxstack 3
        .locals  (
            [0] uint32 addr_val,
            [1] uint32 got
        )


        // loop start (head: IL_0013)
            IL_0006: ldloc.1
            IL_0007: stloc.0

            IL_0008:  ldarg.0
            IL_0009:   ldloc.0
            IL_000a:   ldarg.1
            IL_000b:  xor
            IL_000c:  ldloc.0
            IL_000d: call uint32 [System.Threading]System.Threading.Interlocked::CompareExchange(uint32&, uint32, uint32)
            IL_0012: stloc.1

            IL_0013: ldloc.0
            IL_0014: ldloc.1
            IL_0015: bne.un.s IL_0006
        // end loop
        IL_0017: ldloc.0
        IL_0018: ret
    } // end of method Tmp::atomic_xor

*/

#[cfg(test)]
mod tests {
    use super::{ATOMIC_INTS, AtomicRmwOp, generate_all_atomics};
    use crate::{
        Assembly, CILIter, CILIterElem, CILNode, CILRoot, Int, MethodImpl, MissingMethodPatcher,
        Type, cilnode::MethodKind,
    };

    fn assert_legacy_helper_return_cast(
        asm: &mut Assembly,
        patcher: &MissingMethodPatcher,
        name: &str,
        int: Int,
        argument_count: usize,
    ) {
        let symbol = asm.alloc_string(name);
        let tpe = Type::Int(int);
        let mut inputs = vec![asm.nref(tpe)];
        inputs.extend(std::iter::repeat_n(tpe, argument_count - 1));
        let signature = asm.sig(inputs, tpe);
        let owner = *asm.main_module();
        let method = asm.new_methodref(owner, name, signature, MethodKind::Static, vec![]);
        let implementation = patcher.get(&symbol).expect("legacy atomic generator")(method, asm);
        let MethodImpl::MethodBody { blocks, .. } = implementation else {
            panic!("legacy atomic generator must produce a method body");
        };
        let return_root = *blocks
            .last()
            .expect("legacy helper exit block")
            .roots()
            .last()
            .expect("legacy helper return");
        let CILRoot::Ret(value) = asm.get_root(return_root) else {
            panic!("legacy helper must end in a return");
        };
        assert!(matches!(
            asm.get_node(*value),
            CILNode::IntCast { target, .. } if *target == int
        ));
    }

    fn generated_u16_add_call_names(native_subword: bool) -> Vec<String> {
        let mut asm = Assembly::default();
        let mut patcher = MissingMethodPatcher::default();
        generate_all_atomics(&mut asm, &mut patcher, native_subword);
        let symbol = asm.alloc_string("atomic_add_u16");
        let owner = asm.main_module();
        let signature = asm.sig([], Type::Void);
        let probe = asm.new_methodref(
            *owner,
            "atomic_test_probe",
            signature,
            MethodKind::Static,
            vec![],
        );
        let implementation = patcher.get(&symbol).expect("u16 add generator")(probe, &mut asm);
        let MethodImpl::MethodBody { blocks, .. } = implementation else {
            panic!("atomic generator must produce a method body");
        };
        blocks
            .iter()
            .flat_map(|block| block.roots())
            .flat_map(|root| CILIter::new(asm.get_root(*root).clone(), &asm))
            .filter_map(|element| match element {
                CILIterElem::Node(CILNode::Call(info)) => {
                    let (method, _, _) = *info;
                    Some(asm[asm[method].name()].to_string())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn registers_the_complete_integer_rmw_matrix() {
        let mut asm = Assembly::default();
        let mut patcher = MissingMethodPatcher::default();
        generate_all_atomics(&mut asm, &mut patcher, true);

        for op in AtomicRmwOp::ALL {
            for int in ATOMIC_INTS {
                let symbol = asm.alloc_string(format!("atomic_{}_{}", op.name(), int.name()));
                assert!(
                    patcher.contains_key(&symbol),
                    "missing registration for {} on {int:?}",
                    op.name()
                );
            }
        }
    }

    #[test]
    fn matrix_includes_both_sixteen_bit_integer_types() {
        assert!(ATOMIC_INTS.contains(&crate::Int::U16));
        assert!(ATOMIC_INTS.contains(&crate::Int::I16));
    }

    #[test]
    fn legacy_subword_fallbacks_are_not_registered_for_dotnet_10() {
        let mut public_asm = Assembly::default();
        let mut public_patcher = MissingMethodPatcher::default();
        generate_all_atomics(&mut public_asm, &mut public_patcher, true);
        let fallback = public_asm.alloc_string("atomic_cmpxchng16_correct");
        assert!(!public_patcher.contains_key(&fallback));

        let mut legacy_asm = Assembly::default();
        let mut legacy_patcher = MissingMethodPatcher::default();
        generate_all_atomics(&mut legacy_asm, &mut legacy_patcher, false);
        let fallback = legacy_asm.alloc_string("atomic_cmpxchng16_correct");
        assert!(legacy_patcher.contains_key(&fallback));
        let exchange = legacy_asm.alloc_string("atomic_xchng8_correct");
        assert!(legacy_patcher.contains_key(&exchange));
        let obsolete_exchange = legacy_asm.alloc_string("atomic_xchng_u8");
        assert!(!legacy_patcher.contains_key(&obsolete_exchange));

        let public_calls = generated_u16_add_call_names(true);
        assert!(public_calls.iter().any(|name| name == "CompareExchange"));
        assert!(
            !public_calls
                .iter()
                .any(|name| name == "atomic_cmpxchng16_correct")
        );
        let legacy_calls = generated_u16_add_call_names(false);
        assert!(
            legacy_calls
                .iter()
                .any(|name| name == "atomic_cmpxchng16_correct")
        );

        for int in [Int::U8, Int::I8] {
            assert_legacy_helper_return_cast(
                &mut legacy_asm,
                &legacy_patcher,
                "atomic_xchng8_correct",
                int,
                2,
            );
            assert_legacy_helper_return_cast(
                &mut legacy_asm,
                &legacy_patcher,
                "atomic_cmpxchng8_correct",
                int,
                3,
            );
        }
        for int in [Int::U16, Int::I16] {
            assert_legacy_helper_return_cast(
                &mut legacy_asm,
                &legacy_patcher,
                "atomic_xchng16_correct",
                int,
                2,
            );
            assert_legacy_helper_return_cast(
                &mut legacy_asm,
                &legacy_patcher,
                "atomic_cmpxchng16_correct",
                int,
                3,
            );
        }
    }
}
