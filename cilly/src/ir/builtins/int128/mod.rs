use crate::{
    asm::MissingMethodPatcher, cilnode::ExtendKind, Assembly, BasicBlock, BinOp, BranchCond,
    CILNode, CILRoot, ClassRef, Const, FieldDesc, Int, Interned, MethodImpl, MethodRef, Type,
};

fn op_direct(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    lhs: Int,
    _rhs: Int,
    op: BinOp,
) {
    let name = asm.alloc_string(format!("{op}_{lhs}", op = op.name(), lhs = lhs.name()));
    let generator = move |_, asm: &mut Assembly| {
        let op = asm.biop(CILNode::LdArg(0), CILNode::LdArg(1), op);
        let ret = asm.alloc_root(CILRoot::Ret(op));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
fn op_indirect(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    lhs_type: Int,
    rhs_type: Int,
    op: BinOp,
    ret_type: Type,
) {
    let name = asm.alloc_string(format!(
        "{op}_{lhs_type}",
        op = op.name(),
        lhs_type = lhs_type.name()
    ));
    let generator = move |_, asm: &mut Assembly| {
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let class = lhs_type.class(asm);
        let class = asm[class].clone();
        let call_op = class.static_mref(
            &[Type::Int(lhs_type), Type::Int(rhs_type)],
            ret_type,
            asm.alloc_string(op.dotnet_name()),
            asm,
        );
        let call = asm.alloc_node(CILNode::call(call_op, [lhs, rhs]));
        let ret = asm.alloc_root(CILRoot::Ret(call));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn generate_int128_ops(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, direct: bool) {
    const OPS: [BinOp; 8] = [
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
        BinOp::Or,
        BinOp::XOr,
        BinOp::And,
        BinOp::Rem,
        BinOp::Div,
    ];
    const SHIFTS: [BinOp; 2] = [BinOp::Shl, BinOp::Shr];
    const CMPS: [BinOp; 3] = [BinOp::Lt, BinOp::Gt, BinOp::Eq];
    let ints = [Int::U128, Int::I128];
    for op in OPS {
        for int in ints {
            if direct {
                op_direct(asm, patcher, int, int, op);
            } else {
                op_indirect(asm, patcher, int, int, op, Type::Int(int));
            }
        }
    }
    for op in SHIFTS {
        for int in ints {
            if direct {
                op_direct(asm, patcher, int, Int::I32, op);
            } else {
                op_indirect(asm, patcher, int, Int::I32, op, Type::Int(int));
            }
        }
    }
    for op in CMPS {
        for int in ints {
            if direct {
                op_direct(asm, patcher, int, int, op);
            } else {
                op_indirect(asm, patcher, int, int, op, Type::Bool);
            }
        }
    }
}
pub fn i128_mul_ovf_check(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("i128_mul_ovf_check");
    let generator = move |_, asm: &mut Assembly| {
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let i128_class = ClassRef::int_128(asm);
        let get_zero = asm.alloc_string("get_Zero");
        let op_equality = asm.alloc_string("eq_i128");
        let op_mul = asm.alloc_string("mul_i128");
        let op_div = asm.alloc_string("div_i128");
        let i128_classref = asm[i128_class].clone();
        let main_module = *asm.main_module();
        let main_module = asm[main_module].clone();
        let const_zero = i128_classref.static_mref(&[], Type::Int(Int::I128), get_zero, asm);
        let const_zero = asm.alloc_node(CILNode::call(const_zero, []));
        let i128_eq = main_module.static_mref(
            &[Type::Int(Int::I128), Type::Int(Int::I128)],
            Type::Bool,
            op_equality,
            asm,
        );
        let i128_mul = main_module.static_mref(
            &[Type::Int(Int::I128), Type::Int(Int::I128)],
            Type::Int(Int::I128),
            op_mul,
            asm,
        );
        let i128_div = main_module.static_mref(
            &[Type::Int(Int::I128), Type::Int(Int::I128)],
            Type::Int(Int::I128),
            op_div,
            asm,
        );
        let rhs_zero = asm.alloc_node(CILNode::call(i128_eq, [rhs, const_zero]));
        let jmp_nz = asm.alloc_root(CILRoot::Branch(Box::new((
            0,
            1,
            Some(BranchCond::False(rhs_zero)),
        ))));
        let ret_false = asm.alloc_node(Const::Bool(false));
        let ret_false = asm.alloc_root(CILRoot::Ret(ret_false));
        let lhs_mul_rhs = asm.alloc_node(CILNode::call(i128_mul, [lhs, rhs]));
        let recomputed_rhs = asm.alloc_node(CILNode::call(i128_div, [lhs_mul_rhs, rhs]));
        let ovf = asm.alloc_node(CILNode::call(i128_eq, [recomputed_rhs, rhs]));
        let ret_ovf = asm.alloc_root(CILRoot::Ret(ovf));

        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![jmp_nz, ret_false], 0, None),
                BasicBlock::new(vec![ret_ovf], 1, None),
            ],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
/// Widens a `u64`/`u8` argument to a `System.UInt128` by calling its implicit
/// conversion operator. `IntCast` cannot target `Int::U128` (it is `todo!()` in
/// both the IL and C exporters), so all 64→128 widening must go through the BCL.
fn u64_to_u128(asm: &mut Assembly, input: Interned<CILNode>, src: Int) -> Interned<CILNode> {
    let u128_class = ClassRef::uint_128(asm);
    let u128_classref = asm[u128_class].clone();
    let op_implicit = asm.alloc_string("op_Implicit");
    let mref = u128_classref.static_mref(
        &[Type::Int(src)],
        Type::Int(Int::U128),
        op_implicit,
        asm,
    );
    asm.alloc_node(CILNode::call(mref, [input]))
}
/// Narrows a `System.UInt128` to `dst` (`u64`/`u8`) via its explicit conversion
/// operator. Mirrors [`u64_to_u128`] — `IntCast` from `U128` is also `todo!()`.
fn u128_to_int(asm: &mut Assembly, input: Interned<CILNode>, dst: Int) -> Interned<CILNode> {
    let u128_class = ClassRef::uint_128(asm);
    let u128_classref = asm[u128_class].clone();
    let op_explicit = asm.alloc_string("op_Explicit");
    let mref = u128_classref.static_mref(
        &[Type::Int(Int::U128)],
        Type::Int(dst),
        op_explicit,
        asm,
    );
    asm.alloc_node(CILNode::call(mref, [input]))
}
/// Calls a binary `System.UInt128` operator (`op_Addition`/`op_Subtraction`)
/// returning a `u128`.
fn u128_binop(
    asm: &mut Assembly,
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    op_name: &str,
) -> Interned<CILNode> {
    let u128_class = ClassRef::uint_128(asm);
    let u128_classref = asm[u128_class].clone();
    let name = asm.alloc_string(op_name);
    let mref = u128_classref.static_mref(
        &[Type::Int(Int::U128), Type::Int(Int::U128)],
        Type::Int(Int::U128),
        name,
        asm,
    );
    asm.alloc_node(CILNode::call(mref, [lhs, rhs]))
}
/// Implements the LLVM x86 add-with-carry / subtract-with-borrow platform
/// intrinsics (`llvm.x86.addcarry.64`, `llvm.x86.subborrow.64`) as
/// missing-method builtins, synthesized via `System.UInt128`.
///
/// ABI (matching `@llvm.x86.addcarry.64(i8 carry_in, i64 a, i64 b)`):
///   arg0 = carry_in/borrow_in : u8
///   arg1 = a : u64
///   arg2 = b : u64
/// Returns the Rust 2-tuple `(u8 carry_out/borrow_out, u64 sum/diff)` as the
/// per-build mangled tuple struct whose shape is read from the method's output
/// signature (`Item1`: u8 at offset 0, `Item2`: u64 at offset 8) — exactly like
/// `ovf_check_tuple`, so the struct class name is never hardcoded.
///
/// Only the `.64` variants are emitted by any built program (confirmed by
/// scanning every cargo_tests .NET exe — only `num-bigint`, only `.64`). The
/// `.32` variants would be the analogous `u32`-via-`u128` path if they ever
/// surface.
pub fn generate_x86_wide_carry(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    // ---- llvm.x86.addcarry.64 ----
    let name = asm.alloc_string("llvm.x86.addcarry.64");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let res = *asm[asm[mref].sig()].output();
        let cref = res.as_class_ref().unwrap();
        // Load args.
        let carry_in = asm.alloc_node(CILNode::LdArg(0));
        let a = asm.alloc_node(CILNode::LdArg(1));
        let b = asm.alloc_node(CILNode::LdArg(2));
        // Widen to u128.
        let a128 = u64_to_u128(asm, a, Int::U64);
        let b128 = u64_to_u128(asm, b, Int::U64);
        let c128 = u64_to_u128(asm, carry_in, Int::U8);
        // s = a + b + carry_in (u128, cannot overflow).
        let ab = u128_binop(asm, a128, b128, "op_Addition");
        let s = u128_binop(asm, ab, c128, "op_Addition");
        // sum = (u64)s
        let sum = u128_to_int(asm, s, Int::U64);
        // carry_out = (u8)(s >> 64)
        let shift = asm.alloc_node(Const::I32(64));
        let u128_class = ClassRef::uint_128(asm);
        let u128_classref = asm[u128_class].clone();
        let op_rshift = asm.alloc_string("op_RightShift");
        let rshift = u128_classref.static_mref(
            &[Type::Int(Int::U128), Type::Int(Int::I32)],
            Type::Int(Int::U128),
            op_rshift,
            asm,
        );
        let s_hi = asm.alloc_node(CILNode::call(rshift, [s, shift]));
        let carry_out = u128_to_int(asm, s_hi, Int::U8);
        // Store into the result tuple struct.
        let addr = asm.alloc_node(CILNode::LdLocA(0));
        let item1_name = asm.alloc_string("Item1");
        let item2_name = asm.alloc_string("Item2");
        let item1 = asm.alloc_field(FieldDesc::new(cref, item1_name, Type::Int(Int::U8)));
        let item2 = asm.alloc_field(FieldDesc::new(cref, item2_name, Type::Int(Int::U64)));
        let set_carry = asm.alloc_root(CILRoot::SetField(Box::new((item1, addr, carry_out))));
        let set_sum = asm.alloc_root(CILRoot::SetField(Box::new((item2, addr, sum))));
        let ret_val = asm.alloc_node(CILNode::LdLoc(0));
        let ret = asm.alloc_root(CILRoot::Ret(ret_val));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![set_carry, set_sum, ret], 0, None)],
            locals: vec![(None, asm.alloc_type(res))],
        }
    };
    patcher.insert(name, Box::new(generator));

    // ---- llvm.x86.subborrow.64 ----
    let name = asm.alloc_string("llvm.x86.subborrow.64");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let res = *asm[asm[mref].sig()].output();
        let cref = res.as_class_ref().unwrap();
        // Load args.
        let borrow_in = asm.alloc_node(CILNode::LdArg(0));
        let a = asm.alloc_node(CILNode::LdArg(1));
        let b = asm.alloc_node(CILNode::LdArg(2));
        // Widen to u128.
        let a128 = u64_to_u128(asm, a, Int::U64);
        let b128 = u64_to_u128(asm, b, Int::U64);
        let c128 = u64_to_u128(asm, borrow_in, Int::U8);
        // bc = b + borrow_in (u128, cannot overflow since both < 2^64).
        let bc = u128_binop(asm, b128, c128, "op_Addition");
        // s = a - bc (u128, wraps; low 64 bits are the correct diff).
        let s = u128_binop(asm, a128, bc, "op_Subtraction");
        // diff = (u64)s
        let diff = u128_to_int(asm, s, Int::U64);
        // borrow_out = (a < b + borrow_in) ? 1 : 0
        let u128_class = ClassRef::uint_128(asm);
        let u128_classref = asm[u128_class].clone();
        let op_lt = asm.alloc_string("op_LessThan");
        let lt = u128_classref.static_mref(
            &[Type::Int(Int::U128), Type::Int(Int::U128)],
            Type::Bool,
            op_lt,
            asm,
        );
        let lt = asm.alloc_node(CILNode::call(lt, [a128, bc]));
        let borrow_out = asm.int_cast(lt, Int::U8, ExtendKind::ZeroExtend);
        // Store into the result tuple struct.
        let addr = asm.alloc_node(CILNode::LdLocA(0));
        let item1_name = asm.alloc_string("Item1");
        let item2_name = asm.alloc_string("Item2");
        let item1 = asm.alloc_field(FieldDesc::new(cref, item1_name, Type::Int(Int::U8)));
        let item2 = asm.alloc_field(FieldDesc::new(cref, item2_name, Type::Int(Int::U64)));
        let set_borrow = asm.alloc_root(CILRoot::SetField(Box::new((item1, addr, borrow_out))));
        let set_diff = asm.alloc_root(CILRoot::SetField(Box::new((item2, addr, diff))));
        let ret_val = asm.alloc_node(CILNode::LdLoc(0));
        let ret = asm.alloc_root(CILRoot::Ret(ret_val));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![set_borrow, set_diff, ret], 0, None)],
            locals: vec![(None, asm.alloc_type(res))],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn u128_mul_ovf_check(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("u128_mul_ovf_check");
    let generator = move |_, asm: &mut Assembly| {
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let u128_class = ClassRef::uint_128(asm);
        let get_zero = asm.alloc_string("get_Zero");
        let op_equality = asm.alloc_string("eq_u128");
        let op_mul = asm.alloc_string("mul_u128");
        let op_div = asm.alloc_string("div_u128");
        let u128_classref = asm[u128_class].clone();
        let main_module = *asm.main_module();
        let main_module = asm[main_module].clone();
        let const_zero = u128_classref.static_mref(&[], Type::Int(Int::U128), get_zero, asm);
        let const_zero = asm.alloc_node(CILNode::call(const_zero, []));
        let u128_eq = main_module.static_mref(
            &[Type::Int(Int::U128), Type::Int(Int::U128)],
            Type::Bool,
            op_equality,
            asm,
        );
        let u128_mul = main_module.static_mref(
            &[Type::Int(Int::U128), Type::Int(Int::U128)],
            Type::Int(Int::U128),
            op_mul,
            asm,
        );
        let u128_div = main_module.static_mref(
            &[Type::Int(Int::U128), Type::Int(Int::U128)],
            Type::Int(Int::U128),
            op_div,
            asm,
        );
        let rhs_zero = asm.alloc_node(CILNode::call(u128_eq, [rhs, const_zero]));
        let jmp_nz = asm.alloc_root(CILRoot::Branch(Box::new((
            0,
            1,
            Some(BranchCond::False(rhs_zero)),
        ))));
        let ret_false = asm.alloc_node(Const::Bool(false));
        let ret_false = asm.alloc_root(CILRoot::Ret(ret_false));
        let lhs_mul_rhs = asm.alloc_node(CILNode::call(u128_mul, [lhs, rhs]));
        let recomputed_rhs = asm.alloc_node(CILNode::call(u128_div, [lhs_mul_rhs, rhs]));
        let ovf = asm.alloc_node(CILNode::call(u128_eq, [recomputed_rhs, rhs]));
        /*let ovf2 = asm.alloc_node(CILNode::BinOp(lhs, lhs_mul_rhs, BinOp::GtUn));
        let ovf2 = asm.alloc_node(CILNode::UnOp(ovf2, UnOp::Neg));
        let ovf = asm.alloc_node(CILNode::BinOp(ovf, ovf2, BinOp::Or));*/
        let ret_ovf = asm.alloc_root(CILRoot::Ret(ovf));

        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![jmp_nz, ret_false], 0, None),
                BasicBlock::new(vec![ret_ovf], 1, None),
            ],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
