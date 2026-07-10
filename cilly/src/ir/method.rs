use fxhash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use super::{
    asm_link::{RelocateCtx, RelocateValue},
    basic_block::BlockId,
    bimap::Interned,
    cilnode::{IsPure, MethodKind},
    class::ClassDefIdx,
    Access, Assembly, BasicBlock, CILIterElem, CILNode, ClassRef, FnSig, Int, IntoAsmIndex, Type,
};
use crate::{cilnode::PtrCastRes, iter::TpeIter};
use crate::{CILRoot, IString};
pub type LocalId = u32;
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct MethodRef {
    class: Interned<ClassRef>,
    name: Interned<IString>,
    sig: Interned<FnSig>,
    /// Static/instance/virtual/constructor — which ECMA-335 calling convention and dispatch
    /// (`call` vs `callvirt`) this reference requires.
    kind: MethodKind,
    /// Generic arguments bound at the call site for this *method* (`!!N`), distinct from
    /// `class`'s own generic arguments (`!N`) — the two tiers are bound independently.
    generics: Box<[Type]>,
}
impl RelocateValue for MethodRef {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self {
            class,
            name,
            sig,
            kind,
            generics,
        } = self;
        Self {
            class: ctx.class_ref(destination, class),
            name: ctx.string(destination, name),
            sig: ctx.signature(destination, sig),
            kind,
            generics: generics
                .iter()
                .map(|tpe| destination.translate_type(ctx, *tpe))
                .collect(),
        }
    }
}
impl IntoAsmIndex<Interned<MethodRef>> for MethodRef {
    fn into_idx(self, asm: &mut Assembly) -> Interned<MethodRef> {
        asm.alloc_methodref(self)
    }
}
impl Interned<MethodRef> {
    pub fn builtin(
        asm: &mut Assembly,
        name: &str,
        inputs: &[Type],
        output: impl IntoAsmIndex<Type>,
    ) -> Self {
        let main_module = asm.main_module();
        let inputs: Box<_> = inputs.to_vec().into();
        let output = output.into_idx(asm);
        let sig = asm.alloc_sig(FnSig::new(inputs, output));
        asm.new_methodref(*main_module, name, sig, MethodKind::Static, [])
    }
    pub fn unaligned_read(asm: &mut Assembly, tpe: Type) -> Self {
        let tpe_ptr = asm.alloc_type(tpe);
        Self::builtin(asm, "unaligned_read", &[Type::Ptr(tpe_ptr)], tpe)
    }
}
impl MethodRef {
    #[must_use]
    pub fn into_def(
        &self,
        implementation: MethodImpl,
        access: Access,
        asm: &Assembly,
    ) -> MethodDef {
        let class = asm.class_ref_to_def(self.class()).unwrap();
        let arg_names = (0..(asm[self.sig()].inputs().len()))
            .map(|_| None)
            .collect();
        MethodDef::new(
            access,
            class,
            self.name,
            self.sig,
            self.kind,
            implementation,
            arg_names,
        )
    }
    #[must_use]
    pub fn new(
        class: Interned<ClassRef>,
        name: Interned<IString>,
        sig: Interned<FnSig>,
        kind: MethodKind,
        generics: Box<[Type]>,
    ) -> Self {
        Self {
            class,
            name,
            sig,
            kind,
            generics,
        }
    }

    #[must_use]
    pub fn class(&self) -> Interned<ClassRef> {
        self.class
    }

    #[must_use]
    pub fn name(&self) -> Interned<IString> {
        self.name
    }

    #[must_use]
    pub fn sig(&self) -> Interned<FnSig> {
        self.sig
    }

    #[must_use]
    pub fn kind(&self) -> MethodKind {
        self.kind
    }

    #[must_use]
    pub fn generics(&self) -> &[Type] {
        &self.generics
    }
    /// Returns the inputs of this methods, excluding this for constructors.
    pub fn stack_inputs<'s, 'asm: 's>(&'s self, asm: &'asm Assembly) -> &'s [Type] {
        let sig = &asm[self.sig];
        match self.kind() {
            MethodKind::Static => sig.inputs(),
            MethodKind::Instance => sig.inputs(),
            MethodKind::Virtual => sig.inputs(),
            MethodKind::Constructor => &sig.inputs()[1..],
        }
    }
    /// Returns the output of this method.
    pub fn output(&self, asm: &Assembly) -> Type {
        let sig = &asm[self.sig];
        match self.kind() {
            MethodKind::Static => *sig.output(),
            MethodKind::Instance => *sig.output(),
            MethodKind::Virtual => *sig.output(),
            MethodKind::Constructor => Type::ClassRef(self.class()),
        }
    }

    pub fn aligned_alloc(asm: &mut crate::Assembly) -> MethodRef {
        let void_ptr = asm.nptr(Type::Void);
        let sig = asm.sig([Type::Int(Int::USize), Type::Int(Int::USize)], void_ptr);
        MethodRef::new(
            ClassRef::native_mem(asm),
            asm.alloc_string("AlignedAlloc"),
            sig,
            MethodKind::Static,
            vec![].into(),
        )
    }
    pub fn alloc(asm: &mut crate::Assembly) -> MethodRef {
        let sig = asm.sig([Type::Int(Int::ISize)], Type::Int(Int::ISize));
        MethodRef::new(
            ClassRef::marshal(asm),
            asm.alloc_string("AllocHGlobal"),
            sig,
            MethodKind::Static,
            vec![].into(),
        )
    }

    pub fn aligned_free(asm: &mut Assembly) -> Interned<MethodRef> {
        let void_ptr = asm.nptr(Type::Void);
        let sig = asm.sig([void_ptr], Type::Void);
        let aligned_free = asm.alloc_string("AlignedFree");
        let native_mem = ClassRef::native_mem(asm);
        asm.alloc_methodref(MethodRef::new(
            native_mem,
            aligned_free,
            sig,
            MethodKind::Static,
            [].into(),
        ))
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct MethodDef {
    access: Access,
    class: ClassDefIdx,
    name: Interned<IString>,
    sig: Interned<FnSig>,
    arg_names: Vec<Option<Interned<IString>>>,
    kind: MethodKind,
    implementation: MethodImpl,
    /// An explicit ECMA-335 `.override` (§II.15.4.2.3, a `MethodImpl` metadata row) naming the
    /// *base class* virtual method this one overrides — distinct from ordinary `MethodKind::
    /// Virtual` implicit binding (used for `implements=` interface satisfaction, which CLR binds
    /// by name+signature alone with no explicit override needed). Overriding a base class's
    /// virtual must land in that base's exact vtable slot, which implicit binding can't guarantee
    /// (a name/signature mismatch would silently create a NEW slot — a shadow method — instead of
    /// an override). `None` for every method that existed before this field: this is additive,
    /// no existing caller sets it. See `MethodDef::with_override`'s doc for the scope this was
    /// built for (a single well-known-base-virtual spike, not general base-class wrapping).
    overrides: Option<Interned<MethodRef>>,
    /// Marks this method as an ECMA-335 abstract member (§II.15.4.2.2: `Abstract` flag, RVA=0, no
    /// body) — used for synthesizing a real C# `interface` from a Rust trait (see `ClassDef::
    /// with_interface`'s doc). Distinct from every existing `MethodImpl` variant: `Missing` looks
    /// like a candidate but is NOT one — it gives a real body that throws at runtime, so reusing it
    /// for an abstract slot would silently produce a concrete throwing method instead of a genuine
    /// abstract member (a miscompilation class, not a clean failure). Kept as a plain bool flag
    /// (like `overrides`) rather than a new `MethodImpl` variant so this doesn't force an exhaustive
    /// match update across the ~30 `MethodImpl` match sites outside `il_exporter` that don't need to
    /// know about it yet — `implementation` is still set to `MethodImpl::Missing` as an unused
    /// placeholder body for an abstract method (never read: `il_exporter` checks `is_abstract()`
    /// before ever looking at `implementation`). `false` for every method that existed before this
    /// field: additive, no existing caller sets it.
    is_abstract: bool,
    /// 1-based `Param` **Sequence** numbers (§II.22.33 — the receiver-stripped, caller-visible
    /// argument positions) whose `Param` row gets `ParamAttributes.Out` (0x0002, §II.23.1.13).
    /// Combined with an `ELEMENT_TYPE_BYREF` parameter type in the signature blob this is exactly
    /// what makes C# surface the parameter as `out T` instead of `ref T` (a BYREF param with
    /// `Flags == 0` reads back as `ref`; csc sets no other bit and no modreq for `out`). Used by
    /// `#[dotnet_interface]`'s `#[dotnet_out]` parameter marker; empty for every method that
    /// existed before this field (additive — like `overrides`/`is_abstract`, `new()` never sets
    /// it, only `with_out_params`). NOTE: changing this struct changes the serialized (postcard)
    /// `.bc` IR format — rebuild dylib + linker together and `cargo clean` consumers (the
    /// build-std fingerprint trap).
    out_params: Vec<u16>,
    /// The DECLARED generic type-parameter names of a generic method DEFINITION (`T Echo<T>(T
    /// value)` on an interface, from `#[dotnet_interface]`'s `fn Echo<T>(&self, value: T) -> T`),
    /// in declaration order — arity = `len()`. Non-empty means the PE writer stamps the method's
    /// signature blob with `SIG_GENERIC` (0x10) + a compressed `GenParamCount` (§II.23.2.1) and
    /// emits one method-owned `GenericParam` row (§II.22.20, coded `TypeOrMethodDef` owner tag 1)
    /// per name; the signature's `Type::PlatformGeneric(N, GenericKind::CallGeneric)` markers
    /// (`ELEMENT_TYPE_MVAR`, `!!N`) must all satisfy `N < len()` (asserted in `export.rs` Pass 3).
    /// The METHOD-definition analogue of `ClassDef::generic_names` (a generic TYPE definition's
    /// parameter names) — and a different axis from `MethodRef::generics`, which is a CALL SITE's
    /// concrete instantiation arguments (`MethodSpec`). Empty for every method that existed
    /// before this field (additive — like `overrides`/`out_params`, `new()` never sets it, only
    /// `with_generic_params`). NOTE: changing this struct changes the serialized (postcard) `.bc`
    /// IR format — rebuild dylib + linker together and `cargo clean` consumers (the build-std
    /// fingerprint trap).
    generic_params: Vec<Interned<IString>>,
    /// Marks this method as an ECMA-335 `SpecialName` (§II.23.1.10, 0x0800) member OUTSIDE the
    /// event/property-accessor cases (those are detected by identity against the owning
    /// `ClassDef`'s `EventDef`/`PropertyDef` lists instead — see `il_exporter`'s
    /// `is_event_accessor`/`is_property_accessor` — so they don't need this flag). Used for CLR
    /// operator-overload methods (`op_Addition`, `op_Equality`, …): Roslyn requires `SpecialName`
    /// on these for `+`/`==`/etc. syntax to bind to them — without it the method is only callable
    /// by its literal name (`X.op_Addition(a, b)`), never via the operator. `false` for every
    /// method that existed before this field: additive, `new()` never sets it, only
    /// `with_special_name`. NOTE: changing this struct changes the serialized (postcard) `.bc` IR
    /// format — rebuild dylib + linker together and `cargo clean` consumers (the build-std
    /// fingerprint trap).
    is_special_name: bool,
}

impl RelocateValue for MethodDef {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self {
            access,
            class,
            name,
            sig,
            arg_names,
            kind,
            implementation,
            overrides,
            is_abstract,
            out_params,
            generic_params,
            is_special_name,
        } = self;
        Self {
            access,
            class: class.relocate(ctx, destination),
            name: ctx.string(destination, name),
            sig: ctx.signature(destination, sig),
            arg_names: arg_names
                .into_iter()
                .map(|name| name.map(|name| ctx.string(destination, name)))
                .collect(),
            kind,
            implementation: implementation.relocate(ctx, destination),
            overrides: overrides.map(|method| ctx.method_ref(destination, method)),
            is_abstract,
            out_params,
            generic_params: generic_params
                .into_iter()
                .map(|name| ctx.string(destination, name))
                .collect(),
            is_special_name,
        }
    }
}

impl MethodDef {
    pub fn iter_types<'a, 'asm: 'a>(
        &'a self,
        asm: &'asm Assembly,
    ) -> impl Iterator<Item = Type> + 'a {
        let defining_class = Type::ClassRef(*self.class());
        let sig = &asm[self.sig()];
        let sig_types = sig.iter_types();
        let local_types = self.iter_locals(asm).map(|(_, tpe)| asm[*tpe]);
        let body_types = self
            .iter_cil(asm)
            .into_iter()
            .map(|cil| cil.iter_types(asm));
        let body_types = body_types.flatten();
        std::iter::once(defining_class)
            .chain(sig_types)
            .chain(local_types)
            .chain(body_types)
    }
    #[must_use]
    pub fn iter_cil<'asm: 'method, 'method>(
        &'method self,
        asm: &'asm Assembly,
    ) -> Option<Box<dyn Iterator<Item = CILIterElem> + 'method>> {
        match self.resolved_implementation(asm) {
            MethodImpl::MethodBody { blocks, .. } => Some(Box::new(
                blocks
                    .iter()
                    .flat_map(super::basic_block::BasicBlock::iter_roots)
                    .flat_map(|root| super::CILIter::new(asm.get_root(root).clone(), asm)),
            )),
            MethodImpl::RegionBody {
                blocks,
                cleanup_blocks,
                ..
            } => Some(Box::new(
                blocks
                    .iter()
                    .chain(cleanup_blocks)
                    .flat_map(super::basic_block::BasicBlock::iter_roots)
                    .flat_map(|root| super::CILIter::new(asm.get_root(root).clone(), asm)),
            )),
            MethodImpl::Extern { .. } => None,
            MethodImpl::AliasFor(_) => {
                panic!("Unresolved alias returned by MethodDef::resolved_implementation")
            }
            MethodImpl::Missing => None,
        }
    }
    #[must_use]
    pub fn ref_to(&self) -> MethodRef {
        MethodRef::new(
            *self.class(),
            self.name(),
            self.sig(),
            self.kind(),
            vec![].into(),
        )
    }
    #[must_use]
    pub fn new(
        access: Access,
        class: ClassDefIdx,
        name: Interned<IString>,
        sig: Interned<FnSig>,
        kind: MethodKind,
        implementation: MethodImpl,
        arg_names: Vec<Option<Interned<IString>>>,
    ) -> Self {
        Self {
            access,
            class,
            name,
            sig,
            arg_names,
            kind,
            implementation,
            overrides: None,
            is_abstract: false,
            out_params: vec![],
            generic_params: vec![],
            is_special_name: false,
        }
    }

    /// Marks this method as an explicit ECMA-335 `.override` of `base` — the *base class's*
    /// virtual method this one overrides (not an interface member; use `implements=`/`ClassDef::
    /// add_interface` for that, which needs no explicit override). `self.kind()` must already be
    /// `MethodKind::Virtual` for this to mean anything to an exporter; this method doesn't
    /// enforce that itself (a narrow builder, not a validated state machine) — callers are
    /// expected to already be emitting a virtual method.
    ///
    /// Scoped intentionally narrow: proven end-to-end for exactly one well-known, parameterless,
    /// unsealed base virtual (`System.Object.ToString()`, see `cargo_tests/cd_override`). Real
    /// base-class wrapping (a framework type with a non-trivial constructor, protected members,
    /// `sealed` methods the CLR would reject at load time) is a larger, separate problem — see
    /// `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md`'s Tier C finding #1 for the full scope discussion.
    #[must_use]
    pub fn with_override(mut self, base: Interned<MethodRef>) -> Self {
        self.overrides = Some(base);
        self
    }

    #[must_use]
    pub fn overrides(&self) -> Option<Interned<MethodRef>> {
        self.overrides
    }

    /// Marks this method as an ECMA-335 abstract member with no body (§II.15.4.2.2) — the shape a
    /// real C# `interface` member (or an abstract base-class member) needs. See the `is_abstract`
    /// field's doc for why this is a bool flag rather than a new `MethodImpl` variant. Scoped
    /// intentionally narrow, matching `ClassDef::with_interface`'s scope: proven for a single
    /// no-argument interface member via `cargo_tests/cd_interface_export`, IL-exporter-only.
    #[must_use]
    pub fn with_abstract(mut self) -> Self {
        self.is_abstract = true;
        self
    }

    #[must_use]
    pub fn is_abstract(&self) -> bool {
        self.is_abstract
    }

    /// Marks this method `SpecialName` (§II.23.1.10) — see the `is_special_name` field's doc.
    /// Used for CLR operator-overload methods (`op_Addition`, …), which Roslyn requires this on
    /// to bind `+`/`==`/etc. syntax to them.
    #[must_use]
    pub fn with_special_name(mut self) -> Self {
        self.is_special_name = true;
        self
    }

    #[must_use]
    pub fn is_special_name(&self) -> bool {
        self.is_special_name
    }

    /// Marks the given 1-based, receiver-stripped parameter Sequence numbers as `[out]`
    /// (`ParamAttributes.Out`, §II.23.1.13) — see the `out_params` field's doc. The caller is
    /// responsible for each named position's signature type being `Type::Ref` (BYREF): `Out` on a
    /// non-BYREF param is metadata C# cannot consume as `out` (the comptime layer validates this
    /// before calling — `src/comptime.rs`'s abstract-member loop).
    #[must_use]
    pub fn with_out_params(mut self, out_params: Vec<u16>) -> Self {
        self.out_params = out_params;
        self
    }

    /// The 1-based, receiver-stripped parameter Sequence numbers flagged `[out]` — empty for
    /// almost every method (see the `out_params` field's doc).
    #[must_use]
    pub fn out_params(&self) -> &[u16] {
        &self.out_params
    }

    /// Declares this method as a generic method DEFINITION with the given ordered type-parameter
    /// names (see the `generic_params` field's doc). The caller is responsible for every
    /// `Type::PlatformGeneric(N, GenericKind::CallGeneric)` (`!!N`) marker in the signature
    /// satisfying `N < names.len()` — an out-of-range `ELEMENT_TYPE_MVAR`, or a `GenParamCount`
    /// that disagrees with the emitted `GenericParam` row count, is exactly the malformed shape
    /// CoreCLR's type loader rejects at load time (`export.rs` Pass 3 asserts this loudly at
    /// export instead).
    #[must_use]
    pub fn with_generic_params(mut self, names: Vec<Interned<IString>>) -> Self {
        self.generic_params = names;
        self
    }

    /// The declared generic type-parameter names of a generic method definition, in declaration
    /// order — empty for every non-generic method (see the `generic_params` field's doc).
    #[must_use]
    pub fn generic_params(&self) -> &[Interned<IString>] {
        &self.generic_params
    }

    #[must_use]
    pub fn class(&self) -> ClassDefIdx {
        self.class
    }

    #[must_use]
    pub fn name(&self) -> Interned<IString> {
        self.name
    }

    #[must_use]
    pub fn sig(&self) -> Interned<FnSig> {
        self.sig
    }

    #[must_use]
    pub fn kind(&self) -> MethodKind {
        self.kind
    }

    #[must_use]
    pub fn implementation(&self) -> &MethodImpl {
        &self.implementation
    }
    #[must_use]
    pub fn resolved_implementation<'asm: 'method, 'method>(
        &'method self,
        asm: &'asm Assembly,
    ) -> &'method MethodImpl {
        match self.implementation {
            MethodImpl::MethodBody { .. } | MethodImpl::RegionBody { .. }
            | MethodImpl::Extern { .. } | MethodImpl::Missing => &self.implementation,
            MethodImpl::AliasFor(method) => asm
                .method_def_from_ref(method)
                .expect("ERROR: a method is an alias for an extern function")
                .resolved_implementation(asm),
        }
    }
    pub fn implementation_mut(&mut self) -> &mut MethodImpl {
        &mut self.implementation
    }


    /// Builds a `MethodDef` directly from already-lowered, interned `BasicBlock`s. Performs
    /// debug-name uniquing on argument/local names and argument-count reconciliation against the
    /// signature.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn from_blocks(
        access: Access,
        class: ClassDefIdx,
        name: &str,
        sig: Interned<FnSig>,
        kind: MethodKind,
        blocks: Vec<BasicBlock>,
        mut locals: Vec<LocalDef>,
        mut arg_names: Vec<Option<Interned<IString>>>,
        asm: &mut Assembly,
    ) -> Self {
        // Debug-name uniquing, identical to `Method::new`.
        let mut used_names = FxHashSet::default();
        for name in arg_names
            .iter_mut()
            .chain(locals.iter_mut().map(|loc| &mut loc.0))
            .flatten()
        {
            let mut postfix = 0;
            while used_names.contains(&if postfix == 0 {
                *name
            } else {
                let new_name = format!("{name}{postfix}", name = &asm[*name]);
                asm.alloc_string(new_name)
            }) {
                postfix += 1;
            }
            if postfix != 0 {
                let new_name = format!("{name}{postfix}", name = &asm[*name]);
                *name = asm.alloc_string(new_name);
            }
            used_names.insert(*name);
        }
        let name = asm.alloc_string(name);
        let implementation = MethodImpl::MethodBody { blocks, locals };
        // Argument-count reconciliation against the signature.
        let arg_debug_count = arg_names.len();
        let arg_sig_count = asm[sig].inputs().len();
        match arg_debug_count.cmp(&arg_sig_count) {
            std::cmp::Ordering::Less => {
                arg_names.extend((arg_debug_count..arg_sig_count).map(|_| None));
            }
            std::cmp::Ordering::Equal => (),
            std::cmp::Ordering::Greater => {
                arg_names.truncate(arg_sig_count);
            }
        }
        assert_eq!(arg_names.len(), asm[sig].inputs().len());
        MethodDef::new(access, class, name, sig, kind, implementation, arg_names)
    }

    /// Builds a canonical exception-region body while reusing `from_blocks`' debug-name and
    /// argument reconciliation logic.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn from_region_blocks(
        access: Access,
        class: ClassDefIdx,
        name: &str,
        sig: Interned<FnSig>,
        kind: MethodKind,
        blocks: Vec<BasicBlock>,
        cleanup_blocks: Vec<BasicBlock>,
        exception_regions: Vec<ExceptionRegion>,
        locals: Vec<LocalDef>,
        arg_names: Vec<Option<Interned<IString>>>,
        asm: &mut Assembly,
    ) -> Self {
        // Cleanup blocks have no ordinary CFG predecessor; without a protected-region edge they
        // are unreachable. Preserve the compact legacy body for the overwhelmingly common
        // no-unwind method and avoid exporter-time cloning of every such method.
        if exception_regions.is_empty() {
            return Self::from_blocks(
                access, class, name, sig, kind, blocks, locals, arg_names, asm,
            );
        }
        let mut method = Self::from_blocks(
            access, class, name, sig, kind, blocks, locals, arg_names, asm,
        );
        let MethodImpl::MethodBody { blocks, locals } =
            std::mem::replace(method.implementation_mut(), MethodImpl::Missing)
        else {
            unreachable!()
        };
        *method.implementation_mut() = MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            exception_regions,
            locals,
        };
        method
    }

    #[must_use]
    pub fn access(&self) -> &Access {
        &self.access
    }

    #[must_use]
    pub fn arg_names(&self) -> &[Option<Interned<IString>>] {
        &self.arg_names
    }

    pub(crate) fn iter_locals<'a>(
        &'a self,
        asm: &'a Assembly,
    ) -> impl Iterator<Item = &'a (Option<Interned<IString>>, Interned<Type>)> {
        match self.resolved_implementation(asm) {
            MethodImpl::MethodBody { blocks: _, locals }
            | MethodImpl::RegionBody { locals, .. } => locals.iter(),
            MethodImpl::Extern { .. } | MethodImpl::Missing => [].iter(),
            MethodImpl::AliasFor(_) => panic!(),
        }
    }

    /// Sets the accesibility of this method to `access`.
    pub fn set_access(&mut self, access: Access) {
        self.access = access;
    }

    pub fn stack_inputs(&self, asm: &mut Assembly) -> Vec<(Type, Option<Interned<IString>>)> {
        let mut arg_names = self.arg_names().to_vec();
        let sig = asm[self.sig()].clone();
        arg_names.extend((arg_names.len()..(sig.inputs().len())).map(|_| None));
        sig.inputs()
            .iter()
            .copied()
            .zip(arg_names.iter().copied())
            .collect::<Vec<_>>()
    }

    pub fn blocks<'s, 'asm: 's>(&'s self, asm: &'asm Assembly) -> Option<&'s [BasicBlock]> {
        self.resolved_implementation(asm)
            .blocks()
            .map(|vec| vec.as_ref())
    }
    pub fn adjust_alignment(&mut self, asm: &mut Assembly, guaranteed_align: u8) {
        let Some((blocks, mut cleanup_blocks, locals)) = self.implementation_mut().body_parts_mut() else {
            return;
        };
        assert!(!blocks.is_empty());
        let to_map: Vec<_> = locals
            .iter()
            .map(|(name, tpe)| (*name, *tpe, asm.alignof_type(*tpe)))
            .enumerate()
            .collect();
        // Check which locals get their address taken.
        let mut local_address_of = vec![false; locals.len()];
        for node in blocks
            .iter()
            .chain(
                cleanup_blocks
                    .as_deref()
                    .into_iter()
                    .flat_map(|blocks| blocks.iter()),
            )
            .flat_map(super::basic_block::BasicBlock::iter_roots)
            .flat_map(|root| super::CILIter::new(asm.get_root(root).clone(), asm))
            .filter_map(super::iter::CILIterElem::as_node)
        {
            if let CILNode::LdLocA(loc) = node {
                local_address_of[loc as usize] = true
            }
        }
        let mut preamble = vec![];
        for (local_id, (_, tpe_idx, align)) in to_map {
            if align <= u64::from(guaranteed_align) {
                // Aligement guanrateed by .NET, skip.
                continue;
            }
            // Check that the address of this local is ever taken. If not, just ignore it.
            if !local_address_of[local_id] {
                continue;
            }
            // Change the type of the local var.
            let tpe_ptr = asm.nptr(tpe_idx);
            let tpe_ptr = asm.alloc_type(tpe_ptr);
            locals[local_id].1 = tpe_ptr;
            // Allocate a new buffer for the local var, aligned to align.
            let tpe = asm[tpe_idx];
            let local_buff = asm.alloc_node(CILNode::LocAllocAlgined {
                tpe: tpe_idx,
                align,
            });
            preamble.push(asm.alloc_root(CILRoot::StLoc(local_id as u32, local_buff)));
            // Map all usages of this local, to ensure it is propely alligned.
            blocks
                .iter_mut()
                .chain(
                    cleanup_blocks
                        .as_deref_mut()
                        .into_iter()
                        .flat_map(|blocks| blocks.iter_mut()),
                )
                .flat_map(|block| block.roots_mut())
                .for_each(|root_idx| {
                    let root = asm[*root_idx].clone();
                    let local_addr = asm.alloc_node(CILNode::LdLoc(local_id as u32));
                    let root = root.map(
                        asm,
                        &mut |root, _| match root {
                            CILRoot::StLoc(loc, val) if loc == local_id as u32 => {
                                CILRoot::StInd(Box::new((local_addr, val, tpe, false)))
                            }
                            _ => root,
                        },
                        &mut |node, _| match node {
                            CILNode::LdLocA(loc) if loc == local_id as u32 => {
                                CILNode::LdLoc(local_id as u32)
                            }
                            CILNode::LdLoc(loc) if loc == local_id as u32 => CILNode::LdInd {
                                addr: local_addr,
                                tpe: tpe_idx,
                                volatile: false,
                            },
                            _ => node,
                        },
                    );
                    *root_idx = asm.alloc_root(root);
                });
        }
        preamble.extend(blocks[0].roots().iter().copied());
        *blocks[0].roots_mut() = preamble;
    }

    pub(crate) fn remove_dead_blocks(&mut self, asm: &Assembly) {
        if let MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            exception_regions,
            ..
        } = self.implementation_mut()
        {
            if blocks.is_empty() {
                exception_regions.clear();
                cleanup_blocks.clear();
                return;
            }

            // Preserve the legacy normal-CFG policy, then make the method-level unwind edges
            // explicit: regions protecting removed normal blocks are removed, and the canonical
            // cleanup graph is retained only when reachable from a remaining handler entry.
            let mut alive_normal: FxHashSet<_> =
                blocks.iter().flat_map(|block| block.targets(asm)).collect();
            alive_normal.insert(blocks[0].block_id());
            blocks.retain(|block| alive_normal.contains(&block.block_id()));
            exception_regions.retain(|region| alive_normal.contains(&region.protected()));

            let cleanup_ids: FxHashSet<_> =
                cleanup_blocks.iter().map(BasicBlock::block_id).collect();
            let mut alive_cleanup = FxHashSet::default();
            let mut pending: Vec<_> = exception_regions
                .iter()
                .map(|region| region.handler_entry())
                .collect();
            while let Some(block_id) = pending.pop() {
                assert!(
                    cleanup_ids.contains(&block_id),
                    "exception region references missing cleanup block {block_id}"
                );
                if !alive_cleanup.insert(block_id) {
                    continue;
                }
                let block = cleanup_blocks
                    .iter()
                    .find(|block| block.block_id() == block_id)
                    .expect("cleanup id set and cleanup block vector disagree");
                for (target, sub_target) in block.targets_with_sub(asm) {
                    assert_eq!(
                        sub_target, 0,
                        "canonical cleanup blocks cannot contain handler subtargets"
                    );
                    assert!(
                        cleanup_ids.contains(&target),
                        "cleanup block {block_id} branches outside the cleanup graph to {target}"
                    );
                    if !alive_cleanup.contains(&target) {
                        pending.push(target);
                    }
                }
            }
            cleanup_blocks.retain(|block| alive_cleanup.contains(&block.block_id()));
            return;
        }

        // This opt only makes sense if this method has an impl
        let Some(blocks) = self.implementation().blocks() else {
            return;
        };
        // Check if the entry block does not jump anywhere(no targets) and has no handler - if so, only keep it.
        if blocks[0].targets(asm).count() == 0 && blocks[0].handler().is_none() {
            let entry = blocks[0].clone();
            *self.implementation_mut().blocks_mut().unwrap() = vec![entry];
            return;
        }
        let mut alive: FxHashSet<_> = blocks.iter().flat_map(|block| block.targets(asm)).collect();
        // entry block is always live
        alive.insert(blocks[0].block_id());
        // if alive < total, then there are some dead blocks, then remove them.
        if alive.len() >= blocks.len() {
            return;
        }
        // If handlers jump to normal blocks, do not GC.
        if blocks
            .iter()
            .flat_map(|block| block.handler())
            .flatten()
            .flat_map(|block| block.roots())
            .any(|root| {
                matches!(
                    asm[*root],
                    CILRoot::ExitSpecialRegion {
                        target: _,
                        source: _
                    }
                )
            })
        {
            return;
        }
        //let blocks_copy = blocks.clone();
        self.implementation_mut()
            .blocks_mut()
            .unwrap()
            .retain(|block| alive.contains(&block.block_id()));
    }

    pub(crate) fn locals(&self) -> Option<&[LocalDef]> {
        match self.implementation() {
            MethodImpl::MethodBody { locals, .. } | MethodImpl::RegionBody { locals, .. } => {
                Some(locals)
            }
            _ => None,
        }
    }

    pub fn accesses_statics(&self, asm: &Assembly) -> bool {
        let Some(mut cil) = self.iter_cil(asm) else {
            return false;
        };
        cil.any(|node| {
            matches!(
                node,
                CILIterElem::Node(CILNode::LdStaticField(_) | CILNode::LdStaticFieldAddress(_))
                    | CILIterElem::Root(CILRoot::SetStaticField { .. })
            )
        })
    }
}
pub type LocalDef = (Option<Interned<IString>>, Interned<Type>);
/// A method-scope catch-all unwind association. `protected` identifies a normal CFG block and
/// `handler_entry` identifies the entry block in `MethodImpl::RegionBody::cleanup_blocks`.
///
/// The staged representation deliberately keeps associations singleton. Exporters materialize
/// the historical one-try-per-block shape today; a later lexical-region planner may coalesce
/// compatible associations without changing this canonical cleanup graph.
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ExceptionRegion {
    protected: BlockId,
    handler_entry: BlockId,
}

impl ExceptionRegion {
    #[must_use]
    pub const fn new(protected: BlockId, handler_entry: BlockId) -> Self {
        Self {
            protected,
            handler_entry,
        }
    }

    #[must_use]
    pub const fn protected(self) -> BlockId {
        self.protected
    }

    #[must_use]
    pub const fn handler_entry(self) -> BlockId {
        self.handler_entry
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub enum MethodImpl {
    MethodBody {
        blocks: Vec<BasicBlock>,
        locals: Vec<LocalDef>,
    },
    Extern {
        lib: Interned<IString>,
        preserve_errno: bool,
    },
    AliasFor(Interned<MethodRef>),
    Missing,
    /// Canonical method body with cleanup CFG storage shared by every protected block that refers
    /// to it. Appended after all legacy variants so prefixless legacy postcard assemblies retain
    /// their historical enum discriminants during the schema-v3 transition.
    RegionBody {
        blocks: Vec<BasicBlock>,
        cleanup_blocks: Vec<BasicBlock>,
        exception_regions: Vec<ExceptionRegion>,
        locals: Vec<LocalDef>,
    },
}
impl RelocateValue for MethodImpl {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        match self {
            Self::MethodBody { blocks, locals } => Self::MethodBody {
                blocks: blocks
                    .into_iter()
                    .map(|block| block.relocate(ctx, destination))
                    .collect(),
                locals: locals
                    .into_iter()
                    .map(|(name, tpe)| {
                        (
                            name.map(|name| ctx.string(destination, name)),
                            ctx.type_id(destination, tpe),
                        )
                    })
                    .collect(),
            },
            Self::RegionBody {
                blocks,
                cleanup_blocks,
                exception_regions,
                locals,
            } => Self::RegionBody {
                blocks: blocks
                    .into_iter()
                    .map(|block| block.relocate(ctx, destination))
                    .collect(),
                cleanup_blocks: cleanup_blocks
                    .into_iter()
                    .map(|block| block.relocate(ctx, destination))
                    .collect(),
                exception_regions,
                locals: locals
                    .into_iter()
                    .map(|(name, tpe)| {
                        (
                            name.map(|name| ctx.string(destination, name)),
                            ctx.type_id(destination, tpe),
                        )
                    })
                    .collect(),
            },
            Self::Extern {
                lib,
                preserve_errno,
            } => Self::Extern {
                lib: ctx.string(destination, lib),
                preserve_errno,
            },
            Self::AliasFor(method) => Self::AliasFor(ctx.method_ref(destination, method)),
            Self::Missing => Self::Missing,
        }
    }
}
impl MethodImpl {
    pub(crate) fn all_blocks_mut(
        &mut self,
    ) -> Option<Box<dyn Iterator<Item = &mut BasicBlock> + '_>> {
        match self {
            Self::MethodBody { blocks, .. } => Some(Box::new(blocks.iter_mut())),
            Self::RegionBody {
                blocks,
                cleanup_blocks,
                ..
            } => Some(Box::new(blocks.iter_mut().chain(cleanup_blocks))),
            _ => None,
        }
    }

    pub(crate) fn body_parts_mut(
        &mut self,
    ) -> Option<(
        &mut Vec<BasicBlock>,
        Option<&mut Vec<BasicBlock>>,
        &mut Vec<LocalDef>,
    )> {
        match self {
            Self::MethodBody { blocks, locals } => Some((blocks, None, locals)),
            Self::RegionBody {
                blocks,
                cleanup_blocks,
                locals,
                ..
            } => Some((blocks, Some(cleanup_blocks), locals)),
            _ => None,
        }
    }

    pub fn root_count(&self) -> usize {
        match self {
            MethodImpl::MethodBody { blocks, .. } => {
                blocks.iter().map(|block| block.roots().len()).sum()
            }
            MethodImpl::RegionBody {
                blocks,
                cleanup_blocks,
                ..
            } => blocks
                .iter()
                .chain(cleanup_blocks)
                .map(|block| block.roots().len())
                .sum(),
            MethodImpl::Extern { .. } => 0,
            MethodImpl::AliasFor(_) => 0,
            MethodImpl::Missing => 3,
        }
    }
    pub fn blocks_mut(&mut self) -> Option<&mut Vec<BasicBlock>> {
        match self {
            Self::MethodBody { blocks, .. } | Self::RegionBody { blocks, .. } => Some(blocks),
            _ => None,
        }
    }
    pub fn blocks(&self) -> Option<&Vec<BasicBlock>> {
        match self {
            Self::MethodBody { blocks, .. } | Self::RegionBody { blocks, .. } => Some(blocks),
            _ => None,
        }
    }

    /// Returns canonical cleanup blocks for a region body. Legacy bodies embed handlers in their
    /// normal blocks and therefore return `None`.
    #[must_use]
    pub fn cleanup_blocks(&self) -> Option<&[BasicBlock]> {
        match self {
            Self::RegionBody { cleanup_blocks, .. } => Some(cleanup_blocks),
            _ => None,
        }
    }

    #[must_use]
    pub fn exception_regions(&self) -> Option<&[ExceptionRegion]> {
        match self {
            Self::RegionBody {
                exception_regions, ..
            } => Some(exception_regions),
            _ => None,
        }
    }

    /// Produces the exact legacy per-block handler representation used by all current exporters.
    /// The canonical body is never mutated; only this scratch clone receives jumpstarters,
    /// `ExitSpecialRegion` pads, source-specific branches, and cloned reachable cleanup blocks.
    #[must_use]
    pub fn materialize_legacy_body(
        &self,
        asm: &mut Assembly,
    ) -> Option<(Vec<BasicBlock>, Vec<LocalDef>)> {
        match self {
            Self::MethodBody { blocks, locals } => Some((blocks.clone(), locals.clone())),
            Self::RegionBody {
                blocks,
                cleanup_blocks,
                exception_regions,
                locals,
            } => {
                let mut by_protected = FxHashMap::default();
                for region in exception_regions {
                    assert!(
                        by_protected
                            .insert(region.protected(), region.handler_entry())
                            .is_none(),
                        "normal block {} has more than one exception region",
                        region.protected()
                    );
                }
                let mut materialized = blocks.clone();
                for block in &mut materialized {
                    if let Some(handler_entry) = by_protected.remove(&block.block_id()) {
                        block.resolve_exception_handler(handler_entry, cleanup_blocks, asm);
                    }
                }
                assert!(
                    by_protected.is_empty(),
                    "exception region protects a missing normal block"
                );
                Some((materialized, locals.clone()))
            }
            Self::Extern { .. } | Self::AliasFor(_) | Self::Missing => None,
        }
    }

    pub(crate) fn verify_exception_regions(&self, asm: &Assembly) -> Result<(), String> {
        let Self::RegionBody {
            blocks,
            cleanup_blocks,
            exception_regions,
            ..
        } = self
        else {
            return Ok(());
        };

        let normal_ids: FxHashSet<_> = blocks.iter().map(BasicBlock::block_id).collect();
        let cleanup_ids: FxHashSet<_> = cleanup_blocks.iter().map(BasicBlock::block_id).collect();
        if normal_ids.len() != blocks.len() || cleanup_ids.len() != cleanup_blocks.len() {
            return Err("duplicate block id within a canonical normal or cleanup CFG".into());
        }
        if normal_ids.iter().any(|id| cleanup_ids.contains(id)) {
            return Err("normal and cleanup CFG block ids overlap".into());
        }

        let mut protected = FxHashSet::default();
        for region in exception_regions {
            if !normal_ids.contains(&region.protected()) {
                return Err(format!(
                    "exception region protects missing normal block {}",
                    region.protected()
                ));
            }
            if !cleanup_ids.contains(&region.handler_entry()) {
                return Err(format!(
                    "exception region references missing cleanup entry {}",
                    region.handler_entry()
                ));
            }
            if !protected.insert(region.protected()) {
                return Err(format!(
                    "normal block {} has multiple exception regions",
                    region.protected()
                ));
            }
        }

        for (is_cleanup, block, ids) in blocks
            .iter()
            .map(|block| (false, block, &normal_ids))
            .chain(
                cleanup_blocks
                    .iter()
                    .map(|block| (true, block, &cleanup_ids)),
            )
        {
            if block.handler().is_some() || block.handler_id().is_some() {
                return Err(format!(
                    "canonical {} block {} contains legacy handler state",
                    if is_cleanup { "cleanup" } else { "normal" },
                    block.block_id()
                ));
            }
            for (target, sub_target) in block.targets_with_sub(asm) {
                if sub_target != 0 {
                    return Err(format!(
                        "canonical block {} contains legacy handler subtarget {sub_target}",
                        block.block_id()
                    ));
                }
                if !ids.contains(&target) {
                    return Err(format!(
                        "canonical {} block {} branches across the CFG partition to {target}",
                        if is_cleanup { "cleanup" } else { "normal" },
                        block.block_id()
                    ));
                }
            }
            if block
                .roots()
                .iter()
                .any(|root| matches!(asm.get_root(*root), CILRoot::ExitSpecialRegion { .. }))
            {
                return Err(format!(
                    "canonical block {} contains legacy ExitSpecialRegion",
                    block.block_id()
                ));
            }
        }
        Ok(())
    }

    /// Returns `true` if the method impl is [`Extern`].
    ///
    /// [`Extern`]: MethodImpl::Extern
    #[must_use]
    pub fn is_extern(&self) -> bool {
        matches!(self, Self::Extern { .. })
    }

    /// Shared `AggressiveInlining` (§II.23.1.11, `0x100`) hint heuristic, used identically by both
    /// `il_exporter` (as a `.method ... aggressiveinlining` keyword) and `pe_exporter` (as the
    /// `MethodDefRow.impl_flags` bit) so the two exporters never again drift out of parity on this
    /// (see `pe_exporter/pdb.rs`'s module doc, "Phase-0 probe" gap (a), for the history: `pe_exporter`
    /// silently never wrote this bit until that gap was closed).
    ///
    /// Originally scoped to single-block, handler-free, <=24-root bodies (small straight-line
    /// leaves — monomorphized closure/iterator-adapter wrappers). Widened (fractal-rs perf
    /// investigation, 2026-07) to also cover small, branchy-but-loop-free, call-free leaves like
    /// `cilly::ir::builtins::casts`'s saturating float->int cast helpers (4 blocks: a NaN check
    /// plus overflow/underflow clamps, no internal calls, ~7 roots total) — confirmed empirically
    /// that hinting a 4-block leaf like this DOES get RyuJIT to inline it (a standalone repro
    /// showed `fcvtzu` emitted inline at every call site, replacing what was otherwise 3 `blr`
    /// indirect calls per escaping pixel in the real kernel). The call-free requirement keeps this
    /// conservative: it never asks RyuJIT to inline a body that itself contains a call (no
    /// unbounded call-graph expansion risk), and the block/root caps bound JIT/codegen cost. Pure
    /// JIT hint — cannot affect correctness (verified: no typecheck/codegen semantics change).
    #[must_use]
    pub fn should_hint_aggressive_inline(&self, asm: &Assembly) -> bool {
        let Self::MethodBody { blocks, .. } = self else {
            return false;
        };
        if blocks.is_empty() || blocks.len() > 8 {
            return false;
        }
        let total_roots: usize = blocks.iter().map(|b| b.roots().len()).sum();
        if total_roots > 24 {
            return false;
        }
        if blocks.iter().any(|b| b.handler().is_some()) {
            return false;
        }
        // No block may (transitively) contain a Call/CallVirt/CallI — keeps this conservative (no
        // call-graph expansion risk) and matches the single-block case, which (being a leaf as a
        // consequence of the old scoping) never had internal calls either.
        !blocks.iter().any(|block| {
            block.roots().iter().any(|root| {
                super::CILIter::new(asm.get_root(*root).clone(), asm).any(|elem| {
                    matches!(
                        elem,
                        CILIterElem::Node(CILNode::Call(_) | CILNode::CallI(_))
                            | CILIterElem::Root(CILRoot::Call(_))
                    )
                })
            })
        })
    }
    // While this function is a bit long, this is not an issue.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn merge_cctor_impls(&mut self, implementation: &MethodImpl, asm: &Assembly) {
        let tmp = match (&self, &implementation) {
            (
                MethodImpl::MethodBody { blocks, locals },
                MethodImpl::MethodBody {
                    blocks: other_blocks,
                    locals: other_locals,
                },
            ) => {
                assert_eq!(locals, other_locals);
                let mut blocks = blocks.clone();
                // First, check that 1st blocks end in `VoidRet`, and remove it.
                let last_root = if blocks.is_empty() {
                    blocks.push(BasicBlock::new(vec![], 0, None));
                    CILRoot::VoidRet
                } else {
                    blocks
                        .last_mut()
                        .unwrap()
                        .roots_mut()
                        .pop()
                        .map_or(CILRoot::VoidRet, |root_id| asm.get_root(root_id).clone())
                };
                assert_eq!(last_root, CILRoot::VoidRet);
                assert_eq!(
                    other_blocks.len(),
                    1,
                    "Only merging one block method impls is currently supported"
                );
                blocks
                    .last_mut()
                    .unwrap()
                    .roots_mut()
                    .extend(other_blocks.last().unwrap().roots());
                MethodImpl::MethodBody {
                    blocks,
                    locals: locals.clone(),
                }
            }
            (MethodImpl::MethodBody { .. }, MethodImpl::Extern { .. }) => {
                panic!("Unmergable method impl: Can't merge MethodBody with Extern.")
            }
            (MethodImpl::MethodBody { .. }, MethodImpl::AliasFor(_)) => {
                panic!("Unmergable method impl: Can't merge MethodBody with AliasFor.")
            }

            (MethodImpl::Extern { .. }, MethodImpl::MethodBody { .. }) => {
                panic!("Unmergable method impl: Can't merge Extern with MethodBody.")
            }
            (
                MethodImpl::Extern {
                    lib,
                    preserve_errno,
                },
                MethodImpl::Extern {
                    lib: liba,
                    preserve_errno: preserve_errnoa,
                },
            ) => {
                assert_eq!(lib, liba);
                assert_eq!(preserve_errno, preserve_errnoa);
                self.clone()
            }
            (MethodImpl::Extern { .. }, MethodImpl::AliasFor(_)) => {
                panic!("Unmergable method impl: Can't merge Extern with AliasFor.")
            }
            (
                MethodImpl::Extern {
                    lib,
                    preserve_errno,
                },
                MethodImpl::Missing,
            )
            | (
                MethodImpl::Missing,
                MethodImpl::Extern {
                    lib,
                    preserve_errno,
                },
            ) => MethodImpl::Extern {
                lib: *lib,
                preserve_errno: *preserve_errno,
            },
            (
                MethodImpl::AliasFor(_),
                MethodImpl::MethodBody { .. } | MethodImpl::Extern { .. },
            ) => {
                panic!("Unmergable method impl: can't merge alias.")
            }
            (MethodImpl::AliasFor(a), MethodImpl::AliasFor(b)) => {
                assert_eq!(a, b);
                self.clone()
            }
            (MethodImpl::AliasFor(alias), MethodImpl::Missing)
            | (MethodImpl::Missing, MethodImpl::AliasFor(alias)) => MethodImpl::AliasFor(*alias),
            (MethodImpl::Missing, MethodImpl::MethodBody { blocks, locals })
            | (MethodImpl::MethodBody { blocks, locals }, MethodImpl::Missing) => {
                MethodImpl::MethodBody {
                    blocks: blocks.clone(),
                    locals: locals.clone(),
                }
            }

            (MethodImpl::Missing, MethodImpl::Missing) => MethodImpl::Missing,
            (MethodImpl::Missing, MethodImpl::RegionBody { .. }) => (*implementation).clone(),
            (MethodImpl::RegionBody { .. }, MethodImpl::Missing) => (*self).clone(),
            (MethodImpl::RegionBody { .. }, _) | (_, MethodImpl::RegionBody { .. }) => {
                panic!("Unmergable method impl: canonical exception-region bodies cannot be merged")
            }
        };
        *self = tmp;
    }

    pub(crate) fn realloc_locals(&mut self, asm: &mut Assembly) {
        // Optimization only suported for methods with locals
        let Some((blocks, mut cleanup_blocks, locals)) = self.body_parts_mut() else {
            return;
        };
        let mut new_locals = std::sync::Mutex::new(Vec::new());
        let local_map = std::sync::Mutex::new(FxHashMap::default());
        for block in blocks.iter_mut().chain(
            cleanup_blocks
                .as_deref_mut()
                .into_iter()
                .flat_map(|blocks| blocks.iter_mut()),
        ) {
            block.map_roots(
                asm,
                &mut |root, _| match root {
                    CILRoot::StLoc(loc, tree) => CILRoot::StLoc(
                        match local_map.lock().unwrap().entry(loc) {
                            std::collections::hash_map::Entry::Occupied(val) => *val.get(),
                            std::collections::hash_map::Entry::Vacant(empty) => {
                                let mut new_locals = new_locals.lock().unwrap();
                                let new_idx = new_locals.len();
                                new_locals.push(locals[loc as usize]);
                                *empty.insert(new_idx as u32)
                            }
                        },
                        tree,
                    ),
                    _ => root,
                },
                &mut |node, _| match node {
                    CILNode::LdLoc(loc) => {
                        CILNode::LdLoc(match local_map.lock().unwrap().entry(loc) {
                            std::collections::hash_map::Entry::Occupied(val) => *val.get(),
                            std::collections::hash_map::Entry::Vacant(empty) => {
                                let mut new_locals = new_locals.lock().unwrap();
                                let new_idx = new_locals.len();
                                new_locals.push(locals[loc as usize]);
                                *empty.insert(new_idx as u32)
                            }
                        })
                    }
                    CILNode::LdLocA(loc) => {
                        CILNode::LdLocA(match local_map.lock().unwrap().entry(loc) {
                            std::collections::hash_map::Entry::Occupied(val) => *val.get(),
                            std::collections::hash_map::Entry::Vacant(empty) => {
                                let mut new_locals = new_locals.lock().unwrap();
                                let new_idx = new_locals.len();
                                new_locals.push(locals[loc as usize]);
                                *empty.insert(new_idx as u32)
                            }
                        })
                    }
                    _ => node,
                },
            );
        }
        // Swap new and locals
        std::mem::swap(locals, new_locals.get_mut().unwrap());
    }

    pub(crate) fn wrapper(
        alloc: Interned<MethodRef>,
        mref: &MethodRef,
        asm: &mut Assembly,
    ) -> MethodImpl {
        let sig = asm[asm[alloc].sig()].clone();
        let args = sig
            .inputs()
            .iter()
            .enumerate()
            .map(|(idx, _)| asm.alloc_node(CILNode::LdArg(idx.try_into().unwrap())))
            .collect();
        let roots = if asm.sizeof_type(*sig.output()) == 0 {
            let call = asm.alloc_root(CILRoot::Call(Box::new((alloc, args, IsPure::NOT))));
            vec![call, asm.alloc_root(CILRoot::VoidRet)]
        } else {
            let val = asm.alloc_node(CILNode::Call(Box::new((alloc, args, IsPure::NOT))));
            if asm[mref.sig()].output() != sig.output() {
                match (asm[mref.sig()].output(), sig.output()) {
                    (Type::Ptr(a), Type::Ptr(_)) => {
                        let val =
                            asm.alloc_node(CILNode::PtrCast(val, Box::new(PtrCastRes::Ptr(*a))));
                        vec![asm.alloc_root(CILRoot::Ret(val))]
                    }
                    _ => todo!(),
                }
            } else {
                vec![asm.alloc_root(CILRoot::Ret(val))]
            }
        };
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals: vec![].into(),
        }
    }
}
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MethodDefIdx(pub Interned<MethodRef>);
impl RelocateValue for MethodDefIdx {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self(method) = self;
        Self(ctx.method_ref(destination, method))
    }
}
impl MethodDefIdx {
    pub(crate) fn from_raw(method: Interned<MethodRef>) -> MethodDefIdx {
        Self(method)
    }
}

impl std::ops::Deref for MethodDefIdx {
    type Target = Interned<MethodRef>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Interned<MethodRef> {
    pub fn abort(asm: &mut Assembly) -> Interned<MethodRef> {
        let main = asm.main_module();
        let sig = asm.sig([], Type::Void);
        asm.new_methodref(*main, "abort", sig, MethodKind::Static, vec![])
    }
}
#[test]
fn locals() {
    fn method(locals: &[LocalDef], asm: &mut Assembly) -> MethodDef {
        let name: Interned<IString> = asm.alloc_string("DoSomething");
        let mimpl = MethodImpl::MethodBody {
            blocks: vec![],
            locals: locals.into(),
        };
        let main_module = asm.main_module();
        let sig = asm.sig([], Type::Void);
        MethodDef::new(
            Access::Extern,
            main_module,
            name,
            sig,
            MethodKind::Static,
            mimpl,
            vec![],
        )
    }
    let mut asm = Assembly::default();
    assert_eq!(method(&[], &mut asm).iter_locals(&asm).count(), 0);
    let tpe = asm.alloc_type(Type::Bool);
    let tpe2 = asm.alloc_type(Type::Bool);
    assert_eq!(
        method(&[(None, tpe)], &mut asm).iter_locals(&asm).count(),
        1
    );
    assert_eq!(
        method(&[(None, tpe), (None, tpe2)], &mut asm)
            .iter_locals(&asm)
            .cloned()
            .collect::<Vec<(Option<Interned<IString>>, _)>>(),
        vec![(None, tpe), (None, tpe2)]
    );
    let mut method = method(&[(None, tpe), (None, tpe2)], &mut asm);
    assert_eq!(
        method
            .iter_locals(&asm)
            .cloned()
            .collect::<Vec<(Option<Interned<IString>>, _)>>(),
        vec![(None, tpe), (None, tpe2)]
    );
    method.implementation.realloc_locals(&mut asm);
    assert_eq!(method.iter_locals(&asm).count(), 0);
}
#[test]
fn test_extern() {
    assert!(!MethodImpl::MethodBody {
        blocks: vec![],
        locals: vec![],
    }
    .is_extern());
    let mut asm = Assembly::default();
    let name: Interned<IString> = asm.alloc_string("libsomething.so");
    assert!(MethodImpl::Extern {
        lib: name,
        preserve_errno: false,
    }
    .is_extern())
}
#[test]
fn cil() {
    fn method(roots: &[Interned<CILRoot>], asm: &mut Assembly) -> MethodDef {
        let name: Interned<IString> = asm.alloc_string("DoSomething");
        let mimpl = MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots.to_vec(), 0, None)],
            locals: vec![],
        };
        let main_module = asm.main_module();
        let sig = asm.sig([], Type::Void);
        MethodDef::new(
            Access::Extern,
            main_module,
            name,
            sig,
            MethodKind::Static,
            mimpl,
            vec![],
        )
    }
    let mut asm = Assembly::default();
    assert_eq!(
        method(&[], &mut asm)
            .iter_cil(&asm)
            .map(|iter| iter.count()),
        Some(0)
    );
    let void_ret = asm.alloc_root(CILRoot::VoidRet);
    assert_eq!(
        method(&[void_ret], &mut asm)
            .iter_cil(&asm)
            .map(|iter| iter.collect::<Vec<_>>()),
        Some(vec![CILIterElem::Root(CILRoot::VoidRet)])
    );
    let const0 = asm.alloc_node(crate::Const::I32(0));
    let const0_ret = asm.alloc_root(CILRoot::Ret(const0));
    assert_eq!(
        method(&[const0_ret], &mut asm)
            .iter_cil(&asm)
            .map(|iter| iter.collect::<Vec<_>>()),
        Some(vec![
            CILIterElem::Root(CILRoot::Ret(const0)),
            CILIterElem::Node(crate::Const::I32(0).into()),
        ])
    );
    let name: Interned<IString> = asm.alloc_string("DoSomething");
    let main_module = asm.main_module();
    let sig = asm.sig([], Type::Void);
    assert_eq!(
        MethodDef::new(
            Access::Extern,
            main_module,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::Extern {
                lib: name,
                preserve_errno: false,
            },
            vec![],
        )
        .iter_cil(&asm)
        .map(|iter| iter.count()),
        None,
    );
    assert_eq!(
        MethodDef::new(
            Access::Extern,
            main_module,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![],
        )
        .iter_cil(&asm)
        .map(|iter| iter.count()),
        None,
    );
}
/// Regression test for the `pe_exporter`/`il_exporter` `AggressiveInlining` parity gap +
/// widening (fractal-rs perf investigation, 2026-07 — see `should_hint_aggressive_inline`'s doc).
/// A small, branchy-but-loop-free, call-free multi-block leaf — the exact shape
/// `cilly::ir::builtins::casts`'s saturating float->int cast helpers produce (a NaN-check block
/// plus 3 single-`Ret` blocks) — must be hinted even though it is NOT single-block.
#[test]
fn should_hint_aggressive_inline_true_for_a_small_multi_block_call_free_leaf() {
    let mut asm = Assembly::default();
    let zero = asm.alloc_node(crate::Const::I32(0));
    let one = asm.alloc_node(crate::Const::I32(1));
    let ret0 = asm.alloc_root(CILRoot::Ret(zero));
    let ret1 = asm.alloc_root(CILRoot::Ret(one));
    // 4 blocks total (mirrors casts.rs's `float_to_int` generator shape), no calls anywhere,
    // well under the root/block caps.
    let blocks = vec![
        BasicBlock::new(vec![ret0], 0, None),
        BasicBlock::new(vec![ret1], 1, None),
        BasicBlock::new(vec![ret0], 2, None),
        BasicBlock::new(vec![ret1], 3, None),
    ];
    let mimpl = MethodImpl::MethodBody {
        blocks,
        locals: vec![],
    };
    assert!(
        mimpl.should_hint_aggressive_inline(&asm),
        "a small, call-free, handler-free, loop-free multi-block leaf must be hinted"
    );
}
/// Negative case: a body containing a `Call` anywhere must NOT be hinted — keeps the widened
/// heuristic conservative (no call-graph-expansion risk from asking RyuJIT to inline a body that
/// itself calls out).
#[test]
fn should_hint_aggressive_inline_false_when_the_body_contains_a_call() {
    let mut asm = Assembly::default();
    let main_module = asm.main_module();
    let void_sig = asm.sig([], Type::Void);
    let callee_name = asm.alloc_string("SomeCallee");
    let mref = MethodRef::new(
        *main_module,
        callee_name,
        void_sig,
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    let call_root = asm.alloc_root(CILRoot::Call(Box::new((mref, vec![].into(), IsPure::NOT))));
    let void_ret = asm.alloc_root(CILRoot::VoidRet);
    let blocks = vec![BasicBlock::new(vec![call_root, void_ret], 0, None)];
    let mimpl = MethodImpl::MethodBody {
        blocks,
        locals: vec![],
    };
    assert!(
        !mimpl.should_hint_aggressive_inline(&asm),
        "a body containing a Call must never be hinted, regardless of size"
    );
}
/// Negative case: a body with a handler (try/catch) must NOT be hinted, matching the original
/// single-block heuristic's handler-free requirement.
#[test]
fn should_hint_aggressive_inline_false_when_a_block_has_a_handler() {
    let mut asm = Assembly::default();
    let void_ret = asm.alloc_root(CILRoot::VoidRet);
    let handler_block = BasicBlock::new(vec![void_ret], 1, None);
    let blocks = vec![BasicBlock::new(
        vec![void_ret],
        0,
        Some(vec![handler_block]),
    )];
    let mimpl = MethodImpl::MethodBody {
        blocks,
        locals: vec![],
    };
    assert!(
        !mimpl.should_hint_aggressive_inline(&asm),
        "a block with a handler must never be hinted"
    );
}
/// Negative case: too many blocks (beyond the small-leaf cap) must NOT be hinted, bounding
/// JIT/codegen cost the same way the original 24-root cap did for the single-block case.
#[test]
fn should_hint_aggressive_inline_false_when_too_many_blocks() {
    let mut asm = Assembly::default();
    let void_ret = asm.alloc_root(CILRoot::VoidRet);
    let blocks: Vec<_> = (0..9)
        .map(|id| BasicBlock::new(vec![void_ret], id, None))
        .collect();
    let mimpl = MethodImpl::MethodBody {
        blocks,
        locals: vec![],
    };
    assert!(
        !mimpl.should_hint_aggressive_inline(&asm),
        "a leaf with more than the small-body block cap must not be hinted"
    );
}
/// Non-`MethodBody` implementations (`Extern`/`AliasFor`/`Missing`) must never be hinted — there
/// is no IL body to attach the hint to.
#[test]
fn should_hint_aggressive_inline_false_for_non_method_body_impls() {
    let mut asm = Assembly::default();
    let lib_name = asm.alloc_string("libsomething.so");
    assert!(!MethodImpl::Extern {
        lib: lib_name,
        preserve_errno: false,
    }
    .should_hint_aggressive_inline(&asm));
    assert!(!MethodImpl::Missing.should_hint_aggressive_inline(&asm));
}

#[cfg(test)]
fn region_body_fixture(asm: &mut Assembly) -> MethodImpl {
    let normal_root = asm.alloc_root(CILRoot::Nop);
    let cleanup_root = asm.alloc_root(CILRoot::ReThrow);
    MethodImpl::RegionBody {
        blocks: vec![BasicBlock::new(vec![normal_root], 0, None)],
        cleanup_blocks: vec![BasicBlock::new(vec![cleanup_root], 10, None)],
        exception_regions: vec![ExceptionRegion::new(0, 10)],
        locals: vec![],
    }
}

#[test]
fn region_builder_keeps_methods_without_protected_regions_compact() {
    let mut asm = Assembly::default();
    let class = asm.main_module();
    let sig = asm.alloc_sig(FnSig::new([], Type::Void));
    let ret = asm.alloc_root(CILRoot::VoidRet);
    let method = MethodDef::from_region_blocks(
        Access::Extern,
        class,
        "no_regions",
        sig,
        MethodKind::Static,
        vec![BasicBlock::new(vec![ret], 0, None)],
        vec![BasicBlock::new(vec![], 10, None)],
        vec![],
        vec![],
        vec![],
        &mut asm,
    );
    assert!(matches!(
        method.implementation(),
        MethodImpl::MethodBody { .. }
    ));
}

#[test]
fn canonical_region_body_materializes_exactly_like_legacy_resolution() {
    let mut asm = Assembly::default();
    let leave_normal = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));
    let cleanup_next = asm.alloc_root(CILRoot::Branch(Box::new((11, 0, None))));
    let rethrow = asm.alloc_root(CILRoot::ReThrow);
    let cleanup = vec![
        BasicBlock::new(vec![cleanup_next], 10, None),
        BasicBlock::new(vec![rethrow], 11, None),
    ];

    let mut legacy = BasicBlock::new_raw(vec![leave_normal], 0, Some(10));
    legacy.resolve_exception_handlers(&cleanup, &mut asm);

    let canonical = MethodImpl::RegionBody {
        blocks: vec![BasicBlock::new(vec![leave_normal], 0, None)],
        cleanup_blocks: cleanup,
        exception_regions: vec![ExceptionRegion::new(0, 10)],
        locals: vec![],
    };
    let (materialized, locals) = canonical.materialize_legacy_body(&mut asm).unwrap();
    assert!(locals.is_empty());
    assert_eq!(materialized, vec![legacy]);
}

#[test]
fn canonical_region_body_postcard_round_trip_and_enum_tags_are_stable() {
    let mut asm = Assembly::default();
    let region = region_body_fixture(&mut asm);
    let encoded = postcard::to_stdvec(&region).unwrap();
    assert_eq!(
        encoded[0], 4,
        "RegionBody must remain appended after legacy tags"
    );
    let decoded: MethodImpl = postcard::from_bytes(&encoded).unwrap();
    assert_eq!(decoded, region);

    let legacy = MethodImpl::MethodBody {
        blocks: vec![],
        locals: vec![],
    };
    assert_eq!(postcard::to_stdvec(&legacy).unwrap()[0], 0);
    let lib = asm.alloc_string("legacy-lib");
    assert_eq!(
        postcard::to_stdvec(&MethodImpl::Extern {
            lib,
            preserve_errno: false,
        })
        .unwrap()[0],
        1
    );
    let owner = asm.main_module();
    let sig = asm.sig([], Type::Void);
    let alias_name = asm.alloc_string("legacy-alias");
    let alias = asm.alloc_methodref(MethodRef::new(
        *owner,
        alias_name,
        sig,
        MethodKind::Static,
        vec![].into(),
    ));
    assert_eq!(
        postcard::to_stdvec(&MethodImpl::AliasFor(alias)).unwrap()[0],
        2
    );
    assert_eq!(postcard::to_stdvec(&MethodImpl::Missing).unwrap()[0], 3);
}

#[test]
fn canonical_region_roots_are_mapped_and_typechecked_once() {
    let mut asm = Assembly::default();
    let owner = asm.main_module();
    let sig = asm.sig([], Type::Void);
    let name = asm.alloc_string("region_visit_once");
    let mut method = MethodDef::new(
        Access::Public,
        owner,
        name,
        sig,
        MethodKind::Static,
        region_body_fixture(&mut asm),
        vec![],
    );
    let mut visits = 0;
    method.map_roots(
        &mut asm,
        &mut |root, _| {
            visits += 1;
            root
        },
        &mut |node, _| node,
    );
    assert_eq!(
        visits, 2,
        "normal and canonical cleanup roots are each visited once"
    );
    method.typecheck(&mut asm).unwrap();
}

#[test]
fn typechecker_rejects_an_invalid_canonical_cleanup_root() {
    let mut asm = Assembly::default();
    let owner = asm.main_module();
    let sig = asm.sig([], Type::Void);
    let name = asm.alloc_string("invalid_cleanup_root");
    let normal = asm.alloc_root(CILRoot::Nop);
    let wrong_value = asm.alloc_node(crate::Const::I32(7));
    let invalid_cleanup = asm.alloc_root(CILRoot::StLoc(0, wrong_value));
    let local_type = asm.alloc_type(Type::Float(crate::Float::F32));
    let mut method = MethodDef::new(
        Access::Public,
        owner,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::RegionBody {
            blocks: vec![BasicBlock::new(vec![normal], 0, None)],
            cleanup_blocks: vec![BasicBlock::new(vec![invalid_cleanup], 10, None)],
            exception_regions: vec![ExceptionRegion::new(0, 10)],
            locals: vec![(None, local_type)],
        },
        vec![],
    );
    assert!(method.typecheck(&mut asm).is_err());
}

#[test]
fn malformed_exception_region_is_rejected_by_export_verification() {
    let mut asm = Assembly::default();
    let owner = asm.main_module();
    let sig = asm.sig([], Type::Void);
    let name = asm.alloc_string("invalid_region");
    let mut body = region_body_fixture(&mut asm);
    let MethodImpl::RegionBody {
        exception_regions, ..
    } = &mut body
    else {
        unreachable!()
    };
    exception_regions.push(ExceptionRegion::new(0, 10));
    asm.new_method(MethodDef::new(
        Access::Public,
        owner,
        name,
        sig,
        MethodKind::Static,
        body,
        vec![],
    ));
    let error = asm
        .verify_for_export()
        .err()
        .expect("invalid region must fail");
    assert!(matches!(
        error.error,
        crate::ir::typecheck::TypeCheckError::InvalidExceptionRegion { .. }
    ));
}

#[test]
fn structural_region_verifier_rejects_each_noncanonical_shape() {
    let mut asm = Assembly::default();
    let handler_root = asm.alloc_root(CILRoot::ReThrow);

    let mut duplicate_ids = region_body_fixture(&mut asm);
    let MethodImpl::RegionBody { cleanup_blocks, .. } = &mut duplicate_ids else {
        unreachable!()
    };
    cleanup_blocks.push(BasicBlock::new(vec![handler_root], 10, None));
    assert!(duplicate_ids.verify_exception_regions(&asm).is_err());

    let mut duplicate_normal_ids = region_body_fixture(&mut asm);
    let MethodImpl::RegionBody { blocks, .. } = &mut duplicate_normal_ids else {
        unreachable!()
    };
    blocks.push(BasicBlock::new(vec![], 0, None));
    assert!(duplicate_normal_ids.verify_exception_regions(&asm).is_err());

    let mut overlapping_ids = region_body_fixture(&mut asm);
    let MethodImpl::RegionBody { cleanup_blocks, .. } = &mut overlapping_ids else {
        unreachable!()
    };
    cleanup_blocks[0] = BasicBlock::new(vec![handler_root], 0, None);
    assert!(overlapping_ids.verify_exception_regions(&asm).is_err());

    let mut missing_protected = region_body_fixture(&mut asm);
    let MethodImpl::RegionBody {
        exception_regions, ..
    } = &mut missing_protected
    else {
        unreachable!()
    };
    exception_regions[0] = ExceptionRegion::new(99, 10);
    assert!(missing_protected.verify_exception_regions(&asm).is_err());

    let mut missing_handler = region_body_fixture(&mut asm);
    let MethodImpl::RegionBody {
        exception_regions, ..
    } = &mut missing_handler
    else {
        unreachable!()
    };
    exception_regions[0] = ExceptionRegion::new(0, 99);
    assert!(missing_handler.verify_exception_regions(&asm).is_err());

    let mut embedded_handler = region_body_fixture(&mut asm);
    let MethodImpl::RegionBody { blocks, .. } = &mut embedded_handler else {
        unreachable!()
    };
    blocks[0] = BasicBlock::new(
        blocks[0].roots().to_vec(),
        0,
        Some(vec![BasicBlock::new(vec![handler_root], 10, None)]),
    );
    assert!(embedded_handler.verify_exception_regions(&asm).is_err());

    let mut invalid_subtarget = region_body_fixture(&mut asm);
    let branch = asm.alloc_root(CILRoot::Branch(Box::new((10, 11, None))));
    let MethodImpl::RegionBody { cleanup_blocks, .. } = &mut invalid_subtarget else {
        unreachable!()
    };
    cleanup_blocks[0] = BasicBlock::new(vec![branch], 10, None);
    assert!(invalid_subtarget.verify_exception_regions(&asm).is_err());

    let mut cross_partition = region_body_fixture(&mut asm);
    let branch = asm.alloc_root(CILRoot::Branch(Box::new((0, 0, None))));
    let MethodImpl::RegionBody { cleanup_blocks, .. } = &mut cross_partition else {
        unreachable!()
    };
    cleanup_blocks[0] = BasicBlock::new(vec![branch], 10, None);
    assert!(cross_partition.verify_exception_regions(&asm).is_err());
}
