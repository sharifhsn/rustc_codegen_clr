use crate::{
    asm::MissingMethodPatcher, cilnode::ExtendKind, Assembly, BasicBlock, CILNode, CILRoot, Float,
    Int, MethodImpl,
};
fn clampy_float_to_int(
    asm: &mut Assembly,
    int: Int,
    float: Float,
    patcher: &mut MissingMethodPatcher,
) {
    let name = format!("cast_{}_{}", float.name(), int.name());
    let name = asm.alloc_string(name);
    let generator = move |_, asm: &mut Assembly| {
        // Consts
        let imax = int.max(asm);
        let imax = asm.alloc_node(imax);
        let fmax = asm.alloc_node(CILNode::FloatCast {
            input: imax,
            target: float,
            is_signed: int.is_signed(),
        });
        let imin = int.min(asm);
        let imin = asm.alloc_node(imin);
        let fmin = asm.alloc_node(CILNode::FloatCast {
            input: imin,
            target: float,
            is_signed: int.is_signed(),
        });
        // Args
        let ld_arg_0 = asm.alloc_node(CILNode::LdArg(0));
        let clamped = float.clamp(ld_arg_0, fmin, fmax, asm);
        // Return the cast if in range.
        let cast = asm.alloc_node(CILNode::IntCast {
            input: clamped,
            target: int,
            extend: if int.is_signed() {
                ExtendKind::SignExtend
            } else {
                ExtendKind::ZeroExtend
            },
        });
        let return_cast = asm.alloc_root(CILRoot::Ret(cast));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![return_cast], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
fn float_to_int(asm: &mut Assembly, int: Int, float: Float, patcher: &mut MissingMethodPatcher) {
    let name = format!("cast_{}_{}", float.name(), int.name());
    let name = asm.alloc_string(name);
    let generator = move |_, asm: &mut Assembly| {
        // Consts
        let imax = int.max(asm);
        let imax = asm.alloc_node(imax);
        let fmax = asm.alloc_node(CILNode::FloatCast {
            input: imax,
            target: float,
            is_signed: int.is_signed(),
        });
        let imin = int.min(asm);
        let imin = asm.alloc_node(imin);
        let fmin = asm.alloc_node(CILNode::FloatCast {
            input: imin,
            target: float,
            is_signed: int.is_signed(),
        });
        // Args
        let ld_arg_0 = asm.alloc_node(CILNode::LdArg(0));

        // NaN maps to 0 in Rust's `as` casts. A value is NaN iff it is unequal to itself, so
        // jump to block 3 (returns 0) when `arg != arg`. This must be checked before the
        // overflow/underflow branches, since the unordered `bge.un`/`ble.un` comparisons below
        // would otherwise (incorrectly) send NaN to imax/imin.
        let is_nan = asm.alloc_root(CILRoot::Branch(Box::new((
            3,
            0,
            Some(crate::cilroot::BranchCond::Ne(ld_arg_0, ld_arg_0)),
        ))));
        // If arg is smaller than max, pass. Else jump to block 1.
        let overflow = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(crate::cilroot::BranchCond::Ge(
                ld_arg_0,
                fmax,
                crate::cilroot::CmpKind::Unordered,
            )),
        ))));
        // If arg is bigger than min, pass. Else jump to block 2.
        let underflow = asm.alloc_root(CILRoot::Branch(Box::new((
            2,
            0,
            Some(crate::cilroot::BranchCond::Le(
                ld_arg_0,
                fmin,
                crate::cilroot::CmpKind::Unordered,
            )),
        ))));
        // Return the cast if in range.
        let cast = asm.alloc_node(CILNode::IntCast {
            input: ld_arg_0,
            target: int,
            extend: if int.is_signed() {
                ExtendKind::SignExtend
            } else {
                ExtendKind::ZeroExtend
            },
        });
        let return_cast = asm.alloc_root(CILRoot::Ret(cast));
        // Zero of the target int type, returned for NaN inputs.
        let izero = asm.alloc_node(crate::Const::I32(0));
        let izero = asm.alloc_node(CILNode::IntCast {
            input: izero,
            target: int,
            extend: ExtendKind::ZeroExtend,
        });
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![is_nan, overflow, underflow, return_cast], 0, None),
                BasicBlock::new(vec![asm.alloc_root(CILRoot::Ret(imax))], 1, None),
                BasicBlock::new(vec![asm.alloc_root(CILRoot::Ret(imin))], 2, None),
                BasicBlock::new(vec![asm.alloc_root(CILRoot::Ret(izero))], 3, None),
            ],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn insert_casts(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let floats = [Float::F32, Float::F64];
    let ints = [
        Int::U32,
        Int::I32,
        Int::U64,
        Int::I64,
        Int::ISize,
        Int::USize,
    ];
    for int in ints {
        for float in floats {
            float_to_int(asm, int, float, patcher);
        }
    }
    // Those can be done in a more optimal way.
    let ints = [Int::U8, Int::I8, Int::U16, Int::I16];
    for int in ints {
        for float in floats {
            clampy_float_to_int(asm, int, float, patcher);
        }
    }
}
