use crate::{
    Assembly, BasicBlock, BinOp, CILNode, CILRoot, ClassRef, Const, Float, Int, MethodImpl,
    MethodRef, Type,
    asm::MissingMethodPatcher,
    bimap::Interned,
    cilnode::MethodKind,
    hashable::{HashableF32, HashableF64},
};

pub fn int_max(
    asm: &mut Assembly,
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    int: Int,
) -> Interned<CILNode> {
    let math = ClassRef::math(asm);
    let max = asm.alloc_string("Max");
    let sig = asm.sig([Type::Int(int), Type::Int(int)], Type::Int(int));
    let mref = asm.alloc_methodref(MethodRef::new(
        math,
        max,
        sig,
        MethodKind::Static,
        vec![].into(),
    ));
    asm.alloc_node(CILNode::call(mref, [lhs, rhs]))
}

pub fn int_min(
    asm: &mut Assembly,
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    int: Int,
) -> Interned<CILNode> {
    let math = ClassRef::math(asm);
    let max = asm.alloc_string("Min");
    let sig = asm.sig([Type::Int(int), Type::Int(int)], Type::Int(int));
    let mref = asm.alloc_methodref(MethodRef::new(
        math,
        max,
        sig,
        MethodKind::Static,
        vec![].into(),
    ));
    asm.alloc_node(CILNode::call(mref, [lhs, rhs]))
}
pub fn ldexpf(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("ldexpf");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let exp = asm.alloc_node(CILNode::LdArg(1));
        let exp = asm.alloc_node(CILNode::FloatCast {
            input: exp,
            target: Float::F32,
            is_signed: true,
        });
        let two = asm.alloc_node(Const::F32(HashableF32(2.0)));
        let pow = Float::F32.pow(two, exp, asm);
        let res_val = asm.alloc_node(CILNode::BinOp(arg, pow, BinOp::Mul));
        let ret = asm.alloc_root(CILRoot::Ret(res_val));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn sinhf(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("sinhf");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let sinh = Float::F32.math1(arg, asm, "Sinh");
        let ret = asm.alloc_root(CILRoot::Ret(sinh));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn sinh(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("sinh");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let sinh = Float::F64.math1(arg, asm, "Sinh");
        let ret = asm.alloc_root(CILRoot::Ret(sinh));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn coshf(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("coshf");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let cosh = Float::F32.math1(arg, asm, "Cosh");
        let ret = asm.alloc_root(CILRoot::Ret(cosh));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn cosh(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("cosh");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let cosh = Float::F64.math1(arg, asm, "Cosh");
        let ret = asm.alloc_root(CILRoot::Ret(cosh));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
// `expm1`/`log1p` (and their f32 forms) have no `System.Math` equivalent, so compose them from
// `Exp`/`Log`. The naive `exp(x)-1` / `log(1+x)` lose precision for |x| very near 0 (catastrophic
// cancellation), but every std/libm caller (e.g. the Zipf rejection sampler's `helper1`/`helper2`)
// already switches to a Taylor series for |x| < ~1e-8 and only calls these for larger |x|, where the
// naive forms are accurate. A portable `Math`-based body also avoids the Linux-only `libm.so.6`
// P/Invoke (`LIBM_FNS`), which does not resolve on macOS/Windows. Surfaced by alloctests
// `sort::*::correct_i32_random_z{1_03,2}` (Zipf with a non-1.0 exponent), which crashed with
// `missing method expm1`.
//
// The naive forms also LOSE THE SIGN OF ZERO: `exp(-0)-1 = 1-1 = +0` and `log(1+-0) = log(1) = +0`,
// but IEEE/libm require `expm1(-0) = -0` and `log1p(-0) = -0`. Both are monotonic through the
// origin, so `sign(result) == sign(x)` for every value in range; restoring the input's sign with
// `CopySign` fixes the signed zero and is a no-op for all other inputs. (Needed because `f32::atanh`
// = `0.5 * ((2x)/(1-x)).ln_1p()` and `atanh(-0)` must be `-0` — coretests `num::floats::atanh`.)
pub fn expm1(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("expm1");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let exp = Float::F64.math1(arg, asm, "Exp");
        let one = asm.alloc_node(Const::F64(HashableF64(1.0)));
        let res = asm.alloc_node(CILNode::BinOp(exp, one, BinOp::Sub));
        let res = Float::F64.math2(res, arg, asm, "CopySign");
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn expm1f(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("expm1f");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let exp = Float::F32.math1(arg, asm, "Exp");
        let one = asm.alloc_node(Const::F32(HashableF32(1.0)));
        let res = asm.alloc_node(CILNode::BinOp(exp, one, BinOp::Sub));
        let res = Float::F32.math2(res, arg, asm, "CopySign");
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn log1p(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("log1p");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let one = asm.alloc_node(Const::F64(HashableF64(1.0)));
        let onepx = asm.alloc_node(CILNode::BinOp(one, arg, BinOp::Add));
        let log = Float::F64.math1(onepx, asm, "Log");
        let log = Float::F64.math2(log, arg, asm, "CopySign");
        let ret = asm.alloc_root(CILRoot::Ret(log));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn log1pf(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("log1pf");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let one = asm.alloc_node(Const::F32(HashableF32(1.0)));
        let onepx = asm.alloc_node(CILNode::BinOp(one, arg, BinOp::Add));
        let log = Float::F32.math1(onepx, asm, "Log");
        let log = Float::F32.math2(log, arg, asm, "CopySign");
        let ret = asm.alloc_root(CILRoot::Ret(log));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
// Inverse hyperbolic functions. Like `sinh`/`cosh`, these are libm externs (`acosh`/`asinh`/`atanh`
// + f32 forms) that `core`/`std` reach through `cmath`; with no implementation the linker leaves
// them unresolved and the first call throws `missing method <name>` — which unwinds the test thread
// (caught → FAILED) or crosses a nounwind boundary (→ process abort). Surfaced by coretests
// `num::floats::{acosh,asinh,atanh}::test_f{32,64}`. .NET has exact equivalents
// (`System.Math.Acosh/Asinh/Atanh`, `System.MathF.*`), so map them directly via `math1` — no
// precision-losing composition needed (unlike `expm1`/`log1p`).
pub fn asinh(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("asinh");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let res = Float::F64.math1(arg, asm, "Asinh");
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn asinhf(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("asinhf");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let res = Float::F32.math1(arg, asm, "Asinh");
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn acosh(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("acosh");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let res = Float::F64.math1(arg, asm, "Acosh");
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn acoshf(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("acoshf");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let res = Float::F32.math1(arg, asm, "Acosh");
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
// `System.Math.Atanh` loses the sign of `-0.0` (returns `+0.0`), but IEEE/Rust require
// `atanh(-0.0) == -0.0`. `atanh` is odd and sign-preserving across its whole domain `(-1, 1)`
// (and the ±1→±inf / out-of-domain→NaN edges keep the input's sign too), so copying the input's
// sign onto the result via `CopySign` restores the signed zero without changing any other value.
// Surfaced by coretests `num::floats::atanh::test_f{32,64}` (`atanh(-0.0)` biteq `-0.0`).
pub fn atanh(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("atanh");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let res = Float::F64.math1(arg, asm, "Atanh");
        let res = Float::F64.math2(res, arg, asm, "CopySign");
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn atanhf(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("atanhf");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let res = Float::F32.math1(arg, asm, "Atanh");
        let res = Float::F32.math2(res, arg, asm, "CopySign");
        let ret = asm.alloc_root(CILRoot::Ret(res));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn ldexp(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("ldexp");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let exp = asm.alloc_node(CILNode::LdArg(1));
        let exp = asm.alloc_node(CILNode::FloatCast {
            input: exp,
            target: Float::F64,
            is_signed: true,
        });
        let two = asm.alloc_node(Const::F64(HashableF64(2.0)));
        let pow = Float::F64.pow(two, exp, asm);
        let res_val = asm.alloc_node(CILNode::BinOp(arg, pow, BinOp::Mul));
        let ret = asm.alloc_root(CILRoot::Ret(res_val));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
/*
pub fn bitreverse_u128(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("bitreverse_u128");
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let loc0 = asm.alloc_node(CILNode::LdLoc(0));
        let ret = asm.alloc_root(CILRoot::Ret(res_val));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![(None, asm.alloc_type(Type::Int(Int::I128)))],
        }
    };
    patcher.insert(name, Box::new(generator));
} */
pub fn math(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    ldexp(asm, patcher);
    ldexpf(asm, patcher);
    sinhf(asm, patcher);
    sinh(asm, patcher);
    coshf(asm, patcher);
    cosh(asm, patcher);
    asinh(asm, patcher);
    asinhf(asm, patcher);
    acosh(asm, patcher);
    acoshf(asm, patcher);
    atanh(asm, patcher);
    atanhf(asm, patcher);
    expm1(asm, patcher);
    expm1f(asm, patcher);
    log1p(asm, patcher);
    log1pf(asm, patcher);
}
pub fn bitreverse(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    bitreverse_u32(asm, patcher);
    bitreverse_u64(asm, patcher);
    bitreverse_u128(asm, patcher);
}

fn bitreverse_u32(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("bitreverse_u32");
    let generator = move |_, asm: &mut Assembly| {
        let curr = asm.alloc_node(CILNode::LdLoc(0));
        let mut shift = 16;
        let arg0 = asm.alloc_node(CILNode::LdArg(0));
        let mut trees = vec![asm.alloc_root(CILRoot::StLoc(0, arg0))];
        let mut i = 0;
        let masks = [
            0b11111111111111110000000000000000,
            0b11111111000000001111111100000000,
            0b11110000111100001111000011110000,
            0b11001100110011001100110011001100,
            0b10101010101010101010101010101010,
        ];
        while shift > 0 {
            let mask = asm.alloc_node(Const::U32(masks[i]));
            let inv_mask = asm.alloc_node(Const::U32(!masks[i]));
            let masked = asm.alloc_node(CILNode::BinOp(curr, mask, BinOp::And));
            let inv_masked = asm.alloc_node(CILNode::BinOp(curr, inv_mask, BinOp::And));
            let shift_amount = asm.alloc_node(Const::I32(shift));
            let masked_shifted = asm.alloc_node(CILNode::BinOp(masked, shift_amount, BinOp::ShrUn));
            let inv_masked_shifted =
                asm.alloc_node(CILNode::BinOp(inv_masked, shift_amount, BinOp::Shl));
            let curr_val = asm.alloc_node(CILNode::BinOp(
                masked_shifted,
                inv_masked_shifted,
                BinOp::Or,
            ));
            trees.push(asm.alloc_root(CILRoot::StLoc(0, curr_val)));
            i += 1;
            shift /= 2;
        }
        trees.push(asm.alloc_root(CILRoot::Ret(curr)));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(trees, 0, None)],
            locals: vec![(None, asm.alloc_type(Type::Int(Int::U32)))],
        }
    };
    patcher.insert(name, Box::new(generator));
}
fn bitreverse_u64(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("bitreverse_u64");
    let generator = move |_, asm: &mut Assembly| {
        let curr = asm.alloc_node(CILNode::LdLoc(0));
        let mut shift = 32;
        let arg0 = asm.alloc_node(CILNode::LdArg(0));
        let mut trees = vec![asm.alloc_root(CILRoot::StLoc(0, arg0))];
        let mut i = 0;
        let masks = [
            0b1111111111111111111111111111111100000000000000000000000000000000,
            0b1111111111111111000000000000000011111111111111110000000000000000,
            0b1111111100000000111111110000000011111111000000001111111100000000,
            0b1111000011110000111100001111000011110000111100001111000011110000,
            0b1100110011001100110011001100110011001100110011001100110011001100,
            0b1010101010101010101010101010101010101010101010101010101010101010,
        ];
        while shift > 0 {
            let mask = asm.alloc_node(Const::U64(masks[i]));
            let inv_mask = asm.alloc_node(Const::U64(!masks[i]));
            let masked = asm.alloc_node(CILNode::BinOp(curr, mask, BinOp::And));
            let inv_masked = asm.alloc_node(CILNode::BinOp(curr, inv_mask, BinOp::And));
            let shift_amount = asm.alloc_node(Const::I32(shift));
            let masked_shifted = asm.alloc_node(CILNode::BinOp(masked, shift_amount, BinOp::ShrUn));
            let inv_masked_shifted =
                asm.alloc_node(CILNode::BinOp(inv_masked, shift_amount, BinOp::Shl));
            let curr_val = asm.alloc_node(CILNode::BinOp(
                masked_shifted,
                inv_masked_shifted,
                BinOp::Or,
            ));
            trees.push(asm.alloc_root(CILRoot::StLoc(0, curr_val)));
            i += 1;
            shift /= 2;
        }
        trees.push(asm.alloc_root(CILRoot::Ret(curr)));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(trees, 0, None)],
            locals: vec![(None, asm.alloc_type(Type::Int(Int::U64)))],
        }
    };
    patcher.insert(name, Box::new(generator));
}
fn bitreverse_u128(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("bitreverse_u128");
    let generator = move |_, asm: &mut Assembly| {
        let u128_class = ClassRef::uint_128(asm);
        let u128_class = asm[u128_class].clone();
        let mut shift = 64;
        //let op_add = asm.alloc_string("op_Addition");
        let op_and = asm.alloc_string("op_BitwiseAnd");
        let and = u128_class.static_mref(
            &[Type::Int(Int::U128), Type::Int(Int::U128)],
            Type::Int(Int::U128),
            op_and,
            asm,
        );
        let op_or = asm.alloc_string("op_BitwiseOr");
        let or = u128_class.static_mref(
            &[Type::Int(Int::U128), Type::Int(Int::U128)],
            Type::Int(Int::U128),
            op_or,
            asm,
        );
        let op_lshift = asm.alloc_string("op_LeftShift");
        let lshift = u128_class.static_mref(
            &[Type::Int(Int::U128), Type::Int(Int::I32)],
            Type::Int(Int::U128),
            op_lshift,
            asm,
        );
        let op_rshift = asm.alloc_string("op_RightShift");
        let rshift = u128_class.static_mref(
            &[Type::Int(Int::U128), Type::Int(Int::I32)],
            Type::Int(Int::U128),
            op_rshift,
            asm,
        );
        let curr = asm.alloc_node(CILNode::LdLoc(0));
        let arg0 = asm.alloc_node(CILNode::LdArg(0));
        let mut trees = vec![asm.alloc_root(CILRoot::StLoc(0, arg0))];
        let mut i = 0;
        let masks = [
            0b11111111111111111111111111111111111111111111111111111111111111110000000000000000000000000000000000000000000000000000000000000000,
            0b11111111111111111111111111111111000000000000000000000000000000001111111111111111111111111111111100000000000000000000000000000000,
            0b11111111111111110000000000000000111111111111111100000000000000001111111111111111000000000000000011111111111111110000000000000000,
            0b11111111000000001111111100000000111111110000000011111111000000001111111100000000111111110000000011111111000000001111111100000000,
            0b11110000111100001111000011110000111100001111000011110000111100001111000011110000111100001111000011110000111100001111000011110000,
            0b11001100110011001100110011001100110011001100110011001100110011001100110011001100110011001100110011001100110011001100110011001100,
            0b10101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010_u128,
        ];
        while shift > 0 {
            let curr_mask = masks[i];
            let mask = asm.alloc_node(Const::U128(curr_mask));
            let curr_mask = !masks[i];
            let inv_mask = asm.alloc_node(Const::U128(curr_mask));
            let masked = asm.alloc_node(CILNode::call(and, [curr, mask]));
            let inv_masked = asm.alloc_node(CILNode::call(and, [curr, inv_mask]));
            let shift_amount = asm.alloc_node(Const::I32(shift));
            let masked_shifted = asm.alloc_node(CILNode::call(rshift, [masked, shift_amount]));
            let inv_masked_shifted =
                asm.alloc_node(CILNode::call(lshift, [inv_masked, shift_amount]));

            let curr_val = asm.alloc_node(CILNode::call(or, [masked_shifted, inv_masked_shifted]));
            trees.push(asm.alloc_root(CILRoot::StLoc(0, curr_val)));
            i += 1;
            shift /= 2;
        }
        trees.push(asm.alloc_root(CILRoot::Ret(curr)));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(trees, 0, None)],
            locals: vec![(None, asm.alloc_type(Type::Int(Int::U128)))],
        }
    };
    patcher.insert(name, Box::new(generator));
}
