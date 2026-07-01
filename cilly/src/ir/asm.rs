use super::{
    bimap::{BiMap, BiMapIndex, Interned, IntoBiMapIndex},
    cilnode::{BinOp, ExtendKind, IsPure, MethodKind, PtrCastRes, UnOp},
    class::{ClassDefIdx, LayoutError, StaticFieldDef},
    opt::{OptFuel, SideEffectInfoCache},
    Access, CILNode, CILRoot, ClassDef, ClassRef, Const, Exporter, FieldDesc, FnSig, Int,
    IntoAsmIndex, MethodDef, MethodDefIdx, MethodRef, StaticFieldDesc, Type,
};
use crate::{config, utilis::assert_unique, IString};
use crate::{utilis::encode, MethodImpl};
use fxhash::{hash64, FxHashMap, FxHashSet};

use serde::{Deserialize, Serialize};
use std::{any::type_name, ops::Index};

pub type MissingMethodPatcher =
    FxHashMap<Interned<IString>, Box<dyn Fn(Interned<MethodRef>, &mut Assembly) -> MethodImpl>>;
type StringMap = BiMap<IString>;
type TypeMap = BiMap<Type>;
#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Assembly {
    /// A list of strings used in this assembly
    strings: StringMap,
    /// A list of all types in this assembly
    types: TypeMap,
    class_refs: BiMap<ClassRef>,
    class_defs: FxHashMap<ClassDefIdx, ClassDef>,
    nodes: BiMap<CILNode>,
    roots: BiMap<CILRoot>,
    sigs: BiMap<FnSig>,
    method_refs: BiMap<MethodRef>,
    fields: BiMap<FieldDesc>,
    statics: BiMap<StaticFieldDesc>,
    method_defs: FxHashMap<MethodDefIdx, MethodDef>,
    sections: FxHashMap<String, Vec<u8>>,
    /// A list of all buffers within this assembly.
    pub(crate) const_data: BiMap<Box<[u8]>>,
}
impl Index<Interned<IString>> for Assembly {
    type Output = str;

    fn index(&self, index: Interned<IString>) -> &Self::Output {
        &self.strings[index]
    }
}
impl Index<ClassDefIdx> for Assembly {
    type Output = ClassDef;

    fn index(&self, index: ClassDefIdx) -> &Self::Output {
        &self.class_defs[&index]
    }
}
impl Index<Interned<MethodRef>> for Assembly {
    type Output = MethodRef;

    fn index(&self, index: Interned<MethodRef>) -> &Self::Output {
        &self.method_refs[index]
    }
}
impl Index<MethodDefIdx> for Assembly {
    type Output = MethodDef;

    fn index(&self, index: MethodDefIdx) -> &Self::Output {
        &self.method_defs[&index]
    }
}
impl Index<Interned<ClassRef>> for Assembly {
    type Output = ClassRef;

    fn index(&self, index: Interned<ClassRef>) -> &Self::Output {
        &self.class_refs[index]
    }
}
impl Index<Interned<Type>> for Assembly {
    type Output = Type;

    fn index(&self, index: Interned<Type>) -> &Self::Output {
        &self.types[index]
    }
}
impl Index<Interned<FnSig>> for Assembly {
    type Output = FnSig;

    fn index(&self, index: Interned<FnSig>) -> &Self::Output {
        &self.sigs[index]
    }
}
impl Index<Interned<CILRoot>> for Assembly {
    type Output = CILRoot;

    fn index(&self, index: Interned<CILRoot>) -> &Self::Output {
        &self.roots[index]
    }
}
impl Index<Interned<CILNode>> for Assembly {
    type Output = CILNode;

    fn index(&self, index: Interned<CILNode>) -> &Self::Output {
        &self.nodes[index]
    }
}
impl Index<Interned<StaticFieldDesc>> for Assembly {
    type Output = StaticFieldDesc;

    fn index(&self, index: Interned<StaticFieldDesc>) -> &Self::Output {
        &self.statics[index]
    }
}
impl Index<Interned<FieldDesc>> for Assembly {
    type Output = FieldDesc;

    fn index(&self, index: Interned<FieldDesc>) -> &Self::Output {
        &self.fields[index]
    }
}
impl Assembly {
    /// Returns a pointer to an immutable(!) byte buffer of a given type.
    pub fn bytebuffer(
        &mut self,
        buffer: &[u8],
        tpe: impl IntoAsmIndex<Interned<Type>>,
    ) -> Interned<CILNode> {
        let data = self.const_data.alloc(buffer.into());
        let tpe = tpe.into_idx(self);
        self.alloc_node(Const::ByteBuffer { data, tpe })
    }
    /// Offsets `addr` by `index` * sizeof(`tpe`)
    pub fn offset(
        &mut self,
        addr: impl IntoAsmIndex<Interned<CILNode>>,
        index: impl IntoAsmIndex<Interned<CILNode>>,
        tpe: impl IntoAsmIndex<Interned<Type>>,
    ) -> Interned<CILNode> {
        let index = index.into_idx(self);
        // A byte offset is inherently a usize/native-int quantity, but the index can arrive
        // narrower — notably the `u32` lane index of `simd_extract`/`simd_insert`. Zero-extend
        // it to USize so the `index * stride` Mul has matching operand widths (an array/lane
        // index is a small non-negative value, so the zero-extension is value-preserving and
        // computes the identical byte address). Redundant USize→USize casts on the array/slice
        // callers (which already pass USize) are well-typed and optimized away.
        let index = self.int_cast(index, Int::USize, ExtendKind::ZeroExtend);
        let stride = self.size_of(tpe);
        let stride = self.int_cast(stride, Int::USize, ExtendKind::ZeroExtend);
        let offset = self.biop(index, stride, BinOp::Mul);
        self.biop(addr, offset, BinOp::Add)
    }
    /// Dereferences `addr`, loading data of type `tpe`
    pub fn load(
        &mut self,
        addr: impl IntoAsmIndex<Interned<CILNode>>,
        tpe: impl IntoAsmIndex<Interned<Type>>,
    ) -> Interned<CILNode> {
        let addr = addr.into_idx(self);
        let tpe = tpe.into_idx(self);
        self.alloc_node(CILNode::LdInd {
            addr,
            tpe,
            volatile: false,
        })
    }
    /// Gets the field of a valuetype / pointer `addr`.
    pub fn ld_field(
        &mut self,
        addr: impl IntoAsmIndex<Interned<CILNode>>,
        field: impl IntoAsmIndex<Interned<FieldDesc>>,
    ) -> Interned<CILNode> {
        let addr = addr.into_idx(self);
        let field = field.into_idx(self);
        self.alloc_node(CILNode::LdField { addr, field })
    }
    /// Casts a pointer / usize / isize (`addr`) to a pointer to `tpe`.
    pub fn cast_ptr(
        &mut self,
        addr: impl IntoAsmIndex<Interned<CILNode>>,
        tpe: impl IntoAsmIndex<Interned<Type>>,
    ) -> Interned<CILNode> {
        let addr = addr.into_idx(self);
        let tpe = tpe.into_idx(self);
        self.alloc_node(CILNode::PtrCast(addr, Box::new(PtrCastRes::Ptr(tpe))))
    }
    /// Gets the addres of a field of a pointer to valuetype `addr`.
    pub fn ld_field_addr(
        &mut self,
        addr: impl IntoAsmIndex<Interned<CILNode>>,
        field: impl IntoAsmIndex<Interned<FieldDesc>>,
    ) -> Interned<CILNode> {
        let addr = addr.into_idx(self);
        let field = field.into_idx(self);
        self.alloc_node(CILNode::LdFieldAddress { addr, field })
    }
    /// Run the CIL type-verifier over every emitted method.
    ///
    /// Wiring for Phase P1 of `docs/ABSOLUTE_CORRECTNESS_PLAN.md` (invariant I1). Behaviour is
    /// controlled by three env flags (declared in `cilly/src/lib.rs`):
    ///  * `TYPECHECK_CIL` / `VERIFY_METHODS` — if *both* are `0`, the verifier is skipped entirely
    ///    (escape hatch; default is on).
    ///  * `ALLOW_MISCOMPILATIONS` — when `true` (default) a violation is logged and codegen
    ///    continues (historical advisory behaviour); when `false` the **first** violation makes this
    ///    function `panic!`, naming the offending method + the typecheck error, which aborts the
    ///    rustc/linker process and fails the build. That is the "fatal type gate".
    ///
    /// Returns the number of methods that failed to typecheck (0 when the assembly is clean). Callers
    /// in advisory mode may ignore it; in fatal mode a non-zero count never returns (we panic first).
    pub fn typecheck(&mut self) -> usize {
        // Read the wiring flags once, then delegate to the pure-policy implementation. Splitting it
        // out keeps the fatal/advisory decision unit-testable without fighting the env `LazyLock`s.
        let enabled = *crate::TYPECHECK_CIL || *crate::VERIFY_METHODS;
        let fatal = !*crate::ALLOW_MISCOMPILATIONS;
        self.typecheck_with_policy(enabled, fatal)
    }
    /// Policy core of [`Assembly::typecheck`]. `enabled` gates the whole pass; `fatal` makes the
    /// first violation `panic!` (the I1 build-failing gate) instead of just logging it.
    pub fn typecheck_with_policy(&mut self, enabled: bool, fatal: bool) -> usize {
        if !enabled {
            return 0;
        }
        let method_def_idxs: Box<[_]> = self.method_defs.keys().copied().collect();
        let dump_filter = crate::dump_fn_filter();
        let mut violations = 0usize;
        for method in method_def_idxs {
            let mut tmp_method = self.method_def(method).clone();
            // DUMP_FN tooling: emit a readable, type-annotated dump of any method whose (de)mangled
            // name contains the filter substring — fires whether or not it passes the checker, so a
            // failing method can be inspected next to its callers/callees.
            if let Some(filter) = dump_filter {
                let mname = self[self[method].name()].to_string();
                let dem = format!("{:#}", rustc_demangle::demangle(&mname));
                if mname.contains(filter) || dem.contains(filter) {
                    let dump = crate::ir::dump::dump_method(&tmp_method, self);
                    eprintln!("{dump}");
                }
            }
            if let Err(err) = tmp_method.typecheck(self) {
                violations += 1;
                let mname = self[self[method].name()].to_string();
                // Always dump the offending method in full (deterministic tooling for diagnosing
                // verifier rejections): every node is annotated with its inferred type, so the
                // node that introduces a wrong type / extra indirection is read straight off.
                let dump = crate::ir::dump::dump_method(&tmp_method, self);
                eprintln!("{dump}");
                if fatal {
                    // Fatal type gate: never emit an ill-typed method.
                    panic!(
                        "CIL type-verifier rejected method `{mname}`: {err:?}. \
                         Refusing to emit ill-typed CIL (ALLOW_MISCOMPILATIONS=0). \
                         This is invariant I1 of the absolute-correctness plan."
                    );
                }
                eprintln!("Typecheck violation in method `{mname}`: {err:?}");
            };
        }
        violations
    }
    #[must_use]
    pub fn class_defs(&self) -> &FxHashMap<ClassDefIdx, ClassDef> {
        &self.class_defs
    }

    #[must_use]
    pub fn method_ref_to_def(&self, method: Interned<MethodRef>) -> Option<MethodDefIdx> {
        if self
            .method_defs
            .contains_key(&MethodDefIdx::from_raw(method))
        {
            Some(MethodDefIdx::from_raw(method))
        } else {
            None
        }
    }
    #[must_use]
    pub fn fuel_from_env(&self) -> OptFuel {
        match std::env::var("OPT_FUEL") {
            Ok(fuel) => match fuel.parse::<u32>() {
                Ok(fuel) => OptFuel::new(fuel),
                Err(_) => self.default_fuel(),
            },
            Err(_) => self.default_fuel(),
        }
    }
    #[must_use]
    pub fn default_fuel(&self) -> OptFuel {
        OptFuel::new((self.method_defs.len() * 4 + self.roots.len() * 16) as u32)
    }
    pub(crate) fn borrow_methoddef(&mut self, def_id: MethodDefIdx) -> MethodDef {
        self.method_defs.remove(&def_id).unwrap()
    }
    pub(crate) fn return_methoddef(&mut self, def_id: MethodDefIdx, def: MethodDef) {
        assert!(
            self.method_defs.insert(def_id, def).is_none(),
            "Could not return a methoddef, because a method def is already present."
        );
    }
    /// Optimizes the assembly uitill all fuel is consumed, or no more progress can be made
    pub fn opt(&mut self, fuel: &mut OptFuel) {
        // The CIL optimizer is purely local/intra-method (copy-prop, DCE, peepholes, block
        // linearization). It does NOT inline calls — Rust's zero-cost abstractions are inlined at the
        // MIR level by rustc's own inliner (the backend raises `-Zinline-mir-hint-threshold`), which
        // is correct by construction and runs before codegen. So a fixpoint of local passes suffices;
        // no soundness snapshot/revert is needed.
        let mut cache = SideEffectInfoCache::default();
        while !fuel.exchausted() {
            let prev = fuel.clone();
            self.opt_sigle_pass(fuel, &mut cache);
            // No fuel consumed, progress can't be made, break.
            if *fuel == prev {
                break;
            }
            //let _pass_min_cost: bool = fuel.consume(1);
        }
    }
    /// Optimizes the assembly, cosuming some fuel. This performs a single optimization pass.
    pub fn opt_sigle_pass(&mut self, fuel: &mut OptFuel, cache: &mut SideEffectInfoCache) {
        let method_def_idxs: Box<[_]> = self.method_defs.keys().copied().collect();
        for method in method_def_idxs {
            let mut tmp_method = self.borrow_methoddef(method);
            tmp_method.optimize(self, cache, fuel);
            tmp_method.remove_dead_blocks(self);
            self.return_methoddef(method, tmp_method);
            if fuel.exchausted() {
                break;
            }
        }
    }
    /// Finds all methods matching the closure
    pub fn methods_with<'a>(
        &'a self,
        mut filter: impl FnMut(&Self, MethodDefIdx, &MethodDef) -> bool + 'a,
    ) -> impl Iterator<Item = (&'a MethodDefIdx, &'a MethodDef)> + 'a {
        self.method_defs
            .iter()
            .filter(move |(id, def)| filter(self, **id, def))
    }
    /// Modifies the method deifinition by running the closure on it
    pub fn modify_methodef(
        &mut self,
        modify: impl FnOnce(&mut Self, &mut MethodDef),
        def_id: MethodDefIdx,
    ) {
        let mut borrowed = self.borrow_methoddef(def_id);
        modify(self, &mut borrowed);
        self.return_methoddef(def_id, borrowed);
    }
    pub fn find_methods_matching<'a, P: std::str::pattern::Pattern + Clone + 'a>(
        &self,
        pat: P,
    ) -> Option<impl Iterator<Item = MethodDefIdx> + '_> {
        let names: Box<[Interned<IString>]> = self.find_strs_containing(pat).collect();
        Some(self.method_defs.iter().filter_map(move |(mdefidx, mdef)| {
            if names.iter().any(|name| *name == mdef.name()) {
                Some(*mdefidx)
            } else {
                None
            }
        }))
    }
    pub fn find_strs_containing<'a, P: std::str::pattern::Pattern + Clone + 'a>(
        &'a self,
        pat: P,
    ) -> impl Iterator<Item = Interned<IString>> + 'a {
        self.strings
            .0
            .iter()
            .enumerate()
            .filter_map(move |(idx, str)| {
                if str.contains(pat.clone()) {
                    Some(Interned::from_index(
                        BiMapIndex::new((idx + 1) as u32).unwrap(),
                    ))
                } else {
                    None
                }
            })
    }
    pub fn get_prealllocated_string(
        &self,
        string: impl Into<IString>,
    ) -> Option<Interned<IString>> {
        self.strings.1.get(&(string.into())).copied()
    }
    pub fn class_mut(&mut self, id: ClassDefIdx) -> &mut ClassDef {
        self.class_defs.get_mut(&id).unwrap()
    }
    #[must_use]
    pub fn get_class_def(&self, id: ClassDefIdx) -> &ClassDef {
        &self.class_defs[&id]
    }
    #[must_use]
    pub fn class_ref(&self, cref: Interned<ClassRef>) -> &ClassRef {
        self.class_refs.get(cref)
    }
    #[must_use]
    pub fn method_def(&self, dref: MethodDefIdx) -> &MethodDef {
        self.method_defs.get(&dref).unwrap()
    }
    pub fn alloc_string(&mut self, string: impl Into<IString>) -> Interned<IString> {
        self.strings.alloc(string.into())
    }

    pub fn sig(
        &mut self,
        input: impl Into<Box<[Type]>>,
        output: impl Into<Type>,
    ) -> Interned<FnSig> {
        self.sigs.alloc(FnSig::new(input.into(), output.into()))
    }
    pub fn fn_ptr(&mut self, input: impl Into<Box<[Type]>>, output: impl Into<Type>) -> Type {
        let sig = self.sig(input, output);
        Type::FnPtr(sig)
    }
    pub fn nptr(&mut self, inner: impl IntoAsmIndex<Interned<Type>>) -> Type {
        Type::Ptr(inner.into_idx(self))
    }
    pub fn nref(&mut self, inner: impl IntoAsmIndex<Interned<Type>>) -> Type {
        Type::Ref(inner.into_idx(self))
    }

    #[must_use]
    pub fn get_root(&self, root: Interned<CILRoot>) -> &CILRoot {
        self.roots.get(root)
    }
    pub fn size_of(&mut self, tpe: impl IntoAsmIndex<Interned<Type>>) -> Interned<CILNode> {
        let idx = tpe.into_idx(self);
        assert_ne!(self[idx], Type::Void);
        self.alloc_node(CILNode::SizeOf(idx))
    }
    pub fn biop(
        &mut self,
        lhs: impl IntoAsmIndex<Interned<CILNode>>,
        rhs: impl IntoAsmIndex<Interned<CILNode>>,
        op: BinOp,
    ) -> Interned<CILNode> {
        let lhs = lhs.into_idx(self);
        let rhs = rhs.into_idx(self);
        self.alloc_node(CILNode::BinOp(lhs, rhs, op))
    }
    pub fn unop(&mut self, val: impl Into<CILNode>, op: UnOp) -> CILNode {
        let val = self.nodes.alloc(val.into());
        CILNode::UnOp(val, op)
    }
    pub fn int_cast(
        &mut self,
        input: impl IntoAsmIndex<Interned<CILNode>>,
        target: Int,
        extend: ExtendKind,
    ) -> Interned<CILNode> {
        let input = input.into_idx(self);
        self.alloc_node(CILNode::IntCast {
            input,
            target,
            extend,
        })
    }
    pub fn ptr_cast(
        &mut self,
        input: impl IntoAsmIndex<Interned<CILNode>>,
        res: PtrCastRes,
    ) -> CILNode {
        CILNode::PtrCast(input.into_idx(self), Box::new(res))
    }
    pub fn ldstr(&mut self, msg: impl Into<IString>) -> CILNode {
        CILNode::Const(Box::new(Const::PlatformString(self.alloc_string(msg))))
    }
    pub fn strct(&mut self, name: IString) -> Interned<ClassRef> {
        let class = ClassRef::new(self.alloc_string(name), None, true, vec![].into());
        self.class_refs.alloc(class)
    }

    pub fn alloc_node(&mut self, node: impl Into<CILNode>) -> Interned<CILNode> {
        self.nodes.alloc(node.into())
    }

    pub fn alloc_class_ref(&mut self, cref: ClassRef) -> Interned<ClassRef> {
        self.class_refs.alloc(cref)
    }

    /// The distinct names of all external assemblies referenced by this assembly's class refs (e.g.
    /// `System.Runtime`, `System.Private.CoreLib`). Used to emit `.assembly extern` directives with
    /// real BCL identities, so the produced assembly can be referenced by a C# *compiler* (otherwise
    /// ilasm defaults extern refs to version 0.0.0.0 and Roslyn rejects them — CS0012). Sorted for
    /// deterministic output.
    #[must_use]
    pub fn external_assembly_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for cref in self.class_refs.iter_keys() {
            if let Some(asm) = self.class_refs[cref].asm() {
                let name = &self[asm];
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        names.dedup();
        names
    }

    pub fn alloc_sig(&mut self, sig: FnSig) -> Interned<FnSig> {
        self.sigs.alloc(sig)
    }

    pub fn alloc_methodref(&mut self, method_ref: MethodRef) -> Interned<MethodRef> {
        self.method_refs.alloc(method_ref)
    }
    pub fn new_methodref(
        &mut self,
        class: Interned<ClassRef>,
        name: impl Into<IString>,
        sig: Interned<FnSig>,
        kind: MethodKind,
        generics: impl Into<Box<[Type]>>,
    ) -> Interned<MethodRef> {
        let name = self.alloc_string(name);

        self.alloc_methodref(MethodRef::new(class, name, sig, kind, generics.into()))
    }
    pub fn alloc_root(&mut self, val: CILRoot) -> Interned<CILRoot> {
        self.roots.alloc(val)
    }

    pub fn alloc_type(&mut self, tpe: impl Into<Type>) -> Interned<Type> {
        self.types.alloc(tpe.into())
    }

    pub(crate) fn get_node(&self, key: Interned<CILNode>) -> &CILNode {
        self.nodes.get(key)
    }

    pub fn alloc_field(&mut self, field: FieldDesc) -> Interned<FieldDesc> {
        self.fields.alloc(field)
    }
    #[must_use]
    pub fn get_field(&self, key: Interned<FieldDesc>) -> &FieldDesc {
        self.fields.get(key)
    }
    pub fn alloc_sfld(&mut self, sfld: StaticFieldDesc) -> Interned<StaticFieldDesc> {
        self.statics.alloc(sfld)
    }
    #[must_use]
    pub fn get_static_field(&self, key: Interned<StaticFieldDesc>) -> &StaticFieldDesc {
        self.statics.get(key)
    }
    pub fn add_static(
        &mut self,
        tpe: Type,
        name: impl Into<IString>,
        thread_local: bool,
        in_class: ClassDefIdx,
        default_value: Option<Const>,
        is_const: bool,
    ) -> Interned<StaticFieldDesc> {
        let name = self.alloc_string(name);
        let sfld = StaticFieldDesc::new(*in_class, name, tpe);
        let idx = self.alloc_sfld(sfld);
        if !self
            .class_mut(in_class)
            .static_fields()
            .contains(&StaticFieldDef {
                tpe,
                name,
                is_tls: thread_local,
                default_value,
                is_const,
            })
        {
            self.class_mut(in_class)
                .static_fields_mut()
                .push(StaticFieldDef {
                    tpe,
                    name,
                    is_tls: thread_local,
                    default_value,
                    is_const,
                });
        }

        idx
    }
    pub fn annon_const(
        &mut self,
        node: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<StaticFieldDesc> {
        let main_module = self.main_module();
        let node = node.into_idx(self);

        let sig = self.sig([], Type::Void);
        let tpe = self[node].clone().typecheck(sig, &[], self).unwrap();
        let name = format!(
            "n_{}_{}",
            encode(node.as_bimap_index().get() as u64),
            encode(self.alloc_type(tpe).as_bimap_index().get() as u64)
        );
        let name_idx = self.alloc_string(name.clone());
        let field = StaticFieldDesc::new(*main_module, name_idx, tpe);
        let field = self.alloc_sfld(field);
        if self[main_module].has_static_field(name_idx, tpe) {
            return field;
        }
        self.add_static(tpe, &name[..], false, main_module, None, false);
        let init = self.alloc_root(CILRoot::SetStaticField { field, val: node });
        self.add_cctor(&[init]);

        return field;
    }
    /// Adds a new class definition to this type
    pub fn class_def(&mut self, def: ClassDef) -> Result<ClassDefIdx, LayoutError> {
        def.layout_check(self)?;
        let cref = def.ref_to();
        let cref = self.alloc_class_ref(cref);

        if self.class_defs.contains_key(&ClassDefIdx(cref)) {
            if self[def.name()].contains("core.ffi.c_void")
                || self[def.name()].contains("RustVoid")
                || &self[def.name()] == "f128"
            {
                return Ok(ClassDefIdx(cref));
            }
            panic!(
                "Class name collision: the name {:?} is already used by a different class \
                 definition. If this appeared after enabling de-mangling, two distinct types \
                 mapped to the same stable name — disambiguate one of them.",
                &self[def.name()]
            )
        }
        self.class_defs.insert(ClassDefIdx(cref), def.clone());
        Ok(ClassDefIdx(cref))
    }
    pub fn main_module(&mut self) -> ClassDefIdx {
        let main_module = self.alloc_string(MAIN_MODULE);

        let class_def = ClassDef::new(
            main_module,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        );
        let cref = class_def.ref_to();
        let cref = self.class_refs.alloc(cref);
        // Check if that definition already exists
        if self.class_defs.contains_key(&ClassDefIdx(cref)) {
            ClassDefIdx(cref)
        } else {
            self.class_def(class_def).unwrap()
        }
    }
    /// Adds a method definition to this assembly.
    pub fn new_method(&mut self, def: MethodDef) -> MethodDefIdx {
        let mref = def.ref_to();
        let def_class = def.class();
        let ref_idx = self.alloc_methodref(mref);
        // Check that this def is unique
        if !self
            .method_defs
            .contains_key(&MethodDefIdx::from_raw(ref_idx))
        {
            self.class_defs
                .get_mut(&def_class)
                .expect("Method added without a class")
                .add_def(MethodDefIdx::from_raw(ref_idx));
        }

        self.method_defs
            .insert(MethodDefIdx::from_raw(ref_idx), def);

        MethodDefIdx::from_raw(ref_idx)
    }
    pub fn user_init(&mut self) -> MethodDefIdx {
        let main_module = self.main_module();
        let user_init = self.alloc_string(USER_INIT);
        let ctor_sig = self.sig([], Type::Void);
        let mref = MethodRef::new(
            *main_module,
            user_init,
            ctor_sig,
            MethodKind::Static,
            vec![].into(),
        );
        let mref = self.alloc_methodref(mref);
        if self.method_defs.contains_key(&MethodDefIdx::from_raw(mref)) {
            MethodDefIdx::from_raw(mref)
        } else {
            let mimpl = MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(
                    vec![self.alloc_root(CILRoot::VoidRet)],
                    0,
                    None,
                )],
                locals: vec![],
            };
            let cctor_def = MethodDef::new(
                Access::Extern,
                main_module,
                user_init,
                ctor_sig,
                MethodKind::Static,
                mimpl,
                vec![],
            );
            self.new_method(cctor_def)
        }
    }
    /// Returns a reference to tht thread local constructor.
    pub fn tcctor(&mut self) -> MethodDefIdx {
        let main_module = self.main_module();
        let user_init = self.alloc_string(TCCTOR);
        let ctor_sig = self.sig([], Type::Void);
        let mref = MethodRef::new(
            *main_module,
            user_init,
            ctor_sig,
            MethodKind::Static,
            vec![].into(),
        );
        let mref = self.alloc_methodref(mref);
        if self.method_defs.contains_key(&MethodDefIdx::from_raw(mref)) {
            MethodDefIdx::from_raw(mref)
        } else {
            let mimpl = MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(
                    vec![self.alloc_root(CILRoot::VoidRet)],
                    0,
                    None,
                )],
                locals: vec![],
            };
            let cctor_def = MethodDef::new(
                Access::Extern,
                main_module,
                user_init,
                ctor_sig,
                MethodKind::Static,
                mimpl,
                vec![],
            );
            self.new_method(cctor_def)
        }
    }
    fn cctor_mref(&mut self) -> Interned<MethodRef> {
        let main_module = self.main_module();
        let user_init = self.alloc_string(CCTOR);
        let ctor_sig = self.sig([], Type::Void);
        self.alloc_methodref(MethodRef::new(
            *main_module,
            user_init,
            ctor_sig,
            MethodKind::Static,
            vec![].into(),
        ))
    }
    fn has_builtin(&self, name: &str, input: impl Into<Box<[Type]>>, output: Type) -> bool {
        let Some(main_module) = self.get_prealllocated_string(MAIN_MODULE) else {
            return false;
        };
        let class_def = ClassDef::new(
            main_module,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            None,
            None,
            true,
        );

        let Some(cref) = self.get_prealllocated_class_ref(class_def.ref_to()) else {
            return false;
        };
        // Check if that definition already exists
        let main_module = if self.class_defs.contains_key(&ClassDefIdx(cref)) {
            ClassDefIdx(cref)
        } else {
            return false;
        };

        let Some(user_init) = self.get_prealllocated_string(name) else {
            return false;
        };

        let Some(ctor_sig) = self.get_prealllocated_sig(FnSig::new(input.into(), output)) else {
            return false;
        };
        let Some(cctor) = self.get_prealllocated_methodref(MethodRef::new(
            *main_module,
            user_init,
            ctor_sig,
            MethodKind::Static,
            vec![].into(),
        )) else {
            return false;
        };

        self.method_ref_to_def(cctor).is_some()
    }
    pub fn has_cctor(&self) -> bool {
        self.has_builtin(CCTOR, [], Type::Void)
    }
    pub fn has_tcctor(&self) -> bool {
        self.has_builtin(TCCTOR, [], Type::Void)
    }
    pub fn get_prealllocated_class_ref(&self, cref: ClassRef) -> Option<Interned<ClassRef>> {
        self.class_refs.1.get(&cref).copied()
    }
    pub fn get_prealllocated_sig(&self, sig: FnSig) -> Option<Interned<FnSig>> {
        self.sigs.1.get(&sig).copied()
    }
    pub fn get_prealllocated_methodref(&self, mref: MethodRef) -> Option<Interned<MethodRef>> {
        self.method_refs.1.get(&mref).copied()
    }
    /// Returns a reference to the static initializer
    pub fn cctor(&mut self) -> MethodDefIdx {
        let mref = self.cctor_mref();
        if self.method_defs.contains_key(&MethodDefIdx::from_raw(mref)) {
            MethodDefIdx::from_raw(mref)
        } else {
            let mimpl = MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(
                    vec![self.alloc_root(CILRoot::VoidRet)],
                    0,
                    None,
                )],
                locals: vec![],
            };
            let main_module = self.main_module();
            let user_init = self.alloc_string(CCTOR);
            let ctor_sig = self.sig([], Type::Void);
            let cctor_def = MethodDef::new(
                Access::Extern,
                main_module,
                user_init,
                ctor_sig,
                MethodKind::Static,
                mimpl,
                vec![],
            );
            self.new_method(cctor_def)
        }
    }
    /// Adds new rooots to the user init list.
    pub fn add_user_init(&mut self, roots: &[Interned<CILRoot>]) {
        let user_init = self.user_init();
        let user_init = self.method_defs.get_mut(&user_init).unwrap();
        let blocks = user_init
            .implementation_mut()
            .blocks_mut()
            .expect("EROROR: {USER_INIT} has no body.");
        let last = blocks
            .iter_mut()
            .last()
            .expect("ERROR: {USER_INIT} has a body without blocks.");
        let last_root_idx = if last.roots().is_empty() {
            0
        } else {
            last.roots().len() - 1
        };
        for (idx, root) in roots.iter().enumerate() {
            last.roots_mut().insert(idx + last_root_idx, *root);
        }
    }
    /// Adds new rooots to the thread local intiailzer .
    pub fn add_tcctor(&mut self, roots: &[Interned<CILRoot>]) {
        let user_init = self.tcctor();
        let user_init = self.method_defs.get_mut(&user_init).unwrap();
        let blocks = user_init
            .implementation_mut()
            .blocks_mut()
            .expect("EROROR: {TCCTOR} has no body.");
        let last = blocks
            .iter_mut()
            .last()
            .expect("ERROR: {TCCTOR} has a body without blocks.");
        let last_root_idx = if last.roots().is_empty() {
            0
        } else {
            last.roots().len() - 1
        };
        for (idx, root) in roots.iter().enumerate() {
            last.roots_mut().insert(idx + last_root_idx, *root);
        }
    }
    /// Adds new rooots to the static initializer
    pub fn add_cctor(&mut self, roots: &[Interned<CILRoot>]) {
        let user_init = self.cctor();
        let user_init = self.method_defs.get_mut(&user_init).unwrap();
        let blocks = user_init
            .implementation_mut()
            .blocks_mut()
            .expect("EROROR: {CCTOR} has no body.");
        let last = blocks
            .iter_mut()
            .last()
            .expect("ERROR: {CCTOR} has a body without blocks.");
        let last_root_idx = if last.roots().is_empty() {
            0
        } else {
            last.roots().len() - 1
        };

        for (idx, root) in roots.iter().enumerate() {
            last.roots_mut().insert(idx + last_root_idx, *root);
        }
    }
    /// Serializes and saves this assembly
    pub fn save_tmp<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        w.write_all(&postcard::to_stdvec(&self).unwrap())
    }
    pub(crate) fn rust_void(&mut self) -> ClassDefIdx {
        let rust_void = self.alloc_string("RustVoid");
        self.class_def(ClassDef::new(
            rust_void,
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
        .unwrap()
    }
    /// Finalizes a freshly-built assembly: ensures the `RustVoid` class exists and runs a
    /// debug-only sanity check.
    #[must_use]
    pub fn prepared(mut self) -> Self {
        self.rust_void();

        #[cfg(debug_assertions)]
        self.sanity_check();
        self
    }
    #[track_caller]
    pub fn sanity_check(&self) {
        self.class_defs.values().for_each(|class| {
            assert_unique(class.methods(), class.ref_to().display(self));
        });
    }
    #[cfg(not(miri))]
    pub fn export(&self, out: impl AsRef<std::path::Path>, mut exporter: impl Exporter) {
        if *LINKER_RECOVER {
            eprintln!("{:?}", exporter.export(self, out.as_ref()));
        } else {
            exporter.export(self, out.as_ref()).unwrap();
        }
    }
    pub fn memory_info(&self) {
        let mut stats = vec![
            encoded_stats(self),
            encoded_stats(&self.strings),
            encoded_stats(&self.types),
            encoded_stats(&self.class_refs),
            encoded_stats(&self.class_defs),
            encoded_stats(&self.nodes),
            encoded_stats(&self.roots),
            encoded_stats(&self.sigs),
            encoded_stats(&self.types),
            encoded_stats(&self.fields),
            encoded_stats(&self.statics),
            encoded_stats(&self.method_defs),
        ];
        stats.sort_by(|(_, a), (_, b)| a.cmp(b));
        for stat in stats {
            println!("{}:\t{} bytes", stat.0, stat.1);
        }
    }

    pub(crate) fn iter_class_defs(&self) -> impl Iterator<Item = &ClassDef> {
        self.class_defs.values()
    }
    pub(crate) fn iter_class_def_ids(&self) -> impl Iterator<Item = &ClassDefIdx> {
        self.class_defs.keys()
    }
    pub(crate) fn method_def_from_ref(&self, mref: Interned<MethodRef>) -> Option<&MethodDef> {
        self.method_defs.get(&MethodDefIdx::from_raw(mref))
    }
    pub(crate) fn eliminate_dead_fns(&mut self, only_imports: bool) {
        // 1st. Collect all "extern" method definitons, since those are always alive.
        let mut previosly_ressurected: FxHashSet<MethodDefIdx> = self
            .method_defs
            .iter()
            .filter(|(_, def)| def.access().is_extern())
            .map(|(idx, _)| *idx)
            .collect();
        let mut to_resurrect: FxHashSet<MethodDefIdx> = FxHashSet::default();
        let mut alive: FxHashSet<MethodDefIdx> = FxHashSet::default();
        // If only cleaning up imports, assume all non-import fns are alive.
        if only_imports {
            alive.extend(
                self.method_defs
                    .iter()
                    .filter(|(_, def)| !matches!(def.implementation(), MethodImpl::Extern { .. }))
                    .map(|(id, _)| *id),
            );
        }
        while !previosly_ressurected.is_empty() {
            for def in previosly_ressurected
                .iter()
                .map(|def: &MethodDefIdx| self.method_defs.get(def).unwrap())
            {
                // An aliasing method (e.g. a comptime-defined virtual method) has no CIL body of its
                // own, so the `iter_cil` walk below would miss its target — keep the alias target alive
                // explicitly.
                if let MethodImpl::AliasFor(target) = def.implementation() {
                    let tdef = MethodDefIdx::from_raw(*target);
                    if self.method_defs.contains_key(&tdef) && !alive.contains(&tdef) {
                        to_resurrect.insert(tdef);
                    }
                }
                // Iterate torugh the cil of this method, if present
                let Some(cil) = def.iter_cil(self) else {
                    continue;
                };
                // Get all the ref ids of the methods used in the cil.
                let refids = cil.filter_map(|elem| match elem {
                    crate::CILIterElem::Node(CILNode::Call(args)) => Some(args.0),
                    crate::CILIterElem::Node(CILNode::LdFtn(mref)) => Some(mref),
                    crate::CILIterElem::Node(_) => None,
                    crate::CILIterElem::Root(CILRoot::Call(args)) => Some(args.0),
                    crate::CILIterElem::Root(_) => None,
                });
                // Check if this method reference is also a def. If so, map it to a def
                let defids = refids.filter_map(|refid| {
                    self.method_defs
                        .get(&MethodDefIdx::from_raw(refid))
                        .map(|_| MethodDefIdx::from_raw(refid))
                        .and_then(|refid| {
                            if alive.contains(&refid) {
                                None
                            } else {
                                Some(refid)
                            }
                        })
                });
                to_resurrect.extend(defids);
            }
            alive.extend(previosly_ressurected);
            previosly_ressurected = to_resurrect;
            to_resurrect = FxHashSet::default();
        }

        // Some cheap sanity checks
        assert!(previosly_ressurected.is_empty());
        assert!(to_resurrect.is_empty());
        // Set the method set to only include alive methods
        self.method_defs = alive
            .iter()
            .map(|id| (*id, self.method_defs.remove(id).unwrap()))
            .collect();
        // clean up typedefs
        self.class_defs.values_mut().for_each(|tdef| {
            tdef.methods_mut()
                .retain(|def| self.method_defs.contains_key(def));
        });
    }
    pub fn eliminate_dead_code(&mut self) {
        self.eliminate_dead_fns(false);
        self.eliminate_dead_types();
    }
    pub(crate) fn eliminate_dead_types(&mut self) {
        let mut previosly_ressurected: FxHashSet<ClassDefIdx> = self
            .method_defs()
            .values()
            .flat_map(|method| method.iter_types(self))
            .flat_map(|tpe| tpe.iter_class_refs(self).collect::<Vec<_>>())
            .filter_map(|cref| self.class_ref_to_def(cref))
            .collect();
        previosly_ressurected.extend(self.class_defs().iter().filter_map(|(defid, def)| {
            if def.access().is_extern() {
                Some(defid)
            } else {
                None
            }
        }));
        let rust_void = self.alloc_string("RustVoid");
        let rust_void = self.alloc_class_ref(ClassRef::new(rust_void, None, true, vec![].into()));
        if let Some(cref) = self.class_ref_to_def(rust_void) {
            previosly_ressurected.insert(cref);
        }
        let f128 = self.alloc_string("f128");
        let f128 = self.alloc_class_ref(ClassRef::new(f128, None, true, vec![].into()));
        if let Some(cref) = self.class_ref_to_def(f128) {
            previosly_ressurected.insert(cref);
        }

        let mut to_resurrect: FxHashSet<ClassDefIdx> = FxHashSet::default();
        let mut alive: FxHashSet<ClassDefIdx> = FxHashSet::default();
        while !previosly_ressurected.is_empty() {
            for def in &previosly_ressurected {
                let defids: FxHashSet<ClassDefIdx> = self.class_defs[def]
                    .iter_types()
                    .flat_map(|tpe| tpe.iter_class_refs(self).collect::<Vec<_>>())
                    .filter_map(|cref| self.class_ref_to_def(cref))
                    .filter(|refid| !alive.contains(refid))
                    .collect();

                to_resurrect.extend(defids);
            }
            alive.extend(previosly_ressurected);
            previosly_ressurected = to_resurrect;
            to_resurrect = FxHashSet::default();
        }
        // Some cheap sanity checks
        assert!(previosly_ressurected.is_empty());
        assert!(to_resurrect.is_empty());
        // Set the class_defs to only include alive classes
        self.class_defs = alive
            .iter()
            .map(|id| (*id, self.class_defs.remove(id).unwrap()))
            .collect();
    }
    /*pub fn realloc_nodes(&mut self){

    }*/
    /// Reallocates the roots, freeing all dead ones.
    pub fn realloc_roots(&mut self) {
        let mut new_roots = BiMap::default();
        for block in self
            .method_defs
            .values_mut()
            .filter_map(|def| def.implementation_mut().blocks_mut())
            .flatten()
        {
            let (handler, roots) = block.handler_and_root_mut();
            for root in roots.iter_mut().chain(
                handler
                    .into_iter()
                    .flat_map(|blocks| blocks.iter_mut())
                    .flat_map(super::basic_block::BasicBlock::roots_mut),
            ) {
                let mut val = self.roots.get(*root).clone();
                // A `TerminateRegion`'s `protected` child is NOT in any block's root list, so it
                // would be dropped by the rebuild. Re-intern it into `new_roots` first and rewrite
                // the region to point at the new index (`realloc_roots` rebuilds only the ROOT
                // bimap, so the protected root's own node indices remain valid).
                if let CILRoot::TerminateRegion { protected, .. } = &mut val {
                    let inner = self.roots.get(*protected).clone();
                    *protected = new_roots.alloc(inner);
                }
                *root = new_roots.alloc(val);
            }
        }
        self.roots = new_roots;
    }

    pub fn patch_missing_methods(
        &mut self,
        externs: &FxHashMap<&str, String>,
        modifies_errno: &FxHashSet<&str>,
        override_methods: &MissingMethodPatcher,
    ) {
        let mref_count = self.method_refs.0.len();
        let externs: FxHashMap<_, _> = externs
            .iter()
            .map(|(fn_name, lib_name)| {
                (
                    self.alloc_string(*fn_name),
                    self.alloc_string(lib_name.clone()),
                )
            })
            .collect();
        let preserve_errno: FxHashSet<_> = modifies_errno
            .iter()
            .map(|fn_name| self.alloc_string(*fn_name))
            .collect();
        for index in 0..mref_count {
            // Get the full method refernce
            let mref = self.method_refs.0[index].clone();
            // Check if this method reference's class has an assembly. If it has, then the method is extern. If it has not, then it is defined in this assembly
            // and must have some kind of implementation
            let class = self.class_ref(mref.class());

            if class.asm().is_some() {
                // Is extern, skip

                continue;
            }
            let mref_idx =
                Interned::from_index(std::num::NonZeroU32::new(index as u32 + 1).unwrap());
            // Check if this method already has an implementation.
            if self
                .method_defs
                .contains_key(&MethodDefIdx::from_raw(mref_idx))
            {
                // A method defintion already present, so we don't need to do anyting, so skip.

                continue;
            }
            let name = rustc_demangle::demangle(&self[mref.name()]).to_string();
            let name = self.alloc_string(name.split("::").last().unwrap());
            if let Some(overrider) = override_methods.get(&name) {
                let mref = mref.clone();
                let implementation = overrider(mref_idx, self);
                self.new_method(mref.into_def(implementation, Access::Private, self));
                continue;
            }

            // Check if this method is in the extern list
            if let Some(lib) = externs.get(&mref.name()) {
                let arg_names = (0..(self[mref.sig()].inputs().len()))
                    .map(|_| None)
                    .collect();
                let method_def = MethodDef::new(
                    Access::Public,
                    ClassDefIdx(mref.class()),
                    mref.name(),
                    mref.sig(),
                    mref.kind(),
                    MethodImpl::Extern {
                        lib: *lib,
                        preserve_errno: preserve_errno.contains(&mref.name()),
                    },
                    arg_names,
                );
                assert!(
                    self.class_defs.contains_key(&ClassDefIdx(mref.class())),
                    "Can't yet handle missing types."
                );

                self.new_method(method_def);

                continue;
            }
            // Create a replacement method.

            let arg_names = (0..(self[mref.sig()].inputs().len()))
                .map(|_| None)
                .collect();
            let name = &self[mref.name()];
            let is_alloc =
                name.contains("__rust_alloc") && !name.contains("__rust_alloc_error_handler");
            // `__rust_no_alloc_shim_is_unstable[_v2]` is a marker the alloc shim references purely to
            // keep the global allocator symbols linked; it has no effect. Recent rustc emits it as a
            // (mangled) *function* call rather than the old `u8` static, so provide a no-op body
            // (a bare return) — otherwise it resolves to `MethodImpl::Missing` and throws at runtime
            // (`box_new_uninit` -> alloc -> missing method) the moment anything allocates.
            let is_noalloc_shim = name.contains("__rust_no_alloc_shim_is_unstable");
            let imp = if is_alloc {
                let alloc = MethodRef::aligned_alloc(self);
                MethodImpl::wrapper(self.alloc_methodref(alloc), &mref, self)
            } else if is_noalloc_shim {
                MethodImpl::MethodBody {
                    blocks: vec![super::BasicBlock::new(
                        vec![self.alloc_root(CILRoot::VoidRet)],
                        0,
                        None,
                    )],
                    locals: vec![],
                }
            } else {
                MethodImpl::Missing
            };
            let method_def = MethodDef::new(
                Access::Public,
                ClassDefIdx(mref.class()),
                mref.name(),
                mref.sig(),
                mref.kind(),
                imp,
                arg_names,
            );
            assert!(
                self.class_defs.contains_key(&ClassDefIdx(mref.class())),
                "Can't yet handle missing types. Type {} with id {:?} is missing. Method {}",
                self.class_ref(mref.class()).display(self),
                mref.class(),
                &self[mref.name()]
            );

            self.new_method(method_def);
        }
    }

    #[must_use]
    pub fn class_ref_to_def(&self, class: Interned<ClassRef>) -> Option<ClassDefIdx> {
        if self.class_defs.contains_key(&ClassDefIdx(class)) {
            Some(ClassDefIdx(class))
        } else {
            None
        }
    }

    #[must_use]
    pub fn link(mut self, other: Self) -> Self {
        let original_str = self.alloc_string(MAIN_MODULE);
        for def in other.iter_class_defs() {
            let translated = self.translate_class_def(&other, def);
            let class_ref = self.alloc_class_ref(translated.ref_to());
            match self.class_defs.entry(ClassDefIdx(class_ref)) {
                std::collections::hash_map::Entry::Occupied(mut occupied) => {
                    occupied.get_mut().merge_defs(translated);
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(translated);
                }
            }
        }
        assert_eq!(self.alloc_string(MAIN_MODULE), original_str);
        self.sections.extend(other.sections);
        self
    }

    pub fn method_defs(&self) -> &FxHashMap<MethodDefIdx, MethodDef> {
        &self.method_defs
    }

    /// Checks if this assembly contains a reference [`ClassRef`]
    #[must_use]
    pub fn contains_ref(&self, cref: &ClassRef) -> bool {
        self.class_refs.1.contains_key(cref)
    }

    pub(crate) fn class_defs_mut_strings(
        &mut self,
    ) -> (&mut FxHashMap<ClassDefIdx, ClassDef>, &BiMap<IString>) {
        (&mut self.class_defs, &self.strings)
    }
    /// Iteates trough *all the nodes* in this assembly
    pub fn iter_nodes(&self) -> impl Iterator<Item = &CILNode> {
        self.nodes.0.iter()
    }
    /// Iterates trough *all the roots* in this assembly
    pub fn iter_roots(&self) -> impl Iterator<Item = &CILRoot> {
        self.roots.0.iter()
    }
    pub fn remove_dead_statics(&mut self) {
        /*// Check which statics are referenced by real code.
                let alive_statics: FxHashSet<Interned<StaticFieldDesc>> = self
                    .iter_nodes()
                    .filter_map(|node| match node {
                        CILNode::LdStaticField(fld) | CILNode::LdStaticFieldAdress(fld) => Some(*fld),
                        _ => None,
                    })
                    .collect();
                let defs: Vec<_> = self.iter_class_def_ids().copied().collect();
                for class_id in defs {
                    let class = self.get_class_def(class_id).clone();
                    // Collect all statics which, to which there exists a corresponding static field desc.
                    let statics: Vec<_> = class
                        .static_fields()
                        .iter()
                        .copied()
                        .filter(|(tpe, name, _)| {
                            alive_statics
                                .contains(&self.alloc_sfld(StaticFieldDesc::new(*class_id, *name, *tpe)))
                        })
                        .collect();
                    let class = self.class_mut(class_id);
                    *class.static_fields_mut() = statics;
                }
                // After removing all statics whose address nor value is not taken, replace any writes to those statics with pops.
        */
    }
    /// Preforms a "shallow" GC pass on all method defs, removing them if and only if:
    /// 1. They are not referenced by anything inside this assembly
    /// 2. They are not accessible from outside of it.
    ///
    /// **WARNING**: This gc is highly conservative, and will often not collect some things.
    /// To improve its accuracy, first do `link_gc`.
    pub fn shallow_methodef_gc(&mut self) {
        let live: FxHashSet<Interned<MethodRef>> = self
            .iter_nodes()
            .filter_map(|node| match node {
                CILNode::Call(boxed) => Some(boxed.0),
                CILNode::LdFtn(method_ref_idx) => Some(*method_ref_idx),
                CILNode::Const(_)
                | CILNode::BinOp(_, _, _)
                | CILNode::UnOp(_, _)
                | CILNode::LdLoc(_)
                | CILNode::LdLocA(_)
                | CILNode::LdArg(_)
                | CILNode::LdArgA(_)
                | CILNode::IntCast { .. }
                | CILNode::FloatCast { .. }
                | CILNode::RefToPtr(_)
                | CILNode::PtrCast(_, _)
                | CILNode::LdFieldAddress { .. }
                | CILNode::LdField { .. }
                | CILNode::LdInd { .. }
                | CILNode::SizeOf(_)
                | CILNode::GetException
                | CILNode::IsInst(_, _)
                | CILNode::CheckedCast(_, _)
                | CILNode::CallI(_)
                | CILNode::LocAlloc { .. }
                | CILNode::LdStaticField(_)
                | CILNode::LdStaticFieldAddress(_)
                | CILNode::LdTypeToken(_)
                | CILNode::LdLen(_)
                | CILNode::LocAllocAlgined { .. }
                | CILNode::LdElelemRef { .. }
                | CILNode::NewArr { .. }
                | CILNode::UnboxAny { .. } => None,
            })
            .chain(self.iter_roots().filter_map(|root| match root {
                CILRoot::Call(boxed) => Some(boxed.0),
                CILRoot::StLoc(_, _)
                | CILRoot::InitObj(_, _)
                | CILRoot::StArg(_, _)
                | CILRoot::Ret(_)
                | CILRoot::Pop(_)
                | CILRoot::Throw(_)
                | CILRoot::VoidRet
                | CILRoot::Break
                | CILRoot::Nop
                | CILRoot::Branch(_)
                | CILRoot::SourceFileInfo { .. }
                | CILRoot::SetField(_)
                | CILRoot::StInd(_)
                | CILRoot::InitBlk(_)
                | CILRoot::CpBlk(_)
                | CILRoot::CallI(_)
                | CILRoot::ExitSpecialRegion { .. }
                | CILRoot::ReThrow
                // `protected` is interned in the same root bimap, so this `iter_roots` over
                // `self.roots.0` already visits it directly — the region itself holds no MethodRef.
                | CILRoot::TerminateRegion { .. }
                | CILRoot::SetStaticField { .. }
                | CILRoot::CpObj { .. }
                | CILRoot::StElem { .. }
                | CILRoot::Unreachable(_) => None,
            }))
            .collect();

        let mut live: FxHashSet<MethodDefIdx> = live
            .into_iter()
            .filter_map(|mref| self.method_ref_to_def(mref))
            .collect();
        if live.len() == self.method_defs.len() {
            println!("shallow_methodref_gc failed(no unreferenced methods)");
            return;
        }
        self.method_defs.retain(|id, def| {
            if live.contains(id) {
                true
            } else if !matches!(def.implementation(), MethodImpl::Extern { .. }) {
                live.insert(*id);
                true
            } else {
                false
            }
        });
        if live.len() == self.method_defs.len() {
            println!("shallow_methodref_gc failed(no unreferenced, externaly invisible methods)");
        }
        self.class_defs.values_mut().for_each(|tdef| {
            tdef.methods_mut()
                .retain(|methodef| live.contains(methodef));
        });
    }
    pub fn split_to_parts(&self, parts: u32) -> impl Iterator<Item = Self> + use<'_> {
        let lib_name = Interned::from_index(std::num::NonZeroU32::new(1).unwrap());
        // Since 1st part is dedicated to methods which access statics, split the rest into n-1 parts.
        let div = (self.method_refs.len().div_ceil(parts as usize - 1)) as u32;
        // Into 1st. Only split out the methods where it is known, for sure, that they don't access any statics.
        (0..parts).map(move |rem| {
            let mut part = self.clone();
            part.method_defs.iter_mut().for_each(|(idx, def)| {
                if def.accesses_statics(self) {
                    if 0 != rem {
                        *def.implementation_mut() = MethodImpl::Extern {
                            lib: lib_name,
                            preserve_errno: false,
                        }
                    }
                } else if idx.as_bimap_index().get() / div + 1 != rem {
                    *def.implementation_mut() = MethodImpl::Extern {
                        lib: lib_name,
                        preserve_errno: false,
                    }
                }
            });
            if 0 != rem {
                part.class_defs
                    .iter_mut()
                    .for_each(|(_, def)| *def.static_fields_mut() = vec![]);
            }
            part.eliminate_dead_types();
            //part.eliminate_dead_fns(true);
            part = part.link_gc();
            part.shallow_methodef_gc();
            part
        })
    }
    pub fn only_statics(&self) -> Self {
        let lib_name = Interned::from_index(std::num::NonZeroU32::new(1).unwrap());
        let mut empty = self.clone();
        empty.method_defs.iter_mut().for_each(|(_, def)| {
            *def.implementation_mut() = MethodImpl::Extern {
                lib: lib_name,
                preserve_errno: false,
            }
        });
        empty.eliminate_dead_types();
        empty = empty.link_gc();
        empty
    }
    pub fn fix_aligement(&mut self) {
        let method_def_idxs: Box<[_]> = self.method_defs.keys().copied().collect();
        for method in method_def_idxs {
            let mut tmp_method = self.borrow_methoddef(method);
            tmp_method.adjust_aligement(self);
            self.return_methoddef(method, tmp_method);
        }
    }
    pub fn alignof_type(&self, tpe: Interned<Type>) -> u64 {
        match self[tpe] {
            Type::FnPtr(_) | Type::Ptr(_) | Type::Ref(_) => 8, // ASSUMES alignof<*T>() = 8.
            Type::Int(int) => int.size().unwrap_or(8) as u64,  // ASSUMES alignof<usize>() = 8.
            Type::ClassRef(class_ref_idx) => match self.class_ref_to_def(class_ref_idx) {
                Some(def) => self[def]
                    .align()
                    .unwrap_or(std::num::NonZeroU32::new(8).unwrap())
                    .get() as u64,
                None => 8,
            },
            Type::Float(float) => float.size() as u64,
            Type::PlatformString | Type::PlatformObject | Type::PlatformArray { .. } => 8, // ASSUMES alignof<&managed T>() = 8.
            Type::PlatformChar => 2,
            Type::PlatformGeneric(_, _) => 8,
            Type::Bool => 1,
            Type::Void => 0,
            Type::SIMDVector(simdvector) => match simdvector.elem() {
                super::tpe::simd::SIMDElem::Int(int) => int.size().unwrap_or(8) as u64, // ASSUMES alignof<usize>() = 8.
                super::tpe::simd::SIMDElem::Float(float) => float.size() as u64,
            },
        }
    }

    pub fn method_refs(&self) -> &BiMap<MethodRef> {
        &self.method_refs
    }

    pub fn strings(&self) -> &StringMap {
        &self.strings
    }

    pub fn shorten_strings(&mut self, size_cap: usize) {
        self.strings.map_values(|string| {
            if string.len() > size_cap {
                eprint!("shortening {string}");
                *string = encode(hash64(string)).into();
                eprintln!("to {string}");
            }
        })
    }

    fn link_gc(self) -> Self {
        let mut clone = self.clone();
        clone = clone.link(self);
        clone
    }
    pub(crate) fn ptr_size(&self) -> u32 {
        8
    }
    pub(crate) fn sizeof_type(&self, field_tpe: Type) -> u32 {
        match field_tpe {
            Type::Ref(_) | Type::Ptr(_) => self.ptr_size(),
            Type::Int(int) => int
                .size()
                .unwrap_or(self.ptr_size().try_into().unwrap())
                .into(),
            Type::ClassRef(class_ref_idx) => self
                .class_ref_to_def(class_ref_idx)
                .and_then(|def| self[def].explict_size())
                .map_or_else(
                    // An EXTERNAL managed type (a BCL valuetype like `KeyValuePair<K,V>`) has only a
                    // `ClassRef`, no local `ClassDef`, so the backend doesn't know its size — only the
                    // CLR does. Fall back to a conservative pointer size instead of panicking (mirrors
                    // the `PlatformObject`/`!N` arms). The one caller that needs an EXACT size — the
                    // `scalarize` layout pass — separately bails on a field whose def can't be
                    // resolved, so this fallback only ever feeds size-presence checks.
                    || self.ptr_size(),
                    |sz| sz.get(),
                ),
            Type::Float(float) => float.size().into(),
            Type::PlatformString => self.ptr_size(),
            Type::PlatformChar => 1,
            // A generic parameter `!N` / `!!N` (WF-9): only ever appears in a methodref's
            // definition-shape signature, where it is bound to a concrete type at the call site —
            // it is never materialized as a sized local. Any incidental sizing pass gets a
            // conservative pointer-sized answer (a generic slot is reference-or-pointer-sized).
            Type::PlatformGeneric(_, _) => self.ptr_size(),
            Type::PlatformObject => self.ptr_size(),
            Type::Bool => 1,
            Type::Void => 0,
            Type::PlatformArray { .. } => todo!(),
            Type::FnPtr(_) => self.ptr_size(),
            Type::SIMDVector(simdvector) => (simdvector.bits() / 8).into(),
        }
    }

    pub fn add_section(&mut self, arg: &str, packed_metadata: impl Into<Vec<u8>>) {
        self.sections.insert(arg.into(), packed_metadata.into());
    }

    pub(crate) fn get_section(&self, arg: &str) -> Option<&Vec<u8>> {
        self.sections.get(arg)
    }

    pub(crate) fn guaranted_align(&self) -> u8 {
        *GUARANTEED_ALIGN
    }

    pub fn max_static_size(&self) -> usize {
        *MAX_STATIC_SIZE
    }

    pub(crate) fn global_void(&mut self) -> Interned<StaticFieldDesc> {
        let main = self.main_module();
        self.add_static(Type::Void, "global_void", false, main, None, true)
    }

    pub(crate) fn alloc_const_data(&mut self, data: &[u8]) -> Interned<Box<[u8]>> {
        self.const_data.alloc(data.into())
    }

    pub(crate) fn char_is_u8(&self) -> bool {
        true
    }

    pub fn load_static(
        &mut self,
        stotic: impl IntoAsmIndex<Interned<StaticFieldDesc>>,
    ) -> Interned<CILNode> {
        let stotic = stotic.into_idx(self);
        self.alloc_node(CILNode::LdStaticField(stotic))
    }
    pub fn static_addr(
        &mut self,
        stotic: impl IntoAsmIndex<Interned<StaticFieldDesc>>,
    ) -> Interned<CILNode> {
        let stotic = stotic.into_idx(self);
        self.alloc_node(CILNode::LdStaticFieldAddress(stotic))
    }
    /// Transmutes a value from one type to another.
    pub fn transmute_on_stack(
        &mut self,
        src: impl IntoAsmIndex<Interned<Type>>,
        dst: impl IntoAsmIndex<Interned<Type>>,
        val: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILNode> {
        let src = src.into_idx(self);
        let dst = dst.into_idx(self);
        let val = val.into_idx(self);
        if src == dst {
            return val;
        }
        // Inline the pervasive transparent-newtype reinterpret (e.g. `NonNull<T>` -> `*T`, `Box`-style
        // wrappers) as a plain field load. A single-field struct whose sole field sits at offset 0 and
        // is exactly `dst` has identical bits to that field, so `ldfld` is an exact, allocation-free,
        // RyuJIT-inlinable replacement for the `transmute` HELPER CALL — which RyuJIT refuses to inline
        // because it returns a struct, leaving a per-call cost in every hot pointer-threading loop
        // (iterator adapters, Box deref). Guards (single field / offset 0 / type-equal / size-equal)
        // keep it a true bit-reinterpret; anything else falls through to the general helper below.
        let dst_ty = self[dst];
        if let Type::ClassRef(cref) = self[src] {
            if let Some(cdef) = self.class_ref_to_def(cref) {
                let field = {
                    let flds = self[cdef].fields();
                    (flds.len() == 1
                        && flds[0].0 == dst_ty
                        && matches!(flds[0].2, None | Some(0)))
                    .then_some((flds[0].0, flds[0].1))
                };
                if let Some((ftpe, fname)) = field {
                    if self.sizeof_type(self[src]) == self.sizeof_type(dst_ty) {
                        let field = self.alloc_field(FieldDesc::new(cref, fname, ftpe));
                        return self.alloc_node(CILNode::LdField { addr: val, field });
                    }
                }
            }
        }
        let main_module = *self.main_module();

        let sig = self.sig([self[src]], self[dst]);
        let mref = self.new_methodref(main_module, "transmute", sig, MethodKind::Static, vec![]);
        self.call(mref, &[val], IsPure::PURE)
    }
    /// Returns a reference to a `static` method of the assembly's main module
    /// (the synthetic `RustModule` class that holds the backend's builtins),
    /// with the given `name`, parameter types `inputs`, and return type `output`.
    ///
    /// This is the main-module counterpart of [`ClassRef::static_mref`]: it folds the
    /// recurring `*self.main_module()` + `alloc_string` + `sig` + `MethodRef::new(.., Static, ..)`
    /// + `alloc_methodref` boilerplate into one call. Use [`Self::call_static`] /
    /// [`Self::call_static_root`] when you immediately call the method (the common case).
    pub fn static_mref(
        &mut self,
        name: &str,
        inputs: impl Into<Box<[Type]>>,
        output: Type,
    ) -> Interned<MethodRef> {
        let main_module = *self.main_module();
        let name = self.alloc_string(name);
        let sig = self.sig(inputs, output);
        self.alloc_methodref(MethodRef::new(
            main_module,
            name,
            sig,
            MethodKind::Static,
            [].into(),
        ))
    }
    /// Builds a `Call` node invoking a `static` main-module method `name` with the given
    /// signature (`inputs` -> `output`) and `args`, with `IsPure::NOT`.
    ///
    /// Equivalent to the hand-rolled `MethodRef::new(*self.main_module(), .., Static, ..)`
    /// + `alloc_methodref` + `self.call(.., IsPure::NOT)` idiom, collapsed to one line.
    pub fn call_static(
        &mut self,
        name: &str,
        inputs: impl Into<Box<[Type]>>,
        output: Type,
        args: &[Interned<CILNode>],
    ) -> Interned<CILNode> {
        let mref = self.static_mref(name, inputs, output);
        self.call(mref, args, IsPure::NOT)
    }
    /// `Root`-producing (void-call) counterpart of [`Self::call_static`]: invokes a `static`
    /// main-module method `name` as a side-effecting [`CILRoot`] (with `IsPure::NOT`),
    /// returning the interned root.
    pub fn call_static_root(
        &mut self,
        name: &str,
        inputs: impl Into<Box<[Type]>>,
        output: Type,
        args: &[Interned<CILNode>],
    ) -> Interned<CILRoot> {
        let mref = self.static_mref(name, inputs, output);
        self.alloc_root(CILRoot::call(mref, args.to_vec()))
    }
    /// Calls a function with arguments and a certain purity.
    pub fn call(
        &mut self,
        mref: impl IntoAsmIndex<Interned<MethodRef>>,
        args: &[impl IntoAsmIndex<Interned<CILNode>> + Clone],
        is_pure: IsPure,
    ) -> Interned<CILNode> {
        let mref = mref.into_idx(self);
        let args: Vec<Interned<CILNode>> = args
            .into_iter()
            .map(|arg| IntoAsmIndex::<Interned<_>>::into_idx(arg.clone(), self))
            .collect();
        self.alloc_node(CILNode::Call(Box::new((mref, args.into(), is_pure))))
    }
    pub fn uninit_val(&mut self, tpe: impl IntoAsmIndex<Interned<Type>>) -> Interned<CILNode> {
        let tpe = tpe.into_idx(self);
        if self[tpe] == Type::Void {
            let gv = self.global_void();
            return self.load_static(gv);
        }
        let main = self.main_module();
        let sig = self.sig([], self[tpe]);
        let uninit_val = self.new_methodref(*main, "uninit_val", sig, MethodKind::Static, []);
        const EMPTY: [Interned<CILNode>; 0] = [];
        self.call(uninit_val, &EMPTY, IsPure::PURE)
    }
    /// Builds a fat-pointer value of class `slice_tpe` (a `FatPtr*` / slice class, as produced by
    /// [`crate::r#type::fat_ptr_to`]) from a thin data `ptr` and `metadata`, via the `create_slice`
    /// builtin. Used by the place pipeline (`rustc_codegen_clr_place`'s `body.rs`).
    pub fn create_slice(
        &mut self,
        slice_tpe: Interned<ClassRef>,
        ptr: impl IntoAsmIndex<Interned<CILNode>>,
        metadata: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILNode> {
        let ptr = ptr.into_idx(self);
        let metadata = metadata.into_idx(self);
        let void_ptr = self.nptr(Type::Void);
        let main = self.main_module();
        let sig = self.sig([void_ptr, Type::Int(Int::USize)], Type::ClassRef(slice_tpe));
        let create_slice = self.new_methodref(*main, "create_slice", sig, MethodKind::Static, []);
        self.call(create_slice, &[ptr, metadata], IsPure::PURE)
    }

    // ---------------------------------------------------------------------
    // Node / root construction helpers.
    //
    // Each builds one specific CIL node or root in its canonical interned form.
    // They are intentionally minimal and produce a fixed CIL shape that the
    // optimizer and exporters rely on; keep them simple — do not fold extra
    // logic into them.
    // ---------------------------------------------------------------------

    /// Loads the value of local number `arg`.
    pub fn ld_loc(&mut self, arg: u32) -> Interned<CILNode> {
        self.alloc_node(CILNode::LdLoc(arg))
    }

    /// Dereferences `addr`, loading data of type `tpe`, marking the load as `volatile`.
    pub fn load_volatile(
        &mut self,
        addr: impl IntoAsmIndex<Interned<CILNode>>,
        tpe: impl IntoAsmIndex<Interned<Type>>,
    ) -> Interned<CILNode> {
        let addr = addr.into_idx(self);
        let tpe = tpe.into_idx(self);
        self.alloc_node(CILNode::LdInd {
            addr,
            tpe,
            volatile: true,
        })
    }

    /// Casts the float `input` to the float type `target`. A signed conversion uses
    /// `is_signed = true`; an unsigned conversion uses `is_signed = false`.
    pub fn float_cast(
        &mut self,
        input: impl IntoAsmIndex<Interned<CILNode>>,
        target: super::Float,
        is_signed: bool,
    ) -> Interned<CILNode> {
        let input = input.into_idx(self);
        self.alloc_node(CILNode::FloatCast {
            input,
            target,
            is_signed,
        })
    }

    /// Reinterprets a managed reference as a raw pointer.
    pub fn ref_to_ptr(
        &mut self,
        val: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILNode> {
        let val = val.into_idx(self);
        self.alloc_node(CILNode::RefToPtr(val))
    }

    /// Loads a pointer to the function `mref`.
    pub fn ld_ftn(
        &mut self,
        mref: impl IntoAsmIndex<Interned<MethodRef>>,
    ) -> Interned<CILNode> {
        let mref = mref.into_idx(self);
        self.alloc_node(CILNode::LdFtn(mref))
    }

    /// Produces a function-pointer (`LdFtn`) value whose CIL arity matches `target_sig`, for the
    /// physical method `real_ref` (whose own physical signature is `real_ref.sig()`).
    ///
    /// The backend builds a physical method signature from *every* `fn_abi.args` entry — including
    /// `PassMode::Ignore`/ZST receivers, which lower to `Type::Void` (`valuetype RustVoid`)
    /// parameters. A bare `fn`-pointer *type*, by contrast, is built receiver-free. So when a
    /// closure / `FnDef` with a ZST receiver is turned into a bare `fn` pointer (either via a MIR
    /// `ClosureFnPointer`/`ReifyFnPointer` coercion, or via a const fn-pointer relocation in static
    /// data), the physical method has *more* parameters (the elided `Void` ones) than the fn-pointer
    /// type it is stored into and later invoked through. Taking the method's address with a plain
    /// `ldftn` then stores a pointer whose arity disagrees with the eventual indirect `calli`: the
    /// `calli` pushes too few arguments, and the callee's `ldarg.N` reads a non-existent slot
    /// (garbage). This was the root cause of the TLS `LazyStorage::initialize`
    /// `AccessViolationException`.
    ///
    /// The fix is an adapter thunk: a fresh static method whose physical signature *is* `target_sig`;
    /// it loads each real argument from its target slot, supplies an uninitialised `RustVoid` value
    /// for each elided `Void`/ZST parameter, calls the real method, and returns its result. The
    /// returned `LdFtn` points at this adapter, so the indirect `calli` and the callee agree on
    /// arity.
    ///
    /// Fast path: when `real_ref.sig()` already equals `target_sig` (no elided params), the real
    /// method's address is taken directly with no adapter. The adapter is memoised by
    /// `(real method name, target sig)`, so it is emitted exactly once across coercion sites.
    pub fn reify_fnptr(
        &mut self,
        real_ref: MethodRef,
        target_sig: Interned<FnSig>,
    ) -> Interned<CILNode> {
        let real_sig_idx = real_ref.sig();
        // Fast path: signatures already agree (the common case: no ZST/Ignore receiver was elided).
        if real_sig_idx == target_sig {
            let m = self.alloc_methodref(real_ref);
            return self.ld_ftn(m);
        }
        let real_sig = self[real_sig_idx].clone();
        let target_sig_val = self[target_sig].clone();
        let real_inputs = real_sig.inputs();
        let target_inputs = target_sig_val.inputs();
        // Compute the positional mapping between the real (keep-ZST) inputs and the target inputs.
        // The two sigs may differ ONLY by keep-ZST params whose CIL type is `Type::Void` (the elided
        // `PassMode::Ignore`/ZST args). For each real input, record whether it consumes the next
        // target slot (non-Void) or is filled with an uninitialised `Void` value (elided).
        let mut target_idx = 0usize;
        // `slot_map[i] = Some(j)` => real arg `i` is loaded from target slot `j`;
        // `slot_map[i] = None`    => real arg `i` is an elided Void/ZST arg, supplied as `uninit_val`.
        let mut slot_map: Vec<Option<usize>> = Vec::with_capacity(real_inputs.len());
        for real in real_inputs {
            if *real == Type::Void {
                slot_map.push(None);
            } else {
                let Some(target) = target_inputs.get(target_idx) else {
                    panic!(
                        "reify_fnptr: real sig has more non-Void params than target sig. real:{:?} target:{:?}",
                        real_inputs, target_inputs
                    );
                };
                assert_eq!(
                    *real, *target,
                    "reify_fnptr: real param at non-Void position {target_idx} does not match \
                     target param. real:{real_inputs:?} target:{target_inputs:?}"
                );
                slot_map.push(Some(target_idx));
                target_idx += 1;
            }
        }
        assert_eq!(
            target_idx,
            target_inputs.len(),
            "reify_fnptr: target sig has params the real sig does not consume. \
             real:{real_inputs:?} target:{target_inputs:?}"
        );
        // Deterministic, collision-free adapter name keyed on the real method + the target sig, so
        // the adapter is emitted exactly once even across multiple coercion sites (`new_method` is
        // idempotent for the same MethodRef).
        let real_name = self[real_ref.name()].to_string();
        let adapter_name = format!("{real_name}$fnptr_adapter${}", target_sig.inner());
        let main_module = self.main_module();
        let adapter_ref = MethodRef::new(
            *main_module,
            self.alloc_string(adapter_name.clone()),
            target_sig,
            MethodKind::Static,
            vec![].into(),
        );
        let adapter_ref_idx = self.alloc_methodref(adapter_ref);
        // Build the adapter body: load each real argument from its slot (or an uninit Void value),
        // call the real method, and return its result.
        let real_ret = *real_sig.output();
        let real_ref_idx = self.alloc_methodref(real_ref);
        let args: Vec<Interned<CILNode>> = slot_map
            .iter()
            .map(|slot| match slot {
                Some(target_index) => self.alloc_node(CILNode::LdArg(*target_index as u32)),
                None => self.uninit_val(Type::Void),
            })
            .collect();
        let call = self.call(real_ref_idx, &args, IsPure::NOT);
        let ret_root = if real_ret == Type::Void {
            self.alloc_root(CILRoot::VoidRet)
        } else {
            self.alloc_root(CILRoot::Ret(call))
        };
        let block = super::BasicBlock::new(vec![ret_root], 0, None);
        let arg_names = (0..target_inputs.len()).map(|_| None).collect();
        let adapter_def = MethodDef::new(
            Access::Private,
            main_module,
            self.alloc_string(adapter_name),
            target_sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![block],
                locals: vec![],
            },
            arg_names,
        );
        self.new_method(adapter_def);
        self.ld_ftn(adapter_ref_idx)
    }

    /// Loads the length of a platform array `arr`.
    pub fn ld_len(
        &mut self,
        arr: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILNode> {
        let arr = arr.into_idx(self);
        self.alloc_node(CILNode::LdLen(arr))
    }

    /// Loads a reference to the element of `array` at `index`.
    pub fn ld_elem_ref(
        &mut self,
        array: impl IntoAsmIndex<Interned<CILNode>>,
        index: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILNode> {
        let array = array.into_idx(self);
        let index = index.into_idx(self);
        self.alloc_node(CILNode::LdElelemRef { array, index })
    }

    /// Allocates a new 1-D managed (platform) array of `elem` with `len` elements (`newarr`).
    pub fn new_arr(
        &mut self,
        elem: impl IntoAsmIndex<Interned<Type>>,
        len: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILNode> {
        let elem = elem.into_idx(self);
        let len = len.into_idx(self);
        self.alloc_node(CILNode::NewArr { elem, len })
    }

    /// Stores `value` (of element type `elem`) into managed array `array` at `index` (`stelem`).
    pub fn st_elem(
        &mut self,
        array: impl IntoAsmIndex<Interned<CILNode>>,
        index: impl IntoAsmIndex<Interned<CILNode>>,
        value: impl IntoAsmIndex<Interned<CILNode>>,
        elem: impl IntoAsmIndex<Interned<Type>>,
    ) -> CILRoot {
        let array = array.into_idx(self);
        let index = index.into_idx(self);
        let value = value.into_idx(self);
        let elem = elem.into_idx(self);
        CILRoot::StElem {
            array,
            index,
            value,
            elem,
        }
    }

    /// Unboxes the managed `object` into a value of `tpe`.
    pub fn unbox_any(
        &mut self,
        object: impl IntoAsmIndex<Interned<CILNode>>,
        tpe: impl IntoAsmIndex<Interned<Type>>,
    ) -> Interned<CILNode> {
        let object = object.into_idx(self);
        let tpe = tpe.into_idx(self);
        self.alloc_node(CILNode::UnboxAny { object, tpe })
    }

    /// Allocates `size` bytes from the local (per-call) pool.
    pub fn loc_alloc(
        &mut self,
        size: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILNode> {
        let size = size.into_idx(self);
        self.alloc_node(CILNode::LocAlloc { size })
    }

    /// Allocates a local buffer of `sizeof(tpe)` aligned to `align`.
    pub fn loc_alloc_aligned(
        &mut self,
        tpe: impl IntoAsmIndex<Interned<Type>>,
        align: u64,
    ) -> Interned<CILNode> {
        let tpe = tpe.into_idx(self);
        self.alloc_node(CILNode::LocAllocAlgined { tpe, align })
    }

    /// Loads a "type token" for `tpe`.
    pub fn ld_type_token(
        &mut self,
        tpe: impl IntoAsmIndex<Interned<Type>>,
    ) -> Interned<CILNode> {
        let tpe = tpe.into_idx(self);
        self.alloc_node(CILNode::LdTypeToken(tpe))
    }

    /// Checks whether `val` is an instance of class `class` (the class ref is wrapped in
    /// `Type::ClassRef`).
    pub fn is_inst(
        &mut self,
        val: impl IntoAsmIndex<Interned<CILNode>>,
        class: Interned<ClassRef>,
    ) -> Interned<CILNode> {
        let val = val.into_idx(self);
        let tpe = self.alloc_type(Type::ClassRef(class));
        self.alloc_node(CILNode::IsInst(val, tpe))
    }

    /// Casts `val` to an instance of class `class`, throwing on failure (the class ref is wrapped
    /// in `Type::ClassRef`).
    pub fn checked_cast(
        &mut self,
        val: impl IntoAsmIndex<Interned<CILNode>>,
        class: Interned<ClassRef>,
    ) -> Interned<CILNode> {
        let val = val.into_idx(self);
        let tpe = self.alloc_type(Type::ClassRef(class));
        self.alloc_node(CILNode::CheckedCast(val, tpe))
    }

    /// Calls function pointer `fn_ptr` of signature `sig` with `args`.
    pub fn call_indirect(
        &mut self,
        sig: Interned<FnSig>,
        fn_ptr: impl IntoAsmIndex<Interned<CILNode>>,
        args: impl Into<Box<[Interned<CILNode>]>>,
    ) -> Interned<CILNode> {
        let fn_ptr = fn_ptr.into_idx(self);
        self.alloc_node(CILNode::CallI(Box::new((fn_ptr, sig, args.into()))))
    }

    // --- Roots ---

    /// Stores `tree` into local number `local`.
    pub fn st_loc(
        &mut self,
        local: u32,
        tree: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILRoot> {
        let tree = tree.into_idx(self);
        self.alloc_root(CILRoot::StLoc(local, tree))
    }

    /// Stores `tree` into argument number `arg`.
    pub fn st_arg(
        &mut self,
        arg: u32,
        tree: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILRoot> {
        let tree = tree.into_idx(self);
        self.alloc_root(CILRoot::StArg(arg, tree))
    }

    /// Returns `tree`.
    pub fn ret(&mut self, tree: impl IntoAsmIndex<Interned<CILNode>>) -> Interned<CILRoot> {
        let tree = tree.into_idx(self);
        self.alloc_root(CILRoot::Ret(tree))
    }

    /// Pops (and discards) `tree`.
    pub fn pop(&mut self, tree: impl IntoAsmIndex<Interned<CILNode>>) -> Interned<CILRoot> {
        let tree = tree.into_idx(self);
        self.alloc_root(CILRoot::Pop(tree))
    }

    /// Stores `val` (of type `tpe`) at address `addr`. This single root expresses every
    /// indirect store, regardless of the stored type. `volatile` is `false` for an ordinary
    /// store; set it to `true` for a volatile store.
    pub fn st_ind(
        &mut self,
        addr: impl IntoAsmIndex<Interned<CILNode>>,
        val: impl IntoAsmIndex<Interned<CILNode>>,
        tpe: Type,
        volatile: bool,
    ) -> Interned<CILRoot> {
        let addr = addr.into_idx(self);
        let val = val.into_idx(self);
        self.alloc_root(CILRoot::StInd(Box::new((addr, val, tpe, volatile))))
    }

    /// Sets `field` of the object at `addr` to `value`. The resulting root is
    /// `SetField(field, addr, value)`.
    pub fn set_field(
        &mut self,
        field: Interned<FieldDesc>,
        addr: impl IntoAsmIndex<Interned<CILNode>>,
        value: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILRoot> {
        let addr = addr.into_idx(self);
        let value = value.into_idx(self);
        self.alloc_root(CILRoot::SetField(Box::new((field, addr, value))))
    }

    /// Zero-initializes the value of `tpe` at address `addr`.
    pub fn init_obj(
        &mut self,
        addr: impl IntoAsmIndex<Interned<CILNode>>,
        tpe: Interned<Type>,
    ) -> Interned<CILRoot> {
        let addr = addr.into_idx(self);
        self.alloc_root(CILRoot::InitObj(addr, tpe))
    }

    /// Fills `count` bytes at `dst` with `val`.
    pub fn init_blk(
        &mut self,
        dst: impl IntoAsmIndex<Interned<CILNode>>,
        val: impl IntoAsmIndex<Interned<CILNode>>,
        count: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILRoot> {
        let dst = dst.into_idx(self);
        let val = val.into_idx(self);
        let count = count.into_idx(self);
        self.alloc_root(CILRoot::InitBlk(Box::new((dst, val, count))))
    }

    /// Copies `len` bytes from `src` to `dst`.
    pub fn cp_blk(
        &mut self,
        dst: impl IntoAsmIndex<Interned<CILNode>>,
        src: impl IntoAsmIndex<Interned<CILNode>>,
        len: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILRoot> {
        let dst = dst.into_idx(self);
        let src = src.into_idx(self);
        let len = len.into_idx(self);
        self.alloc_root(CILRoot::CpBlk(Box::new((dst, src, len))))
    }

    /// Sets static field `field` to `val`.
    pub fn set_static_field(
        &mut self,
        field: impl IntoAsmIndex<Interned<StaticFieldDesc>>,
        val: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILRoot> {
        let field = field.into_idx(self);
        let val = val.into_idx(self);
        self.alloc_root(CILRoot::SetStaticField { field, val })
    }

    /// A branch to `target`/`sub_target`; unconditional when `cond` is `None`.
    pub fn branch(
        &mut self,
        target: u32,
        sub_target: u32,
        cond: Option<super::BranchCond>,
    ) -> Interned<CILRoot> {
        self.alloc_root(CILRoot::Branch(Box::new((target, sub_target, cond))))
    }

    /// Calls fn pointer `fn_ptr` of signature `sig` with `args` as a statement.
    pub fn call_indirect_root(
        &mut self,
        sig: Interned<FnSig>,
        fn_ptr: impl IntoAsmIndex<Interned<CILNode>>,
        args: impl Into<Box<[Interned<CILNode>]>>,
    ) -> Interned<CILRoot> {
        let fn_ptr = fn_ptr.into_idx(self);
        self.alloc_root(CILRoot::CallI(Box::new((fn_ptr, sig, args.into()))))
    }

    /// Casts the pointer-like `val` to the pointer type `new_ptr`: dispatches on `new_ptr` to the
    /// matching [`PtrCastRes`]. `new_ptr` must be a `Ptr`/`Ref`/`FnPtr`/`USize`/`ISize`.
    pub fn cast_ptr_to(
        &mut self,
        val: impl IntoAsmIndex<Interned<CILNode>>,
        new_ptr: Type,
    ) -> Interned<CILNode> {
        let val = val.into_idx(self);
        let res = match new_ptr {
            Type::Int(Int::USize) => PtrCastRes::USize,
            Type::Int(Int::ISize) => PtrCastRes::ISize,
            Type::Ptr(inner) => PtrCastRes::Ptr(inner),
            Type::Ref(inner) => PtrCastRes::Ref(inner),
            Type::FnPtr(sig) => PtrCastRes::FnPtr(sig),
            _ => panic!("Type {new_ptr:?} is not a pointer."),
        };
        self.alloc_node(CILNode::PtrCast(val, Box::new(res)))
    }

    /// Selects between `a` and `b` based on `predicate`.
    pub fn select(
        &mut self,
        tpe: Type,
        a: impl IntoAsmIndex<Interned<CILNode>>,
        b: impl IntoAsmIndex<Interned<CILNode>>,
        predicate: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILNode> {
        let a = a.into_idx(self);
        let b = b.into_idx(self);
        let predicate = predicate.into_idx(self);
        match tpe {
            Type::Int(
                int @ (Int::I8
                | Int::U8
                | Int::I16
                | Int::U16
                | Int::I32
                | Int::U32
                | Int::I64
                | Int::U64
                | Int::I128
                | Int::U128
                | Int::ISize
                | Int::USize),
            ) => {
                let main = *self.main_module();
                let name = format!("select_{}", int.name());
                let sig = self.sig([Type::Int(int), Type::Int(int), Type::Bool], Type::Int(int));
                let select = self.new_methodref(main, name, sig, MethodKind::Static, []);
                self.call(select, &[a, b, predicate], IsPure::PURE)
            }
            Type::Ptr(inner) => {
                let int = Int::USize;
                let main = *self.main_module();
                let name = format!("select_{}", int.name());
                let sig = self.sig([Type::Int(int), Type::Int(int), Type::Bool], Type::Int(int));
                let select = self.new_methodref(main, name, sig, MethodKind::Static, []);
                let a = self.ptr_cast(a, PtrCastRes::USize);
                let a = self.alloc_node(a);
                let b = self.ptr_cast(b, PtrCastRes::USize);
                let b = self.alloc_node(b);
                let call = self.call(select, &[a, b, predicate], IsPure::PURE);
                self.cast_ptr(call, inner)
            }
            _ => todo!("Can't select {tpe:?}"),
        }
    }

    /// Builds the overflow-check result tuple `(val, out_of_range)` of class `tuple`: a pure call
    /// to `ovf_check_tuple(tpe, bool) -> tuple` with args `[val, out_of_range]`.
    pub fn ovf_check_tuple(
        &mut self,
        tuple: Interned<ClassRef>,
        out_of_range: impl IntoAsmIndex<Interned<CILNode>>,
        val: impl IntoAsmIndex<Interned<CILNode>>,
        tpe: Type,
    ) -> Interned<CILNode> {
        let out_of_range = out_of_range.into_idx(self);
        let val = val.into_idx(self);
        let main = self.main_module();
        let sig = self.sig([tpe, Type::Bool], Type::ClassRef(tuple));
        let site = self.new_methodref(*main, "ovf_check_tuple", sig, MethodKind::Static, []);
        self.call(site, &[val, out_of_range], IsPure::PURE)
    }

    /// Negates `val`.
    pub fn neg(&mut self, val: impl IntoAsmIndex<Interned<CILNode>>) -> Interned<CILNode> {
        let val = val.into_idx(self);
        self.alloc_node(CILNode::UnOp(val, UnOp::Neg))
    }

    /// Bitwise/logical-nots `val`.
    pub fn not(&mut self, val: impl IntoAsmIndex<Interned<CILNode>>) -> Interned<CILNode> {
        let val = val.into_idx(self);
        self.alloc_node(CILNode::UnOp(val, UnOp::Not))
    }

    /// Allocates an anonymous static initialized to `val` and loads its address.
    pub fn stack_addr(
        &mut self,
        val: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILNode> {
        let val = val.into_idx(self);
        let sfld = self.annon_const(val);
        self.alloc_node(CILNode::LdStaticFieldAddress(sfld))
    }

    /// Calls `mref` with `args` as a statement. `is_pure` is taken explicitly to match the
    /// node-level [`Self::call`] for the rare pure-call statement cases.
    pub fn call_root(
        &mut self,
        mref: impl IntoAsmIndex<Interned<MethodRef>>,
        args: &[impl IntoAsmIndex<Interned<CILNode>> + Clone],
        is_pure: IsPure,
    ) -> Interned<CILRoot> {
        let mref = mref.into_idx(self);
        let args: Vec<Interned<CILNode>> = args
            .iter()
            .map(|arg| IntoAsmIndex::<Interned<_>>::into_idx(arg.clone(), self))
            .collect();
        self.alloc_root(CILRoot::Call(Box::new((mref, args.into(), is_pure))))
    }

    pub(crate) fn throw(
        &mut self,
        exception: impl IntoAsmIndex<Interned<CILNode>>,
    ) -> Interned<CILRoot> {
        let exception = exception.into_idx(self);
        self.alloc_root(CILRoot::Throw(exception))
    }

    /// Builds a runtime string node concatenating `pieces` (used by [`Self::debug_msg`]).
    fn runtime_string(&mut self, pieces: &[&str]) -> Interned<CILNode> {
        match pieces.len() {
            0 => panic!("Incorrect piece count"),
            1 => {
                let s = self.alloc_string(pieces[0].to_owned());
                self.alloc_node(CILNode::Const(Box::new(Const::PlatformString(s))))
            }
            n @ (2 | 3 | 4) => {
                let string = ClassRef::string(self);
                let name = self.alloc_string("Concat");
                let inputs: Vec<Type> = (0..n).map(|_| Type::PlatformString).collect();
                let sig = self.sig(inputs, Type::PlatformString);
                let mref = self.alloc_methodref(MethodRef::new(
                    string,
                    name,
                    sig,
                    MethodKind::Static,
                    vec![].into(),
                ));
                let args: Vec<Interned<CILNode>> = pieces
                    .iter()
                    .map(|p| {
                        let s = self.alloc_string((*p).to_owned());
                        self.alloc_node(CILNode::Const(Box::new(Const::PlatformString(s))))
                    })
                    .collect();
                self.call(mref, &args, IsPure::NOT)
            }
            _ => {
                let sub_part = pieces.len() / 4;
                let string = ClassRef::string(self);
                let name = self.alloc_string("Concat");
                let sig = self.sig(
                    [
                        Type::PlatformString,
                        Type::PlatformString,
                        Type::PlatformString,
                        Type::PlatformString,
                    ],
                    Type::PlatformString,
                );
                let mref = self.alloc_methodref(MethodRef::new(
                    string,
                    name,
                    sig,
                    MethodKind::Static,
                    vec![].into(),
                ));
                let a = self.runtime_string(&pieces[..sub_part]);
                let b = self.runtime_string(&pieces[sub_part..(sub_part * 2)]);
                let c = self.runtime_string(&pieces[(sub_part * 2)..(sub_part * 3)]);
                let d = self.runtime_string(&pieces[(sub_part * 3)..]);
                self.call(mref, &[a, b, c, d], IsPure::NOT)
            }
        }
    }

    /// Re-emits the `StInd` `root` with its volatile flag set to `true`.
    /// Panics if `root` is not a `StInd`.
    pub fn make_store_volatile(&mut self, root: Interned<CILRoot>) -> Interned<CILRoot> {
        let CILRoot::StInd(inner) = self.get_root(root).clone() else {
            panic!("make_store_volatile called on a non-StInd root")
        };
        let (addr, val, tpe, _) = *inner;
        self.alloc_root(CILRoot::StInd(Box::new((addr, val, tpe, true))))
    }

    /// Builds a root that writes `msg` to the console.
    pub fn debug_msg(&mut self, msg: &str) -> Interned<CILRoot> {
        let class = ClassRef::console(self);
        let name = self.alloc_string("WriteLine");
        let signature = self.sig([Type::PlatformString], Type::Void);
        let mref = self.alloc_methodref(MethodRef::new(
            class,
            name,
            signature,
            MethodKind::Static,
            vec![].into(),
        ));
        let pieces: Vec<&str> = msg.split_inclusive(char::is_whitespace).collect();
        let message = self.runtime_string(&pieces);
        self.call_root(mref, &[message], IsPure::NOT)
    }

    /// Builds a root that writes the integer `val` (widened to i64) to the console — for runtime value
    /// tracing (e.g. a `SwitchInt` discriminant / niche tag). `signed` selects sign- vs zero-extension.
    /// Used by the `TRACE_VAL` debug hook (see `src/terminator/mod.rs::handle_switch`).
    pub fn debug_val(&mut self, val: Interned<CILNode>, signed: bool) -> Interned<CILRoot> {
        let class = ClassRef::console(self);
        let name = self.alloc_string("WriteLine");
        let signature = self.sig([Type::Int(Int::I64)], Type::Void);
        let mref = self.alloc_methodref(MethodRef::new(
            class,
            name,
            signature,
            MethodKind::Static,
            vec![].into(),
        ));
        let extend = if signed {
            ExtendKind::SignExtend
        } else {
            ExtendKind::ZeroExtend
        };
        let i64val = self.int_cast(val, Int::I64, extend);
        self.call_root(mref, &[i64val], IsPure::NOT)
    }

    /// Builds a root that throws a new `Exception` with message `msg`.
    pub fn throw_msg(&mut self, msg: &str) -> Interned<CILRoot> {
        let class = ClassRef::exception(self);
        let name = self.alloc_string(".ctor");
        let signature = self.sig([class.into(), Type::PlatformString], Type::Void);
        let ctor = self.alloc_methodref(MethodRef::new(
            class,
            name,
            signature,
            MethodKind::Constructor,
            vec![].into(),
        ));
        let msg = self.alloc_string(msg);
        let msg = self.alloc_node(CILNode::Const(Box::new(Const::PlatformString(msg))));
        let exception = self.call(ctor, &[msg], IsPure::NOT);
        self.throw(exception)
    }
}
config!(GUARANTEED_ALIGN, u8, 8);
config!(MAX_STATIC_SIZE, usize, 16);
/// An initializer, which runs before everything else. By convention, it is used to initialize static / const data. Should not execute any user code
pub const CCTOR: &str = ".cctor";
/// An thread-local initializer. Runs before each thread starts. By convention, it is used to initialize thread local data. Should not execute any user code.
pub const TCCTOR: &str = ".tcctor";
/// An initializer, which runs after the [`CCTOR`] and [`TCCTOR`], but before the [`ENTRYPOINT`]. Meant to execute user code, is roughly equivalnt to `.init_array` on GNU.
pub const USER_INIT: &str = "static_init";
/// The entrypoint of a program
pub const ENTRYPOINT: &str = "entrypoint";
/// Main class of this module
pub const MAIN_MODULE: &str = "MainModule";
#[test]
fn test_encoded_stats() {
    assert_eq!(encoded_stats(&u64::MAX), (type_name::<u64>(), 10));
    assert_eq!(encoded_stats(&0_i32), (type_name::<i32>(), 1));
}
pub fn encoded_stats<T: Serialize + for<'a> Deserialize<'a>>(val: &T) -> (&'static str, usize) {
    let buff = postcard::to_allocvec(val).unwrap();
    let start = std::time::Instant::now();
    let _: T = postcard::from_bytes(&buff).unwrap();
    let end = std::time::Instant::now();
    println!(
        "Decoding {} took {} ms",
        type_name::<T>(),
        end.duration_since(start).as_millis()
    );
    (type_name::<T>(), buff.len())
}

pub static ILASM_FLAVOUR: std::sync::LazyLock<IlasmFlavour> = std::sync::LazyLock::new(|| {
    if String::from_utf8_lossy(
            &std::process::Command::new(ilasm_path()).arg("--help")
                .output()
                .unwrap_or_else(|_| panic!("Could not run the IL assembler(ilasm) at path {:?}. Is ilasm propely installed? If so, try specifying a precise path by seting the ILASM_PATH enviroment variable",*ILASM_PATH))
                .stdout,
        )
        .contains("PDB")
        {
            IlasmFlavour::Modern
        } else {
            IlasmFlavour::Clasic
        }
});

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IlasmFlavour {
    Clasic,
    Modern,
}
#[must_use]
pub fn ilasm_path() -> &'static str {
    ILASM_PATH.as_str()
}
// Only exercised by the `test_chunked_range` unit test; unused in non-test builds.
#[allow(dead_code)]
fn chunked_range(top: u32, parts: u32) -> impl Iterator<Item = std::ops::Range<u32>> {
    let chunk_size = top.div_ceil(parts); // Ceiling of n / m

    assert!(parts < top);
    (0..top).filter_map(move |i| {
        let start = i * chunk_size;
        let end = std::cmp::min(start + chunk_size, top);
        if start < top {
            Some(start..end)
        } else {
            None
        }
    })
}
#[doc = "Specifies the path to the IL assembler."]
pub static ILASM_PATH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::vars()
        .find_map(|(key, value)| {
            if key == "ILASM_PATH" {
                Some(value)
            } else {
                None
            }
        })
        .unwrap_or(get_default_ilasm())
});

/// The target .NET runtime version. Single source of truth for every version-specific string the
/// backend emits (`runtimeconfig.json` TFM, `.assembly extern` `.ver` stamps) and for version-gated
/// codegen (e.g. native sub-word `Interlocked` overloads, added in .NET 9). Read once from the
/// `DOTNET_VERSION` env var via [`dotnet_version`]; defaults to [`DotnetVersion::Net8`] so an
/// unconfigured build keeps the historical .NET 8 behaviour.
///
/// Declaration order is load-bearing: `Net8 < Net9`, so feature gates read `version >= Net9`
/// (and a future `Net10` auto-takes the newer path with no edit).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Default)]
pub enum DotnetVersion {
    /// .NET 8 (the default / primary CI target).
    #[default]
    Net8,
    /// .NET 9.
    Net9,
}
impl DotnetVersion {
    /// Target-framework moniker for `runtimeconfig.json` / `.nuspec` (`net8.0` / `net9.0`).
    #[must_use]
    pub fn tfm(self) -> &'static str {
        match self {
            DotnetVersion::Net8 => "net8.0",
            DotnetVersion::Net9 => "net9.0",
        }
    }
    /// The `.ver` triplet for a BCL `.assembly extern` stamp (`8:0:0:0` / `9:0:0:0`).
    ///
    /// NOTE: the public-key *tokens* are version-INVARIANT (verified identical on 8 and 9) — only
    /// this triplet changes — so there is deliberately no token accessor.
    #[must_use]
    pub fn assembly_ver(self) -> &'static str {
        match self {
            DotnetVersion::Net8 => "8:0:0:0",
            DotnetVersion::Net9 => "9:0:0:0",
        }
    }
    /// A `Microsoft.NETCore.App` framework-version floor for `runtimeconfig.json` (`8.0.0` / `9.0.0`),
    /// paired with roll-forward to the latest installed patch.
    #[must_use]
    pub fn framework_version(self) -> &'static str {
        match self {
            DotnetVersion::Net8 => "8.0.0",
            DotnetVersion::Net9 => "9.0.0",
        }
    }
    /// The major version number (`8` / `9`).
    #[must_use]
    pub fn major(self) -> u32 {
        match self {
            DotnetVersion::Net8 => 8,
            DotnetVersion::Net9 => 9,
        }
    }
}
impl std::str::FromStr for DotnetVersion {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "8" | "net8" | "net8.0" => Ok(DotnetVersion::Net8),
            "9" | "net9" | "net9.0" => Ok(DotnetVersion::Net9),
            other => Err(format!(
                "DOTNET_VERSION has invalid value {other:?}; expected 8 or 9 (or net8.0/net9.0)"
            )),
        }
    }
}
/// The target .NET version for this process, read once from the `DOTNET_VERSION` env var (default
/// [`DotnetVersion::Net8`]). Both the codegen backend and the (separate-process) linker read this,
/// so a build must set `DOTNET_VERSION` in BOTH environments.
pub static DOTNET_VERSION: std::sync::LazyLock<DotnetVersion> = std::sync::LazyLock::new(|| {
    match std::env::var("DOTNET_VERSION") {
        Ok(val) => val.parse().unwrap_or_else(|e| panic!("{e}")),
        Err(_) => DotnetVersion::default(),
    }
});
/// Convenience accessor for [`DOTNET_VERSION`] — the target .NET version of this build.
#[must_use]
pub fn dotnet_version() -> DotnetVersion {
    *DOTNET_VERSION
}

#[cfg(test)]
mod dotnet_version_tests {
    use super::DotnetVersion;
    #[test]
    fn parse_and_order() {
        assert_eq!("8".parse(), Ok(DotnetVersion::Net8));
        assert_eq!("9".parse(), Ok(DotnetVersion::Net9));
        assert_eq!("net9.0".parse(), Ok(DotnetVersion::Net9));
        assert_eq!(DotnetVersion::default(), DotnetVersion::Net8);
        assert!(DotnetVersion::Net9 > DotnetVersion::Net8);
        assert!(!(DotnetVersion::Net8 >= DotnetVersion::Net9));
        assert!("7".parse::<DotnetVersion>().is_err());
        assert!("".parse::<DotnetVersion>().is_err());
        assert_eq!(DotnetVersion::Net8.tfm(), "net8.0");
        assert_eq!(DotnetVersion::Net9.assembly_ver(), "9:0:0:0");
        assert_eq!(DotnetVersion::Net9.major(), 9);
    }
}

#[cfg(not(target_os = "windows"))]
/// Finds the default instance of the IL assembler.
fn get_default_ilasm() -> String {
    "ilasm".into()
}
#[test]
#[cfg(not(miri))]
fn test_chunked_range() {
    for count in 1..100 {
        for parts in 1..count {
            let range = chunked_range(count, parts);
            assert_eq!(
                range.flatten().max().unwrap(),
                count - 1,
                "count:{count},parts:{parts},range:"
            );
            let range = chunked_range(count, parts);
            assert_eq!(
                range.flatten().count(),
                count.try_into().unwrap(),
                "count:{count},parts:{parts},range:"
            );
        }
    }
}
#[test]
fn test_get_default_ilasm() {
    assert!(get_default_ilasm().contains("ilasm"));
}
#[cfg(target_os = "windows")]
fn get_default_ilasm() -> String {
    if std::process::Command::new("ilasm")
        .arg("--help")
        .output()
        .is_ok()
    {
        return "ilasm".into();
    }
    // Framework Path
    let framework_path = std::path::PathBuf::from("C:\\Windows\\Microsoft.NET\\Framework");
    let framework_dir = std::fs::read_dir(&framework_path).unwrap_or_else(|_| panic!("Could not find the .NET framework directory at {framework_path:?}, when searching for ilasm."));
    for entry in framework_dir {
        let entry = entry.unwrap();
        // TODO: find the most recent framework
        if entry.metadata().unwrap().is_dir() {
            let mut ilasm_path = entry.path();
            ilasm_path.push("ilasm");
            ilasm_path.set_extension("exe");
            if !std::fs::exists(&ilasm_path).unwrap_or(false) {
                eprintln!("Could not find ilasm at:{ilasm_path:?}");
                continue;
            }
            return ilasm_path.display().to_string();
        }
    }
    panic!("Could not find a .NET framework in directory {framework_path:?}, when searching for ilasm.")
}
#[test]
fn user_init() {
    let mut asm = Assembly::default();
    asm.user_init();
}
#[test]
fn add_user_init() {
    let mut asm = Assembly::default();
    let roots = vec![
        asm.alloc_root(CILRoot::VoidRet),
        asm.alloc_root(CILRoot::Break),
        asm.alloc_root(CILRoot::Nop),
    ];
    asm.add_user_init(&roots);
}
#[test]
fn export() {
    use super::il_exporter::*;

    let mut asm = Assembly::default();
    let main_module = asm.main_module();
    let name = asm.alloc_string("entrypoint");
    let sig = asm.sig([], Type::Void);
    let body = vec![asm.alloc_root(CILRoot::VoidRet)];
    asm.new_method(MethodDef::new(
        Access::Extern,
        main_module,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(body, 0, None)],
            locals: vec![],
        },
        vec![],
    ));
    let type_idx = asm.alloc_type(Type::Int(super::Int::I8));
    let sig = asm.sig([Type::Ptr(type_idx)], Type::Void);
    let name = asm.alloc_string("pritnf");
    let lib = asm.alloc_string("/lib/libc.so");
    asm.new_method(MethodDef::new(
        Access::Extern,
        main_module,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::Extern {
            lib,
            preserve_errno: false,
        },
        vec![None],
    ));
    #[cfg(not(miri))]
    asm.export("/tmp/export.exe", ILExporter::new(*ILASM_FLAVOUR, false, None));
}
#[test]
fn export2() {
    use super::il_exporter::*;

    let mut asm = Assembly::default();
    let main_module = asm.main_module();
    let name = asm.alloc_string("entrypoint");
    let sig = asm.sig([], Type::Void);
    let buff = asm.bytebuffer(b"Hewwo!", Int::U8);
    let body1 = vec![
        asm.alloc_root(CILRoot::VoidRet),
        asm.alloc_root(CILRoot::Pop(buff)),
    ];
    let hbody = vec![asm.alloc_root(CILRoot::ExitSpecialRegion {
        target: 2,
        source: 0,
    })];
    asm.alloc_root(CILRoot::Break);
    let body2 = vec![asm.alloc_root(CILRoot::VoidRet)];
    asm.new_method(MethodDef::new(
        Access::Extern,
        main_module,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![
                super::BasicBlock::new(
                    body1,
                    0,
                    Some(vec![super::BasicBlock::new(hbody, 1, None)]),
                ),
                super::BasicBlock::new(body2, 2, None),
            ],
            locals: vec![],
        },
        vec![],
    ));
    let type_idx = asm.alloc_type(Type::Int(super::Int::I8));
    let sig = asm.sig([Type::Ptr(type_idx)], Type::Void);
    let name = asm.alloc_string("pritnf");
    let lib = asm.alloc_string("/lib/libc.so");
    asm.new_method(MethodDef::new(
        Access::Public,
        main_module,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::Extern {
            lib,
            preserve_errno: false,
        },
        vec![None],
    ));
    let uinit = asm.alloc_root(CILRoot::Break);
    asm.add_user_init(&[uinit]);
    asm.eliminate_dead_code();
    asm.realloc_roots();
    #[cfg(not(miri))]
    asm.export("/tmp/export2.exe", ILExporter::new(*ILASM_FLAVOUR, false, None));
}
#[test]
fn link() {
    use super::il_exporter::*;

    let asm1 = {
        let mut asm = Assembly::default();
        let main_module = asm.main_module();
        let name = asm.alloc_string("entrypoint");
        let sig = asm.sig([], Type::Void);
        let body1 = vec![asm.alloc_root(CILRoot::VoidRet)];
        let hbody = vec![asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 2,
            source: 0,
        })];
        asm.alloc_root(CILRoot::Break);
        let body2 = vec![asm.alloc_root(CILRoot::VoidRet)];
        asm.new_method(MethodDef::new(
            Access::Extern,
            main_module,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![
                    super::BasicBlock::new(
                        body1,
                        0,
                        Some(vec![super::BasicBlock::new(hbody, 1, None)]),
                    ),
                    super::BasicBlock::new(body2, 2, None),
                ],
                locals: vec![],
            },
            vec![],
        ));
        asm
    };
    let asm2 = {
        let mut asm = Assembly::default();
        let main_module = asm.main_module();
        let type_idx = asm.alloc_type(Type::Int(super::Int::I8));
        let sig = asm.sig([Type::Ptr(type_idx)], Type::Void);
        let name = asm.alloc_string("pritnf");
        let lib = asm.alloc_string("/lib/libc.so");
        asm.new_method(MethodDef::new(
            Access::Public,
            main_module,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::Extern {
                lib,
                preserve_errno: false,
            },
            vec![None],
        ));
        let uinit = asm.alloc_root(CILRoot::Break);
        asm.add_user_init(&[uinit]);
        asm
    };
    let mut asm = asm1.link(asm2);
    asm.eliminate_dead_code();
    asm.realloc_roots();
    #[cfg(not(miri))]
    asm.export("/tmp/link_test.exe", ILExporter::new(*ILASM_FLAVOUR, false, None));
}
config! {LINKER_RECOVER,bool,false}
