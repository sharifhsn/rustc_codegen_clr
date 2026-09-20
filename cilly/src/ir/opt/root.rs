use super::super::{
    BinOp, CILNode, CILRoot, Const, FnSig, Type, cilroot::BranchCond, method::LocalDef,
};
pub use super::opt_fuel::OptFuel;
use super::opt_if_fuel;
pub use super::side_effect::*;
use crate::Assembly;
use crate::bimap::Interned;
use crate::cilroot::CmpKind;

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
    if is_float { float_kind } else { int_kind }
}

fn optimized_branch(
    target: u32,
    sub_target: u32,
    cond: Option<BranchCond>,
    original: CILRoot,
    fuel: &mut OptFuel,
) -> CILRoot {
    opt_if_fuel(
        CILRoot::Branch(Box::new((target, sub_target, cond))),
        original,
        fuel,
    )
}

fn is_zero_const(cst: &Const) -> bool {
    matches!(
        cst,
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
            | Const::U8(0)
    )
}

fn true_branch_condition(
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    op: BinOp,
) -> Option<BranchCond> {
    Some(match op {
        BinOp::Eq => BranchCond::Eq(lhs, rhs),
        BinOp::GtUn => BranchCond::Gt(lhs, rhs, CmpKind::Unordered),
        BinOp::Gt => BranchCond::Gt(lhs, rhs, CmpKind::Ordered),
        BinOp::LtUn => BranchCond::Lt(lhs, rhs, CmpKind::Unordered),
        BinOp::Lt => BranchCond::Lt(lhs, rhs, CmpKind::Ordered),
        _ => return None,
    })
}

fn false_branch_condition(
    lhs: Interned<CILNode>,
    rhs: Interned<CILNode>,
    op: BinOp,
    sig: Interned<FnSig>,
    locals: &[LocalDef],
    asm: &mut Assembly,
) -> Option<BranchCond> {
    Some(match op {
        BinOp::Eq => BranchCond::Ne(lhs, rhs),
        BinOp::Gt | BinOp::GtUn => {
            let (int_kind, float_kind) = if op == BinOp::Gt {
                (CmpKind::Ordered, CmpKind::Unordered)
            } else {
                (CmpKind::Unordered, CmpKind::Ordered)
            };
            BranchCond::Le(
                lhs,
                rhs,
                negation_cmp_kind(lhs, int_kind, float_kind, sig, locals, asm),
            )
        }
        BinOp::Lt | BinOp::LtUn => {
            let (int_kind, float_kind) = if op == BinOp::Lt {
                (CmpKind::Ordered, CmpKind::Unordered)
            } else {
                (CmpKind::Unordered, CmpKind::Ordered)
            };
            BranchCond::Ge(
                lhs,
                rhs,
                negation_cmp_kind(lhs, int_kind, float_kind, sig, locals, asm),
            )
        }
        _ => return None,
    })
}

pub fn root_opt(
    root: CILRoot,
    asm: &mut Assembly,
    root_fuel: &mut OptFuel,
    cache: &mut EffectInfoCache,
    locals: &[LocalDef],
    sig: Interned<FnSig>,
) -> CILRoot {
    match root {
        CILRoot::Pop(pop) => match asm.get_node(pop) {
            CILNode::LdLoc(_) => CILRoot::Nop,
            _ => {
                if cache.summary(pop, asm).is_pure_total() {
                    CILRoot::Nop
                } else {
                    root
                }
            }
        },
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
            let (field, addr, val) = info.as_ref();
            let mut addr = *addr;
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
                            Const::Bool(false) => {
                                optimized_branch(*target, *sub_target, None, root, root_fuel)
                            }
                            Const::Bool(true) => opt_if_fuel(CILRoot::Nop, root, root_fuel),
                            _ => root,
                        },
                        CILNode::BinOp(lhs, rhs, op) => {
                            match false_branch_condition(lhs, rhs, op, sig, locals, asm) {
                                Some(cond) => optimized_branch(
                                    *target,
                                    *sub_target,
                                    Some(cond),
                                    root,
                                    root_fuel,
                                ),
                                None => root,
                            }
                        }
                        _ => root,
                    }
                }
                Some(BranchCond::True(cond)) => match asm.get_node(*cond) {
                    CILNode::BinOp(lhs, rhs, op) => match true_branch_condition(*lhs, *rhs, *op) {
                        Some(cond) => {
                            optimized_branch(*target, *sub_target, Some(cond), root, root_fuel)
                        }
                        None => root,
                    },
                    _ => root,
                },
                Some(BranchCond::Ne(lhs, rhs)) => match (asm.get_node(*lhs), asm.get_node(*rhs)) {
                    (_, CILNode::Const(cst)) if is_zero_const(cst) => optimized_branch(
                        *target,
                        *sub_target,
                        Some(BranchCond::True(*lhs)),
                        root,
                        root_fuel,
                    ),
                    (_, CILNode::Const(cst)) if matches!(cst.as_ref(), Const::Bool(true)) => {
                        optimized_branch(
                            *target,
                            *sub_target,
                            Some(BranchCond::False(*lhs)),
                            root,
                            root_fuel,
                        )
                    }
                    (CILNode::Const(cst), _) if is_zero_const(cst) => optimized_branch(
                        *target,
                        *sub_target,
                        Some(BranchCond::True(*rhs)),
                        root,
                        root_fuel,
                    ),
                    _ => root,
                },
                Some(BranchCond::Eq(lhs, rhs)) => match (asm.get_node(*lhs), asm.get_node(*rhs)) {
                    (_, CILNode::Const(cst)) if is_zero_const(cst) => optimized_branch(
                        *target,
                        *sub_target,
                        Some(BranchCond::False(*lhs)),
                        root,
                        root_fuel,
                    ),
                    (CILNode::Const(cst), _) if is_zero_const(cst) => optimized_branch(
                        *target,
                        *sub_target,
                        Some(BranchCond::False(*rhs)),
                        root,
                        root_fuel,
                    ),
                    _ => root,
                },
                Some(_) | None => root,
            }
        }
        CILRoot::StLoc(loc, val) if asm[val] == CILNode::LdLoc(loc) => CILRoot::Nop,
        CILRoot::StArg(loc, val) if asm[val] == CILNode::LdArg(loc) => CILRoot::Nop,
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
    if let CILNode::RefToPtr(inner) = asm[addr]
        && fuel.consume(1)
    {
        addr = inner;
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
