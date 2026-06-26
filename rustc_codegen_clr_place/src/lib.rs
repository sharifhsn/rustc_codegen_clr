#![feature(rustc_private)]
extern crate rustc_abi;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_middle;
use cilly::Interned;
use cilly::Const;
use rustc_codegen_clr_ctx::MethodCompileCtx;
use rustc_codegen_clr_type::GetTypeExt;
use rustc_codegen_clr_type::utilis::pointer_to_is_fat;

use rustc_middle::mir::Place;

mod address;
mod body;
mod get;
mod set;
pub use address::*;
pub use body::*;
pub use get::*;
use rustc_middle::ty::{Ty, TyKind};
pub use set::*;

fn slice_head<T>(slice: &[T]) -> (&T, &[T]) {
    assert!(!slice.is_empty());
    let last = &slice[slice.len() - 1];
    (last, &slice[..(slice.len() - 1)])
}
fn pointed_type(ty: PlaceTy) -> Ty {
    if let PlaceTy::Ty(ty) = ty {
        if let TyKind::Ref(_region, inner, _mut) = ty.kind() {
            *inner
        } else if let TyKind::RawPtr(ty, _) = ty.kind() {
            *ty
        } else {
            panic!("{ty:?} is not a pointer type!");
        }
    } else {
        panic!("Can't dereference enum variant!");
    }
}
fn body_ty_is_by_address<'tcx>(last_ty: Ty<'tcx>, ctx: &mut MethodCompileCtx<'tcx, '_>) -> bool {
    match *last_ty.kind() {
        // True for non-0 tuples
        TyKind::Tuple(elements) => !elements.is_empty(),

        //TODO: check if slices are handled propely
        TyKind::Adt(_, _)
        | TyKind::Closure(_, _)
        | TyKind::Coroutine(_, _)
        | TyKind::Array(_, _)
        | TyKind::Slice(_)
        | TyKind::Str => true,

        // A `fn()` is a pointer-sized leaf scalar with no projectable structure, so it is
        // by-value exactly like the other scalar leaves. (Defensive for I3 totality — a bare
        // fn-ptr only reaches here in well-formed MIR wrapped in a Ref/RawPtr, handled below.)
        TyKind::Int(_)
        | TyKind::Float(_)
        | TyKind::Uint(_)
        | TyKind::Bool
        | TyKind::Char
        | TyKind::FnPtr(..) => false,
        TyKind::Ref(_, ty, _) | TyKind::RawPtr(ty, _) => {
            pointer_to_is_fat(ty, ctx.tcx(), ctx.instance())
        }
        _ => todo!(
            "TODO: body_ty_is_by_address does not support type {last_ty:?} kind:{kind:?}",
            kind = last_ty.kind()
        ),
    }
}

/// Given a type `derefed_type`, it retuns a set of instructions to get a value behind a pointer to `derefed_type`.
pub fn deref_op<'tcx>(
    derefed_type: PlaceTy<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    ptr: Interned<cilly::ir::CILNode>,
) -> Interned<cilly::ir::CILNode> {
    let ptr = Box::new(ptr);
    let res = if let PlaceTy::Ty(derefed_type) = derefed_type {
        let derefed_type = ctx.type_from_cache(derefed_type);
        ctx.load(*ptr, derefed_type)
    } else {
        todo!("Can't dereference enum variants!")
    };
    res
}

/// Returns the ops for getting the address of a given place.
pub fn place_address<'a>(
    place: &Place<'a>,
    ctx: &mut MethodCompileCtx<'a, '_>,
) -> Interned<cilly::ir::CILNode> {
    let place_ty = place.ty(ctx.body(), ctx.tcx());
    let place_ty = ctx.monomorphize(place_ty).ty;

    let layout = ctx.layout_of(place_ty);
    // A *free-standing* ZST place (a ZST local with no projection) has no storage, so a dangling
    // but correctly-aligned pointer is the right answer (matching `NonNull::dangling`). But this
    // must NOT be applied to a PROJECTED ZST place — the address of `struct.zst_field` /
    // `(*ptr).zst_field` is a real, offset-correct pointer (only *dereferencing* it is a no-op),
    // and code relies on that: e.g. `Arc::as_ptr` is `&raw (*inner).data` for a ZST `data` field,
    // a handle that `Arc::from_raw` reverses by subtracting the field offset. Short-circuiting a
    // projected ZST place to a dangling sentinel made `Arc<ZST>`/`Waker::from(Arc<W>)`
    // AccessViolate. Projected places fall through to the projection machinery below (the final
    // ZST field is handled by `field_address`, which computes `base + offset`).
    if place.projection.is_empty() {
        if layout.is_zst() {
            let place_type = ctx.type_from_cache(place_ty);
            let node = ctx.alloc_node(Const::USize(layout.align.abi.bytes()));
            return ctx.cast_ptr(node, place_type);
        }
        let loc_ty = ctx.monomorphize(ctx.body().local_decls[place.local].ty);
        if pointer_to_is_fat(loc_ty, ctx.tcx(), ctx.instance()) {
            local_get(place.local.as_usize(), ctx.body(), ctx)
        } else {
            local_address(place.local.as_usize(), ctx.body(), ctx)
        }
    } else {
        // A projected ZST place keeps the dangling sentinel UNLESS it ends in a `Field` projection:
        // the address of a ZST field of a (non-ZST) container is a real, offset-correct pointer that
        // code round-trips (`Arc::as_ptr` = `&raw (*inner).data`), handled by `field_address`. Other
        // ZST projections (Index / Deref / Downcast) keep the sentinel — their address is never used
        // as a handle, and routing them through the general machinery would surface ZST paths the
        // blanket short-circuit historically masked.
        if layout.is_zst() {
            // Only a `Field` reached through `Deref`s alone (`&s.z` = `[Field]`, `Arc::as_ptr` =
            // `[Deref, Field]`) gets the real base+offset address — `field_address` handles the final
            // ZST field, and the `Deref`-only body cannot hit the field-of-non-object paths the
            // blanket short-circuit historically masked. Every other ZST place keeps the dangling
            // sentinel (its address is never used as a round-trip handle).
            let (head, body) = slice_head(place.projection);
            let field_of_derefs = matches!(head, rustc_middle::mir::PlaceElem::Field(..))
                && body
                    .iter()
                    .all(|e| matches!(e, rustc_middle::mir::PlaceElem::Deref));
            if !field_of_derefs {
                let place_type = ctx.type_from_cache(place_ty);
                let node = ctx.alloc_node(Const::USize(layout.align.abi.bytes()));
                return ctx.cast_ptr(node, place_type);
            }
        }
        let (mut addr_calc, mut ty) = local_body(place.local.as_usize(), ctx);

        ty = ctx.monomorphize(ty);
        let mut ty = ty.into();

        let (head, body) = slice_head(place.projection);
        for elem in body {
            let (curr_ty, curr_ops) = place_elem_body(elem, ty, ctx, addr_calc);
            ty = curr_ty.monomorphize(ctx);
            addr_calc = curr_ops;
        }
        address::place_elem_address(head, ty, ctx, place_ty, addr_calc)
    }
}
/// Should be only used in certain builit-in features. For unsized types, returns the address of the fat pointer, not the address contained within it.
pub fn place_address_raw<'a>(
    place: &Place<'a>,
    ctx: &mut MethodCompileCtx<'a, '_>,
) -> Interned<cilly::ir::CILNode> {
    let place_ty = place.ty(ctx.body(), ctx.tcx());
    let place_ty = ctx.monomorphize(place_ty).ty;

    let layout = ctx.layout_of(place_ty);
    if layout.is_zst() {
        return ctx.alloc_node(Const::USize(layout.align.abi.bytes()));
    }
    if place.projection.is_empty() {
        local_address(place.local.as_usize(), ctx.body(), ctx)
    } else if place.projection.len() == 1
        && matches!(
            slice_head(place.projection).0,
            rustc_middle::mir::PlaceElem::Deref
        )
        && pointer_to_is_fat(place_ty, ctx.tcx(), ctx.instance())
    {
        // The deref'd place is *itself* unsized (a DST), so a pointer to it is fat.
        // `place_address_raw`'s contract is to hand back the address of the fat-pointer
        // storage in that case, which is exactly the address of the local being deref'd.
        return local_address(place.local.as_usize(), ctx.body(), ctx);
    } else {
        let (mut addr_calc, mut ty) = local_body(place.local.as_usize(), ctx);

        ty = ctx.monomorphize(ty);
        let mut ty = ty.into();

        let (head, body) = slice_head(place.projection);
        for elem in body {
            let (curr_ty, curr_ops) = place_elem_body(elem, ty, ctx, addr_calc);
            ty = curr_ty.monomorphize(ctx);
            addr_calc = curr_ops;
        }
        address::place_elem_address(head, ty, ctx, place_ty, addr_calc)
    }
}
pub fn place_set<'tcx>(
    place: &Place<'tcx>,
    value_calc: Interned<cilly::ir::CILNode>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<cilly::ir::CILRoot> {
    if place.projection.is_empty() {
        set::local_set(place.local.as_usize(), ctx.body(), value_calc, ctx)
    } else {
        let (mut addr_calc, ty) = local_body(place.local.as_usize(), ctx);

        let mut ty: PlaceTy = ty.into();
        ty = ty.monomorphize(ctx);

        let (head, body) = slice_head(place.projection);
        for elem in body {
            let (curr_ty, curr_ops) = place_elem_body(elem, ty, ctx, addr_calc);
            ty = curr_ty.monomorphize(ctx);
            addr_calc = curr_ops;
        }
        //
        ty = ty.monomorphize(ctx);
        place_elem_set(head, ty, ctx, addr_calc, value_calc)
    }
}
#[derive(Debug, Clone, Copy)]
pub enum PlaceTy<'tcx> {
    Ty(Ty<'tcx>),
    EnumVariant(Ty<'tcx>, u32),
}
impl<'tcx> From<Ty<'tcx>> for PlaceTy<'tcx> {
    fn from(ty: Ty<'tcx>) -> Self {
        Self::Ty(ty)
    }
}
impl<'tcx> PlaceTy<'tcx> {
    pub fn monomorphize(&self, ctx: &mut MethodCompileCtx<'tcx, '_>) -> Self {
        match self {
            Self::Ty(inner) => Self::Ty(ctx.monomorphize(*inner)),
            Self::EnumVariant(enm, variant) => Self::EnumVariant(ctx.monomorphize(*enm), *variant),
        }
    }
    pub fn as_ty(&self) -> Option<Ty<'tcx>> {
        match self {
            Self::Ty(inner) => Some(*inner),
            Self::EnumVariant(..) => None,
        }
    }
    /// Returns the kind of the underlyting Ty.
    pub fn kind(&self) -> &TyKind<'tcx> {
        match self {
            Self::Ty(ty) => ty.kind(),
            //TODO: find a better way to get the emum variant!
            Self::EnumVariant(ty, _variant) => ty.kind(),
        }
    }
}
