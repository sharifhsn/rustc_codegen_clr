use serde::{Deserialize, Serialize};

use super::{
    Assembly, CILNode, FieldDesc, FnSig, MethodRef, StaticFieldDesc, Type, bimap::Interned,
    cilnode::IsPure,
};
use crate::IString;
#[derive(PartialEq, Hash, Eq, Clone, Debug, Serialize, Deserialize)]
pub enum CILRoot {
    StLoc(u32, Interned<CILNode>),
    StArg(u32, Interned<CILNode>),
    Ret(Interned<CILNode>),
    Pop(Interned<CILNode>),
    Throw(Interned<CILNode>),
    VoidRet,
    Break,
    Nop,
    /// target subtarget cond
    Branch(Box<(u32, u32, Option<BranchCond>)>),
    SourceFileInfo {
        line_start: u32,
        line_len: u16,
        col_start: u16,
        col_len: u16,
        file: Interned<IString>,
    },
    /// Field,  addr,value
    SetField(Box<(Interned<FieldDesc>, Interned<CILNode>, Interned<CILNode>)>),
    Call(Box<(Interned<MethodRef>, Box<[Interned<CILNode>]>, IsPure)>),
    /// addr, value, type
    StInd(Box<(Interned<CILNode>, Interned<CILNode>, Type, bool)>),
    /// dst, val, count
    InitBlk(Box<(Interned<CILNode>, Interned<CILNode>, Interned<CILNode>)>),
    /// dst src len
    CpBlk(Box<(Interned<CILNode>, Interned<CILNode>, Interned<CILNode>)>),
    /// Calls fn pointer with args
    CallI(Box<(Interned<CILNode>, Interned<FnSig>, Box<[Interned<CILNode>]>)>),
    /// Exits a protected region of code.
    ExitSpecialRegion {
        target: u32,
        source: u32,
    },
    /// A self-contained, single-op protected region that aborts uncatchably (`FailFast`) if the
    /// `protected` root throws. Used to model a `Drop`-glue call sitting on a MIR **cleanup** block
    /// whose `UnwindAction` is `Terminate` — i.e. a destructor that may panic *while already
    /// unwinding* (a double panic, `InCleanup`) or that crosses a `nounwind` boundary mid-cleanup
    /// (`Abi`). At export time this renders the inner CLR region
    /// `.try{ <protected>; leave done } catch System.Object { pop; ldstr <msg>; FailFast; rethrow } done: nop`.
    /// It is NOT a `BasicBlock` handler (so it never trips the single-layer handler ban) and its only
    /// child is the single `protected` root. `reason`: 0 = Abi, 1 = InCleanup (kept a plain `u8` so
    /// `cilly` stays free of rustc's `UnwindTerminateReason`).
    TerminateRegion {
        protected: Interned<CILRoot>,
        reason: u8,
    },
    /// Rethrows the current exception
    ReThrow,
    /// Sets the static field to a value.
    SetStaticField {
        field: Interned<StaticFieldDesc>,
        val: Interned<CILNode>,
    },
    CpObj {
        src: Interned<CILNode>,
        dst: Interned<CILNode>,
        tpe: Interned<Type>,
    },
    /// Executing this root is instant UB.
    Unreachable(Interned<IString>),
    /// Zero-initializes the value at *address* of *type*.
    InitObj(Interned<CILNode>, Interned<Type>),
    /// Stores `value` (of element type `elem`) into the managed array `array` at `index` (`stelem`).
    StElem {
        array: Interned<CILNode>,
        index: Interned<CILNode>,
        value: Interned<CILNode>,
        elem: Interned<Type>,
    },
}

#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub enum BranchCond {
    True(Interned<CILNode>),
    False(Interned<CILNode>),
    Eq(Interned<CILNode>, Interned<CILNode>),
    Ne(Interned<CILNode>, Interned<CILNode>),
    Lt(Interned<CILNode>, Interned<CILNode>, CmpKind),
    Gt(Interned<CILNode>, Interned<CILNode>, CmpKind),
    Le(Interned<CILNode>, Interned<CILNode>, CmpKind),
    Ge(Interned<CILNode>, Interned<CILNode>, CmpKind),
}
impl BranchCond {
    /// Returns all the nodes used by this branch cond.
    /// ```
    /// # use cilly::*;
    /// # let mut asm = Assembly::default();
    /// let ldarg_0 = asm.alloc_node(CILNode::LdArg(0));
    /// let ldloc_1 = asm.alloc_node(CILNode::LdLoc(1));
    /// let eq = BranchCond::Eq(ldarg_0,ldloc_1);
    /// // Two child nodes - ldarg_0 and ldloc_1
    /// assert_eq!(eq.nodes(),vec![ldarg_0,ldloc_1]);
    /// let cond_true = BranchCond::True(ldarg_0);
    /// // One child node - ldarg_0
    /// assert_eq!(cond_true.nodes(),vec![ldarg_0]);
    /// ```
    pub fn nodes(&self) -> Vec<Interned<CILNode>> {
        match self {
            BranchCond::True(cond) | BranchCond::False(cond) => vec![*cond],
            BranchCond::Eq(lhs, rhs)
            | BranchCond::Ne(lhs, rhs)
            | BranchCond::Lt(lhs, rhs, _)
            | BranchCond::Gt(lhs, rhs, _)
            | BranchCond::Le(lhs, rhs, _)
            | BranchCond::Ge(lhs, rhs, _) => vec![*lhs, *rhs],
        }
    }
}
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub enum CmpKind {
    Ordered,
    Unordered,
    Signed,
    Unsigned,
}
impl CILRoot {
    pub fn call(mref: Interned<MethodRef>, args: impl Into<Box<[Interned<CILNode>]>>) -> Self {
        Self::Call(Box::new((mref, args.into(), IsPure::NOT)))
    }
    /// Checks if this root has any effect on the execution of this program.
    pub fn is_meaningufull(&self) -> bool {
        !matches!(self, CILRoot::Nop | CILRoot::SourceFileInfo { .. })
    }
    /// Returns a mutable reference to all the arguments of this CIL root, in the order they are evaluated.
    pub fn nodes_mut(&mut self) -> Box<[&mut Interned<CILNode>]> {
        match self {
            CILRoot::Unreachable(_) => [].into(),
            CILRoot::StLoc(_, tree)
            | CILRoot::StArg(_, tree)
            | CILRoot::Ret(tree)
            | CILRoot::Pop(tree)
            | CILRoot::Throw(tree)
            | CILRoot::InitObj(tree, _)
            | CILRoot::SetStaticField { val: tree, .. } => [tree].into(),
            CILRoot::SourceFileInfo { .. }
            | CILRoot::ExitSpecialRegion { .. }
            | CILRoot::VoidRet
            | CILRoot::Break
            | CILRoot::Nop
            | CILRoot::TerminateRegion { .. }
            | CILRoot::ReThrow => [].into(),
            CILRoot::Branch(info) => {
                let (_, _, cond) = info.as_mut();
                let Some(cond) = cond else { return [].into() };
                match cond {
                    BranchCond::True(cond) | BranchCond::False(cond) => [cond].into(),
                    BranchCond::Eq(lhs, rhs)
                    | BranchCond::Ne(lhs, rhs)
                    | BranchCond::Lt(lhs, rhs, _)
                    | BranchCond::Gt(lhs, rhs, _)
                    | BranchCond::Le(lhs, rhs, _)
                    | BranchCond::Ge(lhs, rhs, _) => [lhs, rhs].into(),
                }
            }
            CILRoot::SetField(info) => {
                let (_, addr, val) = info.as_mut();
                [addr, val].into()
            }
            CILRoot::Call(info) => many_mut(&mut info.1).into(),
            CILRoot::StInd(info) => {
                let (addr, val, _, _) = info.as_mut();
                [addr, val].into()
            }
            CILRoot::InitBlk(info) | CILRoot::CpBlk(info) => {
                let (addr, val, len) = info.as_mut();
                [addr, val, len].into()
            }
            CILRoot::CallI(info) => {
                let (ptr, _, args) = info.as_mut();
                let mut args = many_mut(args);
                args.push(ptr);
                args.into()
            }
            CILRoot::CpObj { src, dst, .. } => [src, dst].into(),
            CILRoot::StElem {
                array,
                index,
                value,
                ..
            } => [array, index, value].into(),
        }
    }
    pub fn nodes(&self) -> Box<[&Interned<CILNode>]> {
        match self {
            CILRoot::Unreachable(_) => [].into(),
            CILRoot::StLoc(_, tree)
            | CILRoot::StArg(_, tree)
            | CILRoot::Ret(tree)
            | CILRoot::Pop(tree)
            | CILRoot::Throw(tree)
            | CILRoot::InitObj(tree, _)
            | CILRoot::SetStaticField { val: tree, .. } => [tree].into(),
            CILRoot::SourceFileInfo { .. }
            | CILRoot::ExitSpecialRegion { .. }
            | CILRoot::VoidRet
            | CILRoot::Break
            | CILRoot::Nop
            | CILRoot::TerminateRegion { .. }
            | CILRoot::ReThrow => [].into(),
            CILRoot::Branch(info) => {
                let (_, _, cond) = info.as_ref();
                let Some(cond) = cond else { return [].into() };
                match cond {
                    BranchCond::True(cond) | BranchCond::False(cond) => [cond].into(),
                    BranchCond::Eq(lhs, rhs)
                    | BranchCond::Ne(lhs, rhs)
                    | BranchCond::Lt(lhs, rhs, _)
                    | BranchCond::Gt(lhs, rhs, _)
                    | BranchCond::Le(lhs, rhs, _)
                    | BranchCond::Ge(lhs, rhs, _) => [lhs, rhs].into(),
                }
            }
            CILRoot::SetField(info) => {
                let (_, addr, val) = info.as_ref();
                [addr, val].into()
            }
            CILRoot::Call(info) => many_ref(&info.1).into(),
            CILRoot::StInd(info) => {
                let (addr, val, _, _) = info.as_ref();
                [addr, val].into()
            }
            CILRoot::InitBlk(info) | CILRoot::CpBlk(info) => {
                let (addr, val, len) = info.as_ref();
                [addr, val, len].into()
            }
            CILRoot::CallI(info) => {
                let (ptr, _, args) = info.as_ref();
                let mut args = many_ref(args);
                args.push(ptr);
                args.into()
            }
            CILRoot::CpObj { src, dst, .. } => [src, dst].into(),
            CILRoot::StElem {
                array,
                index,
                value,
                ..
            } => [array, index, value].into(),
        }
    }
    /// Maps this root using `root_map` and `node_map`.
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub fn map(
        self,
        asm: &mut Assembly,
        root_map: &mut dyn FnMut(Self, &mut Assembly) -> Self,
        node_map: &mut dyn FnMut(CILNode, &mut Assembly) -> CILNode,
    ) -> Self {
        match self {
            CILRoot::Unreachable(_) => root_map(self, asm),
            CILRoot::StLoc(loc, val) => {
                let val: CILNode = asm.get_node(val).clone().map(asm, node_map);
                let root = CILRoot::StLoc(loc, asm.alloc_node(val));
                root_map(root, asm)
            }
            CILRoot::StArg(arg, val) => {
                let val: CILNode = asm.get_node(val).clone().map(asm, node_map);
                let root = CILRoot::StArg(arg, asm.alloc_node(val));
                root_map(root, asm)
            }
            CILRoot::Ret(ret) => {
                let ret = asm.get_node(ret).clone().map(asm, node_map);
                let root = CILRoot::Ret(asm.alloc_node(ret));
                root_map(root, asm)
            }
            CILRoot::InitObj(addr, tpe) => {
                let addr = asm.get_node(addr).clone().map(asm, node_map);
                let root = CILRoot::InitObj(asm.alloc_node(addr), tpe);
                root_map(root, asm)
            }
            CILRoot::Pop(pop) => {
                let pop = asm.get_node(pop).clone().map(asm, node_map);
                let root = CILRoot::Pop(asm.alloc_node(pop));
                root_map(root, asm)
            }
            CILRoot::Throw(throw) => {
                let throw = asm.get_node(throw).clone().map(asm, node_map);
                let root = CILRoot::Throw(asm.alloc_node(throw));
                root_map(root, asm)
            }
            CILRoot::SourceFileInfo { .. }
            | CILRoot::VoidRet
            | CILRoot::Break
            | CILRoot::Nop
            | CILRoot::ExitSpecialRegion { .. }
            | CILRoot::ReThrow => root_map(self, asm),
            CILRoot::TerminateRegion { protected, reason } => {
                // Recurse into the single protected child root so optimizer/realloc passes that go
                // through `map` see and rewrite it, then re-wrap.
                let inner = asm.get_root(protected).clone().map(asm, root_map, node_map);
                let protected = asm.alloc_root(inner);
                let root = CILRoot::TerminateRegion { protected, reason };
                root_map(root, asm)
            }
            CILRoot::Branch(branch) => {
                let (a, b, cond) = *branch;
                let cond = match cond {
                    Some(BranchCond::True(tr)) => {
                        let tr = asm.get_node(tr).clone().map(asm, node_map);
                        Some(BranchCond::True(asm.alloc_node(tr)))
                    }
                    Some(BranchCond::False(fl)) => {
                        let fl = asm.get_node(fl).clone().map(asm, node_map);
                        Some(BranchCond::False(asm.alloc_node(fl)))
                    }
                    Some(BranchCond::Eq(lhs, rhs)) => {
                        let lhs = asm.get_node(lhs).clone().map(asm, node_map);
                        let rhs = asm.get_node(rhs).clone().map(asm, node_map);
                        Some(BranchCond::Eq(asm.alloc_node(lhs), asm.alloc_node(rhs)))
                    }
                    Some(BranchCond::Ne(lhs, rhs)) => {
                        let lhs = asm.get_node(lhs).clone().map(asm, node_map);
                        let rhs = asm.get_node(rhs).clone().map(asm, node_map);
                        Some(BranchCond::Ne(asm.alloc_node(lhs), asm.alloc_node(rhs)))
                    }
                    Some(BranchCond::Lt(lhs, rhs, cmp_kind)) => {
                        let lhs = asm.get_node(lhs).clone().map(asm, node_map);
                        let rhs = asm.get_node(rhs).clone().map(asm, node_map);
                        Some(BranchCond::Lt(
                            asm.alloc_node(lhs),
                            asm.alloc_node(rhs),
                            cmp_kind,
                        ))
                    }
                    Some(BranchCond::Gt(lhs, rhs, cmp_kind)) => {
                        let lhs = asm.get_node(lhs).clone().map(asm, node_map);
                        let rhs = asm.get_node(rhs).clone().map(asm, node_map);
                        Some(BranchCond::Gt(
                            asm.alloc_node(lhs),
                            asm.alloc_node(rhs),
                            cmp_kind,
                        ))
                    }
                    Some(BranchCond::Le(lhs, rhs, cmp_kind)) => {
                        let lhs = asm.get_node(lhs).clone().map(asm, node_map);
                        let rhs = asm.get_node(rhs).clone().map(asm, node_map);
                        Some(BranchCond::Le(
                            asm.alloc_node(lhs),
                            asm.alloc_node(rhs),
                            cmp_kind,
                        ))
                    }
                    Some(BranchCond::Ge(lhs, rhs, cmp_kind)) => {
                        let lhs = asm.get_node(lhs).clone().map(asm, node_map);
                        let rhs = asm.get_node(rhs).clone().map(asm, node_map);
                        Some(BranchCond::Ge(
                            asm.alloc_node(lhs),
                            asm.alloc_node(rhs),
                            cmp_kind,
                        ))
                    }
                    None => None,
                };
                let root = CILRoot::Branch(Box::new((a, b, cond)));
                root_map(root, asm)
            }
            CILRoot::SetStaticField { field, val } => {
                let val = asm.get_node(val).clone().map(asm, node_map);
                let root = CILRoot::SetStaticField {
                    field,
                    val: asm.alloc_node(val),
                };
                root_map(root, asm)
            }
            CILRoot::SetField(set_field) => {
                let (field, addr, val) = *set_field;
                let addr = asm.get_node(addr).clone().map(asm, node_map);
                let val = asm.get_node(val).clone().map(asm, node_map);
                let root =
                    CILRoot::SetField(Box::new((field, asm.alloc_node(addr), asm.alloc_node(val))));
                root_map(root, asm)
            }
            CILRoot::Call(call_info) => {
                let (method_id, args, is_pure) = *call_info;
                let args = args
                    .iter()
                    .map(|arg| {
                        let node = asm.get_node(*arg).clone().map(asm, node_map);
                        asm.alloc_node(node)
                    })
                    .collect();

                let root = CILRoot::Call(Box::new((method_id, args, is_pure)));
                root_map(root, asm)
            }
            CILRoot::StInd(ind) => {
                let (addr, val, tpe, volitale) = *ind;
                let addr = asm.get_node(addr).clone().map(asm, node_map);
                let val = asm.get_node(val).clone().map(asm, node_map);
                let root = CILRoot::StInd(Box::new((
                    asm.alloc_node(addr),
                    asm.alloc_node(val),
                    tpe,
                    volitale,
                )));
                root_map(root, asm)
            }
            CILRoot::CpObj { src, dst, tpe } => {
                let src = asm.get_node(src).clone().map(asm, node_map);
                let dst = asm.get_node(dst).clone().map(asm, node_map);
                let root = CILRoot::CpObj {
                    src: asm.alloc_node(src),
                    dst: asm.alloc_node(dst),
                    tpe,
                };
                root_map(root, asm)
            }
            CILRoot::InitBlk(blk) => {
                let (dst, val, count) = *blk;
                let dst = asm.get_node(dst).clone().map(asm, node_map);
                let val = asm.get_node(val).clone().map(asm, node_map);
                let count = asm.get_node(count).clone().map(asm, node_map);
                let root = CILRoot::InitBlk(Box::new((
                    asm.alloc_node(dst),
                    asm.alloc_node(val),
                    asm.alloc_node(count),
                )));
                root_map(root, asm)
            }
            CILRoot::CpBlk(blk) => {
                let (dst, src, len) = *blk;
                let dst = asm.get_node(dst).clone().map(asm, node_map);
                let src = asm.get_node(src).clone().map(asm, node_map);
                let len = asm.get_node(len).clone().map(asm, node_map);
                let root = CILRoot::CpBlk(Box::new((
                    asm.alloc_node(dst),
                    asm.alloc_node(src),
                    asm.alloc_node(len),
                )));
                root_map(root, asm)
            }
            CILRoot::CallI(call_info) => {
                let (ptr, sig, args) = *call_info;
                let args = args
                    .iter()
                    .map(|arg| {
                        let node = asm.get_node(*arg).clone().map(asm, node_map);
                        asm.alloc_node(node)
                    })
                    .collect();
                let ptr = asm.get_node(ptr).clone().map(asm, node_map);
                let root = CILRoot::CallI(Box::new((asm.alloc_node(ptr), sig, args)));
                root_map(root, asm)
            }
            CILRoot::StElem {
                array,
                index,
                value,
                elem,
            } => {
                let array = asm.get_node(array).clone().map(asm, node_map);
                let index = asm.get_node(index).clone().map(asm, node_map);
                let value = asm.get_node(value).clone().map(asm, node_map);
                let root = CILRoot::StElem {
                    array: asm.alloc_node(array),
                    index: asm.alloc_node(index),
                    value: asm.alloc_node(value),
                    elem,
                };
                root_map(root, asm)
            }
        }
    }
    /// Returns a debug string, representing this root. This debug repr contains additional info not included by std::fmt::Debug.
    /// ```
    /// # use cilly::cilroot::CILRoot;
    /// # let mut asm = cilly::Assembly::default();
    /// # let sig = asm.sig([],cilly::Type::Void);
    /// # let locals = [];
    /// let root = CILRoot::Nop;
    /// assert_eq!(root.display(&mut asm,sig,&locals),"Nop");
    /// ```
    pub fn display(
        &self,
        asm: &mut Assembly,
        _sig: Interned<FnSig>,
        locals: &[(Option<Interned<IString>>, Interned<Type>)],
    ) -> String {
        match self {
            Self::StInd(boxed) => {
                let (addr, val, tpe, is_volitile) = boxed.as_ref();
                let tpe = tpe.mangle(asm);
                format!("StInd{{addr:{addr:?},val:{val:?},tpe:{tpe},is_volitile:{is_volitile}}}")
            }
            Self::StLoc(loc, val) => match locals.get(*loc as usize) {
                Some((Some(name), tpe)) => format!(
                    "StLoc({loc}: {loc_tpe:?} {name:?}, {val:?})",
                    loc_tpe = asm[*tpe].clone().mangle(asm),
                    name = &asm[*name],
                ),
                Some((None, tpe)) => format!(
                    "StLoc({loc}: {loc_tpe},{val:?})",
                    loc_tpe = asm[*tpe].clone().mangle(asm),
                ),
                None => format!("{self:?}"),
            },
            _ => format!("{self:?}"),
        }
    }
}
/// Changes a mutable reference to a slice to an vec of mutable references to the elements.
fn many_mut<T>(input: &mut [T]) -> Vec<&mut T> {
    let input_len = input.len();
    let res = match input.len() {
        0 => [].into(),
        1 => [&mut input[0]].into(),
        2 => {
            let (a, b) = input.split_at_mut(1);

            [&mut a[0], &mut b[0]].into()
        }
        3 => {
            let (a, b) = input.split_at_mut(1);
            let (b, c) = b.split_at_mut(1);
            [&mut a[0], &mut b[0], &mut c[0]].into()
        }
        4 => {
            let (lhs, rhs) = input.split_at_mut(2);
            let (a, b) = lhs.split_at_mut(1);
            let (c, d) = rhs.split_at_mut(1);
            [&mut a[0], &mut b[0], &mut c[0], &mut d[0]].into()
        }
        _ => {
            let half = input.len() / 2;
            let (lhs, rhs) = input.split_at_mut(half);
            let mut result = many_mut(lhs);
            result.extend(many_mut(rhs));
            result
        }
    };
    assert_eq!(res.len(), input_len);
    res
}
/// Changes a reference to a slice to an vec of references to the elements.
fn many_ref<T>(inputs: &[T]) -> Vec<&T> {
    inputs.iter().collect()
}
#[test]
fn test_many_ref() {
    let inputs = [0, 1, 2, 3, 4];
    let res = many_ref(&inputs);
    assert_eq!(res.len(), inputs.len());
    assert_eq!(res[0], &0);
    assert_eq!(res[4], &4);
}
#[test]
fn test_many_mut() {
    // 0 elements
    many_mut::<i32>(&mut []);
    // 1 element
    *many_mut(&mut [1])[0] = 1;
    // 2 elements
    *many_mut(&mut [1, 2])[0] = 1;
    *many_mut(&mut [1, 2])[1] = 2;
    // 3 elements
    *many_mut(&mut [1, 2, 3])[0] = 1;
    *many_mut(&mut [1, 2, 3])[1] = 2;
    *many_mut(&mut [1, 2, 3])[2] = 3;
    // 4 elements
    *many_mut(&mut [1, 2, 3, 4])[0] = 1;
    *many_mut(&mut [1, 2, 3, 4])[1] = 2;
    *many_mut(&mut [1, 2, 3, 4])[2] = 3;
    *many_mut(&mut [1, 2, 3, 4])[3] = 4;
    // 5 elements
    *many_mut(&mut [1, 2, 3, 4, 5])[0] = 1;
    *many_mut(&mut [1, 2, 3, 4, 5])[1] = 2;
    *many_mut(&mut [1, 2, 3, 4, 5])[2] = 3;
    *many_mut(&mut [1, 2, 3, 4, 5])[3] = 4;
    *many_mut(&mut [1, 2, 3, 4, 5])[4] = 5;
    #[cfg(not(miri))]
    for i in 0..100 {
        let mut vec = vec![0; i];
        assert_eq!(many_mut(&mut vec).len(), i);
    }
}
