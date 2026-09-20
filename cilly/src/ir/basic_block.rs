use fxhash::{FxBuildHasher, FxHashSet};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use super::{
    Assembly, BranchCond, CILNode, CILRoot, Const,
    asm_link::{RelocateCtx, RelocateValue},
    bimap::Interned,
    cilroot::CmpKind,
    opt,
};
pub type BlockId = u32;
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
/// A basic block - sequence of roots, protected by a handler, identified by a unique, per-method id.
/// The first block in a method ought to have the id 0, and ought not be jumped to.
pub struct BasicBlock {
    roots: Vec<Interned<CILRoot>>,
    block_id: BlockId,
    handler: Option<Vec<Self>>,
    /// An *unresolved* exception-handler target id, set during MIR lowering and consumed by
    /// [`BasicBlock::resolve_exception_handlers`] (which turns it into `handler`).
    /// `None` for blocks with no handler or already-resolved handlers.
    #[serde(default)]
    handler_id: Option<BlockId>,
}

impl RelocateValue for BasicBlock {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self {
            roots,
            block_id,
            handler,
            handler_id,
        } = self;
        Self {
            roots: roots
                .into_iter()
                .map(|root| ctx.root(destination, root))
                .collect(),
            block_id,
            handler: handler.map(|blocks| {
                blocks
                    .into_iter()
                    .map(|block| block.relocate(ctx, destination))
                    .collect()
            }),
            handler_id,
        }
    }
}

impl BasicBlock {
    /// Canonicalizes the executable prefix of this block and its legacy handler blocks.
    ///
    /// MIR lowering represents a conditional transfer as a conditional branch followed by its
    /// fallthrough branch. After monomorphization, the first transfer can already be unconditional
    /// (for example an `if const` whose generic size predicate is false). Every root after an
    /// unconditional transfer is unreachable CIL and must not contribute call-graph or CFG edges.
    /// Direct constant branch conditions are folded here as a mandatory correctness pass, without
    /// optimizer fuel.
    pub(crate) fn canonicalize_control_flow(&mut self, asm: &mut Assembly) {
        // Legacy embedded handlers append addressable `ExitSpecialRegion` launching pads after
        // ordinary branches in the protected/handler block. They look like post-terminator roots,
        // but branches target the labels they define, so truncating that suffix would corrupt EH.
        // Canonical `RegionBody` blocks never contain these pads; exporters materialize them only
        // on a scratch clone after this pass. Preserve legacy blocks conservatively.
        let has_legacy_leave_pads = self
            .roots
            .iter()
            .any(|root| matches!(asm.get_root(*root), CILRoot::ExitSpecialRegion { .. }));
        if has_legacy_leave_pads {
            if let Some(handler) = &mut self.handler {
                for block in handler {
                    block.canonicalize_control_flow(asm);
                }
            }
            return;
        }

        let mut canonical = Vec::with_capacity(self.roots.len());
        for root in std::mem::take(&mut self.roots) {
            match asm.get_root(root).clone() {
                CILRoot::Branch(info) => {
                    let (target, sub_target, cond) = *info;
                    match cond
                        .as_ref()
                        .and_then(|condition| constant_branch_value(condition, asm))
                    {
                        Some(false) => continue,
                        Some(true) => {
                            canonical.push(
                                asm.alloc_root(CILRoot::Branch(Box::new((
                                    target, sub_target, None,
                                )))),
                            );
                            break;
                        }
                        None => {
                            canonical.push(root);
                            if cond.is_none() {
                                break;
                            }
                        }
                    }
                }
                CILRoot::Ret(_)
                | CILRoot::VoidRet
                | CILRoot::Throw(_)
                | CILRoot::ReThrow
                | CILRoot::Unreachable(_) => {
                    canonical.push(root);
                    break;
                }
                _ => canonical.push(root),
            }
        }
        self.roots = canonical;

        if let Some(handler) = &mut self.handler {
            for block in handler {
                block.canonicalize_control_flow(asm);
            }
        }
    }

    /// Returns the list of blocks this block can potentially jump to.
    pub fn targets<'block, 'asm: 'block>(
        &'block self,
        asm: &'asm Assembly,
    ) -> impl Iterator<Item = BlockId> + 'block {
        self.roots().iter().filter_map(|root| {
            match asm.get_root(*root) {
                CILRoot::Branch(info) => {
                    let (target, sub_target, _) = info.as_ref();
                    //Some(*sub_target)
                    //(eprintln!("{target} {sub_target}");
                    if *sub_target == 0 {
                        Some(*target)
                    } else {
                        Some(*sub_target)
                    }
                }
                CILRoot::ExitSpecialRegion { target, .. } => Some(*target),
                _ => None,
            }
        })
    }
    /// Creates a new block with a given unique id, roots, and an optional list of handler blocks.
    /// The handler ought not have a handler of its own, and the roots should end with a diverging root.
    /// The handler will start executing at the first block in the handler, regardless of its id.
    /// ```
    /// # use cilly::BasicBlock;
    /// # use cilly::{CILRoot, Interned};
    /// # let roots = vec![];
    /// # let handler_roots = vec![];
    /// // Create a block
    /// let bb = BasicBlock::new(roots, 0, None);
    /// // With a handler
    /// # let roots = vec![];
    /// let handler = BasicBlock::new(roots, 0, None);
    /// let bb = BasicBlock::new(handler_roots, 0, Some(vec![handler]));
    /// ```
    /// ```should_panic
    /// # use cilly::BasicBlock;
    /// # use cilly::{CILRoot, Interned};
    /// # let roots = vec![];
    /// # let handler_roots = vec![];
    /// # let handlerer_roots = vec![];
    /// // 2 layers of handlers - not supported.
    /// let handlerer = BasicBlock::new(handlerer_roots, 1, None);
    /// let handler = BasicBlock::new(handler_roots, 1, Some(vec![handlerer]));
    /// let bb = BasicBlock::new(roots, 0, Some(vec![handler]));
    /// ```
    #[must_use]
    pub fn new(
        roots: Vec<Interned<CILRoot>>,
        block_id: BlockId,
        handler: Option<Vec<Self>>,
    ) -> Self {
        debug_assert!(
            handler
                .as_ref()
                .is_none_or(|handler| handler.iter().all(|h| h.handler.is_none()))
        );
        Self {
            roots,
            block_id,
            handler,
            handler_id: None,
        }
    }
    /// Creates a new block with an *unresolved* exception handler id `handler_id`.
    /// The handler is resolved later by [`Self::resolve_exception_handlers`].
    #[must_use]
    pub fn new_raw(
        roots: Vec<Interned<CILRoot>>,
        block_id: BlockId,
        handler_id: Option<BlockId>,
    ) -> Self {
        Self {
            roots,
            block_id,
            handler: None,
            handler_id,
        }
    }
    /// Returns the *unresolved* handler id of this block, if any.
    #[must_use]
    pub fn handler_id(&self) -> Option<BlockId> {
        self.handler_id
    }

    #[must_use]
    /// Retrives the list of all roots in this block.
    pub fn roots(&self) -> &[Interned<CILRoot>] {
        &self.roots
    }

    #[must_use]
    /// Retrives the id of this block.
    /// ```
    /// # use cilly::BasicBlock;
    /// # use cilly::{CILRoot, Interned};
    /// # let roots = vec![];
    /// let bb = BasicBlock::new(roots, 0, None);
    /// assert_eq!(bb.block_id(), 0);
    /// # let roots = vec![];
    /// let bb = BasicBlock::new(roots, 12345, None);
    /// assert_eq!(bb.block_id(), 12345);
    /// ```
    pub fn block_id(&self) -> BlockId {
        self.block_id
    }
    /// Goes trough all the roots in this block **and its handler**.
    pub fn iter_roots(&self) -> impl Iterator<Item = Interned<CILRoot>> + '_ {
        let handler_iter: Box<dyn Iterator<Item = Interned<CILRoot>>> = match self.handler() {
            Some(handler) => Box::new(handler.iter().flat_map(BasicBlock::iter_roots)),
            None => Box::new(std::iter::empty()),
        };
        self.roots().iter().copied().chain(handler_iter)
    }
    /// Iterates trough all the roots of this block and its handlers - mutablu.
    pub fn iter_roots_mut(&mut self) -> impl Iterator<Item = &mut Interned<CILRoot>> + '_ {
        let handler_iter: Box<dyn Iterator<Item = &mut Interned<CILRoot>>> =
            match self.handler.as_mut() {
                Some(handler) => Box::new(handler.iter_mut().flat_map(BasicBlock::iter_roots_mut)),
                None => Box::new(std::iter::empty()),
            };
        self.roots.iter_mut().chain(handler_iter)
    }
    /// Modifies all nodes and roots in this `BasicBlock`
    pub fn map_roots(
        &mut self,
        asm: &mut Assembly,
        root_map: &mut impl Fn(CILRoot, &mut Assembly) -> CILRoot,
        node_map: &mut impl Fn(CILNode, &mut Assembly) -> CILNode,
    ) {
        self.iter_roots_mut().for_each(|root| {
            let get_root = asm.get_root(*root).clone();
            let val = get_root.map(asm, root_map, node_map);
            *root = asm.alloc_root(val);
        });
    }
    #[must_use]
    /// Returns an immutable reference to this blocks handler.
    /// ```
    /// # use cilly::BasicBlock;
    /// let block = BasicBlock::new(vec![],0,Some(vec![BasicBlock::new(vec![],1,None)]));
    /// assert_eq!(block.handler().unwrap().len(),1);
    /// ```
    pub fn handler(&self) -> Option<&[BasicBlock]> {
        self.handler.as_ref().map(std::convert::AsRef::as_ref)
    }
    /// Returns a mutable reference to this blocks handler.
    /// ```
    /// # use cilly::BasicBlock;
    /// let mut block = BasicBlock::new(vec![],0,Some(vec![BasicBlock::new(vec![],1,None)]));
    /// assert_eq!(block.handler_mut().unwrap().len(),1);
    /// // Add another block to this handler
    /// block.handler_mut().unwrap().push(BasicBlock::new(vec![],2,None));
    /// assert_eq!(block.handler_mut().unwrap().len(),2);
    /// ```
    pub fn handler_mut(&mut self) -> Option<&mut Vec<BasicBlock>> {
        self.handler.as_mut()
    }
    /// Returns a mutable reference to the roots of this block - **excluding the handler**.
    pub fn roots_mut(&mut self) -> &mut Vec<Interned<CILRoot>> {
        &mut self.roots
    }
    /// Returns a mutable reference to the roots of this block and its handler - *separately*.
    pub fn handler_and_root_mut(
        &mut self,
    ) -> (Option<&mut [BasicBlock]>, &mut Vec<Interned<CILRoot>>) {
        (
            self.handler.as_mut().map(std::convert::AsMut::as_mut),
            &mut self.roots,
        )
    }
    /// Checks if this basic block consists of nothing more than an unconditional jump to another block.
    /// ```
    /// # use cilly::*;
    /// # use cilly::BasicBlock;
    /// # let mut asm = Assembly::default();
    /// # let mut void_ret = asm.alloc_root(CILRoot::VoidRet);
    /// # let mut rethrow = asm.alloc_root(CILRoot::ReThrow);
    /// # let mut val = asm.alloc_node(0);
    /// # let mut do_sth = asm.alloc_root(CILRoot::StLoc(0,val));
    /// let target = 11;
    /// let mut jump = asm.alloc_root(CILRoot::Branch(Box::new((target,0,None))));
    /// assert_eq!(BasicBlock::new(vec![],0,None).is_direct_jump(&asm),None);
    /// assert_eq!(BasicBlock::new(vec![void_ret],0,None).is_direct_jump(&asm),None);
    /// assert_eq!(BasicBlock::new(vec![jump],0,None).is_direct_jump(&asm),Some((target,0)));
    /// assert_eq!(BasicBlock::new(vec![do_sth,jump],0,None).is_direct_jump(&asm),None);
    /// ```
    #[must_use]
    pub fn is_direct_jump(&self, asm: &Assembly) -> Option<(BlockId, BlockId)> {
        let mut roots = self.meaningfull_roots(asm);
        let root = roots.next()?;
        let CILRoot::Branch(binfo) = asm.get_root(root) else {
            return None;
        };
        if opt::is_branch_unconditional(binfo) && roots.next().is_none() {
            Some((binfo.0, binfo.1))
        } else {
            None
        }
    }
    /// Checks if this basic block consists of nothing more thaan an uncondtional rethrow
    /// ```
    /// # use cilly::*;
    /// # use cilly::BasicBlock;
    /// # let mut asm = Assembly::default();
    /// # let mut void_ret = asm.alloc_root(CILRoot::VoidRet);
    /// # let mut rethrow = asm.alloc_root(CILRoot::ReThrow);
    /// # let mut val = asm.alloc_node(0);
    /// # let mut do_sth = asm.alloc_root(CILRoot::StLoc(0,val));
    /// assert!(!BasicBlock::new(vec![],0,None).is_only_rethrow(&asm));
    /// assert!(!BasicBlock::new(vec![void_ret],0,None).is_only_rethrow(&asm));
    /// assert!(BasicBlock::new(vec![rethrow],0,None).is_only_rethrow(&asm));
    /// assert!(!BasicBlock::new(vec![do_sth,rethrow],0,None).is_only_rethrow(&asm));
    /// ```
    #[must_use]
    pub fn is_only_rethrow(&self, asm: &Assembly) -> bool {
        let mut roots = self.meaningfull_roots(asm);
        let Some(root) = roots.next() else {
            return false;
        };
        CILRoot::ReThrow == *asm.get_root(root) && roots.next().is_none()
    }
    /// Returns a list of all roots, excluding NOPs and SFI.
    pub fn meaningfull_roots<'s, 'asm: 's>(
        &'s self,
        asm: &'asm Assembly,
    ) -> impl Iterator<Item = Interned<CILRoot>> + 's {
        self.iter_roots().filter(move |root| {
            !matches!(
                asm.get_root(*root),
                CILRoot::Nop | CILRoot::SourceFileInfo { .. }
            )
        })
    }
    /// Removes this blocks handler.
    /// ```
    /// # use cilly::BasicBlock;
    /// # let mut asm = cilly::Assembly::default();
    /// let mut block = BasicBlock::new(vec![],0,Some(vec![BasicBlock::new(vec![],1,None)]));
    /// assert!(block.handler().is_some());
    /// // Add another block to this handler
    /// block.remove_handler(&mut asm);
    /// assert!(block.handler().is_none());
    /// ```
    pub fn remove_handler(&mut self, asm: &mut Assembly) {
        self.handler = None;
        self.roots_mut().iter_mut().for_each(|root| {
            if let CILRoot::ExitSpecialRegion { target, source: _ } = asm[*root] {
                *root = asm.alloc_root(CILRoot::Branch(Box::new((target, 0, None))));
            }
        });
    }
    /// Returns the `(target, sub_target)` pairs this block (excluding its handler) branches to,
    /// reading `CILRoot::Branch` roots.
    #[must_use]
    pub fn targets_with_sub(&self, asm: &Assembly) -> Vec<(BlockId, BlockId)> {
        self.roots
            .iter()
            .filter_map(|root| match asm.get_root(*root) {
                CILRoot::Branch(info) => Some((info.0, info.1)),
                _ => None,
            })
            .collect()
    }
    /// Returns the target of a trailing unconditional jump, if this block ends in one (and the
    /// sub_target is 0).
    #[must_use]
    pub fn final_uncond_jump(&self, asm: &Assembly) -> Option<BlockId> {
        match self.roots.last().map(|root| asm.get_root(*root)) {
            Some(CILRoot::Branch(info)) if info.2.is_none() && info.1 == 0 => Some(info.0),
            _ => None,
        }
    }
    /// Rewrites every branch root in this block so it targets the handler "jumpstarter" block `id`
    /// instead of its original target (the original target becomes the `sub_target`).
    /// Asserts each branch's `sub_target` is 0.
    fn fix_for_exception_handler(&mut self, id: BlockId, asm: &mut Assembly) {
        for root in &mut self.roots {
            if let CILRoot::Branch(info) = asm.get_root(*root) {
                let (target, sub_target, cond) = (info.0, info.1, info.2.clone());
                assert_eq!(
                    sub_target, 0,
                    "An exception handler can't contain inner exception handler!"
                );
                *root = asm.alloc_root(CILRoot::Branch(Box::new((id, target, cond))));
            }
        }
    }
    /// Roots are already flat (no nested trees), so shedding is a no-op. Kept for call-site parity
    /// with `add_fn`.
    pub fn sheed_trees(&mut self) {}
    /// Resolves this block's *unresolved* exception handler (set via [`Self::new_raw`]) against the
    /// full set of cleanup blocks `handler_bbs`: it garbage-collects the reachable handler blocks,
    /// fixes their branches to point back through a "jumpstarter", inserts the jumpstarter, emits
    /// `ExitSpecialRegion` launching pads for cross-block branches, and rewrites this block's
    /// branches to use them. Must run before any optimization/serialization.
    pub fn resolve_exception_handlers(&mut self, handler_bbs: &[Self], asm: &mut Assembly) {
        let Some(handler_id) = self.handler_id else {
            return;
        };
        self.resolve_exception_handler(handler_id, handler_bbs, asm);
    }

    /// Materializes one explicit method-level exception-region association into the legacy
    /// per-block handler shape consumed by the current exporters.
    ///
    /// Unlike [`Self::resolve_exception_handlers`], this does not read `self.handler_id`; canonical
    /// region bodies keep that association at method scope and call this only on an exporter-local
    /// scratch clone.
    pub fn resolve_exception_handler(
        &mut self,
        handler_id: BlockId,
        handler_bbs: &[Self],
        asm: &mut Assembly,
    ) {
        assert!(
            self.handler.is_none(),
            "canonical exception-region block already has a materialized handler"
        );
        // Get alive handler blocks.
        let mut handler = block_gc(handler_id, handler_bbs, asm);
        // Fix up handler jumps.
        let id = self.block_id;
        for bb in &mut handler {
            bb.fix_for_exception_handler(id, asm);
        }
        // Insert the "jumpstarter": an unconditional branch into the handler region.
        handler.insert(
            0,
            Self::new(
                vec![asm.alloc_root(CILRoot::Branch(Box::new((id, handler_id, None))))],
                BlockId::MAX,
                None,
            ),
        );
        // Generate launching pads (ExitSpecialRegion) for cross-block branches.
        let targets = self.targets_with_sub(asm);
        let targets: FxHashSet<_> = targets.iter().collect();
        for (target, sub_target) in targets {
            assert_eq!(*sub_target, 0);
            let pad = asm.alloc_root(CILRoot::ExitSpecialRegion {
                target: *target,
                source: id,
            });
            self.roots.push(pad);
        }
        // Change branches to use launching pads.
        self.fix_for_exception_handler(id, asm);

        // Every CLR catch handler must transfer control explicitly; falling through the end of a
        // handler is invalid IL and CoreCLR rejects the whole method at JIT time. These handlers
        // model Rust cleanup-only unwind paths, so a terminal cleanup block that has performed its
        // side effects but has no explicit transfer must resume the current exception.
        let terminal = handler
            .last_mut()
            .expect("a reachable exception-handler entry must materialize at least one block");
        let ends_with_transfer =
            terminal
                .meaningfull_roots(asm)
                .last()
                .is_some_and(|root| match asm.get_root(root) {
                    CILRoot::ReThrow
                    | CILRoot::Throw(_)
                    | CILRoot::Ret(_)
                    | CILRoot::VoidRet
                    | CILRoot::Unreachable(_)
                    | CILRoot::ExitSpecialRegion { .. } => true,
                    CILRoot::Branch(info) => opt::is_branch_unconditional(info),
                    _ => false,
                });
        if !ends_with_transfer {
            terminal.roots.push(asm.alloc_root(CILRoot::ReThrow));
        }

        self.handler = Some(handler);
        self.handler_id = None;
    }
}

fn constant_branch_value(condition: &BranchCond, asm: &Assembly) -> Option<bool> {
    let constant = |node| match asm.get_node(node) {
        CILNode::Const(value) => Some(value.as_ref()),
        _ => None,
    };
    match condition {
        BranchCond::True(value) => constant(*value).and_then(constant_truthiness),
        BranchCond::False(value) => constant(*value).and_then(constant_truthiness).map(|v| !v),
        BranchCond::Eq(lhs, rhs) => constant_equality(constant(*lhs)?, constant(*rhs)?),
        BranchCond::Ne(lhs, rhs) => {
            constant_equality(constant(*lhs)?, constant(*rhs)?).map(|value| !value)
        }
        BranchCond::Lt(lhs, rhs, kind) => {
            constant_order(constant(*lhs)?, constant(*rhs)?, kind, Ordering::is_lt)
        }
        BranchCond::Gt(lhs, rhs, kind) => {
            constant_order(constant(*lhs)?, constant(*rhs)?, kind, Ordering::is_gt)
        }
        BranchCond::Le(lhs, rhs, kind) => {
            constant_order(constant(*lhs)?, constant(*rhs)?, kind, |order| {
                order.is_lt() || order.is_eq()
            })
        }
        BranchCond::Ge(lhs, rhs, kind) => {
            constant_order(constant(*lhs)?, constant(*rhs)?, kind, |order| {
                order.is_gt() || order.is_eq()
            })
        }
    }
}

fn constant_truthiness(value: &Const) -> Option<bool> {
    match value {
        Const::Bool(value) => Some(*value),
        Const::I8(value) => Some(*value != 0),
        Const::I16(value) => Some(*value != 0),
        Const::I32(value) => Some(*value != 0),
        Const::I64(value) | Const::ISize(value) => Some(*value != 0),
        Const::U8(value) => Some(*value != 0),
        Const::U16(value) => Some(*value != 0),
        Const::U32(value) => Some(*value != 0),
        Const::U64(value) | Const::USize(value) => Some(*value != 0),
        Const::I128(_)
        | Const::U128(_)
        | Const::PlatformString(_)
        | Const::F32(_)
        | Const::F64(_)
        | Const::Null(_)
        | Const::ByteBuffer { .. } => None,
    }
}

fn constant_equality(lhs: &Const, rhs: &Const) -> Option<bool> {
    if lhs.get_type() != rhs.get_type() {
        return None;
    }
    match (lhs, rhs) {
        (Const::Bool(lhs), Const::Bool(rhs)) => Some(lhs == rhs),
        (Const::I8(lhs), Const::I8(rhs)) => Some(lhs == rhs),
        (Const::I16(lhs), Const::I16(rhs)) => Some(lhs == rhs),
        (Const::I32(lhs), Const::I32(rhs)) => Some(lhs == rhs),
        (Const::I64(lhs), Const::I64(rhs)) | (Const::ISize(lhs), Const::ISize(rhs)) => {
            Some(lhs == rhs)
        }
        (Const::U8(lhs), Const::U8(rhs)) => Some(lhs == rhs),
        (Const::U16(lhs), Const::U16(rhs)) => Some(lhs == rhs),
        (Const::U32(lhs), Const::U32(rhs)) => Some(lhs == rhs),
        (Const::U64(lhs), Const::U64(rhs)) | (Const::USize(lhs), Const::USize(rhs)) => {
            Some(lhs == rhs)
        }
        (Const::F32(lhs), Const::F32(rhs)) => Some(lhs.0 == rhs.0),
        (Const::F64(lhs), Const::F64(rhs)) => Some(lhs.0 == rhs.0),
        _ => None,
    }
}

fn constant_order(
    lhs: &Const,
    rhs: &Const,
    kind: &CmpKind,
    accepts: impl FnOnce(std::cmp::Ordering) -> bool,
) -> Option<bool> {
    if lhs.get_type() != rhs.get_type() {
        return None;
    }
    let order = match (lhs, rhs) {
        (Const::F32(lhs), Const::F32(rhs)) => {
            return float_order(f64::from(lhs.0), f64::from(rhs.0), kind, accepts);
        }
        (Const::F64(lhs), Const::F64(rhs)) => {
            return float_order(lhs.0, rhs.0, kind, accepts);
        }
        _ => {
            let (lhs, lhs_width) = integer_bits(lhs)?;
            let (rhs, rhs_width) = integer_bits(rhs)?;
            if lhs_width != rhs_width {
                return None;
            }
            if matches!(kind, CmpKind::Signed | CmpKind::Ordered) {
                signed_bits(lhs, lhs_width).cmp(&signed_bits(rhs, rhs_width))
            } else {
                lhs.cmp(&rhs)
            }
        }
    };
    Some(accepts(order))
}

fn float_order(
    lhs: f64,
    rhs: f64,
    kind: &CmpKind,
    accepts: impl FnOnce(std::cmp::Ordering) -> bool,
) -> Option<bool> {
    match lhs.partial_cmp(&rhs) {
        Some(order) => Some(accepts(order)),
        None => Some(matches!(kind, CmpKind::Unordered | CmpKind::Unsigned)),
    }
}

fn integer_bits(value: &Const) -> Option<(u128, u32)> {
    match value {
        Const::I8(value) => Some((u128::from(*value as u8), 8)),
        Const::I16(value) => Some((u128::from(*value as u16), 16)),
        Const::I32(value) => Some((u128::from(*value as u32), 32)),
        Const::I64(value) | Const::ISize(value) => Some((u128::from(*value as u64), 64)),
        Const::U8(value) => Some((u128::from(*value), 8)),
        Const::U16(value) => Some((u128::from(*value), 16)),
        Const::U32(value) => Some((u128::from(*value), 32)),
        Const::U64(value) | Const::USize(value) => Some((u128::from(*value), 64)),
        _ => None,
    }
}

fn signed_bits(value: u128, width: u32) -> i128 {
    let shift = 128 - width;
    ((value << shift) as i128) >> shift
}
fn find_bb(id: BlockId, bbs: &[BasicBlock]) -> &BasicBlock {
    bbs.iter().find(|bb| bb.block_id() == id).unwrap()
}
/// Garbage-collects the handler blocks reachable from `entrypoint`.
fn block_gc(entrypoint: BlockId, bbs: &[BasicBlock], asm: &Assembly) -> Vec<BasicBlock> {
    let mut alive: FxHashSet<BlockId> = FxHashSet::with_hasher(FxBuildHasher::default());
    let mut resurecting = FxHashSet::with_hasher(FxBuildHasher::default());
    let mut to_resurect = FxHashSet::with_hasher(FxBuildHasher::default());
    to_resurect.insert(entrypoint);
    while !to_resurect.is_empty() {
        alive.extend(&resurecting);
        resurecting.clear();
        resurecting.extend(&to_resurect);
        to_resurect.clear();
        for (target, sub_target) in resurecting
            .iter()
            .flat_map(|bb| find_bb(*bb, bbs).targets_with_sub(asm))
        {
            assert_eq!(
                sub_target, 0,
                "No block can have subblocks before the exception handler resolving phase!"
            );
            if !alive.contains(&target) && !resurecting.contains(&target) {
                to_resurect.insert(target);
            }
        }
    }
    alive.extend(&resurecting);
    bbs.iter()
        .filter(|bb| alive.contains(&bb.block_id))
        .cloned()
        .collect()
}
#[test]
fn is_direct_jump() {
    let asm = &mut Assembly::default();
    let block = BasicBlock::new(vec![], 0, None);
    // A Block which is empty is not a direwct jump anywhere.'
    assert!(block.is_direct_jump(asm).is_none());
}
#[test]
fn is_only_rethrow() {
    let asm = &mut Assembly::default();
    let block = BasicBlock::new(vec![], 0, None);
    // A Block which is empty is not a rethrow.
    assert!(!block.is_only_rethrow(asm));
    let rethrow = asm.alloc_root(CILRoot::ReThrow);
    let block = BasicBlock::new(vec![rethrow], 0, None);
    // A Block which is just a rethrow is, well, a rethrow.
    assert!(block.is_only_rethrow(asm));
    let dbg_break = asm.alloc_root(CILRoot::Break);
    let block = BasicBlock::new(vec![dbg_break, rethrow], 0, None);
    // A dbg break has side effects, this should return false
    assert!(!block.is_only_rethrow(asm));
    let dbg_break = asm.alloc_root(CILRoot::Break);
    let block = BasicBlock::new(vec![rethrow, dbg_break], 0, None);
    // A dbf break has side effects, this should return false
    assert!(!block.is_only_rethrow(asm));
}

#[test]
fn materialized_handler_rethrows_after_side_effecting_terminal_cleanup() {
    let asm = &mut Assembly::default();
    let side_effect = asm.alloc_root(CILRoot::Break);
    let cleanup = [BasicBlock::new(vec![side_effect], 10, None)];
    let mut protected = BasicBlock::new_raw(vec![], 0, Some(10));

    protected.resolve_exception_handlers(&cleanup, asm);

    let terminal = protected.handler().unwrap().last().unwrap();
    let meaningful: Vec<_> = terminal.meaningfull_roots(asm).collect();
    assert_eq!(meaningful.len(), 2);
    assert_eq!(*asm.get_root(meaningful[0]), CILRoot::Break);
    assert_eq!(*asm.get_root(meaningful[1]), CILRoot::ReThrow);
}

#[test]
fn mandatory_cfg_cleanup_preserves_legacy_exception_leave_pads() {
    let mut asm = Assembly::default();
    let branch = asm.alloc_root(CILRoot::Branch(Box::new((0, 7, None))));
    let first_pad = asm.alloc_root(CILRoot::ExitSpecialRegion {
        target: 7,
        source: 0,
    });
    let second_pad = asm.alloc_root(CILRoot::ExitSpecialRegion {
        target: 8,
        source: 0,
    });
    let expected = vec![branch, first_pad, second_pad];
    let mut block = BasicBlock::new(expected.clone(), 0, None);

    block.canonicalize_control_flow(&mut asm);

    assert_eq!(block.roots(), expected);
}

#[test]
fn mandatory_cfg_cleanup_evaluates_boolean_and_scalar_constant_branches() {
    let mut asm = Assembly::default();
    let false_node = asm.alloc_node(Const::Bool(false));
    let minus_one = asm.alloc_node(Const::I32(-1));
    let one = asm.alloc_node(Const::I32(1));
    let nan = asm.alloc_node(Const::F64(super::hashable::HashableF64(f64::NAN)));
    let zero = asm.alloc_node(Const::F64(super::hashable::HashableF64(0.0)));

    assert_eq!(
        constant_branch_value(&BranchCond::False(false_node), &asm),
        Some(true)
    );
    assert_eq!(
        constant_branch_value(&BranchCond::Lt(minus_one, one, CmpKind::Signed), &asm),
        Some(true)
    );
    assert_eq!(
        constant_branch_value(&BranchCond::Lt(nan, zero, CmpKind::Ordered), &asm),
        Some(false)
    );
    assert_eq!(
        constant_branch_value(&BranchCond::Lt(nan, zero, CmpKind::Unordered), &asm),
        Some(true)
    );
}
