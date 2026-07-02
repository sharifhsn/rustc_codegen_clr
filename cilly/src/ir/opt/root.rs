use super::inline::inline_trivial_call_root;

use super::super::{
    cilroot::BranchCond, method::LocalDef, BinOp, CILNode, CILRoot, Const, FnSig, Type,
};
pub use super::opt_fuel::OptFuel;
use super::opt_if_fuel;
pub use super::side_effect::*;
use crate::bimap::Interned;
use crate::cilroot::CmpKind;
use crate::Assembly;

/// Pick the `CmpKind` for a comparison produced by *negating* an ordered/signed comparison-branch
/// (`!(a < b)` → `a >= b`, `!(a > b)` → `a <= b`, and their unordered/unsigned `…Un` forms). The
/// fused branch (`bge`/`ble`) is only correct for FLOATS when it uses the UNORDERED complement
/// (`bge.un`/`ble.un`): ordered `clt`/`cgt` are false for NaN, so e.g. `!(a < b)` must still branch
/// when an operand is NaN — which only `bge.un` does, not `bge`. For integers the signed/ordered
/// complement (`int_kind`) is correct. `BinOp::Lt`/`Gt` (and `…Un`) are shared by float-ordered and
/// int-signed comparisons, so the kind is chosen from the operand's type; if the type can't be
/// determined, `int_kind` (the historical, integer-correct choice) is used.
fn negation_cmp_kind(
    operand: Interned<CILNode>,
    int_kind: CmpKind,
    float_kind: CmpKind,
    sig: Interned<FnSig>,
    locals: &[LocalDef],
    asm: &mut Assembly,
) -> CmpKind {
    let is_float = asm
        .get_node(operand)
        .clone()
        .typecheck(sig, locals, asm)
        .map(|t| matches!(t, Type::Float(_)))
        .unwrap_or(false);
    if is_float {
        float_kind
    } else {
        int_kind
    }
}
pub fn root_opt(
    root: CILRoot,
    asm: &mut Assembly,
    root_fuel: &mut OptFuel,
    cache: &mut SideEffectInfoCache,
    locals: &[LocalDef],
    sig: Interned<FnSig>,
) -> CILRoot {
    match root {
        CILRoot::Pop(pop) => match asm.get_node(pop) {
            CILNode::LdLoc(_) => CILRoot::Nop,
            _ => {
                let has_side_effects = cache.has_side_effects(pop, asm);
                if has_side_effects {
                    root
                } else {
                    CILRoot::Nop
                }
            }
        },
        CILRoot::Call(info) => inline_trivial_call_root(info.0, &info.1, root_fuel, asm),

        // As with the `LdInd`->`LdLoc` fold in `opt_node.rs`: only collapse `stind(ldloca X, v)`
        // to `stloc X, v` when the store is NOT volatile. A `volatile.` store is a release fence
        // (ECMA-335 I.12.6.8) — folding it into a plain `stloc` would silently drop that fence,
        // which would be unsound for `volatile_store`/`atomic_store` against a directly-owned
        // local's address (`info.3` is the volatile flag).
        CILRoot::StInd(ref info) => match asm.get_node(info.0) {
            CILNode::LdLocA(loc) if !info.3 && asm[locals[*loc as usize].1] == info.2 => {
                CILRoot::StLoc(*loc, info.1)
            }
            _ => root,
        },
        CILRoot::SetField(info) => {
            let (field, mut addr, val) = info.as_ref();
            if let CILNode::RefToPtr(inner) = asm[addr] {
                addr = inner;
            }
            CILRoot::SetField(Box::new((*field, addr, *val)))
        }
        CILRoot::InitObj(addr, tpe) => opt_init_obj(addr, tpe, asm, root_fuel),
        CILRoot::Branch(ref info) => {
            let (target, sub_target, cond) = info.as_ref();
            match cond {
                Some(BranchCond::False(cond)) => {
                    // `.clone()` so the `asm` borrow is released — the negation arms below need a
                    // mutable `asm` to typecheck the operand and pick ordered-vs-unordered.
                    match asm.get_node(*cond).clone() {
                        CILNode::Const(cst) => match cst.as_ref() {
                            Const::Bool(false) => opt_if_fuel(
                                CILRoot::Branch(Box::new((*target, *sub_target, None))),
                                root,
                                root_fuel,
                            ),
                            Const::Bool(true) => opt_if_fuel(CILRoot::Nop, root, root_fuel),
                            _ => root,
                        },
                        // a == b is false <=> a != b. `Ne` lowers to `bne.un`, which is the correct
                        // negation of ordered `ceq` for both ints and floats (NaN ≠ NaN is true), so
                        // no kind selection is needed here.
                        CILNode::BinOp(ref lhs, ref rhs, BinOp::Eq) => opt_if_fuel(
                            {
                                CILRoot::Branch(Box::new((
                                    *target,
                                    *sub_target,
                                    Some(BranchCond::Ne(*lhs, *rhs)),
                                )))
                            },
                            root,
                            root_fuel,
                        ),
                        // a > b is false <=> a <= b
                        CILNode::BinOp(ref lhs, ref rhs, BinOp::Gt) => {
                            let kind = negation_cmp_kind(
                                *lhs,
                                CmpKind::Ordered,
                                CmpKind::Unordered,
                                sig,
                                locals,
                                asm,
                            );
                            opt_if_fuel(
                                CILRoot::Branch(Box::new((
                                    *target,
                                    *sub_target,
                                    Some(BranchCond::Le(*lhs, *rhs, kind)),
                                ))),
                                root,
                                root_fuel,
                            )
                        }
                        CILNode::BinOp(ref lhs, ref rhs, BinOp::GtUn) => {
                            let kind = negation_cmp_kind(
                                *lhs,
                                CmpKind::Unordered,
                                CmpKind::Ordered,
                                sig,
                                locals,
                                asm,
                            );
                            opt_if_fuel(
                                CILRoot::Branch(Box::new((
                                    *target,
                                    *sub_target,
                                    Some(BranchCond::Le(*lhs, *rhs, kind)),
                                ))),
                                root,
                                root_fuel,
                            )
                        }
                        // a < b is false <=> a >= b
                        CILNode::BinOp(ref lhs, ref rhs, BinOp::Lt) => {
                            let kind = negation_cmp_kind(
                                *lhs,
                                CmpKind::Ordered,
                                CmpKind::Unordered,
                                sig,
                                locals,
                                asm,
                            );
                            opt_if_fuel(
                                CILRoot::Branch(Box::new((
                                    *target,
                                    *sub_target,
                                    Some(BranchCond::Ge(*lhs, *rhs, kind)),
                                ))),
                                root,
                                root_fuel,
                            )
                        }
                        CILNode::BinOp(ref lhs, ref rhs, BinOp::LtUn) => {
                            let kind = negation_cmp_kind(
                                *lhs,
                                CmpKind::Unordered,
                                CmpKind::Ordered,
                                sig,
                                locals,
                                asm,
                            );
                            opt_if_fuel(
                                CILRoot::Branch(Box::new((
                                    *target,
                                    *sub_target,
                                    Some(BranchCond::Ge(*lhs, *rhs, kind)),
                                ))),
                                root,
                                root_fuel,
                            )
                        }
                        //CILNode::IntCast { input, target, extend }
                        _ => root,
                    }
                }
                Some(BranchCond::True(cond)) => match asm.get_node(*cond) {
                    // a == b  is true <=> a == b
                    CILNode::BinOp(lhs, rhs, BinOp::Eq) => opt_if_fuel(
                        CILRoot::Branch(Box::new((
                            *target,
                            *sub_target,
                            Some(BranchCond::Eq(*lhs, *rhs)),
                        ))),
                        root,
                        root_fuel,
                    ),
                    CILNode::BinOp(lhs, rhs, BinOp::GtUn) => opt_if_fuel(
                        {
                            CILRoot::Branch(Box::new((
                                *target,
                                *sub_target,
                                Some(BranchCond::Gt(*lhs, *rhs, CmpKind::Unordered)),
                            )))
                        },
                        root,
                        root_fuel,
                    ),
                    CILNode::BinOp(lhs, rhs, BinOp::Gt) => opt_if_fuel(
                        CILRoot::Branch(Box::new((
                            *target,
                            *sub_target,
                            Some(BranchCond::Gt(*lhs, *rhs, CmpKind::Ordered)),
                        ))),
                        root,
                        root_fuel,
                    ),
                    CILNode::BinOp(lhs, rhs, BinOp::LtUn) => opt_if_fuel(
                        {
                            CILRoot::Branch(Box::new((
                                *target,
                                *sub_target,
                                Some(BranchCond::Lt(*lhs, *rhs, CmpKind::Unordered)),
                            )))
                        },
                        root,
                        root_fuel,
                    ),
                    CILNode::BinOp(lhs, rhs, BinOp::Lt) => opt_if_fuel(
                        CILRoot::Branch(Box::new((
                            *target,
                            *sub_target,
                            Some(BranchCond::Lt(*lhs, *rhs, CmpKind::Ordered)),
                        ))),
                        root,
                        root_fuel,
                    ),
                    _ => root,
                },
                Some(BranchCond::Ne(lhs, rhs)) => {
                    match (asm.get_node(*lhs), asm.get_node(*rhs)) {
                        (_, CILNode::Const(cst)) => match cst.as_ref() {
                            // val != false <=> val is true
                            Const::Bool(false)
                            | Const::ISize(0)
                            | Const::USize(0)
                            | Const::I64(0)
                            | Const::U64(0)
                            | Const::I32(0)
                            | Const::U32(0)
                            | Const::I16(0)
                            | Const::U16(0)
                            | Const::I8(0)
                            | Const::U8(0) => opt_if_fuel(
                                CILRoot::Branch(Box::new((
                                    *target,
                                    *sub_target,
                                    Some(BranchCond::True(*lhs)),
                                ))),
                                root,
                                root_fuel,
                            ),
                            // val != true <=> val is false
                            Const::Bool(true) => opt_if_fuel(
                                CILRoot::Branch(Box::new((
                                    *target,
                                    *sub_target,
                                    Some(BranchCond::False(*lhs)),
                                ))),
                                root,
                                root_fuel,
                            ),
                            _ => root,
                        },
                        (CILNode::Const(cst), _) => match cst.as_ref() {
                            // val != false <=> val is true
                            Const::Bool(false)
                            | Const::ISize(0)
                            | Const::USize(0)
                            | Const::I64(0)
                            | Const::U64(0)
                            | Const::I32(0)
                            | Const::U32(0)
                            | Const::I16(0)
                            | Const::U16(0)
                            | Const::I8(0)
                            | Const::U8(0) => opt_if_fuel(
                                CILRoot::Branch(Box::new((
                                    *target,
                                    *sub_target,
                                    Some(BranchCond::True(*rhs)),
                                ))),
                                root,
                                root_fuel,
                            ),
                            _ => root,
                        },
                        _ => root,
                    }
                }
                Some(BranchCond::Eq(lhs, rhs)) => match (asm.get_node(*lhs), asm.get_node(*rhs)) {
                    (_, CILNode::Const(cst)) => match cst.as_ref() {
                        Const::Bool(false)
                        | Const::ISize(0)
                        | Const::USize(0)
                        | Const::I64(0)
                        | Const::U64(0)
                        | Const::I32(0)
                        | Const::U32(0)
                        | Const::I16(0)
                        | Const::U16(0)
                        | Const::I8(0)
                        | Const::U8(0) => opt_if_fuel(
                            CILRoot::Branch(Box::new((
                                *target,
                                *sub_target,
                                Some(BranchCond::False(*lhs)),
                            ))),
                            root,
                            root_fuel,
                        ),
                        _ => root,
                    },
                    (CILNode::Const(cst), _) => match cst.as_ref() {
                        Const::Bool(false)
                        | Const::ISize(0)
                        | Const::USize(0)
                        | Const::I64(0)
                        | Const::U64(0)
                        | Const::I32(0)
                        | Const::U32(0)
                        | Const::I16(0)
                        | Const::U16(0)
                        | Const::I8(0)
                        | Const::U8(0) => opt_if_fuel(
                            CILRoot::Branch(Box::new((
                                *target,
                                *sub_target,
                                Some(BranchCond::False(*rhs)),
                            ))),
                            root,
                            root_fuel,
                        ),
                        _ => root,
                    },
                    _ => root,
                },
                Some(_) | None => root,
            }
        }
        CILRoot::StLoc(loc, val) if asm[val] == CILNode::LdLoc(loc) => CILRoot::Nop,
        CILRoot::StArg(loc, val) if asm[val] == CILNode::LdArg(loc) => CILRoot::Nop,
        // A managed-array element store is meaningful and is left untouched by the optimizer.
        CILRoot::StElem { .. } => root,
        _ => root,
    }
}
fn opt_init_obj(
    mut addr: Interned<CILNode>,
    tpe: Interned<Type>,
    asm: &mut Assembly,
    fuel: &mut OptFuel,
) -> CILRoot {
    // 1. Check if the addr is RefToPtr. If so, remove that.
    if let CILNode::RefToPtr(inner) = asm[addr] {
        if fuel.consume(1) {
            addr = inner;
        }
    }
    // 2. Check if the type is a small primitive - if so, replace this with StObj to allow for more optimizations.
    match asm[tpe] {
        Type::Int(int) if int.size().unwrap_or(8) <= 8 && fuel.consume(1) => {
            return CILRoot::StInd(Box::new((
                addr,
                asm.alloc_node(int.zero()),
                Type::Int(int),
                false,
            )));
        }
        Type::Float(float) if fuel.consume(1) && matches!(float.size(), 32 | 64) => {
            return CILRoot::StInd(Box::new((
                addr,
                asm.alloc_node(float.zero()),
                Type::Float(float),
                false,
            )));
        }
        Type::Bool if fuel.consume(1) => {
            return CILRoot::StInd(Box::new((addr, asm.alloc_node(false), Type::Bool, false)));
        }
        _ => (),
    }
    CILRoot::InitObj(addr, tpe)
}
