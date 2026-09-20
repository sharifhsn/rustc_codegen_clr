use cilly::{Assembly, CILNode, Const, Interned, IntoAsmIndex, Type};
use rustc_abi::HasDataLayout;
use rustc_middle::ty::layout::HasTypingEnv;
use rustc_middle::ty::{Instance, PseudoCanonicalInput, TyCtxt};
use rustc_span::Span;
pub struct MethodCompileCtx<'tcx, 'asm> {
    tcx: TyCtxt<'tcx>,
    target_layout: crate::target_layout::TargetLayout,
    method: Option<&'tcx rustc_middle::mir::Body<'tcx>>,
    method_instance: Instance<'tcx>,
    asm: &'asm mut Assembly,
    span: Option<Span>,
    synthetic_local_next: u32,
    synthetic_local_end: u32,
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
            target_layout: self.target_layout,
            method: Some(body),
            method_instance: self.method_instance,
            asm: self.asm,
            span: Some(body.span),
            synthetic_local_next: 0,
            synthetic_local_end: 0,
        }
    }
    pub fn new(
        tcx: TyCtxt<'tcx>,
        method: Option<&'tcx rustc_middle::mir::Body<'tcx>>,
        method_instance: Instance<'tcx>,
        asm: &'asm mut Assembly,
    ) -> Self {
        let target_layout = crate::target_layout::TargetLayout::from_data_layout(tcx.data_layout())
            .expect("codegen_crate must validate the target layout before lowering methods");
        Self {
            tcx,
            target_layout,
            method,
            method_instance,
            asm,
            span: None,
            synthetic_local_next: 0,
            synthetic_local_end: 0,
        }
    }

    /// Installs a predeclared range of lowering-only locals.
    ///
    /// MIR locals are fixed before individual statements and terminators are lowered, while a few
    /// faithful lowerings need an evaluation-once temporary. The owner must append the matching
    /// local definitions before calling this method so per-root typechecking sees the same indices.
    pub fn reserve_synthetic_local_range(&mut self, start: u32, count: u32) {
        assert_eq!(
            self.synthetic_local_next, self.synthetic_local_end,
            "cannot replace a synthetic-local range while it is in use"
        );
        self.synthetic_local_next = start;
        self.synthetic_local_end = start
            .checked_add(count)
            .expect("synthetic local range exceeds u32");
    }

    /// Claims the next local from the range installed by [`Self::reserve_synthetic_local_range`].
    pub fn next_synthetic_local(&mut self) -> u32 {
        assert!(
            self.synthetic_local_next < self.synthetic_local_end,
            "lowering requested an undeclared synthetic local"
        );
        let local = self.synthetic_local_next;
        self.synthetic_local_next += 1;
        local
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
    /// Rust target facts. These deliberately do not come from the host running the backend.
    #[must_use]
    pub const fn target_layout(&self) -> crate::target_layout::TargetLayout {
        self.target_layout
    }
    /// Returns the MIR body of this method is compiled.
    #[must_use]
    pub fn body(&self) -> &'tcx rustc_middle::mir::Body<'tcx> {
        self.method.unwrap()
    }
    /// Returns the MIR body when this context lowers a real function. Synthetic static
    /// initializers and reification helpers intentionally have no body.
    #[must_use]
    pub const fn body_opt(&self) -> Option<&'tcx rustc_middle::mir::Body<'tcx>> {
        self.method
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
                rustc_middle::ty::EarlyBinder::bind(self.tcx(), ty),
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

    /// Emits Rust's semantic `size_of`, which can differ from the CLR storage size.
    ///
    /// Most lowered types use the CLR `sizeof` instruction directly. Rust enums/coroutines with
    /// managed-reference payloads are the exception: their GC-bearing fields are hoisted into
    /// non-overlapping CLR sidecar slots, growing physical storage while Rust's language-level size
    /// and pointer stride remain the rustc layout. Those types are registered on the assembly and
    /// become a native-width constant here before serialization.
    pub fn size_of(&mut self, tpe: impl IntoAsmIndex<Interned<Type>>) -> Interned<CILNode> {
        let tpe = tpe.into_idx(self.asm);
        let semantic_size = match self.asm[tpe] {
            Type::ClassRef(class) => self.asm.rust_semantic_size(class),
            _ => None,
        };
        if let Some(size) = semantic_size {
            self.asm.alloc_node(Const::USize(size))
        } else {
            self.asm.size_of(tpe)
        }
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
/// Returns one program-wide name for a Rust instance, independent of which crate instantiated it.
///
/// `TyCtxt::symbol_name` deliberately appends the *instantiating* crate to generic and
/// `GloballyShared { may_conflict: true }` copies. That is necessary for native object files, where
/// separate crates may emit private copies without comparing them. This backend links typed IR
/// shards and compares competing definitions structurally, so carrying that incidental suffix into
/// method references makes identical upstream instances look different transitively (for example,
/// through a function pointer embedded in a promoted allocation). Ask rustc's mangler for the same
/// complete `Instance` as if it were instantiated in its defining crate. Definition identity,
/// substitutions, shim kind, and explicit `no_mangle`/export names are all still encoded; only the
/// codegen-owner suffix is canonicalized. Any genuinely different duplicate body remains a hard
/// linker conflict.
pub fn fn_name_for_instance<'tcx>(tcx: TyCtxt<'tcx>, instance: Instance<'tcx>) -> String {
    shorten_symbol(rustc_symbol_mangling::symbol_name_for_instance_in_crate(
        tcx,
        instance,
        instance.def_id().krate,
    ))
}

fn shorten_symbol(name: String) -> String {
    const PREFIX_BYTES: usize = 1000;
    if name.len() <= PREFIX_BYTES {
        return name;
    }
    let mut prefix_end = PREFIX_BYTES;
    while !name.is_char_boundary(prefix_end) {
        prefix_end -= 1;
    }
    let digest = crate::stable_identity::digest_fields("long-rust-symbol", [name.as_bytes()]);
    format!("{}_{digest}", &name[..prefix_end])
}

#[cfg(test)]
mod tests {
    use super::shorten_symbol;

    #[test]
    fn long_symbol_identity_is_repeatable_and_uses_the_full_symbol() {
        let common = "a".repeat(1001);
        let first = shorten_symbol(format!("{common}x"));
        let repeated = shorten_symbol(format!("{common}x"));
        let different_tail = shorten_symbol(format!("{common}y"));
        assert_eq!(first, repeated);
        assert_ne!(first, different_tail);
        assert!(first.len() <= 1000 + 1 + 64);
    }

    #[test]
    fn truncation_respects_utf8_boundaries() {
        let symbol = format!("{}é", "a".repeat(999));
        let shortened = shorten_symbol(symbol);
        assert!(shortened.is_char_boundary(shortened.len()));
    }
}
