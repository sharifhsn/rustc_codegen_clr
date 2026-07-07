use fxhash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use super::{
    bimap::Interned,
    cilnode::{IsPure, MethodKind},
    class::ClassDefIdx,
    Access, Assembly, BasicBlock, CILIterElem, CILNode, ClassRef, FnSig, Int, IntoAsmIndex, Type,
};
use crate::{cilnode::PtrCastRes, iter::TpeIter};
use crate::{CILRoot, IString};
pub type LocalId = u32;
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct MethodRef {
    class: Interned<ClassRef>,
    name: Interned<IString>,
    sig: Interned<FnSig>,
    kind: MethodKind,
    generics: Box<[Type]>,
}
impl IntoAsmIndex<Interned<MethodRef>> for MethodRef {
    fn into_idx(self, asm: &mut Assembly) -> Interned<MethodRef> {
        asm.alloc_methodref(self)
    }
}
impl Interned<MethodRef> {
    pub fn builtin(
        asm: &mut Assembly,
        name: &str,
        inputs: &[Type],
        output: impl IntoAsmIndex<Type>,
    ) -> Self {
        let main_module = asm.main_module();
        let inputs: Box<_> = inputs.to_vec().into();
        let output = output.into_idx(asm);
        let sig = asm.alloc_sig(FnSig::new(inputs, output));
        asm.new_methodref(*main_module, name, sig, MethodKind::Static, [])
    }
    pub fn unaligned_read(asm: &mut Assembly, tpe: Type) -> Self {
        let tpe_ptr = asm.alloc_type(tpe);
        Self::builtin(asm, "unaligned_read", &[Type::Ptr(tpe_ptr)], tpe)
    }
}
impl MethodRef {
    #[must_use]
    pub fn into_def(
        &self,
        implementation: MethodImpl,
        access: Access,
        asm: &Assembly,
    ) -> MethodDef {
        let class = asm.class_ref_to_def(self.class()).unwrap();
        let arg_names = (0..(asm[self.sig()].inputs().len()))
            .map(|_| None)
            .collect();
        MethodDef::new(
            access,
            class,
            self.name,
            self.sig,
            self.kind,
            implementation,
            arg_names,
        )
    }
    #[must_use]
    pub fn new(
        class: Interned<ClassRef>,
        name: Interned<IString>,
        sig: Interned<FnSig>,
        kind: MethodKind,
        generics: Box<[Type]>,
    ) -> Self {
        Self {
            class,
            name,
            sig,
            kind,
            generics,
        }
    }

    #[must_use]
    pub fn class(&self) -> Interned<ClassRef> {
        self.class
    }

    #[must_use]
    pub fn name(&self) -> Interned<IString> {
        self.name
    }

    #[must_use]
    pub fn sig(&self) -> Interned<FnSig> {
        self.sig
    }

    #[must_use]
    pub fn kind(&self) -> MethodKind {
        self.kind
    }

    #[must_use]
    pub fn generics(&self) -> &[Type] {
        &self.generics
    }
    /// Returns the inputs of this methods, excluding this for constructors.
    pub fn stack_inputs<'s, 'asm: 's>(&'s self, asm: &'asm Assembly) -> &'s [Type] {
        let sig = &asm[self.sig];
        match self.kind() {
            MethodKind::Static => sig.inputs(),
            MethodKind::Instance => sig.inputs(),
            MethodKind::Virtual => sig.inputs(),
            MethodKind::Constructor => &sig.inputs()[1..],
        }
    }
    /// Returns the output of this method.
    pub fn output(&self, asm: &Assembly) -> Type {
        let sig = &asm[self.sig];
        match self.kind() {
            MethodKind::Static => *sig.output(),
            MethodKind::Instance => *sig.output(),
            MethodKind::Virtual => *sig.output(),
            MethodKind::Constructor => Type::ClassRef(self.class()),
        }
    }

    pub fn aligned_alloc(asm: &mut crate::Assembly) -> MethodRef {
        let void_ptr = asm.nptr(Type::Void);
        let sig = asm.sig([Type::Int(Int::USize), Type::Int(Int::USize)], void_ptr);
        MethodRef::new(
            ClassRef::native_mem(asm),
            asm.alloc_string("AlignedAlloc"),
            sig,
            MethodKind::Static,
            vec![].into(),
        )
    }
    pub fn alloc(asm: &mut crate::Assembly) -> MethodRef {
        let sig = asm.sig([Type::Int(Int::ISize)], Type::Int(Int::ISize));
        MethodRef::new(
            ClassRef::marshal(asm),
            asm.alloc_string("AllocHGlobal"),
            sig,
            MethodKind::Static,
            vec![].into(),
        )
    }

    pub fn aligned_free(asm: &mut Assembly) -> Interned<MethodRef> {
        let void_ptr = asm.nptr(Type::Void);
        let sig = asm.sig([void_ptr], Type::Void);
        let aligned_free = asm.alloc_string("AlignedFree");
        let native_mem = ClassRef::native_mem(asm);
        asm.alloc_methodref(MethodRef::new(
            native_mem,
            aligned_free,
            sig,
            MethodKind::Static,
            [].into(),
        ))
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct MethodDef {
    access: Access,
    class: ClassDefIdx,
    name: Interned<IString>,
    sig: Interned<FnSig>,
    arg_names: Vec<Option<Interned<IString>>>,
    kind: MethodKind,
    implementation: MethodImpl,
    /// An explicit ECMA-335 `.override` (§II.15.4.2.3, a `MethodImpl` metadata row) naming the
    /// *base class* virtual method this one overrides — distinct from ordinary `MethodKind::
    /// Virtual` implicit binding (used for `implements=` interface satisfaction, which CLR binds
    /// by name+signature alone with no explicit override needed). Overriding a base class's
    /// virtual must land in that base's exact vtable slot, which implicit binding can't guarantee
    /// (a name/signature mismatch would silently create a NEW slot — a shadow method — instead of
    /// an override). `None` for every method that existed before this field: this is additive,
    /// no existing caller sets it. See `MethodDef::with_override`'s doc for the scope this was
    /// built for (a single well-known-base-virtual spike, not general base-class wrapping).
    overrides: Option<Interned<MethodRef>>,
    /// Marks this method as an ECMA-335 abstract member (§II.15.4.2.2: `Abstract` flag, RVA=0, no
    /// body) — used for synthesizing a real C# `interface` from a Rust trait (see `ClassDef::
    /// with_interface`'s doc). Distinct from every existing `MethodImpl` variant: `Missing` looks
    /// like a candidate but is NOT one — it gives a real body that throws at runtime, so reusing it
    /// for an abstract slot would silently produce a concrete throwing method instead of a genuine
    /// abstract member (a miscompilation class, not a clean failure). Kept as a plain bool flag
    /// (like `overrides`) rather than a new `MethodImpl` variant so this doesn't force an exhaustive
    /// match update across the ~30 `MethodImpl` match sites outside `il_exporter` that don't need to
    /// know about it yet — `implementation` is still set to `MethodImpl::Missing` as an unused
    /// placeholder body for an abstract method (never read: `il_exporter` checks `is_abstract()`
    /// before ever looking at `implementation`). `false` for every method that existed before this
    /// field: additive, no existing caller sets it.
    is_abstract: bool,
}

impl MethodDef {
    pub fn iter_types<'a, 'asm: 'a>(
        &'a self,
        asm: &'asm Assembly,
    ) -> impl Iterator<Item = Type> + 'a {
        let defining_class = Type::ClassRef(*self.class());
        let sig = &asm[self.sig()];
        let sig_types = sig.iter_types();
        let local_types = self.iter_locals(asm).map(|(_, tpe)| asm[*tpe]);
        let body_types = self
            .iter_cil(asm)
            .into_iter()
            .map(|cil| cil.iter_types(asm));
        let body_types = body_types.flatten();
        std::iter::once(defining_class)
            .chain(sig_types)
            .chain(local_types)
            .chain(body_types)
    }
    #[must_use]
    pub fn iter_cil<'asm: 'method, 'method>(
        &'method self,
        asm: &'asm Assembly,
    ) -> Option<impl Iterator<Item = CILIterElem> + 'method> {
        match self.resolved_implementation(asm) {
            MethodImpl::MethodBody { blocks, .. } => Some(
                blocks
                    .iter()
                    .flat_map(super::basic_block::BasicBlock::iter_roots)
                    .flat_map(|root| super::CILIter::new(asm.get_root(root).clone(), asm)),
            ),
            MethodImpl::Extern { .. } => None,
            MethodImpl::AliasFor(_) => {
                panic!("Unresolved alias returned by MethodDef::resolved_implementation")
            }
            MethodImpl::Missing => None,
        }
    }
    #[must_use]
    pub fn ref_to(&self) -> MethodRef {
        MethodRef::new(
            *self.class(),
            self.name(),
            self.sig(),
            self.kind(),
            vec![].into(),
        )
    }
    #[must_use]
    pub fn new(
        access: Access,
        class: ClassDefIdx,
        name: Interned<IString>,
        sig: Interned<FnSig>,
        kind: MethodKind,
        implementation: MethodImpl,
        arg_names: Vec<Option<Interned<IString>>>,
    ) -> Self {
        Self {
            access,
            class,
            name,
            sig,
            arg_names,
            kind,
            implementation,
            overrides: None,
            is_abstract: false,
        }
    }

    /// Marks this method as an explicit ECMA-335 `.override` of `base` — the *base class's*
    /// virtual method this one overrides (not an interface member; use `implements=`/`ClassDef::
    /// add_interface` for that, which needs no explicit override). `self.kind()` must already be
    /// `MethodKind::Virtual` for this to mean anything to an exporter; this method doesn't
    /// enforce that itself (a narrow builder, not a validated state machine) — callers are
    /// expected to already be emitting a virtual method.
    ///
    /// Scoped intentionally narrow: proven end-to-end for exactly one well-known, parameterless,
    /// unsealed base virtual (`System.Object.ToString()`, see `cargo_tests/cd_override`). Real
    /// base-class wrapping (a framework type with a non-trivial constructor, protected members,
    /// `sealed` methods the CLR would reject at load time) is a larger, separate problem — see
    /// `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md`'s Tier C finding #1 for the full scope discussion.
    #[must_use]
    pub fn with_override(mut self, base: Interned<MethodRef>) -> Self {
        self.overrides = Some(base);
        self
    }

    #[must_use]
    pub fn overrides(&self) -> Option<Interned<MethodRef>> {
        self.overrides
    }

    /// Marks this method as an ECMA-335 abstract member with no body (§II.15.4.2.2) — the shape a
    /// real C# `interface` member (or an abstract base-class member) needs. See the `is_abstract`
    /// field's doc for why this is a bool flag rather than a new `MethodImpl` variant. Scoped
    /// intentionally narrow, matching `ClassDef::with_interface`'s scope: proven for a single
    /// no-argument interface member via `cargo_tests/cd_interface_export`, IL-exporter-only.
    #[must_use]
    pub fn with_abstract(mut self) -> Self {
        self.is_abstract = true;
        self
    }

    #[must_use]
    pub fn is_abstract(&self) -> bool {
        self.is_abstract
    }

    #[must_use]
    pub fn class(&self) -> ClassDefIdx {
        self.class
    }

    #[must_use]
    pub fn name(&self) -> Interned<IString> {
        self.name
    }

    #[must_use]
    pub fn sig(&self) -> Interned<FnSig> {
        self.sig
    }

    #[must_use]
    pub fn kind(&self) -> MethodKind {
        self.kind
    }

    #[must_use]
    pub fn implementation(&self) -> &MethodImpl {
        &self.implementation
    }
    #[must_use]
    pub fn resolved_implementation<'asm: 'method, 'method>(
        &'method self,
        asm: &'asm Assembly,
    ) -> &'method MethodImpl {
        match self.implementation {
            MethodImpl::MethodBody { .. } | MethodImpl::Extern { .. } | MethodImpl::Missing => {
                &self.implementation
            }
            MethodImpl::AliasFor(method) => asm
                .method_def_from_ref(method)
                .expect("ERROR: a method is an alias for an extern function")
                .resolved_implementation(asm),
        }
    }
    pub fn implementation_mut(&mut self) -> &mut MethodImpl {
        &mut self.implementation
    }


    /// Builds a `MethodDef` directly from already-lowered, interned `BasicBlock`s. Performs
    /// debug-name uniquing on argument/local names and argument-count reconciliation against the
    /// signature.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn from_blocks(
        access: Access,
        class: ClassDefIdx,
        name: &str,
        sig: Interned<FnSig>,
        kind: MethodKind,
        blocks: Vec<BasicBlock>,
        mut locals: Vec<LocalDef>,
        mut arg_names: Vec<Option<Interned<IString>>>,
        asm: &mut Assembly,
    ) -> Self {
        // Debug-name uniquing, identical to `Method::new`.
        let mut used_names = FxHashSet::default();
        for name in arg_names
            .iter_mut()
            .chain(locals.iter_mut().map(|loc| &mut loc.0))
            .flatten()
        {
            let mut postfix = 0;
            while used_names.contains(&if postfix == 0 {
                *name
            } else {
                let new_name = format!("{name}{postfix}", name = &asm[*name]);
                asm.alloc_string(new_name)
            }) {
                postfix += 1;
            }
            if postfix != 0 {
                let new_name = format!("{name}{postfix}", name = &asm[*name]);
                *name = asm.alloc_string(new_name);
            }
            used_names.insert(*name);
        }
        let name = asm.alloc_string(name);
        let implementation = MethodImpl::MethodBody { blocks, locals };
        // Argument-count reconciliation against the signature.
        let arg_debug_count = arg_names.len();
        let arg_sig_count = asm[sig].inputs().len();
        match arg_debug_count.cmp(&arg_sig_count) {
            std::cmp::Ordering::Less => {
                arg_names.extend((arg_debug_count..arg_sig_count).map(|_| None));
            }
            std::cmp::Ordering::Equal => (),
            std::cmp::Ordering::Greater => {
                arg_names.truncate(arg_sig_count);
            }
        }
        assert_eq!(arg_names.len(), asm[sig].inputs().len());
        MethodDef::new(access, class, name, sig, kind, implementation, arg_names)
    }

    #[must_use]
    pub fn access(&self) -> &Access {
        &self.access
    }

    #[must_use]
    pub fn arg_names(&self) -> &[Option<Interned<IString>>] {
        &self.arg_names
    }

    pub(crate) fn iter_locals<'a>(
        &'a self,
        asm: &'a Assembly,
    ) -> impl Iterator<Item = &'a (Option<Interned<IString>>, Interned<Type>)> {
        match self.resolved_implementation(asm) {
            MethodImpl::MethodBody { blocks: _, locals } => locals.iter(),
            MethodImpl::Extern { .. } | MethodImpl::Missing => [].iter(),
            MethodImpl::AliasFor(_) => panic!(),
        }
    }

    /// Sets the accesibility of this method to `access`.
    pub fn set_access(&mut self, access: Access) {
        self.access = access;
    }

    pub fn stack_inputs(&self, asm: &mut Assembly) -> Vec<(Type, Option<Interned<IString>>)> {
        let mut arg_names = self.arg_names().to_vec();
        let sig = asm[self.sig()].clone();
        arg_names.extend((arg_names.len()..(sig.inputs().len())).map(|_| None));
        sig.inputs()
            .iter()
            .copied()
            .zip(arg_names.iter().copied())
            .collect::<Vec<_>>()
    }

    pub fn blocks<'s, 'asm: 's>(&'s self, asm: &'asm Assembly) -> Option<&'s [BasicBlock]> {
        self.resolved_implementation(asm)
            .blocks()
            .map(|vec| vec.as_ref())
    }
    pub fn adjust_aligement(&mut self, asm: &mut Assembly) {
        let MethodImpl::MethodBody { blocks, locals } = self.implementation_mut() else {
            return;
        };
        assert!(!blocks.is_empty());
        let to_map: Vec<_> = locals
            .iter()
            .map(|(name, tpe)| (*name, *tpe, asm.alignof_type(*tpe)))
            .enumerate()
            .collect();
        // Check which locals get their address taken.
        let mut local_address_of = vec![false; locals.len()];
        for node in blocks
            .iter()
            .flat_map(super::basic_block::BasicBlock::iter_roots)
            .flat_map(|root| super::CILIter::new(asm.get_root(root).clone(), asm))
            .filter_map(super::iter::CILIterElem::as_node)
        {
            if let CILNode::LdLocA(loc) = node {
                local_address_of[loc as usize] = true
            }
        }
        let mut preamble = vec![];
        for (local_id, (_, tpe_idx, align)) in to_map {
            if align <= asm.guaranted_align() as u64 {
                // Aligement guanrateed by .NET, skip.
                continue;
            }
            // Check that the address of this local is ever taken. If not, just ignore it.
            if !local_address_of[local_id] {
                continue;
            }
            // Change the type of the local var.
            let tpe_ptr = asm.nptr(tpe_idx);
            let tpe_ptr = asm.alloc_type(tpe_ptr);
            locals[local_id].1 = tpe_ptr;
            // Allocate a new buffer for the local var, aligned to align.
            let tpe = asm[tpe_idx];
            let local_buff = asm.alloc_node(CILNode::LocAllocAlgined {
                tpe: tpe_idx,
                align,
            });
            preamble.push(asm.alloc_root(CILRoot::StLoc(local_id as u32, local_buff)));
            // Map all usages of this local, to ensure it is propely alligned.
            blocks
                .iter_mut()
                .flat_map(|block| block.roots_mut())
                .for_each(|root_idx| {
                    let root = asm[*root_idx].clone();
                    let local_addr = asm.alloc_node(CILNode::LdLoc(local_id as u32));
                    let root = root.map(
                        asm,
                        &mut |root, _| match root {
                            CILRoot::StLoc(loc, val) if loc == local_id as u32 => {
                                CILRoot::StInd(Box::new((local_addr, val, tpe, false)))
                            }
                            _ => root,
                        },
                        &mut |node, _| match node {
                            CILNode::LdLocA(loc) if loc == local_id as u32 => {
                                CILNode::LdLoc(local_id as u32)
                            }
                            CILNode::LdLoc(loc) if loc == local_id as u32 => CILNode::LdInd {
                                addr: local_addr,
                                tpe: tpe_idx,
                                volatile: false,
                            },
                            _ => node,
                        },
                    );
                    *root_idx = asm.alloc_root(root);
                });
        }
        preamble.extend(blocks[0].roots().iter().copied());
        *blocks[0].roots_mut() = preamble;
    }

    pub(crate) fn remove_dead_blocks(&mut self, asm: &Assembly) {
        // This opt only makes sense if this method has an impl
        let Some(blocks) = self.implementation().blocks() else {
            return;
        };
        // Check if the entry block does not jump anywhere(no targets) and has no handler - if so, only keep it.
        if blocks[0].targets(asm).count() == 0 && blocks[0].handler().is_none() {
            let entry = blocks[0].clone();
            *self.implementation_mut().blocks_mut().unwrap() = vec![entry];
            return;
        }
        let mut alive: FxHashSet<_> = blocks.iter().flat_map(|block| block.targets(asm)).collect();
        // entry block is always live
        alive.insert(blocks[0].block_id());
        // if alive < total, then there are some dead blocks, then remove them.
        if alive.len() >= blocks.len() {
            return;
        }
        // If handlers jump to normal blocks, do not GC.
        if blocks
            .iter()
            .flat_map(|block| block.handler())
            .flatten()
            .flat_map(|block| block.roots())
            .any(|root| {
                matches!(
                    asm[*root],
                    CILRoot::ExitSpecialRegion {
                        target: _,
                        source: _
                    }
                )
            })
        {
            return;
        }
        //let blocks_copy = blocks.clone();
        self.implementation_mut()
            .blocks_mut()
            .unwrap()
            .retain(|block| alive.contains(&block.block_id()));
    }

    pub(crate) fn locals(&self) -> Option<&[LocalDef]> {
        let MethodImpl::MethodBody { blocks: _, locals } = self.implementation() else {
            return None;
        };
        Some(locals)
    }

    pub fn accesses_statics(&self, asm: &Assembly) -> bool {
        let Some(mut cil) = self.iter_cil(asm) else {
            return false;
        };
        cil.any(|node| {
            matches!(
                node,
                CILIterElem::Node(CILNode::LdStaticField(_) | CILNode::LdStaticFieldAddress(_))
                    | CILIterElem::Root(CILRoot::SetStaticField { .. })
            )
        })
    }
}
pub type LocalDef = (Option<Interned<IString>>, Interned<Type>);
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub enum MethodImpl {
    MethodBody {
        blocks: Vec<BasicBlock>,
        locals: Vec<LocalDef>,
    },
    Extern {
        lib: Interned<IString>,
        preserve_errno: bool,
    },
    AliasFor(Interned<MethodRef>),
    Missing,
}
impl MethodImpl {
    pub fn root_count(&self) -> usize {
        match self {
            MethodImpl::MethodBody { blocks, .. } => {
                blocks.iter().map(|block| block.roots().len()).sum()
            }
            MethodImpl::Extern { .. } => 0,
            MethodImpl::AliasFor(_) => 0,
            MethodImpl::Missing => 3,
        }
    }
    pub fn blocks_mut(&mut self) -> Option<&mut Vec<BasicBlock>> {
        match self {
            Self::MethodBody { blocks, .. } => Some(blocks),
            _ => None,
        }
    }
    pub fn blocks(&self) -> Option<&Vec<BasicBlock>> {
        match self {
            Self::MethodBody { blocks, .. } => Some(blocks),
            _ => None,
        }
    }

    /// Returns `true` if the method impl is [`Extern`].
    ///
    /// [`Extern`]: MethodImpl::Extern
    #[must_use]
    pub fn is_extern(&self) -> bool {
        matches!(self, Self::Extern { .. })
    }

    /// Shared `AggressiveInlining` (§II.23.1.11, `0x100`) hint heuristic, used identically by both
    /// `il_exporter` (as a `.method ... aggressiveinlining` keyword) and `pe_exporter` (as the
    /// `MethodDefRow.impl_flags` bit) so the two exporters never again drift out of parity on this
    /// (see `pe_exporter/pdb.rs`'s module doc, "Phase-0 probe" gap (a), for the history: `pe_exporter`
    /// silently never wrote this bit until that gap was closed).
    ///
    /// Originally scoped to single-block, handler-free, <=24-root bodies (small straight-line
    /// leaves — monomorphized closure/iterator-adapter wrappers). Widened (fractal-rs perf
    /// investigation, 2026-07) to also cover small, branchy-but-loop-free, call-free leaves like
    /// `cilly::ir::builtins::casts`'s saturating float->int cast helpers (4 blocks: a NaN check
    /// plus overflow/underflow clamps, no internal calls, ~7 roots total) — confirmed empirically
    /// that hinting a 4-block leaf like this DOES get RyuJIT to inline it (a standalone repro
    /// showed `fcvtzu` emitted inline at every call site, replacing what was otherwise 3 `blr`
    /// indirect calls per escaping pixel in the real kernel). The call-free requirement keeps this
    /// conservative: it never asks RyuJIT to inline a body that itself contains a call (no
    /// unbounded call-graph expansion risk), and the block/root caps bound JIT/codegen cost. Pure
    /// JIT hint — cannot affect correctness (verified: no typecheck/codegen semantics change).
    #[must_use]
    pub fn should_hint_aggressive_inline(&self, asm: &Assembly) -> bool {
        let Self::MethodBody { blocks, .. } = self else {
            return false;
        };
        if blocks.is_empty() || blocks.len() > 8 {
            return false;
        }
        let total_roots: usize = blocks.iter().map(|b| b.roots().len()).sum();
        if total_roots > 24 {
            return false;
        }
        if blocks.iter().any(|b| b.handler().is_some()) {
            return false;
        }
        // No block may (transitively) contain a Call/CallVirt/CallI — keeps this conservative (no
        // call-graph expansion risk) and matches the single-block case, which (being a leaf as a
        // consequence of the old scoping) never had internal calls either.
        !blocks.iter().any(|block| {
            block.roots().iter().any(|root| {
                super::CILIter::new(asm.get_root(*root).clone(), asm).any(|elem| {
                    matches!(
                        elem,
                        CILIterElem::Node(CILNode::Call(_) | CILNode::CallI(_))
                            | CILIterElem::Root(CILRoot::Call(_))
                    )
                })
            })
        })
    }
    // While this function is a bit long, this is not an issue.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn merge_cctor_impls(&mut self, implementation: &MethodImpl, asm: &Assembly) {
        let tmp = match (&self, &implementation) {
            (
                MethodImpl::MethodBody { blocks, locals },
                MethodImpl::MethodBody {
                    blocks: other_blocks,
                    locals: other_locals,
                },
            ) => {
                assert_eq!(locals, other_locals);
                let mut blocks = blocks.clone();
                // First, check that 1st blocks end in `VoidRet`, and remove it.
                let last_root = if blocks.is_empty() {
                    blocks.push(BasicBlock::new(vec![], 0, None));
                    CILRoot::VoidRet
                } else {
                    blocks
                        .last_mut()
                        .unwrap()
                        .roots_mut()
                        .pop()
                        .map_or(CILRoot::VoidRet, |root_id| asm.get_root(root_id).clone())
                };
                assert_eq!(last_root, CILRoot::VoidRet);
                assert_eq!(
                    other_blocks.len(),
                    1,
                    "Only merging one block method impls is currently supported"
                );
                blocks
                    .last_mut()
                    .unwrap()
                    .roots_mut()
                    .extend(other_blocks.last().unwrap().roots());
                MethodImpl::MethodBody {
                    blocks,
                    locals: locals.clone(),
                }
            }
            (MethodImpl::MethodBody { .. }, MethodImpl::Extern { .. }) => {
                panic!("Unmergable method impl: Can't merge MethodBody with Extern.")
            }
            (MethodImpl::MethodBody { .. }, MethodImpl::AliasFor(_)) => {
                panic!("Unmergable method impl: Can't merge MethodBody with AliasFor.")
            }

            (MethodImpl::Extern { .. }, MethodImpl::MethodBody { .. }) => {
                panic!("Unmergable method impl: Can't merge Extern with MethodBody.")
            }
            (
                MethodImpl::Extern {
                    lib,
                    preserve_errno,
                },
                MethodImpl::Extern {
                    lib: liba,
                    preserve_errno: preserve_errnoa,
                },
            ) => {
                assert_eq!(lib, liba);
                assert_eq!(preserve_errno, preserve_errnoa);
                self.clone()
            }
            (MethodImpl::Extern { .. }, MethodImpl::AliasFor(_)) => {
                panic!("Unmergable method impl: Can't merge Extern with AliasFor.")
            }
            (
                MethodImpl::Extern {
                    lib,
                    preserve_errno,
                },
                MethodImpl::Missing,
            )
            | (
                MethodImpl::Missing,
                MethodImpl::Extern {
                    lib,
                    preserve_errno,
                },
            ) => MethodImpl::Extern {
                lib: *lib,
                preserve_errno: *preserve_errno,
            },
            (
                MethodImpl::AliasFor(_),
                MethodImpl::MethodBody { .. } | MethodImpl::Extern { .. },
            ) => {
                panic!("Unmergable method impl: can't merge alias.")
            }
            (MethodImpl::AliasFor(a), MethodImpl::AliasFor(b)) => {
                assert_eq!(a, b);
                self.clone()
            }
            (MethodImpl::AliasFor(alias), MethodImpl::Missing)
            | (MethodImpl::Missing, MethodImpl::AliasFor(alias)) => MethodImpl::AliasFor(*alias),
            (MethodImpl::Missing, MethodImpl::MethodBody { blocks, locals })
            | (MethodImpl::MethodBody { blocks, locals }, MethodImpl::Missing) => {
                MethodImpl::MethodBody {
                    blocks: blocks.clone(),
                    locals: locals.clone(),
                }
            }

            (MethodImpl::Missing, MethodImpl::Missing) => MethodImpl::Missing,
        };
        *self = tmp;
    }

    pub(crate) fn realloc_locals(&mut self, asm: &mut Assembly) {
        // Optimization only suported for methods with locals
        let MethodImpl::MethodBody {
            blocks,
            ref mut locals,
        } = self
        else {
            return;
        };
        let mut new_locals = std::sync::Mutex::new(Vec::new());
        let local_map = std::sync::Mutex::new(FxHashMap::default());
        for block in blocks.iter_mut() {
            block.map_roots(
                asm,
                &mut |root, _| match root {
                    CILRoot::StLoc(loc, tree) => CILRoot::StLoc(
                        match local_map.lock().unwrap().entry(loc) {
                            std::collections::hash_map::Entry::Occupied(val) => *val.get(),
                            std::collections::hash_map::Entry::Vacant(empty) => {
                                let mut new_locals = new_locals.lock().unwrap();
                                let new_idx = new_locals.len();
                                new_locals.push(locals[loc as usize]);
                                *empty.insert(new_idx as u32)
                            }
                        },
                        tree,
                    ),
                    _ => root,
                },
                &mut |node, _| match node {
                    CILNode::LdLoc(loc) => {
                        CILNode::LdLoc(match local_map.lock().unwrap().entry(loc) {
                            std::collections::hash_map::Entry::Occupied(val) => *val.get(),
                            std::collections::hash_map::Entry::Vacant(empty) => {
                                let mut new_locals = new_locals.lock().unwrap();
                                let new_idx = new_locals.len();
                                new_locals.push(locals[loc as usize]);
                                *empty.insert(new_idx as u32)
                            }
                        })
                    }
                    CILNode::LdLocA(loc) => {
                        CILNode::LdLocA(match local_map.lock().unwrap().entry(loc) {
                            std::collections::hash_map::Entry::Occupied(val) => *val.get(),
                            std::collections::hash_map::Entry::Vacant(empty) => {
                                let mut new_locals = new_locals.lock().unwrap();
                                let new_idx = new_locals.len();
                                new_locals.push(locals[loc as usize]);
                                *empty.insert(new_idx as u32)
                            }
                        })
                    }
                    _ => node,
                },
            );
        }
        // Swap new and locals
        std::mem::swap(locals, new_locals.get_mut().unwrap());
    }

    pub(crate) fn wrapper(
        alloc: Interned<MethodRef>,
        mref: &MethodRef,
        asm: &mut Assembly,
    ) -> MethodImpl {
        let sig = asm[asm[alloc].sig()].clone();
        let args = sig
            .inputs()
            .iter()
            .enumerate()
            .map(|(idx, _)| asm.alloc_node(CILNode::LdArg(idx.try_into().unwrap())))
            .collect();
        let roots = if asm.sizeof_type(*sig.output()) == 0 {
            let call = asm.alloc_root(CILRoot::Call(Box::new((alloc, args, IsPure::NOT))));
            vec![call, asm.alloc_root(CILRoot::VoidRet)]
        } else {
            let val = asm.alloc_node(CILNode::Call(Box::new((alloc, args, IsPure::NOT))));
            if asm[mref.sig()].output() != sig.output() {
                match (asm[mref.sig()].output(), sig.output()) {
                    (Type::Ptr(a), Type::Ptr(_)) => {
                        let val =
                            asm.alloc_node(CILNode::PtrCast(val, Box::new(PtrCastRes::Ptr(*a))));
                        vec![asm.alloc_root(CILRoot::Ret(val))]
                    }
                    _ => todo!(),
                }
            } else {
                vec![asm.alloc_root(CILRoot::Ret(val))]
            }
        };
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals: vec![].into(),
        }
    }
}
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MethodDefIdx(pub Interned<MethodRef>);
impl MethodDefIdx {
    pub(crate) fn from_raw(method: Interned<MethodRef>) -> MethodDefIdx {
        Self(method)
    }
}

impl std::ops::Deref for MethodDefIdx {
    type Target = Interned<MethodRef>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Interned<MethodRef> {
    pub fn abort(asm: &mut Assembly) -> Interned<MethodRef> {
        let main = asm.main_module();
        let sig = asm.sig([], Type::Void);
        asm.new_methodref(*main, "abort", sig, MethodKind::Static, vec![])
    }
}
#[test]
fn locals() {
    fn method(locals: &[LocalDef], asm: &mut Assembly) -> MethodDef {
        let name: Interned<IString> = asm.alloc_string("DoSomething");
        let mimpl = MethodImpl::MethodBody {
            blocks: vec![],
            locals: locals.into(),
        };
        let main_module = asm.main_module();
        let sig = asm.sig([], Type::Void);
        MethodDef::new(
            Access::Extern,
            main_module,
            name,
            sig,
            MethodKind::Static,
            mimpl,
            vec![],
        )
    }
    let mut asm = Assembly::default();
    assert_eq!(method(&[], &mut asm).iter_locals(&asm).count(), 0);
    let tpe = asm.alloc_type(Type::Bool);
    let tpe2 = asm.alloc_type(Type::Bool);
    assert_eq!(
        method(&[(None, tpe)], &mut asm).iter_locals(&asm).count(),
        1
    );
    assert_eq!(
        method(&[(None, tpe), (None, tpe2)], &mut asm)
            .iter_locals(&asm)
            .cloned()
            .collect::<Vec<(Option<Interned<IString>>, _)>>(),
        vec![(None, tpe), (None, tpe2)]
    );
    let mut method = method(&[(None, tpe), (None, tpe2)], &mut asm);
    assert_eq!(
        method
            .iter_locals(&asm)
            .cloned()
            .collect::<Vec<(Option<Interned<IString>>, _)>>(),
        vec![(None, tpe), (None, tpe2)]
    );
    method.implementation.realloc_locals(&mut asm);
    assert_eq!(method.iter_locals(&asm).count(), 0);
}
#[test]
fn test_extern() {
    assert!(!MethodImpl::MethodBody {
        blocks: vec![],
        locals: vec![],
    }
    .is_extern());
    let mut asm = Assembly::default();
    let name: Interned<IString> = asm.alloc_string("libsomething.so");
    assert!(MethodImpl::Extern {
        lib: name,
        preserve_errno: false,
    }
    .is_extern())
}
#[test]
fn cil() {
    fn method(roots: &[Interned<CILRoot>], asm: &mut Assembly) -> MethodDef {
        let name: Interned<IString> = asm.alloc_string("DoSomething");
        let mimpl = MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots.to_vec(), 0, None)],
            locals: vec![],
        };
        let main_module = asm.main_module();
        let sig = asm.sig([], Type::Void);
        MethodDef::new(
            Access::Extern,
            main_module,
            name,
            sig,
            MethodKind::Static,
            mimpl,
            vec![],
        )
    }
    let mut asm = Assembly::default();
    assert_eq!(
        method(&[], &mut asm)
            .iter_cil(&asm)
            .map(|iter| iter.count()),
        Some(0)
    );
    let void_ret = asm.alloc_root(CILRoot::VoidRet);
    assert_eq!(
        method(&[void_ret], &mut asm)
            .iter_cil(&asm)
            .map(|iter| iter.collect::<Vec<_>>()),
        Some(vec![CILIterElem::Root(CILRoot::VoidRet)])
    );
    let const0 = asm.alloc_node(crate::Const::I32(0));
    let const0_ret = asm.alloc_root(CILRoot::Ret(const0));
    assert_eq!(
        method(&[const0_ret], &mut asm)
            .iter_cil(&asm)
            .map(|iter| iter.collect::<Vec<_>>()),
        Some(vec![
            CILIterElem::Root(CILRoot::Ret(const0)),
            CILIterElem::Node(crate::Const::I32(0).into()),
        ])
    );
    let name: Interned<IString> = asm.alloc_string("DoSomething");
    let main_module = asm.main_module();
    let sig = asm.sig([], Type::Void);
    assert_eq!(
        MethodDef::new(
            Access::Extern,
            main_module,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::Extern {
                lib: name,
                preserve_errno: false,
            },
            vec![],
        )
        .iter_cil(&asm)
        .map(|iter| iter.count()),
        None,
    );
    assert_eq!(
        MethodDef::new(
            Access::Extern,
            main_module,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![],
        )
        .iter_cil(&asm)
        .map(|iter| iter.count()),
        None,
    );
}
/// Regression test for the `pe_exporter`/`il_exporter` `AggressiveInlining` parity gap +
/// widening (fractal-rs perf investigation, 2026-07 — see `should_hint_aggressive_inline`'s doc).
/// A small, branchy-but-loop-free, call-free multi-block leaf — the exact shape
/// `cilly::ir::builtins::casts`'s saturating float->int cast helpers produce (a NaN-check block
/// plus 3 single-`Ret` blocks) — must be hinted even though it is NOT single-block.
#[test]
fn should_hint_aggressive_inline_true_for_a_small_multi_block_call_free_leaf() {
    let mut asm = Assembly::default();
    let zero = asm.alloc_node(crate::Const::I32(0));
    let one = asm.alloc_node(crate::Const::I32(1));
    let ret0 = asm.alloc_root(CILRoot::Ret(zero));
    let ret1 = asm.alloc_root(CILRoot::Ret(one));
    // 4 blocks total (mirrors casts.rs's `float_to_int` generator shape), no calls anywhere,
    // well under the root/block caps.
    let blocks = vec![
        BasicBlock::new(vec![ret0], 0, None),
        BasicBlock::new(vec![ret1], 1, None),
        BasicBlock::new(vec![ret0], 2, None),
        BasicBlock::new(vec![ret1], 3, None),
    ];
    let mimpl = MethodImpl::MethodBody {
        blocks,
        locals: vec![],
    };
    assert!(
        mimpl.should_hint_aggressive_inline(&asm),
        "a small, call-free, handler-free, loop-free multi-block leaf must be hinted"
    );
}
/// Negative case: a body containing a `Call` anywhere must NOT be hinted — keeps the widened
/// heuristic conservative (no call-graph-expansion risk from asking RyuJIT to inline a body that
/// itself calls out).
#[test]
fn should_hint_aggressive_inline_false_when_the_body_contains_a_call() {
    let mut asm = Assembly::default();
    let main_module = asm.main_module();
    let void_sig = asm.sig([], Type::Void);
    let callee_name = asm.alloc_string("SomeCallee");
    let mref = MethodRef::new(
        *main_module,
        callee_name,
        void_sig,
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    let call_root = asm.alloc_root(CILRoot::Call(Box::new((mref, vec![].into(), IsPure::NOT))));
    let void_ret = asm.alloc_root(CILRoot::VoidRet);
    let blocks = vec![BasicBlock::new(vec![call_root, void_ret], 0, None)];
    let mimpl = MethodImpl::MethodBody {
        blocks,
        locals: vec![],
    };
    assert!(
        !mimpl.should_hint_aggressive_inline(&asm),
        "a body containing a Call must never be hinted, regardless of size"
    );
}
/// Negative case: a body with a handler (try/catch) must NOT be hinted, matching the original
/// single-block heuristic's handler-free requirement.
#[test]
fn should_hint_aggressive_inline_false_when_a_block_has_a_handler() {
    let mut asm = Assembly::default();
    let void_ret = asm.alloc_root(CILRoot::VoidRet);
    let handler_block = BasicBlock::new(vec![void_ret], 1, None);
    let blocks = vec![BasicBlock::new(
        vec![void_ret],
        0,
        Some(vec![handler_block]),
    )];
    let mimpl = MethodImpl::MethodBody {
        blocks,
        locals: vec![],
    };
    assert!(
        !mimpl.should_hint_aggressive_inline(&asm),
        "a block with a handler must never be hinted"
    );
}
/// Negative case: too many blocks (beyond the small-leaf cap) must NOT be hinted, bounding
/// JIT/codegen cost the same way the original 24-root cap did for the single-block case.
#[test]
fn should_hint_aggressive_inline_false_when_too_many_blocks() {
    let mut asm = Assembly::default();
    let void_ret = asm.alloc_root(CILRoot::VoidRet);
    let blocks: Vec<_> = (0..9)
        .map(|id| BasicBlock::new(vec![void_ret], id, None))
        .collect();
    let mimpl = MethodImpl::MethodBody {
        blocks,
        locals: vec![],
    };
    assert!(
        !mimpl.should_hint_aggressive_inline(&asm),
        "a leaf with more than the small-body block cap must not be hinted"
    );
}
/// Non-`MethodBody` implementations (`Extern`/`AliasFor`/`Missing`) must never be hinted — there
/// is no IL body to attach the hint to.
#[test]
fn should_hint_aggressive_inline_false_for_non_method_body_impls() {
    let mut asm = Assembly::default();
    let lib_name = asm.alloc_string("libsomething.so");
    assert!(!MethodImpl::Extern {
        lib: lib_name,
        preserve_errno: false,
    }
    .should_hint_aggressive_inline(&asm));
    assert!(!MethodImpl::Missing.should_hint_aggressive_inline(&asm));
}
