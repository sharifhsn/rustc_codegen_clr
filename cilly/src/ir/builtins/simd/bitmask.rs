use crate::{
    Assembly, BasicBlock, BinOp, CILNode, CILRoot, Const, Int, MethodImpl, MethodRef, Type,
    asm::MissingMethodPatcher, bimap::Interned, cilnode::ExtendKind,
};

use super::binop::simd_lane_info;
use crate::tpe::simd::SIMDElem;

/// Builds Rust's `simd_bitmask`: bit `i` of the scalar result is the most-significant bit of
/// lane `i`.
///
/// Keep this target-independent. Besides avoiding a dependency on a particular BCL intrinsic,
/// the spill-and-index form also covers the fixed-array representation used when a vector width
/// has no `System.Runtime.Intrinsics.VectorN<T>` counterpart. The optimizer can still fold the
/// straight-line pack for the managed-vector case.
fn most_significant_bits_body(mref: Interned<MethodRef>, asm: &mut Assembly) -> MethodImpl {
    let sig = asm[asm[mref].sig()].clone();
    let output = sig
        .output()
        .as_int()
        .expect("simd_get_most_significant_bits must return an integer");
    let (elem, count) = simd_lane_info(sig.inputs()[0], asm)
        .expect("simd_get_most_significant_bits input is not a vector");
    let SIMDElem::Int(elem) = elem else {
        panic!("simd_get_most_significant_bits requires integer lanes");
    };
    let elem_bits = elem.bits().unwrap_or(64);
    assert!(
        elem_bits <= 64 && count <= 64,
        "simd_get_most_significant_bits supports at most 64 lanes of at most 64 bits"
    );

    let elem_type = Type::Int(elem);
    let elem_type_idx = asm.alloc_type(elem_type);
    let src = asm.alloc_node(CILNode::LdArgA(0));
    let src = asm.cast_ptr(src, elem_type);
    let mut packed = asm.alloc_node(Const::U64(0));

    for lane_index in 0..count {
        let slot = asm.offset(src, Const::USize(lane_index), elem_type);
        let lane = asm.alloc_node(CILNode::LdInd {
            addr: slot,
            tpe: elem_type_idx,
            volatile: false,
        });
        // Normalize every lane to one unsigned stack width before shifting. Sign extension is
        // intentional for signed lanes: after a logical right shift and `& 1`, both 0x80_i8 and
        // 0x80_u8 yield exactly one, while lanes with a clear MSB yield zero.
        let extend = if elem.is_signed() {
            ExtendKind::SignExtend
        } else {
            ExtendKind::ZeroExtend
        };
        let lane = asm.int_cast(lane, Int::U64, extend);
        let msb_shift = asm.alloc_node(Const::I32(i32::from(elem_bits) - 1));
        let msb = asm.biop(lane, msb_shift, BinOp::ShrUn);
        let one = asm.alloc_node(Const::U64(1));
        let bit = asm.biop(msb, one, BinOp::And);
        let result_shift = asm.alloc_node(Const::I32(
            i32::try_from(lane_index).expect("SIMD lane index exceeds i32"),
        ));
        let bit = asm.biop(bit, result_shift, BinOp::Shl);
        packed = asm.biop(packed, bit, BinOp::Or);
    }

    let packed = asm.int_cast(packed, output, ExtendKind::ZeroExtend);
    let ret = asm.alloc_root(CILRoot::Ret(packed));
    MethodImpl::MethodBody {
        blocks: vec![BasicBlock::new(vec![ret], 0, None)],
        locals: vec![],
    }
}

/// Registers the shared scalar bit-packer for both the .NET SIMD set and the fallback SIMD set.
pub(super) fn register_most_significant_bits(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
) {
    let name = asm.alloc_string("simd_get_most_significant_bits");
    patcher.insert(
        name,
        Box::new(|mref, asm| most_significant_bits_body(mref, asm)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Access, MethodDef, MethodRef, cilnode::MethodKind, class::ClassDefIdx,
        tpe::simd::SIMDVector,
    };

    fn generated_body(elem: Int) -> (Assembly, Interned<CILNode>) {
        let mut asm = Assembly::default();
        let vector = Type::SIMDVector(SIMDVector::new(elem.into(), 16));
        let sig = asm.sig([vector], Type::Int(Int::I32));
        let method = MethodRef::new(
            *asm.main_module(),
            asm.alloc_string("simd_get_most_significant_bits"),
            sig,
            MethodKind::Static,
            [].into(),
        );
        let method = asm.alloc_methodref(method);
        let body = most_significant_bits_body(method, &mut asm);
        let inspect = body.clone();
        let main_module = *asm.main_module();
        let method_name = asm[method].name();
        asm.new_method(MethodDef::new(
            Access::Public,
            ClassDefIdx(main_module),
            method_name,
            sig,
            MethodKind::Static,
            body,
            vec![None],
        ));
        assert_eq!(asm.typecheck(), 0);

        let MethodImpl::MethodBody { blocks, locals } = inspect else {
            unreachable!()
        };
        assert!(locals.is_empty());
        assert_eq!(blocks.len(), 1);
        let roots = blocks[0].roots();
        assert_eq!(roots.len(), 1);
        let CILRoot::Ret(value) = asm[*roots.first().unwrap()] else {
            panic!("bit-pack builtin did not return its packed value");
        };
        (asm, value)
    }

    #[test]
    fn bitmask_packer_is_registered_for_dotnet_and_fallback_simd() {
        for register in [
            super::super::simd as fn(&mut Assembly, &mut MissingMethodPatcher),
            super::super::fallback_simd,
        ] {
            let mut asm = Assembly::default();
            let mut patcher = MissingMethodPatcher::default();
            register(&mut asm, &mut patcher);
            let name = asm.alloc_string("simd_get_most_significant_bits");
            assert!(patcher.contains_key(&name));
        }
    }

    fn lane_index(addr: Interned<CILNode>, asm: &Assembly) -> Option<usize> {
        match &asm[addr] {
            CILNode::Const(c) => match c.as_ref() {
                Const::USize(index) => usize::try_from(*index).ok(),
                _ => None,
            },
            node => node
                .child_nodes()
                .into_iter()
                .find_map(|child| lane_index(child, asm)),
        }
    }

    /// Evaluate the deliberately small integer-expression subset emitted by the bit-packer. This
    /// keeps the regression at the Cilly unit layer while checking the actual generated IR rather
    /// than a second, test-only implementation of the packing algorithm.
    fn eval(node: Interned<CILNode>, asm: &Assembly, lanes: &[u8; 16], elem: Int) -> u64 {
        match &asm[node] {
            CILNode::Const(c) => match c.as_ref() {
                Const::I32(value) => *value as u64,
                Const::U64(value) => *value,
                other => panic!("unexpected bit-pack constant {other:?}"),
            },
            CILNode::LdInd { addr, .. } => {
                let index = lane_index(*addr, asm).expect("lane load has no constant lane index");
                u64::from(lanes[index])
            }
            CILNode::IntCast {
                input,
                target,
                extend,
            } => {
                let value = eval(*input, asm, lanes, elem);
                if *target == Int::U64 && *extend == ExtendKind::SignExtend && elem == Int::I8 {
                    (i64::from(value as u8 as i8)) as u64
                } else {
                    match target.bits().unwrap_or(64) {
                        64 => value,
                        bits => value & ((1_u64 << bits) - 1),
                    }
                }
            }
            CILNode::BinOp(lhs, rhs, op) => {
                let lhs = eval(*lhs, asm, lanes, elem);
                let rhs = eval(*rhs, asm, lanes, elem);
                match op {
                    BinOp::ShrUn => lhs >> rhs,
                    BinOp::And => lhs & rhs,
                    BinOp::Shl => lhs << rhs,
                    BinOp::Or => lhs | rhs,
                    other => panic!("unexpected bit-pack operation {other:?}"),
                }
            }
            other => panic!("unexpected bit-pack node {other:?}"),
        }
    }

    #[test]
    fn i8x16_packs_negative_lane_sign_bits() {
        let lanes = [
            0x80, 0x00, 0xff, 0x7f, 0xfe, 0x01, 0x81, 0x7e, 0x90, 0x10, 0xf0, 0x70, 0xaa, 0x2a,
            0xcc, 0x4c,
        ];
        let (asm, result) = generated_body(Int::I8);
        assert_eq!(eval(result, &asm, &lanes, Int::I8), 0x5555);
    }

    #[test]
    fn u8x16_packs_high_bits_without_signed_conversion() {
        let lanes = [
            0x00, 0x80, 0x7f, 0xff, 0x01, 0xfe, 0x7e, 0x81, 0x10, 0x90, 0x70, 0xf0, 0x2a, 0xaa,
            0x4c, 0xcc,
        ];
        let (asm, result) = generated_body(Int::U8);
        assert_eq!(eval(result, &asm, &lanes, Int::U8), 0xaaaa);
    }
}
