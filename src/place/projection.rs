//! Shared planning for MIR place projections.
//!
//! A place is lowered in two stages: [`LoweredPlace::new`] walks every projection except the
//! final one exactly once, then the final projection is interpreted by the requested operation
//! (address, read, or write). Field, sequence, and subslice facts are also planned here so those
//! operations cannot silently disagree about layout offsets, ZST stride, or fat-pointer metadata.

use super::{PlaceTy, body_ty_is_by_address, indexed_element_address};
use crate::fn_ctx::MethodCompileCtx;
use crate::r#type::{
    GetTypeExt,
    adt::{FieldOffsetIterator, field_descrptor, variant_field_desc},
    fat_ptr_to,
    utilis::ptr_is_fat,
};
use cilly::{BinOp, CILNode, CILRoot, Const, FieldDesc, Int, Interned, Type, cilnode::ExtendKind};
use rustc_middle::mir::{Place, PlaceElem};
use rustc_middle::ty::{Mutability, Ty, TyKind};

type Node = Interned<CILNode>;

/// A MIR place after its projection prefix has been lowered.
pub(super) struct LoweredPlace<'tcx> {
    pub(super) local: usize,
    pub(super) result_ty: Ty<'tcx>,
    pub(super) projection: Option<LoweredProjection<'tcx>>,
}

pub(super) struct LoweredProjection<'tcx> {
    pub(super) owner_ty: PlaceTy<'tcx>,
    pub(super) base: Node,
    pub(super) elem: PlaceElem<'tcx>,
}

impl<'tcx> LoweredPlace<'tcx> {
    pub(super) fn new(place: &Place<'tcx>, ctx: &mut MethodCompileCtx<'tcx, '_>) -> Self {
        let result_ty = ctx.monomorphize(place.ty(ctx.body(), ctx.tcx()).ty);
        let Some((last, prefix)) = place.projection.split_last() else {
            return Self {
                local: place.local.as_usize(),
                result_ty,
                projection: None,
            };
        };

        let (mut base, owner_ty) = super::local_body(place.local.as_usize(), ctx);
        let mut owner_ty = PlaceTy::Ty(ctx.monomorphize(owner_ty));
        for elem in prefix {
            let (next_ty, next_base) = super::place_elem_body(elem, owner_ty, ctx, base);
            owner_ty = next_ty.monomorphize(ctx);
            base = next_base;
        }

        Self {
            local: place.local.as_usize(),
            result_ty,
            projection: Some(LoweredProjection {
                owner_ty,
                base,
                elem: last.clone(),
            }),
        }
    }
}

/// The physical storage selected by a `Field` projection.
///
/// `Descriptor` is a real CLR field. `Address` is native-layout storage (including a ZST's
/// provenance-carrying conceptual address). `Unsized` is an already-materialized fat pointer for
/// a DST tail and therefore cannot be read or written by value.
enum FieldStorage {
    Descriptor(Interned<FieldDesc>),
    Address(Node),
    Unsized(Node),
}

pub(super) struct FieldProjection<'tcx> {
    field_ty: Ty<'tcx>,
    lowered_ty: Type,
    storage: FieldStorage,
}

impl<'tcx> FieldProjection<'tcx> {
    pub(super) fn lower(
        elem: &PlaceElem<'tcx>,
        owner_ty: PlaceTy<'tcx>,
        base: Node,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Option<Self> {
        let PlaceElem::Field(field_idx, field_ty) = elem else {
            return None;
        };
        let field_idx = field_idx.as_u32();
        let field_ty = ctx.monomorphize(*field_ty);
        let lowered_ty = ctx.type_from_cache(field_ty);

        let storage = match owner_ty.monomorphize(ctx) {
            PlaceTy::Ty(owner_ty) => {
                let owner_ty = ctx.monomorphize(owner_ty);
                let owner_is_fat = ptr_is_fat(owner_ty, ctx.tcx(), ctx.instance());
                let field_is_fat = ptr_is_fat(field_ty, ctx.tcx(), ctx.instance());
                match (owner_is_fat, field_is_fat) {
                    (false, false) => {
                        let lowered_owner = ctx.type_from_cache(owner_ty);
                        if lowered_owner == Type::Void || lowered_ty == Type::Void {
                            FieldStorage::Address(super::projected_field_address(
                                owner_ty, field_ty, field_idx, base, ctx,
                            ))
                        } else {
                            FieldStorage::Descriptor(field_descrptor(owner_ty, field_idx, ctx))
                        }
                    }
                    (false, true) => {
                        panic!("sized type {owner_ty:?} contains an unsized field {field_ty:?}")
                    }
                    (true, false) => FieldStorage::Address(fat_owner_field_address(
                        owner_ty, field_ty, field_idx, base, ctx,
                    )),
                    (true, true) => FieldStorage::Unsized(fat_owner_tail_pointer(
                        owner_ty, field_ty, field_idx, base, ctx,
                    )),
                }
            }
            PlaceTy::EnumVariant(owner_ty, variant_idx) => {
                let owner_ty = ctx.monomorphize(owner_ty);
                if lowered_ty == Type::Void {
                    FieldStorage::Address(super::projected_variant_field_address(
                        owner_ty,
                        field_ty,
                        field_idx,
                        variant_idx,
                        base,
                        ctx,
                    ))
                } else {
                    FieldStorage::Descriptor(variant_field_desc(
                        owner_ty,
                        field_idx,
                        variant_idx,
                        ctx,
                    ))
                }
            }
        };
        Some(Self {
            field_ty,
            lowered_ty,
            storage,
        })
    }

    pub(super) fn address(self, base: Node, ctx: &mut MethodCompileCtx<'tcx, '_>) -> Node {
        match self.storage {
            FieldStorage::Descriptor(field) => ctx.ld_field_addr(base, field),
            FieldStorage::Address(address) | FieldStorage::Unsized(address) => address,
        }
    }

    pub(super) fn get(self, base: Node, ctx: &mut MethodCompileCtx<'tcx, '_>) -> Node {
        match self.storage {
            FieldStorage::Descriptor(field) => ctx.ld_field(base, field),
            FieldStorage::Address(_) if self.lowered_ty == Type::Void => ctx.uninit_val(Type::Void),
            FieldStorage::Address(address) => ctx.load(address, self.lowered_ty),
            FieldStorage::Unsized(_) => panic!(
                "cannot read unsized field {:?} by value; use its address",
                self.field_ty
            ),
        }
    }

    pub(super) fn set(
        self,
        base: Node,
        value: Node,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Interned<CILRoot> {
        match self.storage {
            FieldStorage::Descriptor(field) => ctx.set_field(field, base, value),
            FieldStorage::Address(_) if self.lowered_ty == Type::Void => {
                ctx.alloc_root(CILRoot::Nop)
            }
            FieldStorage::Address(address) => {
                super::ptr_set_op(self.field_ty.into(), ctx, address, value)
            }
            FieldStorage::Unsized(_) => {
                panic!("cannot assign unsized field {:?} by value", self.field_ty)
            }
        }
    }

    pub(super) fn body(
        self,
        base: Node,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> (PlaceTy<'tcx>, Node) {
        let node = match self.storage {
            FieldStorage::Descriptor(field) if body_ty_is_by_address(self.field_ty, ctx) => {
                ctx.ld_field_addr(base, field)
            }
            FieldStorage::Descriptor(field) => ctx.ld_field(base, field),
            FieldStorage::Address(address)
                if self.lowered_ty == Type::Void || body_ty_is_by_address(self.field_ty, ctx) =>
            {
                address
            }
            FieldStorage::Address(address) => ctx.load(address, self.lowered_ty),
            FieldStorage::Unsized(pointer) => pointer,
        };
        (self.field_ty.into(), node)
    }
}

fn field_offset<'tcx>(
    owner_ty: Ty<'tcx>,
    field_idx: u32,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> u32 {
    FieldOffsetIterator::fields(ctx.layout_of(owner_ty).layout.0.0.clone())
        .nth(field_idx as usize)
        .expect("field index not in rustc layout")
}

fn fat_owner_data<'tcx>(
    owner_ty: Ty<'tcx>,
    base: Node,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> (Interned<cilly::ClassRef>, Node) {
    let fat_ptr = ctx
        .type_from_cache(Ty::new_ptr(ctx.tcx(), owner_ty, Mutability::Mut))
        .as_class_ref()
        .expect("fat pointer did not lower to a class");
    let void_ptr = ctx.nptr(Type::Void);
    let data = FieldDesc::new(fat_ptr, ctx.alloc_string(cilly::DATA_PTR), void_ptr);
    (fat_ptr, ctx.ld_field(base, data))
}

fn fat_owner_field_address<'tcx>(
    owner_ty: Ty<'tcx>,
    field_ty: Ty<'tcx>,
    field_idx: u32,
    base: Node,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    let offset = field_offset(owner_ty, field_idx, ctx);
    let (_, data) = fat_owner_data(owner_ty, base, ctx);
    let address = if offset == 0 {
        data
    } else {
        ctx.biop(data, Const::USize(u64::from(offset)), BinOp::Add)
    };
    let lowered = ctx.type_from_cache(field_ty);
    ctx.cast_ptr(address, lowered)
}

fn fat_owner_tail_pointer<'tcx>(
    owner_ty: Ty<'tcx>,
    field_ty: Ty<'tcx>,
    field_idx: u32,
    base: Node,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Node {
    let offset = field_offset(owner_ty, field_idx, ctx);
    let (fat_ptr, data) = fat_owner_data(owner_ty, base, ctx);
    let metadata = FieldDesc::new(
        fat_ptr,
        ctx.alloc_string(cilly::METADATA),
        Type::Int(Int::USize),
    );
    let metadata = ctx.ld_field(base, metadata);

    // A dyn tail's runtime alignment may exceed its statically known minimum layout alignment.
    let tail_is_dyn = matches!(
        ctx.tcx()
            .struct_tail_for_codegen(field_ty, rustc_middle::ty::TypingEnv::fully_monomorphized(),)
            .kind(),
        TyKind::Dynamic(..)
    );
    let offset = if tail_is_dyn {
        let isize_size = ctx.size_of(Int::ISize);
        let two = ctx.alloc_node(2_i32);
        let align_slot = ctx.biop(isize_size, two, BinOp::Mul);
        let align_slot = ctx.int_cast(align_slot, Int::USize, ExtendKind::ZeroExtend);
        let align_addr = ctx.biop(metadata, align_slot, BinOp::Add);
        let align_ptr = ctx.cast_ptr(align_addr, Type::Int(Int::USize));
        let align = ctx.load(align_ptr, Type::Int(Int::USize));
        let one = ctx.alloc_node(Const::USize(1));
        let align_minus_one = ctx.biop(align, one, BinOp::Sub);
        let offset = ctx.alloc_node(Const::USize(u64::from(offset)));
        let rounded = ctx.biop(offset, align_minus_one, BinOp::Add);
        let all_ones = u64::try_from(ctx.target_layout().unsigned_pointer_max())
            .expect("supported target pointers fit in u64");
        let all_ones = ctx.alloc_node(Const::USize(all_ones));
        let mask = ctx.biop(align_minus_one, all_ones, BinOp::XOr);
        ctx.biop(rounded, mask, BinOp::And)
    } else {
        ctx.alloc_node(Const::USize(u64::from(offset)))
    };
    let data = ctx.biop(data, offset, BinOp::Add);
    let field_ptr = ctx
        .type_from_cache(Ty::new_ptr(ctx.tcx(), field_ty, Mutability::Mut))
        .as_class_ref()
        .expect("unsized field pointer did not lower to a fat-pointer class");
    ctx.create_slice(field_ptr, data, metadata)
}

pub(super) struct SequenceProjection<'tcx> {
    pub(super) element_ty: Ty<'tcx>,
    pub(super) address: Node,
}

impl<'tcx> SequenceProjection<'tcx> {
    pub(super) fn lower(
        elem: &PlaceElem<'tcx>,
        owner_ty: PlaceTy<'tcx>,
        base: Node,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Option<Self> {
        let owner_ty = owner_ty
            .monomorphize(ctx)
            .as_ty()
            .unwrap_or_else(|| panic!("sequence projection on enum variant: {elem:?}"));
        let index = match elem {
            PlaceElem::Index(index) => super::local_get(index.as_usize(), ctx.body(), ctx),
            PlaceElem::ConstantIndex {
                offset, from_end, ..
            } => match owner_ty.kind() {
                TyKind::Slice(element) if *from_end => {
                    let element = ctx.monomorphize(*element);
                    let fat_ptr = fat_ptr_to(element, ctx);
                    let metadata = FieldDesc::new(
                        fat_ptr,
                        ctx.alloc_string(cilly::METADATA),
                        Type::Int(Int::USize),
                    );
                    let len = ctx.ld_field(base, metadata);
                    ctx.biop(len, Const::USize(*offset), BinOp::Sub)
                }
                TyKind::Array(_, _) if *from_end => {
                    panic!("rustc emitted from-end ConstantIndex for an array")
                }
                _ => ctx.alloc_node(Const::USize(*offset)),
            },
            _ => return None,
        };

        let (element_ty, address) = match owner_ty.kind() {
            TyKind::Slice(element) => {
                let element_ty = ctx.monomorphize(*element);
                let lowered = ctx.type_from_cache(element_ty);
                let fat_ptr = fat_ptr_to(element_ty, ctx);
                let data = FieldDesc::new(
                    fat_ptr,
                    ctx.alloc_string(cilly::DATA_PTR),
                    ctx.nptr(Type::Void),
                );
                let data = ctx.ld_field(base, data);
                (
                    element_ty,
                    indexed_element_address(data, index, lowered, ctx),
                )
            }
            TyKind::Array(element, _) => {
                let element_ty = ctx.monomorphize(*element);
                let address =
                    super::address::array_element_address(ctx, element_ty, owner_ty, base, index);
                (element_ty, address)
            }
            _ => return None,
        };
        Some(Self {
            element_ty,
            address,
        })
    }

    pub(super) fn body(self, ctx: &mut MethodCompileCtx<'tcx, '_>) -> (PlaceTy<'tcx>, Node) {
        let node = if body_ty_is_by_address(self.element_ty, ctx) {
            self.address
        } else {
            let lowered = ctx.type_from_cache(self.element_ty);
            if lowered == Type::Void {
                self.address
            } else {
                ctx.load(self.address, lowered)
            }
        };
        (self.element_ty.into(), node)
    }
}

pub(super) struct SubsliceProjection<'tcx> {
    pub(super) result_ty: Ty<'tcx>,
    pub(super) address: Node,
}

impl<'tcx> SubsliceProjection<'tcx> {
    pub(super) fn lower(
        elem: &PlaceElem<'tcx>,
        owner_ty: PlaceTy<'tcx>,
        base: Node,
        ctx: &mut MethodCompileCtx<'tcx, '_>,
    ) -> Option<Self> {
        let PlaceElem::Subslice { from, to, from_end } = elem else {
            return None;
        };
        let owner_ty = owner_ty
            .monomorphize(ctx)
            .as_ty()
            .expect("subslice projection on enum variant");
        let element_ty = ctx.monomorphize(owner_ty.sequence_element_type(ctx.tcx()));
        let lowered_element = ctx.type_from_cache(element_ty);
        let from_node = ctx.alloc_node(Const::USize(*from));

        match owner_ty.kind() {
            TyKind::Array(_, length) => {
                let length = length
                    .try_to_target_usize(ctx.tcx())
                    .expect("non-constant array length in subslice projection");
                let result_length = if *from_end {
                    length - (*from + *to)
                } else {
                    *to - *from
                };
                let result_ty = Ty::new_array(ctx.tcx(), element_ty, result_length);
                let address = indexed_element_address(base, from_node, lowered_element, ctx);
                let result_ptr =
                    ctx.type_from_cache(Ty::new_ptr(ctx.tcx(), result_ty, Mutability::Mut));
                Some(Self {
                    result_ty,
                    address: ctx.cast_ptr_to(address, result_ptr),
                })
            }
            TyKind::Slice(_) => {
                debug_assert!(
                    *from_end,
                    "PlaceElem slice subslices count `to` from the end"
                );
                let fat_ptr = fat_ptr_to(element_ty, ctx);
                let data = FieldDesc::new(
                    fat_ptr,
                    ctx.alloc_string(cilly::DATA_PTR),
                    ctx.nptr(Type::Void),
                );
                let metadata = FieldDesc::new(
                    fat_ptr,
                    ctx.alloc_string(cilly::METADATA),
                    Type::Int(Int::USize),
                );
                let source_len = ctx.ld_field(base, metadata);
                let result_len = if *from_end {
                    ctx.biop(source_len, Const::USize(*from + *to), BinOp::Sub)
                } else {
                    ctx.alloc_node(Const::USize(*to - *from))
                };
                let data = ctx.ld_field(base, data);
                let data = indexed_element_address(data, from_node, lowered_element, ctx);
                Some(Self {
                    result_ty: Ty::new_slice(ctx.tcx(), element_ty),
                    address: ctx.create_slice(fat_ptr, data, result_len),
                })
            }
            _ => None,
        }
    }
}
