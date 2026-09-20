use std::fmt::Debug;

use fxhash::FxHashSet;

use super::{
    Assembly, CILNode, CILRoot, ClassRef, FnSig, MethodDef, MethodRef, Type, bimap::Interned,
    class::ClassDefIdx, method::MethodImpl,
};
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub enum CILIterElem {
    Node(CILNode),
    Root(CILRoot),
}

impl CILIterElem {
    #[must_use]
    pub fn as_node(self) -> Option<CILNode> {
        if let Self::Node(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
impl From<CILRoot> for CILIterElem {
    fn from(v: CILRoot) -> Self {
        Self::Root(v)
    }
}
impl From<CILNode> for CILIterElem {
    fn from(v: CILNode) -> Self {
        Self::Node(v)
    }
}

/// Memoized traversal of every type identity reachable through IR metadata.
///
/// A `ClassRef` is not just its name: its constructed generic arguments are part of the type, and
/// a `MethodRef` carries three independent type-bearing edges (owner, signature, and method generic
/// arguments). In particular, an external call has no local `MethodDef` whose metadata can be used
/// as a substitute. Keeping these rules in one walker prevents DCE consumers from each growing a
/// subtly different, incomplete notion of reachability.
pub(crate) struct SemanticReachability<'asm> {
    asm: &'asm Assembly,
    types: FxHashSet<Interned<Type>>,
    signatures: FxHashSet<Interned<FnSig>>,
    methods: FxHashSet<Interned<MethodRef>>,
    classes: FxHashSet<Interned<ClassRef>>,
    class_definitions: FxHashSet<ClassDefIdx>,
}

impl<'asm> SemanticReachability<'asm> {
    pub(crate) fn new(asm: &'asm Assembly) -> Self {
        Self {
            asm,
            types: FxHashSet::default(),
            signatures: FxHashSet::default(),
            methods: FxHashSet::default(),
            classes: FxHashSet::default(),
            class_definitions: FxHashSet::default(),
        }
    }

    pub(crate) fn visit_type(&mut self, tpe: Type) {
        match tpe {
            Type::Ptr(inner) | Type::Ref(inner) | Type::PlatformArray { elem: inner, .. } => {
                self.visit_type_id(inner);
            }
            Type::ClassRef(class) => self.visit_class_ref(class),
            Type::FnPtr(signature) => self.visit_signature(signature),
            Type::Int(_)
            | Type::Float(_)
            | Type::PlatformString
            | Type::PlatformChar
            | Type::PlatformGeneric(_, _)
            | Type::PlatformObject
            | Type::Bool
            | Type::Void
            | Type::SIMDVector(_) => {}
        }
    }

    fn visit_type_id(&mut self, tpe: Interned<Type>) {
        if self.types.insert(tpe) {
            self.visit_type(self.asm[tpe]);
        }
    }

    fn visit_signature(&mut self, signature: Interned<FnSig>) {
        if !self.signatures.insert(signature) {
            return;
        }
        let types: Vec<_> = self.asm[signature].iter_types().collect();
        for tpe in types {
            self.visit_type(tpe);
        }
    }

    pub(crate) fn visit_class_ref(&mut self, class: Interned<ClassRef>) {
        if !self.classes.insert(class) {
            return;
        }
        let generics = self.asm[class].generics().to_vec();
        for generic in generics {
            self.visit_type(generic);
        }
    }

    pub(crate) fn visit_method_ref(&mut self, method: Interned<MethodRef>) {
        if !self.methods.insert(method) {
            return;
        }
        let types: Vec<_> = self.asm[method].iter_types(self.asm).collect();
        for tpe in types {
            self.visit_type(tpe);
        }
    }

    pub(crate) fn visit_method_definition(&mut self, method: &MethodDef) {
        let types: Vec<_> = method.iter_types(self.asm).collect();
        for tpe in types {
            self.visit_type(tpe);
        }
        if let MethodImpl::AliasFor(target) = method.implementation() {
            self.visit_method_ref(*target);
        }
        if let Some(target) = method.overrides() {
            self.visit_method_ref(target);
        }
    }

    pub(crate) fn visit_class_definition(&mut self, class: ClassDefIdx) {
        if !self.class_definitions.insert(class) {
            return;
        }
        self.visit_class_ref(class.0);
        let definition = &self.asm[class];
        let types: Vec<_> = definition.iter_types().collect();
        let accessors: Vec<_> = definition.iter_member_method_refs().collect();
        for tpe in types {
            self.visit_type(tpe);
        }
        for accessor in accessors {
            self.visit_method_ref(accessor);
        }
    }

    /// Follows every discovered assembly-local class definition until its metadata closure is
    /// complete. Constructed external classes remain references, while their generic arguments are
    /// still traversed by `visit_class_ref` above.
    pub(crate) fn close_local_class_definitions(&mut self) {
        loop {
            let pending: Vec<_> = self
                .classes
                .iter()
                .filter_map(|class| self.asm.class_ref_to_def(*class))
                .filter(|class| !self.class_definitions.contains(class))
                .collect();
            if pending.is_empty() {
                return;
            }
            for class in pending {
                self.visit_class_definition(class);
            }
        }
    }

    pub(crate) fn class_definitions(&self) -> impl Iterator<Item = ClassDefIdx> + '_ {
        self.class_definitions.iter().copied()
    }

    pub(crate) fn into_class_refs(self) -> Vec<Interned<ClassRef>> {
        let mut classes: Vec<_> = self.classes.into_iter().collect();
        classes.sort_unstable_by_key(|class| class.inner());
        classes
    }
}
pub struct CILIter<'asm> {
    elems: Vec<CILIterElem>,
    asm: &'asm Assembly,
}

impl<'asm> CILIter<'asm> {
    pub fn new(elems: impl Into<CILIterElem>, asm: &'asm Assembly) -> Self {
        Self {
            elems: vec![elems.into()],
            asm,
        }
    }
}

impl Iterator for CILIter<'_> {
    type Item = CILIterElem;

    fn next(&mut self) -> Option<Self::Item> {
        let elem = self.elems.pop()?;
        let mut children = Vec::new();
        match &elem {
            CILIterElem::Node(node) => {
                node.visit_child_nodes(|child| {
                    children.push(CILIterElem::Node(self.asm.get_node(*child).clone()));
                });
            }
            CILIterElem::Root(root) => {
                root.visit_nodes(|child| {
                    children.push(CILIterElem::Node(self.asm.get_node(*child).clone()));
                });
                root.visit_child_roots(|child| {
                    children.push(CILIterElem::Root(self.asm.get_root(*child).clone()));
                });
            }
        }
        self.elems.extend(children.into_iter().rev());
        Some(elem)
    }
}

pub struct CILIterMut<'start> {
    start: Either<&'start mut CILNode, &'start mut CILRoot>,
    idx: u32,
    elems: Vec<(CILIterElem, usize)>,
    asm: &'start mut Assembly,
}
#[derive(Debug)]
pub enum Either<A, B> {
    A(A),
    B(B),
}

impl<A, B> From<A> for Either<A, B> {
    fn from(v: A) -> Self {
        Self::A(v)
    }
}

pub trait CILIterMutTrait {
    type Ctx;
    type A: Debug;
    type B: Debug;
    fn advance(&mut self);
    #[allow(clippy::type_complexity)]
    fn get(&mut self) -> Option<(&mut Self::Ctx, Either<&mut Self::A, &mut Self::B>)>;
    #[allow(clippy::type_complexity)]
    fn next(&mut self) -> Option<(&mut Self::Ctx, Either<&mut Self::A, &mut Self::B>)> {
        self.advance();
        self.get()
    }
}
impl CILIterMutTrait for CILIterMut<'_> {
    type Ctx = Assembly;

    type A = CILNode;
    type B = CILRoot;

    fn advance(&mut self) {
        let mut curr: Option<CILIterElem> = None;
        'main: loop {
            if self.elems.is_empty() {
                if self.idx == u32::MAX {
                    self.idx = 0;
                    return;
                } else {
                    match &mut self.start {
                        Either::A(CILNode::BinOp(lhs, rhs, _)) => match self.idx {
                            0 => {
                                let lhs = self.asm.get_node(*lhs);
                                self.elems.push((CILIterElem::Node(lhs.clone()), 0));
                                continue 'main;
                            }
                            1 => {
                                let curr = curr.take().expect("Iterator error").as_node().unwrap();
                                *lhs = self.asm.alloc_node(curr);

                                let rhs = self.asm.get_node(*rhs);
                                self.elems.push((CILIterElem::Node(rhs.clone()), 0));
                                continue 'main;
                            }
                            2 => {
                                let curr = curr.take().expect("Iterator error").as_node().unwrap();
                                *rhs = self.asm.alloc_node(curr);
                                self.idx += 1;
                                return;
                            }
                            _ => return,
                        },
                        Either::A(node) => todo!("{node:?}"),
                        Either::B(root) => todo!("{root:?}"),
                    }
                }
            } else {
                let (elem, idx) = self.elems.iter_mut().last().unwrap();
                if *idx == 0 {
                    *idx += 1;
                    return;
                }
                match elem {
                    CILIterElem::Node(CILNode::Const(_)) => {
                        assert!(curr.is_none());
                        curr = Some(self.elems.pop().unwrap().0);
                        if self.elems.is_empty() {
                            self.idx += 1;
                        } else {
                            let (_, idx) = self.elems.iter_mut().last().unwrap();
                            *idx += 1;
                        }
                        continue 'main;
                    }
                    CILIterElem::Node(_) => todo!(),
                    CILIterElem::Root(_) => todo!(),
                }
            }
        }
    }

    fn get(&mut self) -> Option<(&mut Self::Ctx, Either<&mut Self::A, &mut Self::B>)> {
        {
            if self.elems.is_empty() {
                if self.idx == 0 {
                    Some((
                        self.asm,
                        match &mut self.start {
                            Either::A(a) => Either::A(a),
                            Either::B(b) => Either::B(b),
                        },
                    ))
                } else {
                    None
                }
            } else {
                let (elem, idx) = self.elems.iter_mut().last()?;
                if *idx == 1 {
                    return Some((
                        self.asm,
                        match elem {
                            CILIterElem::Node(node) => Either::A(node),
                            CILIterElem::Root(root) => Either::B(root),
                        },
                    ));
                }
                match elem {
                    //CILIterElem::Node(CILNode::Const(_)) =>
                    CILIterElem::Node(node) => todo!("node:{node:?}"),
                    CILIterElem::Root(root) => todo!("root:{root:?}"),
                }
            }
        }
    }
}
impl<'start> CILIterMut<'start> {
    pub fn new<'asm: 'start>(
        start: impl Into<Either<&'start mut CILNode, &'start mut CILRoot>>,
        asm: &'asm mut Assembly,
    ) -> Self {
        Self {
            start: start.into(),
            idx: u32::MAX,
            elems: vec![],
            asm,
        }
    }
}
pub(crate) trait TpeIter<'this>: Sized + 'this {
    fn iter_types<'asm: 'this>(self, asm: &'asm Assembly) -> impl Iterator<Item = Type> + 'this;
}
impl<'this, T: Iterator<Item = CILIterElem> + 'this> TpeIter<'this> for T {
    fn iter_types<'asm: 'this>(self, asm: &'asm Assembly) -> impl Iterator<Item = Type> + 'this {
        let this = self;
        this.filter_map(|cil_item| {
            let iter: Option<Box<dyn Iterator<Item = Type>>> = match cil_item {
                crate::CILIterElem::Node(node) => match node {
                    CILNode::Const(_)
                    | CILNode::BinOp(_, _, _)
                    | CILNode::UnOp(_, _)
                    | CILNode::LdLoc(_)
                    | CILNode::LdLocA(_)
                    | CILNode::LdArg(_)
                    | CILNode::LdArgA(_)
                    | CILNode::IntCast { .. }
                    | CILNode::FloatCast { .. }
                    | CILNode::RefToPtr(_)
                    | CILNode::GetException
                    | CILNode::LocAlloc { .. }
                    | CILNode::LdLen(_)
                    | CILNode::LdElelemRef { .. } => None,
                    CILNode::Call(info) => Some(Box::new(asm[info.0].iter_types(asm))),
                    CILNode::LdFtn(method) => Some(Box::new(asm[method].iter_types(asm))),
                    CILNode::PtrCast(_, res) => match res.as_ref() {
                        crate::cilnode::PtrCastRes::Ptr(inner)
                        | crate::cilnode::PtrCastRes::Ref(inner) => {
                            Some(Box::new(std::iter::once(asm[*inner])))
                        }
                        crate::cilnode::PtrCastRes::FnPtr(sig) => {
                            Some(Box::new(asm[*sig].iter_types()))
                        }
                        crate::cilnode::PtrCastRes::USize | crate::cilnode::PtrCastRes::ISize => {
                            None
                        }
                    },
                    CILNode::LdFieldAddress { field, .. } | CILNode::LdField { field, .. } => {
                        let field = asm.get_field(field);
                        let class = Type::ClassRef(field.owner());
                        let tpe = field.tpe();
                        Some(Box::new([class, tpe].into_iter()))
                    }
                    CILNode::LdInd { tpe, .. } => Some(Box::new(std::iter::once(asm[tpe]))),
                    CILNode::SizeOf(tpe)
                    | CILNode::IsInst(_, tpe)
                    | CILNode::CheckedCast(_, tpe)
                    | CILNode::LdTypeToken(tpe)
                    | CILNode::UnboxAny { tpe, .. }
                    | CILNode::Box { tpe, .. }
                    | CILNode::NewArr { elem: tpe, .. }
                    | CILNode::LdElem { elem: tpe, .. }
                    | CILNode::LocAllocAlgined { tpe, .. } => {
                        Some(Box::new(std::iter::once(asm[tpe])))
                    }
                    CILNode::CallI(info) => Some(Box::new(asm[info.1].iter_types())),
                    CILNode::LdStaticField(sfld) | CILNode::LdStaticFieldAddress(sfld) => {
                        let field = asm.get_static_field(sfld);
                        let class = Type::ClassRef(field.owner());
                        let tpe = field.tpe();
                        Some(Box::new([class, tpe].into_iter()))
                    }
                },
                crate::CILIterElem::Root(root) => match root {
                    CILRoot::StLoc(_, _)
                    | CILRoot::StArg(_, _)
                    | CILRoot::Ret(_)
                    | CILRoot::Pop(_)
                    | CILRoot::Throw(_)
                    | CILRoot::VoidRet
                    | CILRoot::Break
                    | CILRoot::Nop
                    | CILRoot::InitFragmentBoundary
                    | CILRoot::Branch(_)
                    | CILRoot::SourceFileInfo { .. }
                    | CILRoot::ExitSpecialRegion { .. }
                    | CILRoot::InitBlk(_)
                    | CILRoot::CpBlk(_)
                    | CILRoot::ReThrow
                    // The protected child root is yielded separately by the iterator (see the
                    // `TerminateRegion` arm in `next`), so this region itself contributes no types.
                    | CILRoot::TerminateRegion { .. }
                    | CILRoot::Unreachable(_)
                    | CILRoot::CallI(_) => None,
                    CILRoot::SetStaticField { field, .. } => {
                        let field = asm.get_static_field(field);
                        let class = Type::ClassRef(field.owner());
                        let tpe = field.tpe();
                        Some(Box::new([class, tpe].into_iter()))
                    }
                    CILRoot::SetField(info) => {
                        let field = asm.get_field(info.0);
                        let class = Type::ClassRef(field.owner());
                        let tpe = field.tpe();
                        Some(Box::new([class, tpe].into_iter()))
                    }
                    CILRoot::CpObj { tpe, .. }
                    | CILRoot::InitObj(_, tpe)
                    | CILRoot::StElem { elem: tpe, .. } => {
                        Some(Box::new(std::iter::once(asm[tpe])))
                    }
                    CILRoot::Call(info) => Some(Box::new(asm[info.0].iter_types(asm))),
                    CILRoot::StInd(info) => Some(Box::new(std::iter::once(info.2))),
                },
            };
            iter
        })
        .flatten()
    }
}
#[test]
pub fn nodes() {
    use super::{BinOp, Const};
    let mut asm = Assembly::default();
    let add = asm.biop(Const::I8(2), Const::I8(1), BinOp::Add);
    let mut add = asm[add].clone();
    let mut iter = CILIterMut::new(&mut add, &mut asm);
    assert!(matches!(
        iter.next(),
        Some((_, Either::A(CILNode::BinOp(_, _, BinOp::Add))))
    ));
    assert!(matches!(
        iter.next(),
        Some((_, Either::A(CILNode::Const(_))))
    ));
    assert!(matches!(
        iter.next(),
        Some((_, Either::A(CILNode::Const(_))))
    ));
    assert!(iter.next().is_none());
}
