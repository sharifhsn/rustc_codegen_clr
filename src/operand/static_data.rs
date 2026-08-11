use crate::operand::constant::{get_vtable, static_ty};
use cilly::{
    Access, CILRoot, Const, FnSig, Int, Interned, MethodDef, MethodDefIdx, MethodRef,
    StaticFieldDesc, Type,
    cilnode::MethodKind,
    ir::{BasicBlock, CILNode},
};

type Root = Interned<cilly::ir::CILRoot>;
use crate::abi::AbiPlan;
pub use crate::fn_ctx::MethodCompileCtx;
use crate::fn_ctx::fn_name_for_instance;
use crate::r#type::{GetTypeExt, align_of, fixed_array};
use rustc_hir::def::DefKind;
use rustc_middle::{
    mir::interpret::{AllocId, Allocation, ConstAllocation, GlobalAlloc, InitChunk},
    ty::{Instance, List, Ty, TyCtxt, TypingEnv},
};
use rustc_span::def_id::DefId;

/// Stable semantic root for one anonymous allocation graph.
///
/// `AllocId` is retained only as an in-process graph key. It is deliberately never written into
/// emitted metadata: rustc assigns those IDs from a session-global counter whose value depends on
/// unrelated const-eval scheduling. The digest names the source-level owner/use, while a target
/// allocation's deterministic relocation path below distinguishes multiple mutable nodes with the
/// same bytes.
#[derive(Clone, Debug)]
pub(crate) struct AllocationOrigin {
    semantic_digest: String,
    graph_root: AllocId,
}

impl AllocationOrigin {
    pub(crate) fn new(
        graph_root: AllocId,
        domain: &str,
        owner: &str,
        details: impl IntoIterator<Item = String>,
    ) -> Self {
        let mut identity = crate::stable_identity::StableIdentity::new("allocation-origin");
        identity.write_str(domain);
        identity.write_str(owner);
        for detail in details {
            identity.write_str(&detail);
        }
        Self {
            semantic_digest: identity.finish_hex(),
            graph_root,
        }
    }

    pub(crate) fn for_current_instance(
        graph_root: AllocId,
        domain: &str,
        details: impl IntoIterator<Item = String>,
        ctx: &MethodCompileCtx<'_, '_>,
    ) -> Self {
        let owner = fn_name_for_instance(ctx.tcx(), ctx.instance());
        Self::new(graph_root, domain, &owner, details)
    }

    fn named_static(graph_root: AllocId, symbol: &str) -> Self {
        Self::new(graph_root, "named-static", symbol, std::iter::empty())
    }
}

/// A `static X: &[T] = &[..]` (and similar `&[..]`-valued statics) lifts its array
/// literal into an *anonymous nested static*: `DefKind::Static { nested: true, .. }`.
/// Such a static is **untyped** — its HIR owner node is `Node::Synthetic`, so
/// `tcx.type_of(def_id)` has no valid arm and ICEs with
/// `unexpected sort of node in type_of(): Synthetic`. We must therefore obtain its
/// storage size/align from the allocation itself, never from `type_of`. This mirrors
/// rustc's own `GlobalAlloc::size_and_align`, which branches on exactly this flag.
pub(crate) fn static_is_nested(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    matches!(tcx.def_kind(def_id), DefKind::Static { nested: true, .. })
}

/// Materialize a function allocation using an explicit ABI slot map.
///
/// An ordinary function keeps every source argument, including a leading ZST. A captureless
/// closure-to-fn coercion is the one supported shape that drops an argument here: exactly slot 0,
/// the closure receiver. Inferring that distinction by stripping all leading `Type::Void` values
/// corrupts perfectly legal pointers such as `fn((), u32)`.
pub(crate) fn reify_allocation_function<'tcx>(
    instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Interned<CILNode> {
    let abi = AbiPlan::from_instance(instance, ctx);
    let real_sig = abi.signature().clone();
    let function_name = fn_name_for_instance(ctx.tcx(), instance);
    let real_sig_idx = ctx.alloc_sig(real_sig.clone());
    let method = MethodRef::new(
        *ctx.main_module(),
        ctx.alloc_string(function_name),
        real_sig_idx,
        MethodKind::Static,
        vec![].into(),
    );
    let (target, ignored) = abi.bare_fn_pointer_signature();
    let target = if target == real_sig {
        real_sig_idx
    } else {
        ctx.alloc_sig(target)
    };
    ctx.reify_fnptr_with_ignored(method, target, &ignored)
}

/// Build the .NET storage `Type` for a static's backing field directly from its
/// evaluated allocation's real `len` + `align`, with no `type_of` query. Used for
/// nested/anonymous statics (whose `type_of` would ICE) and as the type-of-free
/// fallback. The shape mirrors the `add_allocation` Memory-arm blob: a fixed-size
/// array of the largest integer the alignment guarantees, sized to cover the bytes.
fn nested_static_blob_type(alloc: &Allocation, ctx: &mut MethodCompileCtx<'_, '_>) -> Type {
    let align = alloc.align.bytes().max(1);
    let elem = match align {
        ..1 => Int::U8,
        ..2 => Int::U16,
        ..4 => Int::U32,
        _ => Int::U64,
    };
    let elem_size = elem.size().unwrap_or(8) as u64;
    let len = alloc.len() as u64;
    if len == 0 {
        return Type::Void;
    }
    let blob_arr = fixed_array(
        ctx,
        Type::Int(elem),
        len.div_ceil(elem_size),
        len.next_multiple_of(elem_size),
        len.next_multiple_of(elem_size),
        elem_size,
    );
    Type::ClassRef(blob_arr)
}

fn reserve_static_field<'tcx>(
    def_id: DefId,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> (Interned<CILNode>, bool, String, bool, ConstAllocation<'tcx>) {
    let main_module_id = ctx.main_module();
    let attrs = ctx.tcx().codegen_fn_attrs(def_id);

    let thread_local = attrs
        .flags
        .contains(rustc_middle::middle::codegen_fn_attrs::CodegenFnAttrFlags::THREAD_LOCAL);
    // An anonymous nested static (`static X: &[T] = &[..]` lifts its `&[..]` into one)
    // is untyped: `tcx.type_of` has no arm for its `Node::Synthetic` owner and ICEs.
    // For that case derive the backing-field type from the evaluated allocation's real
    // len+align (the type_of-free path rustc itself uses in `GlobalAlloc::size_and_align`),
    // never from `static_ty`. Top-level named statics keep the exact existing behaviour.
    let nested = static_is_nested(ctx.tcx(), def_id);
    // `eval_static_initializer` is valid for nested statics too (it asserts only
    // `is_static`, and we need the alloc for the type below as well as for Phase 2).
    let alloc = ctx.tcx().eval_static_initializer(def_id).unwrap();
    let tpe = if nested {
        nested_static_blob_type(&alloc.0, ctx)
    } else {
        let ty = static_ty(def_id, ctx.tcx());
        if let Some(violation) = crate::managed_storage::static_storage_violation(ty, ctx) {
            ctx.tcx().dcx().span_fatal(
                ctx.tcx().def_span(def_id),
                format!(
                    "managed_reference_storage: static stores {} at {} through native Rust bytes (type {ty:?}); use a GCHandle-backed wrapper",
                    violation.kind.description(),
                    violation.path,
                ),
            );
        }
        assert!(ty.is_sized(ctx.tcx(), TypingEnv::fully_monomorphized()));
        let tpe = ctx.type_from_cache(ty);
        // Cross-check the alloc's align against the type's (named statics only; a nested
        // static has no type to compare and is the whole reason this branch is split).
        assert_eq!(alloc.0.align.bytes().max(1), align_of(ty, ctx.tcx()));
        tpe
    };
    // A named static has one exact linkage symbol and no instantiating-crate suffix; preserve
    // rustc's authority here, including explicit `#[no_mangle]`/`#[export_name]` contracts.
    let symbol: String = ctx
        .tcx()
        .symbol_name(Instance::new_raw(def_id, List::empty()))
        .to_string();

    // Reserve the field before lowering its initializer/provenance. Self- and mutually
    // referential statics then resolve through `static_address` to declaration-only fields instead
    // of recursively synthesizing another initializer. The field list is also the shard-local memo:
    // repeated references agree on one descriptor while the owning MonoItem emits the sole body.
    let name = ctx.alloc_string(symbol.clone());
    let present = ctx.class_mut(main_module_id).has_static_field(name, tpe);
    let sfld = ctx.add_static(
        tpe,
        symbol.clone(),
        thread_local,
        main_module_id,
        None,
        false,
    );
    let ptr = ctx.alloc_node(CILNode::LdStaticFieldAddress(sfld));
    let ptr = ctx.cast_ptr(ptr, Int::U8);
    (ptr, present, symbol, thread_local, alloc)
}

/// Declares a static's backing field and returns its address without also defining an initializer.
///
/// Function shards may reference a static before the static's own `MonoItem` shard is linked. They
/// must agree on the field identity, but only the owning `MonoItem` emits its initializer method;
/// otherwise strict shard preflight correctly sees two competing real method definitions.
pub(crate) fn static_address(
    def_id: DefId,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Interned<CILNode> {
    reserve_static_field(def_id, ctx).0
}

pub fn add_static(def_id: DefId, ctx: &mut MethodCompileCtx<'_, '_>) -> Interned<CILNode> {
    let (ptr, present, symbol, thread_local, alloc) = reserve_static_field(def_id, ctx);
    if present {
        // The field (and its initializer) were registered by an earlier call in this shard;
        // return the same U8-ptr address node without recursing again.
        return ptr;
    }

    // The owning static MonoItem builds and registers the initializer exactly once in this shard.
    // The allocation was evaluated above because it determines the field type for nested statics
    // and is cross-checked for named ones.
    let root_alloc_id = ctx.tcx().reserve_and_set_memory_alloc(alloc);
    let origin = AllocationOrigin::named_static(root_alloc_id, &symbol);
    let initialzer = allocation_initializer_method(&alloc.0, &symbol, &origin, ctx, ptr, true);
    let root = ctx.alloc_root(cilly::CILRoot::call(*initialzer, []));

    if thread_local {
        ctx.add_tcctor(&[root]);
    } else {
        ctx.add_cctor(&[root]);
    }

    ptr
}

/// Returns the allocation bytes that may participate in emitted output or stable identity.
///
/// rustc deliberately leaves the backing bytes of an `Uninit` range unspecified; the raw
/// inspection API exposes whatever happens to occupy those slots in this compiler process. Rust
/// cannot observe that content without undefined behavior, so copying or hashing it only leaks
/// allocation history into generated artifacts. Initialized ranges (including pointer addends) are
/// preserved exactly, while every uninitialized byte is represented as zero. The digest below also
/// records the init-mask runs, so initialized zeroes and uninitialized storage remain distinct
/// semantic inputs even though their canonical payload byte is the same.
fn canonical_allocation_bytes(allocation: &Allocation) -> Vec<u8> {
    let raw = allocation.inspect_with_uninit_and_ptr_outside_interpreter(0..allocation.len());
    let mut canonical = vec![0; raw.len()];
    let range = rustc_const_eval::interpret::AllocRange {
        start: rustc_abi::Size::ZERO,
        size: allocation.size(),
    };
    for chunk in allocation.init_mask().range_as_init_chunks(range) {
        let InitChunk::Init(range) = chunk else {
            continue;
        };
        let start = range.start.bytes_usize();
        let end = range.end.bytes_usize();
        canonical[start..end].copy_from_slice(&raw[start..end]);
    }
    canonical
}

/// Stable semantic digest of an allocation graph. Graph-local backreferences preserve cycles and
/// sharing without ever encoding rustc's session-local `AllocId` values for immutable data.
fn allocation_graph_digest(
    root_id: AllocId,
    allocation: &Allocation,
    field_type: Type,
    ctx: &MethodCompileCtx<'_, '_>,
) -> String {
    fn write_memory(
        identity: &mut crate::stable_identity::StableIdentity,
        allocation: &Allocation,
        seen: &mut std::collections::HashMap<AllocId, u64>,
        ctx: &MethodCompileCtx<'_, '_>,
    ) {
        identity.write_str("memory");
        identity.write_u64(allocation.align.bytes());
        identity.write_u64(allocation.len() as u64);
        identity.write_str(match allocation.mutability {
            rustc_middle::mir::Mutability::Not => "immutable",
            rustc_middle::mir::Mutability::Mut => "mutable",
        });
        identity.write_bytes(&canonical_allocation_bytes(allocation));
        let init_chunks: Vec<_> = allocation
            .init_mask()
            .range_as_init_chunks(rustc_const_eval::interpret::AllocRange {
                start: rustc_abi::Size::ZERO,
                size: allocation.size(),
            })
            .collect();
        identity.write_u64(init_chunks.len() as u64);
        for chunk in init_chunks {
            identity.write_str(if chunk.is_init() { "init" } else { "uninit" });
            let range = chunk.range();
            identity.write_u64(range.start.bytes());
            identity.write_u64(range.end.bytes());
        }

        let mut relocations: Vec<_> = allocation.provenance().ptrs().iter().collect();
        relocations.sort_by_key(|(offset, _)| offset.bytes());
        identity.write_u64(relocations.len() as u64);
        for (offset, provenance) in relocations {
            identity.write_u64(offset.bytes());
            write_global(identity, provenance.alloc_id(), seen, ctx);
        }
    }

    fn write_global(
        identity: &mut crate::stable_identity::StableIdentity,
        alloc_id: AllocId,
        seen: &mut std::collections::HashMap<AllocId, u64>,
        ctx: &MethodCompileCtx<'_, '_>,
    ) {
        if let Some(index) = seen.get(&alloc_id) {
            identity.write_str("backref");
            identity.write_u64(*index);
            return;
        }
        let index = seen.len() as u64;
        seen.insert(alloc_id, index);
        identity.write_str("node");
        identity.write_u64(index);
        match ctx.tcx().global_alloc(alloc_id) {
            GlobalAlloc::Function { instance } => {
                identity.write_str("function");
                identity.write_str(&fn_name_for_instance(ctx.tcx(), instance));
            }
            GlobalAlloc::VTable(ty, predicates) => {
                identity.write_str("vtable");
                identity.write_str(&format!("{:032x}", ctx.tcx().type_id_hash(ty)));
                // A concrete type can implement multiple traits, whose vtable pointers are not
                // interchangeable. Hash the complete existential predicate list by embedding it
                // in a `dyn ... + 'static` type; hashing only `ty` aliases (for example)
                // `&S as &dyn TraitA` with `&S as &dyn TraitB` and can make immutable-allocation
                // dedup reuse the first vtable for both constants.
                let dyn_ty = Ty::new_dynamic(ctx.tcx(), predicates, ctx.tcx().lifetimes.re_static);
                identity.write_str(&format!("{:032x}", ctx.tcx().type_id_hash(dyn_ty)));
            }
            GlobalAlloc::Static(def_id) => {
                identity.write_str("static");
                let instance = Instance::new_raw(def_id, List::empty());
                // Statics have no instantiating-crate suffix: unlike callable MonoItems, they are
                // neither generic nor `GloballyShared { may_conflict: true }`. Keep rustc's exact
                // symbol authority here so `#[no_mangle]`/`#[export_name]` and native linkage names
                // remain byte-for-byte intact.
                identity.write_str(&ctx.tcx().symbol_name(instance).to_string());
            }
            GlobalAlloc::Memory(memory) => write_memory(identity, memory.inner(), seen, ctx),
            GlobalAlloc::TypeId { ty } => {
                identity.write_str("type-id");
                identity.write_str(&format!("{:032x}", ctx.tcx().type_id_hash(ty)));
            }
        }
    }

    let mut identity = crate::stable_identity::StableIdentity::new("immutable-allocation-graph");
    let mut seen = std::collections::HashMap::new();
    seen.insert(root_id, 0);
    identity.write_str("node");
    identity.write_u64(0);
    write_memory(&mut identity, allocation, &mut seen, ctx);
    identity.write_str("storage-type");
    crate::stable_identity::write_type(&mut identity, field_type, ctx.asm());
    identity.finish_hex()
}

/// Returns the first path found by a deterministic depth-first walk from `root` to `target`.
/// Relocations are ordered by byte offset, and `AllocId` participates only in the visited set and
/// equality checks. Consequently the returned offsets remain stable even when rustc assigns every
/// node a different session-local ID.
fn allocation_relocation_path(
    root: AllocId,
    target: AllocId,
    ctx: &MethodCompileCtx<'_, '_>,
) -> Option<Vec<u64>> {
    fn visit(
        current: AllocId,
        target: AllocId,
        path: &mut Vec<u64>,
        seen: &mut std::collections::HashSet<AllocId>,
        ctx: &MethodCompileCtx<'_, '_>,
    ) -> Option<Vec<u64>> {
        if current == target {
            return Some(path.clone());
        }
        if !seen.insert(current) {
            return None;
        }
        let GlobalAlloc::Memory(memory) = ctx.tcx().global_alloc(current) else {
            return None;
        };
        let mut relocations: Vec<_> = memory.inner().provenance().ptrs().iter().collect();
        relocations.sort_by_key(|(offset, _)| offset.bytes());
        for (offset, provenance) in relocations {
            path.push(offset.bytes());
            if let Some(found) = visit(provenance.alloc_id(), target, path, seen, ctx) {
                return Some(found);
            }
            path.pop();
        }
        None
    }

    visit(
        root,
        target,
        &mut Vec::new(),
        &mut std::collections::HashSet::new(),
        ctx,
    )
}

fn mutable_allocation_name_from_parts(
    semantic_origin: &str,
    relocation_path: &[u64],
    graph_digest: &str,
) -> String {
    let mut identity = crate::stable_identity::StableIdentity::new("mutable-allocation");
    identity.write_str(semantic_origin);
    identity.write_u64(relocation_path.len() as u64);
    for offset in relocation_path {
        identity.write_u64(*offset);
    }
    identity.write_str(graph_digest);
    format!("mut_{}", identity.finish_hex())
}

fn mutable_allocation_name(
    alloc_id: AllocId,
    origin: &AllocationOrigin,
    graph_digest: &str,
    ctx: &MethodCompileCtx<'_, '_>,
) -> String {
    let path = allocation_relocation_path(origin.graph_root, alloc_id, ctx).unwrap_or_else(|| {
        ctx.tcx().dcx().span_fatal(
            ctx.span(),
            format!(
                "UnsupportedFeature(allocation_origin): anonymous mutable allocation {alloc_id} \
                 is not reachable from its semantic allocation root {}",
                origin.graph_root,
            ),
        )
    });
    mutable_allocation_name_from_parts(&origin.semantic_digest, &path, graph_digest)
}

/// Returns a pointer to the backing buffer of a const-allocation static, rounded up to `align` at
/// runtime when the allocation is over-aligned (`over_aligned == align > elem_size`).
///
/// .NET does not guarantee >8-byte alignment for value-type *static* fields, so an over-aligned
/// const allocation (a `#[repr(align(N>8))]` value, or the `const_allocate(_, 64)` metadata buffer
/// behind `ThinBox::<dyn>::new_unsize_zst`) has its field over-allocated by `align` bytes (see
/// `add_allocation`) and the usable buffer base is `align_up(field_addr, align)`. Computing it here
/// — once, deterministically from the fixed static address — keeps the initializer's writes and
/// every consumer's reads in agreement. For the common `align <= elem_size` case the field address
/// is already adequately aligned, so it is returned unchanged.
fn aligned_static_buf(
    ctx: &mut MethodCompileCtx<'_, '_>,
    field_desc: StaticFieldDesc,
    align: u64,
    over_aligned: bool,
) -> Interned<CILNode> {
    let base = ctx.static_addr(field_desc);
    if !over_aligned {
        return base;
    }
    // align_up(p, a) == (p + (a - 1)) & !(a - 1), computed in usize then cast back to *u8.
    let base_int = ctx.cast_ptr_to(base, Type::Int(Int::USize));
    let added = ctx.biop(base_int, Const::USize(align - 1), cilly::BinOp::Add);
    let aligned = ctx.biop(added, Const::USize(!(align - 1)), cilly::BinOp::And);
    let u8_ptr = ctx.nptr(Int::U8);
    ctx.cast_ptr_to(aligned, u8_ptr)
}

/// Chooses the inline field shape that backs one raw allocation.
///
/// The CLR guarantees at most the natural alignment of the largest scalar field we use (`u64`).
/// For a stricter Rust alignment, reserve one extra alignment quantum; [`aligned_static_buf`]
/// selects an aligned interior address at runtime. Keeping this calculation pure makes the
/// over-alignment guarantee independently testable without a rustc context.
fn static_storage_shape(len: u64, align: u64) -> (Int, u64, bool) {
    let elem = match align {
        ..=1 => Int::U8,
        ..=2 => Int::U16,
        ..=4 => Int::U32,
        _ => Int::U64,
    };
    let elem_size = u64::from(elem.size().unwrap_or(8));
    let over_aligned = align > elem_size;
    let padding = if over_aligned { align } else { 0 };
    let storage_size = (len + padding).next_multiple_of(elem_size);
    (elem, storage_size, over_aligned)
}

#[cfg(test)]
mod alignment_tests {
    use super::{
        canonical_allocation_bytes, mutable_allocation_name_from_parts, static_storage_shape,
    };
    use cilly::Int;

    #[test]
    fn naturally_aligned_storage_needs_no_runtime_padding() {
        assert_eq!(static_storage_shape(17, 8), (Int::U64, 24, false));
    }

    #[test]
    fn over_aligned_storage_reserves_room_for_an_aligned_interior_pointer() {
        assert_eq!(static_storage_shape(32, 16), (Int::U64, 48, true));
        assert_eq!(static_storage_shape(1, 64), (Int::U64, 72, true));
    }

    #[test]
    fn mutable_names_are_repeatable_but_never_merge_distinct_graph_paths() {
        let first = mutable_allocation_name_from_parts("owner", &[8, 24], "graph");
        let repeated = mutable_allocation_name_from_parts("owner", &[8, 24], "graph");
        let sibling = mutable_allocation_name_from_parts("owner", &[8, 32], "graph");
        let other_owner = mutable_allocation_name_from_parts("other", &[8, 24], "graph");
        assert_eq!(first, repeated);
        assert_ne!(first, sibling);
        assert_ne!(first, other_owner);
        assert!(first.starts_with("mut_"));
        assert_eq!(first.len(), "mut_".len() + 64);
    }

    #[test]
    fn uninitialized_payload_bytes_are_canonicalized() {
        let mut allocation: rustc_middle::mir::interpret::Allocation =
            rustc_middle::mir::interpret::Allocation::from_bytes(
                &[1_u8, 2, 3, 4],
                rustc_abi::Align::ONE,
                rustc_middle::mir::Mutability::Mut,
                (),
            );
        allocation.write_uninit(
            &rustc_abi::TargetDataLayout::default(),
            rustc_const_eval::interpret::AllocRange {
                start: rustc_abi::Size::from_bytes(1),
                size: rustc_abi::Size::from_bytes(2),
            },
        );

        assert_eq!(canonical_allocation_bytes(&allocation), [1, 0, 0, 4]);
    }
}

/// Adds a static field and initialized for allocation represented by `alloc_id`.
///
/// Every [`GlobalAlloc`] variant is self-describing here: memory supplies its byte length and
/// alignment, statics supply their `DefId`, and function/vtable/type-id allocations have dedicated
/// lowering. A use-site type hint would be both redundant and unsafe for interior pointers, so this
/// API intentionally accepts only the allocation identity.
pub fn add_allocation(
    alloc_id: AllocId,
    origin: &AllocationOrigin,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Interned<CILNode> {
    let main_module_id = ctx.main_module();
    let const_alloc = match ctx.tcx().global_alloc(alloc_id) {
        GlobalAlloc::Memory(alloc) => alloc,
        GlobalAlloc::Static(def_id) => return static_address(def_id, ctx),
        GlobalAlloc::VTable(..) => {
            // Resolve the symbolic VTable alloc into the real vtable blob exactly as
            // `load_scalar_ptr`'s VTable arm (constant.rs) does. `get_vtable` queries
            // `tcx.vtable_allocation`, which returns a *separate* `GlobalAlloc::Memory`
            // alloc holding the actual vtable bytes (drop-glue/size/align/method ptrs);
            // its Memory arm materializes that blob and its reloc loop patches the method
            // pointers. The returned node is the ADDRESS of that blob = the correct vtable
            // pointer. Previously this arm registered an UNINITIALIZED null `v_{id}` static
            // and returned its (null) *value*, silently null-ing the vtable field of any
            // `static OBJ: &dyn T = &S;` and faulting at first virtual dispatch. Delegating
            // to the shared `get_vtable` eliminates that drift (one resolver, both paths).
            let global_alloc = ctx.tcx().global_alloc(alloc_id);
            let (ty, polyref) = global_alloc.unwrap_vtable();
            return get_vtable(
                ctx,
                ty,
                polyref.map(|principal| ctx.tcx().instantiate_bound_regions_with_erased(principal)),
            );
        }
        GlobalAlloc::Function { instance } => {
            // Defensive: `allocation_initializer_method`'s Function-provenance branch
            // intercepts function relocations before they reach here, so for reloc-walking
            // this arm is dead. But `add_allocation` is also a public entry point, so resolve
            // a function alloc to a real fn-ptr (mirroring `load_scalar_ptr`'s Function arm in
            // constant.rs) instead of returning a null `f_{id}` static, closing the latent
            // null for any direct `add_allocation(Function)` caller.
            return reify_allocation_function(instance, ctx);
        }
        // A `TypeId` alloc has no backing memory: the pointer's *offset* is the
        // type-id hash fragment and equality only requires it be self-consistent.
        // Use a zero base, so the materialized pointer value equals the offset
        // (the hash fragment). In practice the reloc loop in
        // `allocation_initializer_method` short-circuits the TypeId case before
        // reaching here, but keep this for any other caller.
        GlobalAlloc::TypeId { .. } => return ctx.alloc_node(Const::USize(0)),
    };

    let const_alloc = const_alloc.inner();

    let bytes = canonical_allocation_bytes(const_alloc);
    let align = const_alloc.align.bytes().max(1);
    if const_alloc.len() == 0 {
        return ctx.alloc_node(Const::USize(align));
    }
    // Check if const literal can be used
    if const_alloc.provenance().ptrs().is_empty() && align <= 1 {
        return ctx.bytebuffer(&bytes, Int::U8);
    }
    match (align, bytes.len()) {
        _ => {
            // The initializer `cpblk`s the full allocation, so derive storage exclusively from
            // the allocation's own length/alignment. Use-site pointer types routinely describe a
            // subobject rather than this backing blob and must never participate in sizing it.
            let len = const_alloc.len() as u64;
            // .NET does NOT guarantee >8-byte alignment for a value-type *static* field (a
            // `[FieldOffset]`/classlayout `.pack` controls *instance* layout, not where the runtime
            // places the static's storage), so an OVER-aligned const allocation — e.g. a
            // `#[repr(align(64))]` value, or the `const_allocate(_, 64)` metadata buffer that
            // `ThinBox::<dyn>::new_unsize_zst` const-makes-global for a 64-aligned ZST — cannot rely
            // on the field landing 64-aligned (it lands ~8-aligned, silently corrupting any pointer
            // round-trip that asserts alignment — the ThinBox `verify_aligned` 32/8-vs-64 failure).
            // Fix it at RUNTIME: when `align > elem_size`, over-allocate the field by `align` bytes
            // and return an interior pointer rounded up to `align` (see `aligned_static_buf`). For
            // the overwhelmingly common `align <= 8` case nothing changes — no padding, no rounding.
            let (elem, arr_size, over_aligned) = static_storage_shape(len, align);
            let elem_size = u64::from(elem.size().unwrap_or(8));
            let blob_arr = fixed_array(
                ctx,
                Type::Int(elem),
                arr_size / elem_size,
                arr_size,
                arr_size,
                elem_size,
            );
            let field_tpe = Type::ClassRef(blob_arr);
            // Content-based dedup of READ-ONLY (immutable) allocations: identical immutable
            // allocations must share ONE backing static so `ptr::eq` on two references to the same
            // promoted const holds — most visibly `Waker::will_wake`, which compares the addresses
            // of two `RawWakerVTable` promotions (the `Waker::from`/`clone_waker` sites get distinct
            // `AllocId`s for the same const). Native does this via LLVM merging identical read-only
            // `unnamed_addr` globals; Rust permits it (const/promoted addresses are NOT guaranteed
            // distinct). Naming a read-only alloc by its CONTENT (bytes + align + len + relocation
            // targets) instead of its `AllocId` lets the linker's merge-by-name collapse the
            // duplicates. The relocation targets are part of the fingerprint so two byte-identical
            // allocations pointing at DIFFERENT functions/statics never wrongly merge. MUTABLE
            // statics must stay distinct, so they keep the unique `AllocId` in the name.
            let graph_digest = allocation_graph_digest(alloc_id, const_alloc, field_tpe, ctx);
            let alloc_name = if const_alloc.mutability == rustc_middle::mir::Mutability::Not {
                format!("ro_{graph_digest}")
            } else {
                // Mutable allocations must remain distinct even when their bytes are identical.
                // Their stable source owner plus relocation path provides that identity; rustc's
                // session-local `AllocId` is used only to discover the path and is never hashed.
                mutable_allocation_name(alloc_id, origin, &graph_digest, ctx)
            };
            let name = ctx.alloc_string(alloc_name.clone());
            let field_desc = StaticFieldDesc::new(*ctx.main_module(), name, field_tpe);
            // Currently, all static fields are in one module. Consider spliting them up.

            let main_module = ctx.class_mut(main_module_id);

            if main_module.has_static_field(name, field_desc.tpe()) {
                return aligned_static_buf(ctx, field_desc, align, over_aligned);
            }
            ctx.add_static(field_tpe, &*alloc_name, false, main_module_id, None, false);

            // The runtime-aligned interior pointer is the canonical buffer base: the initializer
            // writes (and patches relocations) at it, and every consumer reads from it, so the two
            // always agree. `Interned` is `Copy`, so we reuse `buf` for both the init and the return.
            let buf = aligned_static_buf(ctx, field_desc, align, over_aligned);
            let ptr = ctx.cast_ptr(buf, Int::U8);

            let initialzer: MethodDefIdx = allocation_initializer_method(
                const_alloc,
                &alloc_name,
                origin,
                ctx,
                ptr.into(),
                true,
            );

            // Calls the static initialzer, and sets the static field to the returned pointer.
            let root = ctx.alloc_root(cilly::CILRoot::call(*initialzer, []));
            ctx.add_cctor(&[root]);

            buf
        }
    }
}
fn allocation_initializer_method(
    const_allocation: &Allocation,
    name: &str,
    origin: &AllocationOrigin,
    ctx: &mut MethodCompileCtx<'_, '_>,
    ptr: Interned<CILNode>,
    void_ret: bool,
) -> MethodDefIdx {
    let bytes = canonical_allocation_bytes(const_allocation);
    let ptrs = const_allocation.provenance().ptrs();
    let mut trees: Vec<Root> = Vec::new();

    // Emit the static-initialization roots directly.
    // STLoc(0, ptr)
    trees.push(ctx.alloc_root(CILRoot::StLoc(0, ptr)));
    // CpBlk(dst = LdLoc(0), src = bytebuffer, len = const)
    {
        let dst = ctx.alloc_node(CILNode::LdLoc(0));
        let src = ctx.bytebuffer(&bytes, Int::U8);
        let len = ctx.alloc_node(Const::USize(bytes.len() as u64));
        let cpblk = ctx.cp_blk(dst, src, len);
        trees.push(cpblk);
    }

    if !ptrs.is_empty() {
        for (offset, prov) in ptrs.iter() {
            let offset = u32::try_from(offset.bytes_usize()).unwrap();
            // Check if this allocation is a function
            let target_alloc = ctx.tcx().global_alloc(prov.alloc_id());
            // `TypeId` provenance is opaque and has no real address: the pointer's
            // offset (already written into the raw bytes copied above by `CpBlk`) is
            // a segment of the 128-bit type-id hash. Leaving the raw bytes in place
            // (base address 0 + offset == hash fragment) keeps `TypeId::of::<T>()`
            // self-consistent for equality, which is all the program can observe.
            if matches!(target_alloc, GlobalAlloc::TypeId { .. }) {
                continue;
            }
            if let GlobalAlloc::Function {
                instance: finstance,
            } = target_alloc
            {
                // If it is a function, patch its pointer up.
                let mut ctx = MethodCompileCtx::new(ctx.tcx(), None, finstance, ctx);
                // addr = (LdLoc(0) + offset) cast to *usize
                let ld_loc = ctx.alloc_node(CILNode::LdLoc(0));
                let off = ctx.alloc_node(Const::USize(offset.into()));
                let addr = ctx.biop(ld_loc, off, cilly::BinOp::Add);
                let usize_ptr = ctx.nptr(Type::Int(Int::USize));
                let addr = ctx.cast_ptr_to(addr, usize_ptr);
                // val = LdFtn(adapter-or-method) cast to usize
                let ftn = reify_allocation_function(finstance, &mut ctx);
                let val = ctx.cast_ptr_to(ftn, Type::Int(Int::USize));
                trees.push(ctx.alloc_root(CILRoot::StInd(Box::new((
                    addr,
                    val,
                    Type::Int(Int::ISize),
                    false,
                )))));
            } else {
                let ptr_alloc = add_allocation(prov.alloc_id(), origin, ctx);

                // A provenance pointer embedded in this static stores its offset INTO the
                // target allocation inline in the raw bytes (already copied by the `CpBlk`
                // above); the relocation itself only names the target's BASE. Recover that
                // inline addend and add it back — otherwise a `&OTHER_STATIC.field`
                // reference (or any interior pointer into another static) collapses to the
                // START of the target allocation. This was the encoding_rs single-byte-table
                // miscompile: every `&SINGLE_BYTE_DATA.<encoding>` read the FIRST field
                // (`ibm866`), so windows-1252 decoded as IBM866. Mirrors the scalar-pointer
                // offset handling in `constant.rs` (`create_const_from_data`).
                let target_layout = ctx.target_layout();
                let ptr_size = target_layout.pointer_bytes();
                let addend = {
                    let start = offset as usize;
                    target_layout.decode_pointer(&bytes[start..start + ptr_size])
                };

                // addr = (LdLoc(0) + offset) cast to *usize
                let ld_loc = ctx.alloc_node(CILNode::LdLoc(0));
                let off = ctx.alloc_node(Const::USize(offset.into()));
                let addr = ctx.biop(ld_loc, off, cilly::BinOp::Add);
                let usize_ptr = ctx.nptr(Type::Int(Int::USize));
                let addr = ctx.cast_ptr_to(addr, usize_ptr);
                // val = (ptr_alloc base + inline addend) cast to usize
                let val = ctx.cast_ptr_to(ptr_alloc, Type::Int(Int::USize));
                let val = if addend != 0 {
                    let addend = ctx.alloc_node(Const::USize(addend));
                    ctx.biop(val, addend, cilly::BinOp::Add)
                } else {
                    val
                };
                trees.push(ctx.alloc_root(CILRoot::StInd(Box::new((
                    addr,
                    val,
                    Type::Int(Int::ISize),
                    false,
                )))));
            }
        }
    }
    if void_ret {
        trees.push(ctx.alloc_root(CILRoot::VoidRet));
    } else {
        let ld_loc = ctx.alloc_node(CILNode::LdLoc(0));
        trees.push(ctx.alloc_root(CILRoot::Ret(ld_loc)));
    }
    let uint8_ptr = ctx.nptr(Type::Int(Int::U8));
    let ret = if void_ret { Type::Void } else { uint8_ptr };
    let uint8_ptr_idx = ctx.alloc_type(uint8_ptr);
    let alloc_ptr_name = ctx.alloc_string("alloc_ptr");
    let sig = ctx.alloc_sig(FnSig::new([], ret));
    let main_module_id = ctx.main_module();
    let init_method = MethodDef::from_blocks(
        Access::Private,
        main_module_id,
        &format!("init_{name}"),
        sig,
        MethodKind::Static,
        vec![BasicBlock::new(trees, 0, None)],
        vec![(Some(alloc_ptr_name), uint8_ptr_idx)],
        vec![],
        ctx,
    );
    ctx.new_method(init_method)
}
