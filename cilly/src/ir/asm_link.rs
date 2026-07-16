use super::{
    Assembly, CILNode, CILRoot, ClassDef, ClassRef, Const, FieldDesc, FnSig, MethodDefIdx,
    MethodRef, StaticFieldDesc, Type,
    asm::{CCTOR, TCCTOR, USER_INIT},
    bimap::Interned,
    class::ClassDefIdx,
};

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
pub(crate) struct RelocateCtx<'source> {
    source: &'source Assembly,
    /// Optional final-link projection for the compiler's internal `MainModule` sentinel.
    ///
    /// It is intentionally applied while relocating rather than changing codegen: old serialized
    /// artifacts remain readable and all definition/reference arenas are rewritten together.
    main_module_name: Option<&'source str>,
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
        Self::with_main_module_name(source, None)
    }

    fn with_main_module_name(
        source: &'source Assembly,
        main_module_name: Option<&'source str>,
    ) -> Self {
        Self {
            source,
            main_module_name,
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
        let class_ref = self
            .source
            .class_ref(source)
            .clone()
            .relocate(self, destination);
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
            CILRoot::VoidRet | CILRoot::Break | CILRoot::Nop | CILRoot::ReThrow => root,
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
    pub(crate) fn translate_class_def(&mut self, ctx: &mut RelocateCtx<'_>, def: &ClassDef) {
        let super::class::RelocatedClassDef {
            definition: translated,
            source_methods,
        } = def.clone().relocate(ctx, self);
        let class_ref = self.alloc_class_ref(translated.ref_to());
        if let Some(existing_def) = self.class_defs().get(&ClassDefIdx(class_ref)) {
            for incoming @ (_, incoming_name, _) in translated.fields() {
                if let Some(existing) = existing_def
                    .fields()
                    .iter()
                    .find(|(_, existing_name, _)| existing_name == incoming_name)
                {
                    if existing != incoming {
                        let existing_type = existing.0.mangle(self);
                        let incoming_type = incoming.0.mangle(self);
                        panic!(
                            "class field differs across codegen shards: class={}, field={}, \
                             existing={existing:?} ({existing_type}), incoming={incoming:?} \
                             ({incoming_type})",
                            &self[translated.name()],
                            &self[*incoming_name]
                        );
                    }
                }
            }
        }
        let (defs_mut, _) = self.class_defs_mut_strings();
        match defs_mut.entry(ClassDefIdx(class_ref)) {
            std::collections::hash_map::Entry::Occupied(mut occupied) => {
                occupied.get_mut().merge_defs(translated);
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(translated);
            }
        }

        for source_method in source_methods {
            let source_method = ctx.source.method_def(source_method).clone();
            let mut method_def = source_method.relocate(ctx, self);
            let method_ref = self.alloc_methodref(method_def.ref_to());
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
                        method_def
                            .implementation_mut()
                            .merge_cctor_impls(original.implementation(), self);
                        method_def
                    } else {
                        assert_eq!(method_def.access(), original.access());
                        assert_eq!(method_def.class(), original.class());
                        assert_eq!(method_def.sig(), original.sig());
                        assert_eq!(method_def.kind(), original.kind());
                        method_def
                    }
                }
                None => method_def,
            };
            self.new_method(method_def);
        }
    }
}
const SPECIAL_METHOD_NAMES: &[&str] = &[CCTOR, TCCTOR, USER_INIT];

pub(crate) fn relocate_assembly(
    mut destination: Assembly,
    source: &Assembly,
) -> (Assembly, RelocationStats) {
    source.assert_relocation_arena_coverage();
    destination.assert_relocation_arena_coverage();
    let original_str = destination.alloc_string(super::asm::MAIN_MODULE);
    let mut class_ids: Vec<_> = source.iter_class_def_ids().copied().collect();
    class_ids.sort_unstable_by_key(|class_id| class_id.0.inner());
    let mut ctx = RelocateCtx::new(source);
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
        destination.translate_class_def(&mut ctx, def);
    }
    assert_eq!(
        destination.alloc_string(super::asm::MAIN_MODULE),
        original_str
    );
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
        destination.translate_class_def(&mut ctx, def);
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
}
