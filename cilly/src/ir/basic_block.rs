use fxhash::{FxBuildHasher, FxHashSet};
use serde::{Deserialize, Serialize};

use super::{
    asm_link::{RelocateCtx, RelocateValue},
    bimap::Interned,
    opt, Assembly, CILNode, CILRoot,
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
        debug_assert!(handler
            .as_ref()
            .is_none_or(|handler| handler.iter().all(|h| h.handler.is_none())));
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

        self.handler = Some(handler);
        self.handler_id = None;
    }
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
