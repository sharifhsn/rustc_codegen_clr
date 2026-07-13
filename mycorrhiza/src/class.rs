use crate::{
    FromManagedSafe, IntoManagedSafe, ManagedSafe,
    intrinsics::{
        RustcCLRInteropManagedClass, RustcCLRInteropManagedGeneric, RustcCLRInteropManagedStruct,
    },
};
type GCHandle = RustcCLRInteropManagedStruct<
    "System.Runtime",
    "System.Runtime.InteropServices.GCHandle",
    { size_of::<usize>() },
>;
pub struct Class<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str> {
    handle: GCHandle,
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str>
    IntoManagedSafe<RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>>
    for Class<ASSEMBLY, CLASS_PATH>
{
    fn into_managed(self) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH> {
        unsafe { self.get_naked_ref() }
    }
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str> Class<ASSEMBLY, CLASS_PATH> {
    pub type NakedRef = RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>;
    pub fn ctor0() -> Self {
        Self::from_naked_ref(Self::NakedRef::ctor0())
    }
    pub fn ctor1<Arg: ManagedSafe>(arg: impl IntoManagedSafe<Arg>) -> Self {
        Self::from_naked_ref(Self::NakedRef::ctor1(arg.into_managed()))
    }
    /// Returns the inner *naked* managed reference this handle keeps alive (via the `GCHandle`).
    ///
    /// This is the escape hatch for calling a .NET member `Class`'s own `instanceN`/`virtN` wrappers
    /// don't cover (e.g. a `static` method, an argument position, or an arity this module hasn't
    /// wired) — pass the returned naked reference straight to any [`crate::bindings`] call expecting
    /// `RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>`.
    ///
    /// It is genuinely inherent, not a gap that could be closed with a runtime check: unlike the
    /// `GCHandle`-backed `Class` itself (a value type with no gcref field, safe to hold across an
    /// `.await` — see `crate::task`'s module docs), the *naked reference* this returns is an ordinary
    /// managed reference again. If you store it (rather than using it transiently, e.g. as a call
    /// argument) inside a coroutine's saved state, `cilly::ir::class`'s `layout_check` will reject the
    /// generated `async fn` (a gcref can't live in an `async fn` state machine's overlapping variant
    /// storage) — but that check only fires at codegen time for a `.await`-spanning local, so a naked
    /// ref stashed anywhere *else* unsound (a `static`, a `Vec`, returned past the `GCHandle`'s
    /// lifetime) has no such backstop and is exactly the kind of misuse the type system cannot catch.
    /// That's why this stays `unsafe`: the caller must prove "used transiently, not persisted
    /// independently of the owning `Class`'s `GCHandle`" — a call-shape property, not a checkable type
    /// or runtime invariant. See the [`crate::StackOnly`] trait documentation for the general pattern.
    /// # Safety
    /// Use the returned reference only within the current call (e.g. pass it straight to another .NET
    /// call as an argument); do not store it anywhere that could outlive `self`'s `GCHandle` or that
    /// crosses an `.await` point.
    pub unsafe fn get_naked_ref(&self) -> RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH> {
        // `GCHandle.get_Target()` is declared `object Target { get; }` — its SigRet is always
        // `System.Object`, never the concrete handle type, regardless of what was `Alloc`'d. Read it
        // as `System.Object` first, then `castclass` down to the concrete handle (mirrors
        // `from_naked_ref`'s upcast in reverse). Calling `instance0` with `Self::NakedRef` directly
        // as the SigRet (the previous version of this function) emits a methodref whose signature
        // doesn't match the real `get_Target`, which the CLR rejects at JIT time with
        // `MissingMethodException`.
        let obj: RustcCLRInteropManagedClass<"System.Runtime", "System.Object"> =
            self.handle.instance0::<"get_Target", RustcCLRInteropManagedClass<"System.Runtime", "System.Object">>();
        crate::intrinsics::rustc_clr_interop_managed_checked_cast(obj)
    }
    pub fn from_naked_ref(naked: RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>) -> Self {
        let obj: RustcCLRInteropManagedClass<"System.Runtime", "System.Object"> =
            crate::intrinsics::rustc_clr_interop_managed_checked_cast(naked);
        let h = GCHandle::static1::<"Alloc", _, _>(obj);
        Self { handle: h }
    }
    pub fn instance0<
        const NAME: &'static str,
        SigRet: ManagedSafe,
        RealRet: FromManagedSafe<SigRet>,
    >(
        &mut self,
    ) -> RealRet {
        RealRet::from_managed(unsafe { self.get_naked_ref() }.instance0::<NAME, SigRet>())
    }
    pub fn virt0<
        const NAME: &'static str,
        SigRet: ManagedSafe,
        RealRet: FromManagedSafe<SigRet>,
    >(
        &mut self,
    ) -> RealRet {
        RealRet::from_managed(unsafe { self.get_naked_ref() }.virt0::<NAME, SigRet>())
    }
    pub fn instance1<
        const NAME: &'static str,
        Arg: ManagedSafe,
        SigRet: ManagedSafe,
        RealRet: FromManagedSafe<SigRet>,
    >(
        &mut self,
        arg: impl IntoManagedSafe<Arg>,
    ) -> RealRet {
        RealRet::from_managed(
            unsafe { self.get_naked_ref() }.instance1::<NAME, Arg, SigRet>(arg.into_managed()),
        )
    }
    pub fn instance2<
        const NAME: &'static str,
        Arg: ManagedSafe,
        Arg2: ManagedSafe,
        SigRet: ManagedSafe,
        RealRet: FromManagedSafe<SigRet>,
    >(
        &mut self,
        arg: impl IntoManagedSafe<Arg>,
        arg2: impl IntoManagedSafe<Arg2>,
    ) -> RealRet {
        RealRet::from_managed(
            unsafe { self.get_naked_ref() }
                .instance2::<NAME, Arg, Arg2, SigRet>(arg.into_managed(), arg2.into_managed()),
        )
    }
    //pub fn to_mstring(&self)->
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str> Drop
    for Class<ASSEMBLY, CLASS_PATH>
{
    fn drop(&mut self) {
        self.handle.instance0::<"Free", ()>()
    }
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str> Clone
    for Class<ASSEMBLY, CLASS_PATH>
{
    fn clone(&self) -> Self {
        Self::from_naked_ref(unsafe { self.get_naked_ref() })
    }
}

/// The generic-managed-type counterpart to [`Class`] -- wraps a
/// `RustcCLRInteropManagedGeneric<ASSEMBLY, CLASS_PATH, ClassGenerics>` (e.g. `Task<T>`, `List<T>`)
/// behind the same `GCHandle`-backed value-type indirection. `Class` only wraps the non-generic
/// [`RustcCLRInteropManagedClass`]; a generic instantiation's handle is a different Rust type
/// ([`RustcCLRInteropManagedGeneric`]) even though both are, at the CLR level, just an object
/// reference -- hence a separate (structurally identical) wrapper rather than a shared one. See
/// [`crate::task`]'s module docs for why this exists: a coroutine holding a generic managed handle
/// (e.g. `TaskT<T>`) directly across a suspend point trips `layout_check`'s
/// `ManagedRefInOverlapingField` the same way a non-generic handle does; wrapping it in a
/// `GenericClass` (a plain `GCHandle`/`IntPtr` value type, no gcref field) fixes it identically to
/// [`Class`].
pub struct GenericClass<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, ClassGenerics>
{
    handle: GCHandle,
    pd: core::marker::PhantomData<ClassGenerics>,
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, ClassGenerics>
    GenericClass<ASSEMBLY, CLASS_PATH, ClassGenerics>
{
    pub type NakedRef = RustcCLRInteropManagedGeneric<ASSEMBLY, CLASS_PATH, ClassGenerics>;

    /// Returns the inner *naked* managed reference this handle keeps alive (via the `GCHandle`).
    /// Same contract as [`Class::get_naked_ref`] -- use transiently only.
    /// # Safety
    /// Use the returned reference only within the current call (e.g. pass it straight to another .NET
    /// call as an argument); do not store it anywhere that could outlive `self`'s `GCHandle` or that
    /// crosses an `.await` point.
    pub unsafe fn get_naked_ref(&self) -> Self::NakedRef {
        let obj: RustcCLRInteropManagedClass<"System.Runtime", "System.Object"> =
            self.handle.instance0::<"get_Target", RustcCLRInteropManagedClass<"System.Runtime", "System.Object">>();
        crate::intrinsics::rustc_clr_interop_managed_checked_cast(obj)
    }

    pub fn from_naked_ref(naked: Self::NakedRef) -> Self {
        let obj: RustcCLRInteropManagedClass<"System.Runtime", "System.Object"> =
            crate::intrinsics::rustc_clr_interop_managed_checked_cast(naked);
        let h = GCHandle::static1::<"Alloc", _, _>(obj);
        Self {
            handle: h,
            pd: core::marker::PhantomData,
        }
    }
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, ClassGenerics> Drop
    for GenericClass<ASSEMBLY, CLASS_PATH, ClassGenerics>
{
    fn drop(&mut self) {
        self.handle.instance0::<"Free", ()>()
    }
}
impl<const ASSEMBLY: &'static str, const CLASS_PATH: &'static str, ClassGenerics> Clone
    for GenericClass<ASSEMBLY, CLASS_PATH, ClassGenerics>
{
    fn clone(&self) -> Self {
        Self::from_naked_ref(unsafe { self.get_naked_ref() })
    }
}
