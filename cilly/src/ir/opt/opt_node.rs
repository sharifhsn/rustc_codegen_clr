use fxhash::FxHashMap;

use crate::{
    Assembly, BasicBlock, BinOp, CILIter, CILIterElem, CILNode, CILRoot, Const, FnSig, Int,
    MethodDef, MethodImpl, MethodRef, Type,
    bimap::Interned,
    cilnode::{ExtendKind, MethodKind},
    class::ClassDefIdx,
    method::LocalDef,
};

use super::{EffectInfoCache, OptFuel, opt_if_fuel};

/// Returns the value of a same-class static method whose whole body is one constant return.
///
/// Same-class is important: replacing a cross-type static call could suppress that type's CLR
/// initializer. Source locations are metadata-only and do not disqualify an otherwise trivial
/// body. Cleanup/exception regions and legacy nested handlers are rejected rather than analyzed.
fn constant_static_return(
    method: Interned<MethodRef>,
    caller_class: ClassDefIdx,
    asm: &Assembly,
) -> Option<Const> {
    let reference = &asm[method];
    if reference.kind() != MethodKind::Static
        || asm.class_ref_to_def(reference.class()) != Some(caller_class)
    {
        return None;
    }
    let definition = asm.method_def_from_ref(method)?;
    if definition.kind() != MethodKind::Static || definition.class() != caller_class {
        return None;
    }
    constant_return_from_definition(definition, asm)
}

fn constant_return_from_definition(definition: &MethodDef, asm: &Assembly) -> Option<Const> {
    let blocks: &[BasicBlock] = match definition.resolved_implementation(asm) {
        MethodImpl::MethodBody { blocks, .. } => blocks,
        MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            exception_regions,
            ..
        } if cleanup_blocks.is_empty() && exception_regions.is_empty() => blocks,
        _ => return None,
    };
    let [block] = blocks else {
        return None;
    };
    if block.handler().is_some() {
        return None;
    }
    let mut roots = block.meaningfull_roots(asm);
    let root = roots.next()?;
    if roots.next().is_some() {
        return None;
    }
    let CILRoot::Ret(value) = asm.get_root(root) else {
        return None;
    };
    let CILNode::Const(value) = asm.get_node(*value) else {
        return None;
    };
    Some(**value)
}

pub(crate) type ConstantReturnMap = FxHashMap<Interned<MethodRef>, (ClassDefIdx, Const)>;

pub(crate) fn constant_return_map(asm: &Assembly) -> ConstantReturnMap {
    asm.methods_with(|_, _, _| true)
        .filter_map(|(method, definition)| {
            if definition.kind() != MethodKind::Static {
                return None;
            }
            constant_return_from_definition(definition, asm)
                .map(|value| (method.0, (definition.class(), value)))
        })
        .collect()
}

pub(crate) fn has_foldable_constant_call(
    method: &MethodDef,
    returns: &ConstantReturnMap,
    asm: &Assembly,
) -> bool {
    let blocks: Box<dyn Iterator<Item = &BasicBlock> + '_> = match method.implementation() {
        MethodImpl::MethodBody { blocks, .. } => Box::new(blocks.iter()),
        MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            ..
        } => Box::new(blocks.iter().chain(cleanup_blocks)),
        _ => return false,
    };
    blocks.flat_map(BasicBlock::iter_roots).any(|root| {
        CILIter::new(asm.get_root(root).clone(), asm).any(|element| {
            matches!(
                element,
                CILIterElem::Node(CILNode::Call(info))
                    if returns
                        .get(&info.0)
                        .is_some_and(|(owner, _)| *owner == method.class())
            )
        })
    })
}

pub(crate) fn fold_constant_calls(
    method: &mut MethodDef,
    returns: &ConstantReturnMap,
    asm: &mut Assembly,
    fuel: &mut OptFuel,
) -> bool {
    if !has_foldable_constant_call(method, returns, asm) {
        return false;
    }
    let sig = method.sig();
    let caller_class = method.class();
    let locals = method
        .locals()
        .map(|locals| locals.to_vec())
        .unwrap_or_default();
    let changed = std::cell::Cell::new(false);
    let fuel = std::cell::RefCell::new(fuel);
    let mut cache = EffectInfoCache::default();
    method.map_roots(asm, &mut |root, _| root, &mut |node, asm| {
        let CILNode::Call(ref info) = node else {
            return node;
        };
        let Some((owner, value)) = returns.get(&info.0) else {
            return node;
        };
        if *owner != caller_class
            || node.clone().typecheck(sig, &locals, asm).is_err()
            || !info
                .1
                .iter()
                .all(|argument| cache.summary(*argument, asm).is_pure_total())
        {
            return node;
        }
        let mut fuel = fuel.borrow_mut();
        if !fuel.consume(1) {
            return node;
        }
        changed.set(true);
        (*value).into()
    });
    changed.get()
}
/// Optimizes an intiger cast.
fn opt_int_cast(
    original: CILNode,
    asm: &mut Assembly,
    fuel: &mut OptFuel,
    input: Interned<CILNode>,
    target: Int,
    extend: ExtendKind,
) -> CILNode {
    match asm.get_node(input) {
        CILNode::LdField { addr: _, field } if asm[*field].tpe() == Type::Int(target) => {
            asm.get_node(input).clone()
        }
        CILNode::Const(cst) => match (cst.as_ref(), target) {
            (Const::U64(val), Int::USize) => opt_if_fuel(Const::USize(*val).into(), original, fuel),
            (Const::I64(val), Int::ISize) => opt_if_fuel(Const::ISize(*val).into(), original, fuel),
            (Const::U64(val), Int::U64) => opt_if_fuel(Const::U64(*val).into(), original, fuel),
            (Const::I64(val), Int::I64) => opt_if_fuel(Const::I64(*val).into(), original, fuel),
            (Const::U32(val), Int::U32) => opt_if_fuel(Const::U32(*val).into(), original, fuel),
            (Const::I32(val), Int::I32) => opt_if_fuel(Const::I32(*val).into(), original, fuel),
            (Const::I32(val), Int::U32) => {
                opt_if_fuel(Const::U32(*val as u32).into(), original, fuel)
            }
            (Const::U64(val), Int::U8) => opt_if_fuel(Const::U8(*val as u8).into(), original, fuel),
            (Const::I32(val), Int::USize) => match extend {
                ExtendKind::SignExtend => {
                    opt_if_fuel(Const::USize(*val as i64 as u64).into(), original, fuel)
                }
                ExtendKind::ZeroExtend => {
                    opt_if_fuel(Const::USize(*val as u32 as u64).into(), original, fuel)
                }
            },
            _ => original,
        },
        CILNode::IntCast {
            input: input2,
            target: target2,
            extend: extend2,
        } => {
            if target == *target2 && extend == *extend2 {
                return opt_if_fuel(asm.get_node(input).clone(), original, fuel);
            }
            match (target, target2) {
                (Int::USize | Int::ISize, Int::USize | Int::ISize) => {
                    // A usize to isize cast does nothing, except change the type on the evaulation stack(the bits are unchanged).
                    // So, we can just create a cast like it.
                    opt_if_fuel(
                        CILNode::IntCast {
                            input: *input2,
                            target,
                            extend: *extend2,
                        },
                        original,
                        fuel,
                    )
                }
                (Int::U64 | Int::I64, Int::U64 | Int::I64)
                | (Int::U32 | Int::I32, Int::U32 | Int::I32) => {
                    // Same-width signedness casts preserve the bits and only change stack typing.
                    opt_if_fuel(
                        CILNode::IntCast {
                            input: *input2,
                            target,
                            extend: *extend2,
                        },
                        original,
                        fuel,
                    )
                }
                _ => original,
            }
        }
        _ => original,
    }
}
pub fn opt_node(
    original: CILNode,
    asm: &mut Assembly,
    fuel: &mut OptFuel,
    cache: &mut EffectInfoCache,
    sig: Interned<FnSig>,
    locals: &[LocalDef],
    caller_class: ClassDefIdx,
) -> CILNode {
    match original {
        CILNode::SizeOf(tpe) => match asm[tpe] {
            Type::Int(
                int @ (Int::I128
                | Int::I64
                | Int::I32
                | Int::I16
                | Int::I8
                | Int::U128
                | Int::U64
                | Int::U32
                | Int::U16
                | Int::U8),
            ) => opt_if_fuel(
                Const::I32(int.size().unwrap() as i32).into(),
                original,
                fuel,
            ),
            Type::Float(float) => {
                opt_if_fuel(Const::I32(float.size() as i32).into(), original, fuel)
            }
            _ => original,
        },
        CILNode::IntCast {
            input,
            target,
            extend,
        } => opt_int_cast(original, asm, fuel, input, target, extend),
        // This is deliberately not general call inlining. A locally resolved same-class static
        // method that consists solely of `ret <constant>` can be folded only when the original
        // call typechecks and every argument is pure and total. Those conditions preserve
        // once-only argument evaluation, keep malformed IR visible to the fatal verifier, and
        // avoid suppressing another CLR type's initializer.
        CILNode::Call(ref info) => {
            let valid_call = original.clone().typecheck(sig, locals, asm).is_ok();
            let arguments_are_discardable = info
                .1
                .iter()
                .all(|argument| cache.summary(*argument, asm).is_pure_total());
            match (
                valid_call && arguments_are_discardable,
                constant_static_return(info.0, caller_class, asm),
            ) {
                (true, Some(value)) => opt_if_fuel(value.into(), original, fuel),
                _ => original,
            }
        }
        CILNode::LdInd {
            addr,
            tpe,
            volatile: volitale,
        } => match asm.get_node(addr) {
            CILNode::RefToPtr(inner) => opt_if_fuel(
                CILNode::LdInd {
                    addr: *inner,
                    tpe,
                    volatile: volitale,
                },
                original,
                fuel,
            ),
            // Only fold `ldind(ldloca X)` -> `ldloc X` for NON-volatile loads: a `volatile.`
            // prefix is a real acquire fence (ECMA-335 I.12.6.7) and folding it away to a plain
            // `ldloc` would silently drop that fence, which is unsound for e.g. `volatile_load`/
            // `atomic_load` against a directly-owned local's address. Plain loads carry no fence
            // semantics, so folding them is safe.
            CILNode::LdLocA(loc) if !volitale => opt_if_fuel(CILNode::LdLoc(*loc), original, fuel),
            CILNode::LdArgA(loc) if !volitale => opt_if_fuel(CILNode::LdArg(*loc), original, fuel),
            CILNode::LdFieldAddress { addr, field } => {
                let field_desc = asm.get_field(*field);
                if field_desc.tpe() == asm[tpe] {
                    opt_if_fuel(
                        CILNode::LdField {
                            addr: *addr,
                            field: *field,
                        },
                        original,
                        fuel,
                    )
                } else {
                    original
                }
            }
            _ => original,
        },
        CILNode::LdFieldAddress { addr, field } => match asm.get_node(addr) {
            CILNode::RefToPtr(inner) => {
                CILNode::RefToPtr(asm.alloc_node(CILNode::LdFieldAddress {
                    addr: *inner,
                    field,
                }))
            }
            _ => original,
        },
        CILNode::BinOp(lhs, rhs, op @ (BinOp::Add | BinOp::Sub)) => {
            match (asm.get_node(lhs), asm.get_node(rhs)) {
                (CILNode::Const(cst), rhs) if cst.as_ref().is_zero() && op != BinOp::Sub => {
                    rhs.clone()
                }
                (lhs, CILNode::Const(cst)) if cst.as_ref().is_zero() => lhs.clone(),
                _ => original,
            }
        }
        CILNode::BinOp(lhs, rhs, BinOp::Rem | BinOp::RemUn) => {
            match (asm.get_node(lhs), asm.get_node(rhs)) {
                (CILNode::Const(a), CILNode::Const(b)) if a.get_type() == b.get_type() => {
                    match (a.as_ref(), b.as_ref()) {
                        (Const::U8(a), Const::U8(b)) => Const::U8(a.wrapping_rem(*b)).into(),
                        (Const::U16(a), Const::U16(b)) => Const::U16(a.wrapping_rem(*b)).into(),
                        (Const::U32(a), Const::U32(b)) => Const::U32(a.wrapping_rem(*b)).into(),
                        (Const::U64(a), Const::U64(b)) => Const::U64(a.wrapping_rem(*b)).into(),
                        (Const::U128(a), Const::U128(b)) => Const::U128(a.wrapping_rem(*b)).into(),
                        (Const::USize(a), Const::USize(b)) => {
                            Const::USize(a.wrapping_rem(*b)).into()
                        }
                        (Const::I8(a), Const::I8(b)) => Const::I8(a.wrapping_rem(*b)).into(),
                        (Const::I16(a), Const::I16(b)) => Const::I16(a.wrapping_rem(*b)).into(),
                        (Const::I32(a), Const::I32(b)) => Const::I32(a.wrapping_rem(*b)).into(),
                        (Const::I64(a), Const::I64(b)) => Const::I64(a.wrapping_rem(*b)).into(),
                        (Const::I128(a), Const::I128(b)) => Const::I128(a.wrapping_rem(*b)).into(),
                        (Const::ISize(a), Const::ISize(b)) => {
                            Const::ISize(a.wrapping_rem(*b)).into()
                        }
                        _ => original,
                    }
                }
                _ => original,
            }
        }
        CILNode::BinOp(lhs, rhs, BinOp::Mul) => match (asm.get_node(lhs), asm.get_node(rhs)) {
            (CILNode::Const(a), CILNode::Const(b)) if a.get_type() == b.get_type() => {
                match (a.as_ref(), b.as_ref()) {
                    (Const::U8(a), Const::U8(b)) => Const::U8(a.wrapping_mul(*b)).into(),
                    (Const::U16(a), Const::U16(b)) => Const::U16(a.wrapping_mul(*b)).into(),
                    (Const::U32(a), Const::U32(b)) => Const::U32(a.wrapping_mul(*b)).into(),
                    (Const::U64(a), Const::U64(b)) => Const::U64(a.wrapping_mul(*b)).into(),
                    (Const::U128(a), Const::U128(b)) => Const::U128(a.wrapping_mul(*b)).into(),
                    (Const::USize(a), Const::USize(b)) => Const::USize(a.wrapping_mul(*b)).into(),
                    (Const::I8(a), Const::I8(b)) => Const::I8(a.wrapping_mul(*b)).into(),
                    (Const::I16(a), Const::I16(b)) => Const::I16(a.wrapping_mul(*b)).into(),
                    (Const::I32(a), Const::I32(b)) => Const::I32(a.wrapping_mul(*b)).into(),
                    (Const::I64(a), Const::I64(b)) => Const::I64(a.wrapping_mul(*b)).into(),
                    (Const::I128(a), Const::I128(b)) => Const::I128(a.wrapping_mul(*b)).into(),
                    (Const::ISize(a), Const::ISize(b)) => Const::ISize(a.wrapping_mul(*b)).into(),
                    _ => original,
                }
            }
            (CILNode::Const(a), b) if a.is_one() => b.clone(),
            (a, CILNode::Const(b)) if b.is_one() => a.clone(),
            _ => original,
        },
        CILNode::LdField { addr, field } => match asm.get_node(addr) {
            CILNode::RefToPtr(addr) => {
                opt_if_fuel(CILNode::LdField { addr: *addr, field }, original, fuel)
            }
            CILNode::LdLocA(loc) => opt_if_fuel(
                CILNode::LdField {
                    addr: asm.alloc_node(CILNode::LdLoc(*loc)),
                    field,
                },
                original,
                fuel,
            ),
            CILNode::LdArgA(loc) => opt_if_fuel(
                CILNode::LdField {
                    addr: asm.alloc_node(CILNode::LdArg(*loc)),
                    field,
                },
                original,
                fuel,
            ),
            CILNode::LdFieldAddress {
                addr: addr2,
                field: field2,
            } => opt_if_fuel(
                CILNode::LdField {
                    addr: asm.alloc_node(CILNode::LdField {
                        addr: *addr2,
                        field: *field2,
                    }),
                    field,
                },
                original,
                fuel,
            ),
            CILNode::LdInd {
                addr,
                tpe,
                volatile: _,
            } => {
                if let Type::ClassRef(tpe) = asm[*tpe] {
                    if tpe == asm.get_field(field).owner() {
                        opt_if_fuel(CILNode::LdField { addr: *addr, field }, original, fuel)
                    } else {
                        original
                    }
                } else {
                    original
                }
            }
            _ => original,
        },
        _ => original,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Access, MethodDef, cilnode::IsPure};

    fn constant_call_fixture() -> (
        Assembly,
        CILNode,
        Interned<FnSig>,
        ClassDefIdx,
        Interned<CILNode>,
    ) {
        let mut asm = Assembly::default();
        let owner = asm.main_module();
        let u8_type = asm.alloc_type(Type::Int(Int::U8));
        let pointer = Type::Ptr(u8_type);
        let sig = asm.sig([pointer, pointer], Type::Bool);
        let value = asm.alloc_node(Const::Bool(false));
        let ret = asm.alloc_root(CILRoot::Ret(value));
        let method = MethodDef::new(
            Access::Private,
            owner,
            asm.alloc_string("constant_false"),
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![None, None],
        );
        let method = asm.new_method(method).0;
        let lhs = asm.alloc_node(CILNode::LdArg(0));
        let rhs = asm.alloc_node(CILNode::LdArg(1));
        let call = CILNode::Call(Box::new((method, [lhs, rhs].into(), IsPure::PURE)));
        (asm, call, sig, owner, value)
    }

    #[test]
    fn folds_same_class_constant_return_with_pure_total_arguments() {
        let (mut asm, call, sig, owner, value) = constant_call_fixture();
        let mut fuel = OptFuel::new(8);
        let mut cache = EffectInfoCache::default();

        let optimized = opt_node(call, &mut asm, &mut fuel, &mut cache, sig, &[], owner);

        assert_eq!(optimized, asm.get_node(value).clone());
    }

    #[test]
    fn malformed_call_remains_visible_to_the_verifier() {
        let (mut asm, call, sig, owner, _) = constant_call_fixture();
        let CILNode::Call(info) = call else {
            unreachable!()
        };
        let invalid = asm.alloc_node(CILNode::LdArg(99));
        let call = CILNode::Call(Box::new((info.0, [invalid, info.1[1]].into(), info.2)));
        let mut fuel = OptFuel::new(8);
        let mut cache = EffectInfoCache::default();

        let optimized = opt_node(
            call.clone(),
            &mut asm,
            &mut fuel,
            &mut cache,
            sig,
            &[],
            owner,
        );

        assert_eq!(optimized, call);
    }

    #[test]
    fn effectful_argument_evaluation_is_not_discarded() {
        let (mut asm, call, sig, owner, _) = constant_call_fixture();
        let CILNode::Call(info) = call else {
            unreachable!()
        };
        let size = asm.alloc_node(Const::USize(1));
        let allocation = asm.alloc_node(CILNode::LocAlloc { size });
        let call = CILNode::Call(Box::new((info.0, [allocation, info.1[1]].into(), info.2)));
        let mut fuel = OptFuel::new(8);
        let mut cache = EffectInfoCache::default();

        let optimized = opt_node(
            call.clone(),
            &mut asm,
            &mut fuel,
            &mut cache,
            sig,
            &[],
            owner,
        );

        assert_eq!(optimized, call);
    }
}
