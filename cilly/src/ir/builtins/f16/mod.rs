use crate::{
    Assembly, BasicBlock, BinOp, CILNode, CILRoot, ClassRef, Float, Interned, MethodImpl,
    MethodRef, Type, asm::MissingMethodPatcher, cilnode::MethodKind,
};

/// Converts `input` (a 16-bit float, `System.Half`) to the wider float `target` (`f32`/`f64`),
/// using `System.Half`'s explicit conversion operators. `target` must not be `F16`.
pub fn f16_to_float(
    asm: &mut Assembly,
    input: Interned<CILNode>,
    target: Float,
) -> Interned<CILNode> {
    assert!(
        matches!(target, Float::F32 | Float::F64),
        "f16_to_float target must be a wider float, got {target:?}"
    );
    let half = ClassRef::half(asm);
    let name = asm.alloc_string("op_Explicit");
    let sig = asm.sig([Type::Float(Float::F16)], Type::Float(target));
    let mref = asm.alloc_methodref(MethodRef::new(
        half,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ));
    asm.alloc_node(CILNode::call(mref, [input]))
}

/// Converts `input` (a wider float `src`, `f32`/`f64`) to a 16-bit float (`System.Half`),
/// using `System.Half`'s explicit conversion operators. `src` must not be `F16`.
pub fn float_to_f16(asm: &mut Assembly, input: Interned<CILNode>, src: Float) -> Interned<CILNode> {
    assert!(
        matches!(src, Float::F32 | Float::F64),
        "float_to_f16 src must be a wider float, got {src:?}"
    );
    let half = ClassRef::half(asm);
    let name = asm.alloc_string("op_Explicit");
    let sig = asm.sig([Type::Float(src)], Type::Float(Float::F16));
    let mref = asm.alloc_methodref(MethodRef::new(
        half,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ));
    asm.alloc_node(CILNode::call(mref, [input]))
}
/// Implements a given BinOp directly, as an operation on two floating-point args.
fn op_direct(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    lhs: Float,
    _rhs: Float,
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
/// Implements a given binop indirectly, delegating it to a .NET impl.
fn op_indirect(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    lhs_type: Float,
    rhs_type: Float,
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
            &[Type::Float(lhs_type), Type::Float(rhs_type)],
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
/// Generates all ops operating on a 16 bit float.
pub fn generate_f16_ops(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, direct: bool) {
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
    const CMPS: [BinOp; 3] = [BinOp::Lt, BinOp::Gt, BinOp::Eq];
    let ints = [Float::F16];
    for op in OPS {
        for float in ints {
            if direct {
                op_direct(asm, patcher, float, float, op);
            } else {
                op_indirect(asm, patcher, float, float, op, Type::Float(float));
            }
        }
    }

    for op in CMPS {
        for float in ints {
            if direct {
                op_direct(asm, patcher, float, float, op);
            } else {
                op_indirect(asm, patcher, float, float, op, Type::Bool);
            }
        }
    }
}
