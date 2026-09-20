//! Elision of mechanically proven finite counted loops whose bodies are total no-ops.
//!
//! rustc expands `drop_in_place::<[T]>` into a counted loop before this backend sees MIR. After
//! lowering proves an element drop is empty (notably `T = ()`), the remaining CIL can still count
//! from zero to `usize::MAX` while doing nothing. This pass recognizes only the canonical
//! two-branch loop emitted for that expansion and preserves the induction variable's final value.

use fxhash::FxHashSet;

use super::{EffectInfoCache, OptFuel};
use crate::{
    Assembly, BasicBlock, BinOp, BranchCond, CILIter, CILIterElem, CILNode, CILRoot, Const, FnSig,
    Interned, MethodImpl, Type, cilroot::CmpKind, method::LocalDef,
};

#[derive(Clone, Copy)]
struct Candidate {
    header_index: usize,
    exit: u32,
    index_local: u32,
    bound: Interned<CILNode>,
    preserve_final_index: bool,
}

struct CountedStepPath {
    block_ids: Vec<u32>,
    guard_locals: Vec<u32>,
}

/// Replaces each proven loop with `index = bound; goto exit`.
///
/// The unreachable body and any exception region protecting it are deliberately left in place.
/// `MethodDef::remove_dead_blocks`, which runs after every optimizer iteration, owns synchronized
/// normal-CFG, exception-region, and cleanup-CFG pruning.
pub(super) fn eliminate_total_counted_loops(
    implementation: &mut MethodImpl,
    sig: Interned<FnSig>,
    asm: &mut Assembly,
    cache: &mut EffectInfoCache,
    fuel: &mut OptFuel,
) {
    let Some((blocks, cleanup_blocks, locals)) = implementation.body_parts_mut() else {
        return;
    };

    let cleanup_blocks = cleanup_blocks.map(|blocks| blocks.as_slice());
    let mut header_index = 0;
    while header_index < blocks.len() {
        let candidate = find_candidate(
            header_index,
            blocks,
            cleanup_blocks,
            locals,
            sig,
            asm,
            cache,
        );
        let Some(candidate) = candidate else {
            header_index += 1;
            continue;
        };
        if !fuel.consume(2) {
            return;
        }

        let mut roots: Vec<_> = blocks[candidate.header_index]
            .roots()
            .iter()
            .copied()
            .filter(|root| matches!(asm.get_root(*root), CILRoot::SourceFileInfo { .. }))
            .collect();
        if candidate.preserve_final_index {
            roots.push(asm.alloc_root(CILRoot::StLoc(candidate.index_local, candidate.bound)));
        }
        roots.push(asm.alloc_root(CILRoot::Branch(Box::new((candidate.exit, 0, None)))));
        *blocks[candidate.header_index].roots_mut() = roots;
        header_index += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn find_candidate(
    header_index: usize,
    blocks: &[BasicBlock],
    cleanup_blocks: Option<&[BasicBlock]>,
    locals: &[LocalDef],
    sig: Interned<FnSig>,
    asm: &mut Assembly,
    cache: &mut EffectInfoCache,
) -> Option<Candidate> {
    let header = blocks.get(header_index)?;
    if header.handler().is_some() {
        return None;
    }
    let meaningful: Vec<_> = header.meaningfull_roots(asm).collect();
    let [conditional, fallback] = meaningful.as_slice() else {
        return None;
    };
    let CILRoot::Branch(conditional) = asm.get_root(*conditional).clone() else {
        return None;
    };
    let (conditional_target, conditional_subtarget, Some(condition)) = conditional.as_ref() else {
        return None;
    };
    let CILRoot::Branch(fallback) = asm.get_root(*fallback).clone() else {
        return None;
    };
    let (fallback_target, fallback_subtarget, None) = fallback.as_ref() else {
        return None;
    };
    if *conditional_subtarget != 0 || *fallback_subtarget != 0 {
        return None;
    }
    let (body_id, exit, index, bound, preserve_final_index, unsigned_exit_test) = match condition {
        BranchCond::Ne(index, bound) => (
            *conditional_target,
            *fallback_target,
            *index,
            *bound,
            true,
            false,
        ),
        BranchCond::Ge(index, bound, CmpKind::Unsigned | CmpKind::Unordered) => (
            *fallback_target,
            *conditional_target,
            *index,
            *bound,
            false,
            true,
        ),
        _ => return None,
    };
    if exit == body_id || exit == header.block_id() {
        return None;
    }

    let CILNode::LdLoc(index_local) = asm.get_node(index) else {
        return None;
    };
    let index_local = *index_local;
    let index_type = asm[locals.get(index_local as usize)?.1];
    let Type::Int(index_int) = index_type else {
        return None;
    };
    if (unsigned_exit_test && index_int.is_signed())
        || bound_type(bound, sig, locals, asm)? != index_type
    {
        return None;
    }

    let step_path = counted_step_path(
        body_id,
        header.block_id(),
        index_local,
        index,
        blocks,
        cleanup_blocks,
        locals,
        index_type,
        sig,
        asm,
        cache,
    )?;
    let entry_predecessors = normal_predecessors(body_id, blocks, asm)?;
    if entry_predecessors.as_slice() != [header.block_id()] {
        return None;
    }
    for pair in step_path.block_ids.windows(2) {
        if normal_predecessors(pair[1], blocks, asm)?.as_slice() != [pair[0]] {
            return None;
        }
    }
    let backedge_id = *step_path.block_ids.last()?;
    let header_predecessors = normal_predecessors(header.block_id(), blocks, asm)?;
    if !header_predecessors.contains(&backedge_id)
        || !header_predecessors
            .iter()
            .any(|predecessor| *predecessor != backedge_id)
    {
        return None;
    }

    let loop_blocks: FxHashSet<_> = std::iter::once(header.block_id())
        .chain(step_path.block_ids.iter().copied())
        .collect();

    if local_address_taken(index_local, blocks, cleanup_blocks, asm) {
        return None;
    }
    if let CILNode::LdLoc(bound_local) = asm.get_node(bound) {
        if local_address_taken(*bound_local, blocks, cleanup_blocks, asm) {
            return None;
        }
    } else if let CILNode::LdArg(bound_arg) = asm.get_node(bound)
        && argument_address_taken(*bound_arg, blocks, cleanup_blocks, asm)
    {
        return None;
    }
    if step_path.guard_locals.iter().any(|guard_local| {
        local_address_taken(*guard_local, blocks, cleanup_blocks, asm)
            || local_read_outside_blocks(*guard_local, &loop_blocks, blocks, cleanup_blocks, asm)
    }) {
        return None;
    }
    if !preserve_final_index
        && local_read_outside_blocks(index_local, &loop_blocks, blocks, cleanup_blocks, asm)
    {
        return None;
    }

    Some(Candidate {
        header_index,
        exit,
        index_local,
        bound,
        preserve_final_index,
    })
}

fn local_read_outside_blocks(
    local: u32,
    excluded_blocks: &FxHashSet<u32>,
    blocks: &[BasicBlock],
    cleanup_blocks: Option<&[BasicBlock]>,
    asm: &Assembly,
) -> bool {
    blocks
        .iter()
        .filter(|block| !excluded_blocks.contains(&block.block_id()))
        .chain(cleanup_blocks.into_iter().flatten())
        .flat_map(BasicBlock::iter_roots)
        .any(|root| {
            CILIter::new(asm.get_root(root).clone(), asm).any(|element| {
                matches!(
                    element,
                    CILIterElem::Node(CILNode::LdLoc(found) | CILNode::LdLocA(found))
                        if found == local
                )
            })
        })
}

fn bound_type(
    bound: Interned<CILNode>,
    sig: Interned<FnSig>,
    locals: &[LocalDef],
    asm: &Assembly,
) -> Option<Type> {
    match asm.get_node(bound) {
        CILNode::LdLoc(local) => Some(asm[locals.get(*local as usize)?.1]),
        CILNode::LdArg(argument) => asm[sig].inputs().get(*argument as usize).copied(),
        CILNode::Const(value) => Some(value.get_type()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn counted_step_path(
    body_id: u32,
    header_id: u32,
    index_local: u32,
    index: Interned<CILNode>,
    blocks: &[BasicBlock],
    cleanup_blocks: Option<&[BasicBlock]>,
    locals: &[LocalDef],
    index_type: Type,
    sig: Interned<FnSig>,
    asm: &mut Assembly,
    cache: &mut EffectInfoCache,
) -> Option<CountedStepPath> {
    let body = blocks.iter().find(|block| block.block_id() == body_id)?;
    if body.handler().is_some() {
        return None;
    }
    if body_is_total_counted_step(
        body,
        header_id,
        index_local,
        index,
        blocks,
        cleanup_blocks,
        locals,
        index_type,
        sig,
        asm,
        cache,
    ) {
        return Some(CountedStepPath {
            block_ids: vec![body_id],
            guard_locals: vec![],
        });
    }

    // A constant-return comparison commonly lowers across CGUs as three blocks even after the
    // call itself folds: `guard = false; goto test`, `if !guard { goto step } else { goto fail }`,
    // then the ordinary induction step. Prove that exact dominating-store shape here so loop
    // elimination does not depend on cross-block copy propagation or a large peephole budget.
    let body_roots: Vec<_> = body.meaningfull_roots(asm).collect();
    let [store, to_guard] = body_roots.as_slice() else {
        return None;
    };
    let CILRoot::StLoc(guard_local, guard_value) = asm.get_root(*store).clone() else {
        return None;
    };
    if guard_local == index_local || asm[locals.get(guard_local as usize)?.1] != Type::Bool {
        return None;
    }
    let CILNode::Const(value) = asm.get_node(guard_value) else {
        return None;
    };
    let Const::Bool(guard_value) = value.as_ref() else {
        return None;
    };
    let guard_value = *guard_value;
    let CILRoot::Branch(to_guard) = asm.get_root(*to_guard).clone() else {
        return None;
    };
    let (guard_id, 0, None) = *to_guard else {
        return None;
    };
    let guard = blocks.iter().find(|block| block.block_id() == guard_id)?;
    if guard.handler().is_some() {
        return None;
    }
    let guard_roots: Vec<_> = guard.meaningfull_roots(asm).collect();
    let [conditional, fallback] = guard_roots.as_slice() else {
        return None;
    };
    let CILRoot::Branch(conditional) = asm.get_root(*conditional).clone() else {
        return None;
    };
    let (conditional_target, 0, Some(condition)) = *conditional else {
        return None;
    };
    let CILRoot::Branch(fallback) = asm.get_root(*fallback).clone() else {
        return None;
    };
    let (fallback_target, 0, None) = *fallback else {
        return None;
    };
    let branch_reads_guard = |node: Interned<CILNode>| matches!(asm.get_node(node), CILNode::LdLoc(local) if *local == guard_local);
    let step_id = match &condition {
        BranchCond::True(node) if branch_reads_guard(*node) => {
            if guard_value {
                conditional_target
            } else {
                fallback_target
            }
        }
        BranchCond::False(node) if branch_reads_guard(*node) => {
            if !guard_value {
                conditional_target
            } else {
                fallback_target
            }
        }
        _ => return None,
    };
    let step = blocks.iter().find(|block| block.block_id() == step_id)?;
    if step.handler().is_some()
        || !body_is_total_counted_step(
            step,
            header_id,
            index_local,
            index,
            blocks,
            cleanup_blocks,
            locals,
            index_type,
            sig,
            asm,
            cache,
        )
    {
        return None;
    }
    let block_ids = vec![body_id, guard_id, step_id];
    if block_ids.iter().copied().collect::<FxHashSet<_>>().len() != block_ids.len()
        || block_ids.contains(&header_id)
    {
        return None;
    }
    Some(CountedStepPath {
        block_ids,
        guard_locals: vec![guard_local],
    })
}

#[allow(clippy::too_many_arguments)]
fn body_is_total_counted_step(
    body: &BasicBlock,
    header_id: u32,
    index_local: u32,
    index: Interned<CILNode>,
    blocks: &[BasicBlock],
    cleanup_blocks: Option<&[BasicBlock]>,
    locals: &[LocalDef],
    index_type: Type,
    sig: Interned<FnSig>,
    asm: &mut Assembly,
    cache: &mut EffectInfoCache,
) -> bool {
    let meaningful: Vec<_> = body.meaningfull_roots(asm).collect();
    let Some((last, prefix)) = meaningful.split_last() else {
        return false;
    };
    let CILRoot::Branch(backedge) = asm.get_root(*last) else {
        return false;
    };
    if backedge.as_ref() != &(header_id, 0, None) {
        return false;
    }

    let mut update_seen = false;
    for root in prefix {
        match asm.get_root(*root).clone() {
            CILRoot::StLoc(local, value) if local == index_local && !update_seen => {
                let CILNode::BinOp(lhs, rhs, BinOp::Add) = asm.get_node(value) else {
                    return false;
                };
                let CILNode::Const(one) = asm.get_node(*rhs) else {
                    return false;
                };
                if *lhs != index || one.get_type() != index_type || !one.is_one() {
                    return false;
                }
                update_seen = true;
            }
            CILRoot::Pop(value)
                if removable_pop(value, sig, blocks, cleanup_blocks, locals, asm, cache) => {}
            _ => return false,
        }
    }
    update_seen
}

#[allow(clippy::too_many_arguments)]
fn removable_pop(
    node: Interned<CILNode>,
    sig: Interned<FnSig>,
    blocks: &[BasicBlock],
    cleanup_blocks: Option<&[BasicBlock]>,
    locals: &[LocalDef],
    asm: &mut Assembly,
    cache: &mut EffectInfoCache,
) -> bool {
    // Optimization must not erase malformed IR before the fatal verifier sees it. The effect
    // cache is intentionally structural and cannot validate local/argument indices or field
    // ownership, so establish type validity independently for every discarded expression.
    if asm
        .get_node(node)
        .clone()
        .typecheck(sig, locals, asm)
        .is_err()
    {
        return false;
    }
    cache.summary(node, asm).is_pure_total()
        || safe_value_local_field_load(node, blocks, cleanup_blocks, locals, asm)
}

fn safe_value_local_field_load(
    node: Interned<CILNode>,
    blocks: &[BasicBlock],
    cleanup_blocks: Option<&[BasicBlock]>,
    locals: &[LocalDef],
    asm: &Assembly,
) -> bool {
    match asm.get_node(node) {
        CILNode::PtrCast(input, _) | CILNode::IntCast { input, .. } => {
            safe_value_local_field_load(*input, blocks, cleanup_blocks, locals, asm)
        }
        CILNode::LdField { addr, field } => {
            let CILNode::LdLoc(local) = asm.get_node(*addr) else {
                return false;
            };
            let Some((_, local_type)) = locals.get(*local as usize) else {
                return false;
            };
            let Type::ClassRef(local_class) = asm[*local_type] else {
                return false;
            };
            let field = asm.get_field(*field);
            let Some(local_definition) = asm.class_ref_to_def(local_class) else {
                // An external ClassRef does not prove that the field exists or is accessible.
                return false;
            };
            field.owner() == local_class
                && asm.class_ref(local_class).is_valuetype()
                && asm[local_definition]
                    .fields()
                    .iter()
                    .any(|(field_type, field_name, _)| {
                        *field_type == field.tpe() && *field_name == field.name()
                    })
                && !local_address_taken(*local, blocks, cleanup_blocks, asm)
        }
        _ => false,
    }
}

fn normal_predecessors(target: u32, blocks: &[BasicBlock], asm: &Assembly) -> Option<Vec<u32>> {
    let mut predecessors = FxHashSet::default();
    for block in blocks {
        for root in block.roots() {
            let CILRoot::Branch(branch) = asm.get_root(*root) else {
                continue;
            };
            let (branch_target, subtarget, _) = branch.as_ref();
            if (*branch_target == target || *subtarget == target) && *subtarget != 0 {
                return None;
            }
            if *branch_target == target {
                predecessors.insert(block.block_id());
            }
        }
    }
    let mut predecessors: Vec<_> = predecessors.into_iter().collect();
    predecessors.sort_unstable();
    Some(predecessors)
}

fn local_address_taken(
    local: u32,
    blocks: &[BasicBlock],
    cleanup_blocks: Option<&[BasicBlock]>,
    asm: &Assembly,
) -> bool {
    all_roots(blocks, cleanup_blocks).any(|root| {
        CILIter::new(asm.get_root(root).clone(), asm).any(
            |element| matches!(element, CILIterElem::Node(CILNode::LdLocA(found)) if found == local),
        )
    })
}

fn argument_address_taken(
    argument: u32,
    blocks: &[BasicBlock],
    cleanup_blocks: Option<&[BasicBlock]>,
    asm: &Assembly,
) -> bool {
    all_roots(blocks, cleanup_blocks).any(|root| {
        CILIter::new(asm.get_root(root).clone(), asm).any(
            |element| matches!(element, CILIterElem::Node(CILNode::LdArgA(found)) if found == argument),
        )
    })
}

fn all_roots<'a>(
    blocks: &'a [BasicBlock],
    cleanup_blocks: Option<&'a [BasicBlock]>,
) -> impl Iterator<Item = Interned<CILRoot>> + 'a {
    blocks
        .iter()
        .chain(cleanup_blocks.into_iter().flatten())
        .flat_map(BasicBlock::iter_roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Access, ClassDef, ClassRef, Const, ExceptionRegion, FieldDesc, Int, MethodDef,
        StaticFieldDesc,
        cilnode::{IsPure, MethodKind, PtrCastRes},
    };

    #[derive(Clone, Copy)]
    enum InvalidPop {
        Local,
        Argument,
    }

    #[derive(Clone, Copy)]
    struct FixtureConfig {
        step: Const,
        value_type_owner: bool,
        local_value_definition: bool,
        define_field: bool,
        take_index_address: bool,
        effectful_body: bool,
        extra_body_predecessor: bool,
        shared_cleanup: bool,
        invalid_pop: Option<InvalidPop>,
        unsigned_exit_test: bool,
        read_index_on_exit: bool,
        constant_false_guard: bool,
        guard_uses_constant_call: bool,
        read_guard_on_exit: bool,
    }

    impl Default for FixtureConfig {
        fn default() -> Self {
            Self {
                step: Const::USize(1),
                value_type_owner: true,
                local_value_definition: true,
                define_field: true,
                take_index_address: false,
                effectful_body: false,
                extra_body_predecessor: false,
                shared_cleanup: false,
                invalid_pop: None,
                unsigned_exit_test: false,
                read_index_on_exit: false,
                constant_false_guard: false,
                guard_uses_constant_call: false,
                read_guard_on_exit: false,
            }
        }
    }

    fn fixture(config: FixtureConfig) -> (Assembly, MethodDef) {
        let mut asm = Assembly::default();
        let method_owner = asm.main_module();
        let value_name = asm.alloc_string("CountedLoopValue");
        let field_name = asm.alloc_string("field");
        let value_owner = if config.local_value_definition {
            let fields = if config.define_field {
                vec![(Type::Int(Int::USize), field_name, None)]
            } else {
                vec![]
            };
            asm.class_def(ClassDef::new(
                value_name,
                config.value_type_owner,
                0,
                None,
                fields,
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap()
            .0
        } else {
            let external_assembly = asm.alloc_string("External.CountedLoop");
            asm.alloc_class_ref(ClassRef::new(
                value_name,
                Some(external_assembly),
                config.value_type_owner,
                Box::new([]),
            ))
        };
        let field = asm.alloc_field(FieldDesc::new(
            value_owner,
            field_name,
            Type::Int(Int::USize),
        ));
        let value_type = asm.alloc_type(Type::ClassRef(value_owner));
        let usize_type = asm.alloc_type(Type::Int(Int::USize));
        let bool_type = asm.alloc_type(Type::Bool);
        let locals = vec![
            (None, value_type),
            (None, usize_type),
            (None, usize_type),
            (None, bool_type),
        ];

        let mut entry_roots = Vec::new();
        if config.take_index_address {
            let index_address = asm.alloc_node(CILNode::LdLocA(2));
            entry_roots.push(asm.alloc_root(CILRoot::Pop(index_address)));
        }
        entry_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None)))));

        let value = asm.alloc_node(CILNode::LdLoc(0));
        let field_load = asm.alloc_node(CILNode::LdField { addr: value, field });
        let field_load = asm.alloc_node(CILNode::PtrCast(field_load, Box::new(PtrCastRes::USize)));
        let mut step_roots = vec![asm.alloc_root(CILRoot::Pop(field_load))];
        if let Some(invalid_pop) = config.invalid_pop {
            let invalid = match invalid_pop {
                InvalidPop::Local => CILNode::LdLoc(999),
                InvalidPop::Argument => CILNode::LdArg(999),
            };
            let invalid = asm.alloc_node(invalid);
            step_roots.push(asm.alloc_root(CILRoot::Pop(invalid)));
        }
        if config.effectful_body {
            let observed_name = asm.alloc_string("counted_loop_observed");
            let observed = asm.alloc_sfld(StaticFieldDesc::new(
                method_owner.0,
                observed_name,
                Type::Int(Int::USize),
            ));
            let observed_value = asm.alloc_node(Const::USize(1));
            step_roots.push(asm.alloc_root(CILRoot::SetStaticField {
                field: observed,
                val: observed_value,
            }));
        }
        let index = asm.alloc_node(CILNode::LdLoc(2));
        let step = asm.alloc_node(config.step);
        let next = asm.alloc_node(CILNode::BinOp(index, step, BinOp::Add));
        step_roots.push(asm.alloc_root(CILRoot::StLoc(2, next)));
        step_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None)))));

        let bound = asm.alloc_node(CILNode::LdLoc(1));
        let header_roots = if config.unsigned_exit_test {
            vec![
                asm.alloc_root(CILRoot::Branch(Box::new((
                    1,
                    0,
                    Some(BranchCond::Ge(index, bound, CmpKind::Unsigned)),
                )))),
                asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None)))),
            ]
        } else {
            vec![
                asm.alloc_root(CILRoot::Branch(Box::new((
                    2,
                    0,
                    Some(BranchCond::Ne(index, bound)),
                )))),
                asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None)))),
            ]
        };

        let exit_roots = if config.shared_cleanup {
            vec![asm.alloc_root(CILRoot::Branch(Box::new((4, 0, None))))]
        } else if config.read_guard_on_exit {
            let guard = asm.alloc_node(CILNode::LdLoc(3));
            vec![
                asm.alloc_root(CILRoot::Pop(guard)),
                asm.alloc_root(CILRoot::VoidRet),
            ]
        } else if config.read_index_on_exit {
            let index = asm.alloc_node(CILNode::LdLoc(2));
            vec![
                asm.alloc_root(CILRoot::Pop(index)),
                asm.alloc_root(CILRoot::VoidRet),
            ]
        } else {
            vec![asm.alloc_root(CILRoot::VoidRet)]
        };
        let mut blocks = vec![
            BasicBlock::new(entry_roots, 0, None),
            BasicBlock::new(exit_roots, 1, None),
        ];
        if config.constant_false_guard {
            let false_node = if config.guard_uses_constant_call {
                let guard_sig = asm.sig([], Type::Bool);
                let callee_value = asm.alloc_node(Const::Bool(false));
                let callee_ret = asm.alloc_root(CILRoot::Ret(callee_value));
                let callee_name = asm.alloc_string("constant_false_guard_callee");
                let callee = asm.new_method(MethodDef::new(
                    Access::Private,
                    method_owner,
                    callee_name,
                    guard_sig,
                    MethodKind::Static,
                    MethodImpl::MethodBody {
                        blocks: vec![BasicBlock::new(vec![callee_ret], 20, None)],
                        locals: vec![],
                    },
                    vec![],
                ));
                asm.call(callee.0, &[] as &[Interned<CILNode>], IsPure::PURE)
            } else {
                asm.alloc_node(Const::Bool(false))
            };
            let store_guard = asm.alloc_root(CILRoot::StLoc(3, false_node));
            let to_guard = asm.alloc_root(CILRoot::Branch(Box::new((6, 0, None))));
            blocks.push(BasicBlock::new(vec![store_guard, to_guard], 2, None));

            let guard = asm.alloc_node(CILNode::LdLoc(3));
            let to_step = asm.alloc_root(CILRoot::Branch(Box::new((
                7,
                0,
                Some(BranchCond::False(guard)),
            ))));
            let to_failure = asm.alloc_root(CILRoot::Branch(Box::new((8, 0, None))));
            blocks.push(BasicBlock::new(vec![to_step, to_failure], 6, None));
            blocks.push(BasicBlock::new(step_roots, 7, None));
            let failure = asm.alloc_root(CILRoot::VoidRet);
            blocks.push(BasicBlock::new(vec![failure], 8, None));
        } else {
            blocks.push(BasicBlock::new(step_roots, 2, None));
        }
        blocks.push(BasicBlock::new(header_roots, 3, None));
        if config.extra_body_predecessor {
            let to_body = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));
            blocks.push(BasicBlock::new(vec![to_body], 5, None));
        }
        if config.shared_cleanup {
            let ret = asm.alloc_root(CILRoot::VoidRet);
            blocks.push(BasicBlock::new(vec![ret], 4, None));
        }

        let rethrow = asm.alloc_root(CILRoot::ReThrow);
        let cleanup_blocks = vec![BasicBlock::new(vec![rethrow], 10, None)];
        let mut exception_regions = vec![ExceptionRegion::new(2, 10)];
        if config.shared_cleanup {
            exception_regions.push(ExceptionRegion::new(4, 10));
        }
        let sig = asm.sig([], Type::Void);
        let method_name = asm.alloc_string("counted_loop_fixture");
        let method = MethodDef::new(
            Access::Private,
            method_owner,
            method_name,
            sig,
            MethodKind::Static,
            MethodImpl::RegionBody {
                blocks,
                cleanup_blocks,
                exception_regions,
                locals,
            },
            vec![],
        );
        (asm, method)
    }

    fn run_elimination(method: &mut MethodDef, asm: &mut Assembly) {
        let sig = method.sig();
        let mut cache = EffectInfoCache::default();
        let mut fuel = OptFuel::new(u32::MAX);
        eliminate_total_counted_loops(method.implementation_mut(), sig, asm, &mut cache, &mut fuel);
    }

    fn header_was_rewritten(method: &MethodDef, asm: &Assembly) -> bool {
        let MethodImpl::RegionBody { blocks, .. } = method.implementation() else {
            panic!("fixture changed implementation kind")
        };
        let header = blocks
            .iter()
            .find(|block| block.block_id() == 3)
            .expect("fixture header is present");
        let roots: Vec<_> = header.meaningfull_roots(asm).collect();
        matches!(
            roots.as_slice(),
            [store, branch]
                if matches!(asm.get_root(*store), CILRoot::StLoc(index, value) if matches!(asm.get_node(*value), CILNode::LdLoc(bound) if bound != index))
                    && matches!(asm.get_root(*branch), CILRoot::Branch(info) if info.as_ref() == &(1, 0, None))
        )
    }

    fn header_is_direct_exit(method: &MethodDef, asm: &Assembly) -> bool {
        let MethodImpl::RegionBody { blocks, .. } = method.implementation() else {
            panic!("fixture changed implementation kind")
        };
        let header = blocks
            .iter()
            .find(|block| block.block_id() == 3)
            .expect("fixture header is present");
        let roots: Vec<_> = header.meaningfull_roots(asm).collect();
        matches!(
            roots.as_slice(),
            [branch]
                if matches!(asm.get_root(*branch), CILRoot::Branch(info) if info.as_ref() == &(1, 0, None))
        )
    }

    #[test]
    fn eliminates_total_value_field_loop_and_prunes_its_exception_region() {
        let (mut asm, mut method) = fixture(FixtureConfig::default());
        method.typecheck(&mut asm).unwrap();

        run_elimination(&mut method, &mut asm);
        assert!(header_was_rewritten(&method, &asm));
        method.remove_dead_blocks(&mut asm);
        method.typecheck(&mut asm).unwrap();

        let MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            exception_regions,
            ..
        } = method.implementation()
        else {
            panic!("fixture changed implementation kind")
        };
        assert!(!blocks.iter().any(|block| block.block_id() == 2));
        assert!(cleanup_blocks.is_empty());
        assert!(exception_regions.is_empty());
    }

    #[test]
    fn eliminates_unsigned_exit_loop_when_final_index_is_dead() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            unsigned_exit_test: true,
            ..FixtureConfig::default()
        });
        method.typecheck(&mut asm).unwrap();

        run_elimination(&mut method, &mut asm);
        assert!(header_is_direct_exit(&method, &asm));
        method.remove_dead_blocks(&mut asm);
        method.typecheck(&mut asm).unwrap();
    }

    #[test]
    fn eliminates_constant_false_guard_before_unsigned_counted_step() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            unsigned_exit_test: true,
            constant_false_guard: true,
            ..FixtureConfig::default()
        });
        method.typecheck(&mut asm).unwrap();

        run_elimination(&mut method, &mut asm);
        assert!(header_is_direct_exit(&method, &asm));
        method.remove_dead_blocks(&mut asm);
        method.typecheck(&mut asm).unwrap();

        let MethodImpl::RegionBody { blocks, .. } = method.implementation() else {
            panic!("fixture changed implementation kind");
        };
        assert!(
            !blocks
                .iter()
                .any(|block| [2, 6, 7, 8].contains(&block.block_id()))
        );
    }

    #[test]
    fn retains_constant_guard_loop_when_guard_local_is_observed_after_exit() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            unsigned_exit_test: true,
            constant_false_guard: true,
            read_guard_on_exit: true,
            ..FixtureConfig::default()
        });

        run_elimination(&mut method, &mut asm);
        assert!(!header_is_direct_exit(&method, &asm));
    }

    #[test]
    fn retains_unsigned_exit_loop_when_final_index_is_observed() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            unsigned_exit_test: true,
            read_index_on_exit: true,
            ..FixtureConfig::default()
        });

        run_elimination(&mut method, &mut asm);
        assert!(!header_is_direct_exit(&method, &asm));
    }

    #[test]
    fn dead_loop_region_does_not_remove_a_shared_live_cleanup() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            shared_cleanup: true,
            ..FixtureConfig::default()
        });
        method.typecheck(&mut asm).unwrap();

        run_elimination(&mut method, &mut asm);
        method.remove_dead_blocks(&mut asm);
        method.typecheck(&mut asm).unwrap();

        let MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            exception_regions,
            ..
        } = method.implementation()
        else {
            panic!("fixture changed implementation kind")
        };
        assert!(!blocks.iter().any(|block| block.block_id() == 2));
        assert!(blocks.iter().any(|block| block.block_id() == 4));
        assert_eq!(exception_regions, &[ExceptionRegion::new(4, 10)]);
        assert_eq!(cleanup_blocks.len(), 1);
        assert_eq!(cleanup_blocks[0].block_id(), 10);
    }

    #[test]
    fn rejects_non_unit_or_wrong_typed_steps() {
        for step in [Const::USize(2), Const::I32(1)] {
            let (mut asm, mut method) = fixture(FixtureConfig {
                step,
                ..FixtureConfig::default()
            });
            run_elimination(&mut method, &mut asm);
            assert!(!header_was_rewritten(&method, &asm), "step {step:?}");
        }
    }

    #[test]
    fn rejects_effectful_body() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            effectful_body: true,
            ..FixtureConfig::default()
        });
        run_elimination(&mut method, &mut asm);
        assert!(!header_was_rewritten(&method, &asm));
    }

    #[test]
    fn rejects_address_taken_induction_local() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            take_index_address: true,
            ..FixtureConfig::default()
        });
        run_elimination(&mut method, &mut asm);
        assert!(!header_was_rewritten(&method, &asm));
    }

    #[test]
    fn rejects_field_load_from_reference_type_local() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            value_type_owner: false,
            ..FixtureConfig::default()
        });
        run_elimination(&mut method, &mut asm);
        assert!(!header_was_rewritten(&method, &asm));
    }

    #[test]
    fn rejects_fabricated_field_on_external_value_type() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            local_value_definition: false,
            ..FixtureConfig::default()
        });
        // Structural type checking cannot inspect an external definition, but optimization must
        // still preserve the possible MissingField/FieldAccess exception.
        method.typecheck(&mut asm).unwrap();
        run_elimination(&mut method, &mut asm);
        assert!(!header_was_rewritten(&method, &asm));
    }

    #[test]
    fn malformed_pure_nodes_remain_visible_to_the_verifier() {
        for invalid_pop in [InvalidPop::Local, InvalidPop::Argument] {
            let (mut asm, mut method) = fixture(FixtureConfig {
                invalid_pop: Some(invalid_pop),
                ..FixtureConfig::default()
            });
            assert!(method.typecheck(&mut asm).is_err());
            run_elimination(&mut method, &mut asm);
            assert!(!header_was_rewritten(&method, &asm));
            assert!(method.typecheck(&mut asm).is_err());
        }
    }

    #[test]
    fn missing_local_field_remains_visible_to_the_verifier() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            define_field: false,
            ..FixtureConfig::default()
        });
        assert!(method.typecheck(&mut asm).is_err());
        run_elimination(&mut method, &mut asm);
        assert!(!header_was_rewritten(&method, &asm));
        assert!(method.typecheck(&mut asm).is_err());
    }

    #[test]
    fn rejects_body_with_an_additional_normal_predecessor() {
        let (mut asm, mut method) = fixture(FixtureConfig {
            extra_body_predecessor: true,
            ..FixtureConfig::default()
        });
        run_elimination(&mut method, &mut asm);
        assert!(!header_was_rewritten(&method, &asm));
    }

    #[test]
    fn production_optimizer_hook_rewrites_prunes_and_verifies() {
        let (mut asm, method) = fixture(FixtureConfig::default());
        let method = asm.new_method(method);
        let mut fuel = OptFuel::new(256);
        asm.opt(&mut fuel);

        let optimized = asm.method_def(method);
        let MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            exception_regions,
            ..
        } = optimized.implementation()
        else {
            panic!("fixture changed implementation kind")
        };
        assert!(!blocks.iter().any(|block| block.block_id() == 2));
        assert!(cleanup_blocks.is_empty());
        assert!(exception_regions.is_empty());
        asm.verify_for_export().unwrap();
    }

    #[test]
    fn production_optimizer_folds_constant_guard_call_then_eliminates_loop() {
        let (mut asm, method) = fixture(FixtureConfig {
            unsigned_exit_test: true,
            constant_false_guard: true,
            guard_uses_constant_call: true,
            ..FixtureConfig::default()
        });
        let method = asm.new_method(method);
        let mut fuel = OptFuel::new(256);
        asm.opt(&mut fuel);

        let optimized = asm.method_def(method);
        let MethodImpl::RegionBody { blocks, .. } = optimized.implementation() else {
            panic!("fixture changed implementation kind");
        };
        assert!(
            !blocks
                .iter()
                .any(|block| [2, 6, 7, 8].contains(&block.block_id())),
            "constant guard and counted-step blocks must be unreachable after optimization"
        );
        asm.verify_for_export().unwrap();
    }

    #[test]
    fn insufficient_fuel_leaves_the_loop_unchanged() {
        let (mut asm, mut method) = fixture(FixtureConfig::default());
        let sig = method.sig();
        let mut cache = EffectInfoCache::default();
        let mut fuel = OptFuel::new(1);
        eliminate_total_counted_loops(
            method.implementation_mut(),
            sig,
            &mut asm,
            &mut cache,
            &mut fuel,
        );
        assert!(!header_was_rewritten(&method, &asm));
    }
}
