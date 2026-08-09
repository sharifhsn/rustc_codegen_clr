use std::collections::HashMap;

use crate::{Assembly, BinOp, CILNode, bimap::Interned};

/// A composable summary of effects produced while evaluating a CIL expression.
///
/// The zero element, [`PURE_TOTAL`](Self::PURE_TOTAL), is the only value which permits deletion:
/// purity alone is insufficient when evaluation may throw, trigger a type initializer, allocate,
/// mutate memory, or alter control/runtime state.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct EffectSummary(u8);

impl EffectSummary {
    pub const PURE_TOTAL: Self = Self(0);
    pub const MAY_THROW: Self = Self(1 << 0);
    pub const MAY_RUN_TYPE_INIT: Self = Self(1 << 1);
    pub const MAY_WRITE_MEMORY: Self = Self(1 << 2);
    pub const MAY_ALTER_CONTROL: Self = Self(1 << 3);
    pub const MAY_ALLOCATE: Self = Self(1 << 4);
    pub const MAY_READ_VOLATILE_MEMORY: Self = Self(1 << 5);

    #[must_use]
    pub const fn is_pure_total(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn contains(self, effect: Self) -> bool {
        self.0 & effect.0 == effect.0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for EffectSummary {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for EffectSummary {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

#[derive(Default)]
pub struct EffectInfoCache {
    summaries: HashMap<Interned<CILNode>, EffectSummary>,
}

impl EffectInfoCache {
    /// Returns the effect lattice element for evaluating `node`, including every child.
    pub fn summary(&mut self, node: Interned<CILNode>, asm: &Assembly) -> EffectSummary {
        if let Some(summary) = self.summaries.get(&node) {
            return *summary;
        }

        let current = asm.get_node(node);
        let mut summary = match current {
            CILNode::Const(_)
            | CILNode::LdLoc(_)
            | CILNode::LdArg(_)
            | CILNode::LdLocA(_)
            | CILNode::LdArgA(_)
            | CILNode::LdTypeToken(_)
            | CILNode::LdFtn(_)
            | CILNode::SizeOf(_)
            | CILNode::UnOp(_, _)
            | CILNode::RefToPtr(_)
            | CILNode::IntCast { .. }
            | CILNode::FloatCast { .. }
            | CILNode::PtrCast(_, _) => EffectSummary::PURE_TOTAL,
            CILNode::BinOp(_, _, BinOp::Div | BinOp::DivUn | BinOp::Rem | BinOp::RemUn) => {
                EffectSummary::MAY_THROW
            }
            CILNode::BinOp(_, _, _) => EffectSummary::PURE_TOTAL,
            CILNode::Call(info) => {
                let mut effect = EffectSummary::MAY_THROW
                    | EffectSummary::MAY_RUN_TYPE_INIT
                    | EffectSummary::MAY_ALTER_CONTROL;
                if !info.2.0 {
                    effect |= EffectSummary::MAY_WRITE_MEMORY;
                }
                effect
            }
            CILNode::CallI(_) => {
                EffectSummary::MAY_THROW
                    | EffectSummary::MAY_WRITE_MEMORY
                    | EffectSummary::MAY_ALTER_CONTROL
            }
            CILNode::LdFieldAddress { .. }
            | CILNode::LdField { .. }
            | CILNode::LdLen(_)
            | CILNode::LdElelemRef { .. }
            | CILNode::LdElem { .. }
            | CILNode::UnboxAny { .. }
            | CILNode::IsInst(_, _)
            | CILNode::CheckedCast(_, _) => EffectSummary::MAY_THROW,
            CILNode::LdInd { volatile, .. } => {
                let mut effect = EffectSummary::MAY_THROW;
                if *volatile {
                    effect |= EffectSummary::MAY_READ_VOLATILE_MEMORY;
                }
                effect
            }
            CILNode::GetException => EffectSummary::MAY_ALTER_CONTROL,
            CILNode::LocAllocAlgined { .. } | CILNode::LocAlloc { .. } => {
                EffectSummary::MAY_ALLOCATE | EffectSummary::MAY_THROW
            }
            CILNode::NewArr { .. } | CILNode::Box { .. } => {
                EffectSummary::MAY_ALLOCATE
                    | EffectSummary::MAY_THROW
                    | EffectSummary::MAY_RUN_TYPE_INIT
            }
            CILNode::LdStaticField(_) | CILNode::LdStaticFieldAddress(_) => {
                EffectSummary::MAY_RUN_TYPE_INIT | EffectSummary::MAY_THROW
            }
        };

        current.visit_child_nodes(|child| summary |= self.summary(*child, asm));
        self.summaries.insert(node, summary);
        summary
    }
}

/// Compatibility alias for downstream optimizer integrations. New code should use
/// [`EffectInfoCache`] and inspect [`EffectSummary`] explicitly.
#[deprecated(note = "use EffectInfoCache and EffectSummary")]
pub type SideEffectInfoCache = EffectInfoCache;

#[test]
fn effect_summary_distinguishes_throw_type_init_write_and_control() {
    use crate::{
        ClassRef, Const, FnSig, Int, MethodRef, Type,
        cilnode::{ExtendKind, IsPure, MethodKind},
    };

    let mut asm = Assembly::default();
    let lhs = asm.alloc_node(Const::I32(1));
    let rhs = asm.alloc_node(Const::I32(0));
    let div = asm.alloc_node(CILNode::BinOp(lhs, rhs, BinOp::Div));
    let harmless_cast = asm.alloc_node(CILNode::IntCast {
        input: lhs,
        target: Int::I64,
        extend: ExtendKind::SignExtend,
    });

    let owner = ClassRef::object(&mut asm);
    let name = asm.alloc_string("effectful");
    let sig = asm.alloc_sig(FnSig::new([], Type::Void));
    let method = asm.alloc_methodref(MethodRef::new(
        owner,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ));
    let pure_call = asm.alloc_node(CILNode::Call(Box::new((method, [].into(), IsPure::PURE))));
    let impure_call = asm.alloc_node(CILNode::Call(Box::new((method, [].into(), IsPure::NOT))));
    let get_exception = asm.alloc_node(CILNode::GetException);

    let mut cache = EffectInfoCache::default();
    assert_eq!(
        cache.summary(harmless_cast, &asm),
        EffectSummary::PURE_TOTAL
    );
    assert_eq!(cache.summary(div, &asm), EffectSummary::MAY_THROW);

    let pure = cache.summary(pure_call, &asm);
    assert!(pure.contains(EffectSummary::MAY_THROW));
    assert!(pure.contains(EffectSummary::MAY_RUN_TYPE_INIT));
    assert!(pure.contains(EffectSummary::MAY_ALTER_CONTROL));
    assert!(!pure.contains(EffectSummary::MAY_WRITE_MEMORY));

    let impure = cache.summary(impure_call, &asm);
    assert!(impure.contains(EffectSummary::MAY_WRITE_MEMORY));
    assert_eq!(
        cache.summary(get_exception, &asm),
        EffectSummary::MAY_ALTER_CONTROL
    );
}

#[test]
fn only_pure_total_expressions_are_deletable() {
    use crate::{
        Const,
        hashable::{HashableF32, HashableF64},
    };

    let consts = [
        true.into(),
        false.into(),
        Const::F32(HashableF32(std::f32::consts::PI)),
        Const::F64(HashableF64(std::f64::consts::PI)),
        Const::I8(5),
        Const::U8(5),
        Const::I16(5),
        Const::U16(5),
        Const::I32(5),
        Const::U32(5),
        Const::I64(5),
        Const::U64(5),
    ];
    let mut asm = Assembly::default();
    let mut cache = EffectInfoCache::default();
    for cst in consts {
        let node = asm.alloc_node(cst.clone());
        assert!(cache.summary(node, &asm).is_pure_total());
        let node = asm.biop(cst.clone(), cst.clone(), crate::BinOp::Add);
        assert!(cache.summary(node, &asm).is_pure_total());
        let node = asm.biop(
            CILNode::LocAlloc { size: node },
            cst.clone(),
            crate::BinOp::Add,
        );
        assert!(!cache.summary(node, &asm).is_pure_total());
        let node = asm.biop(
            cst.clone(),
            CILNode::LocAlloc { size: node },
            crate::BinOp::Add,
        );
        assert!(!cache.summary(node, &asm).is_pure_total());
    }
}

#[test]
fn pure_select_call_is_not_total() {
    use crate::{Int, Type};

    let mut asm = Assembly::default();
    let a = asm.alloc_node(1_usize);
    let b = asm.alloc_node(2_usize);
    let predicate = asm.alloc_node(CILNode::LdLoc(0));
    let select = asm.select(Type::Int(Int::USize), a, b, predicate);
    let mut cache = EffectInfoCache::default();
    let summary = cache.summary(select, &asm);
    assert!(!summary.is_pure_total(), "select:{:?}", asm[select]);
    assert!(!summary.contains(EffectSummary::MAY_WRITE_MEMORY));
}
