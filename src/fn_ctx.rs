use cilly::Assembly;
use rustc_middle::ty::SymbolName;
use rustc_middle::ty::layout::HasTypingEnv;
use rustc_middle::ty::{Instance, PseudoCanonicalInput, TyCtxt};
use rustc_span::Span;
pub struct MethodCompileCtx<'tcx, 'asm> {
    tcx: TyCtxt<'tcx>,
    method: Option<&'tcx rustc_middle::mir::Body<'tcx>>,
    method_instance: Instance<'tcx>,
    asm: &'asm mut Assembly,
    span: Option<Span>,
}

// `Target` is `&'asm mut Assembly`, not `Assembly`: `asm` is itself a borrow with its own
// lifetime `'asm` outliving `self`, so deref'ing to `Assembly` directly would tie the result
// to `self`'s (shorter) lifetime. Going through the extra reference indirection lets `*ctx`
// reborrow `asm` for `'asm`, e.g. via `asm_mut`/`asm` below which return `&'a mut Assembly`
// detached from `&self`.
impl std::ops::DerefMut for MethodCompileCtx<'_, '_> {
    #[allow(clippy::mut_mut)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.asm
    }
}

impl<'asm> std::ops::Deref for MethodCompileCtx<'_, 'asm> {
    type Target = &'asm mut Assembly;

    fn deref(&self) -> &Self::Target {
        &self.asm
    }
}

impl<'tcx, 'asm> MethodCompileCtx<'tcx, 'asm> {
    pub fn set_span(&mut self, span: Span) {
        self.span = Some(span);
    }
    #[must_use]
    /// Creates a [`MethodCompileCtx`] with a certain MIR body.
    pub fn with_body<'a: 'asm>(&'a mut self, body: &'tcx rustc_middle::mir::Body<'tcx>) -> Self {
        assert!(
            self.method.is_none(),
            "ERROR: attempt to change the body of a method compilation context"
        );
        Self {
            tcx: self.tcx,
            method: Some(body),
            method_instance: self.method_instance,
            asm: self.asm,
            span: Some(body.span),
        }
    }
    pub fn new(
        tcx: TyCtxt<'tcx>,
        method: Option<&'tcx rustc_middle::mir::Body<'tcx>>,
        method_instance: Instance<'tcx>,
        asm: &'asm mut Assembly,
    ) -> Self {
        Self {
            tcx,
            method,
            method_instance,
            asm,
            span: None,
        }
    }
    pub fn span(&self) -> Span {
        self.span.unwrap_or(Span::default())
    }
    pub fn tcx_and_asm(&mut self) -> (TyCtxt<'tcx>, &mut Assembly) {
        (self.tcx, self.asm)
    }
    /// Returns the type context this method is compiled in.
    #[must_use]
    pub fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }
    /// Returns the MIR body of this method is compiled.
    #[must_use]
    pub fn body(&self) -> &'tcx rustc_middle::mir::Body<'tcx> {
        self.method.unwrap()
    }
    #[must_use]
    /// Returns the Instance representing the current method
    pub fn instance(&self) -> Instance<'tcx> {
        self.method_instance
    }
    pub fn monomorphize<T: rustc_middle::ty::TypeFoldable<TyCtxt<'tcx>> + Clone>(
        &self,
        ty: T,
    ) -> T {
        self.instance()
            .instantiate_mir_and_normalize_erasing_regions(
                self.tcx(),
                rustc_middle::ty::TypingEnv::fully_monomorphized(),
                rustc_middle::ty::EarlyBinder::bind(ty),
            )
    }

    #[must_use]
    pub fn layout_of(
        &self,
        ty: rustc_middle::ty::Ty<'tcx>,
    ) -> rustc_middle::ty::layout::TyAndLayout<'tcx> {
        let ty = self.monomorphize(ty);
        self.tcx
            .layout_of(PseudoCanonicalInput {
                typing_env: rustc_middle::ty::TypingEnv::fully_monomorphized(),
                value: ty,
            })
            .expect("Could not get type layout!")
    }

    pub fn asm_mut<'s: 'a, 'a>(&'s mut self) -> &'a mut Assembly {
        self.asm
    }
    #[must_use]
    pub fn asm<'s: 'a, 'a>(&'s self) -> &'a Assembly {
        self.asm
    }

    /// Alignment assumed for constant/static allocations embedded as raw byte buffers, not
    /// `tcx.data_layout()`'s real alignment. Kept at a conservative floor of 1 because the
    /// .NET side has no way to request over-aligned static data placement; anything requiring
    /// alignment above this value is routed by callers (see `alloc_ptr`/`alloc_ptr_unaligned`
    /// in crate::operand's constant.rs and static_data.rs) down an "unaligned" path
    /// with an explicit fixup instead of being embedded directly.
    pub fn const_align(&self) -> u64 {
        1
    }
}
impl<'tcx> rustc_middle::ty::layout::HasTyCtxt<'tcx> for MethodCompileCtx<'tcx, '_> {
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }
}
impl rustc_abi::HasDataLayout for MethodCompileCtx<'_, '_> {
    fn data_layout(&self) -> &rustc_abi::TargetDataLayout {
        self.tcx.data_layout()
    }
}
impl<'tcx> HasTypingEnv<'tcx> for MethodCompileCtx<'tcx, '_> {
    fn typing_env(&self) -> rustc_middle::ty::TypingEnv<'tcx> {
        rustc_middle::ty::TypingEnv::fully_monomorphized()
    }
}
/// Escapes the name of a function
pub fn fn_name(name: SymbolName) -> String {
    let name: String = name.to_string();
    /*// Name TOO long
    if *crate::config::ESCAPE_NAMES {
        name = name.replace('.', "_dot_").replace('$', "_ds_");
    }*/
    if name.len() > 1000 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        // TODO: make hashes consitant! `DefaultHasher::new()` uses fixed keys, so the hash
        // is deterministic within one rustc/std version, but that's not guaranteed across
        // toolchain versions — a symbol name truncated+hashed by one compiler could collide
        // or mismatch one hashed by another, breaking the linker's cross-crate name matching
        // (see `stable_adt_name`'s "pure function of identity" requirement).
        fn calculate_hash<T: Hash>(t: &T) -> u64 {
            let mut s = DefaultHasher::new();
            t.hash(&mut s);
            s.finish()
        }
        format!("{}_{}", &name[..1000], calculate_hash(&name))
    } else {
        name
    }
}
