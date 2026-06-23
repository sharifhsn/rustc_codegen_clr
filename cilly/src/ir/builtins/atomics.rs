use crate::{
    asm::MissingMethodPatcher,
    bimap::Interned,
    cilnode::{ExtendKind, MethodKind},
    cilroot::BranchCond,
    BasicBlock, BinOp, CILNode, CILRoot, ClassRef, Const, Int, MethodImpl, MethodRef, Type,
};

use super::{
    super::Assembly,
    math::{int_max, int_min},
};
/// Emulates operations on bytes using operations on int32s. Enidianess dependent, can cause segfuaults when used on a page boundary.
/// TODO: remove when .NET 9 is out.
///
/// NOTE: the `cmpxchng{8,16}` builtins generated here splice the new sub-word *unconditionally* (their
/// body never reads the comparand). That is correct ONLY as the inner step of the re-reading RMW loop
/// in [`generate_atomic`]. For Rust's `atomic_cxchg`/`atomic_xchg` (which must honour the comparand and
/// not write on mismatch) use [`emulate_subword_cmp_xchng`] / [`emulate_subword_xchng`] instead.
pub fn emulate_uint8_cmp_xchng(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    generate_atomic(
        asm,
        patcher,
        "cmpxchng8",
        Box::new(|asm, prev, arg, _| {
            // 1st, mask the previous value
            let prev_mask = asm.alloc_node(Const::I32(0xFFFF_FF00_u32 as i32));
            let prev = asm.alloc_node(CILNode::BinOp(prev, prev_mask, BinOp::And));
            let arg = asm.alloc_node(CILNode::IntCast {
                input: arg,
                target: Int::I32,
                extend: ExtendKind::ZeroExtend,
            });
            asm.alloc_node(CILNode::BinOp(prev, arg, BinOp::Or))
        }),
        Int::I32,
    );
    generate_atomic(
        asm,
        patcher,
        "cmpxchng16",
        Box::new(|asm, prev, arg, _| {
            // 1st, mask the previous value
            let prev_mask = asm.alloc_node(Const::I32(0xFFFF_0000_u32 as i32));
            let prev = asm.alloc_node(CILNode::BinOp(prev, prev_mask, BinOp::And));
            let arg = asm.alloc_node(CILNode::IntCast {
                input: arg,
                target: Int::I32,
                extend: ExtendKind::ZeroExtend,
            });
            asm.alloc_node(CILNode::BinOp(prev, arg, BinOp::Or))
        }),
        Int::I32,
    );
    let name = asm.alloc_string("atomic_xchng_u8");
    let generator = move |_, asm: &mut Assembly| {
        let ldarg_0 = asm.alloc_node(CILNode::LdArg(0));
        let ldarg_1 = asm.alloc_node(CILNode::LdArg(1));
        let ldloc_0 = asm.alloc_node(CILNode::LdLoc(0));
        let uint8_idx = asm.alloc_type(Type::Int(Int::U8));
        // Load value at addr 0 and write it to tmp
        let arg0_val = asm.alloc_node(CILNode::LdInd {
            addr: ldarg_0,
            tpe: uint8_idx,
            volatile: true,
        });
        let set_tmp = asm.alloc_root(CILRoot::StLoc(0, arg0_val));
        // Copy arg1 to addr0
        let copy_arg1 = asm.alloc_root(CILRoot::StInd(Box::new((
            ldarg_0,
            ldarg_1,
            Type::Int(Int::U8),
            true,
        ))));
        let ret = asm.alloc_root(CILRoot::Ret(ldloc_0));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![set_tmp, copy_arg1, ret], 0, None)],
            locals: vec![(None, uint8_idx)],
        }
    };
    patcher.insert(name, Box::new(generator));
}
/// Emits a sub-word (`u8`/`i8`/`u16`/`i16`) atomic exchange, named `atomic_xchng{8,16}_correct`, as a
/// masked 32-bit `Interlocked.CompareExchange` loop that unconditionally splices the new sub-word and
/// retries until the full word swaps. Unlike a plain volatile load/store, this is genuinely atomic
/// against concurrent writers to the SAME word. Returns the old sub-word.
///
/// Signature: `int_ty atomic_xchng{8,16}_correct(int_ty& addr, int_ty new)`.
/// Same LE-only + page-boundary caveats as [`emulate_subword_cmp_xchng`].
pub fn emulate_subword_xchng(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, width: u8) {
    debug_assert!(width == 1 || width == 2, "sub-word xchg width must be 1 or 2");
    let name = asm.alloc_string(format!("atomic_xchng{}_correct", width * 8));
    let full_mask: u32 = if width == 1 { 0xFF } else { 0xFFFF };
    let generator = move |_, asm: &mut Assembly| {
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
/// * Little-endian only (the LE x86_64 / .NET 8 target): the sub-word byte lives in the LOW bits of
///   the containing word at `(addr & 3) * 8`.
/// * Page-boundary hazard: the address is aligned DOWN to its containing 32-bit word, so up to 3
///   bytes before the target byte are touched. A naturally-aligned `u8`/`u16` atomic is always
///   contained within one word (Rust requires natural alignment), so the aligned-down word stays in
///   the same allocation in practice — but the general caveat stands. Remove once .NET 9's native
///   sub-word `Interlocked.CompareExchange` is the floor.
pub fn emulate_subword_cmp_xchng(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, width: u8) {
    debug_assert!(width == 1 || width == 2, "sub-word CAS width must be 1 or 2");
    let name = asm.alloc_string(format!("atomic_cmpxchng{}_correct", width * 8));
    let full_mask: u32 = if width == 1 { 0xFF } else { 0xFFFF };
    let generator = move |_, asm: &mut Assembly| {
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
) -> Interned<CILNode> {
    match int.size().unwrap_or(8) {
        // u16 is buggy :(. TODO: fix it.
        1 | 2 => {
            let compare_exchange = asm.alloc_string(format!(
                "atomic_cmpxchng{}_i32",
                int.size().unwrap_or(8) * 8
            ));

            let i32 = Type::Int(int);
            let i32_ref = asm.nref(Type::Int(Int::I32));
            let cmpxchng_sig = asm.sig([i32_ref, i32, i32], i32);
            let main_mod = asm.main_module();
            let mref = asm.alloc_methodref(MethodRef::new(
                *main_mod,
                compare_exchange,
                cmpxchng_sig,
                MethodKind::Static,
                vec![].into(),
            ));
            let cast_value = asm.alloc_node(CILNode::IntCast {
                input: value,
                target: int,
                extend: crate::cilnode::ExtendKind::ZeroExtend,
            });
            let cast_comparand = asm.alloc_node(CILNode::IntCast {
                input: comaprand,
                target: int,
                extend: crate::cilnode::ExtendKind::ZeroExtend,
            });
            let addr = asm.alloc_node(CILNode::RefToPtr(addr));
            let i32_tidx = asm.alloc_type(Type::Int(Int::I32));
            let addr = asm.alloc_node(CILNode::PtrCast(
                addr,
                Box::new(crate::cilnode::PtrCastRes::Ptr(i32_tidx)),
            ));
            let res = asm.alloc_node(CILNode::call(mref, [addr, cast_value, cast_comparand]));
            asm.alloc_node(CILNode::IntCast {
                input: res,
                target: int,
                extend: crate::cilnode::ExtendKind::ZeroExtend,
            })
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
type AsmGen =
    dyn Fn(&mut Assembly, Interned<CILNode>, Interned<CILNode>, Int) -> Interned<CILNode>;
pub fn generate_atomic(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    op_name: &str,
    op: Box<AsmGen>,
    int: Int,
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
        let op = op(asm, ldloc_0, ldarg_1, int);
        let call = compare_exchange(asm, int, ldarg_0, op, ldloc_0);

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
pub fn generate_atomic_for_ints(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    op_name: &str,
    op: impl Fn(&mut Assembly, Interned<CILNode>, Interned<CILNode>, Int) -> Interned<CILNode>
        + 'static
        + Clone,
) {
    const ATOMIC_INTS: [Int; 10] = [
        Int::U8,
        Int::I8,
        Int::U16,
        Int::I16,
        Int::U32,
        Int::U64,
        Int::USize,
        Int::I32,
        Int::I64,
        Int::ISize,
    ];
    for int in ATOMIC_INTS {
        generate_atomic(asm, patcher, op_name, Box::new(op.clone()), int);
    }
}
/// Adds all the builitn atomic functions to the patcher, allowing for their use.
pub fn generate_all_atomics(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    generate_atomic_for_ints(asm, patcher, "add", |asm, lhs, rhs, _| {
        asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::Add))
    });
    generate_atomic_for_ints(asm, patcher, "sub", |asm, lhs, rhs, _| {
        asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::Sub))
    });
    // XOR
    generate_atomic_for_ints(asm, patcher, "xor", |asm, lhs, rhs, _| {
        asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::XOr))
    });
    // NAND
    generate_atomic_for_ints(asm, patcher, "nand", |asm, lhs, rhs, _| {
        let and = asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::And));
        asm.alloc_node(CILNode::UnOp(and, crate::cilnode::UnOp::Not))
    });
    // Max
    generate_atomic_for_ints(asm, patcher, "max", int_max);
    // Max
    generate_atomic_for_ints(asm, patcher, "min", int_min);
    // Emulates 1 byte compare exchange
    emulate_uint8_cmp_xchng(asm, patcher);
    // Correct, comparand-checked sub-word compare-exchange for Rust's `atomic_cxchg` (8 & 16 bit).
    emulate_subword_cmp_xchng(asm, patcher, 1);
    emulate_subword_cmp_xchng(asm, patcher, 2);
    // Genuinely-atomic sub-word exchange for Rust's `atomic_xchg` (8 & 16 bit).
    emulate_subword_xchng(asm, patcher, 1);
    emulate_subword_xchng(asm, patcher, 2);
    for int in [Int::ISize, Int::USize, Int::U8, Int::I8] {
        generate_atomic(
            asm,
            patcher,
            "or",
            Box::new(|asm, lhs, rhs, _| asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::Or))),
            int,
        );
        generate_atomic(
            asm,
            patcher,
            "and",
            Box::new(|asm, lhs, rhs, _| asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::And))),
            int,
        );
        generate_atomic(
            asm,
            patcher,
            "add",
            Box::new(|asm, lhs, rhs, _| asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::Add))),
            int,
        );
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
