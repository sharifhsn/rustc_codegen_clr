use crate::{
    Assembly, BasicBlock, BinOp, CILNode, CILRoot, ClassRef, Const, Float, Int, MethodImpl,
    MethodRef, Type,
    asm::MissingMethodPatcher,
    bimap::Interned,
    cilnode::MethodKind,
    hashable::{HashableF32, HashableF64},
};

fn int_extreme(
    asm: &mut Assembly,
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    int: Int,
    operation: &'static str,
) -> Interned<CILNode> {
    let math = ClassRef::math(asm);
    let operation = asm.alloc_string(operation);
    let sig = asm.sig([Type::Int(int), Type::Int(int)], Type::Int(int));
    let mref = asm.alloc_methodref(MethodRef::new(
        math,
        operation,
        sig,
        MethodKind::Static,
        vec![].into(),
    ));
    asm.alloc_node(CILNode::call(mref, [lhs, rhs]))
}

pub fn int_max(
    asm: &mut Assembly,
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    int: Int,
) -> Interned<CILNode> {
    int_extreme(asm, lhs, rhs, int, "Max")
}

pub fn int_min(
    asm: &mut Assembly,
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    int: Int,
) -> Interned<CILNode> {
    int_extreme(asm, lhs, rhs, int, "Min")
}

fn register_unary_math(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    name: &'static str,
    float: Float,
    operation: &'static str,
) {
    let name = asm.alloc_string(name);
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let result = float.math1(arg, asm, operation);
        let ret = asm.alloc_root(CILRoot::Ret(result));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

fn register_ldexp(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    name: &'static str,
    float: Float,
) {
    let name = asm.alloc_string(name);
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let exp = asm.alloc_node(CILNode::LdArg(1));
        let exp = asm.alloc_node(CILNode::FloatCast {
            input: exp,
            target: float,
            is_signed: true,
        });
        let two = asm.alloc_node(match float {
            Float::F32 => Const::F32(HashableF32(2.0)),
            Float::F64 => Const::F64(HashableF64(2.0)),
            _ => unreachable!("ldexp only supports f32/f64"),
        });
        let pow = float.pow(two, exp, asm);
        let result = asm.alloc_node(CILNode::BinOp(arg, pow, BinOp::Mul));
        let ret = asm.alloc_root(CILRoot::Ret(result));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

fn register_binary_math(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    name: &'static str,
    float: Float,
    operation: &'static str,
) {
    let name = asm.alloc_string(name);
    let generator = move |_, asm: &mut Assembly| {
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let result = float.math2(lhs, rhs, asm, operation);
        let ret = asm.alloc_root(CILRoot::Ret(result));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

macro_rules! register_ldexp_fn {
    ($name:ident, $float:expr) => {
        pub fn $name(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
            register_ldexp(asm, patcher, stringify!($name), $float)
        }
    };
}

macro_rules! register_unary_math_fn {
    ($name:ident, $float:expr, $operation:literal) => {
        pub fn $name(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
            register_unary_math(asm, patcher, stringify!($name), $float, $operation)
        }
    };
}

macro_rules! register_binary_math_fn {
    ($name:ident, $float:expr, $operation:literal) => {
        pub fn $name(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
            register_binary_math(asm, patcher, stringify!($name), $float, $operation)
        }
    };
}

macro_rules! register_unary_transform_fn {
    ($name:ident, $float:expr, $body:ident) => {
        pub fn $name(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
            register_unary_transform(asm, patcher, stringify!($name), $float, $body)
        }
    };
}

register_ldexp_fn!(ldexpf, Float::F32);
register_unary_math_fn!(sinhf, Float::F32, "Sinh");
register_unary_math_fn!(sinh, Float::F64, "Sinh");
register_unary_math_fn!(coshf, Float::F32, "Cosh");
register_unary_math_fn!(cosh, Float::F64, "Cosh");
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
type UnaryFloatBody = fn(Float, Interned<CILNode>, &mut Assembly) -> Interned<CILNode>;

fn register_unary_transform(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    name: &'static str,
    float: Float,
    body: UnaryFloatBody,
) {
    let name = asm.alloc_string(name);
    let generator = move |_, asm: &mut Assembly| {
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let result = body(float, arg, asm);
        let ret = asm.alloc_root(CILRoot::Ret(result));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

fn float_one(float: Float) -> Const {
    match float {
        Float::F32 => Const::F32(HashableF32(1.0)),
        Float::F64 => Const::F64(HashableF64(1.0)),
        _ => unreachable!("only f32/f64 math shims are registered"),
    }
}

fn expm1_body(float: Float, arg: Interned<CILNode>, asm: &mut Assembly) -> Interned<CILNode> {
    let exp = float.math1(arg, asm, "Exp");
    let one = asm.alloc_node(float_one(float));
    let result = asm.alloc_node(CILNode::BinOp(exp, one, BinOp::Sub));
    float.math2(result, arg, asm, "CopySign")
}

fn log1p_body(float: Float, arg: Interned<CILNode>, asm: &mut Assembly) -> Interned<CILNode> {
    let one = asm.alloc_node(float_one(float));
    let one_plus_arg = asm.alloc_node(CILNode::BinOp(one, arg, BinOp::Add));
    let log = float.math1(one_plus_arg, asm, "Log");
    float.math2(log, arg, asm, "CopySign")
}

fn atanh_body(float: Float, arg: Interned<CILNode>, asm: &mut Assembly) -> Interned<CILNode> {
    let result = float.math1(arg, asm, "Atanh");
    float.math2(result, arg, asm, "CopySign")
}

register_unary_transform_fn!(expm1, Float::F64, expm1_body);
register_unary_transform_fn!(expm1f, Float::F32, expm1_body);
register_unary_transform_fn!(log1p, Float::F64, log1p_body);
register_unary_transform_fn!(log1pf, Float::F32, log1p_body);
// Inverse hyperbolic functions. Like `sinh`/`cosh`, these are libm externs (`acosh`/`asinh`/`atanh`
// + f32 forms) that `core`/`std` reach through `cmath`; with no implementation the linker leaves
// them unresolved and the first call throws `missing method <name>` — which unwinds the test thread
// (caught → FAILED) or crosses a nounwind boundary (→ process abort). Surfaced by coretests
// `num::floats::{acosh,asinh,atanh}::test_f{32,64}`. .NET has exact equivalents
// (`System.Math.Acosh/Asinh/Atanh`, `System.MathF.*`), so map them directly via `math1` — no
// precision-losing composition needed (unlike `expm1`/`log1p`).
register_unary_math_fn!(asinh, Float::F64, "Asinh");
register_unary_math_fn!(asinhf, Float::F32, "Asinh");
register_unary_math_fn!(acosh, Float::F64, "Acosh");
register_unary_math_fn!(acoshf, Float::F32, "Acosh");
// `System.Math.Atanh` loses the sign of `-0.0` (returns `+0.0`), but IEEE/Rust require
// `atanh(-0.0) == -0.0`. `atanh` is odd and sign-preserving across its whole domain `(-1, 1)`
// (and the ±1→±inf / out-of-domain→NaN edges keep the input's sign too), so copying the input's
// sign onto the result via `CopySign` restores the signed zero without changing any other value.
// Surfaced by coretests `num::floats::atanh::test_f{32,64}` (`atanh(-0.0)` biteq `-0.0`).
register_unary_transform_fn!(atanh, Float::F64, atanh_body);
register_unary_transform_fn!(atanhf, Float::F32, atanh_body);
register_ldexp_fn!(ldexp, Float::F64);
register_unary_math_fn!(expf, Float::F32, "Exp");
register_unary_math_fn!(exp, Float::F64, "Exp");
register_unary_math_fn!(exp2f, Float::F32, "Exp2");
register_unary_math_fn!(exp2, Float::F64, "Exp2");
register_unary_math_fn!(logf, Float::F32, "Log");
register_unary_math_fn!(log, Float::F64, "Log");
register_unary_math_fn!(log2f, Float::F32, "Log2");
register_unary_math_fn!(log2, Float::F64, "Log2");
register_unary_math_fn!(log10f, Float::F32, "Log10");
register_unary_math_fn!(log10, Float::F64, "Log10");
register_binary_math_fn!(powf, Float::F32, "Pow");
register_binary_math_fn!(pow, Float::F64, "Pow");
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
    expf(asm, patcher);
    exp(asm, patcher);
    exp2f(asm, patcher);
    exp2(asm, patcher);
    logf(asm, patcher);
    log(asm, patcher);
    log2f(asm, patcher);
    log2(asm, patcher);
    log10f(asm, patcher);
    log10(asm, patcher);
    powf(asm, patcher);
    pow(asm, patcher);
}
pub fn bitreverse(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    bitreverse_u32(asm, patcher);
    bitreverse_u64(asm, patcher);
    bitreverse_u128(asm, patcher);
}

fn bitreverse_u32(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    bitreverse_word("bitreverse_u32", Int::U32, asm, patcher);
}
fn bitreverse_u64(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    bitreverse_word("bitreverse_u64", Int::U64, asm, patcher);
}

fn bitreverse_word(name: &str, int: Int, asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string(name);
    let generator = move |_, asm: &mut Assembly| {
        let curr = asm.alloc_node(CILNode::LdLoc(0));
        let arg0 = asm.alloc_node(CILNode::LdArg(0));
        let mut roots = vec![asm.alloc_root(CILRoot::StLoc(0, arg0))];
        let mut shift = int.bits().unwrap_or(64) as i32 / 2;
        let bits = int.bits().unwrap_or(64) as i32;
        while shift > 0 {
            let group = (1_u64 << shift) - 1;
            let mut mask_value = 0_u64;
            let mut position = shift;
            while position < bits {
                mask_value |= group << position;
                position += shift * 2;
            }
            let (mask, inverse) = match int {
                Int::U32 => (
                    Const::U32(mask_value as u32),
                    Const::U32(!(mask_value as u32)),
                ),
                Int::U64 => (Const::U64(mask_value), Const::U64(!mask_value)),
                _ => unreachable!("bitreverse_word only supports u32/u64"),
            };
            let mask = asm.alloc_node(mask);
            let inverse = asm.alloc_node(inverse);
            let masked = asm.alloc_node(CILNode::BinOp(curr, mask, BinOp::And));
            let inverse_masked = asm.alloc_node(CILNode::BinOp(curr, inverse, BinOp::And));
            let amount = asm.alloc_node(Const::I32(shift));
            let masked_shifted = asm.alloc_node(CILNode::BinOp(masked, amount, BinOp::ShrUn));
            let inverse_shifted =
                asm.alloc_node(CILNode::BinOp(inverse_masked, amount, BinOp::Shl));
            let value = asm.alloc_node(CILNode::BinOp(masked_shifted, inverse_shifted, BinOp::Or));
            roots.push(asm.alloc_root(CILRoot::StLoc(0, value)));
            shift /= 2;
        }
        roots.push(asm.alloc_root(CILRoot::Ret(curr)));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals: vec![(None, asm.alloc_type(Type::Int(int)))],
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
