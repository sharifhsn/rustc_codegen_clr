use cilly::{
    cilnode::{IsPure, MethodKind},
    Assembly, BinOp, ClassRef, Const, FieldDesc, Float, Int, Interned, MethodRef, Type,
};
use rustc_abi::{FieldIdx, FieldsShape, Layout, LayoutData, TagEncoding, VariantIdx, Variants};
use rustc_middle::ty::Ty;

use crate::fn_ctx::MethodCompileCtx;

#[derive(Clone, Debug)]
pub(crate) enum FieldOffsetIterator {
    Explicit { offsets: Box<[u32]>, index: usize },
    NoOffset { count: u64 },
    Empty,
}
impl Iterator for FieldOffsetIterator {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        match self {
            Self::Explicit { offsets, index } => {
                let next = offsets.get(*index);
                *index += 1;
                next.copied()
            }
            Self::NoOffset { count } => {
                if *count > 0 {
                    *count -= 1;
                    Some(0)
                } else {
                    None
                }
            }
            Self::Empty => None,
        }
    }
}
impl FieldOffsetIterator {
    pub fn from_fields_shape(fields: &rustc_abi::FieldsShape<FieldIdx>) -> Self {
        match fields {
            FieldsShape::Arbitrary {
                offsets,
                in_memory_order,
            } => {
                let offsets: Box<[_]> = in_memory_order
                    .iter()
                    .enumerate()
                    .map(|(index, _mem_idx)| {
                        u32::try_from(
                            offsets[FieldIdx::from(u32::try_from(index).unwrap())].bytes(),
                        )
                        .unwrap()
                    })
                    //TODO: ask what does field offset of 4294967295 means.
                    .map(|offset| {
                        if offset > u32::from(u16::MAX) {
                            0
                        } else {
                            offset
                        }
                    })
                    .collect();
                FieldOffsetIterator::Explicit { offsets, index: 0 }
            }
            FieldsShape::Union(count) => FieldOffsetIterator::NoOffset {
                count: Into::<usize>::into(*count) as u64,
            },
            FieldsShape::Primitive => Self::Empty,
            FieldsShape::Array { stride, count } => {
                let mut curr: u32 = 0;
                let mut offsets = Vec::new();
                for _ in 0..*count {
                    offsets.push(curr);
                    curr += std::convert::TryInto::<u32>::try_into(stride.bytes())
                        .expect("Array stride too large");
                }
                FieldOffsetIterator::Explicit {
                    offsets: offsets.into(),
                    index: 0,
                }
            }
        }
    }
}
/// Takes layout of an enum as input, and returns the type of its tag(Void if no tag) and the size of the tag(0 if no tag).
pub fn enum_tag_info(r#enum: Layout<'_>, asm: &mut Assembly) -> (Type, u32) {
    match r#enum.variants() {
        Variants::Single { .. } => (
            Type::Void,
            FieldOffsetIterator::from_fields_shape(r#enum.fields())
                .next()
                .unwrap_or(0),
        ),
        Variants::Multiple { tag, tag_field, .. } => (
            scalr_to_type(*tag, asm),
            FieldOffsetIterator::from_fields_shape(r#enum.fields())
                .nth((*tag_field).into())
                .unwrap_or(0),
        ),
        Variants::Empty => (Type::Void, 0),
    }
}
fn scalr_to_type(scalar: rustc_abi::Scalar, asm: &mut Assembly) -> Type {
    let primitive = match scalar {
        rustc_abi::Scalar::Union { value } | rustc_abi::Scalar::Initialized { value, .. } => value,
    };
    primitive_to_type(primitive, asm)
}
fn primitive_to_type(primitive: rustc_abi::Primitive, asm: &mut Assembly) -> Type {
    use rustc_abi::Integer;
    use rustc_abi::Primitive;
    match primitive {
        Primitive::Int(int, sign) => match (int, sign) {
            (Integer::I8, true) => Type::Int(Int::I8),
            (Integer::I16, true) => Type::Int(Int::I16),
            (Integer::I32, true) => Type::Int(Int::I32),
            (Integer::I64, true) => Type::Int(Int::I64),
            (Integer::I128, true) => Type::Int(Int::I128),
            (Integer::I8, false) => Type::Int(Int::U8),
            (Integer::I16, false) => Type::Int(Int::U16),
            (Integer::I32, false) => Type::Int(Int::U32),
            (Integer::I64, false) => Type::Int(Int::U64),
            (Integer::I128, false) => Type::Int(Int::U128),
        },
        Primitive::Float(rustc_abi::Float::F16) => Type::Float(Float::F16),
        Primitive::Float(rustc_abi::Float::F32) => Type::Float(Float::F32),
        Primitive::Float(rustc_abi::Float::F64) => Type::Float(Float::F64),
        Primitive::Float(rustc_abi::Float::F128) => todo!("No support for 128 bit floats yet!"),
        Primitive::Pointer(_) => asm.nptr(Type::Void),
    }
}
pub fn get_variant_at_index(
    variant_index: VariantIdx,
    layout: LayoutData<FieldIdx, rustc_abi::VariantIdx>,
) -> LayoutData<FieldIdx, rustc_abi::VariantIdx> {
    match layout.variants {
        Variants::Single { .. } => layout,
        // `Variants::Multiple.variants` now stores reduced `VariantLayout`s rather than full
        // `LayoutData`s. `LayoutData::for_variant` reconstructs the full per-variant `LayoutData`.
        Variants::Multiple { .. } => LayoutData::for_variant(&layout, variant_index),
        Variants::Empty => todo!("Empty variants have no variants."),
    }
}
pub fn set_discr<'tcx>(
    layout: Layout<'tcx>,
    variant_index: VariantIdx,
    enum_addr: Interned<cilly::ir::CILNode>,
    enum_tpe: Interned<ClassRef>,
    ty: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<cilly::ir::CILRoot> {
    if get_variant_at_index(variant_index, (*layout.0).clone()).is_uninhabited() {
        // Could be skipped, but keeping a throw here can with CIL correctnes. Each block *must* terminate with a jump, return or a throw.
        // By inserting a throw, we are able to remove all code
        // after it safely.
        return ctx.throw_msg(
            "UB: SetDiscirminant used, but the specified enum variant is not inhabited.",
        );
    }
    match layout.variants {
        Variants::Empty => ctx.alloc_root(cilly::CILRoot::Nop),
        Variants::Single { index } => {
            assert_eq!(index, variant_index);
            ctx.alloc_root(cilly::CILRoot::Nop)
        }
        Variants::Multiple {
            tag_encoding: TagEncoding::Direct,
            ..
        } => {
            let (tag_tpe, _) = enum_tag_info(layout, ctx);
            let discr_val = ty
                .discriminant_for_variant(ctx.tcx(), variant_index)
                .unwrap()
                .val;
            // The discriminant is a u128; for 128-bit tags emit it as a real 128-bit literal
            // (no u64 truncation, no IntCast-to-128). Otherwise narrow to u64 then int_to_int.
            let tag_val = match tag_tpe {
                Type::Int(Int::U128) => ctx.alloc_node(discr_val),
                Type::Int(Int::I128) => ctx.alloc_node(discr_val as i128),
                _ => {
                    let tag_val = std::convert::TryInto::<u64>::try_into(discr_val)
                        .expect("Enum varaint id can't fit in u64.");
                    let tag_val = ctx.alloc_node(tag_val);
                    crate::casts::int_to_int(Type::Int(Int::U64), tag_tpe, tag_val, ctx)
                }
            };
            let enum_tag_name = ctx.alloc_string(crate::ENUM_TAG);
            let desc = ctx.alloc_field(FieldDesc::new(enum_tpe, enum_tag_name, tag_tpe));
            ctx.set_field(desc, enum_addr, tag_val)
        }
        Variants::Multiple {
            tag_encoding:
                TagEncoding::Niche {
                    untagged_variant,
                    ref niche_variants,
                    niche_start,
                },
            ..
        } => {
            if variant_index == untagged_variant {
                ctx.alloc_root(cilly::CILRoot::Nop)
            } else {
                let (tag_tpe, _) = enum_tag_info(layout, ctx);
                //let niche = self.project_field(bx, tag_field);
                //let niche_llty = bx.cx().immediate_backend_type(niche.layout);
                let niche_value = variant_index.as_u32() - niche_variants.start.as_u32();
                let niche_value = u128::from(niche_value).wrapping_add(niche_start);
                // niche_value is a u128; for 128-bit tags emit it as a real 128-bit literal.
                let tag_val = match tag_tpe {
                    Type::Int(Int::U128) => ctx.alloc_node(niche_value),
                    Type::Int(Int::I128) => ctx.alloc_node(niche_value as i128),
                    _ => {
                        let tag_val = ctx.alloc_node(
                            std::convert::TryInto::<u64>::try_into(niche_value)
                                .expect("Enum varaint id can't fit in u64."),
                        );
                        crate::casts::int_to_int(Type::Int(Int::U64), tag_tpe, tag_val, ctx)
                    }
                };
                let enum_tag_name = ctx.alloc_string(crate::ENUM_TAG);
                let desc = ctx.alloc_field(FieldDesc::new(enum_tpe, enum_tag_name, tag_tpe));
                ctx.set_field(desc, enum_addr, tag_val)
            }
        }
    }
}

pub fn get_discr<'tcx>(
    layout: Layout<'tcx>,
    enum_addr: Interned<cilly::ir::CILNode>,
    enum_tpe: Interned<ClassRef>,
    ty: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<cilly::ir::CILNode> {
    //return CILNode::
    assert!(!layout.is_uninhabited(), "UB: enum layout is unanhibited!");
    let (tag_tpe, _) = enum_tag_info(layout, ctx);
    let tag_encoding = match layout.variants {
        Variants::Single { index } => {
            let discr_val = ty
                .discriminant_for_variant(ctx.tcx(), index)
                .map_or(u128::from(index.as_u32()), |discr| discr.val);
            // discr_val is a u128; for 128-bit tags emit it as a real 128-bit literal.
            return match tag_tpe {
                Type::Int(Int::U128) => ctx.alloc_node(discr_val),
                Type::Int(Int::I128) => ctx.alloc_node(discr_val as i128),
                _ => {
                    let tag_val = ctx.alloc_node(
                        std::convert::TryInto::<u64>::try_into(discr_val)
                            .expect("Tag does not fit within a u64"),
                    );
                    crate::casts::int_to_int(Type::Int(Int::U64), tag_tpe, tag_val, ctx)
                }
            };
        }
        Variants::Multiple {
            ref tag_encoding, ..
        } => tag_encoding,
        Variants::Empty => {
            let zero = ctx.alloc_node(Const::U64(0));
            return crate::casts::int_to_int(Type::Int(Int::U64), tag_tpe, zero, ctx);
        }
    };

    // Decode the discriminant (specifically if it's niche-encoded).
    let discr = match *tag_encoding {
        TagEncoding::Direct => {
            if tag_tpe == Type::Void {
                //CILNode::LDOb
                todo!();
            } else {
                let enum_tag_name = ctx.alloc_string(crate::ENUM_TAG);
                let field = ctx.alloc_field(FieldDesc::new(enum_tpe, enum_tag_name, tag_tpe));
                ctx.ld_field(enum_addr, field)
            }
        }
        TagEncoding::Niche {
            untagged_variant,
            ref niche_variants,
            niche_start,
        } => {
            let (disrc_type, _) = enum_tag_info(layout, ctx);
            let relative_max = niche_variants.last.as_u32() - niche_variants.start.as_u32();
            let enum_tag_name = ctx.alloc_string(crate::ENUM_TAG);
            let field = ctx.alloc_field(FieldDesc::new(enum_tpe, enum_tag_name, disrc_type));
            let tag = ctx.ld_field(enum_addr, field);
            // We have a subrange `niche_start..=niche_end` inside `range`.
            // If the value of the tag is inside this subrange, it's a
            // "niche value", an increment of the discriminant. Otherwise it
            // indicates the untagged variant.
            // A general algorithm to extract the discriminant from the tag
            // is:
            // relative_tag = tag - niche_start
            // is_niche = relative_tag <= (ule) relative_max
            // discr = if is_niche {
            //     cast(relative_tag) + niche_variants.start
            // } else {
            //     untagged_variant
            // }
            // However, we will likely be able to emit simpler code.
            let (is_niche, tagged_discr, delta) = if relative_max == 0 {
                // Best case scenario: only one tagged variant. This will
                // likely become just a comparison and a jump.
                // The algorithm is:
                // is_niche = tag == niche_start
                // discr = if is_niche {
                //     niche_start
                // } else {
                //     untagged_variant
                // }
                let is_niche = match tag_tpe {
                    Type::Int(Int::U128) => {
                        let mref = ctx.static_mref(
                            "eq_u128",
                            [Type::Int(Int::U128), Type::Int(Int::U128)],
                            Type::Bool,
                        );
                        // `is_niche = tag == niche_start` — compare against the niche *value*
                        // (`niche_start`), NOT the variant index (`niche_variants.start`). They
                        // coincide for niche_start==0 enums (e.g. `Option`), but differ for a
                        // shifted niche like `Result<Big, Small>` (niche_start = 2^128-2), where
                        // using the index would misread the niche variant as the untagged one.
                        let nc = ctx.alloc_node(niche_start);
                        ctx.call(mref, &[tag, nc], IsPure::NOT)
                    }
                    Type::Int(Int::I128) => {
                        let mref = ctx.static_mref(
                            "eq_i128",
                            [Type::Int(Int::I128), Type::Int(Int::I128)],
                            Type::Bool,
                        );
                        let nc = ctx.alloc_node(niche_start as i128);
                        ctx.call(mref, &[tag, nc], IsPure::NOT)
                    }

                    _ => {
                        let nc = ctx.alloc_node(
                            std::convert::TryInto::<u64>::try_into(niche_start)
                                .expect("tag is too big to fit within u64"),
                        );
                        let nc =
                            crate::casts::int_to_int(Type::Int(Int::U64), disrc_type, nc, ctx);
                        ctx.biop(tag, nc, BinOp::Eq)
                    }
                }; //bx.icmp(IntPredicate::IntEQ, tag, niche_start);

                let ts = ctx.alloc_node(u64::from(niche_variants.start.as_u32()));
                let tagged_discr =
                    crate::casts::int_to_int(Type::Int(Int::U64), disrc_type, ts, ctx);
                (is_niche, tagged_discr, 0)
            } else {
                // The special cases don't apply, so we'll have to go with
                // the general algorithm.
                //let tag = crate::casts::int_to_int(disrc_type.clone(), &Type::Int(Int::U64), tag);
                // relative_discr = tag - niche_start. For 128-bit tags use the System.[U]Int128
                // op_Subtraction operator (mirrors the op_GreaterThan arms below); niche_start is
                // emitted as a real 128-bit literal — never a u64 truncation or IntCast-to-128.
                let relative_discr = match tag_tpe {
                    Type::Int(Int::U128) => {
                        let mref = MethodRef::new(
                            ClassRef::uint_128(ctx),
                            ctx.alloc_string("op_Subtraction"),
                            ctx.sig(
                                [Type::Int(Int::U128), Type::Int(Int::U128)],
                                Type::Int(Int::U128),
                            ),
                            MethodKind::Static,
                            vec![].into(),
                        );
                        let mref = ctx.alloc_methodref(mref);
                        let ns = ctx.alloc_node(niche_start);
                        ctx.call(mref, &[tag, ns], IsPure::NOT)
                    }
                    Type::Int(Int::I128) => {
                        let mref = MethodRef::new(
                            ClassRef::int_128(ctx),
                            ctx.alloc_string("op_Subtraction"),
                            ctx.sig(
                                [Type::Int(Int::I128), Type::Int(Int::I128)],
                                Type::Int(Int::I128),
                            ),
                            MethodKind::Static,
                            vec![].into(),
                        );
                        let mref = ctx.alloc_methodref(mref);
                        let ns = ctx.alloc_node(niche_start as i128);
                        ctx.call(mref, &[tag, ns], IsPure::NOT)
                    }
                    _ => {
                        let ns = ctx.alloc_node(
                            std::convert::TryInto::<u64>::try_into(niche_start)
                                .expect("tag is too big to fit within u64"),
                        );
                        let ns =
                            crate::casts::int_to_int(Type::Int(Int::U64), disrc_type, ns, ctx);
                        ctx.biop(tag, ns, BinOp::Sub)
                    }
                };
                let gt = match tag_tpe {
                    Type::Int(Int::U128) => {
                        let mref = MethodRef::new(
                            ClassRef::uint_128(ctx),
                            ctx.alloc_string("op_GreaterThan"),
                            ctx.sig([Type::Int(Int::U128), Type::Int(Int::U128)], Type::Bool),
                            MethodKind::Static,
                            vec![].into(),
                        );
                        let mref = ctx.alloc_methodref(mref);
                        let rm = ctx.alloc_node(u128::from(relative_max));
                        ctx.call(mref, &[relative_discr, rm], IsPure::NOT)
                    }
                    Type::Int(Int::I128) => {
                        let mref = MethodRef::new(
                            ClassRef::int_128(ctx),
                            ctx.alloc_string("op_GreaterThan"),
                            ctx.sig([Type::Int(Int::I128), Type::Int(Int::I128)], Type::Bool),
                            MethodKind::Static,
                            vec![].into(),
                        );
                        let mref = ctx.alloc_methodref(mref);
                        let rm = ctx.alloc_node(u128::from(relative_max) as i128);
                        ctx.call(mref, &[relative_discr, rm], IsPure::NOT)
                    }

                    _ => {
                        let rm = ctx.alloc_node(u64::from(relative_max));
                        let rm =
                            crate::casts::int_to_int(Type::Int(Int::U64), disrc_type, rm, ctx);
                        ctx.biop(relative_discr, rm, BinOp::GtUn)
                    }
                };
                let f = ctx.alloc_node(false);
                let is_niche = ctx.biop(gt, f, BinOp::Eq);
                (
                    is_niche,
                    relative_discr,
                    u128::from(niche_variants.start.as_u32()),
                )
            };

            let tagged_discr = if delta == 0 {
                tagged_discr
            } else {
                // delta = niche_variants.start.as_u32(), always small. The add must happen at the
                // discriminant width: for 128-bit tags use System.[U]Int128 op_Addition (the IR's
                // generic biop Add is not lowered at 128-bit); otherwise narrow to u64 + biop Add.
                match disrc_type {
                    Type::Int(Int::U128) => {
                        let delta = ctx.alloc_node(delta);
                        let mref = MethodRef::new(
                            ClassRef::uint_128(ctx),
                            ctx.alloc_string("op_Addition"),
                            ctx.sig(
                                [Type::Int(Int::U128), Type::Int(Int::U128)],
                                Type::Int(Int::U128),
                            ),
                            MethodKind::Static,
                            vec![].into(),
                        );
                        let mref = ctx.alloc_methodref(mref);
                        ctx.call(mref, &[tagged_discr, delta], IsPure::NOT)
                    }
                    Type::Int(Int::I128) => {
                        let delta = ctx.alloc_node(delta as i128);
                        let mref = MethodRef::new(
                            ClassRef::int_128(ctx),
                            ctx.alloc_string("op_Addition"),
                            ctx.sig(
                                [Type::Int(Int::I128), Type::Int(Int::I128)],
                                Type::Int(Int::I128),
                            ),
                            MethodKind::Static,
                            vec![].into(),
                        );
                        let mref = ctx.alloc_methodref(mref);
                        ctx.call(mref, &[tagged_discr, delta], IsPure::NOT)
                    }
                    _ => {
                        let delta = ctx.alloc_node(
                            std::convert::TryInto::<u64>::try_into(delta)
                                .expect("Tag does not fit within u64"),
                        );
                        let delta =
                            crate::casts::int_to_int(Type::Int(Int::U64), disrc_type, delta, ctx);
                        assert!(matches!(
                            disrc_type,
                            Type::Int(
                                Int::U8
                                    | Int::I8
                                    | Int::U16
                                    | Int::I16
                                    | Int::U32
                                    | Int::I32
                                    | Int::U64
                                    | Int::I64
                                    | Int::USize
                                    | Int::ISize
                            ) | Type::Ptr(_)
                        ));
                        ctx.biop(tagged_discr, delta, BinOp::Add)
                    }
                }
            };

            // In principle we could insert assumes on the possible range of `discr`, but
            // currently in LLVM this seems to be a pessimization.

            let untagged = ctx.alloc_node(u64::from(untagged_variant.as_u32()));
            let untagged =
                crate::casts::int_to_int(Type::Int(Int::U64), disrc_type, untagged, ctx);
            ctx.select(disrc_type, tagged_discr, untagged, is_niche)
        }
    };
    discr

    //discr
}
