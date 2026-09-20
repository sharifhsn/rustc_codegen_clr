//! Fail-closed classification for Rust storage that may contain CLR GC references.
//!
//! A naked CLR reference is valid while it is held in a managed evaluation-stack/local/argument
//! slot. It is not valid inside Rust-owned byte storage: Rust allocation, `cpblk`, enum overlap,
//! and ordinary aggregate moves do not participate in the CLR write barrier or GC map.  Rooted
//! wrappers opt into storage explicitly by containing only a `GCHandle` token.

use crate::{
    assembly::{MethodCompileCtx, is_explicit_local_export},
    fn_ctx::fn_name_for_instance,
    r#type::utilis::{
        INTEROP_ARR_TPE_NAME, INTEROP_BYREF_TPE_NAME, INTEROP_CLASS_TPE_NAME,
        INTEROP_GENERIC_STRUCT_TPE_NAME, INTEROP_GENERIC_TPE_NAME, INTEROP_METHOD_GENERIC_TPE_NAME,
        INTEROP_STRUCT_TPE_NAME, INTEROP_TYPE_GENERIC_TPE_NAME,
    },
};
use rustc_abi::ExternAbi;
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_hir::{attrs::lang_items::LangItem, def::DefKind};
use rustc_infer::infer::TyCtxtInferExt;
use rustc_middle::{
    mir::{
        AggregateKind, CastKind, Local, Location, NonDivergingIntrinsic, Operand, Place,
        ProjectionElem, Rvalue, StatementKind, TerminatorKind,
        visit::{NonMutatingUseContext, PlaceContext, Visitor},
    },
    ty::{
        CoroutineArgsExt, GenericArgKind, GenericParamDefKind, Instance, ParamEnv, Ty, TyKind,
        TypingEnv, TypingMode,
    },
};
use rustc_span::{Symbol, sym};
use rustc_trait_selection::infer::InferCtxtExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedStorageKind {
    NakedReference,
    ManagedByRef,
    OpaqueManagedValue,
}

impl ManagedStorageKind {
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::NakedReference => "a naked CLR object/array reference",
            Self::ManagedByRef => "a managed byref",
            Self::OpaqueManagedValue => {
                "a managed value type whose internal GC-reference map is not known"
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedStorageViolation {
    pub kind: ManagedStorageKind,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectManagedKind {
    Unsafe(ManagedStorageKind),
}

/// Trait selection after monomorphization cannot prove lifetime-specific unsafe contracts: all
/// regions have already been erased. Keep the storage capability on named ADTs and reject any ADT
/// instantiation whose shape still carries a lifetime or reference. This deliberately excludes
/// promises such as `&'static T` and `Container<&'static T>` from being widened to shorter erased
/// lifetimes, while retaining the audited token/value wrappers (whose type/const arguments are
/// region-free).
fn native_storage_capability_shape_is_exact<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
    seen: &mut FxHashSet<Ty<'tcx>>,
) -> bool {
    let TyKind::Adt(def, args) = ty.kind() else {
        return false;
    };
    if !seen.insert(ty) {
        return true;
    }
    if ctx
        .tcx()
        .generics_of(def.did())
        .own_params
        .iter()
        .any(|parameter| matches!(parameter.kind, GenericParamDefKind::Lifetime))
    {
        return false;
    }

    args.iter().all(|argument| match argument.kind() {
        GenericArgKind::Lifetime(_) => false,
        GenericArgKind::Const(_) => true,
        GenericArgKind::Type(argument_ty) => {
            native_storage_capability_argument_is_region_free(argument_ty, ctx, seen)
        }
    })
}

fn native_storage_capability_argument_is_region_free<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
    seen: &mut FxHashSet<Ty<'tcx>>,
) -> bool {
    match ty.kind() {
        TyKind::Adt(..) => native_storage_capability_shape_is_exact(ty, ctx, seen),
        TyKind::Array(element, _)
        | TyKind::Slice(element)
        | TyKind::RawPtr(element, _)
        | TyKind::Pat(element, _) => {
            native_storage_capability_argument_is_region_free(*element, ctx, seen)
        }
        TyKind::Tuple(elements) => elements
            .iter()
            .all(|element| native_storage_capability_argument_is_region_free(element, ctx, seen)),
        // The real capability implementations use concrete ADTs, primitives, and const identity
        // arguments. Unresolved/bound/function shapes are not needed by the public contract, so
        // refusing them is the sound fail-closed boundary.
        TyKind::Ref(..)
        | TyKind::Dynamic(..)
        | TyKind::Alias(..)
        | TyKind::Param(..)
        | TyKind::Bound(..)
        | TyKind::Placeholder(..)
        | TyKind::Infer(..)
        | TyKind::Error(..)
        | TyKind::UnsafeBinder(..)
        | TyKind::FnDef(..)
        | TyKind::FnPtr(..) => false,
        _ => true,
    }
}

/// Query the explicit unsafe capability rather than trusting a forgeable name/doc marker. The
/// trait is deliberately separate from `ManagedSafe`: many CLR value types are legal call-boundary
/// values but contain object refs and are not legal native Rust bytes.
fn is_native_storage_safe<'tcx>(ty: Ty<'tcx>, ctx: &MethodCompileCtx<'tcx, '_>) -> bool {
    let Some(trait_id) = ctx
        .tcx()
        .get_diagnostic_item(Symbol::intern("rustc_codegen_clr_native_storage_safe"))
    else {
        return false;
    };
    // A replacement crate can declare a diagnostic item with the same name. Requiring an unsafe
    // trait preserves the essential contract even without a globally stable crate identity: no
    // safe impl can opt arbitrary bytes out of the GC-reference wall.
    if ctx.tcx().def_kind(trait_id) != DefKind::Trait
        || !ctx.tcx().trait_def(trait_id).safety.is_unsafe()
    {
        return false;
    }
    if !native_storage_capability_shape_is_exact(ty, ctx, &mut FxHashSet::default()) {
        return false;
    }
    ctx.tcx()
        .infer_ctxt()
        .build(TypingMode::PostAnalysis)
        .type_implements_trait(trait_id, [ty], ParamEnv::empty())
        .must_apply_modulo_regions()
}

/// Whether `ty` explicitly opted into the compiler's foundational raw managed-type ABI.
///
/// Crate/module/name/doc provenance remains useful collision resistance, but it is not authority:
/// a replacement crate can reproduce all of it in safe Rust. This separate unsafe trait makes CLR
/// interpretation an explicit unsafe contract while remaining distinct from native-storage safety.
#[must_use]
pub fn is_managed_interop_type<'tcx>(ty: Ty<'tcx>, ctx: &MethodCompileCtx<'tcx, '_>) -> bool {
    let ty = ctx.monomorphize(ty);
    let Some(trait_id) = ctx
        .tcx()
        .get_diagnostic_item(Symbol::intern("rustc_codegen_clr_managed_interop_type"))
    else {
        return false;
    };
    if ctx.tcx().def_kind(trait_id) != DefKind::Trait
        || !ctx.tcx().trait_def(trait_id).safety.is_unsafe()
    {
        return false;
    }
    ctx.tcx()
        .infer_ctxt()
        .build(TypingMode::PostAnalysis)
        .type_implements_trait(trait_id, [ty], ParamEnv::empty())
        .must_apply_modulo_regions()
}

fn direct_managed_kind<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> Option<DirectManagedKind> {
    let TyKind::Adt(def, _) = ty.kind() else {
        return None;
    };
    if !crate::utilis::is_mycorrhiza_intrinsic(ctx.tcx(), def.did())
        || !is_managed_interop_type(ty, ctx)
    {
        return None;
    }
    let name = ctx.tcx().item_name(def.did());
    let kind = match name.as_str() {
        INTEROP_CLASS_TPE_NAME | INTEROP_GENERIC_TPE_NAME | INTEROP_ARR_TPE_NAME => {
            ManagedStorageKind::NakedReference
        }
        INTEROP_BYREF_TPE_NAME => ManagedStorageKind::ManagedByRef,
        INTEROP_STRUCT_TPE_NAME
        | INTEROP_GENERIC_STRUCT_TPE_NAME
        | INTEROP_TYPE_GENERIC_TPE_NAME
        | INTEROP_METHOD_GENERIC_TPE_NAME => ManagedStorageKind::OpaqueManagedValue,
        _ => return None,
    };
    Some(DirectManagedKind::Unsafe(kind))
}

/// A transparent wrapper over a direct CLR value is itself a direct CLR value. This lets wrappers
/// such as `DotNetTimeSpan` remain ordinary managed locals/arguments/returns, while
/// `storage_violation` still rejects them when nested unless they carry `NativeStorageSafe`.
fn effective_direct_managed_kind<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
    seen: &mut FxHashSet<Ty<'tcx>>,
) -> Option<DirectManagedKind> {
    let ty = ctx.monomorphize(ty);
    if let Some(kind) = direct_managed_kind(ty, ctx) {
        return Some(kind);
    }
    if !seen.insert(ty) {
        return None;
    }
    let TyKind::Adt(def, args) = ty.kind() else {
        return None;
    };
    if !def.repr().transparent() {
        return None;
    }
    let mut physical_fields = def.all_fields().filter_map(|field| {
        let field_ty = ctx.monomorphize(field.ty(ctx.tcx(), args).skip_normalization());
        (!ctx.layout_of(field_ty).is_zst()).then_some(field_ty)
    });
    let field = physical_fields.next()?;
    if physical_fields.next().is_some() {
        return None;
    }
    effective_direct_managed_kind(field, ctx, seen)
}

/// Returns the first GC-unsafe value nested in `ty`, if any.
///
/// Types carrying the audited `NativeStorageSafe` capability are terminal safe leaves: a generic
/// parameter may describe a value behind a GCHandle token rather than bytes embedded in the
/// wrapper. Other ADTs are inspected structurally. Only known out-of-line owners have their generic
/// arguments inspected in addition to fields; blanket generic traversal would incorrectly treat
/// `NonNull<Raw>`, `AtomicPtr<Raw>`, and `PhantomData<T>` as though they physically stored `T`.
#[must_use]
pub fn storage_violation<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> Option<ManagedStorageViolation> {
    storage_violation_inner(
        ctx.monomorphize(ty),
        ctx,
        "value",
        &mut FxHashSet::default(),
        false,
    )
}

fn storage_violation_inner<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
    path: &str,
    seen: &mut FxHashSet<Ty<'tcx>>,
    follow_references: bool,
) -> Option<ManagedStorageViolation> {
    let ty = ctx.monomorphize(ty);
    if is_native_storage_safe(ty, ctx) {
        return None;
    }
    match direct_managed_kind(ty, ctx) {
        Some(DirectManagedKind::Unsafe(kind)) => {
            return Some(ManagedStorageViolation {
                kind,
                path: path.to_owned(),
            });
        }
        None => {}
    }

    // `[Raw; 0]`, `(Raw, [u8; 0])`-style zero-storage shapes carry no bytes and cannot hide a CLR
    // ref. Check this only after the direct-leaf test: the fake Rust layout of a direct interop
    // marker is not authoritative for its CLR representation.
    if ty.is_sized(ctx.tcx(), TypingEnv::fully_monomorphized()) && ctx.layout_of(ty).is_zst() {
        return None;
    }
    // Recursive ownership graphs (`Node -> Option<Box<Node>>`) are ordinary finite values at
    // runtime. A repeated interned `Ty` has already had its direct kind and fields examined by the
    // active outer frame, so ending this branch is safe and avoids depth-limit false positives.
    if !seen.insert(ty) {
        return None;
    }

    match ty.kind() {
        TyKind::Adt(def, args) => {
            for (index, field) in def.all_fields().enumerate() {
                let field_ty = field.ty(ctx.tcx(), args).skip_normalization();
                if let Some(violation) = storage_violation_inner(
                    field_ty,
                    ctx,
                    &format!("{path}.field[{index}]"),
                    seen,
                    follow_references,
                ) {
                    return Some(violation);
                }
            }
            // Only out-of-line owners need their element type inspected beyond physical fields.
            // Blanket generic traversal incorrectly treats NonNull<Raw>, AtomicPtr<Raw>, and
            // PhantomData<T> identity wrappers as though they physically stored `T`.
            let owns_generic_storage = ctx.tcx().is_lang_item(def.did(), LangItem::OwnedBox)
                || ctx.tcx().is_diagnostic_item(sym::Vec, def.did());
            if owns_generic_storage {
                for (index, arg_ty) in args.iter().filter_map(|arg| arg.as_type()).enumerate() {
                    if let Some(violation) = storage_violation_inner(
                        arg_ty,
                        ctx,
                        &format!("{path}.owned[{index}]"),
                        seen,
                        follow_references,
                    ) {
                        return Some(violation);
                    }
                }
            }
            None
        }
        TyKind::Closure(_, args) => {
            args.as_closure()
                .upvar_tys()
                .iter()
                .enumerate()
                .find_map(|(index, field)| {
                    storage_violation_inner(
                        field,
                        ctx,
                        &format!("{path}.capture[{index}]"),
                        seen,
                        follow_references,
                    )
                })
        }
        TyKind::Coroutine(def_id, args) => {
            let args = args.as_coroutine();
            args.upvar_tys()
                .iter()
                .enumerate()
                .find_map(|(index, field)| {
                    storage_violation_inner(
                        field,
                        ctx,
                        &format!("{path}.capture[{index}]"),
                        seen,
                        follow_references,
                    )
                })
                .or_else(|| {
                    args.state_tys(*def_id, ctx.tcx())
                        .enumerate()
                        .find_map(|(variant, fields)| {
                            fields.into_iter().enumerate().find_map(|(index, field)| {
                                storage_violation_inner(
                                    field,
                                    ctx,
                                    &format!("{path}.state[{variant}].field[{index}]"),
                                    seen,
                                    follow_references,
                                )
                            })
                        })
                })
        }
        TyKind::CoroutineClosure(_, args) => args
            .as_coroutine_closure()
            .upvar_tys()
            .iter()
            .enumerate()
            .find_map(|(index, field)| {
                storage_violation_inner(
                    field,
                    ctx,
                    &format!("{path}.capture[{index}]"),
                    seen,
                    follow_references,
                )
            }),
        TyKind::Tuple(fields) => fields.iter().enumerate().find_map(|(index, field)| {
            storage_violation_inner(
                field,
                ctx,
                &format!("{path}.tuple[{index}]"),
                seen,
                follow_references,
            )
        }),
        TyKind::Array(field, _) | TyKind::Slice(field) => storage_violation_inner(
            *field,
            ctx,
            &format!("{path}.element"),
            seen,
            follow_references,
        ),
        // A Rust reference is itself native pointer storage. It is legal as a transient direct
        // argument/local when it points at one direct managed value (handled by
        // `local_storage_violation`), but nesting it in another Rust layout would persist a
        // managed byref without a CLR GC descriptor. Raw pointers remain terminal here so pure
        // NonNull/AtomicPtr/PhantomData identities are not mistaken for ownership; their actual
        // formation and external escape are checked separately below.
        TyKind::Ref(_, pointee, _) => storage_violation_inner(
            *pointee,
            ctx,
            &format!("{path}.referent"),
            seen,
            follow_references,
        ),
        TyKind::Pat(base, _) => storage_violation_inner(*base, ctx, path, seen, follow_references),
        // Unsafe binders change which lifetimes may be named, not the physical storage shape. Walk
        // the bound inner type; any residual bound *type* remains fail-closed in the arm below.
        TyKind::UnsafeBinder(bound_ty) => storage_violation_inner(
            *bound_ty.as_ref().skip_binder(),
            ctx,
            path,
            seen,
            follow_references,
        ),
        // A remaining alias/parameter is a lowering invariant violation. Fail closed rather than
        // allowing an unknown representation into Rust byte storage.
        TyKind::Alias(..)
        | TyKind::Param(..)
        | TyKind::Bound(..)
        | TyKind::Placeholder(..)
        | TyKind::Infer(..)
        | TyKind::Error(..)
        | TyKind::CoroutineWitness(..) => Some(ManagedStorageViolation {
            kind: ManagedStorageKind::OpaqueManagedValue,
            path: format!("{path} (unresolved type {ty:?})"),
        }),
        // Pointers do not physically embed their pointee. Copy intrinsics separately classify the
        // pointee being copied.
        _ => None,
    }
}

/// Direct naked values may exist transiently in method arguments, returns, and managed locals.
/// Anything that nests one inside a Rust layout is rejected.
#[must_use]
pub fn local_storage_violation<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> Option<ManagedStorageViolation> {
    let ty = ctx.monomorphize(ty);
    let direct_kind =
        effective_direct_managed_kind(ty, ctx, &mut FxHashSet::default()).or_else(|| {
            match ty.kind() {
                TyKind::Ref(_, pointee, _) => {
                    effective_direct_managed_kind(*pointee, ctx, &mut FxHashSet::default())
                }
                _ => None,
            }
        });
    match direct_kind {
        Some(DirectManagedKind::Unsafe(_)) => None,
        None => storage_violation(ty, ctx),
    }
}

/// A named static owns the allocation reached through a Rust reference initializer, so unlike an
/// ordinary transient `&T` local its pointee must also be native-storage-safe. Raw pointers remain
/// non-owning and are not followed.
#[must_use]
pub fn static_storage_violation<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> Option<ManagedStorageViolation> {
    storage_violation_inner(
        ctx.monomorphize(ty),
        ctx,
        "static",
        &mut FxHashSet::default(),
        true,
    )
}

/// Whether bytewise initialization/copying of `ty` would bypass a CLR GC/write barrier.
#[must_use]
pub fn is_bitwise_managed_unsafe<'tcx>(ty: Ty<'tcx>, ctx: &MethodCompileCtx<'tcx, '_>) -> bool {
    storage_violation(ty, ctx).is_some()
}

/// Whether `ty` is the exact stack-only raw managed-value marker used by the enum boundary.
/// Generic/transparent lookalikes do not qualify.
#[must_use]
pub fn is_raw_managed_struct<'tcx>(ty: Ty<'tcx>, ctx: &MethodCompileCtx<'tcx, '_>) -> bool {
    let ty = ctx.monomorphize(ty);
    let TyKind::Adt(def, _) = ty.kind() else {
        return false;
    };
    crate::utilis::is_mycorrhiza_intrinsic(ctx.tcx(), def.did())
        && is_managed_interop_type(ty, ctx)
        && ctx.tcx().item_name(def.did()).as_str() == INTEROP_STRUCT_TPE_NAME
}

fn unsupported_error<'tcx>(
    operation: &str,
    ty: Ty<'tcx>,
    violation: &ManagedStorageViolation,
) -> crate::codegen_error::CodegenError {
    crate::codegen_error::CodegenError::unsupported(
        "managed_reference_storage",
        format!(
            "{operation} would place/copy {} at {} through Rust-owned byte storage (type {ty:?}); use mycorrhiza's GCHandle-backed Class/GenericClass/ManagedRef/ManagedOption wrapper instead",
            violation.kind.description(),
            violation.path,
        ),
    )
}

/// Reject an indirect access whose selected Rust place would interpret native bytes as a naked
/// managed reference. This catches pointer casts (`slot.cast::<Raw>().write(raw)`), generic
/// allocation owners such as Rc/Arc/VecDeque, and custom owners at their monomorphized write/read
/// site without pretending every pointer/PhantomData generic parameter owns storage.
struct IndirectManagedPlaceVisitor<'ctx, 'tcx, 'asm> {
    ctx: &'ctx MethodCompileCtx<'tcx, 'asm>,
    violation: Option<(Ty<'tcx>, ManagedStorageViolation, PlaceContext)>,
    promoted_violation: Option<(Ty<'tcx>, ManagedStorageViolation)>,
}

impl<'tcx> Visitor<'tcx> for IndirectManagedPlaceVisitor<'_, 'tcx, '_> {
    fn visit_operand(&mut self, operand: &Operand<'tcx>, location: Location) {
        if self.promoted_violation.is_none()
            && let Operand::Constant(constant) = operand
        {
            let constant_ty = self.ctx.monomorphize(constant.const_.ty());
            if let Some((ty, violation)) = pointer_target_violation(constant_ty, self.ctx) {
                // A constant reference/raw pointer to a managed-storage-unsafe value owns or
                // exposes an anonymous promotion whose MonoItem has no queryable Rust type. Catch
                // it at the typed MIR use instead of treating the promotion as an untyped blob.
                self.promoted_violation = Some((ty, violation));
                return;
            }
        }
        self.super_operand(operand, location);
    }

    fn visit_place(&mut self, place: &Place<'tcx>, context: PlaceContext, location: Location) {
        if self.violation.is_some() {
            return;
        }
        let is_indirect = place
            .projection
            .iter()
            .any(|projection| matches!(projection, ProjectionElem::Deref));
        if is_indirect {
            let ty = self
                .ctx
                .monomorphize(place.ty(self.ctx.body(), self.ctx.tcx()).ty);
            if let Some(violation) = storage_violation(ty, self.ctx) {
                // `*shared_ref` of an exact direct managed value is the representation-preserving
                // Copy read used by raw marker Clone impls and value-type receiver shims. It stays
                // in a CLR evaluation/local slot and emits no native-byte load/store. Reads through
                // raw pointers, aggregate projections, moves, and every write remain rejected.
                let mut saw_deref = false;
                let only_shared_ref_derefs = place.iter_projections().all(|(base, projection)| {
                    if !matches!(projection, ProjectionElem::Deref) {
                        return true;
                    }
                    saw_deref = true;
                    matches!(
                        self.ctx
                            .monomorphize(base.ty(self.ctx.body(), self.ctx.tcx()).ty)
                            .kind(),
                        TyKind::Ref(..)
                    )
                });
                let shared_direct_copy =
                    matches!(
                        context,
                        PlaceContext::NonMutatingUse(NonMutatingUseContext::Copy)
                    ) && effective_direct_managed_kind(ty, self.ctx, &mut FxHashSet::default())
                        .is_some()
                        && saw_deref
                        && only_shared_ref_derefs;
                if shared_direct_copy {
                    self.super_place(place, context, location);
                    return;
                }
                self.violation = Some((ty, violation, context));
                return;
            }
        }
        self.super_place(place, context, location);
    }
}

fn pointer_target_violation<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> Option<(Ty<'tcx>, ManagedStorageViolation)> {
    let ty = ctx.monomorphize(ty);
    let pointee = match ty.kind() {
        TyKind::Ref(_, pointee, _) | TyKind::RawPtr(pointee, _) => *pointee,
        _ => return None,
    };
    storage_violation(pointee, ctx).map(|violation| (pointee, violation))
}

fn escape_violation<'tcx>(
    ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> Option<(Ty<'tcx>, ManagedStorageViolation)> {
    let ty = ctx.monomorphize(ty);
    storage_violation(ty, ctx)
        .map(|violation| (ty, violation))
        .or_else(|| pointer_target_violation(ty, ctx))
}

fn is_internal_rust_abi(abi: ExternAbi) -> bool {
    matches!(
        abi,
        ExternAbi::Rust | ExternAbi::RustCall | ExternAbi::RustCold | ExternAbi::RustTail
    )
}

#[derive(Default)]
struct SemanticPlaceUseCounter {
    counts: FxHashMap<Local, usize>,
}

struct RustCallSpreadUseProof {
    local: Local,
    saw_field_read: bool,
    valid: bool,
}

impl<'tcx> Visitor<'tcx> for RustCallSpreadUseProof {
    fn visit_place(&mut self, place: &Place<'tcx>, context: PlaceContext, location: Location) {
        if place.local == self.local {
            let is_direct_field = place.projection.len() == 1
                && matches!(place.projection[0], ProjectionElem::Field(..));
            let is_value_read = matches!(
                context,
                PlaceContext::NonMutatingUse(
                    NonMutatingUseContext::Copy | NonMutatingUseContext::Move
                )
            );
            if is_direct_field && is_value_read {
                self.saw_field_read = true;
            } else {
                self.valid = false;
            }
        }
        self.super_place(place, context, location);
    }
}

impl<'tcx> Visitor<'tcx> for SemanticPlaceUseCounter {
    fn visit_place(&mut self, place: &Place<'tcx>, context: PlaceContext, location: Location) {
        *self.counts.entry(place.local).or_default() += 1;
        self.super_place(place, context, location);
    }
}

fn operand_local(operand: &Operand<'_>) -> Option<Local> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
            Some(place.local)
        }
        Operand::Copy(_) | Operand::Move(_) | Operand::Constant(_) | Operand::RuntimeChecks(_) => {
            None
        }
    }
}

fn tuple_has_only_transient_managed_elements<'tcx>(
    tuple_ty: Ty<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> bool {
    let TyKind::Tuple(elements) = tuple_ty.kind() else {
        return false;
    };
    let mut has_direct_managed_value = false;
    let elements_are_transient_values = elements.iter().all(|element| {
        let element = ctx.monomorphize(element);
        if local_storage_violation(element, ctx).is_some() {
            return false;
        }
        has_direct_managed_value |= storage_violation(element, ctx).is_some();
        true
    });
    elements_are_transient_values && has_direct_managed_value
}

/// Prove which tuple locals are only compiler ABI packs for a RustCall invocation.
///
/// Rust lowers `Fn(A, B)::call` to an aggregate `(A, B)` MIR local followed by a RustCall. The
/// backend materializes that pack as a GC-described CLR tuple local, writes and reads it
/// field-by-field, and passes each field in its physical managed argument slot; it is never stored
/// or copied as opaque Rust bytes or through bulk memory. A blanket aggregate rejection therefore
/// false-positively rejects managed delegate trampolines.
///
/// The exemption is deliberately use-based rather than tied to Mycorrhiza names. A local qualifies
/// only if it has exactly two semantic place occurrences: one whole-local tuple construction and
/// one whole-local move/copy as the final operand of a RustCall. Any borrow, projection, duplicate
/// copy, return, drop, external call, address formation, or other persistence adds another use (or
/// fails one of those two shape checks) and keeps the normal fail-closed rejection.
fn transient_rust_call_tuple_locals<'tcx>(
    body: &rustc_middle::mir::Body<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> FxHashSet<Local> {
    let mut definitions = FxHashMap::<Local, usize>::default();
    let mut rust_call_uses = FxHashMap::<Local, usize>::default();
    let mut place_uses = SemanticPlaceUseCounter::default();

    for (block, data) in body.basic_blocks.iter_enumerated() {
        for (statement_index, statement) in data.statements.iter().enumerate() {
            let location = Location {
                block,
                statement_index,
            };
            place_uses.visit_statement(statement, location);

            let StatementKind::Assign(assignment) = &statement.kind else {
                continue;
            };
            let (destination, rvalue) = assignment.as_ref();
            let Rvalue::Aggregate(kind, _) = rvalue else {
                continue;
            };
            if !destination.projection.is_empty() || !matches!(kind.as_ref(), AggregateKind::Tuple)
            {
                continue;
            }
            let tuple_ty = ctx.monomorphize(body.local_decls[destination.local].ty);
            if tuple_has_only_transient_managed_elements(tuple_ty, ctx) {
                *definitions.entry(destination.local).or_default() += 1;
            }
        }

        let Some(terminator) = &data.terminator else {
            continue;
        };
        let location = Location {
            block,
            statement_index: data.statements.len(),
        };
        place_uses.visit_terminator(terminator, location);
        let (TerminatorKind::Call {
            func: function,
            args,
            ..
        }
        | TerminatorKind::TailCall {
            func: function,
            args,
            ..
        }) = &terminator.kind
        else {
            continue;
        };
        let function_ty = ctx.monomorphize(function.ty(body, ctx.tcx()));
        if !matches!(function_ty.kind(), TyKind::FnDef(..) | TyKind::FnPtr(..))
            || function_ty.fn_sig(ctx.tcx()).abi() != ExternAbi::RustCall
        {
            continue;
        }
        if let Some(local) = args
            .last()
            .and_then(|argument| operand_local(&argument.node))
        {
            *rust_call_uses.entry(local).or_default() += 1;
        }
    }

    let mut proven: FxHashSet<_> = definitions
        .into_iter()
        .filter_map(|(local, definition_count)| {
            (definition_count == 1
                && rust_call_uses.get(&local) == Some(&1)
                && place_uses.counts.get(&local) == Some(&2))
            .then_some(local)
        })
        .collect();

    // On the callee side, rustc represents the incoming RustCall arguments as `spread_arg`. The
    // method prologue reconstructs the same GC-described CLR tuple field-by-field. Permit that
    // synthetic argument only when MIR itself either reads direct fields or forwards the whole
    // value exactly once as the trailing operand of another RustCall (which is decomposed by the
    // same ABI plan). A borrow, nested projection, store, address, non-Rust call, or extra use
    // immediately invalidates it.
    if let Some(spread) = body.spread_arg {
        let instance_ty = ctx
            .instance()
            .ty(ctx.tcx(), TypingEnv::fully_monomorphized());
        let is_rust_call = matches!(instance_ty.kind(), TyKind::FnDef(..))
            && instance_ty.fn_sig(ctx.tcx()).abi() == ExternAbi::RustCall;
        let tuple_ty = ctx.monomorphize(body.local_decls[spread].ty);
        if is_rust_call && tuple_has_only_transient_managed_elements(tuple_ty, ctx) {
            let mut proof = RustCallSpreadUseProof {
                local: spread,
                saw_field_read: false,
                valid: true,
            };
            for (block, data) in body.basic_blocks.iter_enumerated() {
                for (statement_index, statement) in data.statements.iter().enumerate() {
                    proof.visit_statement(
                        statement,
                        Location {
                            block,
                            statement_index,
                        },
                    );
                }
                if let Some(terminator) = &data.terminator {
                    proof.visit_terminator(
                        terminator,
                        Location {
                            block,
                            statement_index: data.statements.len(),
                        },
                    );
                }
            }
            let forwards_once_to_rust_call = rust_call_uses.get(&spread) == Some(&1)
                && place_uses.counts.get(&spread) == Some(&1);
            if forwards_once_to_rust_call || (proof.valid && proof.saw_field_read) {
                proven.insert(spread);
            }
        }
    }

    proven
}

fn validate_intrinsic_storage<'tcx>(
    instance: Instance<'tcx>,
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> Result<(), crate::codegen_error::CodegenError> {
    let Some(intrinsic) = ctx.tcx().intrinsic(instance.def_id()) else {
        return Ok(());
    };
    let name = intrinsic.name;
    if !matches!(
        name.as_str(),
        "copy"
            | "copy_nonoverlapping"
            | "write_bytes"
            | "raw_eq"
            | "volatile_load"
            | "volatile_store"
            | "typed_swap_nonoverlapping"
    ) {
        return Ok(());
    }
    let Some(ty) = instance.args.get(0).and_then(|argument| argument.as_type()) else {
        return Ok(());
    };
    let ty = ctx.monomorphize(ty);
    if let Some(violation) = storage_violation(ty, ctx) {
        return Err(unsupported_error(
            &format!("intrinsic `{name}`"),
            ty,
            &violation,
        ));
    }
    Ok(())
}

/// Validate all storage and bulk-memory operations in a monomorphized MIR body before lowering it.
/// This runs before any CIL definitions are interned, so rejection is transactional and produces a
/// structured unsupported-feature error instead of a half-built method.
pub fn validate_body<'tcx>(
    ctx: &MethodCompileCtx<'tcx, '_>,
) -> Result<(), crate::codegen_error::CodegenError> {
    let body = ctx.body();
    let transient_rust_call_tuples = transient_rust_call_tuple_locals(body, ctx);

    // Some rustc intrinsics have a portable fallback MIR body. Validate the intrinsic instance
    // itself before walking that body so an unsafe T is rejected at the typed operation boundary,
    // rather than at an incidental pointer cast inside libcore's implementation.
    validate_intrinsic_storage(ctx.instance(), ctx)?;

    if let Some((ty, violation)) = pointer_target_violation(body.return_ty(), ctx) {
        return Err(unsupported_error(
            "function return pointer/reference escape",
            ty,
            &violation,
        ));
    }

    let instance_ty = ctx
        .instance()
        .ty(ctx.tcx(), TypingEnv::fully_monomorphized());
    // Closure/coroutine instances have Rust-internal calling conventions but `Ty::fn_sig` is
    // intentionally invalid for their source types. Only an actual FnDef can define an external
    // entrypoint.
    let instance_abi = match instance_ty.kind() {
        TyKind::FnDef(..) => instance_ty.fn_sig(ctx.tcx()).abi(),
        _ => ExternAbi::Rust,
    };
    // A private C-ABI definition can still be wholly managed (for example Mycorrhiza's delegate
    // trampolines, whose address is consumed by an exact backend magic call). Only an explicitly
    // named export from a final library artifact can be entered by native code. Call-site escape
    // checks below remain ABI-based, so arbitrary external/calli uses are not exempted.
    let instance_name = fn_name_for_instance(ctx.tcx(), ctx.instance());
    if !is_internal_rust_abi(instance_abi)
        && is_explicit_local_export(ctx.tcx(), ctx.instance(), &instance_name)
        && !crate::utilis::is_managed_export(ctx.tcx(), ctx.instance().def_id())
    {
        let return_ty = ctx.monomorphize(body.return_ty());
        if let Some((ty, violation)) = escape_violation(return_ty, ctx) {
            return Err(unsupported_error(
                &format!("external `{instance_abi}` function return"),
                ty,
                &violation,
            ));
        }
        for argument in body.args_iter() {
            let argument_ty = ctx.monomorphize(body.local_decls[argument].ty);
            if let Some((ty, violation)) = escape_violation(argument_ty, ctx) {
                return Err(unsupported_error(
                    &format!("external `{instance_abi}` function argument"),
                    ty,
                    &violation,
                ));
            }
        }
    }

    for (local, declaration) in body.local_decls.iter_enumerated() {
        let ty = ctx.monomorphize(declaration.ty);
        if let Some(violation) = local_storage_violation(ty, ctx) {
            if transient_rust_call_tuples.contains(&local) {
                continue;
            }
            return Err(unsupported_error(
                &format!("MIR local {local:?}"),
                ty,
                &violation,
            ));
        }
    }

    let mut place_visitor = IndirectManagedPlaceVisitor {
        ctx,
        violation: None,
        promoted_violation: None,
    };
    place_visitor.visit_body(body);
    if let Some((ty, violation)) = place_visitor.promoted_violation {
        return Err(unsupported_error(
            "constant/promotion pointer escape",
            ty,
            &violation,
        ));
    }
    if let Some((ty, violation, context)) = place_visitor.violation {
        return Err(unsupported_error(
            &format!("indirect MIR place access ({context:?})"),
            ty,
            &violation,
        ));
    }

    for block in body.basic_blocks.iter() {
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Intrinsic(intrinsic) => {
                    let NonDivergingIntrinsic::CopyNonOverlapping(copy) = intrinsic.as_ref() else {
                        continue;
                    };
                    let pointer_ty = ctx.monomorphize(copy.src.ty(body, ctx.tcx()));
                    let pointee = pointer_ty.builtin_deref(true).ok_or_else(|| {
                        crate::codegen_error::CodegenError::unsupported(
                            "managed_reference_storage",
                            format!(
                                "CopyNonOverlapping source is not a pointer after monomorphization: {pointer_ty:?}"
                            ),
                        )
                    })?;
                    if let Some(violation) = storage_violation(pointee, ctx) {
                        return Err(unsupported_error("CopyNonOverlapping", pointee, &violation));
                    }
                }
                StatementKind::Assign(assignment) => {
                    let rvalue = &assignment.as_ref().1;
                    if let Rvalue::RawPtr(_, place) = rvalue {
                        let pointee = ctx.monomorphize(place.ty(body, ctx.tcx()).ty);
                        if let Some(violation) = storage_violation(pointee, ctx) {
                            return Err(unsupported_error(
                                "raw pointer formation",
                                pointee,
                                &violation,
                            ));
                        }
                    }
                    if let Rvalue::Repeat(element, _) = rvalue {
                        let element_ty = ctx.monomorphize(element.ty(body, ctx.tcx()));
                        if let Some(violation) = storage_violation(element_ty, ctx) {
                            return Err(unsupported_error("array repeat", element_ty, &violation));
                        }
                    }
                    if let Rvalue::Cast(CastKind::Transmute, source, destination_ty) = rvalue {
                        let source_ty = ctx.monomorphize(source.ty(body, ctx.tcx()));
                        let destination_ty = ctx.monomorphize(*destination_ty);
                        if let Some(violation) = storage_violation(source_ty, ctx) {
                            return Err(unsupported_error(
                                "transmute source",
                                source_ty,
                                &violation,
                            ));
                        }
                        if let Some(violation) = storage_violation(destination_ty, ctx) {
                            return Err(unsupported_error(
                                "transmute destination",
                                destination_ty,
                                &violation,
                            ));
                        }
                    }
                    if let Rvalue::Cast(_, _, destination_ty) = rvalue
                        && let Some((ty, violation)) =
                            pointer_target_violation(*destination_ty, ctx)
                    {
                        return Err(unsupported_error(
                            "native pointer cast/formation",
                            ty,
                            &violation,
                        ));
                    }
                }
                _ => {}
            }
        }

        let Some(terminator) = &block.terminator else {
            continue;
        };
        let (TerminatorKind::Call {
            func: function,
            args: call_args,
            ..
        }
        | TerminatorKind::TailCall {
            func: function,
            args: call_args,
            ..
        }) = &terminator.kind
        else {
            continue;
        };
        let fn_ty = ctx.monomorphize(function.ty(body, ctx.tcx()));
        let signature = match fn_ty.kind() {
            TyKind::FnDef(..) | TyKind::FnPtr(..) => Some(fn_ty.fn_sig(ctx.tcx())),
            _ => None,
        };
        let abi = signature.map_or(ExternAbi::Rust, |signature| signature.abi());
        // Rust-ABI fn pointers stay within the same checked managed execution model; rejecting
        // them merely for being indirect would break ordinary `fn(Raw) -> Raw` adapters. Only an
        // ABI that can leave Rust/.NET requires the pointer/reference escape wall.
        if !is_internal_rust_abi(abi) {
            for argument in call_args {
                let argument_ty = ctx.monomorphize(argument.node.ty(body, ctx.tcx()));
                if let Some((ty, violation)) = escape_violation(argument_ty, ctx) {
                    return Err(unsupported_error(
                        &format!("external `{abi}` call argument"),
                        ty,
                        &violation,
                    ));
                }
            }
            let output = ctx.monomorphize(
                signature
                    .expect("an external callable must have a function signature")
                    .output()
                    .skip_binder(),
            );
            if let Some((ty, violation)) = escape_violation(output, ctx) {
                return Err(unsupported_error(
                    &format!("external `{abi}` call return"),
                    ty,
                    &violation,
                ));
            }
        }
        let TyKind::FnDef(def_id, args) = fn_ty.kind() else {
            continue;
        };
        let args = args
            .no_bound_vars()
            .expect("managed_reference_storage: function definition had bound generic arguments");
        let Some(instance) = Instance::try_resolve(
            ctx.tcx(),
            rustc_middle::ty::TypingEnv::fully_monomorphized(),
            *def_id,
            args,
        )
        .map_err(|error| {
            crate::codegen_error::CodegenError::unsupported(
                "managed_reference_storage",
                format!("could not resolve call while validating managed storage: {error:?}"),
            )
        })?
        else {
            continue;
        };
        validate_intrinsic_storage(instance, ctx)?;
    }
    Ok(())
}
