use super::{
    Assembly, BasicBlock, CILNode, CILRoot, ClassDef, ClassRef, Const, FieldDesc, FnSig, Int,
    MethodDef, MethodDefIdx, MethodImpl, MethodRef, NativeImport, StaticFieldDesc, Type,
    asm::{CCTOR, TCCTOR, USER_INIT},
    bimap::Interned,
    class::ClassDefIdx,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) type ClassDefinitionIdentity = (Option<String>, String, u32);
pub(crate) type ClassKindOverrides = BTreeMap<ClassDefinitionIdentity, bool>;

#[derive(Default)]
pub(crate) struct AssemblyLinkPlan {
    class_kind_overrides: ClassKindOverrides,
    preflight_stats: LinkPreflightStats,
}

impl AssemblyLinkPlan {
    pub(crate) fn requires_class_kind_rebuild(&self) -> bool {
        !self.class_kind_overrides.is_empty()
    }

    pub(crate) fn class_kind_overrides(&self) -> &ClassKindOverrides {
        &self.class_kind_overrides
    }

    pub(crate) const fn preflight_stats(&self) -> LinkPreflightStats {
        self.preflight_stats
    }
}

/// Work attributable to semantic link preflight rather than IR relocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkPreflightStats {
    /// Destination class-definition semantic identity keys built for a missing index.
    pub destination_class_definitions_indexed: usize,
    /// Destination class-reference semantic identity keys built for a missing index.
    pub destination_class_references_indexed: usize,
    /// Destination method semantic identity keys built for a missing index.
    pub destination_method_definitions_indexed: usize,
    /// Destination native imports scanned to initialize a missing persistent index.
    pub destination_native_imports_indexed: usize,
    /// Destination static-field semantic keys built while validating a missing index.
    pub destination_static_fields_indexed: usize,
    /// Destination instance/interface/member-metadata keys built for a missing index.
    pub destination_class_members_indexed: usize,
    /// Incoming class-definition semantic identity keys built for a missing source index.
    pub source_class_definitions_indexed: usize,
    /// Incoming class-reference semantic identity keys built for a missing source index.
    pub source_class_references_indexed: usize,
    /// Incoming method semantic keys actually built for a missing source index.
    pub source_method_definitions_indexed: usize,
    /// Incoming native imports actually visited while building a missing source index.
    pub source_native_imports_indexed: usize,
    /// Incoming static-field semantic keys built while validating a missing source index.
    pub source_static_fields_indexed: usize,
    /// Incoming instance/interface/member-metadata keys built for a missing source index.
    pub source_class_members_indexed: usize,
    /// Source native-import entries traversed during cross-index preflight.
    pub source_native_import_preflight_visits: usize,
    /// Source authoritative-kind entries traversed during cross-index preflight.
    pub source_authoritative_kind_preflight_visits: usize,
    /// Source class-reference-kind entries traversed during cross-index preflight.
    pub source_class_reference_kind_preflight_visits: usize,
    /// Source class-definition entries traversed during cross-index preflight.
    pub source_class_definition_preflight_visits: usize,
    /// Source method-definition entries traversed during cross-index preflight.
    pub source_method_definition_preflight_visits: usize,
    /// Source static-field entries traversed during cross-index preflight.
    pub source_static_field_preflight_visits: usize,
    /// Source instance/interface/member-metadata entries traversed during cross-index preflight.
    pub source_class_member_preflight_visits: usize,
    /// Source cached override entries copied into a reconciliation plan.
    pub source_required_kind_override_preflight_visits: usize,
    /// Destination cached override entries copied into a reconciliation plan.
    pub destination_required_kind_override_preflight_visits: usize,
    /// Full reachable method-definition graphs canonicalized for structural comparison.
    pub method_definition_semantic_keys_built: usize,
    /// Persistent destination-map lookups performed for source identities.
    pub destination_identity_probes: usize,
    /// Overlapping class definitions that received the complete structural comparison.
    pub cross_class_definition_comparisons: usize,
    /// Overlapping method definitions that received the complete structural comparison.
    pub cross_method_definition_comparisons: usize,
    /// Overlapping static fields whose complete metadata keys were compared.
    pub cross_static_field_comparisons: usize,
    /// Overlapping class members whose complete metadata keys were compared.
    pub cross_class_member_comparisons: usize,
    /// Physical class definitions normalized through authoritative kind projection for structural
    /// duplicate auditing.
    pub authority_normalized_class_definitions: usize,
    /// Method definitions whose owner/signature/body graph was relocated through authoritative
    /// kind projection before normalized-identity grouping.
    pub authority_normalized_method_definitions: usize,
    /// Static-field rows normalized through authoritative kind projection during an additional
    /// cross-shard collision audit.
    pub authority_normalized_static_fields: usize,
}

impl LinkPreflightStats {
    pub(crate) fn accumulate(&mut self, other: Self) {
        self.destination_class_definitions_indexed += other.destination_class_definitions_indexed;
        self.destination_class_references_indexed += other.destination_class_references_indexed;
        self.destination_method_definitions_indexed += other.destination_method_definitions_indexed;
        self.destination_native_imports_indexed += other.destination_native_imports_indexed;
        self.destination_static_fields_indexed += other.destination_static_fields_indexed;
        self.destination_class_members_indexed += other.destination_class_members_indexed;
        self.source_class_definitions_indexed += other.source_class_definitions_indexed;
        self.source_class_references_indexed += other.source_class_references_indexed;
        self.source_method_definitions_indexed += other.source_method_definitions_indexed;
        self.source_native_imports_indexed += other.source_native_imports_indexed;
        self.source_static_fields_indexed += other.source_static_fields_indexed;
        self.source_class_members_indexed += other.source_class_members_indexed;
        self.source_native_import_preflight_visits += other.source_native_import_preflight_visits;
        self.source_authoritative_kind_preflight_visits +=
            other.source_authoritative_kind_preflight_visits;
        self.source_class_reference_kind_preflight_visits +=
            other.source_class_reference_kind_preflight_visits;
        self.source_class_definition_preflight_visits +=
            other.source_class_definition_preflight_visits;
        self.source_method_definition_preflight_visits +=
            other.source_method_definition_preflight_visits;
        self.source_static_field_preflight_visits += other.source_static_field_preflight_visits;
        self.source_class_member_preflight_visits += other.source_class_member_preflight_visits;
        self.source_required_kind_override_preflight_visits +=
            other.source_required_kind_override_preflight_visits;
        self.destination_required_kind_override_preflight_visits +=
            other.destination_required_kind_override_preflight_visits;
        self.method_definition_semantic_keys_built += other.method_definition_semantic_keys_built;
        self.destination_identity_probes += other.destination_identity_probes;
        self.cross_class_definition_comparisons += other.cross_class_definition_comparisons;
        self.cross_method_definition_comparisons += other.cross_method_definition_comparisons;
        self.cross_static_field_comparisons += other.cross_static_field_comparisons;
        self.cross_class_member_comparisons += other.cross_class_member_comparisons;
        self.authority_normalized_class_definitions += other.authority_normalized_class_definitions;
        self.authority_normalized_method_definitions +=
            other.authority_normalized_method_definitions;
        self.authority_normalized_static_fields += other.authority_normalized_static_fields;
    }

    #[cfg(test)]
    fn destination_items_indexed(self) -> usize {
        self.destination_class_definitions_indexed
            + self.destination_class_references_indexed
            + self.destination_method_definitions_indexed
            + self.destination_native_imports_indexed
            + self.destination_static_fields_indexed
            + self.destination_class_members_indexed
    }

    #[cfg(test)]
    fn source_items_indexed(self) -> usize {
        self.source_class_definitions_indexed
            + self.source_class_references_indexed
            + self.source_method_definitions_indexed
            + self.source_native_imports_indexed
            + self.source_static_fields_indexed
            + self.source_class_members_indexed
    }

    #[cfg(test)]
    fn accounted_work(self) -> usize {
        self.destination_items_indexed()
            + self.source_items_indexed()
            + self.source_native_import_preflight_visits
            + self.source_authoritative_kind_preflight_visits
            + self.source_class_reference_kind_preflight_visits
            + self.source_class_definition_preflight_visits
            + self.source_method_definition_preflight_visits
            + self.source_static_field_preflight_visits
            + self.source_class_member_preflight_visits
            + self.source_required_kind_override_preflight_visits
            + self.destination_required_kind_override_preflight_visits
            + self.destination_identity_probes
            + self.cross_class_definition_comparisons
            + self.cross_method_definition_comparisons
            + self.cross_static_field_comparisons
            + self.cross_class_member_comparisons
            + self.method_definition_semantic_keys_built
            + self.authority_normalized_class_definitions
            + self.authority_normalized_method_definitions
            + self.authority_normalized_static_fields
    }
}

#[derive(Clone, Copy, Default)]
struct LinkIndexBuildStats {
    class_definitions: usize,
    class_references: usize,
    method_definitions: usize,
    native_imports: usize,
    static_fields: usize,
    class_members: usize,
    method_semantic_graphs: usize,
    normalization_preflight: LinkPreflightStats,
}

impl LinkPreflightStats {
    fn record_destination_index_build(&mut self, build: LinkIndexBuildStats) {
        self.destination_class_definitions_indexed += build.class_definitions;
        self.destination_class_references_indexed += build.class_references;
        self.destination_method_definitions_indexed += build.method_definitions;
        self.destination_native_imports_indexed += build.native_imports;
        self.destination_static_fields_indexed += build.static_fields;
        self.destination_class_members_indexed += build.class_members;
        self.method_definition_semantic_keys_built += build.method_semantic_graphs;
        self.accumulate(build.normalization_preflight);
    }

    fn record_source_index_build(&mut self, build: LinkIndexBuildStats) {
        self.source_class_definitions_indexed += build.class_definitions;
        self.source_class_references_indexed += build.class_references;
        self.source_method_definitions_indexed += build.method_definitions;
        self.source_native_imports_indexed += build.native_imports;
        self.source_static_fields_indexed += build.static_fields;
        self.source_class_members_indexed += build.class_members;
        self.method_definition_semantic_keys_built += build.method_semantic_graphs;
        self.accumulate(build.normalization_preflight);
    }
}

/// Persistent semantic lookup state for repeated codegen-shard commits.
///
/// This cache is never serialized. It stores stable semantic keys plus current destination ids so
/// ordinary one-item shards can probe only their own identities rather than sorting every method
/// accumulated so far.
#[derive(Clone, Default)]
pub(crate) struct AssemblyLinkIndex {
    class_definitions: BTreeMap<ClassDefinitionIdentity, Vec<ClassDefIdx>>,
    class_reference_kinds: BTreeMap<ClassDefinitionIdentity, u8>,
    authoritative_kinds: ClassKindOverrides,
    required_kind_overrides: ClassKindOverrides,
    method_definitions: BTreeMap<Vec<u8>, MethodDefIdx>,
    special_methods: BTreeMap<Vec<u8>, SpecialMethodLinkInfo>,
    static_fields: BTreeMap<(ClassDefinitionIdentity, StaticFieldIdentity), StaticFieldMetadataKey>,
    class_members: BTreeMap<(ClassDefinitionIdentity, ClassMemberIdentity), ClassMemberMetadataKey>,
    native_imports: BTreeMap<String, NativeImport>,
}

/// An expected semantic conflict discovered before an assembly shard is committed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssemblyLinkError {
    ClassFieldConflict {
        class: String,
        field: String,
        existing_type: String,
        incoming_type: String,
        existing_offset: Option<u32>,
        incoming_offset: Option<u32>,
    },
    StaticFieldConflict {
        class: String,
        field: String,
        field_type: String,
        existing: String,
        incoming: String,
    },
    ClassBaseConflict {
        class: String,
        existing_base: String,
        incoming_base: String,
    },
    ClassDefinitionConflict {
        class: String,
        property: String,
        existing: String,
        incoming: String,
    },
    MethodConflict {
        class: String,
        method: String,
        signature: String,
        existing_access: String,
        incoming_access: String,
        detail: String,
    },
    NativeImportConflict {
        symbol: String,
        existing: NativeImport,
        incoming: NativeImport,
    },
}

impl std::fmt::Display for AssemblyLinkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClassFieldConflict {
                class,
                field,
                existing_type,
                incoming_type,
                existing_offset,
                incoming_offset,
            } => write!(
                formatter,
                "class field differs across codegen shards: class={class}, field={field}, \
                 existing={existing_type}@{existing_offset:?}, \
                 incoming={incoming_type}@{incoming_offset:?}"
            ),
            Self::StaticFieldConflict {
                class,
                field,
                field_type,
                existing,
                incoming,
            } => write!(
                formatter,
                "static field metadata differs across codegen shards: class={class}, \
                 field={field}, type={field_type}, existing={existing}, incoming={incoming}"
            ),
            Self::ClassBaseConflict {
                class,
                existing_base,
                incoming_base,
            } => write!(
                formatter,
                "class base differs across codegen shards: class={class}, \
                 existing_base={existing_base}, incoming_base={incoming_base}"
            ),
            Self::ClassDefinitionConflict {
                class,
                property,
                existing,
                incoming,
            } => write!(
                formatter,
                "class definition differs across codegen shards: class={class}, \
                 property={property}, existing={existing}, incoming={incoming}"
            ),
            Self::MethodConflict {
                class,
                method,
                signature,
                existing_access,
                incoming_access,
                detail,
            } => write!(
                formatter,
                "method differs across codegen shards: {class}::{method}{signature}, \
                 existing_access={existing_access}, incoming_access={incoming_access}, \
                 detail={detail}"
            ),
            Self::NativeImportConflict {
                symbol,
                existing,
                incoming,
            } => write!(
                formatter,
                "conflicting native import declarations for `{symbol}`: \
                 existing={existing:?}, incoming={incoming:?}"
            ),
        }
    }
}

impl std::error::Error for AssemblyLinkError {}

/// Per-arena work performed while relocating one assembly into another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaRelocationStats {
    /// Distinct source ids whose values were translated.
    pub unique_visits: usize,
    /// Repeated source-id lookups satisfied by the dense relocation map.
    pub cache_hits: usize,
}

/// Summary of a single [`Assembly::link_with_stats`] relocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RelocationStats {
    pub preflight: LinkPreflightStats,
    /// Source class-member rows inspected while producing indexed commit deltas.
    pub class_members_visited: usize,
    /// New class-member rows retained after compatible duplicate removal.
    pub class_members_committed: usize,
    /// Source static-field rows inspected during class relocation.
    pub class_static_fields_visited: usize,
    /// Retained source static-field rows appended to class definitions during commit.
    pub class_static_fields_committed: usize,
    /// Source method definitions processed during commit (new or compatible duplicate).
    pub method_definitions_committed: usize,
    pub strings: ArenaRelocationStats,
    pub types: ArenaRelocationStats,
    pub class_refs: ArenaRelocationStats,
    pub nodes: ArenaRelocationStats,
    pub roots: ArenaRelocationStats,
    pub signatures: ArenaRelocationStats,
    pub method_refs: ArenaRelocationStats,
    pub fields: ArenaRelocationStats,
    pub statics: ArenaRelocationStats,
    pub const_data: ArenaRelocationStats,
}

/// Relocates one owned IR value from a source assembly into a destination assembly.
///
/// Implementations live beside the value's private fields so adding metadata forces an exhaustive
/// destructuring update at compile time.
pub(crate) trait RelocateValue: Sized {
    type Output;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self::Output;
}

struct DenseRelocationMap<T> {
    slots: Vec<Option<Interned<T>>>,
}

impl<T> Default for DenseRelocationMap<T> {
    fn default() -> Self {
        Self { slots: Vec::new() }
    }
}

impl<T> DenseRelocationMap<T> {
    fn get(&self, source: Interned<T>) -> Option<Interned<T>> {
        self.slots
            .get(source.inner() as usize - 1)
            .copied()
            .flatten()
    }

    fn insert(&mut self, source: Interned<T>, destination: Interned<T>) {
        let index = source.inner() as usize - 1;
        if self.slots.len() <= index {
            self.slots.resize(index + 1, None);
        }
        assert!(
            self.slots[index].replace(destination).is_none(),
            "source id was relocated more than once"
        );
    }
}

/// Memoized, per-source-assembly relocation state.
type SpecialMethodFragmentOrders = BTreeMap<Vec<u8>, (Vec<Vec<u8>>, Vec<Vec<u8>>)>;

pub(crate) struct RelocateCtx<'source> {
    source: &'source Assembly,
    /// Optional final-link projection for the compiler's internal `MainModule` sentinel.
    ///
    /// It is intentionally applied while relocating rather than changing codegen: old serialized
    /// artifacts remain readable and all definition/reference arenas are rewritten together.
    main_module_name: Option<&'source str>,
    class_kind_overrides: ClassKindOverrides,
    special_method_fragment_orders: SpecialMethodFragmentOrders,
    duplicate_static_fields: BTreeSet<(ClassDefinitionIdentity, StaticFieldIdentity)>,
    /// Static identities already encountered while relocating this source assembly.
    ///
    /// Cross-assembly duplicates are supplied separately by `duplicate_static_fields`, while this
    /// set collapses compatible physical definitions that become identical only after applying
    /// class-kind authority during a whole-assembly rebuild.
    seen_static_fields: BTreeSet<(ClassDefinitionIdentity, StaticFieldIdentity)>,
    seen_class_members: BTreeSet<(ClassDefinitionIdentity, ClassMemberIdentity)>,
    strings: DenseRelocationMap<crate::IString>,
    types: DenseRelocationMap<Type>,
    class_refs: DenseRelocationMap<ClassRef>,
    nodes: DenseRelocationMap<CILNode>,
    roots: DenseRelocationMap<CILRoot>,
    signatures: DenseRelocationMap<FnSig>,
    method_refs: DenseRelocationMap<MethodRef>,
    fields: DenseRelocationMap<FieldDesc>,
    statics: DenseRelocationMap<StaticFieldDesc>,
    const_data: DenseRelocationMap<Box<[u8]>>,
    stats: RelocationStats,
}

impl<'source> RelocateCtx<'source> {
    fn new(source: &'source Assembly) -> Self {
        Self::with_options(source, None, ClassKindOverrides::new())
    }

    fn with_main_module_name(
        source: &'source Assembly,
        main_module_name: Option<&'source str>,
    ) -> Self {
        Self::with_options(source, main_module_name, ClassKindOverrides::new())
    }

    fn with_class_kind_overrides(
        source: &'source Assembly,
        class_kind_overrides: ClassKindOverrides,
    ) -> Self {
        Self::with_options(source, None, class_kind_overrides)
    }

    fn with_options(
        source: &'source Assembly,
        main_module_name: Option<&'source str>,
        class_kind_overrides: ClassKindOverrides,
    ) -> Self {
        Self {
            source,
            main_module_name,
            class_kind_overrides,
            special_method_fragment_orders: SpecialMethodFragmentOrders::new(),
            duplicate_static_fields: BTreeSet::new(),
            seen_static_fields: BTreeSet::new(),
            seen_class_members: BTreeSet::new(),
            strings: DenseRelocationMap::default(),
            types: DenseRelocationMap::default(),
            class_refs: DenseRelocationMap::default(),
            nodes: DenseRelocationMap::default(),
            roots: DenseRelocationMap::default(),
            signatures: DenseRelocationMap::default(),
            method_refs: DenseRelocationMap::default(),
            fields: DenseRelocationMap::default(),
            statics: DenseRelocationMap::default(),
            const_data: DenseRelocationMap::default(),
            stats: RelocationStats::default(),
        }
    }

    fn class_ref_kind_override(&self, source: Interned<ClassRef>) -> Option<bool> {
        let class = &self.source[source];
        let generic_arity = self
            .source
            .class_defs()
            .get(&ClassDefIdx(source))
            .map_or_else(
                || {
                    u32::try_from(class.generics().len())
                        .expect("managed class generic arity exceeds u32")
                },
                ClassDef::generics,
            );
        self.class_kind_overrides
            .get(&(
                class.asm().map(|owner| self.source[owner].to_string()),
                self.source[class.name()].to_string(),
                generic_arity,
            ))
            .copied()
    }

    pub(crate) fn class_definition_kind_override(
        &self,
        name: Interned<crate::IString>,
        generic_arity: u32,
    ) -> Option<bool> {
        self.class_kind_overrides
            .get(&(None, self.source[name].to_string(), generic_arity))
            .copied()
    }

    pub(crate) fn string(
        &mut self,
        destination: &mut Assembly,
        source: Interned<crate::IString>,
    ) -> Interned<crate::IString> {
        if let Some(relocated) = self.strings.get(source) {
            self.stats.strings.cache_hits += 1;
            return relocated;
        }
        let source_value = &self.source[source];
        let relocated = if *source_value == *super::asm::MAIN_MODULE {
            destination.alloc_string(self.main_module_name.unwrap_or(source_value))
        } else {
            destination.alloc_string(source_value)
        };
        self.strings.insert(source, relocated);
        self.stats.strings.unique_visits += 1;
        relocated
    }

    pub(crate) fn type_id(
        &mut self,
        destination: &mut Assembly,
        source: Interned<Type>,
    ) -> Interned<Type> {
        if let Some(relocated) = self.types.get(source) {
            self.stats.types.cache_hits += 1;
            return relocated;
        }
        let value = self.source[source];
        let value = destination.translate_type(self, value);
        let relocated = destination.alloc_type(value);
        self.types.insert(source, relocated);
        self.stats.types.unique_visits += 1;
        relocated
    }

    pub(crate) fn class_ref(
        &mut self,
        destination: &mut Assembly,
        source: Interned<ClassRef>,
    ) -> Interned<ClassRef> {
        if let Some(relocated) = self.class_refs.get(source) {
            self.stats.class_refs.cache_hits += 1;
            return relocated;
        }
        let kind_override = self.class_ref_kind_override(source);
        let mut class_ref = self
            .source
            .class_ref(source)
            .clone()
            .relocate(self, destination);
        if let Some(is_valuetype) = kind_override {
            class_ref.set_is_valuetype_for_link(is_valuetype);
        }
        let relocated = destination.alloc_class_ref(class_ref);
        self.class_refs.insert(source, relocated);
        self.stats.class_refs.unique_visits += 1;
        relocated
    }

    pub(crate) fn signature(
        &mut self,
        destination: &mut Assembly,
        source: Interned<FnSig>,
    ) -> Interned<FnSig> {
        if let Some(relocated) = self.signatures.get(source) {
            self.stats.signatures.cache_hits += 1;
            return relocated;
        }
        let signature = self.source[source].clone().relocate(self, destination);
        let relocated = destination.alloc_sig(signature);
        self.signatures.insert(source, relocated);
        self.stats.signatures.unique_visits += 1;
        relocated
    }

    pub(crate) fn method_ref(
        &mut self,
        destination: &mut Assembly,
        source: Interned<MethodRef>,
    ) -> Interned<MethodRef> {
        if let Some(relocated) = self.method_refs.get(source) {
            self.stats.method_refs.cache_hits += 1;
            return relocated;
        }
        let method = self.source[source].clone().relocate(self, destination);
        let relocated = destination.alloc_methodref(method);
        self.method_refs.insert(source, relocated);
        self.stats.method_refs.unique_visits += 1;
        relocated
    }

    pub(crate) fn field(
        &mut self,
        destination: &mut Assembly,
        source: Interned<FieldDesc>,
    ) -> Interned<FieldDesc> {
        if let Some(relocated) = self.fields.get(source) {
            self.stats.fields.cache_hits += 1;
            return relocated;
        }
        let field = (*self.source.get_field(source)).relocate(self, destination);
        let relocated = destination.alloc_field(field);
        self.fields.insert(source, relocated);
        self.stats.fields.unique_visits += 1;
        relocated
    }

    pub(crate) fn static_field(
        &mut self,
        destination: &mut Assembly,
        source: Interned<StaticFieldDesc>,
    ) -> Interned<StaticFieldDesc> {
        if let Some(relocated) = self.statics.get(source) {
            self.stats.statics.cache_hits += 1;
            return relocated;
        }
        let field = (*self.source.get_static_field(source)).relocate(self, destination);
        let relocated = destination.alloc_sfld(field);
        self.statics.insert(source, relocated);
        self.stats.statics.unique_visits += 1;
        relocated
    }

    pub(crate) fn const_data(
        &mut self,
        destination: &mut Assembly,
        source: Interned<Box<[u8]>>,
    ) -> Interned<Box<[u8]>> {
        if let Some(relocated) = self.const_data.get(source) {
            self.stats.const_data.cache_hits += 1;
            return relocated;
        }
        let relocated = destination.alloc_const_data(&self.source.const_data[source]);
        self.const_data.insert(source, relocated);
        self.stats.const_data.unique_visits += 1;
        relocated
    }

    pub(crate) fn node(
        &mut self,
        destination: &mut Assembly,
        source: Interned<CILNode>,
    ) -> Interned<CILNode> {
        if let Some(relocated) = self.nodes.get(source) {
            self.stats.nodes.cache_hits += 1;
            return relocated;
        }
        let node = self.source.get_node(source).clone();
        let node = destination.translate_node(self, node);
        let relocated = destination.alloc_node(node);
        self.nodes.insert(source, relocated);
        self.stats.nodes.unique_visits += 1;
        relocated
    }

    pub(crate) fn root(
        &mut self,
        destination: &mut Assembly,
        source: Interned<CILRoot>,
    ) -> Interned<CILRoot> {
        if let Some(relocated) = self.roots.get(source) {
            self.stats.roots.cache_hits += 1;
            return relocated;
        }
        let root = self.source.get_root(source).clone();
        let root = destination.translate_root(self, root);
        let relocated = destination.alloc_root(root);
        self.roots.insert(source, relocated);
        self.stats.roots.unique_visits += 1;
        relocated
    }
}

impl Assembly {
    pub(crate) fn translate_type(&mut self, ctx: &mut RelocateCtx<'_>, tpe: Type) -> Type {
        match tpe {
            Type::Ptr(inner) => Type::Ptr(ctx.type_id(self, inner)),
            Type::Ref(inner) => Type::Ref(ctx.type_id(self, inner)),
            Type::Int(_)
            | Type::Float(_)
            | Type::PlatformString
            | Type::PlatformChar
            | Type::Bool
            | Type::Void
            | Type::PlatformObject
            | Type::PlatformGeneric(_, _)
            | Type::SIMDVector(_) => tpe,
            Type::ClassRef(class_ref) => Type::ClassRef(ctx.class_ref(self, class_ref)),
            Type::PlatformArray { elem, dims } => Type::PlatformArray {
                elem: ctx.type_id(self, elem),
                dims,
            },
            Type::FnPtr(sig) => Type::FnPtr(ctx.signature(self, sig)),
        }
    }
    pub(crate) fn translate_const(&mut self, ctx: &mut RelocateCtx<'_>, cst: &Const) -> Const {
        match cst {
            super::Const::PlatformString(pstr) => {
                super::Const::PlatformString(ctx.string(self, *pstr))
            }

            super::Const::Null(cref) => super::Const::Null(ctx.class_ref(self, *cref)),
            super::Const::ByteBuffer { data, tpe } => super::Const::ByteBuffer {
                data: ctx.const_data(self, *data),
                tpe: ctx.type_id(self, *tpe),
            },
            _ => cst.clone(),
        }
    }
    // The complexity of this function is unavoidable.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn translate_node(&mut self, ctx: &mut RelocateCtx<'_>, node: CILNode) -> CILNode {
        match &node {
            CILNode::LdLoc(_) | CILNode::LdLocA(_) | CILNode::LdArg(_) | CILNode::LdArgA(_) => node,
            CILNode::Const(cst) => CILNode::Const(Box::new(self.translate_const(ctx, cst))),
            CILNode::BinOp(a, b, op) => CILNode::BinOp(ctx.node(self, *a), ctx.node(self, *b), *op),
            CILNode::UnOp(a, op) => CILNode::UnOp(ctx.node(self, *a), op.clone()),
            CILNode::Call(call_arg) => {
                let (mref, args, pure) = call_arg.as_ref();
                let mref = ctx.method_ref(self, *mref);
                let args = args.iter().map(|arg| ctx.node(self, *arg)).collect();
                CILNode::Call(Box::new((mref, args, *pure)))
            }
            CILNode::IntCast {
                input,
                target,
                extend,
            } => CILNode::IntCast {
                input: ctx.node(self, *input),
                target: *target,
                extend: *extend,
            },
            CILNode::FloatCast {
                input,
                target,
                is_signed,
            } => CILNode::FloatCast {
                input: ctx.node(self, *input),
                target: *target,
                is_signed: *is_signed,
            },
            CILNode::RefToPtr(input) => CILNode::RefToPtr(ctx.node(self, *input)),
            CILNode::PtrCast(input, cast_res) => {
                let input = ctx.node(self, *input);
                let cast_res = match cast_res.as_ref() {
                    crate::cilnode::PtrCastRes::Ptr(inner) => {
                        crate::cilnode::PtrCastRes::Ptr(ctx.type_id(self, *inner))
                    }
                    crate::cilnode::PtrCastRes::Ref(inner) => {
                        crate::cilnode::PtrCastRes::Ref(ctx.type_id(self, *inner))
                    }
                    crate::cilnode::PtrCastRes::FnPtr(sig) => {
                        crate::cilnode::PtrCastRes::FnPtr(ctx.signature(self, *sig))
                    }
                    crate::cilnode::PtrCastRes::USize | crate::cilnode::PtrCastRes::ISize => {
                        *cast_res.clone()
                    }
                };
                CILNode::PtrCast(input, Box::new(cast_res))
            }
            CILNode::LdFieldAddress { addr, field } => CILNode::LdFieldAddress {
                addr: ctx.node(self, *addr),
                field: ctx.field(self, *field),
            },
            CILNode::LdField { addr, field } => CILNode::LdField {
                addr: ctx.node(self, *addr),
                field: ctx.field(self, *field),
            },
            CILNode::LdInd {
                addr,
                tpe,
                volatile: volitale,
            } => CILNode::LdInd {
                addr: ctx.node(self, *addr),
                tpe: ctx.type_id(self, *tpe),
                volatile: *volitale,
            },
            CILNode::SizeOf(tpe) => CILNode::SizeOf(ctx.type_id(self, *tpe)),
            CILNode::GetException => CILNode::GetException,
            CILNode::IsInst(object, tpe) => {
                CILNode::IsInst(ctx.node(self, *object), ctx.type_id(self, *tpe))
            }
            CILNode::CheckedCast(object, tpe) => {
                CILNode::CheckedCast(ctx.node(self, *object), ctx.type_id(self, *tpe))
            }
            CILNode::CallI(args) => {
                let (fnptr, sig, args) = args.as_ref();
                let fnptr = ctx.node(self, *fnptr);
                let sig = ctx.signature(self, *sig);
                let args = args.iter().map(|arg| ctx.node(self, *arg)).collect();
                CILNode::CallI(Box::new((fnptr, sig, args)))
            }
            CILNode::LocAlloc { size } => CILNode::LocAlloc {
                size: ctx.node(self, *size),
            },
            CILNode::LdStaticField(sfld) => CILNode::LdStaticField(ctx.static_field(self, *sfld)),
            CILNode::LdStaticFieldAddress(sfld) => {
                CILNode::LdStaticFieldAddress(ctx.static_field(self, *sfld))
            }
            CILNode::LdFtn(mref) => CILNode::LdFtn(ctx.method_ref(self, *mref)),
            CILNode::LdTypeToken(tpe) => CILNode::LdTypeToken(ctx.type_id(self, *tpe)),
            CILNode::LdLen(len) => CILNode::LdLen(ctx.node(self, *len)),
            CILNode::LocAllocAlgined { tpe, align } => CILNode::LocAllocAlgined {
                tpe: ctx.type_id(self, *tpe),
                align: *align,
            },
            CILNode::LdElelemRef { array, index } => CILNode::LdElelemRef {
                array: ctx.node(self, *array),
                index: ctx.node(self, *index),
            },
            CILNode::LdElem { array, index, elem } => CILNode::LdElem {
                array: ctx.node(self, *array),
                index: ctx.node(self, *index),
                elem: ctx.type_id(self, *elem),
            },
            CILNode::UnboxAny { object, tpe } => CILNode::UnboxAny {
                object: ctx.node(self, *object),
                tpe: ctx.type_id(self, *tpe),
            },
            CILNode::Box { value, tpe } => CILNode::Box {
                value: ctx.node(self, *value),
                tpe: ctx.type_id(self, *tpe),
            },
            CILNode::NewArr { elem, len } => CILNode::NewArr {
                elem: ctx.type_id(self, *elem),
                len: ctx.node(self, *len),
            },
        }
    }
    // The complexity of this function is unavoidable.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn translate_root(&mut self, ctx: &mut RelocateCtx<'_>, root: CILRoot) -> CILRoot {
        match root {
            CILRoot::Unreachable(str) => CILRoot::Unreachable(ctx.string(self, str)),
            CILRoot::StLoc(loc, node) => CILRoot::StLoc(loc, ctx.node(self, node)),
            CILRoot::StArg(loc, node) => CILRoot::StArg(loc, ctx.node(self, node)),
            CILRoot::Ret(node) => CILRoot::Ret(ctx.node(self, node)),
            CILRoot::Pop(node) => CILRoot::Pop(ctx.node(self, node)),
            CILRoot::Throw(node) => CILRoot::Throw(ctx.node(self, node)),
            CILRoot::Branch(branch) => {
                let (target, sub_target, cond) = branch.as_ref();
                let cond = cond.as_ref().map(|cond| match cond {
                    super::cilroot::BranchCond::True(cond) => {
                        super::cilroot::BranchCond::True(ctx.node(self, *cond))
                    }
                    super::cilroot::BranchCond::False(cond) => {
                        super::cilroot::BranchCond::False(ctx.node(self, *cond))
                    }
                    super::cilroot::BranchCond::Eq(a, b) => {
                        super::cilroot::BranchCond::Eq(ctx.node(self, *a), ctx.node(self, *b))
                    }
                    super::cilroot::BranchCond::Ne(a, b) => {
                        super::cilroot::BranchCond::Ne(ctx.node(self, *a), ctx.node(self, *b))
                    }
                    super::cilroot::BranchCond::Lt(a, b, cmp_kind) => {
                        super::cilroot::BranchCond::Lt(
                            ctx.node(self, *a),
                            ctx.node(self, *b),
                            cmp_kind.clone(),
                        )
                    }
                    super::cilroot::BranchCond::Gt(a, b, cmp_kind) => {
                        super::cilroot::BranchCond::Gt(
                            ctx.node(self, *a),
                            ctx.node(self, *b),
                            cmp_kind.clone(),
                        )
                    }
                    super::cilroot::BranchCond::Le(a, b, cmp_kind) => {
                        super::cilroot::BranchCond::Le(
                            ctx.node(self, *a),
                            ctx.node(self, *b),
                            cmp_kind.clone(),
                        )
                    }
                    super::cilroot::BranchCond::Ge(a, b, cmp_kind) => {
                        super::cilroot::BranchCond::Ge(
                            ctx.node(self, *a),
                            ctx.node(self, *b),
                            cmp_kind.clone(),
                        )
                    }
                });
                CILRoot::Branch(Box::new((*target, *sub_target, cond)))
            }
            CILRoot::VoidRet
            | CILRoot::Break
            | CILRoot::Nop
            | CILRoot::InitFragmentBoundary
            | CILRoot::ReThrow => root,
            CILRoot::SourceFileInfo {
                line_start,
                line_len,
                col_start,
                col_len,
                file,
            } => CILRoot::SourceFileInfo {
                line_start,
                line_len,
                col_start,
                col_len,
                file: ctx.string(self, file),
            },
            CILRoot::SetField(info) => {
                let (field, addr, val) = info.as_ref();
                CILRoot::SetField(Box::new((
                    ctx.field(self, *field),
                    ctx.node(self, *addr),
                    ctx.node(self, *val),
                )))
            }
            CILRoot::Call(call_arg) => {
                let (mref, args, pure) = call_arg.as_ref();
                let mref = ctx.method_ref(self, *mref);
                let args = args.iter().map(|arg| ctx.node(self, *arg)).collect();
                CILRoot::Call(Box::new((mref, args, *pure)))
            }
            CILRoot::StInd(info) => {
                let (addr, val, tpe, volitile) = info.as_ref();
                CILRoot::StInd(Box::new((
                    ctx.node(self, *addr),
                    ctx.node(self, *val),
                    self.translate_type(ctx, *tpe),
                    *volitile,
                )))
            }
            CILRoot::CpObj { src, dst, tpe } => CILRoot::CpObj {
                src: ctx.node(self, src),
                dst: ctx.node(self, dst),
                tpe: ctx.type_id(self, tpe),
            },
            CILRoot::InitObj(src, tpe) => {
                CILRoot::InitObj(ctx.node(self, src), ctx.type_id(self, tpe))
            }
            CILRoot::InitBlk(info) => {
                let (dst, val, count) = info.as_ref();
                CILRoot::InitBlk(Box::new((
                    ctx.node(self, *dst),
                    ctx.node(self, *val),
                    ctx.node(self, *count),
                )))
            }
            CILRoot::CpBlk(info) => {
                let (dst, src, len) = info.as_ref();
                CILRoot::CpBlk(Box::new((
                    ctx.node(self, *dst),
                    ctx.node(self, *src),
                    ctx.node(self, *len),
                )))
            }
            CILRoot::CallI(args) => {
                let (fnptr, sig, args) = args.as_ref();
                let fnptr = ctx.node(self, *fnptr);
                let sig = ctx.signature(self, *sig);
                let args = args.iter().map(|arg| ctx.node(self, *arg)).collect();
                CILRoot::CallI(Box::new((fnptr, sig, args)))
            }
            CILRoot::ExitSpecialRegion { target, source } => {
                CILRoot::ExitSpecialRegion { target, source }
            }
            CILRoot::TerminateRegion { protected, reason } => {
                // The protected child root is NOT in any block's root list (only the region and the
                // continuation `goto` are), so it must be translated + re-interned here explicitly.
                let protected = ctx.root(self, protected);
                CILRoot::TerminateRegion { protected, reason }
            }
            CILRoot::SetStaticField { field, val } => CILRoot::SetStaticField {
                field: ctx.static_field(self, field),
                val: ctx.node(self, val),
            },
            CILRoot::StElem {
                array,
                index,
                value,
                elem,
            } => CILRoot::StElem {
                array: ctx.node(self, array),
                index: ctx.node(self, index),
                value: ctx.node(self, value),
                elem: ctx.type_id(self, elem),
            },
        }
    }
    pub(crate) fn translate_class_def(
        &mut self,
        ctx: &mut RelocateCtx<'_>,
        source_class: ClassDefIdx,
        def: &ClassDef,
    ) {
        let super::class::RelocatedClassDef {
            definition: mut translated,
            source_methods,
        } = def.clone().relocate(ctx, self);
        let class_ref = self.alloc_class_ref(translated.ref_to());
        match ctx.class_refs.get(source_class.0) {
            Some(existing) => assert_eq!(
                existing, class_ref,
                "class-definition relocation disagrees with an earlier class reference"
            ),
            None => {
                ctx.class_refs.insert(source_class.0, class_ref);
                ctx.stats.class_refs.unique_visits += 1;
            }
        }
        if !translated.static_fields().is_empty() {
            let owner = class_ref_definition_identity(self, class_ref);
            let retained = translated
                .static_fields()
                .iter()
                .filter(|field| {
                    let (identity, _) = static_field_semantic_keys(self, &translated, field);
                    let key = (owner.clone(), identity);
                    let is_cross_assembly_duplicate = ctx.duplicate_static_fields.contains(&key);
                    let is_first_source_definition = ctx.seen_static_fields.insert(key);
                    !is_cross_assembly_duplicate && is_first_source_definition
                })
                .cloned()
                .collect();
            *translated.static_fields_mut() = retained;
        }
        let owner = class_ref_definition_identity(self, class_ref);
        retain_new_class_members(self, ctx, &owner, &mut translated);
        ctx.stats.class_static_fields_visited += def.static_fields().len();
        ctx.stats.class_static_fields_committed += translated.static_fields().len();
        let (defs_mut, _) = self.class_defs_mut_strings();
        match defs_mut.entry(ClassDefIdx(class_ref)) {
            std::collections::hash_map::Entry::Occupied(mut occupied) => {
                occupied.get_mut().merge_defs(translated);
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(translated);
            }
        }

        for source_method_id in source_methods {
            ctx.stats.method_definitions_committed += 1;
            let source_method = ctx.source.method_def(source_method_id).clone();
            let method_def = source_method.relocate(ctx, self);
            let method_ref = self.alloc_methodref(method_def.ref_to());
            match ctx.method_refs.get(source_method_id.0) {
                Some(existing) => assert_eq!(
                    existing, method_ref,
                    "method-definition relocation disagrees with an earlier method reference"
                ),
                None => ctx.method_refs.insert(source_method_id.0, method_ref),
            }
            let original = self.method_defs().get(&MethodDefIdx(method_ref));
            let method_def = match original {
                Some(original) => {
                    assert_eq!(method_def.name(), original.name());
                    let name = &self[method_def.name()];
                    if SPECIAL_METHOD_NAMES.iter().any(|val| **val == *name) {
                        assert_eq!(method_def.access(), original.access());
                        assert_eq!(method_def.class(), original.class());
                        assert_eq!(method_def.sig(), original.sig());
                        assert_eq!(method_def.kind(), original.kind());
                        let method_key = self.method_semantic_key(MethodDefIdx(method_ref));
                        let cached_orders =
                            ctx.special_method_fragment_orders.get(&method_key).cloned();
                        merge_special_method_bodies(
                            self,
                            original.clone(),
                            method_def,
                            cached_orders.as_ref(),
                        )
                    } else {
                        assert_eq!(method_def.access(), original.access());
                        assert_eq!(method_def.class(), original.class());
                        assert_eq!(method_def.sig(), original.sig());
                        assert_eq!(method_def.kind(), original.kind());
                        match (method_def.implementation(), original.implementation()) {
                            (MethodImpl::Missing, MethodImpl::Missing) => method_def,
                            (MethodImpl::Missing, _) => original.clone(),
                            (_, MethodImpl::Missing) => method_def,
                            _ if method_def == *original => original.clone(),
                            _ => panic!(
                                "different real method implementations reached relocation without \
                                 preflight rejection: {}::{name}",
                                class_name(self, self[method_ref].class())
                            ),
                        }
                    }
                }
                None => method_def,
            };
            self.new_method(method_def);
        }
    }
}
const SPECIAL_METHOD_NAMES: &[&str] = &[CCTOR, TCCTOR, USER_INIT];

fn class_name(assembly: &Assembly, class: Interned<ClassRef>) -> String {
    let class = &assembly[class];
    let name = assembly[class.name()].to_string();
    class.asm().map_or(name.clone(), |owner| {
        format!("{}::{name}", &assembly[owner])
    })
}

fn signature_name(assembly: &Assembly, signature: Interned<FnSig>) -> String {
    let signature = &assembly[signature];
    let inputs = signature
        .inputs()
        .iter()
        .map(|input| input.mangle(assembly))
        .collect::<Vec<_>>()
        .join(",");
    format!("({inputs})->{}", signature.output().mangle(assembly))
}

fn enum_semantic_key(
    assembly: &Assembly,
    definition: &super::class::EnumDef,
) -> (Int, Vec<(String, Const)>) {
    (
        definition.underlying(),
        definition
            .variants()
            .iter()
            .map(|(name, value)| (assembly[*name].to_string(), value.clone()))
            .collect(),
    )
}

fn fixed_array_semantic_key(
    assembly: &Assembly,
    layout: &super::class::FixedArrayLayout,
) -> (Vec<u8>, (u64, u64, u64, u64)) {
    (
        assembly.type_semantic_key(layout.element()),
        layout.link_semantic_dimensions(),
    )
}

fn class_definition_identity(
    assembly: &Assembly,
    class: ClassDefIdx,
) -> (Option<String>, String, u32) {
    let class_ref = &assembly[class.0];
    let definition = assembly
        .class_defs()
        .get(&class)
        .expect("class definition identity requires a definition");
    (
        class_ref.asm().map(|owner| assembly[owner].to_string()),
        assembly[class_ref.name()].to_string(),
        definition.generics(),
    )
}

fn class_ref_definition_identity(
    assembly: &Assembly,
    class: Interned<ClassRef>,
) -> ClassDefinitionIdentity {
    let class_ref = &assembly[class];
    let generic_arity = assembly.class_defs().get(&ClassDefIdx(class)).map_or_else(
        || {
            u32::try_from(class_ref.generics().len())
                .expect("managed class generic arity exceeds u32")
        },
        ClassDef::generics,
    );
    (
        class_ref.asm().map(|owner| assembly[owner].to_string()),
        assembly[class_ref.name()].to_string(),
        generic_arity,
    )
}

/// Validates the value-kind evidence for every logical type defined in one shard.
///
/// A `ClassRef` interns `is_valuetype` as part of its physical key, so a malformed shard can
/// contain two `ClassDef` rows for the same CLR identity before cross-shard preflight ever sees
/// them. Group those rows by their CLR identity rather than collecting them into a map (which
/// would silently retain only one row). One authoritative row may correct one consistent set of
/// non-authoritative placeholders, but neither evidence class may contradict itself.
fn validate_class_definition_identity_groups(
    assembly: &Assembly,
    classes: &[(ClassDefinitionIdentity, ClassDefIdx)],
) -> Result<(), AssemblyLinkError> {
    let mut kinds = BTreeMap::<(ClassDefinitionIdentity, bool), bool>::new();
    for (identity, class) in classes {
        let definition = &assembly[*class];
        let authoritative = definition.is_valuetype_authoritative();
        let kind = definition.is_valuetype();
        if let Some(existing_kind) = kinds.insert((identity.clone(), authoritative), kind)
            && existing_kind != kind
        {
            return Err(AssemblyLinkError::ClassDefinitionConflict {
                class: identity.1.clone(),
                property: "value type authority".into(),
                existing: format!("{existing_kind} (authoritative={authoritative})"),
                incoming: format!("{kind} (authoritative={authoritative})"),
            });
        }
    }
    Ok(())
}

const fn class_kind_bit(is_valuetype: bool) -> u8 {
    if is_valuetype { 0b10 } else { 0b01 }
}

fn normalized_class_definition_shard(
    source: &Assembly,
    class: ClassDefIdx,
    overrides: &ClassKindOverrides,
) -> Assembly {
    let mut normalized = Assembly::default();
    let mut context = RelocateCtx::with_class_kind_overrides(source, overrides.clone());
    let definition = source
        .class_defs()
        .get(&class)
        .expect("normalized class-definition audit requires a definition")
        .clone_without_methods();
    normalized.translate_class_def(&mut context, class, &definition);
    normalized
}

/// Audits physical definitions that collapse to one CLR identity after authority normalization.
///
/// Each row is relocated in isolation into a fresh assembly with the complete authoritative-kind
/// projection, then the ordinary structural cross-shard preflight compares every pair. This reuses
/// the field/base/layout/member/method-body checks without ever invoking `ClassDef::merge_defs` on
/// the caller's assemblies, so every expected conflict remains recoverable and read-only.
fn validate_internal_normalized_definition_groups(
    assembly: &Assembly,
    groups: &BTreeMap<ClassDefinitionIdentity, Vec<ClassDefIdx>>,
    overrides: &ClassKindOverrides,
) -> Result<LinkPreflightStats, AssemblyLinkError> {
    let mut nested_preflight = LinkPreflightStats::default();
    for classes in groups.values().filter(|classes| classes.len() > 1) {
        nested_preflight.authority_normalized_class_definitions += classes.len();
        let mut classes = classes.iter().copied();
        let first = classes.next().expect("duplicate class group");
        let mut accumulated = normalized_class_definition_shard(assembly, first, overrides);
        for class in classes {
            let incoming = normalized_class_definition_shard(assembly, class, overrides);
            let stats = accumulated.try_link_in_place(incoming)?;
            nested_preflight.accumulate(stats.preflight);
        }
    }
    Ok(nested_preflight)
}

fn normalized_method_definition_shard(
    source: &Assembly,
    method: MethodDefIdx,
    overrides: &ClassKindOverrides,
) -> Assembly {
    let source_method = source.method_def(method).clone();
    let source_owner = source_method.class();
    let owner = source
        .class_defs()
        .get(&source_owner)
        .expect("method-definition normalization requires its owner ClassDef")
        .method_owner_identity_shell();

    let mut normalized = Assembly::default();
    let mut context = RelocateCtx::with_class_kind_overrides(source, overrides.clone());
    normalized.translate_class_def(&mut context, source_owner, &owner);
    let method = source_method.relocate(&mut context, &mut normalized);
    normalized.new_method(method);
    normalized
}

/// Detects method identities that are distinct only because one of their reachable ClassRefs has
/// the pre-authority value kind. Whole-assembly rebuild must never discover these by panicking in
/// `new_method`; normalize each graph independently and reuse structural method preflight first.
fn validate_internal_normalized_method_collisions(
    assembly: &Assembly,
    overrides: &ClassKindOverrides,
) -> Result<LinkPreflightStats, AssemblyLinkError> {
    if overrides.is_empty() {
        return Ok(LinkPreflightStats::default());
    }

    let mut methods: Vec<_> = assembly.method_defs().keys().copied().collect();
    methods.sort_by_cached_key(|method| assembly.method_semantic_key(*method));
    let method_count = methods.len();
    let mut groups = BTreeMap::<Vec<u8>, Vec<Assembly>>::new();
    for method in methods {
        let normalized = normalized_method_definition_shard(assembly, method, overrides);
        let normalized_method = *normalized
            .method_defs()
            .keys()
            .next()
            .expect("normalized method shard must contain its method");
        groups
            .entry(normalized.method_semantic_key(normalized_method))
            .or_default()
            .push(normalized);
    }

    let mut nested_preflight = LinkPreflightStats::default();
    nested_preflight.authority_normalized_method_definitions = method_count;
    for methods in groups.into_values().filter(|methods| methods.len() > 1) {
        let mut methods = methods.into_iter();
        let mut accumulated = methods.next().expect("duplicate method group");
        for method in methods {
            let stats = accumulated.try_link_in_place(method)?;
            nested_preflight.accumulate(stats.preflight);
        }
    }
    Ok(nested_preflight)
}

/// Detects static-field identities that collapse only after authoritative class-kind projection.
/// The whole-assembly rebuild is allowed to deduplicate compatible rows, but must reject different
/// defaults, TLS/const flags, or attached metadata before touching either caller-owned assembly.
fn validate_internal_normalized_static_field_collisions(
    assembly: &Assembly,
    overrides: &ClassKindOverrides,
) -> Result<LinkPreflightStats, AssemblyLinkError> {
    if overrides.is_empty() {
        return Ok(LinkPreflightStats::default());
    }
    let (_, normalized_fields) = build_static_field_index(assembly, overrides)?;
    Ok(LinkPreflightStats {
        authority_normalized_static_fields: normalized_fields,
        ..LinkPreflightStats::default()
    })
}

impl AssemblyLinkIndex {
    fn build(assembly: &Assembly) -> Result<(Self, LinkIndexBuildStats), AssemblyLinkError> {
        let mut build_stats = LinkIndexBuildStats::default();
        let mut index = Self::default();
        let mut classes: Vec<_> = assembly
            .iter_class_def_ids()
            .copied()
            .map(|class| {
                build_stats.class_definitions += 1;
                (class_definition_identity(assembly, class), class)
            })
            .collect();
        classes.sort_by(
            |(left_identity, left_class), (right_identity, right_class)| {
                left_identity
                    .cmp(right_identity)
                    .then_with(|| left_class.0.inner().cmp(&right_class.0.inner()))
            },
        );
        validate_class_definition_identity_groups(assembly, &classes)?;
        for (identity, class) in classes {
            let definition = &assembly[class];
            index
                .class_definitions
                .entry(identity.clone())
                .or_default()
                .push(class);
            if definition.is_valuetype_authoritative() {
                index
                    .authoritative_kinds
                    .insert(identity, definition.is_valuetype());
            }
        }

        for class in assembly.iter_class_ref_ids() {
            build_stats.class_references += 1;
            let identity = class_ref_definition_identity(assembly, class);
            *index.class_reference_kinds.entry(identity).or_default() |=
                class_kind_bit(assembly[class].is_valuetype());
        }
        for (identity, &authoritative_kind) in &index.authoritative_kinds {
            let kinds = index
                .class_reference_kinds
                .get(identity)
                .copied()
                .unwrap_or_default();
            if kinds & class_kind_bit(!authoritative_kind) != 0 {
                index
                    .required_kind_overrides
                    .insert(identity.clone(), authoritative_kind);
            }
        }
        let (class_members, class_member_count) =
            build_class_member_index(assembly, &index.authoritative_kinds)?;
        index.class_members = class_members;
        build_stats.class_members = class_member_count;
        let (static_fields, static_field_count) =
            build_static_field_index(assembly, &index.authoritative_kinds)?;
        index.static_fields = static_fields;
        build_stats.static_fields = static_field_count;

        for &method in assembly.method_defs().keys() {
            build_stats.method_definitions += 1;
            let key = assembly.method_semantic_key(method);
            let method_ref = &assembly[method.0];
            let method_name: &str = &assembly[method_ref.name()];
            if SPECIAL_METHOD_NAMES.contains(&method_name) {
                let definition = assembly
                    .method_defs()
                    .get(&method)
                    .expect("indexed method definition");
                let (info, semantic_graphs) = build_special_method_link_info(
                    assembly, definition, "indexed",
                )
                .map_err(|detail| AssemblyLinkError::MethodConflict {
                    class: class_name(assembly, method_ref.class()),
                    method: method_name.to_string(),
                    signature: signature_name(assembly, method_ref.sig()),
                    existing_access: format!("{:?}", definition.access()),
                    incoming_access: format!("{:?}", definition.access()),
                    detail,
                })?;
                build_stats.method_semantic_graphs += semantic_graphs;
                index.special_methods.insert(key.clone(), info);
            }
            index.method_definitions.insert(key, method);
        }

        for import in assembly.native_imports() {
            build_stats.native_imports += 1;
            if let Some(existing) = index
                .native_imports
                .insert(import.rust_symbol.clone(), import.clone())
                && existing != *import
            {
                return Err(AssemblyLinkError::NativeImportConflict {
                    symbol: import.rust_symbol.clone(),
                    existing,
                    incoming: import.clone(),
                });
            }
        }

        build_stats.normalization_preflight = validate_internal_normalized_definition_groups(
            assembly,
            &index.class_definitions,
            &index.authoritative_kinds,
        )?;
        Ok((index, build_stats))
    }

    fn merge_relocated(&mut self, source: &Self, context: &RelocateCtx<'_>) {
        for (identity, classes) in &source.class_definitions {
            let destination_classes = self.class_definitions.entry(identity.clone()).or_default();
            for class in classes {
                if let Some(relocated) = context.class_refs.get(class.0) {
                    let relocated = ClassDefIdx(relocated);
                    if !destination_classes.contains(&relocated) {
                        destination_classes.push(relocated);
                    }
                }
            }
            destination_classes.sort_unstable_by_key(|class| class.0.inner());
        }
        for (identity, kinds) in &source.class_reference_kinds {
            *self
                .class_reference_kinds
                .entry(identity.clone())
                .or_default() |= kinds;
        }
        self.authoritative_kinds
            .extend(source.authoritative_kinds.clone());
        self.required_kind_overrides
            .extend(source.required_kind_overrides.clone());
        for (key, method) in &source.method_definitions {
            if let Some(relocated) = context.method_refs.get(method.0) {
                self.method_definitions
                    .insert(key.clone(), MethodDefIdx::from_raw(relocated));
            }
        }
        for (key, incoming) in &source.special_methods {
            match self.special_methods.entry(key.clone()) {
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    merge_special_method_link_info(entry.get_mut(), incoming);
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(incoming.clone());
                }
            }
        }
        self.static_fields.extend(source.static_fields.clone());
        self.class_members.extend(source.class_members.clone());
        self.native_imports.extend(source.native_imports.clone());
    }
}

fn custom_attr_arg_semantic_key(
    assembly: &Assembly,
    argument: &super::class::CustomAttrArg,
) -> String {
    use super::class::CustomAttrArg;
    match argument {
        CustomAttrArg::Str(value) => format!("str:{:?}", &assembly[*value]),
        CustomAttrArg::Bool(value) => format!("bool:{value}"),
        CustomAttrArg::U8(value) => format!("u8:{value}"),
        CustomAttrArg::I32(value) => format!("i32:{value}"),
        CustomAttrArg::I64(value) => format!("i64:{value}"),
    }
}

fn custom_attr_semantic_key(
    assembly: &Assembly,
    attribute: &super::class::CustomAttrDef,
) -> String {
    let constructor_arguments: Vec<_> = attribute
        .ctor_args()
        .iter()
        .map(|argument| custom_attr_arg_semantic_key(assembly, argument))
        .collect();
    let mut named_arguments: Vec<_> = attribute
        .named_args()
        .iter()
        .map(|argument| {
            let kind = match argument.kind() {
                super::class::CustomAttrNamedArgKind::Field => 0_u8,
                super::class::CustomAttrNamedArgKind::Property => 1_u8,
            };
            (
                kind,
                assembly[argument.name()].to_string(),
                custom_attr_arg_semantic_key(assembly, argument.value()),
            )
        })
        .collect();
    // ECMA-335 named arguments are keyed by member kind and name. Their blob order is not part of
    // the attribute's meaning, so normalize it while preserving constructor-argument order.
    named_arguments.sort();
    format!(
        "{:?}:{constructor_arguments:?}:{named_arguments:?}",
        assembly.class_semantic_key(attribute.attr_type())
    )
}

type StaticFieldIdentity = (String, Vec<u8>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct StaticFieldMetadataKey {
    // Static storage fields are currently emitted `public static` unconditionally. Keep those
    // fixed flags in the key so adding configurable access/storage later cannot accidentally leave
    // link preflight structurally incomplete.
    access: &'static str,
    storage: &'static str,
    is_tls: bool,
    is_const: bool,
    default_value: Option<Vec<u8>>,
    custom_attributes: Vec<String>,
}

fn const_semantic_key(assembly: &Assembly, value: &Const) -> Vec<u8> {
    let mut canonical = Assembly::default();
    let mut context = RelocateCtx::new(assembly);
    let value = canonical.translate_const(&mut context, value);
    postcard::to_stdvec(&(canonical, value)).expect("canonical constant graph must serialize")
}

fn static_field_semantic_keys(
    assembly: &Assembly,
    definition: &ClassDef,
    field: &super::class::StaticFieldDef,
) -> (StaticFieldIdentity, StaticFieldMetadataKey) {
    let mut custom_attributes: Vec<_> = definition
        .field_custom_attributes(field.name, true)
        .map(|attribute| custom_attr_semantic_key(assembly, attribute))
        .collect();
    // CustomAttribute row order is not a CLR field semantic. Compare the emitted multiset so two
    // shards with the same attributes in a different construction order remain compatible.
    custom_attributes.sort();
    (
        (
            assembly[field.name].to_string(),
            assembly.type_semantic_key(field.tpe),
        ),
        StaticFieldMetadataKey {
            access: "Public",
            storage: "Static",
            is_tls: field.is_tls,
            is_const: field.is_const,
            default_value: field
                .default_value
                .as_ref()
                .map(|value| const_semantic_key(assembly, value)),
            custom_attributes,
        },
    )
}

fn static_field_conflict(
    assembly: &Assembly,
    class: ClassDefIdx,
    identity: &StaticFieldIdentity,
    existing: &StaticFieldMetadataKey,
    incoming: &StaticFieldMetadataKey,
) -> AssemblyLinkError {
    AssemblyLinkError::StaticFieldConflict {
        class: class_name(assembly, class.0),
        field: identity.0.clone(),
        field_type: format!("semantic-key={:?}", identity.1),
        existing: format!("{existing:?}"),
        incoming: format!("{incoming:?}"),
    }
}

/// Build the persistent static-field index while validating duplicates before map insertion can
/// collapse their identity. The owning logical class is part of the key, so repeated MainModule
/// shards never need to rebuild a map of every static accumulated so far.
fn index_class_static_fields(
    assembly: &Assembly,
    class: ClassDefIdx,
    indexed: &mut BTreeMap<(ClassDefinitionIdentity, StaticFieldIdentity), StaticFieldMetadataKey>,
) -> Result<(), AssemblyLinkError> {
    let definition = &assembly[class];
    let owner = class_definition_identity(assembly, class);
    for field in definition.static_fields() {
        let (identity, metadata) = static_field_semantic_keys(assembly, definition, field);
        if let Some(existing) = indexed.insert((owner.clone(), identity.clone()), metadata.clone())
            && existing != metadata
        {
            return Err(static_field_conflict(
                assembly, class, &identity, &existing, &metadata,
            ));
        }
    }
    Ok(())
}

fn build_static_field_index(
    assembly: &Assembly,
    overrides: &ClassKindOverrides,
) -> Result<
    (
        BTreeMap<(ClassDefinitionIdentity, StaticFieldIdentity), StaticFieldMetadataKey>,
        usize,
    ),
    AssemblyLinkError,
> {
    let mut visited = 0;
    let mut indexed = BTreeMap::new();
    let mut classes: Vec<_> = assembly.iter_class_def_ids().copied().collect();
    classes.sort_unstable_by_key(|class| class.0.inner());
    for class in classes {
        let field_count = assembly[class].static_fields().len();
        if field_count == 0 {
            continue;
        }
        visited += field_count;
        if overrides.is_empty() {
            index_class_static_fields(assembly, class, &mut indexed)?;
        } else {
            // Normalize the complete owner once. Relocating one isolated field at a time clones
            // and walks an n-field ClassDef n times whenever any authority override exists.
            let (normalized, normalized_class) =
                normalized_class_metadata_shard(assembly, class, overrides);
            index_class_static_fields(&normalized, normalized_class, &mut indexed)?;
        }
    }
    Ok((indexed, visited))
}

fn event_semantic_key(
    assembly: &Assembly,
    event: &super::class::EventDef,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        assembly.type_semantic_key(event.delegate()),
        assembly.method_ref_semantic_key(event.add()),
        assembly.method_ref_semantic_key(event.remove()),
    )
}

fn property_semantic_key(
    assembly: &Assembly,
    property: &super::class::PropertyDef,
) -> (
    Vec<u8>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<u8>,
    Vec<String>,
) {
    let mut custom_attributes: Vec<_> = property
        .custom_attributes()
        .iter()
        .map(|attribute| custom_attr_semantic_key(assembly, attribute))
        .collect();
    custom_attributes.sort();
    (
        assembly.type_semantic_key(property.tpe()),
        property
            .getter()
            .map(|method| assembly.method_ref_semantic_key(method)),
        property
            .setter()
            .map(|method| assembly.method_ref_semantic_key(method)),
        property.nullability(),
        custom_attributes,
    )
}

type EventMetadataKey = (Vec<u8>, Vec<u8>, Vec<u8>);
type PropertyMetadataKey = (
    Vec<u8>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<u8>,
    Vec<String>,
);

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum ClassMemberIdentity {
    Interface(Vec<u8>),
    Field(String),
    Event(String),
    Property(String),
    CustomAttribute(String),
    FieldCustomAttribute {
        field: String,
        is_static: bool,
        attribute: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ClassMemberMetadataKey {
    Presence,
    Field { tpe: Vec<u8>, offset: Option<u32> },
    Event(EventMetadataKey),
    Property(PropertyMetadataKey),
}

fn class_member_conflict(
    owner: &ClassDefinitionIdentity,
    identity: &ClassMemberIdentity,
    existing: &ClassMemberMetadataKey,
    incoming: &ClassMemberMetadataKey,
) -> AssemblyLinkError {
    match (identity, existing, incoming) {
        (
            ClassMemberIdentity::Field(field),
            ClassMemberMetadataKey::Field {
                tpe: existing_type,
                offset: existing_offset,
            },
            ClassMemberMetadataKey::Field {
                tpe: incoming_type,
                offset: incoming_offset,
            },
        ) => AssemblyLinkError::ClassFieldConflict {
            class: owner.1.clone(),
            field: field.clone(),
            existing_type: format!("semantic-key={existing_type:?}"),
            incoming_type: format!("semantic-key={incoming_type:?}"),
            existing_offset: *existing_offset,
            incoming_offset: *incoming_offset,
        },
        (ClassMemberIdentity::Event(name), _, _) | (ClassMemberIdentity::Property(name), _, _) => {
            AssemblyLinkError::ClassDefinitionConflict {
                class: owner.1.clone(),
                property: format!("member {name}"),
                existing: format!("{existing:?}"),
                incoming: format!("{incoming:?}"),
            }
        }
        _ => AssemblyLinkError::ClassDefinitionConflict {
            class: owner.1.clone(),
            property: format!("member {identity:?}"),
            existing: format!("{existing:?}"),
            incoming: format!("{incoming:?}"),
        },
    }
}

fn class_member_entries(
    assembly: &Assembly,
    definition: &ClassDef,
) -> Vec<(ClassMemberIdentity, ClassMemberMetadataKey)> {
    let mut entries = Vec::with_capacity(
        definition.implements().len()
            + definition.fields().len()
            + definition.events().len()
            + definition.properties().len()
            + definition.custom_attributes().len(),
    );
    entries.extend(definition.implements().iter().map(|interface| {
        (
            ClassMemberIdentity::Interface(assembly.class_semantic_key(*interface)),
            ClassMemberMetadataKey::Presence,
        )
    }));
    entries.extend(definition.fields().iter().map(|(tpe, name, offset)| {
        (
            ClassMemberIdentity::Field(assembly[*name].to_string()),
            ClassMemberMetadataKey::Field {
                tpe: assembly.type_semantic_key(*tpe),
                offset: *offset,
            },
        )
    }));
    entries.extend(definition.events().iter().map(|event| {
        (
            ClassMemberIdentity::Event(assembly[event.name()].to_string()),
            ClassMemberMetadataKey::Event(event_semantic_key(assembly, event)),
        )
    }));
    entries.extend(definition.properties().iter().map(|property| {
        (
            ClassMemberIdentity::Property(assembly[property.name()].to_string()),
            ClassMemberMetadataKey::Property(property_semantic_key(assembly, property)),
        )
    }));
    entries.extend(definition.custom_attributes().iter().map(|attribute| {
        (
            ClassMemberIdentity::CustomAttribute(custom_attr_semantic_key(assembly, attribute)),
            ClassMemberMetadataKey::Presence,
        )
    }));
    for (field, is_static, attributes) in definition.field_custom_attribute_groups() {
        let field = assembly[*field].to_string();
        entries.extend(attributes.iter().map(|attribute| {
            (
                ClassMemberIdentity::FieldCustomAttribute {
                    field: field.clone(),
                    is_static: *is_static,
                    attribute: custom_attr_semantic_key(assembly, attribute),
                },
                ClassMemberMetadataKey::Presence,
            )
        }));
    }
    entries
}

fn index_class_definition_members(
    assembly: &Assembly,
    class: ClassDefIdx,
    indexed: &mut BTreeMap<(ClassDefinitionIdentity, ClassMemberIdentity), ClassMemberMetadataKey>,
) -> Result<usize, AssemblyLinkError> {
    let definition = &assembly[class];
    let owner = class_definition_identity(assembly, class);
    let mut visited = 0;
    for (identity, metadata) in class_member_entries(assembly, definition) {
        visited += 1;
        if let Some(existing) = indexed.insert((owner.clone(), identity.clone()), metadata.clone())
            && existing != metadata
        {
            return Err(class_member_conflict(
                &owner, &identity, &existing, &metadata,
            ));
        }
    }
    Ok(visited)
}

fn normalized_class_metadata_shard(
    source: &Assembly,
    class: ClassDefIdx,
    overrides: &ClassKindOverrides,
) -> (Assembly, ClassDefIdx) {
    let definition = source[class].clone_without_methods();
    let mut normalized = Assembly::default();
    let mut context = RelocateCtx::with_class_kind_overrides(source, overrides.clone());
    // Relocate the raw metadata value directly rather than routing through `translate_class_def`:
    // commit-time duplicate filtering is safe only after preflight has compared every colliding
    // row. Applying it here would hide conflicting fields/members whose ClassRefs collapse under
    // the authority projection that this audit is meant to validate.
    let relocated = definition
        .relocate(&mut context, &mut normalized)
        .definition;
    let class = normalized
        .class_def(relocated)
        .expect("isolated normalized class metadata must define one owner");
    (normalized, class)
}

fn build_class_member_index(
    assembly: &Assembly,
    overrides: &ClassKindOverrides,
) -> Result<
    (
        BTreeMap<(ClassDefinitionIdentity, ClassMemberIdentity), ClassMemberMetadataKey>,
        usize,
    ),
    AssemblyLinkError,
> {
    let mut visited = 0;
    let mut indexed = BTreeMap::new();
    let mut classes: Vec<_> = assembly.iter_class_def_ids().copied().collect();
    classes.sort_unstable_by_key(|class| class.0.inner());
    for class in classes {
        let definition = &assembly[class];
        let member_count = definition.implements().len()
            + definition.fields().len()
            + definition.events().len()
            + definition.properties().len()
            + definition.custom_attributes().len()
            + definition
                .field_custom_attribute_groups()
                .iter()
                .map(|(_, _, attributes)| attributes.len())
                .sum::<usize>();
        if member_count == 0 {
            continue;
        }
        if overrides.is_empty() {
            visited += index_class_definition_members(assembly, class, &mut indexed)?;
        } else {
            let (normalized, class) = normalized_class_metadata_shard(assembly, class, overrides);
            visited += index_class_definition_members(&normalized, class, &mut indexed)?;
        }
    }
    Ok((indexed, visited))
}

fn retain_new_class_members(
    assembly: &Assembly,
    context: &mut RelocateCtx<'_>,
    owner: &ClassDefinitionIdentity,
    definition: &mut ClassDef,
) {
    let mut visited = 0;
    let mut committed = 0;
    definition.implements_mut().retain(|interface| {
        visited += 1;
        let identity = ClassMemberIdentity::Interface(assembly.class_semantic_key(*interface));
        let retained = context.seen_class_members.insert((owner.clone(), identity));
        committed += usize::from(retained);
        retained
    });
    definition.fields_mut().retain(|(_, name, _)| {
        visited += 1;
        let identity = ClassMemberIdentity::Field(assembly[*name].to_string());
        let retained = context.seen_class_members.insert((owner.clone(), identity));
        committed += usize::from(retained);
        retained
    });
    definition.events_mut().retain(|event| {
        visited += 1;
        let identity = ClassMemberIdentity::Event(assembly[event.name()].to_string());
        let retained = context.seen_class_members.insert((owner.clone(), identity));
        committed += usize::from(retained);
        retained
    });
    definition.properties_mut().retain(|property| {
        visited += 1;
        let identity = ClassMemberIdentity::Property(assembly[property.name()].to_string());
        let retained = context.seen_class_members.insert((owner.clone(), identity));
        committed += usize::from(retained);
        retained
    });
    definition.custom_attributes_mut().retain(|attribute| {
        visited += 1;
        let identity =
            ClassMemberIdentity::CustomAttribute(custom_attr_semantic_key(assembly, attribute));
        let retained = context.seen_class_members.insert((owner.clone(), identity));
        committed += usize::from(retained);
        retained
    });
    for (field, is_static, attributes) in definition.field_custom_attribute_groups_mut() {
        let field = assembly[*field].to_string();
        attributes.retain(|attribute| {
            visited += 1;
            let identity = ClassMemberIdentity::FieldCustomAttribute {
                field: field.clone(),
                is_static: *is_static,
                attribute: custom_attr_semantic_key(assembly, attribute),
            };
            let retained = context.seen_class_members.insert((owner.clone(), identity));
            committed += usize::from(retained);
            retained
        });
    }
    definition
        .field_custom_attribute_groups_mut()
        .retain(|(_, _, attributes)| !attributes.is_empty());
    context.stats.class_members_visited += visited;
    context.stats.class_members_committed += committed;
}

fn locals_semantic_key(
    assembly: &Assembly,
    locals: &[super::method::LocalDef],
) -> Vec<(Option<String>, Vec<u8>)> {
    locals
        .iter()
        .map(|(name, tpe)| {
            (
                name.map(|name| assembly[name].to_string()),
                assembly.type_semantic_key(assembly[*tpe]),
            )
        })
        .collect()
}

/// Canonicalizes one method and only the IR it reaches into an otherwise empty assembly.
///
/// This makes interned identifiers comparable across shards without cloning either parent
/// assembly. Cost is proportional to the duplicate method's reachable graph, and semantically
/// different referenced strings, types, methods, fields, constants, nodes, or roots remain present
/// in the serialized key rather than being compared by coincidentally equal arena indices.
fn method_definition_value_semantic_key(assembly: &Assembly, definition: &MethodDef) -> Vec<u8> {
    let mut canonical = Assembly::default();
    let mut context = RelocateCtx::new(assembly);
    let definition = definition.clone().relocate(&mut context, &mut canonical);
    postcard::to_stdvec(&(canonical, definition)).expect("canonical method graph must serialize")
}

#[cfg(test)]
thread_local! {
    static METHOD_DEFINITION_SEMANTIC_KEY_BUILDS: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
    static SPECIAL_METHOD_FRAGMENT_KEY_BUILDS: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

fn method_definition_semantic_key(assembly: &Assembly, method: MethodDefIdx) -> Vec<u8> {
    #[cfg(test)]
    METHOD_DEFINITION_SEMANTIC_KEY_BUILDS.with(|builds| builds.set(builds.get() + 1));
    method_definition_value_semantic_key(
        assembly,
        assembly
            .method_defs()
            .get(&method)
            .expect("semantic key requires a method definition"),
    )
}

#[cfg(test)]
fn reset_method_definition_semantic_key_builds() {
    METHOD_DEFINITION_SEMANTIC_KEY_BUILDS.with(|builds| builds.set(0));
}

#[cfg(test)]
fn method_definition_semantic_key_builds() -> usize {
    METHOD_DEFINITION_SEMANTIC_KEY_BUILDS.with(std::cell::Cell::get)
}

#[cfg(test)]
fn reset_special_method_fragment_key_builds() {
    SPECIAL_METHOD_FRAGMENT_KEY_BUILDS.with(|builds| builds.set(0));
}

#[cfg(test)]
fn special_method_fragment_key_builds() -> usize {
    SPECIAL_METHOD_FRAGMENT_KEY_BUILDS.with(std::cell::Cell::get)
}

fn method_definition_metadata_semantic_key(assembly: &Assembly, definition: &MethodDef) -> Vec<u8> {
    let mut metadata = definition.clone();
    *metadata.implementation_mut() = MethodImpl::Missing;
    method_definition_value_semantic_key(assembly, &metadata)
}

fn special_method_body_fragments(
    assembly: &Assembly,
    role: &str,
    blocks: &[super::BasicBlock],
) -> Result<(Vec<Vec<Interned<CILRoot>>>, Interned<CILRoot>), String> {
    if blocks.len() != 1 {
        return Err(format!(
            "{role} special-method body has {} blocks; only one-block fragments are mergeable",
            blocks.len()
        ));
    }
    let block = &blocks[0];
    if block.block_id() != 0 {
        return Err(format!(
            "{role} special-method entry block has id {}; expected 0",
            block.block_id()
        ));
    }
    if block.handler().is_some() {
        return Err(format!(
            "{role} special-method body has an exception handler; handler-free shape required"
        ));
    }
    let Some((&terminal, preceding)) = block.roots().split_last() else {
        return Err(format!(
            "{role} special-method body is empty; final void return required"
        ));
    };
    if assembly.get_root(terminal) != &CILRoot::VoidRet {
        return Err(format!(
            "{role} special-method body does not end in void return"
        ));
    }

    let mut fragments = Vec::new();
    let mut current = Vec::new();
    for root in preceding {
        match assembly.get_root(*root) {
            CILRoot::InitFragmentBoundary => {
                if current.is_empty() {
                    return Err(format!(
                        "{role} special-method body has an empty initializer fragment"
                    ));
                }
                fragments.push(std::mem::take(&mut current));
            }
            CILRoot::Ret(_)
            | CILRoot::VoidRet
            | CILRoot::Throw(_)
            | CILRoot::Branch(_)
            | CILRoot::ExitSpecialRegion { .. }
            | CILRoot::ReThrow
            | CILRoot::Unreachable(_) => {
                return Err(format!(
                    "{role} special-method body has a control-flow exit before its final void return"
                ));
            }
            _ => current.push(*root),
        }
    }
    if !current.is_empty() {
        fragments.push(current);
    } else if !fragments.is_empty() {
        return Err(format!(
            "{role} special-method body has an empty trailing initializer fragment"
        ));
    }
    Ok((fragments, terminal))
}

fn special_method_fragment_key(
    assembly: &Assembly,
    definition: &MethodDef,
    roots: Vec<Interned<CILRoot>>,
    ret: Interned<CILRoot>,
) -> Vec<u8> {
    #[cfg(test)]
    SPECIAL_METHOD_FRAGMENT_KEY_BUILDS.with(|builds| builds.set(builds.get() + 1));
    let MethodImpl::MethodBody { locals, .. } = definition.implementation() else {
        unreachable!("only method bodies have initializer fragments");
    };
    let mut roots = roots;
    roots.push(ret);
    let mut fragment = definition.clone();
    *fragment.implementation_mut() = MethodImpl::MethodBody {
        blocks: vec![BasicBlock::new(roots, 0, None)],
        locals: locals.clone(),
    };
    method_definition_value_semantic_key(assembly, &fragment)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SpecialMethodLinkInfo {
    Missing,
    MethodBody {
        metadata: Vec<u8>,
        locals: Vec<(Option<String>, Vec<u8>)>,
        fragments: Vec<Vec<u8>>,
    },
    Extern(Vec<u8>),
    Alias(Vec<u8>),
    RegionBody,
}

impl SpecialMethodLinkInfo {
    fn body_fragments(&self) -> Option<&[Vec<u8>]> {
        let Self::MethodBody { fragments, .. } = self else {
            return None;
        };
        Some(fragments)
    }
}

fn special_method_fragment_orders(
    destination: &AssemblyLinkIndex,
    source: &AssemblyLinkIndex,
) -> SpecialMethodFragmentOrders {
    source
        .special_methods
        .iter()
        .filter_map(|(key, incoming)| {
            let existing = destination.special_methods.get(key)?;
            Some((
                key.clone(),
                (
                    existing.body_fragments()?.to_vec(),
                    incoming.body_fragments()?.to_vec(),
                ),
            ))
        })
        .collect()
}

fn duplicate_static_fields(
    destination: &AssemblyLinkIndex,
    source: &AssemblyLinkIndex,
) -> BTreeSet<(ClassDefinitionIdentity, StaticFieldIdentity)> {
    source
        .static_fields
        .keys()
        .filter(|key| destination.static_fields.contains_key(*key))
        .cloned()
        .collect()
}

fn build_special_method_link_info(
    assembly: &Assembly,
    definition: &MethodDef,
    role: &str,
) -> Result<(SpecialMethodLinkInfo, usize), String> {
    match definition.implementation() {
        MethodImpl::Missing => Ok((SpecialMethodLinkInfo::Missing, 0)),
        MethodImpl::MethodBody { blocks, locals } => {
            let (fragments, ret) = special_method_body_fragments(assembly, role, blocks)?;
            let metadata = method_definition_metadata_semantic_key(assembly, definition);
            let fragment_keys = fragments
                .into_iter()
                .map(|roots| special_method_fragment_key(assembly, definition, roots, ret))
                .collect::<Vec<_>>();
            let builds = fragment_keys.len() + 1;
            Ok((
                SpecialMethodLinkInfo::MethodBody {
                    metadata,
                    locals: locals_semantic_key(assembly, locals),
                    fragments: fragment_keys,
                },
                builds,
            ))
        }
        MethodImpl::Extern { .. } => Ok((
            SpecialMethodLinkInfo::Extern(method_definition_value_semantic_key(
                assembly, definition,
            )),
            1,
        )),
        MethodImpl::AliasFor(_) => Ok((
            SpecialMethodLinkInfo::Alias(method_definition_value_semantic_key(
                assembly, definition,
            )),
            1,
        )),
        MethodImpl::RegionBody { .. } => Ok((SpecialMethodLinkInfo::RegionBody, 0)),
    }
}

fn special_method_link_info_conflict(
    existing: &SpecialMethodLinkInfo,
    incoming: &SpecialMethodLinkInfo,
) -> Option<String> {
    use SpecialMethodLinkInfo as Info;
    match (existing, incoming) {
        (Info::Missing, _) | (_, Info::Missing) => None,
        (
            Info::MethodBody {
                metadata: existing_metadata,
                locals: existing_locals,
                ..
            },
            Info::MethodBody {
                metadata: incoming_metadata,
                locals: incoming_locals,
                ..
            },
        ) => {
            if existing_metadata != incoming_metadata {
                Some("special-method metadata differs".into())
            } else if existing_locals != incoming_locals {
                Some("special-method locals differ".into())
            } else {
                None
            }
        }
        (Info::Extern(existing), Info::Extern(incoming)) => {
            (existing != incoming).then_some("special-method extern declarations differ".into())
        }
        (Info::Alias(existing), Info::Alias(incoming)) => {
            (existing != incoming).then_some("special-method aliases differ".into())
        }
        (Info::RegionBody, _) | (_, Info::RegionBody) => {
            Some("canonical exception-region special methods cannot be merged".into())
        }
        (existing, incoming) => Some(format!(
            "special-method implementation kinds are not mergeable: incoming={incoming:?}, existing={existing:?}"
        )),
    }
}

fn merge_special_method_link_info(
    existing: &mut SpecialMethodLinkInfo,
    incoming: &SpecialMethodLinkInfo,
) {
    use SpecialMethodLinkInfo as Info;
    match (&mut *existing, incoming) {
        (Info::Missing, incoming) => *existing = incoming.clone(),
        (_, Info::Missing) => {}
        (
            Info::MethodBody {
                fragments: existing_fragments,
                ..
            },
            Info::MethodBody {
                fragments: incoming_fragments,
                ..
            },
        ) => {
            existing_fragments.extend(incoming_fragments.iter().cloned());
            existing_fragments.sort();
        }
        _ => {
            // Preflight already proved equal compatible non-body implementations.
        }
    }
}

fn merge_special_method_bodies(
    assembly: &mut Assembly,
    mut first: MethodDef,
    second: MethodDef,
    cached_orders: Option<&(Vec<Vec<u8>>, Vec<Vec<u8>>)>,
) -> MethodDef {
    let (first_fragments, first_ret) = match first.implementation() {
        MethodImpl::MethodBody { blocks, .. } => {
            special_method_body_fragments(assembly, "existing", blocks)
                .expect("special-method shape was checked during preflight")
        }
        _ => {
            first
                .implementation_mut()
                .merge_cctor_impls(second.implementation(), assembly);
            return first;
        }
    };
    let (second_fragments, second_ret) = match second.implementation() {
        MethodImpl::MethodBody { blocks, .. } => {
            special_method_body_fragments(assembly, "incoming", blocks)
                .expect("special-method shape was checked during preflight")
        }
        _ => {
            first
                .implementation_mut()
                .merge_cctor_impls(second.implementation(), assembly);
            return first;
        }
    };
    let MethodImpl::MethodBody { locals, .. } = first.implementation() else {
        unreachable!();
    };
    let locals = locals.clone();

    let (first_keys, second_keys) = cached_orders
        .filter(|(first_keys, second_keys)| {
            first_keys.len() == first_fragments.len() && second_keys.len() == second_fragments.len()
        })
        .cloned()
        .unwrap_or_else(|| {
            (
                first_fragments
                    .iter()
                    .cloned()
                    .map(|roots| special_method_fragment_key(assembly, &first, roots, first_ret))
                    .collect(),
                second_fragments
                    .iter()
                    .cloned()
                    .map(|roots| special_method_fragment_key(assembly, &second, roots, second_ret))
                    .collect(),
            )
        });
    let mut keyed = first_keys
        .into_iter()
        .zip(first_fragments)
        .chain(second_keys.into_iter().zip(second_fragments))
        .collect::<Vec<_>>();
    keyed.sort_by(|(left, _), (right, _)| left.cmp(right));

    let boundary = assembly.alloc_root(CILRoot::InitFragmentBoundary);
    let ret = assembly.alloc_root(CILRoot::VoidRet);
    let mut roots = Vec::new();
    for (index, (_, fragment)) in keyed.into_iter().enumerate() {
        if index != 0 {
            roots.push(boundary);
        }
        roots.extend(fragment);
    }
    roots.push(ret);
    let blocks = vec![BasicBlock::new(roots, 0, None)];
    *first.implementation_mut() = MethodImpl::MethodBody { blocks, locals };
    first
}

/// Checks every expected cross-shard conflict before relocation mutates the destination.
///
/// This deliberately does not convert internal relocation assertions into recoverable errors:
/// those remain fail-stop invariants. A returned error, however, is guaranteed to have been found
/// through read-only inspection, so callers may retain and continue using `destination`.
#[cfg(test)]
pub(crate) fn preflight_assembly_link(
    destination: &Assembly,
    source: &Assembly,
) -> Result<AssemblyLinkPlan, AssemblyLinkError> {
    let (destination_index, destination_build) = AssemblyLinkIndex::build(destination)?;
    let (source_index, source_build) = AssemblyLinkIndex::build(source)?;
    let mut stats = LinkPreflightStats::default();
    stats.record_destination_index_build(destination_build);
    stats.record_source_index_build(source_build);
    preflight_assembly_link_indexed(
        destination,
        source,
        &destination_index,
        &source_index,
        stats,
    )
}

/// Cached entry point used by repeated in-place shard commits.
pub(crate) fn preflight_assembly_link_cached(
    destination: &mut Assembly,
    source: &mut Assembly,
) -> Result<AssemblyLinkPlan, AssemblyLinkError> {
    let mut stats = LinkPreflightStats::default();
    // The incoming shard is untrusted and normally tiny. Validate it before doing even the
    // one-time index build for an arbitrary accumulated destination.
    if source.link_preflight_index.is_none() {
        let (index, build) = AssemblyLinkIndex::build(source)?;
        source.link_preflight_index = Some(Box::new(index));
        stats.record_source_index_build(build);
    }
    if destination.link_preflight_index.is_none() {
        let (index, build) = AssemblyLinkIndex::build(destination)?;
        stats.record_destination_index_build(build);
        destination.link_preflight_index = Some(Box::new(index));
    }
    let destination_index = destination
        .link_preflight_index
        .as_deref()
        .expect("destination link index was just initialized");
    let source_index = source
        .link_preflight_index
        .as_deref()
        .expect("source link index was just initialized");
    preflight_assembly_link_indexed(destination, source, destination_index, source_index, stats)
}

fn preflight_assembly_link_indexed(
    destination: &Assembly,
    source: &Assembly,
    destination_index: &AssemblyLinkIndex,
    source_index: &AssemblyLinkIndex,
    mut stats: LinkPreflightStats,
) -> Result<AssemblyLinkPlan, AssemblyLinkError> {
    for incoming in source_index.native_imports.values() {
        stats.source_native_import_preflight_visits += 1;
        stats.destination_identity_probes += 1;
        if let Some(existing) = destination_index.native_imports.get(&incoming.rust_symbol)
            && existing != incoming
        {
            return Err(AssemblyLinkError::NativeImportConflict {
                symbol: incoming.rust_symbol.clone(),
                existing: existing.clone(),
                incoming: incoming.clone(),
            });
        }
    }

    for (identity, &incoming_kind) in &source_index.authoritative_kinds {
        stats.source_authoritative_kind_preflight_visits += 1;
        stats.destination_identity_probes += 1;
        if let Some(&existing_kind) = destination_index.authoritative_kinds.get(identity)
            && existing_kind != incoming_kind
        {
            return Err(AssemblyLinkError::ClassDefinitionConflict {
                class: identity.1.clone(),
                property: "value type authority".into(),
                existing: format!("{existing_kind} (authoritative=true)"),
                incoming: format!("{incoming_kind} (authoritative=true)"),
            });
        }
    }

    let mut plan = AssemblyLinkPlan::default();
    for (identity, &kind) in &destination_index.required_kind_overrides {
        stats.destination_required_kind_override_preflight_visits += 1;
        plan.class_kind_overrides.insert(identity.clone(), kind);
    }
    for (identity, &kind) in &source_index.required_kind_overrides {
        stats.source_required_kind_override_preflight_visits += 1;
        plan.class_kind_overrides.insert(identity.clone(), kind);
    }
    for (identity, &canonical_kind) in &source_index.authoritative_kinds {
        stats.source_authoritative_kind_preflight_visits += 1;
        stats.destination_identity_probes += 1;
        let destination_kinds = destination_index
            .class_reference_kinds
            .get(identity)
            .copied()
            .unwrap_or_default();
        if destination_kinds & class_kind_bit(!canonical_kind) != 0 {
            plan.class_kind_overrides
                .insert(identity.clone(), canonical_kind);
        }
    }
    for (identity, &source_kinds) in &source_index.class_reference_kinds {
        stats.source_class_reference_kind_preflight_visits += 1;
        stats.destination_identity_probes += 1;
        if let Some(&canonical_kind) = destination_index.authoritative_kinds.get(identity)
            && source_kinds & class_kind_bit(!canonical_kind) != 0
        {
            plan.class_kind_overrides
                .insert(identity.clone(), canonical_kind);
        }
    }

    for (identity, incoming_ids) in &source_index.class_definitions {
        for &incoming_id in incoming_ids {
            stats.source_class_definition_preflight_visits += 1;
            stats.destination_identity_probes += 1;
            let Some(existing_id) = destination_index
                .class_definitions
                .get(identity)
                .and_then(|classes| classes.first())
                .copied()
            else {
                continue;
            };
            let incoming = &source[incoming_id];
            let existing = &destination[existing_id];
            if existing.is_valuetype() != incoming.is_valuetype()
                && !destination_index.authoritative_kinds.contains_key(identity)
                && !source_index.authoritative_kinds.contains_key(identity)
            {
                return Err(AssemblyLinkError::ClassDefinitionConflict {
                    class: source[incoming.name()].to_string(),
                    property: "value type authority".into(),
                    existing: format!("{} (authoritative=false)", existing.is_valuetype()),
                    incoming: format!("{} (authoritative=false)", incoming.is_valuetype()),
                });
            }
        }
    }

    // Kind reconciliation changes the semantic keys of every reference to the definition. Rebuild
    // both shards first, then run the complete conflict audit on those normalized graphs.
    if plan.requires_class_kind_rebuild() {
        stats.accumulate(validate_internal_normalized_method_collisions(
            destination,
            &plan.class_kind_overrides,
        )?);
        stats.accumulate(validate_internal_normalized_method_collisions(
            source,
            &plan.class_kind_overrides,
        )?);
        stats.accumulate(validate_internal_normalized_static_field_collisions(
            destination,
            &plan.class_kind_overrides,
        )?);
        stats.accumulate(validate_internal_normalized_static_field_collisions(
            source,
            &plan.class_kind_overrides,
        )?);
        plan.preflight_stats = stats;
        return Ok(plan);
    }

    for ((owner, identity), incoming_metadata) in &source_index.static_fields {
        stats.source_static_field_preflight_visits += 1;
        stats.destination_identity_probes += 1;
        let Some(existing_metadata) = destination_index
            .static_fields
            .get(&(owner.clone(), identity.clone()))
        else {
            continue;
        };
        stats.cross_static_field_comparisons += 1;
        if existing_metadata != incoming_metadata {
            return Err(AssemblyLinkError::StaticFieldConflict {
                class: owner.1.clone(),
                field: identity.0.clone(),
                field_type: format!("semantic-key={:?}", identity.1),
                existing: format!("{existing_metadata:?}"),
                incoming: format!("{incoming_metadata:?}"),
            });
        }
    }

    for ((owner, identity), incoming_metadata) in &source_index.class_members {
        stats.source_class_member_preflight_visits += 1;
        stats.destination_identity_probes += 1;
        let Some(existing_metadata) = destination_index
            .class_members
            .get(&(owner.clone(), identity.clone()))
        else {
            continue;
        };
        stats.cross_class_member_comparisons += 1;
        if existing_metadata != incoming_metadata {
            return Err(class_member_conflict(
                owner,
                identity,
                existing_metadata,
                incoming_metadata,
            ));
        }
    }

    for (incoming_key, incoming_ids) in &source_index.class_definitions {
        for &incoming_id in incoming_ids {
            stats.source_class_definition_preflight_visits += 1;
            stats.destination_identity_probes += 1;
            let Some(existing_id) = destination_index
                .class_definitions
                .get(incoming_key)
                .and_then(|classes| classes.first())
                .copied()
            else {
                continue;
            };
            stats.cross_class_definition_comparisons += 1;
            let incoming = source
                .class_defs()
                .get(&incoming_id)
                .expect("snapshotted incoming class definition");
            let existing = destination
                .class_defs()
                .get(&existing_id)
                .expect("snapshotted destination class definition");
            let class = source[incoming.name()].to_string();

            debug_assert_eq!(existing.is_valuetype(), incoming.is_valuetype());

            if existing.generics() != incoming.generics() {
                return Err(AssemblyLinkError::ClassDefinitionConflict {
                    class,
                    property: "generic arity".into(),
                    existing: existing.generics().to_string(),
                    incoming: incoming.generics().to_string(),
                });
            }
            let existing_generic_names: Vec<_> = existing
                .generic_names()
                .iter()
                .map(|name| destination[*name].to_string())
                .collect();
            let incoming_generic_names: Vec<_> = incoming
                .generic_names()
                .iter()
                .map(|name| source[*name].to_string())
                .collect();
            if existing_generic_names != incoming_generic_names {
                return Err(AssemblyLinkError::ClassDefinitionConflict {
                    class,
                    property: "generic parameter names".into(),
                    existing: format!("{existing_generic_names:?}"),
                    incoming: format!("{incoming_generic_names:?}"),
                });
            }
            if existing.is_interface() != incoming.is_interface() {
                return Err(AssemblyLinkError::ClassDefinitionConflict {
                    class,
                    property: "interface kind".into(),
                    existing: existing.is_interface().to_string(),
                    incoming: incoming.is_interface().to_string(),
                });
            }
            if let (Some(existing_enum), Some(incoming_enum)) =
                (existing.enum_def(), incoming.enum_def())
                && enum_semantic_key(destination, existing_enum)
                    != enum_semantic_key(source, incoming_enum)
            {
                return Err(AssemblyLinkError::ClassDefinitionConflict {
                    class,
                    property: "enum metadata".into(),
                    existing: format!("{:?}", enum_semantic_key(destination, existing_enum)),
                    incoming: format!("{:?}", enum_semantic_key(source, incoming_enum)),
                });
            }
            if let (Some(existing_size), Some(incoming_size)) =
                (existing.explict_size(), incoming.explict_size())
                && existing_size != incoming_size
            {
                return Err(AssemblyLinkError::ClassDefinitionConflict {
                    class,
                    property: "explicit size".into(),
                    existing: existing_size.to_string(),
                    incoming: incoming_size.to_string(),
                });
            }
            if let (Some(existing_align), Some(incoming_align)) =
                (existing.align(), incoming.align())
                && existing_align != incoming_align
            {
                return Err(AssemblyLinkError::ClassDefinitionConflict {
                    class,
                    property: "alignment".into(),
                    existing: existing_align.to_string(),
                    incoming: incoming_align.to_string(),
                });
            }
            if let (Some(existing_layout), Some(incoming_layout)) =
                (existing.fixed_array_layout(), incoming.fixed_array_layout())
                && fixed_array_semantic_key(destination, existing_layout)
                    != fixed_array_semantic_key(source, incoming_layout)
            {
                return Err(AssemblyLinkError::ClassDefinitionConflict {
                    class,
                    property: "fixed-array provenance".into(),
                    existing: format!(
                        "{:?}",
                        fixed_array_semantic_key(destination, existing_layout)
                    ),
                    incoming: format!("{:?}", fixed_array_semantic_key(source, incoming_layout)),
                });
            }
            if existing.access() != incoming.access() {
                return Err(AssemblyLinkError::ClassDefinitionConflict {
                    class,
                    property: "access".into(),
                    existing: format!("{:?}", existing.access()),
                    incoming: format!("{:?}", incoming.access()),
                });
            }

            if let (Some(existing_base), Some(incoming_base)) =
                (existing.extends(), incoming.extends())
                && destination.class_semantic_key(existing_base)
                    != source.class_semantic_key(incoming_base)
            {
                return Err(AssemblyLinkError::ClassBaseConflict {
                    class,
                    existing_base: class_name(destination, existing_base),
                    incoming_base: class_name(source, incoming_base),
                });
            }
        }
    }

    for (incoming_key, &incoming_id) in &source_index.method_definitions {
        stats.source_method_definition_preflight_visits += 1;
        stats.destination_identity_probes += 1;
        let Some(existing_id) = destination_index
            .method_definitions
            .get(incoming_key)
            .copied()
        else {
            continue;
        };
        stats.cross_method_definition_comparisons += 1;
        let incoming = source
            .method_defs()
            .get(&incoming_id)
            .expect("snapshotted incoming method definition");
        let existing = destination
            .method_defs()
            .get(&existing_id)
            .expect("snapshotted destination method definition");
        let method_ref = &source[incoming_id.0];
        let method_name: &str = &source[method_ref.name()];
        let detail = if existing.access() != incoming.access() {
            Some("method access differs".to_string())
        } else if SPECIAL_METHOD_NAMES.contains(&method_name) {
            let existing_info = destination_index
                .special_methods
                .get(incoming_key)
                .expect("indexed destination special method");
            let incoming_info = source_index
                .special_methods
                .get(incoming_key)
                .expect("indexed source special method");
            special_method_link_info_conflict(existing_info, incoming_info)
        } else if !matches!(existing.implementation(), MethodImpl::Missing)
            && !matches!(incoming.implementation(), MethodImpl::Missing)
        {
            stats.method_definition_semantic_keys_built += 2;
            (method_definition_semantic_key(destination, existing_id)
                != method_definition_semantic_key(source, incoming_id))
            .then_some("competing real method implementations differ".into())
        } else {
            None
        };
        if let Some(detail) = detail {
            return Err(AssemblyLinkError::MethodConflict {
                class: class_name(source, method_ref.class()),
                method: method_name.to_string(),
                signature: signature_name(source, method_ref.sig()),
                existing_access: format!("{:?}", existing.access()),
                incoming_access: format!("{:?}", incoming.access()),
                detail,
            });
        }
    }

    plan.preflight_stats = stats;
    Ok(plan)
}

pub(crate) fn relocate_assembly(
    mut destination: Assembly,
    source: &Assembly,
) -> (Assembly, RelocationStats) {
    source.assert_relocation_arena_coverage();
    destination.assert_relocation_arena_coverage();
    let mut destination_index = destination.link_preflight_index.take().map(|index| *index);
    let original_str = destination.alloc_string(super::asm::MAIN_MODULE);
    let mut class_ids: Vec<_> = source.iter_class_def_ids().copied().collect();
    class_ids.sort_unstable_by_key(|class_id| class_id.0.inner());
    let mut ctx = RelocateCtx::new(source);
    if let (Some(destination_index), Some(source_index)) = (
        destination_index.as_ref(),
        source.link_preflight_index.as_deref(),
    ) {
        ctx.special_method_fragment_orders =
            special_method_fragment_orders(destination_index, source_index);
        ctx.duplicate_static_fields = duplicate_static_fields(destination_index, source_index);
        ctx.seen_class_members
            .extend(destination_index.class_members.keys().cloned());
    }
    for class_id in class_ids {
        let def = source
            .class_defs()
            .get(&class_id)
            .expect("snapshotted source class definition");
        // `translate_class_def` owns the complete class relocation transaction: it inserts or
        // merges the translated definition and then relocates its methods. Re-merging the returned
        // snapshot here was redundant for identical layouts and actively wrong when a definition
        // had already been normalized in the destination, because the second merge compared the
        // same logical field through two physical-offset snapshots.
        destination.translate_class_def(&mut ctx, class_id, def);
    }
    assert_eq!(
        destination.alloc_string(super::asm::MAIN_MODULE),
        original_str
    );
    if let (Some(destination_index), Some(source_index)) = (
        destination_index.as_mut(),
        source.link_preflight_index.as_deref(),
    ) {
        destination_index.merge_relocated(source_index, &ctx);
    }
    destination.link_preflight_index = destination_index.map(Box::new);
    (destination, ctx.stats)
}

/// Rebuilds every definition and reachable IR reference with selected class-definition kinds.
/// Used only by the rare cross-shard placeholder/authority reconciliation path.
pub(crate) fn relocate_assembly_with_class_kind_overrides(
    mut destination: Assembly,
    source: &Assembly,
    class_kind_overrides: ClassKindOverrides,
) -> (Assembly, RelocationStats) {
    source.assert_relocation_arena_coverage();
    destination.assert_relocation_arena_coverage();
    let mut class_ids: Vec<_> = source.iter_class_def_ids().copied().collect();
    class_ids.sort_unstable_by_key(|class_id| class_id.0.inner());
    let mut ctx = RelocateCtx::with_class_kind_overrides(source, class_kind_overrides);
    for class_id in class_ids {
        let definition = source
            .class_defs()
            .get(&class_id)
            .expect("snapshotted source class definition");
        destination.translate_class_def(&mut ctx, class_id, definition);
    }
    (destination, ctx.stats)
}

/// Rebuild an assembly while projecting the internal `MainModule` sentinel to one public CLR type.
///
/// This is a final-link operation: every class/method/field/type reference is relocated through one
/// mapping, so a definition and all of its call sites stay coherent. It deliberately does not alter
/// artifacts that did not opt into a managed identity.
pub(crate) fn relocate_assembly_with_main_module_name(
    mut destination: Assembly,
    source: &Assembly,
    main_module_name: &str,
) -> (Assembly, RelocationStats) {
    source.assert_relocation_arena_coverage();
    destination.assert_relocation_arena_coverage();
    let mut class_ids: Vec<_> = source.iter_class_def_ids().copied().collect();
    class_ids.sort_unstable_by_key(|class_id| class_id.0.inner());
    let mut ctx = RelocateCtx::with_main_module_name(source, Some(main_module_name));
    for class_id in class_ids {
        let def = source
            .class_defs()
            .get(&class_id)
            .expect("snapshotted source class definition");
        destination.translate_class_def(&mut ctx, class_id, def);
    }
    (destination, ctx.stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Access, BasicBlock, Const, ExceptionRegion, IString, Int, MethodDef, MethodImpl, Type,
        ir::cilnode::{BinOp, MethodKind},
        ir::class::{
            CustomAttrArg, CustomAttrDef, CustomAttrNamedArg, CustomAttrNamedArgKind, EventDef,
            PropertyDef, StaticFieldDef,
        },
    };
    use std::num::NonZeroU32;

    fn add_void_method(
        asm: &mut Assembly,
        name: &str,
        blocks: Vec<BasicBlock>,
    ) -> Interned<IString> {
        let owner = asm.main_module();
        let name = asm.alloc_string(name);
        let sig = asm.sig([], Type::Void);
        asm.new_method(MethodDef::new(
            Access::Public,
            owner,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks,
                locals: vec![],
            },
            vec![],
        ));
        name
    }

    fn seed_destination_ids(asm: &mut Assembly) {
        let _ = asm.alloc_string("destination-only-string");
        let _ = asm.alloc_type(Type::Int(Int::U16));
        let node = asm.alloc_node(Const::I32(7));
        let _ = asm.alloc_root(CILRoot::Pop(node));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        add_void_method(
            asm,
            "destination_only_method",
            vec![BasicBlock::new(vec![ret], 0, None)],
        );
    }

    #[test]
    fn linking_partial_class_definitions_preserves_instance_fields() {
        let mut destination = Assembly::default();
        let destination_name = destination.alloc_string("ShardDefinedType");
        destination
            .class_def(ClassDef::new(
                destination_name,
                true,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap();

        let mut source = Assembly::default();
        let source_name = source.alloc_string("ShardDefinedType");
        let payload = source.alloc_string("payload");
        source
            .class_def(ClassDef::new(
                source_name,
                true,
                0,
                None,
                vec![(Type::Int(Int::I32), payload, Some(0))],
                vec![],
                Access::Public,
                NonZeroU32::new(4),
                NonZeroU32::new(4),
                true,
            ))
            .unwrap();

        let linked = destination.link(source);
        let definition = linked
            .class_defs()
            .values()
            .find(|definition| &linked[definition.name()] == "ShardDefinedType")
            .expect("linked partial class definition");
        assert_eq!(definition.fields().len(), 1);
        assert_eq!(definition.fields()[0].0, Type::Int(Int::I32));
        assert_eq!(&linked[definition.fields()[0].1], "payload");
        assert_eq!(definition.fields()[0].2, Some(0));
        assert_eq!(definition.explict_size(), NonZeroU32::new(4));
        assert_eq!(definition.align(), NonZeroU32::new(4));
    }

    #[test]
    fn linking_partial_class_definitions_adopts_relocated_base() {
        let mut destination = Assembly::default();
        let destination_name = destination.alloc_string("ShardDerivedType");
        destination
            .class_def(ClassDef::new(
                destination_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap();

        let mut source = Assembly::default();
        let source_name = source.alloc_string("ShardDerivedType");
        let base_name = source.alloc_string("ManagedBase");
        let base = source.alloc_class_ref(ClassRef::new(base_name, None, false, [].into()));
        source
            .class_def(ClassDef::new(
                source_name,
                false,
                0,
                Some(base),
                vec![],
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap();

        let linked = destination.link(source);
        let definition = linked
            .class_defs()
            .values()
            .find(|definition| &linked[definition.name()] == "ShardDerivedType")
            .expect("linked partial class definition");
        let base = definition.extends().expect("authoritative base was lost");
        assert_eq!(&linked[linked[base].name()], "ManagedBase");
    }

    #[test]
    fn link_preserves_unresolved_basic_block_handler_id() {
        let mut destination = Assembly::default();
        seed_destination_ids(&mut destination);

        let mut source = Assembly::default();
        let ret = source.alloc_root(CILRoot::VoidRet);
        let source_name = add_void_method(
            &mut source,
            "source_with_unresolved_handler",
            vec![BasicBlock::new_raw(vec![ret], 17, Some(91))],
        );

        let linked = destination.link(source);
        let method = linked
            .method_defs()
            .values()
            .find(|method| &linked[method.name()] == "source_with_unresolved_handler")
            .expect("linked source method");
        assert_ne!(method.name().inner(), source_name.inner());
        let MethodImpl::MethodBody { blocks, .. } = method.implementation() else {
            panic!("source method must keep its body");
        };
        assert_eq!(blocks[0].block_id(), 17);
        assert_eq!(blocks[0].handler_id(), Some(91));
        assert!(blocks[0].handler().is_none());
    }

    #[test]
    fn link_relocates_canonical_region_body_without_changing_cfg_ids() {
        let mut destination = Assembly::default();
        seed_destination_ids(&mut destination);

        let mut source = Assembly::default();
        let owner = source.main_module();
        let sig = source.sig([], Type::Void);
        let name = source.alloc_string("source_region_body");
        let local_name = source.alloc_string("region_local");
        let local_type = source.alloc_type(Type::Int(Int::I64));
        let value = source.alloc_node(Const::I32(99));
        let normal_root = source.alloc_root(CILRoot::Pop(value));
        let cleanup_root = source.alloc_root(CILRoot::ReThrow);
        source.new_method(MethodDef::new(
            Access::Public,
            owner,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::RegionBody {
                blocks: vec![BasicBlock::new(vec![normal_root], 7, None)],
                cleanup_blocks: vec![BasicBlock::new(vec![cleanup_root], 41, None)],
                exception_regions: vec![ExceptionRegion::new(7, 41)],
                locals: vec![(Some(local_name), local_type)],
            },
            vec![],
        ));

        let linked = destination.link(source);
        let method = linked
            .method_defs()
            .values()
            .find(|method| &linked[method.name()] == "source_region_body")
            .expect("linked region method");
        let MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            exception_regions,
            locals,
        } = method.implementation()
        else {
            panic!("canonical region body must survive linking")
        };
        assert_eq!(blocks[0].block_id(), 7);
        assert_eq!(cleanup_blocks[0].block_id(), 41);
        assert_eq!(exception_regions, &[ExceptionRegion::new(7, 41)]);
        assert!(matches!(linked[blocks[0].roots()[0]], CILRoot::Pop(_)));
        assert_eq!(linked[cleanup_blocks[0].roots()[0]], CILRoot::ReThrow);
        assert_eq!(&linked[locals[0].0.expect("local name")], "region_local");
        assert_eq!(linked[locals[0].1], Type::Int(Int::I64));
    }

    #[test]
    fn link_preserves_valuetype_authority_with_relocated_ids() {
        let mut destination = Assembly::default();
        seed_destination_ids(&mut destination);

        let mut source = Assembly::default();
        let authoritative_name = source.alloc_string("AuthoritativeValueType");
        source
            .class_def(
                ClassDef::new(
                    authoritative_name,
                    true,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Public,
                    None,
                    None,
                    true,
                )
                .with_valuetype_authoritative(),
            )
            .unwrap();
        let placeholder_name = source.alloc_string("NonAuthoritativePlaceholder");
        source
            .class_def(ClassDef::new(
                placeholder_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap();

        let linked = destination.link(source);
        let authoritative = linked
            .class_defs()
            .values()
            .find(|def| &linked[def.name()] == "AuthoritativeValueType")
            .expect("linked authoritative type");
        assert_ne!(authoritative.name().inner(), authoritative_name.inner());
        assert!(authoritative.is_valuetype());
        let mut authoritative = authoritative.clone();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                authoritative.set_is_valuetype(false);
            }))
            .is_err()
        );

        let placeholder = linked
            .class_defs()
            .values()
            .find(|def| &linked[def.name()] == "NonAuthoritativePlaceholder")
            .expect("linked placeholder type");
        let mut placeholder = placeholder.clone();
        placeholder.set_is_valuetype(true);
        assert!(placeholder.is_valuetype());
    }

    #[test]
    fn link_relocates_shared_node_dag_once() {
        const DEPTH: usize = 20;

        let mut destination = Assembly::default();
        seed_destination_ids(&mut destination);

        let mut source = Assembly::default();
        let mut top = source.alloc_node(Const::I32(11));
        for _ in 0..DEPTH {
            top = source.alloc_node(CILNode::BinOp(top, top, BinOp::Add));
        }
        let source_top = top;
        let pop = source.alloc_root(CILRoot::Pop(top));
        let ret = source.alloc_root(CILRoot::VoidRet);
        add_void_method(
            &mut source,
            "source_with_shared_node_dag",
            vec![BasicBlock::new(vec![pop, pop, ret], 0, None)],
        );

        let (linked, stats) = destination.link_with_stats(source);
        let method = linked
            .method_defs()
            .values()
            .find(|method| &linked[method.name()] == "source_with_shared_node_dag")
            .expect("linked source method");
        let MethodImpl::MethodBody { blocks, .. } = method.implementation() else {
            panic!("source method must keep its body");
        };
        assert_eq!(blocks[0].roots()[0], blocks[0].roots()[1]);
        let CILRoot::Pop(relocated_top) = linked.get_root(blocks[0].roots()[0]) else {
            panic!("source method must keep its pop root");
        };
        assert_ne!(relocated_top.inner(), source_top.inner());
        assert_eq!(stats.nodes.unique_visits, DEPTH + 1);
        assert_eq!(stats.nodes.cache_hits, DEPTH);
        assert_eq!(stats.roots.unique_visits, 2);
        assert_eq!(stats.roots.cache_hits, 1);
    }

    #[test]
    fn link_orders_class_definitions_by_source_id() {
        let mut source = Assembly::default();
        let mut class_ids = Vec::new();
        for name in ["Alpha", "Beta", "Gamma"] {
            let name = source.alloc_string(name);
            class_ids.push(
                source
                    .class_def(ClassDef::new(
                        name,
                        false,
                        0,
                        None,
                        vec![],
                        vec![],
                        Access::Public,
                        None,
                        None,
                        true,
                    ))
                    .unwrap(),
            );
        }

        let mut reordered = source.clone();
        let (defs, _) = reordered.class_defs_mut_strings();
        let mut removed: Vec<_> = class_ids
            .iter()
            .map(|id| (*id, defs.remove(id).expect("source class definition")))
            .collect();
        for (id, def) in removed.drain(..).rev() {
            defs.insert(id, def);
        }

        let linked = Assembly::default().link(source);
        let reordered_linked = Assembly::default().link(reordered);
        assert_eq!(
            postcard::to_allocvec(&linked).unwrap(),
            postcard::to_allocvec(&reordered_linked).unwrap()
        );

        let mut linked_classes: Vec<_> = linked
            .class_defs()
            .iter()
            .map(|(id, def)| (id.0.inner(), linked[def.name()].to_string()))
            .collect();
        linked_classes.sort_unstable_by_key(|(id, _)| *id);
        assert_eq!(
            linked_classes
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>(),
            ["Alpha", "Beta", "Gamma"]
        );
    }

    #[test]
    fn compact_keeps_live_graph_and_removes_unreachable_arena_values() {
        let mut asm = Assembly::default();
        let owner = asm.main_module();

        let live_type = asm.alloc_type(Type::Int(Int::U32));
        let live_data = asm.alloc_const_data(&[1, 2, 3, 4]);
        let live_buffer = asm.alloc_node(Const::ByteBuffer {
            data: live_data,
            tpe: live_type,
        });
        let live_field_name = asm.alloc_string("live_field");
        let live_field =
            asm.alloc_field(FieldDesc::new(*owner, live_field_name, Type::Int(Int::I32)));
        let live_addr = asm.alloc_node(Const::Null(*owner));
        let live_field_load = asm.alloc_node(CILNode::LdField {
            addr: live_addr,
            field: live_field,
        });
        let live_static_name = asm.alloc_string("live_static");
        let live_static = asm.alloc_sfld(StaticFieldDesc::new(
            *owner,
            live_static_name,
            Type::Int(Int::I32),
        ));
        let live_static_load = asm.alloc_node(CILNode::LdStaticField(live_static));

        let shared_root = asm.alloc_root(CILRoot::Pop(live_buffer));
        let field_root = asm.alloc_root(CILRoot::Pop(live_field_load));
        let static_root = asm.alloc_root(CILRoot::Pop(live_static_load));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        add_void_method(
            &mut asm,
            "live_method",
            vec![BasicBlock::new(
                vec![shared_root, shared_root, field_root, static_root, ret],
                0,
                None,
            )],
        );
        asm.add_section("compaction-test", [9, 8, 7]);

        let junk_string = asm.alloc_string("unreachable");
        let junk_type = asm.alloc_type(Type::Int(Int::U16));
        let junk_class =
            asm.alloc_class_ref(ClassRef::new(junk_string, None, false, vec![].into()));
        let junk_sig = asm.sig([Type::Int(Int::U8)], Type::Int(Int::U8));
        let _junk_method = asm.alloc_methodref(MethodRef::new(
            junk_class,
            junk_string,
            junk_sig,
            MethodKind::Static,
            vec![].into(),
        ));
        let _junk_field =
            asm.alloc_field(FieldDesc::new(junk_class, junk_string, Type::Int(Int::U8)));
        let _junk_static = asm.alloc_sfld(StaticFieldDesc::new(
            junk_class,
            junk_string,
            Type::Int(Int::U8),
        ));
        let junk_data = asm.alloc_const_data(&[0xde, 0xad]);
        let junk_node = asm.alloc_node(Const::ByteBuffer {
            data: junk_data,
            tpe: junk_type,
        });
        let _junk_root = asm.alloc_root(CILRoot::Pop(junk_node));

        let (compacted, stats) = asm.compact();
        for (arena, before, after) in [
            ("strings", stats.before.strings, stats.after.strings),
            ("types", stats.before.types, stats.after.types),
            (
                "class refs",
                stats.before.class_refs,
                stats.after.class_refs,
            ),
            ("nodes", stats.before.nodes, stats.after.nodes),
            ("roots", stats.before.roots, stats.after.roots),
            (
                "signatures",
                stats.before.signatures,
                stats.after.signatures,
            ),
            (
                "method refs",
                stats.before.method_refs,
                stats.after.method_refs,
            ),
            ("fields", stats.before.fields, stats.after.fields),
            ("statics", stats.before.statics, stats.after.statics),
            (
                "const data",
                stats.before.const_data,
                stats.after.const_data,
            ),
        ] {
            assert_eq!(before, after + 1, "unexpected {arena} compaction");
        }
        assert_eq!(stats.before.class_defs, stats.after.class_defs);
        assert_eq!(stats.before.method_defs, stats.after.method_defs);
        assert_eq!(stats.before.sections, stats.after.sections);
        assert!(stats.relocation.roots.cache_hits >= 1);
        assert_eq!(
            compacted.get_section("compaction-test"),
            Some(&vec![9, 8, 7])
        );

        let method = compacted
            .method_defs()
            .values()
            .find(|method| &compacted[method.name()] == "live_method")
            .expect("live method survives compaction");
        let MethodImpl::MethodBody { blocks, .. } = method.implementation() else {
            panic!("live method must keep its body");
        };
        assert_eq!(blocks[0].roots()[0], blocks[0].roots()[1]);

        let once_bytes = postcard::to_allocvec(&compacted).unwrap();
        let (compacted_twice, second_stats) = compacted.compact();
        assert_eq!(second_stats.before, second_stats.after);
        assert_eq!(once_bytes, postcard::to_allocvec(&compacted_twice).unwrap());
    }

    #[test]
    fn link_relocates_all_owned_metadata_with_offset_ids() {
        let mut destination = Assembly::default();
        seed_destination_ids(&mut destination);

        let mut source = Assembly::default();
        let class_name = source.alloc_string("KitchenSink");
        let generic_name = source.alloc_string("TClass");
        let field_name = source.alloc_string("payload");
        let static_name = source.alloc_string("StaticText");
        let default_text = source.alloc_string("default-value");
        let base_name = source.alloc_string("KitchenBase");
        let base = source.alloc_class_ref(ClassRef::new(base_name, None, false, vec![].into()));
        let interface_name = source.alloc_string("IKitchen");
        let interface =
            source.alloc_class_ref(ClassRef::new(interface_name, None, false, vec![].into()));
        let mut class = ClassDef::new(
            class_name,
            true,
            1,
            Some(base),
            vec![(Type::Int(Int::I32), field_name, Some(4))],
            vec![StaticFieldDef {
                tpe: Type::PlatformString,
                name: static_name,
                is_tls: true,
                default_value: Some(Const::PlatformString(default_text)),
                is_const: true,
            }],
            Access::Public,
            NonZeroU32::new(32),
            NonZeroU32::new(8),
            false,
        )
        .with_valuetype_authoritative()
        .with_interface()
        .with_type_generic_names(vec![generic_name]);
        class.add_interface(interface);
        let class_id = source.class_def(class).unwrap();

        let accessor_sig = source.sig([], Type::Void);
        let add = source.new_methodref(
            *class_id,
            "add_Changed",
            accessor_sig,
            MethodKind::Virtual,
            vec![],
        );
        let remove = source.new_methodref(
            *class_id,
            "remove_Changed",
            accessor_sig,
            MethodKind::Virtual,
            vec![],
        );
        let getter = source.new_methodref(
            *class_id,
            "get_Value",
            accessor_sig,
            MethodKind::Virtual,
            vec![],
        );
        let setter = source.new_methodref(
            *class_id,
            "set_Value",
            accessor_sig,
            MethodKind::Virtual,
            vec![],
        );
        let delegate_name = source.alloc_string("KitchenDelegate");
        let delegate =
            source.alloc_class_ref(ClassRef::new(delegate_name, None, false, vec![].into()));
        let property_inner = source.alloc_type(Type::Int(Int::I16));
        let event_name = source.alloc_string("Changed");
        let property_name = source.alloc_string("Value");
        source.class_mut(class_id).add_event(EventDef::new(
            event_name,
            Type::ClassRef(delegate),
            add,
            remove,
        ));
        source.class_mut(class_id).add_property(PropertyDef::new(
            property_name,
            Type::Ref(property_inner),
            Some(getter),
            Some(setter),
        ));

        let attr_name = source.alloc_string("KitchenAttribute");
        let attr_type =
            source.alloc_class_ref(ClassRef::new(attr_name, None, false, vec![].into()));
        let ctor_text = source.alloc_string("ctor-text");
        let named_name = source.alloc_string("NamedText");
        let named_text = source.alloc_string("named-text");
        let named_field_name = source.alloc_string("NamedFlag");
        source
            .class_mut(class_id)
            .add_custom_attribute(CustomAttrDef::new_with_named_args(
                attr_type,
                vec![
                    CustomAttrArg::Str(ctor_text),
                    CustomAttrArg::Bool(true),
                    CustomAttrArg::I32(17),
                    CustomAttrArg::I64(29),
                ],
                vec![
                    CustomAttrNamedArg::property(named_name, CustomAttrArg::Str(named_text)),
                    CustomAttrNamedArg::field(named_field_name, CustomAttrArg::Bool(true)),
                ],
            ));

        let argument_inner = source.alloc_type(Type::Int(Int::I32));
        let method_sig = source.sig([Type::Ref(argument_inner)], Type::Void);
        let override_name = source.alloc_string("BaseVirtual");
        let override_method = source.alloc_methodref(MethodRef::new(
            base,
            override_name,
            method_sig,
            MethodKind::Virtual,
            vec![].into(),
        ));
        let local_name = source.alloc_string("local_value");
        let local_type = source.alloc_type(Type::Int(Int::U64));
        let argument_name = source.alloc_string("output");
        let method_generic = source.alloc_string("TMethod");
        let method_name = source.alloc_string("KitchenMethod");
        let ret = source.alloc_root(CILRoot::VoidRet);
        source.new_method(
            MethodDef::new(
                Access::Public,
                class_id,
                method_name,
                method_sig,
                MethodKind::Virtual,
                MethodImpl::MethodBody {
                    blocks: vec![BasicBlock::new_raw(vec![ret], 7, Some(41))],
                    locals: vec![(Some(local_name), local_type)],
                },
                vec![Some(argument_name)],
            )
            .with_override(override_method)
            .with_abstract()
            .with_out_params(vec![1])
            .with_generic_params(vec![method_generic])
            .with_special_name(),
        );

        let linked = destination.link(source);
        let (linked_class_id, linked_class) = linked
            .class_defs()
            .iter()
            .find(|(_, class)| &linked[class.name()] == "KitchenSink")
            .expect("linked kitchen-sink class");
        assert_ne!(linked_class.name().inner(), class_name.inner());
        assert!(linked_class.is_valuetype());
        assert!(linked_class.is_interface());
        assert_eq!(linked_class.generics(), 1);
        assert_eq!(&linked[linked_class.generic_names()[0]], "TClass");
        assert_eq!(*linked_class.access(), Access::Public);
        assert_eq!(linked_class.explict_size(), NonZeroU32::new(32));
        assert_eq!(linked_class.align(), NonZeroU32::new(8));
        assert!(!linked_class.has_nonveralpping_layout());
        let mut authority_probe = linked_class.clone();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                authority_probe.set_is_valuetype(false);
            }))
            .is_err()
        );

        let extends = linked_class.extends().expect("relocated base class");
        assert_eq!(&linked[linked.class_ref(extends).name()], "KitchenBase");
        assert_eq!(linked_class.implements().len(), 1);
        assert_eq!(
            &linked[linked.class_ref(linked_class.implements()[0]).name()],
            "IKitchen"
        );
        assert_eq!(linked_class.fields().len(), 1);
        assert_eq!(linked_class.fields()[0].0, Type::Int(Int::I32));
        assert_eq!(&linked[linked_class.fields()[0].1], "payload");
        assert_eq!(linked_class.fields()[0].2, Some(4));
        let static_field = &linked_class.static_fields()[0];
        assert_eq!(static_field.tpe, Type::PlatformString);
        assert_eq!(&linked[static_field.name], "StaticText");
        assert!(static_field.is_tls);
        assert!(static_field.is_const);
        let Some(Const::PlatformString(default_text)) = static_field.default_value else {
            panic!("static default must remain a platform string");
        };
        assert_eq!(&linked[default_text], "default-value");

        let event = &linked_class.events()[0];
        assert_eq!(&linked[event.name()], "Changed");
        let Type::ClassRef(delegate) = event.delegate() else {
            panic!("event delegate must remain a class reference");
        };
        assert_eq!(
            &linked[linked.class_ref(delegate).name()],
            "KitchenDelegate"
        );
        assert_eq!(&linked[linked[event.add()].name()], "add_Changed");
        assert_eq!(&linked[linked[event.remove()].name()], "remove_Changed");

        let property = &linked_class.properties()[0];
        assert_eq!(&linked[property.name()], "Value");
        let Type::Ref(property_inner) = property.tpe() else {
            panic!("property type must remain a managed reference");
        };
        assert_eq!(linked[property_inner], Type::Int(Int::I16));
        let getter = property.getter().expect("property getter");
        let setter = property.setter().expect("property setter");
        assert_eq!(&linked[linked[getter].name()], "get_Value");
        assert_eq!(&linked[linked[setter].name()], "set_Value");

        let attribute = &linked_class.custom_attributes()[0];
        assert_eq!(
            &linked[linked.class_ref(attribute.attr_type()).name()],
            "KitchenAttribute"
        );
        assert!(
            matches!(attribute.ctor_args()[0], CustomAttrArg::Str(value) if &linked[value] == "ctor-text")
        );
        assert_eq!(attribute.ctor_args()[1], CustomAttrArg::Bool(true));
        assert_eq!(attribute.ctor_args()[2], CustomAttrArg::I32(17));
        assert_eq!(attribute.ctor_args()[3], CustomAttrArg::I64(29));
        assert_eq!(
            attribute.named_args()[0].kind(),
            CustomAttrNamedArgKind::Property
        );
        assert_eq!(&linked[attribute.named_args()[0].name()], "NamedText");
        assert!(
            matches!(attribute.named_args()[0].value(), CustomAttrArg::Str(value) if &linked[*value] == "named-text")
        );
        assert_eq!(
            attribute.named_args()[1].kind(),
            CustomAttrNamedArgKind::Field
        );
        assert_eq!(&linked[attribute.named_args()[1].name()], "NamedFlag");
        assert_eq!(
            attribute.named_args()[1].value(),
            &CustomAttrArg::Bool(true)
        );

        let method = linked
            .method_defs()
            .values()
            .find(|method| &linked[method.name()] == "KitchenMethod")
            .expect("linked kitchen-sink method");
        assert_ne!(method.name().inner(), method_name.inner());
        assert_eq!(method.class(), *linked_class_id);
        assert_eq!(*method.access(), Access::Public);
        assert_eq!(method.kind(), MethodKind::Virtual);
        assert_eq!(
            &linked[method.arg_names()[0].expect("argument name")],
            "output"
        );
        assert!(method.is_abstract());
        assert!(method.is_special_name());
        assert_eq!(method.out_params(), [1]);
        assert_eq!(&linked[method.generic_params()[0]], "TMethod");
        let override_method = method.overrides().expect("explicit override");
        assert_eq!(&linked[linked[override_method].name()], "BaseVirtual");
        let signature = &linked[method.sig()];
        let Type::Ref(argument_inner) = signature.inputs()[0] else {
            panic!("method argument must remain a managed reference");
        };
        assert_eq!(linked[argument_inner], Type::Int(Int::I32));
        assert_eq!(*signature.output(), Type::Void);
        let MethodImpl::MethodBody { blocks, locals } = method.implementation() else {
            panic!("method body metadata must survive linking");
        };
        assert_eq!(blocks[0].block_id(), 7);
        assert_eq!(blocks[0].handler_id(), Some(41));
        assert_eq!(linked.get_root(blocks[0].roots()[0]), &CILRoot::VoidRet);
        assert_eq!(&linked[locals[0].0.expect("local name")], "local_value");
        assert_eq!(linked[locals[0].1], Type::Int(Int::U64));
    }

    #[test]
    fn source_internal_native_import_conflict_is_rejected_before_commit() {
        fn import(library: &str) -> NativeImport {
            NativeImport {
                rust_symbol: "duplicate_source_symbol".into(),
                entry_point: "duplicate_source_symbol".into(),
                library: library.into(),
                call_conv: crate::PInvokeCallConv::Cdecl,
                preserve_errno: false,
            }
        }

        let mut destination = Assembly::default();
        destination.add_section("preserved", b"parent");
        let before_counts = destination.arena_counts();
        let before_bytes = postcard::to_stdvec(&destination).unwrap();

        let mut source = Assembly::default();
        source.push_native_import_unchecked_for_test(import("library-one"));
        source.push_native_import_unchecked_for_test(import("library-two"));

        let result = destination.try_link_in_place(source);
        assert!(matches!(
            result,
            Err(AssemblyLinkError::NativeImportConflict { .. })
        ));
        assert_eq!(destination.arena_counts(), before_counts);
        assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);
    }

    fn assembly_with_real_method_after_noise(body_value: i32, noise: usize) -> Assembly {
        let mut assembly = Assembly::default();
        for index in 0..noise {
            assembly.alloc_string(format!("unreachable-noise-{index}"));
            assembly.alloc_node(Const::I32(index as i32));
        }
        let owner = assembly.main_module();
        let signature = assembly.sig([], Type::Void);
        let name = assembly.alloc_string("duplicate_real_method");
        let value = assembly.alloc_node(Const::I32(body_value));
        let pop = assembly.alloc_root(CILRoot::Pop(value));
        let ret = assembly.alloc_root(CILRoot::VoidRet);
        assembly.new_method(MethodDef::new(
            Access::Public,
            owner,
            name,
            signature,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![pop, ret], 0, None)],
                locals: vec![],
            },
            vec![],
        ));
        assembly
    }

    fn assembly_with_real_method(body_value: i32) -> Assembly {
        assembly_with_real_method_after_noise(body_value, 0)
    }

    fn assembly_with_named_real_method(name: &str, body_value: i32) -> Assembly {
        let mut assembly = Assembly::default();
        let owner = assembly.main_module();
        let signature = assembly.sig([], Type::Void);
        let name = assembly.alloc_string(name);
        let value = assembly.alloc_node(Const::I32(body_value));
        let pop = assembly.alloc_root(CILRoot::Pop(value));
        let ret = assembly.alloc_root(CILRoot::VoidRet);
        assembly.new_method(MethodDef::new(
            Access::Private,
            owner,
            name,
            signature,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![pop, ret], 0, None)],
                locals: vec![],
            },
            vec![],
        ));
        assembly
    }

    fn assembly_with_named_empty_class(
        name: &str,
        access: Access,
        field: Option<Type>,
    ) -> Assembly {
        let mut assembly = Assembly::default();
        let name = assembly.alloc_string(name);
        let field_name = assembly.alloc_string("payload");
        assembly
            .class_def(ClassDef::new(
                name,
                false,
                0,
                None,
                field
                    .map(|field| vec![(field, field_name, Some(0))])
                    .unwrap_or_default(),
                vec![],
                access,
                None,
                None,
                true,
            ))
            .unwrap();
        assembly
    }

    fn assembly_with_partial_class_members(index: usize) -> Assembly {
        let mut assembly = Assembly::default();
        let owner_name = assembly.alloc_string("PartialMemberOwner");
        let owner = assembly.alloc_class_ref(ClassRef::new(owner_name, None, false, vec![].into()));
        let interface_name = assembly.alloc_string(format!("IPartial{index}"));
        let interface =
            assembly.alloc_class_ref(ClassRef::new(interface_name, None, false, vec![].into()));
        let signature = assembly.sig([], Type::Void);
        let add = assembly.new_methodref(
            owner,
            format!("add_Event{index}"),
            signature,
            MethodKind::Instance,
            vec![],
        );
        let remove = assembly.new_methodref(
            owner,
            format!("remove_Event{index}"),
            signature,
            MethodKind::Instance,
            vec![],
        );
        let getter_signature = assembly.sig([], Type::Int(Int::I32));
        let getter = assembly.new_methodref(
            owner,
            format!("get_Property{index}"),
            getter_signature,
            MethodKind::Instance,
            vec![],
        );
        let field_name = assembly.alloc_string(format!("field_{index}"));
        let event_name = assembly.alloc_string(format!("Event{index}"));
        let property_name = assembly.alloc_string(format!("Property{index}"));
        let attribute_name = assembly.alloc_string("PartialMemberAttribute");
        let attribute_type =
            assembly.alloc_class_ref(ClassRef::new(attribute_name, None, false, vec![].into()));
        let attribute = CustomAttrDef::new(
            attribute_type,
            vec![CustomAttrArg::I32(i32::try_from(index).unwrap())],
            vec![],
        );
        let mut definition = ClassDef::new(
            owner_name,
            false,
            0,
            None,
            vec![(Type::Int(Int::I32), field_name, None)],
            vec![],
            Access::Private,
            None,
            None,
            true,
        );
        definition.add_interface(interface);
        definition.add_event(EventDef::new(
            event_name,
            Type::ClassRef(interface),
            add,
            remove,
        ));
        definition.add_property(PropertyDef::new(
            property_name,
            Type::Int(Int::I32),
            Some(getter),
            None,
        ));
        definition.add_custom_attribute(attribute.clone());
        definition.add_field_custom_attribute(field_name, false, attribute);
        assembly.class_def(definition).unwrap();
        assembly
    }

    #[test]
    fn persistent_index_bounds_repeated_partial_class_member_commits() {
        let mut previous_work = None;
        for shards in [32_usize, 64, 128, 256] {
            let mut destination = assembly_with_partial_class_members(0);
            let mut total = LinkPreflightStats::default();
            let mut visited = 0;
            let mut committed = 0;
            for shard in 1..=shards {
                let stats = destination
                    .try_link_in_place(assembly_with_partial_class_members(shard))
                    .unwrap();
                total.accumulate(stats.preflight);
                visited += stats.class_members_visited;
                committed += stats.class_members_committed;
            }

            assert_eq!(total.destination_class_members_indexed, 6);
            assert_eq!(total.source_class_members_indexed, 6 * shards);
            assert_eq!(total.source_class_member_preflight_visits, 6 * shards);
            assert_eq!(total.cross_class_member_comparisons, 0);
            assert_eq!(total.cross_class_definition_comparisons, shards);
            assert_eq!(visited, 6 * shards);
            assert_eq!(committed, 6 * shards);

            let definition = destination
                .class_defs()
                .values()
                .find(|definition| &destination[definition.name()] == "PartialMemberOwner")
                .unwrap();
            let expected = shards + 1;
            assert_eq!(definition.implements().len(), expected);
            assert_eq!(definition.fields().len(), expected);
            assert_eq!(definition.events().len(), expected);
            assert_eq!(definition.properties().len(), expected);
            assert_eq!(definition.custom_attributes().len(), expected);
            assert_eq!(
                definition
                    .field_custom_attribute_groups()
                    .iter()
                    .map(|(_, _, attributes)| attributes.len())
                    .sum::<usize>(),
                expected
            );

            let work = total.accounted_work();
            if let Some(previous_work) = previous_work {
                assert!(work >= previous_work * 2 - 32);
                assert!(work <= previous_work * 3);
            }
            previous_work = Some(work);
        }
    }

    fn assembly_with_many_static_fields(count: usize) -> Assembly {
        let mut assembly = Assembly::default();
        let owner_name = assembly.alloc_string("ManyStaticFields");
        let mut fields = Vec::with_capacity(count);
        for index in 0..count {
            fields.push(StaticFieldDef {
                tpe: Type::Int(Int::I32),
                name: assembly.alloc_string(format!("field_{index}")),
                is_tls: false,
                default_value: None,
                is_const: false,
            });
        }
        assembly
            .class_def(ClassDef::new(
                owner_name,
                false,
                0,
                None,
                vec![],
                fields,
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
        assembly
    }

    fn add_unrelated_class_kind_authority(assembly: &mut Assembly) {
        let name = assembly.alloc_string("UnrelatedAuthorityProjection");
        assembly
            .class_def(ClassDef::new(
                name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
        assembly
            .class_def(
                ClassDef::new(
                    name,
                    true,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Private,
                    None,
                    None,
                    true,
                )
                .with_valuetype_authoritative(),
            )
            .unwrap();
    }

    #[test]
    fn one_large_static_field_shard_is_visited_once_per_row() {
        let count = 8_192;
        let mut destination = Assembly::default();
        let stats = destination
            .try_link_in_place(assembly_with_many_static_fields(count))
            .unwrap();
        assert_eq!(stats.preflight.source_static_fields_indexed, count);
        assert_eq!(stats.preflight.source_static_field_preflight_visits, count);
        assert_eq!(stats.class_static_fields_visited, count);
        assert_eq!(stats.class_static_fields_committed, count);
        let definition = destination
            .class_defs()
            .values()
            .find(|definition| &destination[definition.name()] == "ManyStaticFields")
            .unwrap();
        assert_eq!(definition.static_fields().len(), count);
    }

    #[test]
    fn unrelated_authority_normalizes_one_large_static_class_linearly() {
        let count = 8_192;
        let mut source = assembly_with_many_static_fields(count);
        add_unrelated_class_kind_authority(&mut source);
        let mut destination = Assembly::default();
        let stats = destination.try_link_in_place(source).unwrap();
        // The original and rebuilt source each construct one persistent index; neither pass
        // performs per-field owner cloning.
        assert_eq!(stats.preflight.source_static_fields_indexed, count * 2);
        assert_eq!(stats.preflight.authority_normalized_static_fields, count);
        assert_eq!(stats.class_static_fields_visited, count);
        assert_eq!(stats.class_static_fields_committed, count);
    }

    fn assembly_with_many_member_heavy_methods(count: usize) -> Assembly {
        let mut assembly = Assembly::default();
        let owner_name = assembly.alloc_string("ManyMemberHeavyMethods");
        let mut fields = Vec::with_capacity(count);
        let mut statics = Vec::with_capacity(count);
        for index in 0..count {
            fields.push((
                Type::Int(Int::I32),
                assembly.alloc_string(format!("field_{index}")),
                None,
            ));
            statics.push(StaticFieldDef {
                tpe: Type::Int(Int::I32),
                name: assembly.alloc_string(format!("static_{index}")),
                is_tls: false,
                default_value: None,
                is_const: false,
            });
        }
        let owner = assembly
            .class_def(ClassDef::new(
                owner_name,
                false,
                0,
                None,
                fields,
                statics,
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
        let signature = assembly.sig([], Type::Void);
        let ret = assembly.alloc_root(CILRoot::VoidRet);
        for index in 0..count {
            let name = assembly.alloc_string(format!("method_{index}"));
            assembly.new_method(MethodDef::new(
                Access::Private,
                owner,
                name,
                signature,
                MethodKind::Static,
                MethodImpl::MethodBody {
                    blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                    locals: vec![],
                },
                vec![],
            ));
        }
        add_unrelated_class_kind_authority(&mut assembly);
        assembly
    }

    #[test]
    fn unrelated_authority_uses_minimal_method_owner_shells() {
        let count = 2_048;
        let mut destination = Assembly::default();
        let stats = destination
            .try_link_in_place(assembly_with_many_member_heavy_methods(count))
            .unwrap();
        assert_eq!(stats.preflight.source_method_definitions_indexed, count * 2);
        assert_eq!(
            stats.preflight.authority_normalized_method_definitions,
            count
        );
        assert_eq!(stats.preflight.source_class_members_indexed, count * 2);
        assert_eq!(stats.preflight.source_static_fields_indexed, count * 2);
        assert_eq!(destination.method_defs().len(), count);
    }

    #[test]
    fn persistent_index_tracks_empty_classes_and_invalidates_on_public_mutation() {
        let mut destination =
            assembly_with_named_empty_class("InitiallyIndexedClass", Access::Private, None);
        destination
            .try_link_in_place(assembly_with_named_empty_class(
                "RelocatedEmptyClass",
                Access::Private,
                None,
            ))
            .unwrap();

        let before = postcard::to_stdvec(&destination).unwrap();
        let error = destination
            .try_link_in_place(assembly_with_named_empty_class(
                "RelocatedEmptyClass",
                Access::Public,
                None,
            ))
            .unwrap_err();
        assert!(matches!(
            error,
            AssemblyLinkError::ClassDefinitionConflict { property, .. }
                if property == "access"
        ));
        assert_eq!(postcard::to_stdvec(&destination).unwrap(), before);

        let relocated = destination
            .iter_class_def_ids()
            .copied()
            .find(|class| &destination[destination[*class].name()] == "RelocatedEmptyClass")
            .unwrap();
        let field_name = destination.alloc_string("payload");
        destination.class_mut(relocated).fields_mut().push((
            Type::Int(Int::I32),
            field_name,
            Some(0),
        ));
        assert!(destination.link_preflight_index.is_none());

        let error = destination
            .try_link_in_place(assembly_with_named_empty_class(
                "RelocatedEmptyClass",
                Access::Private,
                Some(Type::Int(Int::I64)),
            ))
            .unwrap_err();
        assert!(matches!(
            error,
            AssemblyLinkError::ClassFieldConflict { .. }
        ));
    }

    #[test]
    fn persistent_preflight_index_makes_disjoint_shard_commits_n_log_n_bounded() {
        let mut previous_work = None;
        for shards in [32_usize, 64, 128, 256] {
            let mut destination = assembly_with_named_real_method("preflight_seed", 0);
            let initial_destination_items = destination.class_defs().len()
                + destination.iter_class_ref_ids().len()
                + destination.method_defs().len()
                + destination.native_imports().len();
            let sample = assembly_with_named_real_method("sample_source", 0);
            let source_items_per_shard = sample.class_defs().len()
                + sample.iter_class_ref_ids().len()
                + sample.method_defs().len()
                + sample.native_imports().len();
            let mut total = LinkPreflightStats::default();
            let mut committed_methods = 0;

            for shard in 0..shards {
                let stats = destination
                    .try_link_in_place(assembly_with_named_real_method(
                        &format!("disjoint_method_{shard}"),
                        i32::try_from(shard).unwrap(),
                    ))
                    .unwrap();
                total.accumulate(stats.preflight);
                committed_methods += stats.method_definitions_committed;
            }

            // Every keyed/scanned/probed unit is explicit here. The accumulated destination is
            // indexed once; each incoming shard builds exactly its own keys and performs four
            // logarithmic map probes (class-ref authority, two class-definition phases, method).
            assert_eq!(total.destination_items_indexed(), initial_destination_items);
            assert_eq!(
                total.source_items_indexed(),
                source_items_per_shard * shards
            );
            assert_eq!(total.destination_class_definitions_indexed, 1);
            assert_eq!(total.destination_class_references_indexed, 1);
            assert_eq!(total.destination_method_definitions_indexed, 1);
            assert_eq!(total.destination_native_imports_indexed, 0);
            assert_eq!(total.destination_static_fields_indexed, 0);
            assert_eq!(total.source_class_definitions_indexed, shards);
            assert_eq!(total.source_class_references_indexed, shards);
            assert_eq!(total.source_method_definitions_indexed, shards);
            assert_eq!(total.source_native_imports_indexed, 0);
            assert_eq!(total.source_static_fields_indexed, 0);
            assert_eq!(total.source_native_import_preflight_visits, 0);
            assert_eq!(total.source_authoritative_kind_preflight_visits, 0);
            assert_eq!(total.source_class_reference_kind_preflight_visits, shards);
            assert_eq!(total.source_class_definition_preflight_visits, 2 * shards);
            assert_eq!(total.source_method_definition_preflight_visits, shards);
            assert_eq!(total.source_required_kind_override_preflight_visits, 0);
            assert_eq!(total.destination_required_kind_override_preflight_visits, 0);
            assert_eq!(total.destination_identity_probes, 4 * shards);
            assert_eq!(total.cross_class_definition_comparisons, shards);
            assert_eq!(total.cross_method_definition_comparisons, 0);
            assert_eq!(committed_methods, shards);
            assert_eq!(destination.method_defs().len(), shards + 1);

            let measured_work = total.accounted_work();
            if let Some(previous_work) = previous_work {
                // Doubling n may add only a constant-factor amount of instrumented work. Each
                // destination probe is BTreeMap O(log n), hence this establishes O(n log n) for
                // ordinary disjoint commits and rules out a hidden repeated destination scan.
                assert!(measured_work >= previous_work * 2 - initial_destination_items);
                assert!(measured_work <= previous_work * 3);
            }
            previous_work = Some(measured_work);

            // An identity near-miss must still reach the complete structural body comparator.
            let before_counts = destination.arena_counts();
            let before_bytes = postcard::to_stdvec(&destination).unwrap();
            reset_method_definition_semantic_key_builds();
            let error = destination
                .try_link_in_place(assembly_with_named_real_method(
                    &format!("disjoint_method_{}", shards / 2),
                    -1,
                ))
                .unwrap_err();
            assert!(matches!(
                error,
                AssemblyLinkError::MethodConflict { detail, .. }
                    if detail == "competing real method implementations differ"
            ));
            assert_eq!(method_definition_semantic_key_builds(), 2);
            assert_eq!(destination.arena_counts(), before_counts);
            assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);
        }
    }

    #[test]
    fn shortening_strings_invalidates_and_rebuilds_method_identity_index() {
        let original_name =
            "method_identity_that_is_deliberately_long_enough_to_be_shortened_for_linking";
        let mut destination = assembly_with_named_real_method(original_name, 7);
        destination.try_link_in_place(Assembly::default()).unwrap();
        assert!(destination.link_preflight_index.is_some());

        destination.shorten_strings(16);
        assert!(destination.link_preflight_index.is_none());
        let shortened_name = destination
            .method_defs()
            .values()
            .next()
            .map(|method| destination[method.name()].to_string())
            .expect("shortened method definition");
        assert_ne!(shortened_name, original_name);

        let before_counts = destination.arena_counts();
        let before_bytes = postcard::to_stdvec(&destination).unwrap();
        let error = destination
            .try_link_in_place(assembly_with_named_real_method(&shortened_name, 99))
            .unwrap_err();
        assert!(matches!(error, AssemblyLinkError::MethodConflict { .. }));
        assert_eq!(destination.arena_counts(), before_counts);
        assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);

        let method_count = destination.method_defs().len();
        destination
            .try_link_in_place(assembly_with_named_real_method(&shortened_name, 7))
            .unwrap();
        assert_eq!(destination.method_defs().len(), method_count);
    }

    #[test]
    fn competing_real_method_definitions_are_rejected_in_both_orders() {
        for (existing_value, incoming_value) in [(1, 2), (2, 1)] {
            let mut destination = assembly_with_real_method(existing_value);
            let before_counts = destination.arena_counts();
            let before_bytes = postcard::to_stdvec(&destination).unwrap();

            let result = destination.try_link_in_place(assembly_with_real_method(incoming_value));
            assert!(matches!(
                result,
                Err(AssemblyLinkError::MethodConflict { .. })
            ));
            assert_eq!(destination.arena_counts(), before_counts);
            assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);
        }
    }

    #[test]
    fn identical_real_method_definitions_merge_in_both_orders() {
        let orderings = [
            (
                assembly_with_real_method(1),
                assembly_with_real_method_after_noise(1, 3),
            ),
            (
                assembly_with_real_method_after_noise(1, 3),
                assembly_with_real_method(1),
            ),
        ];
        for (mut destination, source) in orderings {
            destination.try_link_in_place(source).unwrap();
            assert_eq!(destination.method_defs().len(), 1);
            assert!(matches!(
                destination
                    .method_defs()
                    .values()
                    .next()
                    .expect("merged method")
                    .implementation(),
                MethodImpl::MethodBody { .. }
            ));
        }
    }

    fn assembly_with_missing_method() -> Assembly {
        let mut assembly = Assembly::default();
        let owner = assembly.main_module();
        let signature = assembly.sig([], Type::Void);
        let name = assembly.alloc_string("duplicate_real_method");
        assembly.new_method(MethodDef::new(
            Access::Public,
            owner,
            name,
            signature,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![],
        ));
        assembly
    }

    #[test]
    fn one_real_method_definition_wins_over_missing_in_both_orders() {
        let orderings = [
            (assembly_with_missing_method(), assembly_with_real_method(1)),
            (assembly_with_real_method(1), assembly_with_missing_method()),
        ];
        for (mut destination, source) in orderings {
            destination.try_link_in_place(source).unwrap();
            assert_eq!(destination.method_defs().len(), 1);
            let implementation = destination
                .method_defs()
                .values()
                .next()
                .expect("merged method")
                .implementation();
            assert!(matches!(implementation, MethodImpl::MethodBody { .. }));
        }
    }

    fn assembly_with_class_kind(is_valuetype: bool) -> Assembly {
        let mut assembly = Assembly::default();
        let name = assembly.alloc_string("DefinitionKindCollision");
        assembly
            .class_def(ClassDef::new(
                name,
                is_valuetype,
                0,
                None,
                vec![],
                vec![],
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
        assembly
    }

    fn assembly_with_internal_class_kind_evidence(evidence: &[(bool, bool)]) -> Assembly {
        let mut assembly = Assembly::default();
        let name = assembly.alloc_string("InternalDefinitionKindCollision");
        for &(is_valuetype, authoritative) in evidence {
            let mut definition = ClassDef::new(
                name,
                is_valuetype,
                0,
                None,
                vec![],
                vec![],
                Access::Private,
                None,
                None,
                true,
            );
            if authoritative {
                definition = definition.with_valuetype_authoritative();
            }
            assembly.class_def(definition).unwrap();
        }
        assembly
    }

    fn assert_internal_class_kind_conflict_is_read_only(
        destination: &Assembly,
        source: &Assembly,
        authoritative: bool,
    ) {
        let destination_counts = destination.arena_counts();
        let destination_bytes = postcard::to_stdvec(destination).unwrap();
        let source_counts = source.arena_counts();
        let source_bytes = postcard::to_stdvec(source).unwrap();

        let error = match preflight_assembly_link(destination, source) {
            Err(error) => error,
            Ok(_) => panic!("internal class-kind conflict passed link preflight"),
        };
        assert!(matches!(
            error,
            AssemblyLinkError::ClassDefinitionConflict {
                property,
                existing,
                incoming,
                ..
            } if property == "value type authority"
                && existing.contains(&format!("authoritative={authoritative}"))
                && incoming.contains(&format!("authoritative={authoritative}"))
        ));

        let mut attempted_destination = destination.clone();
        assert!(matches!(
            attempted_destination.try_link_in_place(source.clone()),
            Err(AssemblyLinkError::ClassDefinitionConflict { .. })
        ));
        assert_eq!(
            postcard::to_stdvec(&attempted_destination).unwrap(),
            destination_bytes
        );
        assert_eq!(destination.arena_counts(), destination_counts);
        assert_eq!(postcard::to_stdvec(destination).unwrap(), destination_bytes);
        assert_eq!(source.arena_counts(), source_counts);
        assert_eq!(postcard::to_stdvec(source).unwrap(), source_bytes);
    }

    #[test]
    fn source_internal_class_kind_conflicts_are_rejected_before_identity_collection() {
        for authoritative in [false, true] {
            let destination = Assembly::default();
            let source = assembly_with_internal_class_kind_evidence(&[
                (false, authoritative),
                (true, authoritative),
            ]);
            assert_internal_class_kind_conflict_is_read_only(&destination, &source, authoritative);
        }
    }

    #[test]
    fn destination_internal_class_kind_conflicts_are_rejected_before_identity_collection() {
        for authoritative in [false, true] {
            let destination = assembly_with_internal_class_kind_evidence(&[
                (false, authoritative),
                (true, authoritative),
            ]);
            let source = Assembly::default();
            assert_internal_class_kind_conflict_is_read_only(&destination, &source, authoritative);
        }
    }

    #[test]
    fn sole_internal_authority_reconciles_one_placeholder_instead_of_conflicting() {
        for authoritative_kind in [false, true] {
            let mut destination = assembly_with_internal_class_kind_evidence(&[
                (!authoritative_kind, false),
                (authoritative_kind, true),
            ]);
            destination.try_link_in_place(Assembly::default()).unwrap();

            let matching_definitions: Vec<_> = destination
                .class_defs()
                .values()
                .filter(|definition| {
                    &destination[definition.name()] == "InternalDefinitionKindCollision"
                })
                .collect();
            assert_eq!(matching_definitions.len(), 1);
            assert_eq!(matching_definitions[0].is_valuetype(), authoritative_kind);
            assert!(matching_definitions[0].is_valuetype_authoritative());
            assert!(destination.iter_class_refs().all(|class| {
                &destination[class.name()] != "InternalDefinitionKindCollision"
                    || class.is_valuetype() == authoritative_kind
            }));
        }
    }

    fn assembly_with_internal_authority_member_references(reverse: bool) -> Assembly {
        let mut assembly = Assembly::default();
        let target_name = assembly.alloc_string("AuthorityMemberTarget");
        let target_placeholder =
            assembly.alloc_class_ref(ClassRef::new(target_name, None, false, vec![].into()));
        let target_authoritative =
            assembly.alloc_class_ref(ClassRef::new(target_name, None, true, vec![].into()));
        assembly
            .class_def(ClassDef::new(
                target_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
        assembly
            .class_def(
                ClassDef::new(
                    target_name,
                    true,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Private,
                    None,
                    None,
                    true,
                )
                .with_valuetype_authoritative(),
            )
            .unwrap();

        let consumer_name = assembly.alloc_string("AuthorityMemberConsumer");
        let field_name = assembly.alloc_string("target");
        let event_name = assembly.alloc_string("Changed");
        let property_name = assembly.alloc_string("Value");
        let order = if reverse {
            [(target_authoritative, true), (target_placeholder, false)]
        } else {
            [(target_placeholder, false), (target_authoritative, true)]
        };
        for (target, authoritative) in order {
            let consumer = assembly.alloc_class_ref(ClassRef::new(
                consumer_name,
                None,
                authoritative,
                vec![].into(),
            ));
            let event_signature = assembly.sig([Type::ClassRef(target)], Type::Void);
            let add = assembly.new_methodref(
                consumer,
                "add_Changed",
                event_signature,
                MethodKind::Instance,
                vec![],
            );
            let remove = assembly.new_methodref(
                consumer,
                "remove_Changed",
                event_signature,
                MethodKind::Instance,
                vec![],
            );
            let getter_signature = assembly.sig([], Type::ClassRef(target));
            let getter = assembly.new_methodref(
                consumer,
                "get_Value",
                getter_signature,
                MethodKind::Instance,
                vec![],
            );
            let mut definition = ClassDef::new(
                consumer_name,
                authoritative,
                0,
                None,
                vec![(Type::ClassRef(target), field_name, None)],
                vec![],
                Access::Private,
                None,
                None,
                true,
            );
            if authoritative {
                definition = definition.with_valuetype_authoritative();
            }
            definition.add_event(EventDef::new(
                event_name,
                Type::ClassRef(target),
                add,
                remove,
            ));
            definition.add_property(PropertyDef::new(
                property_name,
                Type::ClassRef(target),
                Some(getter),
                None,
            ));
            assembly.class_def(definition).unwrap();
        }
        assembly
    }

    #[test]
    fn member_index_applies_internal_class_kind_authority_before_comparison() {
        for reverse in [false, true] {
            let mut assembly = assembly_with_internal_authority_member_references(reverse);
            assembly.try_link_in_place(Assembly::default()).unwrap();
            let definition = assembly
                .class_defs()
                .values()
                .find(|definition| &assembly[definition.name()] == "AuthorityMemberConsumer")
                .unwrap();
            assert!(definition.is_valuetype());
            assert_eq!(definition.fields().len(), 1);
            assert_eq!(definition.events().len(), 1);
            assert_eq!(definition.properties().len(), 1);
            for tpe in [
                definition.fields()[0].0,
                definition.events()[0].delegate(),
                definition.properties()[0].tpe(),
            ] {
                let Type::ClassRef(target) = tpe else {
                    panic!("member type must remain a class reference");
                };
                assert!(assembly[target].is_valuetype());
            }
        }
    }

    fn assembly_with_internal_authority_static_attributes(reverse: bool) -> Assembly {
        let mut assembly = Assembly::default();
        let target_name = assembly.alloc_string("AuthorityStaticAttribute");
        let target_placeholder =
            assembly.alloc_class_ref(ClassRef::new(target_name, None, false, vec![].into()));
        let target_authoritative =
            assembly.alloc_class_ref(ClassRef::new(target_name, None, true, vec![].into()));
        assembly
            .class_def(ClassDef::new(
                target_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
        assembly
            .class_def(
                ClassDef::new(
                    target_name,
                    true,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Private,
                    None,
                    None,
                    true,
                )
                .with_valuetype_authoritative(),
            )
            .unwrap();

        let owner_name = assembly.alloc_string("AuthorityStaticOwner");
        let field_name = assembly.alloc_string("value");
        let order = if reverse {
            [(target_authoritative, true), (target_placeholder, false)]
        } else {
            [(target_placeholder, false), (target_authoritative, true)]
        };
        for (attribute_type, authoritative) in order {
            let mut definition = ClassDef::new(
                owner_name,
                authoritative,
                0,
                None,
                vec![],
                vec![StaticFieldDef {
                    tpe: Type::Int(Int::I32),
                    name: field_name,
                    is_tls: false,
                    default_value: None,
                    is_const: false,
                }],
                Access::Private,
                None,
                None,
                true,
            );
            if authoritative {
                definition = definition.with_valuetype_authoritative();
            }
            definition.add_field_custom_attribute(
                field_name,
                true,
                CustomAttrDef::new(attribute_type, vec![], vec![]),
            );
            assembly.class_def(definition).unwrap();
        }
        assembly
    }

    #[test]
    fn static_index_applies_internal_class_kind_authority_before_comparison() {
        for reverse in [false, true] {
            let mut assembly = assembly_with_internal_authority_static_attributes(reverse);
            assembly.try_link_in_place(Assembly::default()).unwrap();
            let definition = assembly
                .class_defs()
                .values()
                .find(|definition| &assembly[definition.name()] == "AuthorityStaticOwner")
                .unwrap();
            assert!(definition.is_valuetype());
            assert_eq!(definition.static_fields().len(), 1);
            assert_eq!(
                definition
                    .field_custom_attributes(definition.static_fields()[0].name, true)
                    .count(),
                1
            );
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum InternalNormalizedConflict {
        Field,
        Base,
        Access,
        RealMethod,
    }

    fn assembly_with_internal_normalized_conflict(
        conflict: InternalNormalizedConflict,
    ) -> Assembly {
        let mut assembly = Assembly::default();
        let name = assembly.alloc_string("InternalNormalizedConflict");
        let field_name = assembly.alloc_string("payload");
        let placeholder_base_name = assembly.alloc_string("PlaceholderBase");
        let authoritative_base_name = assembly.alloc_string("AuthoritativeBase");
        let placeholder_base =
            assembly.alloc_class_ref(ClassRef::new(placeholder_base_name, None, false, [].into()));
        let authoritative_base = assembly.alloc_class_ref(ClassRef::new(
            authoritative_base_name,
            None,
            false,
            [].into(),
        ));

        let placeholder = ClassDef::new(
            name,
            false,
            0,
            matches!(conflict, InternalNormalizedConflict::Base).then_some(placeholder_base),
            matches!(conflict, InternalNormalizedConflict::Field)
                .then_some(vec![(Type::Int(Int::I32), field_name, Some(0))])
                .unwrap_or_default(),
            vec![],
            Access::Private,
            None,
            None,
            true,
        );
        let placeholder = assembly.class_def(placeholder).unwrap();

        let authoritative = ClassDef::new(
            name,
            true,
            0,
            matches!(conflict, InternalNormalizedConflict::Base).then_some(authoritative_base),
            matches!(conflict, InternalNormalizedConflict::Field)
                .then_some(vec![(Type::Int(Int::I64), field_name, Some(0))])
                .unwrap_or_default(),
            vec![],
            if matches!(conflict, InternalNormalizedConflict::Access) {
                Access::Public
            } else {
                Access::Private
            },
            None,
            None,
            true,
        )
        .with_valuetype_authoritative();
        let authoritative = assembly.class_def(authoritative).unwrap();

        if matches!(conflict, InternalNormalizedConflict::RealMethod) {
            let signature = assembly.sig([], Type::Void);
            let method_name = assembly.alloc_string("competing_body");
            for (owner, value) in [(placeholder, 1), (authoritative, 2)] {
                let value = assembly.alloc_node(Const::I32(value));
                let pop = assembly.alloc_root(CILRoot::Pop(value));
                let ret = assembly.alloc_root(CILRoot::VoidRet);
                assembly.new_method(MethodDef::new(
                    Access::Private,
                    owner,
                    method_name,
                    signature,
                    MethodKind::Static,
                    MethodImpl::MethodBody {
                        blocks: vec![BasicBlock::new(vec![pop, ret], 0, None)],
                        locals: vec![],
                    },
                    vec![],
                ));
            }
        }
        assembly
    }

    fn assert_internal_normalized_conflict_is_structured(
        conflict: InternalNormalizedConflict,
        invalid_is_destination: bool,
    ) {
        let invalid = assembly_with_internal_normalized_conflict(conflict);
        let (destination, source) = if invalid_is_destination {
            (invalid, Assembly::default())
        } else {
            (Assembly::default(), invalid)
        };
        let destination_counts = destination.arena_counts();
        let source_counts = source.arena_counts();
        let destination_bytes = postcard::to_stdvec(&destination).unwrap();
        let source_bytes = postcard::to_stdvec(&source).unwrap();

        let error = match preflight_assembly_link(&destination, &source) {
            Err(error) => error,
            Ok(_) => panic!("internal normalized conflict passed preflight"),
        };
        match conflict {
            InternalNormalizedConflict::Field => {
                assert!(matches!(
                    error,
                    AssemblyLinkError::ClassFieldConflict { .. }
                ));
            }
            InternalNormalizedConflict::Base => {
                assert!(matches!(error, AssemblyLinkError::ClassBaseConflict { .. }));
            }
            InternalNormalizedConflict::Access => assert!(matches!(
                error,
                AssemblyLinkError::ClassDefinitionConflict { property, .. }
                    if property == "access"
            )),
            InternalNormalizedConflict::RealMethod => {
                assert!(matches!(error, AssemblyLinkError::MethodConflict { .. }));
            }
        }

        let mut attempted_destination = destination.clone();
        assert!(
            attempted_destination
                .try_link_in_place(source.clone())
                .is_err()
        );
        assert_eq!(attempted_destination.arena_counts(), destination_counts);
        assert_eq!(
            postcard::to_stdvec(&attempted_destination).unwrap(),
            destination_bytes
        );
        assert_eq!(destination.arena_counts(), destination_counts);
        assert_eq!(source.arena_counts(), source_counts);
        assert_eq!(
            postcard::to_stdvec(&destination).unwrap(),
            destination_bytes
        );
        assert_eq!(postcard::to_stdvec(&source).unwrap(), source_bytes);
    }

    #[test]
    fn internal_normalized_merge_conflicts_are_rejected_in_destination_and_source() {
        for conflict in [
            InternalNormalizedConflict::Field,
            InternalNormalizedConflict::Base,
            InternalNormalizedConflict::Access,
            InternalNormalizedConflict::RealMethod,
        ] {
            for invalid_is_destination in [false, true] {
                assert_internal_normalized_conflict_is_structured(conflict, invalid_is_destination);
            }
        }
    }

    fn assembly_with_method_identity_collapse(reverse: bool) -> Assembly {
        let mut assembly = Assembly::default();
        let target_name = assembly.alloc_string("MethodIdentityCollapseTarget");
        let placeholder =
            assembly.alloc_class_ref(ClassRef::new(target_name, None, false, Vec::new().into()));
        let authoritative =
            assembly.alloc_class_ref(ClassRef::new(target_name, None, true, Vec::new().into()));
        assembly
            .class_def(
                ClassDef::new(
                    target_name,
                    true,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Private,
                    None,
                    None,
                    true,
                )
                .with_valuetype_authoritative(),
            )
            .unwrap();

        let owner = assembly.main_module();
        let name = assembly.alloc_string("method_identity_collapse");
        let ordered = if reverse {
            [(authoritative, 2), (placeholder, 1)]
        } else {
            [(placeholder, 1), (authoritative, 2)]
        };
        for (parameter, body_value) in ordered {
            let signature = assembly.sig([Type::ClassRef(parameter)], Type::Void);
            let value = assembly.alloc_node(Const::I32(body_value));
            let pop = assembly.alloc_root(CILRoot::Pop(value));
            let ret = assembly.alloc_root(CILRoot::VoidRet);
            assembly.new_method(MethodDef::new(
                Access::Private,
                owner,
                name,
                signature,
                MethodKind::Static,
                MethodImpl::MethodBody {
                    blocks: vec![BasicBlock::new(vec![pop, ret], 0, None)],
                    locals: vec![],
                },
                vec![None],
            ));
        }
        assembly
    }

    #[test]
    fn normalized_method_ref_identity_collisions_are_structured_and_read_only() {
        for reverse in [false, true] {
            for invalid_is_destination in [false, true] {
                let invalid = assembly_with_method_identity_collapse(reverse);
                let (destination, source) = if invalid_is_destination {
                    (invalid, Assembly::default())
                } else {
                    (Assembly::default(), invalid)
                };
                let destination_counts = destination.arena_counts();
                let source_counts = source.arena_counts();
                let destination_bytes = postcard::to_stdvec(&destination).unwrap();
                let source_bytes = postcard::to_stdvec(&source).unwrap();

                let error = match preflight_assembly_link(&destination, &source) {
                    Err(error) => error,
                    Ok(_) => panic!("normalized method identity collision passed preflight"),
                };
                assert!(matches!(
                    error,
                    AssemblyLinkError::MethodConflict { detail, .. }
                        if detail == "competing real method implementations differ"
                ));
                let mut attempted_destination = destination.clone();
                assert!(matches!(
                    attempted_destination.try_link_in_place(source.clone()),
                    Err(AssemblyLinkError::MethodConflict { .. })
                ));
                assert_eq!(attempted_destination.arena_counts(), destination_counts);
                assert_eq!(
                    postcard::to_stdvec(&attempted_destination).unwrap(),
                    destination_bytes
                );
                assert_eq!(destination.arena_counts(), destination_counts);
                assert_eq!(source.arena_counts(), source_counts);
                assert_eq!(
                    postcard::to_stdvec(&destination).unwrap(),
                    destination_bytes
                );
                assert_eq!(postcard::to_stdvec(&source).unwrap(), source_bytes);
            }
        }
    }

    #[derive(Clone, Copy)]
    struct StaticFieldShape {
        default_value: i32,
        is_tls: bool,
        is_const: bool,
        attribute_value: i32,
    }

    fn assembly_with_static_field_after_noise(shape: StaticFieldShape, noise: usize) -> Assembly {
        let mut assembly = Assembly::default();
        for index in 0..noise {
            assembly.alloc_string(format!("static-field-noise-{index}"));
            assembly.alloc_node(Const::I32(i32::try_from(index).unwrap()));
        }
        let class_name = assembly.alloc_string("StaticFieldCollisionOwner");
        let field_name = assembly.alloc_string("SharedStaticField");
        let attribute_name = assembly.alloc_string("StaticFieldAttribute");
        let attribute_type = assembly.alloc_class_ref(ClassRef::new(
            attribute_name,
            None,
            false,
            Vec::new().into(),
        ));
        let mut definition = ClassDef::new(
            class_name,
            false,
            0,
            None,
            vec![],
            vec![StaticFieldDef {
                tpe: Type::Int(Int::I32),
                name: field_name,
                is_tls: shape.is_tls,
                default_value: Some(Const::I32(shape.default_value)),
                is_const: shape.is_const,
            }],
            Access::Private,
            None,
            None,
            true,
        );
        definition.add_field_custom_attribute(
            field_name,
            true,
            CustomAttrDef::new(
                attribute_type,
                vec![CustomAttrArg::I32(shape.attribute_value)],
                vec![],
            ),
        );
        assembly.class_def(definition).unwrap();
        assembly
    }

    fn assembly_with_static_field(shape: StaticFieldShape) -> Assembly {
        assembly_with_static_field_after_noise(shape, 0)
    }

    fn assembly_with_main_module_static(name: &str, value: i32) -> Assembly {
        let mut assembly = Assembly::default();
        let owner = assembly.main_module();
        let name = assembly.alloc_string(name);
        assembly
            .class_mut(owner)
            .static_fields_mut()
            .push(StaticFieldDef {
                tpe: Type::Int(Int::I32),
                name,
                is_tls: false,
                default_value: Some(Const::I32(value)),
                is_const: false,
            });
        assembly
    }

    #[test]
    fn persistent_index_does_not_rescan_accumulated_main_module_statics() {
        let mut previous_work = None;
        for shards in [32_usize, 64, 128, 256] {
            let mut destination = assembly_with_main_module_static("seed_static", 0);
            let mut total = LinkPreflightStats::default();
            let mut committed_static_fields = 0;
            for shard in 0..shards {
                let stats = destination
                    .try_link_in_place(assembly_with_main_module_static(
                        &format!("static_{shard}"),
                        i32::try_from(shard).unwrap(),
                    ))
                    .unwrap();
                total.accumulate(stats.preflight);
                committed_static_fields += stats.class_static_fields_committed;
            }

            assert_eq!(total.destination_static_fields_indexed, 1);
            assert_eq!(total.source_static_fields_indexed, shards);
            assert_eq!(total.source_static_field_preflight_visits, shards);
            assert_eq!(total.cross_static_field_comparisons, 0);
            assert_eq!(committed_static_fields, shards);
            let measured_work = total.accounted_work();
            if let Some(previous_work) = previous_work {
                assert!(measured_work >= previous_work * 2 - 8);
                assert!(measured_work <= previous_work * 3);
            }
            previous_work = Some(measured_work);

            let main_module = destination.main_module();
            assert_eq!(destination[main_module].static_fields().len(), shards + 1);
        }
    }

    #[test]
    fn static_field_metadata_conflicts_are_rejected_in_both_orders() {
        let baseline = StaticFieldShape {
            default_value: 1,
            is_tls: false,
            is_const: false,
            attribute_value: 1,
        };
        let variants = [
            StaticFieldShape {
                default_value: 2,
                ..baseline
            },
            StaticFieldShape {
                is_tls: true,
                ..baseline
            },
            StaticFieldShape {
                is_const: true,
                ..baseline
            },
            StaticFieldShape {
                attribute_value: 2,
                ..baseline
            },
        ];
        for variant in variants {
            for (existing, incoming) in [(baseline, variant), (variant, baseline)] {
                let mut destination = assembly_with_static_field(existing);
                let before_counts = destination.arena_counts();
                let before_bytes = postcard::to_stdvec(&destination).unwrap();
                let error = destination
                    .try_link_in_place(assembly_with_static_field(incoming))
                    .unwrap_err();
                assert!(matches!(
                    error,
                    AssemblyLinkError::StaticFieldConflict {
                        existing,
                        incoming,
                        ..
                    } if existing.contains("access: \"Public\"")
                        && incoming.contains("storage: \"Static\"")
                ));
                assert_eq!(destination.arena_counts(), before_counts);
                assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);
            }
        }
    }

    #[test]
    fn identical_static_fields_deduplicate_across_shards() {
        let shape = StaticFieldShape {
            default_value: 1,
            is_tls: true,
            is_const: true,
            attribute_value: 7,
        };
        let orderings = [(0, 3), (3, 0)];
        for (destination_noise, source_noise) in orderings {
            let mut destination = assembly_with_static_field_after_noise(shape, destination_noise);
            let source = assembly_with_static_field_after_noise(shape, source_noise);
            destination.try_link_in_place(source).unwrap();
            let definition = destination
                .class_defs()
                .values()
                .find(|definition| &destination[definition.name()] == "StaticFieldCollisionOwner")
                .expect("linked static-field owner");
            assert_eq!(definition.static_fields().len(), 1);
            assert_eq!(
                definition
                    .field_custom_attributes(definition.static_fields()[0].name, true)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn internal_static_field_metadata_conflicts_are_rejected_in_destination_and_source() {
        let shape = StaticFieldShape {
            default_value: 1,
            is_tls: false,
            is_const: false,
            attribute_value: 1,
        };
        for invalid_is_destination in [false, true] {
            let mut invalid = assembly_with_static_field(shape);
            let owner = invalid
                .iter_class_def_ids()
                .copied()
                .find(|class| &invalid[invalid[*class].name()] == "StaticFieldCollisionOwner")
                .expect("static-field owner");
            let mut duplicate = invalid[owner].static_fields()[0].clone();
            duplicate.default_value = Some(Const::I32(2));
            invalid.class_mut(owner).static_fields_mut().push(duplicate);

            let (destination, source) = if invalid_is_destination {
                (invalid, Assembly::default())
            } else {
                (Assembly::default(), invalid)
            };
            let destination_counts = destination.arena_counts();
            let source_counts = source.arena_counts();
            let destination_bytes = postcard::to_stdvec(&destination).unwrap();
            let source_bytes = postcard::to_stdvec(&source).unwrap();
            assert!(matches!(
                preflight_assembly_link(&destination, &source),
                Err(AssemblyLinkError::StaticFieldConflict { .. })
            ));
            let mut attempted_destination = destination.clone();
            assert!(matches!(
                attempted_destination.try_link_in_place(source.clone()),
                Err(AssemblyLinkError::StaticFieldConflict { .. })
            ));
            assert_eq!(attempted_destination.arena_counts(), destination_counts);
            assert_eq!(
                postcard::to_stdvec(&attempted_destination).unwrap(),
                destination_bytes
            );
            assert_eq!(destination.arena_counts(), destination_counts);
            assert_eq!(source.arena_counts(), source_counts);
            assert_eq!(
                postcard::to_stdvec(&destination).unwrap(),
                destination_bytes
            );
            assert_eq!(postcard::to_stdvec(&source).unwrap(), source_bytes);
        }
    }

    fn assembly_with_normalized_static_field_collapse(conflicting: bool) -> Assembly {
        let mut assembly = Assembly::default();
        let target_name = assembly.alloc_string("StaticFieldKindCollapseTarget");
        let placeholder =
            assembly.alloc_class_ref(ClassRef::new(target_name, None, false, Vec::new().into()));
        let authoritative =
            assembly.alloc_class_ref(ClassRef::new(target_name, None, true, Vec::new().into()));
        assembly
            .class_def(
                ClassDef::new(
                    target_name,
                    true,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Private,
                    None,
                    None,
                    true,
                )
                .with_valuetype_authoritative(),
            )
            .unwrap();

        let owner_name = assembly.alloc_string("NormalizedStaticFieldOwner");
        let field_name = assembly.alloc_string("CollapsedField");
        assembly
            .class_def(ClassDef::new(
                owner_name,
                false,
                0,
                None,
                vec![],
                vec![
                    StaticFieldDef {
                        tpe: Type::ClassRef(placeholder),
                        name: field_name,
                        is_tls: false,
                        default_value: Some(Const::I32(1)),
                        is_const: false,
                    },
                    StaticFieldDef {
                        tpe: Type::ClassRef(authoritative),
                        name: field_name,
                        is_tls: false,
                        default_value: Some(Const::I32(if conflicting { 2 } else { 1 })),
                        is_const: false,
                    },
                ],
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
        assembly
    }

    #[test]
    fn normalized_static_field_collisions_reject_conflicts_and_deduplicate_matches() {
        for invalid_is_destination in [false, true] {
            let invalid = assembly_with_normalized_static_field_collapse(true);
            let (mut destination, source) = if invalid_is_destination {
                (invalid, Assembly::default())
            } else {
                (Assembly::default(), invalid)
            };
            let before_counts = destination.arena_counts();
            let before_bytes = postcard::to_stdvec(&destination).unwrap();
            assert!(matches!(
                destination.try_link_in_place(source),
                Err(AssemblyLinkError::StaticFieldConflict { .. })
            ));
            assert_eq!(destination.arena_counts(), before_counts);
            assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);
        }

        for matching_is_destination in [false, true] {
            let matching = assembly_with_normalized_static_field_collapse(false);
            let (mut destination, source) = if matching_is_destination {
                (matching, Assembly::default())
            } else {
                (Assembly::default(), matching)
            };
            destination.try_link_in_place(source).unwrap();
            let definition = destination
                .class_defs()
                .values()
                .find(|definition| &destination[definition.name()] == "NormalizedStaticFieldOwner")
                .expect("normalized static-field owner");
            assert_eq!(definition.static_fields().len(), 1);
            assert!(matches!(
                definition.static_fields()[0].tpe,
                Type::ClassRef(class) if destination[class].is_valuetype()
            ));
        }
    }

    #[test]
    fn class_value_type_disagreement_is_rejected_in_both_orders() {
        for (existing_kind, incoming_kind) in [(false, true), (true, false)] {
            let mut destination = assembly_with_class_kind(existing_kind);
            let before_counts = destination.arena_counts();
            let before_bytes = postcard::to_stdvec(&destination).unwrap();

            let result = destination.try_link_in_place(assembly_with_class_kind(incoming_kind));
            assert!(matches!(
                result,
                Err(AssemblyLinkError::ClassDefinitionConflict { .. })
            ));
            assert_eq!(destination.arena_counts(), before_counts);
            assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);
        }
    }

    fn assembly_with_kind_references(
        is_valuetype: bool,
        authoritative: bool,
        include_target_definition: bool,
    ) -> Assembly {
        let mut assembly = Assembly::default();
        let target_name = assembly.alloc_string("AuthorityReconciledTarget");
        let target_ref = assembly.alloc_class_ref(ClassRef::new(
            target_name,
            None,
            is_valuetype,
            vec![].into(),
        ));
        let target_type = Type::ClassRef(target_ref);
        let target_type_id = assembly.alloc_type(target_type);

        if include_target_definition {
            let field_name = assembly.alloc_string("self_pointer");
            let mut target = ClassDef::new(
                target_name,
                is_valuetype,
                0,
                None,
                vec![(Type::Ptr(target_type_id), field_name, None)],
                vec![],
                Access::Public,
                None,
                None,
                true,
            );
            if authoritative {
                target = target.with_valuetype_authoritative();
            }
            assembly.class_def(target).unwrap();
        }

        let consumer_name = assembly.alloc_string(if include_target_definition {
            "AuthorityReconciledConsumer"
        } else {
            "AuthorityRefOnlyConsumer"
        });
        let consumer_ref =
            assembly.alloc_class_ref(ClassRef::new(consumer_name, None, false, vec![].into()));
        let field_name = assembly.alloc_string("target_pointer");
        assembly
            .class_def(ClassDef::new(
                consumer_name,
                false,
                0,
                None,
                vec![(Type::Ptr(target_type_id), field_name, None)],
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap();

        let signature = assembly.sig([target_type], Type::Void);
        let token = assembly.alloc_node(CILNode::LdTypeToken(target_type_id));
        let pop = assembly.alloc_root(CILRoot::Pop(token));
        let ret = assembly.alloc_root(CILRoot::VoidRet);
        let method_name = assembly.alloc_string("observe_target_kind");
        assembly.new_method(MethodDef::new(
            Access::Public,
            ClassDefIdx(consumer_ref),
            method_name,
            signature,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![pop, ret], 0, None)],
                locals: vec![],
            },
            vec![None],
        ));
        assembly
    }

    fn assert_reconciled_kind(assembly: &Assembly, expected_kind: bool) {
        let matching_refs: Vec<_> = assembly
            .iter_class_refs()
            .filter(|class| &assembly[class.name()] == "AuthorityReconciledTarget")
            .collect();
        assert!(!matching_refs.is_empty());
        assert!(
            matching_refs
                .iter()
                .all(|class| class.is_valuetype() == expected_kind)
        );
        let matching_defs: Vec<_> = assembly
            .iter_class_def_ids()
            .filter_map(|id| {
                let definition = &assembly[*id];
                (&assembly[definition.name()] == "AuthorityReconciledTarget").then_some(definition)
            })
            .collect();
        assert_eq!(matching_defs.len(), 1);
        assert_eq!(matching_defs[0].is_valuetype(), expected_kind);
        assert!(matching_defs[0].is_valuetype_authoritative());
    }

    fn link_kind_references(
        destination_kind: bool,
        destination_authoritative: bool,
        incoming_kind: bool,
        incoming_authoritative: bool,
    ) -> Assembly {
        let mut destination =
            assembly_with_kind_references(destination_kind, destination_authoritative, true);
        destination
            .try_link_in_place(assembly_with_kind_references(
                incoming_kind,
                incoming_authoritative,
                true,
            ))
            .unwrap();
        destination
    }

    #[test]
    fn sole_authoritative_class_kind_rewrites_the_complete_graph_in_both_orders() {
        for authoritative_kind in [false, true] {
            let placeholder_kind = !authoritative_kind;
            let placeholder_then_authority =
                link_kind_references(placeholder_kind, false, authoritative_kind, true);
            let authority_then_placeholder =
                link_kind_references(authoritative_kind, true, placeholder_kind, false);
            assert_reconciled_kind(&placeholder_then_authority, authoritative_kind);
            assert_reconciled_kind(&authority_then_placeholder, authoritative_kind);
            assert_eq!(
                postcard::to_stdvec(&placeholder_then_authority).unwrap(),
                postcard::to_stdvec(&authority_then_placeholder).unwrap()
            );

            let (left, _) = placeholder_then_authority.compact();
            let (right, _) = authority_then_placeholder.compact();
            assert_eq!(
                postcard::to_stdvec(&left).unwrap(),
                postcard::to_stdvec(&right).unwrap()
            );

            let options = crate::ir::pe_exporter::export::ExportOptions {
                runtime: rust_dotnet_sdk_core::runtime::DotnetVersion::Net10,
                is_dll: true,
                assembly_name: "class-kind-reconciliation".into(),
                public_module_full_name: None,
                module_name: "class-kind-reconciliation.dll".into(),
                pdb_file_name: String::new(),
            };
            let left_pe = left
                .verify_for_export()
                .unwrap()
                .render_pe(&options)
                .unwrap();
            let right_pe = right
                .verify_for_export()
                .unwrap()
                .render_pe(&options)
                .unwrap();
            assert_eq!(left_pe, right_pe);
        }
    }

    #[test]
    fn authoritative_definition_rewrites_a_ref_only_shard_in_both_orders() {
        for authoritative_first in [false, true] {
            let authoritative = assembly_with_kind_references(true, true, true);
            let ref_only = assembly_with_kind_references(false, false, false);
            let (mut destination, incoming) = if authoritative_first {
                (authoritative, ref_only)
            } else {
                (ref_only, authoritative)
            };
            destination.try_link_in_place(incoming).unwrap();
            assert_reconciled_kind(&destination, true);
            assert!(destination.iter_class_refs().all(|class| {
                &destination[class.name()] != "AuthorityReconciledTarget" || class.is_valuetype()
            }));
        }
    }

    #[test]
    fn authoritative_kind_conflict_preserves_parent_in_both_orders() {
        for (existing_kind, incoming_kind) in [(false, true), (true, false)] {
            let mut destination = assembly_with_kind_references(existing_kind, true, true);
            let before_counts = destination.arena_counts();
            let before_bytes = postcard::to_stdvec(&destination).unwrap();
            let result = destination.try_link_in_place(assembly_with_kind_references(
                incoming_kind,
                true,
                true,
            ));
            assert!(matches!(
                result,
                Err(AssemblyLinkError::ClassDefinitionConflict { .. })
            ));
            assert_eq!(destination.arena_counts(), before_counts);
            assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);
        }
    }

    #[test]
    fn conflict_after_authority_rebuild_preserves_parent_bytes_and_counts() {
        let mut destination = assembly_with_kind_references(false, false, true);
        let before_counts = destination.arena_counts();
        let before_bytes = postcard::to_stdvec(&destination).unwrap();

        let mut source = assembly_with_kind_references(true, true, true);
        let consumer = source
            .iter_class_def_ids()
            .copied()
            .find(|id| &source[source[*id].name()] == "AuthorityReconciledConsumer")
            .expect("consumer definition");
        source.class_mut(consumer).fields_mut()[0].0 = Type::Int(Int::I32);

        let result = destination.try_link_in_place(source);
        assert!(matches!(
            result,
            Err(AssemblyLinkError::ClassFieldConflict { .. })
        ));
        assert_eq!(destination.arena_counts(), before_counts);
        assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);
    }

    fn assembly_with_member_metadata(event_type: Type, property_type: Type) -> Assembly {
        let mut assembly = Assembly::default();
        let class_name = assembly.alloc_string("MemberMetadataCollision");
        let class_ref =
            assembly.alloc_class_ref(ClassRef::new(class_name, None, false, vec![].into()));
        let signature = assembly.sig([], Type::Void);
        let add = assembly.new_methodref(
            class_ref,
            "add_Changed",
            signature,
            MethodKind::Instance,
            vec![],
        );
        let remove = assembly.new_methodref(
            class_ref,
            "remove_Changed",
            signature,
            MethodKind::Instance,
            vec![],
        );
        let getter = assembly.new_methodref(
            class_ref,
            "get_Value",
            signature,
            MethodKind::Instance,
            vec![],
        );
        let mut definition = ClassDef::new(
            class_name,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Private,
            None,
            None,
            true,
        );
        let event_name = assembly.alloc_string("Changed");
        definition.add_event(EventDef::new(event_name, event_type, add, remove));
        let property_name = assembly.alloc_string("Value");
        definition.add_property(PropertyDef::new(
            property_name,
            property_type,
            Some(getter),
            None,
        ));
        assembly.class_def(definition).unwrap();
        assembly
    }

    #[test]
    fn same_named_event_and_property_conflicts_are_rejected_before_commit() {
        let mut event_destination =
            assembly_with_member_metadata(Type::Int(Int::I32), Type::Int(Int::I32));
        let before_counts = event_destination.arena_counts();
        let before_bytes = postcard::to_stdvec(&event_destination).unwrap();
        let event_result = event_destination.try_link_in_place(assembly_with_member_metadata(
            Type::Int(Int::I64),
            Type::Int(Int::I32),
        ));
        assert!(matches!(
            event_result,
            Err(AssemblyLinkError::ClassDefinitionConflict { .. })
        ));
        assert_eq!(event_destination.arena_counts(), before_counts);
        assert_eq!(
            postcard::to_stdvec(&event_destination).unwrap(),
            before_bytes
        );

        let mut property_destination =
            assembly_with_member_metadata(Type::Int(Int::I32), Type::Int(Int::I32));
        let before_counts = property_destination.arena_counts();
        let before_bytes = postcard::to_stdvec(&property_destination).unwrap();
        let property_result = property_destination.try_link_in_place(
            assembly_with_member_metadata(Type::Int(Int::I32), Type::Int(Int::I64)),
        );
        assert!(matches!(
            property_result,
            Err(AssemblyLinkError::ClassDefinitionConflict { .. })
        ));
        assert_eq!(property_destination.arena_counts(), before_counts);
        assert_eq!(
            postcard::to_stdvec(&property_destination).unwrap(),
            before_bytes
        );
    }

    fn ordered_test_attributes(
        assembly: &mut Assembly,
        reverse: bool,
        different: bool,
    ) -> Vec<CustomAttrDef> {
        let first_name = assembly.alloc_string("OrderAttributeA");
        let second_name = assembly.alloc_string("OrderAttributeB");
        let first_type =
            assembly.alloc_class_ref(ClassRef::new(first_name, None, false, vec![].into()));
        let second_type =
            assembly.alloc_class_ref(ClassRef::new(second_name, None, false, vec![].into()));
        let field_name = assembly.alloc_string("NamedField");
        let property_name = assembly.alloc_string("NamedProperty");
        let mut named = vec![
            CustomAttrNamedArg::field(field_name, CustomAttrArg::I32(1)),
            CustomAttrNamedArg::property(
                property_name,
                CustomAttrArg::I32(if different { 99 } else { 2 }),
            ),
        ];
        if reverse {
            named.reverse();
        }
        let mut attributes = vec![
            CustomAttrDef::new_with_named_args(first_type, vec![], named),
            CustomAttrDef::new(second_type, vec![CustomAttrArg::Bool(true)], vec![]),
        ];
        if reverse {
            attributes.reverse();
        }
        attributes
    }

    fn assembly_with_ordered_property_attributes(reverse: bool, different: bool) -> Assembly {
        let mut assembly = Assembly::default();
        let owner_name = assembly.alloc_string("OrderedPropertyOwner");
        let owner = assembly.alloc_class_ref(ClassRef::new(owner_name, None, false, vec![].into()));
        let getter_signature = assembly.sig([], Type::Int(Int::I32));
        let getter = assembly.new_methodref(
            owner,
            "get_Value",
            getter_signature,
            MethodKind::Instance,
            vec![],
        );
        let property_name = assembly.alloc_string("Value");
        let attributes = ordered_test_attributes(&mut assembly, reverse, different);
        let property = PropertyDef::new(property_name, Type::Int(Int::I32), Some(getter), None)
            .with_custom_attributes(attributes);
        let mut definition = ClassDef::new(
            owner_name,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Private,
            None,
            None,
            true,
        );
        definition.add_property(property);
        assembly.class_def(definition).unwrap();
        assembly
    }

    fn assembly_with_ordered_static_field_attributes(reverse: bool, different: bool) -> Assembly {
        let mut assembly = Assembly::default();
        let owner_name = assembly.alloc_string("OrderedStaticOwner");
        let field_name = assembly.alloc_string("Value");
        let mut definition = ClassDef::new(
            owner_name,
            false,
            0,
            None,
            vec![],
            vec![StaticFieldDef {
                tpe: Type::Int(Int::I32),
                name: field_name,
                is_tls: false,
                default_value: Some(Const::I32(1)),
                is_const: true,
            }],
            Access::Private,
            None,
            None,
            true,
        );
        for attribute in ordered_test_attributes(&mut assembly, reverse, different) {
            definition.add_field_custom_attribute(field_name, true, attribute);
        }
        assembly.class_def(definition).unwrap();
        assembly
    }

    #[test]
    fn attribute_order_is_not_a_property_or_static_field_link_semantic() {
        let mut property = assembly_with_ordered_property_attributes(false, false);
        property
            .try_link_in_place(assembly_with_ordered_property_attributes(true, false))
            .unwrap();
        let definition = property
            .class_defs()
            .values()
            .find(|definition| &property[definition.name()] == "OrderedPropertyOwner")
            .unwrap();
        assert_eq!(definition.properties().len(), 1);
        assert_eq!(definition.properties()[0].custom_attributes().len(), 2);

        let mut statics = assembly_with_ordered_static_field_attributes(false, false);
        statics
            .try_link_in_place(assembly_with_ordered_static_field_attributes(true, false))
            .unwrap();
        let definition = statics
            .class_defs()
            .values()
            .find(|definition| &statics[definition.name()] == "OrderedStaticOwner")
            .unwrap();
        assert_eq!(definition.static_fields().len(), 1);
        assert_eq!(
            definition
                .field_custom_attributes(definition.static_fields()[0].name, true)
                .count(),
            2
        );

        assert!(matches!(
            assembly_with_ordered_property_attributes(false, false)
                .try_link_in_place(assembly_with_ordered_property_attributes(true, true)),
            Err(AssemblyLinkError::ClassDefinitionConflict { .. })
        ));
        assert!(matches!(
            assembly_with_ordered_static_field_attributes(false, false)
                .try_link_in_place(assembly_with_ordered_static_field_attributes(true, true)),
            Err(AssemblyLinkError::StaticFieldConflict { .. })
        ));
    }

    fn assembly_with_instance_field_attribute(value: i32) -> Assembly {
        let mut assembly = Assembly::default();
        let owner_name = assembly.alloc_string("AttributedFieldOwner");
        let field_name = assembly.alloc_string("value");
        let attribute_name = assembly.alloc_string("FieldAttribute");
        let attribute_type =
            assembly.alloc_class_ref(ClassRef::new(attribute_name, None, false, vec![].into()));
        let mut definition = ClassDef::new(
            owner_name,
            false,
            0,
            None,
            vec![(Type::Int(Int::I32), field_name, None)],
            vec![],
            Access::Private,
            None,
            None,
            true,
        );
        definition.add_field_custom_attribute(
            field_name,
            false,
            CustomAttrDef::new(attribute_type, vec![CustomAttrArg::I32(value)], vec![]),
        );
        assembly.class_def(definition).unwrap();
        assembly
    }

    fn add_instance_field_attribute(assembly: &mut Assembly, value: i32) {
        let owner = assembly
            .iter_class_def_ids()
            .copied()
            .find(|class| &assembly[assembly[*class].name()] == "AttributedFieldOwner")
            .unwrap();
        let field_name = assembly[owner].fields()[0].1;
        let attribute_name = assembly.alloc_string("FieldAttribute");
        let attribute_type =
            assembly.alloc_class_ref(ClassRef::new(attribute_name, None, false, vec![].into()));
        assembly.class_mut(owner).add_field_custom_attribute(
            field_name,
            false,
            CustomAttrDef::new(attribute_type, vec![CustomAttrArg::I32(value)], vec![]),
        );
    }

    fn instance_field_attribute_count(assembly: &Assembly) -> usize {
        let definition = assembly
            .class_defs()
            .values()
            .find(|definition| &assembly[definition.name()] == "AttributedFieldOwner")
            .unwrap();
        definition
            .field_custom_attributes(definition.fields()[0].1, false)
            .count()
    }

    #[test]
    fn field_attribute_dedup_survives_link_mutation_and_artifact_roundtrip() {
        let mut assembly = assembly_with_instance_field_attribute(1);
        assembly
            .try_link_in_place(assembly_with_instance_field_attribute(2))
            .unwrap();
        assert_eq!(instance_field_attribute_count(&assembly), 2);
        add_instance_field_attribute(&mut assembly, 2);
        assert_eq!(instance_field_attribute_count(&assembly), 2);

        let encoded = crate::artifact::AssemblyArtifact::new(
            assembly,
            crate::artifact::ArtifactAbiConfig::default(),
        )
        .encode()
        .unwrap();
        let mut assembly = crate::artifact::decode_assembly_artifact(&encoded)
            .unwrap()
            .into_parts()
            .1;
        add_instance_field_attribute(&mut assembly, 2);
        assert_eq!(instance_field_attribute_count(&assembly), 2);
    }

    #[derive(Clone, Copy, Debug)]
    enum SpecialInitializerShape {
        Empty,
        Mergeable,
        TwoBlocks,
        DependentTwoBlocks,
        EarlyExit,
    }

    fn assembly_with_special_initializer(
        name: &str,
        body_value: i32,
        shape: SpecialInitializerShape,
    ) -> Assembly {
        let mut assembly = Assembly::default();
        let owner = assembly.main_module();
        let name = assembly.alloc_string(name);
        let signature = assembly.sig([], Type::Void);
        let value = assembly.alloc_node(Const::I32(body_value));
        let pop = assembly.alloc_root(CILRoot::Pop(value));
        let ret = assembly.alloc_root(CILRoot::VoidRet);
        let (blocks, locals) = match shape {
            SpecialInitializerShape::Empty => (vec![BasicBlock::new(vec![ret], 0, None)], vec![]),
            SpecialInitializerShape::Mergeable => {
                (vec![BasicBlock::new(vec![pop, ret], 0, None)], vec![])
            }
            SpecialInitializerShape::TwoBlocks => (
                vec![
                    BasicBlock::new(vec![pop, ret], 0, None),
                    BasicBlock::new(vec![ret], 1, None),
                ],
                vec![],
            ),
            SpecialInitializerShape::DependentTwoBlocks => {
                let store = assembly.alloc_root(CILRoot::StLoc(0, value));
                let branch = assembly.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));
                let load = assembly.alloc_node(CILNode::LdLoc(0));
                let use_local = assembly.alloc_root(CILRoot::Pop(load));
                let local_type = assembly.alloc_type(Type::Int(Int::I32));
                (
                    vec![
                        BasicBlock::new(vec![store, branch], 0, None),
                        BasicBlock::new(vec![use_local, ret], 1, None),
                    ],
                    vec![(None, local_type)],
                )
            }
            SpecialInitializerShape::EarlyExit => {
                let nop = assembly.alloc_root(CILRoot::Nop);
                (
                    vec![BasicBlock::new(vec![pop, ret, nop, ret], 0, None)],
                    vec![],
                )
            }
        };
        assembly.new_method(MethodDef::new(
            Access::Public,
            owner,
            name,
            signature,
            MethodKind::Static,
            MethodImpl::MethodBody { blocks, locals },
            vec![],
        ));
        assembly
    }

    fn assembly_with_missing_special_initializer(name: &str) -> Assembly {
        let mut assembly = Assembly::default();
        let owner = assembly.main_module();
        let name = assembly.alloc_string(name);
        let signature = assembly.sig([], Type::Void);
        assembly.new_method(MethodDef::new(
            Access::Public,
            owner,
            name,
            signature,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![],
        ));
        assembly
    }

    fn special_initializer_values(assembly: &Assembly, name: &str) -> Vec<i32> {
        let method = assembly
            .method_defs()
            .values()
            .find(|method| &assembly[method.name()] == name)
            .expect("special initializer definition");
        let MethodImpl::MethodBody { blocks, .. } = method.implementation() else {
            panic!("special initializer must retain a body");
        };
        blocks
            .iter()
            .flat_map(|block| block.roots().iter())
            .filter_map(|root| {
                let CILRoot::Pop(value) = assembly.get_root(*root) else {
                    return None;
                };
                let CILNode::Const(value) = assembly.get_node(*value) else {
                    return None;
                };
                let Const::I32(value) = **value else {
                    return None;
                };
                Some(value)
            })
            .collect()
    }

    #[test]
    fn special_initializer_merge_is_order_independent_and_keeps_every_fragment() {
        for name in [CCTOR, TCCTOR, USER_INIT] {
            let mut left =
                assembly_with_special_initializer(name, 1, SpecialInitializerShape::Mergeable);
            left.try_link_in_place(assembly_with_special_initializer(
                name,
                2,
                SpecialInitializerShape::Mergeable,
            ))
            .unwrap();
            let mut right =
                assembly_with_special_initializer(name, 2, SpecialInitializerShape::Mergeable);
            right
                .try_link_in_place(assembly_with_special_initializer(
                    name,
                    1,
                    SpecialInitializerShape::Mergeable,
                ))
                .unwrap();

            let left_values = special_initializer_values(&left, name);
            let right_values = special_initializer_values(&right, name);
            assert_eq!(left_values, right_values);
            let mut retained = left_values;
            retained.sort_unstable();
            assert_eq!(retained, [1, 2]);

            let (left, _) = left.compact();
            let (right, _) = right.compact();
            assert_eq!(
                postcard::to_stdvec(&left).unwrap(),
                postcard::to_stdvec(&right).unwrap()
            );
        }
    }

    #[test]
    fn special_initializer_merge_is_associative_across_every_three_way_order() {
        const ORDERS: [[i32; 3]; 6] = [
            [1, 2, 3],
            [1, 3, 2],
            [2, 1, 3],
            [2, 3, 1],
            [3, 1, 2],
            [3, 2, 1],
        ];
        for name in [CCTOR, TCCTOR, USER_INIT] {
            let mut canonical_bytes = None;
            let mut canonical_pe = None;
            for order in ORDERS {
                let mut assembly = assembly_with_special_initializer(
                    name,
                    order[0],
                    SpecialInitializerShape::Mergeable,
                );
                for value in &order[1..] {
                    assembly
                        .try_link_in_place(assembly_with_special_initializer(
                            name,
                            *value,
                            SpecialInitializerShape::Mergeable,
                        ))
                        .unwrap();
                }
                assert_eq!(special_initializer_values(&assembly, name), [1, 2, 3]);
                let (assembly, _) = assembly.compact();
                let bytes = postcard::to_stdvec(&assembly).unwrap();
                match &canonical_bytes {
                    Some(canonical) => assert_eq!(&bytes, canonical),
                    None => canonical_bytes = Some(bytes),
                }
                let options = crate::ir::pe_exporter::export::ExportOptions {
                    runtime: rust_dotnet_sdk_core::runtime::DotnetVersion::Net10,
                    is_dll: true,
                    assembly_name: "special-initializer-order".into(),
                    public_module_full_name: None,
                    module_name: "special-initializer-order.dll".into(),
                    pdb_file_name: String::new(),
                };
                let pe = assembly
                    .verify_for_export()
                    .unwrap()
                    .render_pe(&options)
                    .unwrap();
                match &canonical_pe {
                    Some(canonical) => assert_eq!(&pe, canonical),
                    None => canonical_pe = Some(pe),
                }
            }
        }
    }

    #[test]
    fn empty_special_initializers_are_merge_identities_across_artifact_roundtrip() {
        for name in [CCTOR, TCCTOR, USER_INIT] {
            let mut assembly =
                assembly_with_special_initializer(name, 0, SpecialInitializerShape::Empty);
            assembly
                .try_link_in_place(assembly_with_special_initializer(
                    name,
                    0,
                    SpecialInitializerShape::Empty,
                ))
                .unwrap();
            let method = assembly
                .method_defs()
                .values()
                .find(|method| &assembly[method.name()] == name)
                .unwrap();
            let MethodImpl::MethodBody { blocks, .. } = method.implementation() else {
                panic!("special initializer must retain a body");
            };
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].roots().len(), 1);
            assert_eq!(assembly.get_root(blocks[0].roots()[0]), &CILRoot::VoidRet);

            let encoded = crate::artifact::AssemblyArtifact::new(
                assembly,
                crate::artifact::ArtifactAbiConfig::default(),
            )
            .encode()
            .unwrap();
            let mut assembly = crate::artifact::decode_assembly_artifact(&encoded)
                .unwrap()
                .into_parts()
                .1;
            assembly
                .try_link_in_place(assembly_with_special_initializer(
                    name,
                    7,
                    SpecialInitializerShape::Mergeable,
                ))
                .unwrap();
            assert_eq!(special_initializer_values(&assembly, name), [7]);
        }
    }

    #[test]
    fn persistent_index_reuses_accumulated_special_initializer_fragment_keys() {
        for shards in [32_usize, 64, 128] {
            reset_special_method_fragment_key_builds();
            let mut destination = assembly_with_special_initializer(
                USER_INIT,
                -1,
                SpecialInitializerShape::Mergeable,
            );
            let mut total = LinkPreflightStats::default();
            for shard in 0..shards {
                let stats = destination
                    .try_link_in_place(assembly_with_special_initializer(
                        USER_INIT,
                        i32::try_from(shard).unwrap(),
                        SpecialInitializerShape::Mergeable,
                    ))
                    .unwrap();
                total.accumulate(stats.preflight);
            }

            // One key for the initial destination fragment and one per one-fragment source shard.
            // The commit path consumes those cached orders and never re-canonicalizes the growing
            // destination body.
            assert_eq!(special_method_fragment_key_builds(), shards + 1);
            assert_eq!(
                total.method_definition_semantic_keys_built,
                2 * (shards + 1)
            );
            assert_eq!(total.cross_method_definition_comparisons, shards);
            assert_eq!(
                special_initializer_values(&destination, USER_INIT).len(),
                shards + 1
            );
        }
    }

    #[test]
    fn nonmergeable_special_initializers_are_rejected_symmetrically_before_commit() {
        for name in [CCTOR, TCCTOR, USER_INIT] {
            for shape in [
                SpecialInitializerShape::TwoBlocks,
                SpecialInitializerShape::DependentTwoBlocks,
                SpecialInitializerShape::EarlyExit,
            ] {
                for invalid_is_destination in [false, true] {
                    let invalid = assembly_with_special_initializer(name, 2, shape);
                    let valid = assembly_with_special_initializer(
                        name,
                        1,
                        SpecialInitializerShape::Mergeable,
                    );
                    let (destination, source) = if invalid_is_destination {
                        (invalid, valid)
                    } else {
                        (valid, invalid)
                    };
                    let destination_counts = destination.arena_counts();
                    let source_counts = source.arena_counts();
                    let destination_bytes = postcard::to_stdvec(&destination).unwrap();
                    let source_bytes = postcard::to_stdvec(&source).unwrap();

                    let destination_method = *destination.method_defs().keys().next().unwrap();
                    let source_method = *source.method_defs().keys().next().unwrap();
                    assert_eq!(
                        destination.method_semantic_key(destination_method),
                        source.method_semantic_key(source_method)
                    );
                    let (invalid_assembly, invalid_method) = if invalid_is_destination {
                        (&destination, destination_method)
                    } else {
                        (&source, source_method)
                    };
                    assert!(
                        build_special_method_link_info(
                            invalid_assembly,
                            invalid_assembly.method_def(invalid_method),
                            "invalid",
                        )
                        .is_err()
                    );

                    let error = match preflight_assembly_link(&destination, &source) {
                        Err(error) => error,
                        Ok(_) => panic!(
                            "nonmergeable special initializer passed preflight: \
                             name={name}, shape={shape:?}, invalid_is_destination={invalid_is_destination}"
                        ),
                    };
                    assert!(matches!(error, AssemblyLinkError::MethodConflict { .. }));
                    let mut attempted_destination = destination.clone();
                    assert!(matches!(
                        attempted_destination.try_link_in_place(source.clone()),
                        Err(AssemblyLinkError::MethodConflict { .. })
                    ));
                    assert_eq!(attempted_destination.arena_counts(), destination_counts);
                    assert_eq!(
                        postcard::to_stdvec(&attempted_destination).unwrap(),
                        destination_bytes
                    );
                    assert_eq!(destination.arena_counts(), destination_counts);
                    assert_eq!(source.arena_counts(), source_counts);
                    assert_eq!(
                        postcard::to_stdvec(&destination).unwrap(),
                        destination_bytes
                    );
                    assert_eq!(postcard::to_stdvec(&source).unwrap(), source_bytes);
                }
            }
        }
    }

    #[test]
    fn malformed_special_initializer_is_rejected_even_against_missing() {
        for name in [CCTOR, TCCTOR, USER_INIT] {
            for invalid_is_destination in [false, true] {
                let invalid =
                    assembly_with_special_initializer(name, 1, SpecialInitializerShape::TwoBlocks);
                let missing = assembly_with_missing_special_initializer(name);
                let (mut destination, source) = if invalid_is_destination {
                    (invalid, missing)
                } else {
                    (missing, invalid)
                };
                let before = postcard::to_stdvec(&destination).unwrap();
                let error = destination.try_link_in_place(source).unwrap_err();
                assert!(matches!(error, AssemblyLinkError::MethodConflict { .. }));
                assert_eq!(postcard::to_stdvec(&destination).unwrap(), before);
            }
        }
    }

    #[test]
    fn special_body_and_method_access_mutations_invalidate_the_persistent_index() {
        let mut initializer =
            assembly_with_special_initializer(USER_INIT, 10, SpecialInitializerShape::Mergeable);
        initializer.try_link_in_place(Assembly::default()).unwrap();
        assert!(initializer.link_preflight_index.is_some());
        let extra = initializer.alloc_node(Const::I32(11));
        let extra = initializer.alloc_root(CILRoot::Pop(extra));
        initializer.add_user_init(&[extra]);
        assert!(initializer.link_preflight_index.is_none());
        initializer
            .try_link_in_place(assembly_with_special_initializer(
                USER_INIT,
                20,
                SpecialInitializerShape::Mergeable,
            ))
            .unwrap();
        let values = special_initializer_values(&initializer, USER_INIT);
        let ten = values.iter().position(|value| *value == 10).unwrap();
        assert_eq!(values.get(ten + 1), Some(&11));

        let mut methods = assembly_with_real_method(1);
        methods.try_link_in_place(Assembly::default()).unwrap();
        assert!(methods.link_preflight_index.is_some());
        assert_eq!(methods.hide_main_module_implementation_details(), 1);
        assert!(methods.link_preflight_index.is_none());
    }

    #[test]
    fn incompatible_special_method_implementation_is_rejected_before_commit() {
        let mut destination = Assembly::default();
        let ret = destination.alloc_root(CILRoot::VoidRet);
        add_void_method(
            &mut destination,
            CCTOR,
            vec![BasicBlock::new(vec![ret], 0, None)],
        );
        let before_counts = destination.arena_counts();
        let before_bytes = postcard::to_stdvec(&destination).unwrap();

        let mut source = Assembly::default();
        let owner = source.main_module();
        let name = source.alloc_string(CCTOR);
        let signature = source.sig([], Type::Void);
        let library = source.alloc_string("incompatible-special-method");
        source.new_method(MethodDef::new(
            Access::Public,
            owner,
            name,
            signature,
            MethodKind::Static,
            MethodImpl::Extern {
                lib: library,
                entry_point: None,
                call_conv: crate::PInvokeCallConv::Cdecl,
                preserve_errno: false,
            },
            vec![],
        ));

        let result = destination.try_link_in_place(source);
        assert!(matches!(
            result,
            Err(AssemblyLinkError::MethodConflict { .. })
        ));
        assert_eq!(destination.arena_counts(), before_counts);
        assert_eq!(postcard::to_stdvec(&destination).unwrap(), before_bytes);
    }
}
