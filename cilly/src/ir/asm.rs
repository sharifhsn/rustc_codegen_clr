use super::{
    Access, CILNode, CILRoot, ClassDef, ClassRef, Const, Exporter, FieldDesc, FnSig, Int,
    IntoAsmIndex, MethodDef, MethodDefIdx, MethodRef, StaticFieldDesc, Type,
    bimap::{BiMap, BiMapIndex, Interned, IntoBiMapIndex},
    cilnode::{BinOp, ExtendKind, IsPure, MethodKind, PtrCastRes, UnOp},
    class::{ClassDefIdx, LayoutError, StaticFieldDef},
    iter::SemanticReachability,
    opt::{EffectInfoCache, OptFuel},
    typecheck::TypeCheckError,
};
use crate::{IString, config, utilis::assert_unique};
use crate::{MethodImpl, utilis::encode};
use fxhash::{FxHashMap, FxHashSet, hash64};

use serde::{Deserialize, Serialize};
use std::{any::type_name, ops::Index};

#[cfg(test)]
use super::class::{EventDef, FixedArrayLayout, PropertyDef};

pub type MissingMethodGenerator = Box<dyn Fn(Interned<MethodRef>, &mut Assembly) -> MethodImpl>;

/// A compiler-generated panic entry point whose body may be absent when cilly links against the
/// host toolchain's native (non-cilly) `core` artifact.
///
/// Keep this list exact and pinned-toolchain-shaped. In particular, classification must never use
/// a leaf-name or prefix match: user code is allowed to define functions with all of these leaf
/// names. A real cilly `core` definition always wins before this fallback is considered.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PanicKind {
    Format,
    NoUnwindFormat,
    Explicit,
    NoUnwind,
    NoUnwindNoBacktrace,
    PanicStr2015,
    ConstPanicFormat,
    BoundsCheck,
    MisalignedPointerDereference,
    NullPointerDereference,
    InvalidEnumConstruction,
    CannotUnwind,
    InCleanup,
    AddOverflow,
    SubOverflow,
    MulOverflow,
    DivOverflow,
    RemOverflow,
    NegOverflow,
    ShrOverflow,
    ShlOverflow,
    DivByZero,
    RemByZero,
    CoroutineResumed,
    AsyncFnResumed,
    AsyncGenFnResumed,
    GenFnNone,
    CoroutineResumedAfterPanic,
    AsyncFnResumedAfterPanic,
    AsyncGenFnResumedAfterPanic,
    GenFnNoneAfterPanic,
    CoroutineResumedAfterDrop,
    AsyncFnResumedAfterDrop,
    AsyncGenFnResumedAfterDrop,
    GenFnNoneAfterDrop,
}

impl PanicKind {
    const ALL: [Self; 35] = [
        Self::Format,
        Self::NoUnwindFormat,
        Self::Explicit,
        Self::NoUnwind,
        Self::NoUnwindNoBacktrace,
        Self::PanicStr2015,
        Self::ConstPanicFormat,
        Self::BoundsCheck,
        Self::MisalignedPointerDereference,
        Self::NullPointerDereference,
        Self::InvalidEnumConstruction,
        Self::CannotUnwind,
        Self::InCleanup,
        Self::AddOverflow,
        Self::SubOverflow,
        Self::MulOverflow,
        Self::DivOverflow,
        Self::RemOverflow,
        Self::NegOverflow,
        Self::ShrOverflow,
        Self::ShlOverflow,
        Self::DivByZero,
        Self::RemByZero,
        Self::CoroutineResumed,
        Self::AsyncFnResumed,
        Self::AsyncGenFnResumed,
        Self::GenFnNone,
        Self::CoroutineResumedAfterPanic,
        Self::AsyncFnResumedAfterPanic,
        Self::AsyncGenFnResumedAfterPanic,
        Self::GenFnNoneAfterPanic,
        Self::CoroutineResumedAfterDrop,
        Self::AsyncFnResumedAfterDrop,
        Self::AsyncGenFnResumedAfterDrop,
        Self::GenFnNoneAfterDrop,
    ];

    #[must_use]
    pub const fn canonical_symbol(self) -> &'static str {
        match self {
            Self::Format => "core::panicking::panic_fmt",
            Self::NoUnwindFormat => "core::panicking::panic_nounwind_fmt",
            Self::Explicit => "core::panicking::panic",
            Self::NoUnwind => "core::panicking::panic_nounwind",
            Self::NoUnwindNoBacktrace => "core::panicking::panic_nounwind_nobacktrace",
            Self::PanicStr2015 => "core::panicking::panic_str_2015",
            Self::ConstPanicFormat => "core::panicking::const_panic_fmt",
            Self::BoundsCheck => "core::panicking::panic_bounds_check",
            Self::MisalignedPointerDereference => {
                "core::panicking::panic_misaligned_pointer_dereference"
            }
            Self::NullPointerDereference => "core::panicking::panic_null_pointer_dereference",
            Self::InvalidEnumConstruction => "core::panicking::panic_invalid_enum_construction",
            Self::CannotUnwind => "core::panicking::panic_cannot_unwind",
            Self::InCleanup => "core::panicking::panic_in_cleanup",
            Self::AddOverflow => "core::panicking::panic_const::panic_const_add_overflow",
            Self::SubOverflow => "core::panicking::panic_const::panic_const_sub_overflow",
            Self::MulOverflow => "core::panicking::panic_const::panic_const_mul_overflow",
            Self::DivOverflow => "core::panicking::panic_const::panic_const_div_overflow",
            Self::RemOverflow => "core::panicking::panic_const::panic_const_rem_overflow",
            Self::NegOverflow => "core::panicking::panic_const::panic_const_neg_overflow",
            Self::ShrOverflow => "core::panicking::panic_const::panic_const_shr_overflow",
            Self::ShlOverflow => "core::panicking::panic_const::panic_const_shl_overflow",
            Self::DivByZero => "core::panicking::panic_const::panic_const_div_by_zero",
            Self::RemByZero => "core::panicking::panic_const::panic_const_rem_by_zero",
            Self::CoroutineResumed => "core::panicking::panic_const::panic_const_coroutine_resumed",
            Self::AsyncFnResumed => "core::panicking::panic_const::panic_const_async_fn_resumed",
            Self::AsyncGenFnResumed => {
                "core::panicking::panic_const::panic_const_async_gen_fn_resumed"
            }
            Self::GenFnNone => "core::panicking::panic_const::panic_const_gen_fn_none",
            Self::CoroutineResumedAfterPanic => {
                "core::panicking::panic_const::panic_const_coroutine_resumed_panic"
            }
            Self::AsyncFnResumedAfterPanic => {
                "core::panicking::panic_const::panic_const_async_fn_resumed_panic"
            }
            Self::AsyncGenFnResumedAfterPanic => {
                "core::panicking::panic_const::panic_const_async_gen_fn_resumed_panic"
            }
            Self::GenFnNoneAfterPanic => {
                "core::panicking::panic_const::panic_const_gen_fn_none_panic"
            }
            Self::CoroutineResumedAfterDrop => {
                "core::panicking::panic_const::panic_const_coroutine_resumed_drop"
            }
            Self::AsyncFnResumedAfterDrop => {
                "core::panicking::panic_const::panic_const_async_fn_resumed_drop"
            }
            Self::AsyncGenFnResumedAfterDrop => {
                "core::panicking::panic_const::panic_const_async_gen_fn_resumed_drop"
            }
            Self::GenFnNoneAfterDrop => {
                "core::panicking::panic_const::panic_const_gen_fn_none_drop"
            }
        }
    }

    #[must_use]
    const fn message(self) -> &'static str {
        match self {
            Self::Format => "formatted Rust panic",
            Self::NoUnwindFormat => "non-unwinding formatted Rust panic",
            Self::Explicit => "explicit Rust panic",
            Self::NoUnwind => "non-unwinding Rust panic",
            Self::NoUnwindNoBacktrace => "non-unwinding Rust panic (backtrace disabled)",
            Self::PanicStr2015 => "Rust 2015 panic",
            Self::ConstPanicFormat => "const-formatted Rust panic",
            Self::BoundsCheck => "index out of bounds",
            Self::MisalignedPointerDereference => "misaligned pointer dereference",
            Self::NullPointerDereference => "null pointer dereference occurred",
            Self::InvalidEnumConstruction => "trying to construct an enum from an invalid value",
            Self::CannotUnwind => "panic in a function that cannot unwind",
            Self::InCleanup => "panic in a destructor during cleanup",
            Self::AddOverflow => "attempt to add with overflow",
            Self::SubOverflow => "attempt to subtract with overflow",
            Self::MulOverflow => "attempt to multiply with overflow",
            Self::DivOverflow => "attempt to divide with overflow",
            Self::RemOverflow => "attempt to calculate the remainder with overflow",
            Self::NegOverflow => "attempt to negate with overflow",
            Self::ShrOverflow => "attempt to shift right with overflow",
            Self::ShlOverflow => "attempt to shift left with overflow",
            Self::DivByZero => "attempt to divide by zero",
            Self::RemByZero => "attempt to calculate the remainder with a divisor of zero",
            Self::CoroutineResumed => "coroutine resumed after completion",
            Self::AsyncFnResumed => "`async fn` resumed after completion",
            Self::AsyncGenFnResumed => "`async gen fn` resumed after completion",
            Self::GenFnNone => "`gen fn` should just keep returning `None` after completion",
            Self::CoroutineResumedAfterPanic => "coroutine resumed after panicking",
            Self::AsyncFnResumedAfterPanic => "`async fn` resumed after panicking",
            Self::AsyncGenFnResumedAfterPanic => "`async gen fn` resumed after panicking",
            Self::GenFnNoneAfterPanic => {
                "`gen fn` should just keep returning `None` after panicking"
            }
            Self::CoroutineResumedAfterDrop => "coroutine resumed after async drop",
            Self::AsyncFnResumedAfterDrop => "`async fn` resumed after async drop",
            Self::AsyncGenFnResumedAfterDrop => "`async gen fn` resumed after async drop",
            Self::GenFnNoneAfterDrop => "`gen fn` resumed after async drop",
        }
    }

    #[must_use]
    fn managed_exception(self, asm: &mut Assembly) -> Interned<ClassRef> {
        match self {
            Self::Format
            | Self::NoUnwindFormat
            | Self::Explicit
            | Self::NoUnwind
            | Self::NoUnwindNoBacktrace
            | Self::PanicStr2015
            | Self::ConstPanicFormat
            | Self::CannotUnwind
            | Self::InCleanup => ClassRef::invalid_operation_exception(asm),
            Self::BoundsCheck => ClassRef::index_out_of_range_exception(asm),
            Self::MisalignedPointerDereference => ClassRef::data_misaligned_exception(asm),
            Self::NullPointerDereference => ClassRef::null_reference_exception(asm),
            Self::InvalidEnumConstruction => ClassRef::invalid_cast_exception(asm),
            Self::AddOverflow
            | Self::SubOverflow
            | Self::MulOverflow
            | Self::DivOverflow
            | Self::RemOverflow
            | Self::NegOverflow
            | Self::ShrOverflow
            | Self::ShlOverflow => ClassRef::overflow_exception(asm),
            Self::DivByZero | Self::RemByZero => ClassRef::divide_by_zero_exception(asm),
            Self::CoroutineResumed
            | Self::AsyncFnResumed
            | Self::AsyncGenFnResumed
            | Self::GenFnNone
            | Self::CoroutineResumedAfterPanic
            | Self::AsyncFnResumedAfterPanic
            | Self::AsyncGenFnResumedAfterPanic
            | Self::GenFnNoneAfterPanic
            | Self::CoroutineResumedAfterDrop
            | Self::AsyncFnResumedAfterDrop
            | Self::AsyncGenFnResumedAfterDrop
            | Self::GenFnNoneAfterDrop => ClassRef::invalid_operation_exception(asm),
        }
    }

    /// These entry points carry rustc's `nounwind` contract. The native implementation aborts if
    /// its panic handler attempts to unwind; the managed fallback must therefore terminate through
    /// `Environment.FailFast` rather than letting even a typed managed exception cross the call.
    #[must_use]
    const fn is_nounwind(self) -> bool {
        matches!(
            self,
            Self::NoUnwindFormat
                | Self::NoUnwind
                | Self::NoUnwindNoBacktrace
                | Self::MisalignedPointerDereference
                | Self::NullPointerDereference
                | Self::InvalidEnumConstruction
                | Self::CannotUnwind
                | Self::InCleanup
        )
    }
}

/// A well-known service requested by rustc-generated runtime glue.
///
/// This classification is deliberately not serialized: artifacts retain their historical method
/// symbols, while the final linker classifies them once and resolves them against target/runtime
/// capabilities. That preserves the existing artifact format without letting correctness hinge on
/// repeated ad-hoc string comparisons.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeService {
    Alloc,
    AllocZeroed,
    Dealloc,
    Realloc,
    NoAllocShim,
    Panic(PanicKind),
    /// A monomorphic helper emitted by `core::ub_checks::assert_unsafe_precondition!`.
    CoreUbPrecondition,
    /// The libgcc/libunwind helper used only to turn a program counter into a symbol address.
    /// Managed CIL has no DWARF FDE to query, so the registered capability preserves the PC.
    UnwindFindEnclosingFunction,
    /// The libgcc/libunwind accessor for a native unwind context's stack pointer.
    /// Managed CIL cannot produce such a context, so the registered capability reports no CFA.
    UnwindGetCfa,
    /// The libgcc/libunwind accessor for a native unwind context's instruction pointer.
    /// Managed CIL cannot produce such a context, so the registered capability reports no IP.
    UnwindGetIp,
    /// The libgcc/libunwind frame walker.
    /// Managed CIL has no native unwind table to walk, so the registered capability reports EOF.
    UnwindBacktrace,
}

fn is_core_ub_precondition(demangled: &str) -> bool {
    if !demangled.ends_with("::precondition_check") {
        return false;
    }
    if demangled.starts_with("core::") {
        return true;
    }

    // Alternate rustc demangling erases the encoded `core` owner for inherent methods on
    // primitives. User crates cannot define inherent primitive methods, but keep this exception
    // pinned to the exact method families observed in the pinned core rather than accepting an
    // arbitrary leaf-name suffix.
    if matches!(
        demangled,
        "<*const _>::offset_from_unsigned::precondition_check"
            | "<*mut _>::offset_from_unsigned::precondition_check"
    ) {
        return true;
    }
    let Some((receiver, method)) = demangled
        .strip_prefix('<')
        .and_then(|path| path.split_once(">::"))
    else {
        return false;
    };
    matches!(
        receiver,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
    ) && method == "unchecked_add::precondition_check"
}

impl RuntimeService {
    #[must_use]
    pub fn classify(emitted_symbol: &str) -> Option<Self> {
        let demangled = format!("{:#}", rustc_demangle::demangle(emitted_symbol));
        if is_core_ub_precondition(&demangled) {
            return Some(Self::CoreUbPrecondition);
        }
        match demangled.as_str() {
            "__rustc::__rust_alloc" | "__rust_alloc" => Some(Self::Alloc),
            "__rustc::__rust_alloc_zeroed" | "__rust_alloc_zeroed" => Some(Self::AllocZeroed),
            "__rustc::__rust_dealloc" | "__rust_dealloc" => Some(Self::Dealloc),
            "__rustc::__rust_realloc" | "__rust_realloc" => Some(Self::Realloc),
            "__rustc::__rust_no_alloc_shim_is_unstable"
            | "__rustc::__rust_no_alloc_shim_is_unstable_v2"
            | "__rust_no_alloc_shim_is_unstable"
            | "__rust_no_alloc_shim_is_unstable_v2" => Some(Self::NoAllocShim),
            "_Unwind_FindEnclosingFunction" => Some(Self::UnwindFindEnclosingFunction),
            "_Unwind_GetCFA" => Some(Self::UnwindGetCfa),
            "_Unwind_GetIP" => Some(Self::UnwindGetIp),
            "_Unwind_Backtrace" => Some(Self::UnwindBacktrace),
            _ => PanicKind::ALL
                .into_iter()
                .find(|kind| demangled == kind.canonical_symbol())
                .map(Self::Panic),
        }
    }

    #[must_use]
    pub const fn canonical_symbol(self) -> &'static str {
        match self {
            Self::Alloc => "__rust_alloc",
            Self::AllocZeroed => "__rust_alloc_zeroed",
            Self::Dealloc => "__rust_dealloc",
            Self::Realloc => "__rust_realloc",
            Self::NoAllocShim => "__rust_no_alloc_shim_is_unstable",
            Self::Panic(kind) => kind.canonical_symbol(),
            Self::CoreUbPrecondition => "core::ub_checks::precondition_check",
            Self::UnwindFindEnclosingFunction => "_Unwind_FindEnclosingFunction",
            Self::UnwindGetCfa => "_Unwind_GetCFA",
            Self::UnwindGetIp => "_Unwind_GetIP",
            Self::UnwindBacktrace => "_Unwind_Backtrace",
        }
    }

    const fn requires_registered_capability(self) -> bool {
        !matches!(
            self,
            Self::NoAllocShim | Self::Panic(_) | Self::CoreUbPrecondition
        )
    }
}

/// The concrete capability used to resolve one missing method reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeCapability {
    PatcherOverride,
    BuiltinUnwindIdentity,
    BuiltinUnwindCfaUnavailable,
    BuiltinUnwindIpUnavailable,
    BuiltinUnwindBacktraceEndOfStack,
    DeclaredNativeImport,
    LegacyNativeImport,
    BuiltinNoOp,
    BuiltinPanic,
    BuiltinCoreUbPrecondition,
}

/// Typed result of resolving one method reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MethodResolution {
    ExternalReference,
    AlreadyDefined,
    Resolved {
        capability: RuntimeCapability,
        service: Option<RuntimeService>,
    },
    Unresolved,
}

/// A structured failure while resolving runtime services and missing method references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MissingMethodResolutionError {
    UnsupportedRuntimeService {
        service: RuntimeService,
        emitted_symbol: String,
    },
    InterfaceMemberMismatch {
        interface: String,
        member: String,
        signature: String,
    },
    MissingOwner {
        owner: String,
        member: String,
    },
    RuntimeServiceSignatureMismatch {
        service: RuntimeService,
        emitted_symbol: String,
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for MissingMethodResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedRuntimeService {
                service,
                emitted_symbol,
            } => write!(
                f,
                "runtime service {service:?} requested by `{emitted_symbol}` has no registered linker capability"
            ),
            Self::InterfaceMemberMismatch {
                interface,
                member,
                signature,
            } => write!(
                f,
                "call to `{interface}::{member}` does not match a declared interface member (signature {signature})"
            ),
            Self::MissingOwner { owner, member } => write!(
                f,
                "cannot synthesize missing method `{member}` because owner type `{owner}` has no definition"
            ),
            Self::RuntimeServiceSignatureMismatch {
                service,
                emitted_symbol,
                expected,
                actual,
            } => write!(
                f,
                "runtime service {service:?} requested by `{emitted_symbol}` has signature {actual}; expected {expected}"
            ),
        }
    }
}

impl std::error::Error for MissingMethodResolutionError {}

/// Registry of exact linker shims plus typed implementations of well-known runtime services.
/// Exact symbols remain available for the large intrinsic/libc surface, while allocator/runtime
/// choices are registered and queried through [`RuntimeService`].
#[derive(Default)]
pub struct MissingMethodPatcher {
    symbols: FxHashMap<Interned<IString>, MissingMethodGenerator>,
    services: FxHashMap<RuntimeService, Interned<IString>>,
}

impl MissingMethodPatcher {
    pub fn insert(
        &mut self,
        symbol: Interned<IString>,
        generator: MissingMethodGenerator,
    ) -> Option<MissingMethodGenerator> {
        self.symbols.insert(symbol, generator)
    }

    pub fn insert_runtime_service(
        &mut self,
        asm: &mut Assembly,
        service: RuntimeService,
        generator: MissingMethodGenerator,
    ) -> Option<MissingMethodGenerator> {
        let symbol = asm.alloc_string(service.canonical_symbol());
        self.services.insert(service, symbol);
        self.symbols.insert(symbol, generator)
    }

    #[must_use]
    pub fn get(&self, symbol: &Interned<IString>) -> Option<&MissingMethodGenerator> {
        self.symbols.get(symbol)
    }

    #[must_use]
    pub fn get_runtime_service(&self, service: RuntimeService) -> Option<&MissingMethodGenerator> {
        self.services
            .get(&service)
            .and_then(|symbol| self.symbols.get(symbol))
    }

    #[must_use]
    pub fn contains_key(&self, symbol: &Interned<IString>) -> bool {
        self.symbols.contains_key(symbol)
    }

    pub fn remove(&mut self, symbol: &Interned<IString>) -> Option<MissingMethodGenerator> {
        self.services
            .retain(|_, registered_symbol| registered_symbol != symbol);
        self.symbols.remove(symbol)
    }
}

/// A native symbol declared by Rust through an `extern` block and its CLR P/Invoke contract.
///
/// These records deliberately use owned strings rather than assembly arena indices: they are
/// crate-level linker metadata, survive artifact serialization, and are merged before missing
/// methods are materialized into [`MethodDef`]s.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct NativeImport {
    pub rust_symbol: String,
    pub entry_point: String,
    pub library: String,
    pub call_conv: super::PInvokeCallConv,
    pub preserve_errno: bool,
}

/// Summary of one fixed-point missing-method resolution pass.
///
/// `method_refs_processed` counts every [`MethodRef`] that existed at the start of the pass or was
/// interned by a patcher while the pass was running. Each reference is processed exactly once.
/// `unresolved_missing_methods` counts non-abstract definitions that still intentionally use the
/// runtime-throwing [`MethodImpl::Missing`] implementation after resolution; abstract methods use
/// `Missing` only as an exporter-ignored placeholder and are not included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MissingMethodResolutionStats {
    pub method_refs_processed: usize,
    pub method_refs_added: usize,
    pub external_method_refs: usize,
    pub already_defined: usize,
    pub overrides_applied: usize,
    pub externs_synthesized: usize,
    pub allocator_shims_synthesized: usize,
    pub no_alloc_shims_synthesized: usize,
    pub panic_shims_synthesized: usize,
    pub core_ub_precondition_shims_synthesized: usize,
    pub unwind_shims_synthesized: usize,
    pub unwind_cfa_shims_synthesized: usize,
    pub unwind_ip_shims_synthesized: usize,
    pub unwind_backtrace_shims_synthesized: usize,
    pub missing_stubs_synthesized: usize,
    pub unresolved_missing_methods: usize,
}

impl MissingMethodResolutionStats {
    fn record(&mut self, resolution: MethodResolution) {
        match resolution {
            MethodResolution::ExternalReference => self.external_method_refs += 1,
            MethodResolution::AlreadyDefined => self.already_defined += 1,
            MethodResolution::Resolved {
                capability: RuntimeCapability::PatcherOverride,
                service,
            } => {
                self.overrides_applied += 1;
                if matches!(
                    service,
                    Some(
                        RuntimeService::Alloc
                            | RuntimeService::AllocZeroed
                            | RuntimeService::Dealloc
                            | RuntimeService::Realloc
                    )
                ) {
                    self.allocator_shims_synthesized += 1;
                }
            }
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinUnwindIdentity,
                service: Some(RuntimeService::UnwindFindEnclosingFunction),
            } => self.unwind_shims_synthesized += 1,
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinUnwindIdentity,
                service,
            } => panic!("managed unwind identity recorded for wrong service {service:?}"),
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinUnwindCfaUnavailable,
                service: Some(RuntimeService::UnwindGetCfa),
            } => {
                self.unwind_shims_synthesized += 1;
                self.unwind_cfa_shims_synthesized += 1;
            }
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinUnwindCfaUnavailable,
                service,
            } => panic!("managed unwind CFA fallback recorded for wrong service {service:?}"),
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinUnwindIpUnavailable,
                service: Some(RuntimeService::UnwindGetIp),
            } => {
                self.unwind_shims_synthesized += 1;
                self.unwind_ip_shims_synthesized += 1;
            }
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinUnwindIpUnavailable,
                service,
            } => panic!("managed unwind IP fallback recorded for wrong service {service:?}"),
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinUnwindBacktraceEndOfStack,
                service: Some(RuntimeService::UnwindBacktrace),
            } => {
                self.unwind_shims_synthesized += 1;
                self.unwind_backtrace_shims_synthesized += 1;
            }
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinUnwindBacktraceEndOfStack,
                service,
            } => panic!("managed unwind backtrace fallback recorded for wrong service {service:?}"),
            MethodResolution::Resolved {
                capability: RuntimeCapability::DeclaredNativeImport,
                ..
            }
            | MethodResolution::Resolved {
                capability: RuntimeCapability::LegacyNativeImport,
                ..
            } => self.externs_synthesized += 1,
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinNoOp,
                ..
            } => self.no_alloc_shims_synthesized += 1,
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinPanic,
                service: Some(RuntimeService::Panic(_)),
            } => self.panic_shims_synthesized += 1,
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinPanic,
                service,
            } => panic!("builtin panic resolution recorded for non-panic service {service:?}"),
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinCoreUbPrecondition,
                service: Some(RuntimeService::CoreUbPrecondition),
            } => self.core_ub_precondition_shims_synthesized += 1,
            MethodResolution::Resolved {
                capability: RuntimeCapability::BuiltinCoreUbPrecondition,
                service,
            } => {
                panic!("core UB precondition resolution recorded for wrong service {service:?}")
            }
            MethodResolution::Unresolved => self.missing_stubs_synthesized += 1,
        }
    }
}

/// Stable size snapshot of every assembly-owned interning arena and definition collection.
///
/// The definition and section counts are included alongside the arenas so compaction diagnostics
/// can distinguish removed backing values from accidentally removed program structure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssemblyArenaCounts {
    pub strings: usize,
    pub types: usize,
    pub class_refs: usize,
    pub nodes: usize,
    pub roots: usize,
    pub signatures: usize,
    pub method_refs: usize,
    pub fields: usize,
    pub statics: usize,
    pub const_data: usize,
    pub class_defs: usize,
    pub method_defs: usize,
    pub sections: usize,
}

/// Before/after accounting for one whole-assembly compaction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactionStats {
    pub before: AssemblyArenaCounts,
    pub after: AssemblyArenaCounts,
    pub relocation: super::asm_link::RelocationStats,
}

impl std::fmt::Display for CompactionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        macro_rules! arena {
            ($name:literal, $field:ident) => {
                write!(
                    f,
                    concat!($name, " {}->{} "),
                    self.before.$field, self.after.$field
                )?
            };
        }
        arena!("strings", strings);
        arena!("types", types);
        arena!("class-refs", class_refs);
        arena!("nodes", nodes);
        arena!("roots", roots);
        arena!("signatures", signatures);
        arena!("method-refs", method_refs);
        arena!("fields", fields);
        arena!("statics", statics);
        arena!("const-data", const_data);
        arena!("class-defs", class_defs);
        arena!("method-defs", method_defs);
        write!(
            f,
            "sections {}->{}",
            self.before.sections, self.after.sections
        )
    }
}

impl std::fmt::Display for MissingMethodResolutionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "processed {} method refs ({} discovered during resolution): {} overrides, {} externs, \
             {} allocator shims, {} no-alloc shims, {} panic shims, {} core UB precondition shims, \
             {} unwind shims, \
             {} missing stubs; {} unresolved non-abstract MethodImpl::Missing definitions remain",
            self.method_refs_processed,
            self.method_refs_added,
            self.overrides_applied,
            self.externs_synthesized,
            self.allocator_shims_synthesized,
            self.no_alloc_shims_synthesized,
            self.panic_shims_synthesized,
            self.core_ub_precondition_shims_synthesized,
            self.unwind_shims_synthesized,
            self.missing_stubs_synthesized,
            self.unresolved_missing_methods,
        )
    }
}
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
    native_imports: Vec<NativeImport>,
    /// A list of all buffers within this assembly.
    pub(crate) const_data: BiMap<Box<[u8]>>,
    /// Rust's semantic size for Rust-origin value types whose CLR storage layout may be larger.
    ///
    /// This is codegen-only metadata: every use is lowered to a constant before the assembly is
    /// serialized, so linked artifacts neither need nor should carry rustc-specific layout facts.
    /// In particular, GC-reference fields hoisted out of overlapping enum/coroutine storage grow
    /// the physical CLR value type without changing Rust's `size_of` or pointer stride.
    #[serde(skip)]
    rust_semantic_sizes: FxHashMap<Interned<ClassRef>, u64>,
    /// Non-serialized semantic indexes retained across repeated codegen-shard links.
    #[serde(skip)]
    pub(crate) link_preflight_index: Option<Box<super::asm_link::AssemblyLinkIndex>>,
}

/// A failure from the unconditional final-emission verifier.
#[derive(Debug)]
pub enum VerificationFailure {
    Method {
        method: MethodDefIdx,
        method_name: String,
        error: TypeCheckError,
    },
    AssemblyInvariant {
        message: String,
    },
}

impl std::fmt::Display for VerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Method {
                method,
                method_name,
                error,
            } => write!(
                f,
                "CIL type-verifier rejected method `{method_name}` ({method:?}): {error:?}"
            ),
            Self::AssemblyInvariant { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for VerificationFailure {}

/// A direct-PE render failure, separated into target-independent IR verification and target
/// capability validation. Unsupported PE constructs are reported before the writer can reach a
/// `todo!()` arm or release partial bytes.
#[derive(Debug)]
pub enum PeEmissionError {
    Verification(VerificationFailure),
    Target(super::pe_exporter::PeTargetError),
}

impl std::fmt::Display for PeEmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verification(error) => error.fmt(f),
            Self::Target(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for PeEmissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Verification(error) => Some(error),
            Self::Target(error) => Some(error),
        }
    }
}

impl From<VerificationFailure> for PeEmissionError {
    fn from(error: VerificationFailure) -> Self {
        Self::Verification(error)
    }
}

impl From<super::pe_exporter::PeTargetError> for PeEmissionError {
    fn from(error: super::pe_exporter::PeTargetError) -> Self {
        Self::Target(error)
    }
}

impl PeEmissionError {
    fn into_verification_failure(self) -> VerificationFailure {
        match self {
            Self::Verification(error) => error,
            Self::Target(error) => VerificationFailure::AssemblyInvariant {
                message: error.to_string(),
            },
        }
    }
}

/// An assembly that passed the unconditional verifier after its final mutation.
///
/// The inner [`Assembly`] is deliberately private and this type does not implement `DerefMut`:
/// callers can inspect it and export it, but cannot reopen it for mutation.
pub struct ExportReadyAssembly {
    inner: Assembly,
}

impl std::ops::Deref for ExportReadyAssembly {
    type Target = Assembly;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl ExportReadyAssembly {
    /// Serializes the verified assembly without reopening it for mutation.
    pub fn save_tmp<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        self.inner.save_tmp(w)
    }

    /// Emits the verified assembly through an immutable exporter.
    #[cfg(not(miri))]
    pub fn export(&self, out: impl AsRef<std::path::Path>, mut exporter: impl Exporter) {
        if *LINKER_RECOVER {
            eprintln!("{:?}", exporter.export(&self.inner, out.as_ref()));
        } else {
            exporter.export(&self.inner, out.as_ref()).unwrap();
        }
    }

    /// Renders the direct-PE output, then re-verifies the assembly before any bytes can escape.
    ///
    /// The direct PE renderer still interns a handful of helper references while lowering. Keeping
    /// that mutable access private to this consuming method means those mutations cannot invalidate
    /// the original verification certificate: the returned bytes are released only after a second,
    /// unconditional verification succeeds.
    pub fn render_pe(
        self,
        options: &super::pe_exporter::export::ExportOptions,
    ) -> Result<(Vec<u8>, Vec<u8>), VerificationFailure> {
        self.try_render_pe(options)
            .map_err(PeEmissionError::into_verification_failure)
    }

    /// Renders direct PE with target-capability failures preserved as structured errors.
    pub fn try_render_pe(
        self,
        options: &super::pe_exporter::export::ExportOptions,
    ) -> Result<(Vec<u8>, Vec<u8>), PeEmissionError> {
        self.render_with_reverification(|asm| super::pe_exporter::export::export_pe(asm, options))
    }

    /// Renders direct PE plus a standard Source Link payload in the Portable PDB, then performs
    /// the same unconditional post-render verification as [`Self::render_pe`].
    pub fn render_pe_with_source_link(
        self,
        options: &super::pe_exporter::export::ExportOptions,
        source_link_json: Option<&str>,
    ) -> Result<(Vec<u8>, Vec<u8>), VerificationFailure> {
        self.try_render_pe_with_source_link(options, source_link_json)
            .map_err(PeEmissionError::into_verification_failure)
    }

    /// Renders direct PE plus Source Link while preserving structured target-capability errors.
    pub fn try_render_pe_with_source_link(
        self,
        options: &super::pe_exporter::export::ExportOptions,
        source_link_json: Option<&str>,
    ) -> Result<(Vec<u8>, Vec<u8>), PeEmissionError> {
        self.render_with_reverification(|asm| {
            super::pe_exporter::export::export_pe_with_source_link(asm, options, source_link_json)
        })
    }

    fn render_with_reverification<T>(
        self,
        render: impl FnOnce(&mut Assembly) -> T,
    ) -> Result<T, PeEmissionError> {
        // Definition-owned constraints are already retained and can fail cheaply before copying a
        // very large unsupported graph. Arena-sensitive capability checks run only after compaction,
        // so the validator and byte writer see the same graph and dead interned nodes are irrelevant.
        super::pe_exporter::validate_retained_definitions_for_pe(&self.inner)?;
        let (compacted, _) = self.inner.compact();
        let verified = compacted.verify_for_export()?;
        let mut inner = verified.inner;
        super::pe_exporter::validate_for_pe(&inner)?;
        let output = render(&mut inner);
        let verified = inner.verify_for_export()?;
        super::pe_exporter::validate_for_pe(&verified.inner)?;
        Ok(output)
    }
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
    /// Records a crate-level native import, rejecting contradictory declarations for one symbol.
    pub fn add_native_import(&mut self, import: NativeImport) {
        if let Some(existing) = self
            .native_imports
            .iter()
            .find(|existing| existing.rust_symbol == import.rust_symbol)
        {
            assert_eq!(
                existing, &import,
                "conflicting native import declarations for `{}`",
                import.rust_symbol
            );
            return;
        }
        self.link_preflight_index = None;
        self.native_imports.push(import);
    }

    #[cfg(test)]
    pub(crate) fn push_native_import_unchecked_for_test(&mut self, import: NativeImport) {
        self.link_preflight_index = None;
        self.native_imports.push(import);
    }

    #[must_use]
    pub fn native_imports(&self) -> &[NativeImport] {
        &self.native_imports
    }

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

    /// Consumes this assembly and seals it for final emission.
    ///
    /// Unlike [`Self::typecheck`], this gate is unconditional: the diagnostic/escape-hatch
    /// environment flags used by earlier per-crate checks cannot skip the final post-link check.
    /// The only successful result is an [`ExportReadyAssembly`] whose inner assembly is no longer
    /// available through a mutable reference.
    pub fn verify_for_export(mut self) -> Result<ExportReadyAssembly, VerificationFailure> {
        self.sanity_check();
        self.validate_fixed_array_layouts()
            .map_err(|message| VerificationFailure::AssemblyInvariant { message })?;
        let method_def_idxs = self.stable_method_def_idxs();
        for &method in &method_def_idxs {
            let definition = self.method_def(method);
            if !definition.is_abstract()
                && matches!(definition.implementation(), MethodImpl::Missing)
            {
                let method_name = self[definition.name()].to_string();
                return Err(VerificationFailure::AssemblyInvariant {
                    message: format!(
                        "reachable method `{method_name}` ({method:?}) still has MethodImpl::Missing after resolution and DCE"
                    ),
                });
            }
        }
        for method in method_def_idxs {
            let mut tmp_method = self.method_def(method).clone();
            if let Err(error) = tmp_method.typecheck(&mut self) {
                let method_name = self[self[method].name()].to_string();
                let dump = crate::ir::dump::dump_method(&tmp_method, &mut self);
                eprintln!("{dump}");
                return Err(VerificationFailure::Method {
                    method,
                    method_name,
                    error,
                });
            }
        }
        Ok(ExportReadyAssembly { inner: self })
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
        if !*crate::OPTIMIZE_CIL {
            return OptFuel::new(0);
        }
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
        let total = self
            .method_defs
            .values()
            .map(|method| Self::method_optimization_weight(method))
            .sum::<u64>();
        OptFuel::new(u32::try_from(total).unwrap_or(u32::MAX))
    }

    fn push_semantic_len(key: &mut Vec<u8>, len: usize) {
        key.extend_from_slice(
            &u64::try_from(len)
                .expect("semantic key component exceeds u64")
                .to_le_bytes(),
        );
    }

    fn push_semantic_str(&self, key: &mut Vec<u8>, value: Interned<IString>) {
        let value = &self[value];
        Self::push_semantic_bytes(key, value.as_bytes());
    }

    /// Prefix-free string encoding which preserves ordinary lexical ordering. A zero byte in the
    /// source is escaped as `0,0`; `0,1` terminates the component.
    fn push_semantic_bytes(key: &mut Vec<u8>, value: &[u8]) {
        for &byte in value {
            if byte == 0 {
                key.extend_from_slice(&[0, 0]);
            } else {
                key.push(byte);
            }
        }
        key.extend_from_slice(&[0, 1]);
    }

    fn push_class_semantic_key(&self, key: &mut Vec<u8>, class: Interned<ClassRef>) {
        let class = &self[class];
        match class.asm() {
            Some(assembly) => {
                key.push(1);
                self.push_semantic_str(key, assembly);
            }
            None => key.push(0),
        }
        self.push_semantic_str(key, class.name());
        key.push(u8::from(class.is_valuetype()));
        Self::push_semantic_len(key, class.generics().len());
        for generic in class.generics() {
            self.push_type_semantic_key(key, *generic);
        }
    }

    /// An injective, process-independent encoding of a class reference identity.
    pub(crate) fn class_semantic_key(&self, class: Interned<ClassRef>) -> Vec<u8> {
        let mut key = Vec::new();
        self.push_class_semantic_key(&mut key, class);
        key
    }

    fn push_signature_semantic_key(&self, key: &mut Vec<u8>, signature: Interned<FnSig>) {
        let signature = &self[signature];
        Self::push_semantic_len(key, signature.inputs().len());
        for input in signature.inputs() {
            self.push_type_semantic_key(key, *input);
        }
        self.push_type_semantic_key(key, *signature.output());
    }

    fn push_type_semantic_key(&self, key: &mut Vec<u8>, tpe: Type) {
        match tpe {
            Type::Ptr(inner) => {
                key.push(0);
                self.push_type_semantic_key(key, self[inner]);
            }
            Type::Ref(inner) => {
                key.push(1);
                self.push_type_semantic_key(key, self[inner]);
            }
            Type::Int(int) => {
                key.push(2);
                Self::push_semantic_bytes(key, int.name().as_bytes());
            }
            Type::ClassRef(class) => {
                key.push(3);
                self.push_class_semantic_key(key, class);
            }
            Type::Float(float) => {
                key.push(4);
                Self::push_semantic_bytes(key, float.name().as_bytes());
            }
            Type::PlatformString => key.push(5),
            Type::PlatformChar => key.push(6),
            Type::PlatformGeneric(index, kind) => {
                key.push(7);
                key.push(match kind {
                    super::tpe::GenericKind::MethodGeneric => 0,
                    super::tpe::GenericKind::CallGeneric => 1,
                    super::tpe::GenericKind::TypeGeneric => 2,
                });
                key.extend_from_slice(&index.to_le_bytes());
            }
            Type::PlatformObject => key.push(8),
            Type::Bool => key.push(9),
            Type::Void => key.push(10),
            Type::PlatformArray { elem, dims } => {
                key.push(11);
                key.push(dims.get());
                self.push_type_semantic_key(key, self[elem]);
            }
            Type::FnPtr(signature) => {
                key.push(12);
                self.push_signature_semantic_key(key, signature);
            }
            Type::SIMDVector(vector) => {
                key.push(13);
                let name = vector.name();
                Self::push_semantic_bytes(key, name.as_bytes());
            }
        }
    }

    /// An injective, process-independent encoding of a CIL type identity.
    pub(crate) fn type_semantic_key(&self, tpe: Type) -> Vec<u8> {
        let mut key = Vec::new();
        self.push_type_semantic_key(&mut key, tpe);
        key
    }

    fn push_method_ref_semantic_key(&self, key: &mut Vec<u8>, method: Interned<MethodRef>) {
        let method = &self[method];
        self.push_class_semantic_key(key, method.class());
        self.push_semantic_str(key, method.name());
        key.push(match method.kind() {
            MethodKind::Static => 0,
            MethodKind::Instance => 1,
            MethodKind::Virtual => 2,
            MethodKind::Constructor => 3,
        });
        self.push_signature_semantic_key(key, method.sig());
        Self::push_semantic_len(key, method.generics().len());
        for generic in method.generics() {
            self.push_type_semantic_key(key, *generic);
        }
    }

    pub(crate) fn method_ref_semantic_key(&self, method: Interned<MethodRef>) -> Vec<u8> {
        let mut key = Vec::new();
        self.push_method_ref_semantic_key(&mut key, method);
        key
    }

    /// An injective, process-independent encoding of a method's complete `MethodRef` identity.
    pub(crate) fn method_semantic_key(&self, method: MethodDefIdx) -> Vec<u8> {
        self.method_ref_semantic_key(method.0)
    }
    /// Returns method definitions in a process-independent semantic order.
    ///
    /// `method_defs` is an `FxHashMap`. Its iteration order depends on the table's insertion and
    /// resize history, which can differ after independently linked clean builds even when the
    /// assembly is semantically identical. The optimizer allocates fuel in this order so hash-map
    /// history cannot decide which method receives a remainder unit. In particular, preserving the
    /// order through `realloc_locals` keeps otherwise-identical direct-PE output reproducible.
    fn stable_method_def_idxs(&self) -> Vec<MethodDefIdx> {
        let mut methods: Vec<_> = self.method_defs.keys().copied().collect();
        methods.sort_by_cached_key(|method| self.method_semantic_key(*method));
        methods
    }

    /// Estimates the optimizer work owned by one method. The constants retain the historical
    /// budget shape (four units per method plus sixteen per retained root), but count roots on the
    /// method rather than charging against the process-wide root arena.
    fn method_optimization_weight(method: &MethodDef) -> u64 {
        let roots = method.implementation().root_count();
        4_u64.saturating_add(u64::try_from(roots).unwrap_or(u64::MAX).saturating_mul(16))
    }

    fn optimizer_method_schedule(&self) -> Vec<(MethodDefIdx, u64)> {
        self.stable_method_def_idxs()
            .into_iter()
            .map(|method| {
                (
                    method,
                    Self::method_optimization_weight(&self.method_defs[&method]),
                )
            })
            .collect()
    }

    /// Splits a caller-supplied total budget before optimization starts. No method can consume
    /// another method's allocation, and the largest-remainder tie-break follows semantic method
    /// order, so insertion/hash-map history cannot decide who gets the last work units.
    #[cfg(test)]
    fn optimizer_method_budgets(&self, total: u32) -> Vec<(MethodDefIdx, u32)> {
        let schedule = self.optimizer_method_schedule();
        Self::optimizer_method_budgets_for_schedule(&schedule, total)
    }

    fn optimizer_method_budgets_for_schedule(
        schedule: &[(MethodDefIdx, u64)],
        total: u32,
    ) -> Vec<(MethodDefIdx, u32)> {
        if schedule.is_empty() || total == 0 {
            return schedule.iter().map(|(method, _)| (*method, 0)).collect();
        }
        let total_weight = schedule.iter().map(|(_, weight)| *weight).sum::<u64>();
        debug_assert_ne!(total_weight, 0);

        let mut budgets = Vec::with_capacity(schedule.len());
        let mut remainders = Vec::with_capacity(schedule.len());
        let mut assigned = 0_u32;
        for (position, &(method, weight)) in schedule.iter().enumerate() {
            let numerator = u128::from(total) * u128::from(weight);
            let budget = u32::try_from(numerator / u128::from(total_weight))
                .expect("proportional optimizer budget exceeds its total");
            assigned = assigned
                .checked_add(budget)
                .expect("optimizer budget accounting overflow");
            budgets.push((method, budget));
            remainders.push((numerator % u128::from(total_weight), position));
        }
        remainders.sort_unstable_by(|(lhs_rem, lhs_pos), (rhs_rem, rhs_pos)| {
            rhs_rem.cmp(lhs_rem).then_with(|| lhs_pos.cmp(rhs_pos))
        });
        for &(_, position) in remainders.iter().take((total - assigned) as usize) {
            budgets[position].1 += 1;
        }
        budgets
    }
    pub(crate) fn borrow_methoddef(&mut self, def_id: MethodDefIdx) -> MethodDef {
        // The returned definition can be changed through crate-private setters before it comes
        // back. Conservatively discard semantic link identities at the borrow boundary.
        self.link_preflight_index = None;
        self.method_defs.remove(&def_id).unwrap()
    }
    pub(crate) fn return_methoddef(&mut self, def_id: MethodDefIdx, def: MethodDef) {
        // Most callers obtained `def` through `borrow_methoddef`, which already invalidates this
        // cache. Keep the return boundary independently correct too: crate-internal callers and
        // future refactors may synthesize a replacement whose MethodRef identity differs.
        self.link_preflight_index = None;
        assert!(
            self.method_defs.insert(def_id, def).is_none(),
            "Could not return a methoddef, because a method def is already present."
        );
    }
    /// Canonicalizes every retained method's CFG without consuming optimizer fuel.
    ///
    /// This is a correctness boundary, not a peephole optimization: roots after an unconditional
    /// transfer and blocks unreachable from the method entry must not participate in whole-program
    /// call-graph reachability. Run it before DCE even when optional CIL optimization is disabled.
    pub fn canonicalize_control_flow(&mut self) {
        for method in self.stable_method_def_idxs() {
            let mut definition = self.borrow_methoddef(method);
            definition.remove_dead_blocks(self);
            self.return_methoddef(method, definition);
        }
    }

    /// Optimizes every method with a deterministic, preallocated share of `fuel`.
    pub fn opt(&mut self, fuel: &mut OptFuel) {
        // The CIL optimizer is purely local/intra-method (copy-prop, DCE, peepholes, block
        // linearization). It does NOT inline calls — Rust's zero-cost abstractions are inlined at the
        // MIR level by rustc's own inliner (the backend raises `-Zinline-mir-hint-threshold`), which
        // is correct by construction and runs before codegen. So a fixpoint of local passes suffices;
        // no soundness snapshot/revert is needed.
        let initial_fuel = fuel.raw();
        let schedule = self.optimizer_method_schedule();
        let mut consumed = 0_u32;
        for (method, budget) in Self::optimizer_method_budgets_for_schedule(&schedule, initial_fuel)
        {
            let mut method_fuel = OptFuel::new(budget);
            let mut cache = EffectInfoCache::default();
            let mut tmp_method = self.borrow_methoddef(method);
            while !method_fuel.exchausted() {
                let prev = method_fuel.clone();
                tmp_method.optimize(self, &mut cache, &mut method_fuel);
                tmp_method.remove_dead_blocks(self);
                if method_fuel == prev {
                    break;
                }
            }
            consumed = consumed
                .checked_add(budget - method_fuel.raw())
                .expect("optimizer fuel consumption overflow");
            self.return_methoddef(method, tmp_method);
        }
        *fuel = OptFuel::from_raw(initial_fuel - consumed);

        // Optimization may exhaust the shared fuel immediately after a pass changes or removes
        // locals. Always finish at the same canonical local-slot boundary: otherwise two
        // semantically identical assemblies can retain different MIR/intern insertion histories in
        // their `.locals` order, which changes StandAloneSig rows and every subsequent method-body
        // token in direct-PE output.
        for (method, _) in schedule {
            let mut tmp_method = self.borrow_methoddef(method);
            tmp_method.implementation_mut().realloc_locals(self);
            self.return_methoddef(method, tmp_method);
        }
    }
    /// Optimizes the assembly, cosuming some fuel. This performs a single optimization pass.
    pub fn opt_sigle_pass(&mut self, fuel: &mut OptFuel, cache: &mut EffectInfoCache) {
        let initial_fuel = fuel.raw();
        let schedule = self.optimizer_method_schedule();
        let mut consumed = 0_u32;
        for (method, budget) in Self::optimizer_method_budgets_for_schedule(&schedule, initial_fuel)
        {
            let mut method_fuel = OptFuel::new(budget);
            let mut tmp_method = self.borrow_methoddef(method);
            tmp_method.optimize(self, cache, &mut method_fuel);
            tmp_method.remove_dead_blocks(self);
            self.return_methoddef(method, tmp_method);
            consumed = consumed
                .checked_add(budget - method_fuel.raw())
                .expect("optimizer fuel consumption overflow");
        }
        *fuel = OptFuel::from_raw(initial_fuel - consumed);
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
    pub fn edit_methodef(
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
            .values()
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
    pub fn get_prealloc_string(&self, string: impl Into<IString>) -> Option<Interned<IString>> {
        self.strings.get_id(&string.into())
    }
    pub fn class_mut(&mut self, id: ClassDefIdx) -> &mut ClassDef {
        self.link_preflight_index = None;
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

    /// Records the Rust-language size of a Rust-origin value type.
    ///
    /// The CLR's physical size normally agrees with this value. It deliberately may not agree for
    /// layouts containing hoisted GC-reference sidecars, because CoreCLR requires an unambiguous GC
    /// map while Rust's enum/coroutine ABI permits variant fields to overlap.
    pub fn set_rust_semantic_size(&mut self, class: Interned<ClassRef>, size: u64) {
        if let Some(previous) = self.rust_semantic_sizes.insert(class, size) {
            assert_eq!(
                previous,
                size,
                "conflicting Rust semantic sizes registered for {}",
                self[class].display(self)
            );
        }
    }

    /// Returns the Rust-language size registered for a Rust-origin value type.
    #[must_use]
    pub fn rust_semantic_size(&self, class: Interned<ClassRef>) -> Option<u64> {
        self.rust_semantic_sizes.get(&class).copied()
    }

    /// Validates synthetic fixed-array storage after all codegen shards have been linked.
    ///
    /// Rust arrays and slices encode only a data pointer and a length, so their element stride is
    /// the Rust semantic stride. A managed value type whose CLR storage has grown (for example due
    /// to a GC-reference sidecar for overlapping enum fields) cannot safely inhabit that storage:
    /// fixed-array helper methods would use CLR `sizeof(T)` while raw pointers, slices, `Vec`, and
    /// allocators continue to use Rust's semantic layout. Reject that representation boundary
    /// explicitly until the IR has a typed allocation/stride ABI capable of preserving it.
    pub fn validate_fixed_array_layouts(&self) -> Result<(), String> {
        for (array_idx, array_def) in &self.class_defs {
            let Some(layout) = array_def.fixed_array_layout() else {
                continue;
            };
            let Some(semantic_stride) = layout.semantic_element_stride() else {
                continue;
            };
            let Type::ClassRef(element_ref) = layout.element() else {
                continue;
            };
            let Some(element_idx) = self.class_ref_to_def(element_ref) else {
                // External value types have no local physical layout. The requested Rust layout is
                // the only authoritative information available; CoreCLR validates the final type.
                continue;
            };
            let element_def = &self[element_idx];
            let Some(physical_size) = element_def.explict_size() else {
                // Managed reference classes and opaque external-shaped definitions do not provide
                // inline value storage to compare.
                continue;
            };
            let physical_align = element_def.align().map_or(1, |align| align.get()) as u64;
            let physical_stride = u64::from(physical_size.get()).next_multiple_of(physical_align);
            if physical_stride != semantic_stride || physical_align > layout.requested_align() {
                return Err(format!(
                    "fixed-array layout verification failed: representation-expanded element \
                     cannot inhabit Rust array/slice storage; array={:?}, element={:?}, \
                     semantic_stride={semantic_stride}, physical_stride={physical_stride}, \
                     rust_align={}, clr_align={physical_align}",
                    self.class_ref(array_idx.0).display(self),
                    self.class_ref(element_ref).display(self),
                    layout.requested_align()
                ));
            }
        }
        Ok(())
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
        self.link_preflight_index = None;
        self.class_refs.alloc(class)
    }

    pub fn alloc_node(&mut self, node: impl Into<CILNode>) -> Interned<CILNode> {
        self.nodes.alloc(node.into())
    }

    pub fn alloc_class_ref(&mut self, cref: ClassRef) -> Interned<ClassRef> {
        self.link_preflight_index = None;
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
    pub(crate) fn field_descs(&self) -> &[FieldDesc] {
        self.fields.values()
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
        self.link_preflight_index = None;
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
        let def_idx = MethodDefIdx::from_raw(ref_idx);
        // Call lowering may intern a `Missing` placeholder after a comptime-generated managed
        // method with the same reference has already been defined. Definition order must not turn
        // a real body back into a runtime "missing method" throw. The opposite direction remains
        // valid: a later real definition replaces an earlier placeholder.
        if matches!(def.implementation(), MethodImpl::Missing)
            && self
                .method_defs
                .get(&def_idx)
                .is_some_and(|existing| !matches!(existing.implementation(), MethodImpl::Missing))
        {
            return def_idx;
        }
        // Check that this def is unique
        if !self.method_defs.contains_key(&def_idx) {
            self.class_defs
                .get_mut(&def_class)
                .expect("Method added without a class")
                .add_def(def_idx);
        }

        self.link_preflight_index = None;
        self.method_defs.insert(def_idx, def);

        def_idx
    }

    #[cfg(test)]
    pub(crate) fn add_abstract_methods_bulk_for_test(&mut self, class: ClassDefIdx, count: usize) {
        self.link_preflight_index = None;
        let sig = self.sig([], Type::Void);
        let mut methods = Vec::with_capacity(count);
        for index in 0..count {
            let name = self.alloc_string(format!("bulk_test_method_{index}"));
            let definition = MethodDef::new(
                Access::Private,
                class,
                name,
                sig,
                MethodKind::Static,
                MethodImpl::Missing,
                vec![],
            )
            .with_abstract();
            let method = MethodDefIdx::from_raw(self.alloc_methodref(definition.ref_to()));
            assert!(self.method_defs.insert(method, definition).is_none());
            methods.push(method);
        }
        self.class_defs
            .get_mut(&class)
            .expect("bulk test methods require an existing class")
            .methods_mut()
            .extend(methods);
    }

    fn ensure_init(&mut self, name: &str, access: Access) -> MethodDefIdx {
        let main_module = self.main_module();
        let user_init = self.alloc_string(name);
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
                access,
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
    pub fn user_init(&mut self) -> MethodDefIdx {
        self.ensure_init(USER_INIT, Access::InternalExtern)
    }
    /// Returns a reference to tht thread local constructor.
    pub fn tcctor(&mut self) -> MethodDefIdx {
        self.ensure_init(TCCTOR, Access::InternalExtern)
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
        let Some(main_module) = self.get_prealloc_string(MAIN_MODULE) else {
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

        let Some(cref) = self.get_prealloc_class_ref(class_def.ref_to()) else {
            return false;
        };
        // Check if that definition already exists
        let main_module = if self.class_defs.contains_key(&ClassDefIdx(cref)) {
            ClassDefIdx(cref)
        } else {
            return false;
        };

        let Some(user_init) = self.get_prealloc_string(name) else {
            return false;
        };

        let Some(ctor_sig) = self.get_prealloc_sig(FnSig::new(input.into(), output)) else {
            return false;
        };
        let Some(cctor) = self.get_prealloc_methodref(MethodRef::new(
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
    pub fn get_prealloc_class_ref(&self, cref: ClassRef) -> Option<Interned<ClassRef>> {
        self.class_refs.get_id(&cref)
    }
    pub fn get_prealloc_sig(&self, sig: FnSig) -> Option<Interned<FnSig>> {
        self.sigs.get_id(&sig)
    }
    pub fn get_prealloc_methodref(&self, mref: MethodRef) -> Option<Interned<MethodRef>> {
        self.method_refs.get_id(&mref)
    }
    /// Returns a reference to the static initializer
    pub fn cctor(&mut self) -> MethodDefIdx {
        let mref = self.cctor_mref();
        if self.method_defs.contains_key(&MethodDefIdx::from_raw(mref)) {
            MethodDefIdx::from_raw(mref)
        } else {
            self.ensure_init(CCTOR, Access::Extern)
        }
    }
    fn append_init_roots(&mut self, init: MethodDefIdx, roots: &[Interned<CILRoot>], name: &str) {
        self.link_preflight_index = None;
        let init = self.method_defs.get_mut(&init).unwrap();
        let blocks = init
            .implementation_mut()
            .blocks_mut()
            .unwrap_or_else(|| panic!("EROROR: {name} has no body."));
        let last = blocks
            .iter_mut()
            .last()
            .unwrap_or_else(|| panic!("ERROR: {name} has a body without blocks."));
        let last_root_idx = last.roots().len().saturating_sub(1);
        for (idx, root) in roots.iter().enumerate() {
            last.roots_mut().insert(idx + last_root_idx, *root);
        }
    }
    /// Adds new rooots to the user init list.
    pub fn add_user_init(&mut self, roots: &[Interned<CILRoot>]) {
        let user_init = self.user_init();
        self.append_init_roots(user_init, roots, USER_INIT);
    }
    /// Adds new rooots to the thread local intiailzer .
    pub fn add_tcctor(&mut self, roots: &[Interned<CILRoot>]) {
        let user_init = self.tcctor();
        self.append_init_roots(user_init, roots, TCCTOR);
    }
    /// Adds new rooots to the static initializer
    pub fn add_cctor(&mut self, roots: &[Interned<CILRoot>]) {
        let user_init = self.cctor();
        self.append_init_roots(user_init, roots, CCTOR);
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

    pub(crate) fn iter_class_def_ids(&self) -> impl Iterator<Item = &ClassDefIdx> {
        self.class_defs.keys()
    }
    pub(crate) fn method_def_from_ref(&self, mref: Interned<MethodRef>) -> Option<&MethodDef> {
        self.method_defs.get(&MethodDefIdx::from_raw(mref))
    }

    /// Follows retained type metadata and adds each event/property accessor to the method graph.
    ///
    /// A type can be live without owning a live method itself (for example, a private DTO exposed
    /// by an exported method signature). Walking the type closure here keeps member semantics on
    /// those owners without turning metadata on unrelated types into assembly-wide roots.
    fn extend_member_accessor_edges(
        &self,
        class_roots: impl IntoIterator<Item = ClassDefIdx>,
        seen_classes: &mut FxHashSet<ClassDefIdx>,
        method_wave: &mut FxHashSet<MethodDefIdx>,
        alive_methods: &FxHashSet<MethodDefIdx>,
    ) {
        let roots: Vec<_> = class_roots
            .into_iter()
            .filter(|class| self.class_defs.contains_key(class))
            .collect();
        let mut reachability = SemanticReachability::new(self);
        for class in roots {
            reachability.visit_class_definition(class);
        }
        reachability.close_local_class_definitions();
        let owners: Vec<_> = reachability.class_definitions().collect();
        for class_id in owners {
            if !seen_classes.insert(class_id) {
                continue;
            }
            let class = &self.class_defs[&class_id];
            method_wave.extend(
                class
                    .iter_member_method_refs()
                    .map(MethodDefIdx::from_raw)
                    .filter(|method| {
                        self.method_defs.contains_key(method) && !alive_methods.contains(method)
                    }),
            );
        }
    }

    pub(crate) fn eliminate_dead_fns(&mut self, only_imports: bool) {
        // 1st. Collect all "extern" method definitons, since those are always alive.
        let mut wave: FxHashSet<MethodDefIdx> = self
            .method_defs
            .iter()
            .filter(|(_, def)| def.access().is_extern())
            .map(|(idx, _)| *idx)
            .collect();
        // Event/property metadata is an edge from its owning type, not an assembly-wide root.
        // Only assembly-local exported type definitions are live before any method is reached;
        // accessors on every other class are discovered below when another live method makes that
        // owner reachable. Rooting every accessor unconditionally kept otherwise-dead private
        // types (and their transitive implementation graph) alive forever.
        let exported_classes: Vec<_> = self
            .class_defs
            .iter()
            .filter_map(|(class_id, class)| {
                (class.access().is_extern() && self.class_ref(class_id.0).asm().is_none())
                    .then_some(*class_id)
            })
            .collect();
        let mut seen_member_owners = FxHashSet::default();
        self.extend_member_accessor_edges(
            exported_classes,
            &mut seen_member_owners,
            &mut wave,
            &FxHashSet::default(),
        );
        self.eliminate_dead_fns_from_roots(wave, only_imports, seen_member_owners);
    }
    fn eliminate_dead_fns_from_roots(
        &mut self,
        mut wave: FxHashSet<MethodDefIdx>,
        only_imports: bool,
        mut seen_member_owners: FxHashSet<ClassDefIdx>,
    ) {
        self.canonicalize_control_flow();
        let mut next_wave: FxHashSet<MethodDefIdx> = FxHashSet::default();
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
        while !wave.is_empty() {
            for def in wave
                .iter()
                .map(|def: &MethodDefIdx| self.method_defs.get(def).unwrap())
            {
                // An aliasing method (e.g. a comptime-defined virtual method) has no CIL body of its
                // own, so the `iter_cil` walk below would miss its target — keep the alias target alive
                // explicitly.
                if let MethodImpl::AliasFor(target) = def.implementation() {
                    let tdef = MethodDefIdx::from_raw(*target);
                    if self.method_defs.contains_key(&tdef) && !alive.contains(&tdef) {
                        next_wave.insert(tdef);
                    }
                }
                // An explicit `.override` is an executable metadata edge to the base slot. Keep a
                // same-assembly base MethodDef alive even when no ordinary call references it.
                if let Some(target) = def.overrides() {
                    let tdef = MethodDefIdx::from_raw(target);
                    if self.method_defs.contains_key(&tdef) && !alive.contains(&tdef) {
                        next_wave.insert(tdef);
                    }
                }
                // A live method makes its owner and every type in its metadata signature live.
                // Their event/property accessors are method-graph edges as well. Following the
                // transitive class metadata closure matters for exported signatures such as
                // `fn public(dto: PrivateDto)`, where `PrivateDto` has no independently-rooted
                // method but its property semantics must remain intact.
                let mut reachability = SemanticReachability::new(self);
                reachability.visit_method_definition(def);
                reachability.close_local_class_definitions();
                let class_roots: Vec<_> = reachability.class_definitions().collect();
                self.extend_member_accessor_edges(
                    class_roots,
                    &mut seen_member_owners,
                    &mut next_wave,
                    &alive,
                );
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
                next_wave.extend(defids);
            }
            alive.extend(wave);
            wave = next_wave;
            next_wave = FxHashSet::default();
        }

        // Some cheap sanity checks
        assert!(wave.is_empty());
        assert!(next_wave.is_empty());
        // Set the method set to only include alive methods
        self.link_preflight_index = None;
        self.method_defs = alive
            .iter()
            .map(|id| (*id, self.method_defs.remove(id).unwrap()))
            .collect();
        // clean up typedefs
        let live_method_refs: FxHashSet<_> = self.method_defs.keys().map(|id| id.0).collect();
        self.class_defs.values_mut().for_each(|tdef| {
            tdef.methods_mut()
                .retain(|def| self.method_defs.contains_key(def));
            tdef.retain_member_metadata(|method| live_method_refs.contains(&method));
        });
    }
    pub fn eliminate_dead_code(&mut self) {
        self.eliminate_dead_fns(false);
        self.eliminate_dead_types_with_export_roots(true);
    }
    /// Re-runs reachability after a public facade has replaced the original exported roots.
    ///
    /// Unlike ordinary DCE, empty assembly-local public types are not roots by themselves. Public
    /// types that own a live method or appear in a live signature remain reachable. This keeps a
    /// facade's real API while allowing constrained AOT consumers to avoid parsing unrelated Rust
    /// runtime metadata.
    pub fn eliminate_dead_code_after_facade_projection(&mut self, public_type_name: &str) {
        let public_classes: FxHashSet<_> = self
            .class_defs
            .iter()
            .filter_map(|(id, class)| (&self[class.name()] == public_type_name).then_some(*id))
            .collect();
        let mut roots: FxHashSet<_> = self
            .method_defs
            .iter()
            .filter_map(|(id, method)| public_classes.contains(&method.class()).then_some(*id))
            .collect();
        // Comptime-authored CLR types use local `Extern` ClassDefs to distinguish intentional
        // managed API from ordinary compiler-generated Rust layout classes. Their externally
        // visible members remain public even when no facade method mentions the type directly
        // (DTOs are commonly constructed by C#). Root constructors and ordinary methods as well
        // as property/event accessors, while unrelated runtime symbols still disappear.
        for (class_id, class) in &self.class_defs {
            if !matches!(class.access(), Access::Extern)
                || self.class_ref(class_id.0).asm().is_some()
            {
                continue;
            }
            for definition in class.methods() {
                if let Some(method) = self.method_defs.get(definition)
                    && matches!(method.access(), Access::Public | Access::Extern)
                {
                    roots.insert(*definition);
                }
            }
        }
        self.eliminate_dead_fns_from_roots(roots, false, FxHashSet::default());
        self.eliminate_dead_types_with_export_roots(false);
    }

    /// Installs the small, assembly-internal 128-bit integer compatibility types used only by the
    /// Unity direct-PE profile. Unity's netstandard2.1 facade predates the BCL Int128 types, while
    /// ordinary 64-bit Rust allocation code uses 128-bit intermediates for layout constants and
    /// overflow checks. Keeping two u64 limbs here lets supported Rust code execute without
    /// exposing a public `u128` promise.
    pub fn install_unity_legacy_128_types(&mut self) {
        if self
            .class_defs
            .values()
            .any(|class| &self[class.name()] == "System.UInt128")
        {
            return;
        }

        let uint = self.install_unity_legacy_128_class("System.UInt128");
        self.install_unity_uint128_methods(uint);
        let int = self.install_unity_legacy_128_class("System.Int128");
        self.install_unity_int128_methods(int);
    }

    fn install_unity_legacy_128_class(&mut self, name: &str) -> ClassDefIdx {
        let name = self.alloc_string(name);
        let low = self.alloc_string("_low");
        let high = self.alloc_string("_high");
        let class = self
            .class_def(ClassDef::new(
                name,
                true,
                0,
                None,
                vec![
                    (Type::Int(Int::U64), low, Some(0)),
                    (Type::Int(Int::U64), high, Some(8)),
                ],
                vec![],
                Access::Private,
                std::num::NonZeroU32::new(16),
                std::num::NonZeroU32::new(8),
                true,
            ))
            .expect("Unity Int128 compatibility class must have a valid layout");

        let class_type = Type::ClassRef(class.0);
        let ctor_name = self.alloc_string(".ctor");
        let ctor_sig = self.sig(
            [class_type, Type::Int(Int::U64), Type::Int(Int::U64)],
            Type::Void,
        );
        let low_field = self.alloc_field(FieldDesc::new(class.0, low, Type::Int(Int::U64)));
        let high_field = self.alloc_field(FieldDesc::new(class.0, high, Type::Int(Int::U64)));
        let this = self.alloc_node(CILNode::LdArg(0));
        let low_value = self.alloc_node(CILNode::LdArg(1));
        let set_low = self.alloc_root(CILRoot::SetField(Box::new((low_field, this, low_value))));
        let this = self.alloc_node(CILNode::LdArg(0));
        let high_value = self.alloc_node(CILNode::LdArg(2));
        let set_high = self.alloc_root(CILRoot::SetField(Box::new((high_field, this, high_value))));
        let ret = self.alloc_root(CILRoot::VoidRet);
        self.new_method(MethodDef::new(
            Access::Assembly,
            class,
            ctor_name,
            ctor_sig,
            MethodKind::Constructor,
            MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(
                    vec![set_low, set_high, ret],
                    0,
                    None,
                )],
                locals: vec![],
            },
            vec![None, Some(low), Some(high)],
        ));
        class
    }

    fn unity_128_fields(
        &mut self,
        class: ClassDefIdx,
    ) -> (Interned<FieldDesc>, Interned<FieldDesc>) {
        let low = self.alloc_string("_low");
        let high = self.alloc_string("_high");
        (
            self.alloc_field(FieldDesc::new(class.0, low, Type::Int(Int::U64))),
            self.alloc_field(FieldDesc::new(class.0, high, Type::Int(Int::U64))),
        )
    }

    fn unity_128_ctor_ref(&mut self, class: ClassDefIdx) -> Interned<MethodRef> {
        let name = self.alloc_string(".ctor");
        let sig = self.sig(
            [
                Type::ClassRef(class.0),
                Type::Int(Int::U64),
                Type::Int(Int::U64),
            ],
            Type::Void,
        );
        self.alloc_methodref(MethodRef::new(
            class.0,
            name,
            sig,
            MethodKind::Constructor,
            vec![].into(),
        ))
    }

    fn unity_128_construct(
        &mut self,
        class: ClassDefIdx,
        low: Interned<CILNode>,
        high: Interned<CILNode>,
    ) -> Interned<CILNode> {
        let ctor = self.unity_128_ctor_ref(class);
        self.call(ctor, &[low, high], IsPure::NOT)
    }

    fn install_unity_uint128_methods(&mut self, class: ClassDefIdx) {
        let class_type = Type::ClassRef(class.0);
        let zero = self.alloc_node(Const::U64(0));
        for input in [Int::U32, Int::U64, Int::USize] {
            let value = self.alloc_node(CILNode::LdArg(0));
            let low = if input == Int::U64 {
                value
            } else {
                self.int_cast(value, Int::U64, ExtendKind::ZeroExtend)
            };
            let result = self.unity_128_construct(class, low, zero);
            let ret = self.alloc_root(CILRoot::Ret(result));
            let name = self.alloc_string("op_Implicit");
            let sig = self.sig([Type::Int(input)], class_type);
            self.new_method(MethodDef::new(
                Access::Assembly,
                class,
                name,
                sig,
                MethodKind::Static,
                MethodImpl::MethodBody {
                    blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
                    locals: vec![],
                },
                vec![None],
            ));
        }

        let (low_field, high_field) = self.unity_128_fields(class);
        let equality = self.alloc_string("op_Equality");
        let sig = self.sig([class_type, class_type], Type::Bool);
        let a = self.alloc_node(CILNode::LdArg(0));
        let a_low = self.ld_field(a, low_field);
        let b = self.alloc_node(CILNode::LdArg(1));
        let b_low = self.ld_field(b, low_field);
        let low_eq = self.biop(a_low, b_low, BinOp::Eq);
        let a = self.alloc_node(CILNode::LdArg(0));
        let a_high = self.ld_field(a, high_field);
        let b = self.alloc_node(CILNode::LdArg(1));
        let b_high = self.ld_field(b, high_field);
        let high_eq = self.biop(a_high, b_high, BinOp::Eq);
        let equal = self.biop(low_eq, high_eq, BinOp::And);
        let ret = self.alloc_root(CILRoot::Ret(equal));
        self.new_method(MethodDef::new(
            Access::Assembly,
            class,
            equality,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![None, None],
        ));

        let greater = self.alloc_string("op_GreaterThan");
        let sig = self.sig([class_type, class_type], Type::Bool);
        let a = self.alloc_node(CILNode::LdArg(0));
        let a_high = self.ld_field(a, high_field);
        let b = self.alloc_node(CILNode::LdArg(1));
        let b_high = self.ld_field(b, high_field);
        let high_gt = self.biop(a_high, b_high, BinOp::GtUn);
        let a = self.alloc_node(CILNode::LdArg(0));
        let a_high = self.ld_field(a, high_field);
        let b = self.alloc_node(CILNode::LdArg(1));
        let b_high = self.ld_field(b, high_field);
        let high_eq = self.biop(a_high, b_high, BinOp::Eq);
        let a = self.alloc_node(CILNode::LdArg(0));
        let a_low = self.ld_field(a, low_field);
        let b = self.alloc_node(CILNode::LdArg(1));
        let b_low = self.ld_field(b, low_field);
        let low_gt = self.biop(a_low, b_low, BinOp::GtUn);
        let same_high_low_gt = self.biop(high_eq, low_gt, BinOp::And);
        let result = self.biop(high_gt, same_high_low_gt, BinOp::Or);
        let ret = self.alloc_root(CILRoot::Ret(result));
        self.new_method(MethodDef::new(
            Access::Assembly,
            class,
            greater,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![None, None],
        ));

        self.install_unity_uint128_multiply(class, low_field, high_field);
    }

    fn install_unity_uint128_multiply(
        &mut self,
        class: ClassDefIdx,
        low_field: Interned<FieldDesc>,
        high_field: Interned<FieldDesc>,
    ) {
        let class_type = Type::ClassRef(class.0);
        let load = |asm: &mut Self, arg, field| {
            let value = asm.alloc_node(CILNode::LdArg(arg));
            asm.ld_field(value, field)
        };
        let mask = self.alloc_node(Const::U64(u64::from(u32::MAX)));
        let shift = self.alloc_node(Const::U32(32));
        let a_low = load(self, 0, low_field);
        let b_low = load(self, 1, low_field);
        let a0 = self.biop(a_low, mask, BinOp::And);
        let a1 = self.biop(a_low, shift, BinOp::ShrUn);
        let b0 = self.biop(b_low, mask, BinOp::And);
        let b1 = self.biop(b_low, shift, BinOp::ShrUn);
        let t0 = self.biop(a0, b0, BinOp::Mul);
        let w0 = self.biop(t0, mask, BinOp::And);
        let k = self.biop(t0, shift, BinOp::ShrUn);
        let a1b0 = self.biop(a1, b0, BinOp::Mul);
        let t1 = self.biop(a1b0, k, BinOp::Add);
        let w1 = self.biop(t1, mask, BinOp::And);
        let w2 = self.biop(t1, shift, BinOp::ShrUn);
        let a0b1 = self.biop(a0, b1, BinOp::Mul);
        let t2 = self.biop(a0b1, w1, BinOp::Add);
        let t2_shifted = self.biop(t2, shift, BinOp::Shl);
        let low = self.biop(t2_shifted, w0, BinOp::Add);

        let t2_high = self.biop(t2, shift, BinOp::ShrUn);
        let a1b1 = self.biop(a1, b1, BinOp::Mul);
        let partial = self.biop(t2_high, w2, BinOp::Add);
        let partial = self.biop(partial, a1b1, BinOp::Add);
        let a_low = load(self, 0, low_field);
        let b_high = load(self, 1, high_field);
        let cross1 = self.biop(a_low, b_high, BinOp::Mul);
        let partial = self.biop(partial, cross1, BinOp::Add);
        let a_high = load(self, 0, high_field);
        let b_low = load(self, 1, low_field);
        let cross2 = self.biop(a_high, b_low, BinOp::Mul);
        let high = self.biop(partial, cross2, BinOp::Add);
        let result = self.unity_128_construct(class, low, high);
        let ret = self.alloc_root(CILRoot::Ret(result));
        let name = self.alloc_string("op_Multiply");
        let sig = self.sig([class_type, class_type], class_type);
        self.new_method(MethodDef::new(
            Access::Assembly,
            class,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![None, None],
        ));
    }

    fn install_unity_int128_methods(&mut self, class: ClassDefIdx) {
        let class_type = Type::ClassRef(class.0);
        let value = self.alloc_node(CILNode::LdArg(0));
        let signed = self.int_cast(value, Int::I64, ExtendKind::SignExtend);
        let low = self.int_cast(signed, Int::U64, ExtendKind::ZeroExtend);
        let shift = self.alloc_node(Const::U32(63));
        let high = self.biop(signed, shift, BinOp::Shr);
        let high = self.int_cast(high, Int::U64, ExtendKind::ZeroExtend);
        let result = self.unity_128_construct(class, low, high);
        let ret = self.alloc_root(CILRoot::Ret(result));
        let name = self.alloc_string("op_Implicit");
        let sig = self.sig([Type::Int(Int::I32)], class_type);
        self.new_method(MethodDef::new(
            Access::Assembly,
            class,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![None],
        ));

        let (low_field, high_field) = self.unity_128_fields(class);
        let a = self.alloc_node(CILNode::LdArg(0));
        let a_low = self.ld_field(a, low_field);
        let b = self.alloc_node(CILNode::LdArg(1));
        let b_low = self.ld_field(b, low_field);
        let low_eq = self.biop(a_low, b_low, BinOp::Eq);
        let a = self.alloc_node(CILNode::LdArg(0));
        let a_high = self.ld_field(a, high_field);
        let b = self.alloc_node(CILNode::LdArg(1));
        let b_high = self.ld_field(b, high_field);
        let high_eq = self.biop(a_high, b_high, BinOp::Eq);
        let result = self.biop(low_eq, high_eq, BinOp::And);
        let ret = self.alloc_root(CILRoot::Ret(result));
        let name = self.alloc_string("op_Equality");
        let sig = self.sig([class_type, class_type], Type::Bool);
        self.new_method(MethodDef::new(
            Access::Assembly,
            class,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![None, None],
        ));
    }
    #[cfg(test)]
    pub(crate) fn eliminate_dead_types(&mut self) {
        self.eliminate_dead_types_with_export_roots(true);
    }
    fn eliminate_dead_types_with_export_roots(&mut self, keep_exported_types: bool) {
        let rust_void = self.alloc_string("RustVoid");
        let rust_void = self.alloc_class_ref(ClassRef::new(rust_void, None, true, vec![].into()));
        let f128 = self.alloc_string("f128");
        let f128 = self.alloc_class_ref(ClassRef::new(f128, None, true, vec![].into()));

        let mut reachability = SemanticReachability::new(self);
        for method in self.method_defs().values() {
            reachability.visit_method_definition(method);
        }
        // Keep exported classes defined by this assembly as roots, but do not root
        // external BCL declaration classes merely because they are marked `Extern`.
        // Those declarations are metadata-only conveniences for lowering calls; rooting
        // every one makes their unused signatures (for example Int128/Vector512 and
        // System.Net types) become TypeRef/AssemblyRef rows in otherwise primitive DLLs.
        // A BCL declaration is retained when a live method/type actually references it via
        // the wave above.
        if keep_exported_types {
            let exported: Vec<_> = self
                .class_defs()
                .iter()
                .filter_map(|(defid, def)| {
                    (def.access().is_extern() && self.class_ref(defid.0).asm().is_none())
                        .then_some(*defid)
                })
                .collect();
            for class in exported {
                reachability.visit_class_definition(class);
            }
        }
        if let Some(cref) = self.class_ref_to_def(rust_void) {
            reachability.visit_class_definition(cref);
        }
        if let Some(cref) = self.class_ref_to_def(f128) {
            reachability.visit_class_definition(cref);
        }
        reachability.close_local_class_definitions();
        let alive: FxHashSet<_> = reachability.class_definitions().collect();
        drop(reachability);
        // Set the class_defs to only include alive classes
        self.link_preflight_index = None;
        self.class_defs = alive
            .iter()
            .map(|id| (*id, self.class_defs.remove(id).unwrap()))
            .collect();
    }
    /*pub fn realloc_nodes(&mut self){

    }*/
    /// Reallocates the roots, freeing all dead ones.
    pub fn realloc_roots(&mut self) {
        self.link_preflight_index = None;
        let mut new_roots = BiMap::default();
        for block in self.method_defs.values_mut().flat_map(|def| {
            def.implementation_mut()
                .all_blocks_mut()
                .into_iter()
                .flatten()
        }) {
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

    /// Resolves every in-assembly [`MethodRef`] that has no definition, including references
    /// synthesized by an override while this method is running.
    ///
    /// The growable `method_refs` arena is the worklist: the cursor advances monotonically while
    /// patchers may append new references. Consequently resolution reaches a fixed point without
    /// rescanning old references, and every interned reference is processed at most once per
    /// invocation.
    #[must_use]
    pub fn resolve_missing_methods(
        &mut self,
        externs: &FxHashMap<&str, String>,
        modifies_errno: &FxHashSet<&str>,
        override_methods: &MissingMethodPatcher,
    ) -> MissingMethodResolutionStats {
        self.try_resolve_missing_methods(externs, modifies_errno, override_methods)
            .unwrap_or_else(|error| panic!("linker: {error}"))
    }

    fn is_rust_c_void_pointer(&self, candidate: Type) -> bool {
        let Type::Ptr(pointee) = candidate else {
            return false;
        };
        let Type::ClassRef(class) = self[pointee] else {
            return false;
        };
        let class = self.class_ref(class);
        if !class.is_valuetype() || class.asm().is_some() || !class.generics().is_empty() {
            return false;
        }
        let name = &self[class.name()];
        let Some(identity) = name.strip_prefix("core.ffi.c_void.tid_") else {
            return false;
        };
        identity.len() == 32
            && identity
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    fn is_void_pointer(&self, candidate: Type) -> bool {
        matches!(candidate, Type::Ptr(pointee) if self[pointee] == Type::Void)
    }

    fn is_unwind_reason_code(&self, candidate: Type) -> bool {
        let Type::ClassRef(class) = candidate else {
            return false;
        };
        let class = self.class_ref(class);
        if !class.is_valuetype() || class.asm().is_some() || !class.generics().is_empty() {
            return false;
        }
        let name = &self[class.name()];
        let Some(identity) =
            name.strip_prefix("std.backtrace_rs.backtrace.libunwind.uw._Unwind_Reason_Code.tid_")
        else {
            return false;
        };
        identity.len() == 32
            && identity
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    fn is_unwind_backtrace_signature(&self, signature: &FnSig) -> bool {
        let [Type::FnPtr(callback), argument] = signature.inputs() else {
            return false;
        };
        if !self.is_rust_c_void_pointer(*argument)
            || !self.is_unwind_reason_code(*signature.output())
        {
            return false;
        }
        let callback = &self[*callback];
        let [context, callback_argument] = callback.inputs() else {
            return false;
        };
        self.is_void_pointer(*context)
            && self.is_rust_c_void_pointer(*callback_argument)
            && callback.output() == signature.output()
    }

    /// Strict missing-method resolution. Known runtime services without a registered target
    /// capability and malformed local owners are returned as structured errors rather than being
    /// converted into delayed runtime-throwing stubs.
    pub fn try_resolve_missing_methods(
        &mut self,
        externs: &FxHashMap<&str, String>,
        modifies_errno: &FxHashSet<&str>,
        override_methods: &MissingMethodPatcher,
    ) -> Result<MissingMethodResolutionStats, MissingMethodResolutionError> {
        let initial_mref_count = self.method_refs.len();
        let mut stats = MissingMethodResolutionStats::default();
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
        let mut index = 0;
        while index < self.method_refs.len() {
            stats.method_refs_processed += 1;
            // Get the full method refernce
            let mref_idx = Interned::from_index(
                std::num::NonZeroU32::new(
                    u32::try_from(index)
                        .expect("MethodRef index exceeds u32")
                        .checked_add(1)
                        .expect("MethodRef index overflow"),
                )
                .unwrap(),
            );
            let mref = self.method_refs.get(mref_idx).clone();
            index += 1;
            // Check if this method reference's class has an assembly. If it has, then the method is extern. If it has not, then it is defined in this assembly
            // and must have some kind of implementation
            let class = self.class_ref(mref.class());

            if class.asm().is_some() {
                // Is extern, skip
                stats.record(MethodResolution::ExternalReference);
                continue;
            }
            // Check if this method already has an implementation.
            if self
                .method_defs
                .contains_key(&MethodDefIdx::from_raw(mref_idx))
            {
                // A method defintion already present, so we don't need to do anyting, so skip.
                stats.record(MethodResolution::AlreadyDefined);
                continue;
            }
            // FAIL-LOUDLY BACKSTOP: an in-assembly `MethodRef` that resolves to NO `MethodDef` on
            // an *interface* we define is always a bug, never something to patch. Interface defs
            // (`#[dotnet_interface]`) declare every member up front, so a dangling ref means a
            // call site was lowered with a signature that mismatches the interface's declared
            // member (e.g. a default-body self-call to a member whose `&mut T` parameter is
            // hidden behind a type alias — declared `ref T`, called as `T*` — or a self-call
            // whose generic parameter name was shadowed by a concrete type). Materializing the
            // usual `MethodImpl::Missing` stub here would inject a SECOND, non-abstract member
            // onto the interface: reflection then sees two same-named members
            // (`AmbiguousMatchException`) and the call throws at runtime. Return a structural
            // linker error instead, naming the member.
            let owner = ClassDefIdx(mref.class());
            let owner_name = self.class_ref(mref.class()).display(self);
            let member_name = self[mref.name()].to_string();
            let Some(class_def) = self.class_defs.get(&owner) else {
                return Err(MissingMethodResolutionError::MissingOwner {
                    owner: owner_name,
                    member: member_name,
                });
            };
            if class_def.is_interface() {
                return Err(MissingMethodResolutionError::InterfaceMemberMismatch {
                    interface: owner_name,
                    member: member_name,
                    signature: format!("{:?}", self[mref.sig()]),
                });
            }
            // Patch exact linker symbols, plus the closed rustc-runtime alias set above. Matching
            // arbitrary demangled leaf names is forbidden: `core::fmt::write` must never resolve
            // to the POSIX `write` builtin merely because both end in the same word.
            let emitted_name = self[mref.name()].to_string();
            let service = RuntimeService::classify(&emitted_name);
            if let Some(
                service @ (RuntimeService::UnwindFindEnclosingFunction
                | RuntimeService::UnwindGetCfa
                | RuntimeService::UnwindGetIp
                | RuntimeService::UnwindBacktrace),
            ) = service
            {
                let main_module = *self.main_module();
                let signature = &self[mref.sig()];
                let signature_matches = match service {
                    RuntimeService::UnwindFindEnclosingFunction => {
                        signature.inputs().len() == 1
                            && signature.inputs()[0] == *signature.output()
                            && self.is_rust_c_void_pointer(signature.inputs()[0])
                    }
                    RuntimeService::UnwindGetCfa | RuntimeService::UnwindGetIp => {
                        matches!(signature.inputs(), [input] if self.is_void_pointer(*input))
                            && *signature.output() == Type::Int(Int::USize)
                    }
                    RuntimeService::UnwindBacktrace => {
                        self.is_unwind_backtrace_signature(signature)
                    }
                    _ => unreachable!("matched only managed unwind services"),
                };
                if mref.kind() != MethodKind::Static
                    || mref.class() != main_module
                    || !mref.generics().is_empty()
                    || !self.class_ref(mref.class()).generics().is_empty()
                    || !signature_matches
                {
                    let expected = match service {
                        RuntimeService::UnwindFindEnclosingFunction => {
                            "static nongeneric MainModule identity function with one `core::ffi::c_void` pointer input and the same pointer output"
                        }
                        RuntimeService::UnwindGetCfa | RuntimeService::UnwindGetIp => {
                            "static nongeneric MainModule function with one opaque `*void` unwind-context input and a `usize` output"
                        }
                        RuntimeService::UnwindBacktrace => {
                            "static nongeneric MainModule `(extern C fn(*void, *mut c_void) -> _Unwind_Reason_Code, *mut c_void) -> _Unwind_Reason_Code` function"
                        }
                        _ => unreachable!("matched only managed unwind services"),
                    };
                    return Err(
                        MissingMethodResolutionError::RuntimeServiceSignatureMismatch {
                            service,
                            emitted_symbol: emitted_name,
                            expected: expected.to_string(),
                            actual: format!(
                                "{:?} {:?} with {} method generic arguments and {} owner generic arguments",
                                mref.kind(),
                                signature,
                                mref.generics().len(),
                                self.class_ref(mref.class()).generics().len(),
                            ),
                        },
                    );
                }
            }
            let compatibility_key = service
                .filter(|service| service.requires_registered_capability())
                .map(|service| self.alloc_string(service.canonical_symbol()));
            let overrider = override_methods
                .get(&mref.name())
                .or_else(|| {
                    service.and_then(|service| override_methods.get_runtime_service(service))
                })
                // Compatibility for downstream patcher builders which still register the
                // canonical allocator symbol through `insert` rather than the typed API.
                .or_else(|| compatibility_key.and_then(|key| override_methods.get(&key)));
            if let Some(overrider) = overrider {
                let mref = mref.clone();
                let implementation = overrider(mref_idx, self);
                // `Access::Public`, not `Private`: these patched helpers live on `MainModule`
                // (`transmute`, alloc shims, …) but are called from *other* classes too — e.g. a
                // `#[dotnet_class]` method whose body uses `format!` calls `MainModule.transmute`.
                // A `private` def would make that cross-class call fail at runtime with
                // `MethodAccessException`. Public is not a DCE root, so unused helpers are still
                // culled; intra-class callers (the `::stable` executables) are unaffected.
                self.new_method(mref.into_def(implementation, Access::Public, self));
                stats.record(MethodResolution::Resolved {
                    capability: match service {
                        Some(RuntimeService::UnwindFindEnclosingFunction) => {
                            RuntimeCapability::BuiltinUnwindIdentity
                        }
                        Some(RuntimeService::UnwindGetCfa) => {
                            RuntimeCapability::BuiltinUnwindCfaUnavailable
                        }
                        Some(RuntimeService::UnwindGetIp) => {
                            RuntimeCapability::BuiltinUnwindIpUnavailable
                        }
                        Some(RuntimeService::UnwindBacktrace) => {
                            RuntimeCapability::BuiltinUnwindBacktraceEndOfStack
                        }
                        _ => RuntimeCapability::PatcherOverride,
                    },
                    service,
                });
                continue;
            }

            if service.is_some_and(RuntimeService::requires_registered_capability) {
                return Err(MissingMethodResolutionError::UnsupportedRuntimeService {
                    service: service.expect("checked as Some"),
                    emitted_symbol: emitted_name,
                });
            }

            if service == Some(RuntimeService::NoAllocShim) {
                let arg_names = (0..self[mref.sig()].inputs().len()).map(|_| None).collect();
                let ret = self.alloc_root(CILRoot::VoidRet);
                self.new_method(MethodDef::new(
                    Access::Public,
                    owner,
                    mref.name(),
                    mref.sig(),
                    mref.kind(),
                    MethodImpl::MethodBody {
                        blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
                        locals: vec![],
                    },
                    arg_names,
                ));
                stats.record(MethodResolution::Resolved {
                    capability: RuntimeCapability::BuiltinNoOp,
                    service,
                });
                continue;
            }

            if let Some(RuntimeService::Panic(kind)) = service {
                // Direct-rustc fixtures link the host toolchain's native `core` rlib. The cilly
                // linker cannot decode its LLVM object members, so compiler-generated calls to
                // these non-generic panic entry points have no cilly MethodDef. Product builds use
                // `-Zbuild-std`; their real `core` MethodDef was handled by `AlreadyDefined` above
                // and retains the RustException payload/catch_unwind path.
                //
                // Do not fake a RustException here. Its `data_pointer` must name an allocated Rust
                // panic payload, and these signatures supply only a Location (plus bounds values).
                // A null/fabricated pointer would be handed to `__rust_panic_cleanup`. A typed BCL
                // exception is therefore the safe fallback: it preserves never-returning control
                // flow and is deliberately rethrown by Rust catch_unwind, which accepts only a
                // genuine RustException. `interop_try_catch` remains the catch-all managed bridge.
                let arg_names = (0..self[mref.sig()].inputs().len()).map(|_| None).collect();
                let exception_class = kind.managed_exception(self);
                let throw = self.throw_exception_msg(exception_class, kind.message());
                let mut roots = Vec::with_capacity(usize::from(kind.is_nounwind()) + 1);
                if kind.is_nounwind() {
                    roots.push(self.fail_fast_msg(kind.message()));
                }
                // The typed throw is the observable fallback for ordinary panics. It also leaves
                // nounwind bodies structurally terminal if FailFast were ever to return, while
                // the actual nounwind path remains uncatchable.
                roots.push(throw);
                self.new_method(MethodDef::new(
                    Access::Public,
                    owner,
                    mref.name(),
                    mref.sig(),
                    mref.kind(),
                    MethodImpl::MethodBody {
                        blocks: vec![super::BasicBlock::new(roots, 0, None)],
                        locals: vec![],
                    },
                    arg_names,
                ));
                stats.record(MethodResolution::Resolved {
                    capability: RuntimeCapability::BuiltinPanic,
                    service,
                });
                continue;
            }

            if service == Some(RuntimeService::CoreUbPrecondition) {
                // `assert_unsafe_precondition!` generates a *conditional* diagnostic helper. A
                // valid operation calls the helper and returns after its predicate succeeds; an
                // unconditional FailFast replacement would therefore terminate defined programs.
                // The host's native core artifact cannot provide that monomorphic body to cilly,
                // so omit this optional UB diagnostic while preserving its signature and normal
                // return. Inputs for which the original helper would fail already violate an
                // unsafe Rust precondition and have undefined behavior. Product `-Zbuild-std`
                // builds retain the real definition through `AlreadyDefined` above.
                let arg_names = (0..self[mref.sig()].inputs().len()).map(|_| None).collect();
                let ret = self.alloc_root(CILRoot::VoidRet);
                self.new_method(MethodDef::new(
                    Access::Public,
                    owner,
                    mref.name(),
                    mref.sig(),
                    mref.kind(),
                    MethodImpl::MethodBody {
                        blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
                        locals: vec![],
                    },
                    arg_names,
                ));
                stats.record(MethodResolution::Resolved {
                    capability: RuntimeCapability::BuiltinCoreUbPrecondition,
                    service,
                });
                continue;
            }

            // Prefer the exact crate-level FFI declaration captured from rustc. The linker's
            // historical hardcoded extern map remains below for compiler/runtime shims that do
            // not originate in a user `extern` block.
            if let Some(import) = self
                .native_imports
                .iter()
                .find(|import| import.rust_symbol == emitted_name)
                .cloned()
            {
                let lib = self.alloc_string(import.library);
                let entry_point = self.alloc_string(import.entry_point);
                let arg_names = (0..self[mref.sig()].inputs().len()).map(|_| None).collect();
                let method_def = MethodDef::new(
                    Access::Public,
                    ClassDefIdx(mref.class()),
                    mref.name(),
                    mref.sig(),
                    mref.kind(),
                    MethodImpl::Extern {
                        lib,
                        entry_point: Some(entry_point),
                        call_conv: import.call_conv,
                        preserve_errno: import.preserve_errno,
                    },
                    arg_names,
                );
                self.new_method(method_def);
                stats.record(MethodResolution::Resolved {
                    capability: RuntimeCapability::DeclaredNativeImport,
                    service: None,
                });
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
                        entry_point: None,
                        call_conv: super::PInvokeCallConv::Cdecl,
                        preserve_errno: preserve_errno.contains(&mref.name()),
                    },
                    arg_names,
                );
                self.new_method(method_def);
                stats.record(MethodResolution::Resolved {
                    capability: RuntimeCapability::LegacyNativeImport,
                    service: None,
                });

                continue;
            }
            // Create a replacement method.

            let arg_names = (0..(self[mref.sig()].inputs().len()))
                .map(|_| None)
                .collect();
            let method_def = MethodDef::new(
                Access::Public,
                ClassDefIdx(mref.class()),
                mref.name(),
                mref.sig(),
                mref.kind(),
                MethodImpl::Missing,
                arg_names,
            );
            self.new_method(method_def);
            stats.record(MethodResolution::Unresolved);
        }
        stats.method_refs_added = self.method_refs.len() - initial_mref_count;
        stats.unresolved_missing_methods = self
            .method_defs
            .values()
            .filter(|def| !def.is_abstract() && matches!(def.implementation(), MethodImpl::Missing))
            .count();
        Ok(stats)
    }

    #[must_use]
    pub fn class_ref_to_def(&self, class: Interned<ClassRef>) -> Option<ClassDefIdx> {
        if self.class_defs.contains_key(&ClassDefIdx(class)) {
            Some(ClassDefIdx(class))
        } else {
            None
        }
    }

    /// Finds a registered class definition by NAME ALONE, ignoring the `is_valuetype`/`asm`/
    /// `generics` that are baked into a `ClassRef`'s own identity (and therefore into
    /// [`Self::class_ref_to_def`]'s lookup key).
    ///
    /// Needed by the comptime interpreter's `finish_type` (`src/comptime.rs`): a class can be
    /// described by MULTIPLE comptime entrypoints (a `#[dotnet_class]` struct declaration plus
    /// each `#[dotnet_methods]` impl block re-opening it), and a re-opening entrypoint has no
    /// access to the original struct's `value_type = ...` attribute, so it cannot honestly
    /// construct the SAME `ClassRef` the authoritative entrypoint registered under. Looking the
    /// existing def up by name-only decouples "is this the same class" from "does this
    /// entrypoint happen to agree on is_valuetype" — the latter is reconciled explicitly via
    /// [`ClassDef::set_is_valuetype`] once the def is found, instead of silently registering a
    /// second, phantom same-named `ClassDef` at a different (wrong) `ClassRef` identity, which
    /// `class_ref_to_def`'s exact-`ClassRef`-match lookup would not have caught.
    #[must_use]
    pub fn class_def_by_name(&self, name: Interned<IString>) -> Option<ClassDefIdx> {
        self.class_defs
            .iter()
            .find(|(_, def)| def.name() == name)
            .map(|(idx, _)| *idx)
    }

    /// Returns stable counts for every assembly-owned arena and definition collection.
    #[must_use]
    pub fn arena_counts(&self) -> AssemblyArenaCounts {
        AssemblyArenaCounts {
            strings: self.strings.len(),
            types: self.types.len(),
            class_refs: self.class_refs.len(),
            nodes: self.nodes.len(),
            roots: self.roots.len(),
            signatures: self.sigs.len(),
            method_refs: self.method_refs.len(),
            fields: self.fields.len(),
            statics: self.statics.len(),
            const_data: self.const_data.len(),
            class_defs: self.class_defs.len(),
            method_defs: self.method_defs.len(),
            sections: self.sections.len(),
        }
    }

    /// Compile-time coverage fence for assembly-owned relocation state.
    ///
    /// Adding an arena or definition collection makes this destructure fail to compile until the
    /// relocation design explicitly accounts for the new field.
    pub(crate) fn assert_relocation_arena_coverage(&self) {
        let Self {
            strings: _,
            types: _,
            class_refs: _,
            class_defs: _,
            nodes: _,
            roots: _,
            sigs: _,
            method_refs: _,
            fields: _,
            statics: _,
            method_defs: _,
            sections: _,
            native_imports: _,
            const_data: _,
            // Codegen-only; every semantic size has already become a CIL constant before an
            // assembly reaches relocation, linking, or serialization.
            rust_semantic_sizes: _,
            // Derived, non-serialized semantic lookup cache maintained by the linker.
            link_preflight_index: _,
        } = self;
    }

    /// Rebuilds the assembly from its surviving class and method definitions.
    ///
    /// Only interned values reachable while translating those definitions are copied. Opaque
    /// sections are preserved verbatim because they do not contain assembly-local interned ids.
    #[must_use]
    pub fn compact(mut self) -> (Self, CompactionStats) {
        let before = self.arena_counts();
        let sections = std::mem::take(&mut self.sections);
        let native_imports = std::mem::take(&mut self.native_imports);
        let (mut compacted, relocation) =
            super::asm_link::relocate_assembly(Self::default(), &self);
        compacted.sections = sections;
        compacted.native_imports = native_imports;
        let after = compacted.arena_counts();
        (
            compacted,
            CompactionStats {
                before,
                after,
                relocation,
            },
        )
    }

    /// Hides linker/runtime implementation methods that happen to live on the synthetic
    /// `MainModule` type from external managed consumers.
    ///
    /// `Access::Extern` is the explicit exported API and remains public. Historically, internal
    /// helpers synthesized by cilly and linked Rust dependencies used `Access::Public` so other
    /// generated types could call them. CLR `assembly` visibility provides that same capability
    /// without publishing the entire implementation through reflection and IntelliSense.
    pub fn hide_main_module_implementation_details(&mut self) -> usize {
        self.link_preflight_index = None;
        let main_module_classes: FxHashSet<_> = self
            .class_defs
            .iter()
            .filter_map(|(class, definition)| {
                (&self[definition.name()] == MAIN_MODULE).then_some(*class)
            })
            .collect();
        let mut hidden = 0;
        for method in self.method_defs.values_mut() {
            if main_module_classes.contains(&method.class()) && *method.access() == Access::Public {
                method.set_access(Access::Assembly);
                hidden += 1;
            }
        }
        hidden
    }

    /// Projects explicit managed exports onto a small public facade type.
    ///
    /// rustc and the linker place most generated functions on the synthetic `MainModule` class.
    /// Merely renaming that class to the product's public type makes a CLR load of one tiny export
    /// resolve every unrelated Rust/runtime signature on the same type. That is especially toxic
    /// on Unity's netstandard2.1 profile, where a reachable implementation detail may mention a
    /// newer BCL type such as `System.UInt128` even though the public export does not.
    ///
    /// For each body-backed `Access::Extern` static method, this creates a tiny public forwarding
    /// definition on `public_type_name`, keeps the original implementation and MainModule identity
    /// assembly-local, and leaves all existing internal call sites valid. The public type therefore
    /// contains only intentional API signatures and trivial bridge bodies while the implementation
    /// remains free to call the linked Rust runtime.
    pub fn project_main_module_exports(&mut self, public_type_name: &str) -> usize {
        assert_ne!(
            public_type_name, MAIN_MODULE,
            "the public managed facade must not reuse the internal MainModule sentinel"
        );

        let main_module = self.main_module();
        let public_name = self.alloc_string(public_type_name);
        let public_class = self
            .class_def(ClassDef::new(
                public_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Extern,
                None,
                None,
                true,
            ))
            .unwrap_or_else(|error| {
                panic!("invalid public managed facade {public_type_name:?}: {error:?}")
            });

        let exports: Vec<_> = self
            .method_defs
            .iter()
            .filter(|(_, method)| {
                let name = &self[method.name()];
                method.class() == main_module
                    && *method.access() == Access::Extern
                    && !matches!(method.implementation(), MethodImpl::Extern { .. })
                    && !matches!(name.as_ref(), CCTOR | TCCTOR | USER_INIT)
            })
            .map(|(id, method)| (*id, method.clone()))
            .collect();

        for (original_id, mut projected) in exports.iter().cloned() {
            assert_eq!(
                projected.kind(),
                MethodKind::Static,
                "managed MainModule export {} must be static before facade projection",
                &self[projected.name()]
            );
            let original_ref = self.alloc_methodref(projected.ref_to());
            projected.set_class(public_class);
            let signature = self[projected.sig()].clone();
            let arguments: Vec<_> = (0..signature.inputs().len())
                .map(|index| self.alloc_node(CILNode::LdArg(index as u32)))
                .collect();
            let mut roots = Vec::with_capacity(2);
            if *signature.output() == Type::Void {
                roots.push(self.alloc_root(CILRoot::call(original_ref, arguments)));
                roots.push(self.alloc_root(CILRoot::VoidRet));
            } else {
                let call = self.call(original_ref, &arguments, IsPure::NOT);
                roots.push(self.alloc_root(CILRoot::Ret(call)));
            }
            let forwarding_body = MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(roots, 0, None)],
                locals: vec![],
            };

            let original = self
                .method_defs
                .get_mut(&original_id)
                .expect("snapshotted MainModule export");
            original.set_access(Access::Assembly);
            *projected.implementation_mut() = forwarding_body;
            self.new_method(projected.clone());
        }

        exports.len()
    }

    /// Project the internal `MainModule` sentinel to a configured public CLR type name.
    ///
    /// Kept separate from ordinary codegen so legacy artifact decoding and existing consumers keep
    /// their historical global `MainModule` surface unless a release package explicitly opts in.
    #[must_use]
    pub fn project_main_module(self, public_type_name: &str) -> Self {
        let mut source = self;
        let sections = std::mem::take(&mut source.sections);
        let native_imports = std::mem::take(&mut source.native_imports);
        let (mut projected, _) = super::asm_link::relocate_assembly_with_main_module_name(
            Self::default(),
            &source,
            public_type_name,
        );
        projected.sections = sections;
        projected.native_imports = native_imports;
        projected
    }

    #[must_use]
    pub fn link(self, other: Self) -> Self {
        self.link_with_stats(other).0
    }

    #[must_use]
    pub fn link_with_stats(mut self, other: Self) -> (Self, super::asm_link::RelocationStats) {
        let stats = self
            .try_link_in_place(other)
            .unwrap_or_else(|error| panic!("assembly link failed: {error}"));
        (self, stats)
    }

    fn link_with_stats_unchecked(self, other: Self) -> (Self, super::asm_link::RelocationStats) {
        let (mut linked, stats) = super::asm_link::relocate_assembly(self, &other);
        let link_preflight_index = linked.link_preflight_index.take();
        linked.sections.extend(other.sections);
        for import in other.native_imports {
            linked.add_native_import(import);
        }
        linked.link_preflight_index = link_preflight_index;
        (linked, stats)
    }

    fn rebuild_with_class_kind_overrides(
        mut self,
        overrides: &super::asm_link::ClassKindOverrides,
    ) -> Self {
        let sections = std::mem::take(&mut self.sections);
        let native_imports = std::mem::take(&mut self.native_imports);
        let (mut rebuilt, _) = super::asm_link::relocate_assembly_with_class_kind_overrides(
            Self::default(),
            &self,
            overrides.clone(),
        );
        rebuilt.sections = sections;
        rebuilt.native_imports = native_imports;
        rebuilt
    }

    /// Commits `other` into this assembly after a read-only semantic-conflict preflight.
    ///
    /// Every returned [`AssemblyLinkError`](super::asm_link::AssemblyLinkError) leaves `self`
    /// unchanged. Once preflight succeeds, relocation retains its existing fail-stop invariant
    /// behavior: an unexpected panic is not converted into a recoverable error.
    pub fn try_link_in_place(
        &mut self,
        mut other: Self,
    ) -> Result<super::asm_link::RelocationStats, super::asm_link::AssemblyLinkError> {
        let plan = super::asm_link::preflight_assembly_link_cached(self, &mut other)?;
        if plan.requires_class_kind_rebuild() {
            // Rebuild a staged clone of the parent so every retained reference adopts the sole
            // authoritative value kind. The original parent remains byte-for-byte untouched until
            // normalized graphs pass the complete preflight and link successfully.
            let mut normalized_parent = self
                .clone()
                .rebuild_with_class_kind_overrides(plan.class_kind_overrides());
            let mut normalized_other =
                other.rebuild_with_class_kind_overrides(plan.class_kind_overrides());
            let normalized_plan = super::asm_link::preflight_assembly_link_cached(
                &mut normalized_parent,
                &mut normalized_other,
            )?;
            assert!(
                !normalized_plan.requires_class_kind_rebuild(),
                "class-kind reconciliation did not reach a canonical fixed point"
            );
            let mut preflight_stats = plan.preflight_stats();
            preflight_stats.accumulate(normalized_plan.preflight_stats());
            let (linked, mut stats) = normalized_parent.link_with_stats_unchecked(normalized_other);
            stats.preflight = preflight_stats;
            *self = linked;
            return Ok(stats);
        }
        let destination = std::mem::take(self);
        let (linked, mut stats) = destination.link_with_stats_unchecked(other);
        stats.preflight = plan.preflight_stats();
        *self = linked;
        Ok(stats)
    }

    pub fn method_defs(&self) -> &FxHashMap<MethodDefIdx, MethodDef> {
        &self.method_defs
    }

    /// Checks if this assembly contains a reference [`ClassRef`]
    #[must_use]
    pub fn contains_ref(&self, cref: &ClassRef) -> bool {
        self.class_refs.contains_value(cref)
    }

    pub(crate) fn class_defs_mut_strings(
        &mut self,
    ) -> (&mut FxHashMap<ClassDefIdx, ClassDef>, &BiMap<IString>) {
        self.link_preflight_index = None;
        (&mut self.class_defs, &self.strings)
    }
    /// Iteates trough *all the nodes* in this assembly
    pub fn iter_nodes(&self) -> impl Iterator<Item = &CILNode> {
        self.nodes.values().iter()
    }
    /// Iterates trough *all the roots* in this assembly
    pub fn iter_roots(&self) -> impl Iterator<Item = &CILRoot> {
        self.roots.values().iter()
    }
    pub(crate) fn iter_type_values(&self) -> impl Iterator<Item = &Type> {
        self.types.values().iter()
    }
    pub(crate) fn iter_signatures(&self) -> impl Iterator<Item = &FnSig> {
        self.sigs.values().iter()
    }
    pub(crate) fn iter_class_refs(&self) -> impl Iterator<Item = &ClassRef> {
        self.class_refs.values().iter()
    }
    pub(crate) fn iter_class_ref_ids(
        &self,
    ) -> impl ExactSizeIterator<Item = Interned<ClassRef>> + DoubleEndedIterator {
        self.class_refs.ids()
    }
    pub(crate) fn iter_field_descs(&self) -> impl Iterator<Item = &FieldDesc> {
        self.fields.values().iter()
    }
    pub(crate) fn iter_static_field_descs(&self) -> impl Iterator<Item = &StaticFieldDesc> {
        self.statics.values().iter()
    }
    pub fn fix_alignment(&mut self, guaranteed_align: u8) {
        let method_def_idxs: Box<[_]> = self.method_defs.keys().copied().collect();
        for method in method_def_idxs {
            let mut tmp_method = self.borrow_methoddef(method);
            tmp_method.adjust_alignment(self, guaranteed_align);
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
        // Class/method/native identity keys embed string contents. `map_values` preserves interned
        // ids but changes their semantic values, so every cached key is stale afterward.
        self.link_preflight_index = None;
        self.strings.map_values(|string| {
            if string.len() > size_cap {
                eprint!("shortening {string}");
                *string = encode(hash64(string)).into();
                eprintln!("to {string}");
            }
        })
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

    #[cfg(test)]
    pub(crate) fn get_section(&self, arg: &str) -> Option<&Vec<u8>> {
        self.sections.get(arg)
    }

    pub(crate) fn global_void(&mut self) -> Interned<StaticFieldDesc> {
        let main = self.main_module();
        self.add_static(Type::Void, "global_void", false, main, None, true)
    }

    pub(crate) fn alloc_const_data(&mut self, data: &[u8]) -> Interned<Box<[u8]>> {
        self.const_data.alloc(data.into())
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
                    (flds.len() == 1 && flds[0].0 == dst_ty && matches!(flds[0].2, None | Some(0)))
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

    /// Adapts one wrapper argument from its emitted signature to the signature
    /// of the method it delegates to. All permitted conversions are explicit
    /// in the IR so final verification sees the same types the exporter will.
    pub fn adapt_call_argument(
        &mut self,
        argument: u32,
        source: Type,
        target: Type,
    ) -> Interned<CILNode> {
        if source == target {
            return self.alloc_node(CILNode::LdArg(argument));
        }

        match (target, source) {
            (
                Type::Ptr(_) | Type::Int(Int::ISize | Int::USize) | Type::FnPtr(_),
                Type::ClassRef(_),
            ) => {
                let address = self.alloc_node(CILNode::LdArgA(argument));
                let target = self.alloc_type(target);
                let address =
                    self.alloc_node(CILNode::PtrCast(address, Box::new(PtrCastRes::Ptr(target))));
                self.alloc_node(CILNode::LdInd {
                    addr: address,
                    tpe: target,
                    volatile: false,
                })
            }
            (Type::Ptr(_) | Type::Int(Int::ISize | Int::USize), Type::Int(Int::U64)) => {
                let input = self.alloc_node(CILNode::LdArg(argument));
                self.alloc_node(CILNode::IntCast {
                    input,
                    target: Int::USize,
                    extend: ExtendKind::ZeroExtend,
                })
            }
            (Type::Int(target @ (Int::ISize | Int::USize)), Type::Int(Int::ISize | Int::USize)) => {
                let input = self.alloc_node(CILNode::LdArg(argument));
                self.alloc_node(CILNode::IntCast {
                    input,
                    target,
                    extend: ExtendKind::ZeroExtend,
                })
            }
            (
                Type::Ptr(target),
                Type::Ptr(_) | Type::Int(Int::ISize | Int::USize) | Type::FnPtr(_),
            ) => {
                let input = self.alloc_node(CILNode::LdArg(argument));
                self.alloc_node(CILNode::PtrCast(input, Box::new(PtrCastRes::Ptr(target))))
            }
            (
                Type::FnPtr(target),
                Type::Ptr(_) | Type::Int(Int::ISize | Int::USize) | Type::FnPtr(_),
            ) => {
                let input = self.alloc_node(CILNode::LdArg(argument));
                self.alloc_node(CILNode::PtrCast(input, Box::new(PtrCastRes::FnPtr(target))))
            }
            (Type::Int(target @ (Int::ISize | Int::USize)), Type::Ptr(_) | Type::FnPtr(_)) => {
                let input = self.alloc_node(CILNode::LdArg(argument));
                self.alloc_node(CILNode::IntCast {
                    input,
                    target,
                    extend: ExtendKind::ZeroExtend,
                })
            }
            (Type::Int(Int::I64), Type::Int(Int::U64)) => self.alloc_node(CILNode::LdArg(argument)),
            _ => panic!("cannot adapt wrapper argument {argument} from {source:?} to {target:?}"),
        }
    }

    /// Adapts a delegated call's result back to the wrapper's emitted return
    /// type. This is the return-side counterpart of [`Self::adapt_call_argument`].
    pub fn adapt_call_result(
        &mut self,
        value: Interned<CILNode>,
        source: Type,
        target: Type,
    ) -> Interned<CILNode> {
        self.adapt_call_value(value, source, target)
    }

    /// Explicitly adapts an already-loaded value across a call boundary. This is the neutral,
    /// symmetric primitive behind return adaptation and builtin-generated call arguments; callers
    /// that start from a numbered wrapper argument can use [`Self::adapt_call_argument`].
    pub fn adapt_call_value(
        &mut self,
        value: Interned<CILNode>,
        source: Type,
        target: Type,
    ) -> Interned<CILNode> {
        if source == target {
            return value;
        }

        match (target, source) {
            (
                target @ (Type::Ptr(_)
                | Type::Ref(_)
                | Type::FnPtr(_)
                | Type::Int(Int::ISize | Int::USize)),
                Type::Ptr(_) | Type::Ref(_) | Type::FnPtr(_) | Type::Int(Int::ISize | Int::USize),
            ) => self.cast_ptr_to(value, target),
            (Type::Int(Int::I64), Type::Int(Int::U64))
            | (Type::Int(Int::U64), Type::Int(Int::I64)) => value,
            _ => {
                assert_eq!(
                    self.sizeof_type(source),
                    self.sizeof_type(target),
                    "cannot adapt wrapper return from {source:?} to {target:?}"
                );
                self.transmute_on_stack(source, target, value)
            }
        }
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
    /// `rustc_codegen_clr::r#type::fat_ptr_to`) from a thin data `ptr` and `metadata`, via the `create_slice`
    /// builtin. Used by the place pipeline (`src/place/body.rs`).
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
    pub fn ref_to_ptr(&mut self, val: impl IntoAsmIndex<Interned<CILNode>>) -> Interned<CILNode> {
        let val = val.into_idx(self);
        self.alloc_node(CILNode::RefToPtr(val))
    }

    /// Loads a pointer to the function `mref`.
    pub fn ld_ftn(&mut self, mref: impl IntoAsmIndex<Interned<MethodRef>>) -> Interned<CILNode> {
        let mref = mref.into_idx(self);
        self.alloc_node(CILNode::LdFtn(mref))
    }

    /// Produces a function pointer only when `real_ref` already has exactly `target_sig`.
    ///
    /// Signature adaptation cannot be inferred from `Type::Void`: that type represents both a
    /// physically present `RustVoid`/ZST parameter and a Rust ABI `PassMode::Ignore` slot. Callers
    /// which intentionally elide ABI slots must use [`Self::reify_fnptr_with_ignored`] and provide
    /// those slots explicitly.
    pub fn reify_fnptr(
        &mut self,
        real_ref: MethodRef,
        target_sig: Interned<FnSig>,
    ) -> Interned<CILNode> {
        assert_eq!(
            real_ref.sig(),
            target_sig,
            "reify_fnptr: signature adaptation requires explicit ignored ABI slots; \
             use reify_fnptr_with_ignored"
        );
        let method = self.alloc_methodref(real_ref);
        self.ld_ftn(method)
    }

    /// Reifies `real_ref` as `target_sig`, explicitly naming real-signature argument slots which
    /// are absent from the target function-pointer ABI. Explicit slots avoid guessing from
    /// `Type::Void`: an unignored Void/ZST slot is a real positional argument and consumes the
    /// corresponding target slot, while an ignored slot is synthesized inside the adapter.
    pub fn reify_fnptr_with_ignored(
        &mut self,
        real_ref: MethodRef,
        target_sig: Interned<FnSig>,
        ignored_real_slots: &[usize],
    ) -> Interned<CILNode> {
        let real_sig_idx = real_ref.sig();
        let real_sig = self[real_sig_idx].clone();
        let target_sig_val = self[target_sig].clone();
        let real_inputs = real_sig.inputs();
        let target_inputs = target_sig_val.inputs();
        assert_eq!(
            real_sig.output(),
            target_sig_val.output(),
            "reify_fnptr: adapter return types differ. real:{real_sig:?} target:{target_sig_val:?}"
        );

        let mut ignored = ignored_real_slots.to_vec();
        ignored.sort_unstable();
        assert!(
            ignored.windows(2).all(|pair| pair[0] != pair[1]),
            "reify_fnptr: duplicate ignored real slot"
        );
        assert!(
            ignored.iter().all(|slot| *slot < real_inputs.len()),
            "reify_fnptr: ignored real slot is out of range"
        );
        assert!(
            ignored.iter().all(|slot| real_inputs[*slot] == Type::Void),
            "reify_fnptr: only Type::Void ABI slots may be ignored"
        );
        if real_sig_idx == target_sig && ignored.is_empty() {
            let method = self.alloc_methodref(real_ref);
            return self.ld_ftn(method);
        }

        let mut target_idx = 0usize;
        let mut slot_map: Vec<Option<usize>> = Vec::with_capacity(real_inputs.len());
        for (real_idx, real) in real_inputs.iter().enumerate() {
            if ignored.binary_search(&real_idx).is_ok() {
                slot_map.push(None);
                continue;
            }
            let Some(target) = target_inputs.get(target_idx) else {
                panic!(
                    "reify_fnptr: real sig has more retained params than target sig. real:{real_inputs:?} target:{target_inputs:?} ignored:{ignored:?}"
                );
            };
            assert_eq!(
                *real, *target,
                "reify_fnptr: retained real param {real_idx} does not match target param \
                 {target_idx}. real:{real_inputs:?} target:{target_inputs:?} ignored:{ignored:?}"
            );
            slot_map.push(Some(target_idx));
            target_idx += 1;
        }
        assert_eq!(
            target_idx,
            target_inputs.len(),
            "reify_fnptr: target sig has params the real sig does not consume. \
             real:{real_inputs:?} target:{target_inputs:?} ignored:{ignored:?}"
        );
        // Key the adapter on the callee's full semantic identity rather than its display name.
        // Distinct owners and overloads routinely share a method name; using just that name and an
        // assembly-local signature index could silently alias their adapters after artifact merge.
        let real_name = self[real_ref.name()].to_string();
        let ignored_key = ignored
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join("_");
        let real_class = self[real_ref.class()].clone();
        let class_assembly = real_class
            .asm()
            .map(|name| self[name].to_string())
            .unwrap_or_default();
        let class_generics = real_class
            .generics()
            .iter()
            .map(|tpe| tpe.mangle(self))
            .collect::<Vec<_>>()
            .join(",");
        let real_inputs_key = real_inputs
            .iter()
            .map(|tpe| tpe.mangle(self))
            .collect::<Vec<_>>()
            .join(",");
        let real_generics = real_ref
            .generics()
            .iter()
            .map(|tpe| tpe.mangle(self))
            .collect::<Vec<_>>()
            .join(",");
        let target_inputs_key = target_inputs
            .iter()
            .map(|tpe| tpe.mangle(self))
            .collect::<Vec<_>>()
            .join(",");
        let semantic_key = format!(
            "{class_assembly}|{}|{}|{class_generics}|{real_name}|{:?}|{real_inputs_key}|{}|\
             {real_generics}|{target_inputs_key}|{}|{ignored_key}",
            &self[real_class.name()],
            real_class.is_valuetype(),
            real_ref.kind(),
            real_sig.output().mangle(self),
            target_sig_val.output().mangle(self),
        );
        let adapter_key = crate::utilis::encode(crate::calculate_hash(&semantic_key));
        let adapter_name = format!("{real_name}$fnptr_adapter${adapter_key}");
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
            .enumerate()
            .map(|(real_idx, slot)| match slot {
                Some(target_index) => self.alloc_node(CILNode::LdArg(*target_index as u32)),
                None => self.uninit_val(real_inputs[real_idx]),
            })
            .collect();
        let roots = if real_ret == Type::Void {
            let call = self.call_root(real_ref_idx, &args, IsPure::NOT);
            let ret = self.alloc_root(CILRoot::VoidRet);
            vec![call, ret]
        } else {
            let call = self.call(real_ref_idx, &args, IsPure::NOT);
            vec![self.alloc_root(CILRoot::Ret(call))]
        };
        let block = super::BasicBlock::new(roots, 0, None);
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
    pub fn ld_len(&mut self, arr: impl IntoAsmIndex<Interned<CILNode>>) -> Interned<CILNode> {
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

    /// Loads a typed value from a one-dimensional managed array.
    pub fn ld_elem(
        &mut self,
        array: impl IntoAsmIndex<Interned<CILNode>>,
        index: impl IntoAsmIndex<Interned<CILNode>>,
        elem: impl IntoAsmIndex<Interned<Type>>,
    ) -> Interned<CILNode> {
        let array = array.into_idx(self);
        let index = index.into_idx(self);
        let elem = elem.into_idx(self);
        self.alloc_node(CILNode::LdElem { array, index, elem })
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

    /// Boxes the value-type `value` of type `tpe` into a managed `System.Object` (`box <tpe>`).
    pub fn box_value(
        &mut self,
        value: impl IntoAsmIndex<Interned<CILNode>>,
        tpe: impl IntoAsmIndex<Interned<Type>>,
    ) -> Interned<CILNode> {
        let value = value.into_idx(self);
        let tpe = tpe.into_idx(self);
        self.alloc_node(CILNode::Box { value, tpe })
    }

    /// Allocates `size` bytes from the local (per-call) pool.
    pub fn loc_alloc(&mut self, size: impl IntoAsmIndex<Interned<CILNode>>) -> Interned<CILNode> {
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
    pub fn ld_type_token(&mut self, tpe: impl IntoAsmIndex<Interned<Type>>) -> Interned<CILNode> {
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
    pub fn stack_addr(&mut self, val: impl IntoAsmIndex<Interned<CILNode>>) -> Interned<CILNode> {
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

    /// Builds a root that throws a new exception of `class` with message `msg`.
    fn throw_exception_msg(&mut self, class: Interned<ClassRef>, msg: &str) -> Interned<CILRoot> {
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

    /// Builds the uncatchable terminal call required by rustc's `nounwind` panic entry points.
    fn fail_fast_msg(&mut self, msg: &str) -> Interned<CILRoot> {
        let environment = ClassRef::enviroment(self);
        let name = self.alloc_string("FailFast");
        let signature = self.sig([Type::PlatformString], Type::Void);
        let fail_fast = self.alloc_methodref(MethodRef::new(
            environment,
            name,
            signature,
            MethodKind::Static,
            vec![].into(),
        ));
        let msg = self.alloc_string(msg);
        let msg = self.alloc_node(CILNode::Const(Box::new(Const::PlatformString(msg))));
        self.call_root(fail_fast, &[msg], IsPure::NOT)
    }

    /// Builds a root that throws a new `Exception` with message `msg`.
    pub fn throw_msg(&mut self, msg: &str) -> Interned<CILRoot> {
        let class = ClassRef::exception(self);
        self.throw_exception_msg(class, msg)
    }
}
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

// Only exercised by the `test_chunked_range` unit test; unused in non-test builds.
#[allow(dead_code)]
fn chunked_range(top: u32, parts: u32) -> impl Iterator<Item = std::ops::Range<u32>> {
    let chunk_size = top.div_ceil(parts); // Ceiling of n / m

    assert!(parts < top);
    (0..top).filter_map(move |i| {
        let start = i * chunk_size;
        let end = std::cmp::min(start + chunk_size, top);
        if start < top { Some(start..end) } else { None }
    })
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
fn missing_placeholder_never_overwrites_a_real_method_definition() {
    let mut asm = Assembly::default();
    let class = asm.main_module();
    let name = asm.alloc_string("defined_before_placeholder");
    let sig = asm.sig([], Type::Void);
    let ret = asm.alloc_root(CILRoot::VoidRet);
    let idx = asm.new_method(MethodDef::new(
        Access::Public,
        class,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        },
        vec![],
    ));
    let placeholder = asm.new_method(MethodDef::new(
        Access::Public,
        class,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::Missing,
        vec![],
    ));
    assert_eq!(idx, placeholder);
    assert!(!matches!(
        asm.method_defs.get(&idx).unwrap().implementation(),
        MethodImpl::Missing
    ));
}

#[cfg(test)]
fn add_deliberately_ill_typed_method(asm: &mut Assembly) -> MethodDefIdx {
    let local_ty = asm.alloc_type(Type::Int(Int::USize));
    let value = asm.alloc_node(Const::F64(super::hashable::HashableF64(1.0)));
    let bad_store = asm.alloc_root(CILRoot::StLoc(0, value));
    let main = asm.main_module();
    let name = asm.alloc_string("deliberately_ill_typed");
    let sig = asm.sig([], Type::Void);
    asm.new_method(MethodDef::new(
        Access::Private,
        main,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![bad_store], 0, None)],
            locals: vec![(None, local_ty)],
        },
        vec![],
    ))
}

#[cfg(test)]
fn test_pe_options(name: &str) -> super::pe_exporter::export::ExportOptions {
    super::pe_exporter::export::ExportOptions {
        runtime: rust_dotnet_sdk_core::runtime::DotnetVersion::Net10,
        is_dll: true,
        assembly_name: name.to_string(),
        public_module_full_name: None,
        module_name: format!("{name}.dll"),
        pdb_file_name: String::new(),
    }
}

#[test]
fn fnptr_adapter_roots_void_call_before_return() {
    let mut asm = Assembly::default();
    let owner = asm.main_module();
    let real_sig = asm.sig([Type::Void, Type::Int(Int::I32)], Type::Void);
    let target_sig = asm.sig([Type::Int(Int::I32)], Type::Void);
    let name = asm.alloc_string("void_fnptr_target");
    let real = MethodRef::new(owner.0, name, real_sig, MethodKind::Static, [].into());
    asm.reify_fnptr_with_ignored(real, target_sig, &[0]);

    let adapter = asm
        .method_defs()
        .values()
        .find(|method| asm[method.name()].contains("$fnptr_adapter$"))
        .expect("adapter method");
    let MethodImpl::MethodBody { blocks, .. } = adapter.implementation() else {
        panic!("adapter must have a body");
    };
    assert_eq!(blocks[0].roots().len(), 2);
    assert!(matches!(asm[blocks[0].roots()[0]], CILRoot::Call(_)));
    assert_eq!(asm[blocks[0].roots()[1]], CILRoot::VoidRet);
}

#[test]
fn fnptr_adapter_ignores_only_explicit_slots() {
    let mut asm = Assembly::default();
    let owner = asm.main_module();
    let real_sig = asm.sig([Type::Void, Type::Void, Type::Int(Int::I32)], Type::Void);
    let target_sig = asm.sig([Type::Void, Type::Int(Int::I32)], Type::Void);
    let name = asm.alloc_string("explicit_fnptr_slots");
    let real = MethodRef::new(owner.0, name, real_sig, MethodKind::Static, [].into());
    asm.reify_fnptr_with_ignored(real, target_sig, &[1]);

    let adapter = asm
        .method_defs()
        .values()
        .find(|method| asm[method.name()].contains("$fnptr_adapter$"))
        .expect("adapter method");
    let MethodImpl::MethodBody { blocks, .. } = adapter.implementation() else {
        panic!("adapter must have a body");
    };
    let CILRoot::Call(call) = &asm[blocks[0].roots()[0]] else {
        panic!("void adapter must call before returning");
    };
    assert_eq!(call.1.len(), 3);
    assert_eq!(asm[call.1[0]], CILNode::LdArg(0));
    assert_eq!(asm[call.1[2]], CILNode::LdArg(1));
    assert_ne!(asm[call.1[1]], CILNode::LdArg(0));
    assert_ne!(asm[call.1[1]], CILNode::LdArg(1));
}

#[test]
fn fnptr_adapters_for_same_named_methods_do_not_alias() {
    let mut asm = Assembly::default();
    let owner_a_name = asm.alloc_string("AdapterOwnerA");
    let owner_b_name = asm.alloc_string("AdapterOwnerB");
    let owner_a = asm.alloc_class_ref(ClassRef::new(owner_a_name, None, false, [].into()));
    let owner_b = asm.alloc_class_ref(ClassRef::new(owner_b_name, None, false, [].into()));
    let real_sig = asm.sig([Type::Void], Type::Void);
    let target_sig = asm.sig([], Type::Void);
    let name = asm.alloc_string("same_name");

    for owner in [owner_a, owner_b] {
        let real = MethodRef::new(owner, name, real_sig, MethodKind::Static, [].into());
        asm.reify_fnptr_with_ignored(real, target_sig, &[0]);
    }

    let adapter_names: std::collections::HashSet<_> = asm
        .method_defs()
        .values()
        .filter_map(|method| {
            let name = &asm[method.name()];
            name.contains("$fnptr_adapter$").then(|| name.to_string())
        })
        .collect();
    assert_eq!(adapter_names.len(), 2);
}

#[test]
#[should_panic(expected = "signature adaptation requires explicit ignored ABI slots")]
fn fnptr_legacy_api_does_not_guess_ignored_void_slots() {
    let mut asm = Assembly::default();
    let owner = asm.main_module();
    let real_sig = asm.sig([Type::Void], Type::Void);
    let target_sig = asm.sig([], Type::Void);
    let name = asm.alloc_string("ambiguous_void_slot");
    let real = MethodRef::new(owner.0, name, real_sig, MethodKind::Static, [].into());
    asm.reify_fnptr(real, target_sig);
}

#[test]
fn final_export_verification_accepts_clean_assembly() {
    assert!(Assembly::default().prepared().verify_for_export().is_ok());
}

#[test]
fn optimizer_fuel_is_preallocated_per_method_and_hash_order_independent() {
    fn add_method(asm: &mut Assembly, name: &str, root_count: usize) -> MethodDefIdx {
        let owner = asm.main_module();
        let name = asm.alloc_string(name);
        let sig = asm.sig([], Type::Void);
        let nop = asm.alloc_root(CILRoot::Nop);
        let mut roots = vec![nop; root_count];
        roots.push(asm.alloc_root(CILRoot::VoidRet));
        asm.new_method(MethodDef::new(
            Access::Private,
            owner,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(roots, 0, None)],
                locals: vec![],
            },
            vec![],
        ))
    }

    let mut original = Assembly::default();
    let small = add_method(&mut original, "budget_small", 1);
    let large = add_method(&mut original, "budget_large", 4);
    let budgets: FxHashMap<_, _> = original.optimizer_method_budgets(96).into_iter().collect();
    assert!(budgets[&small] > 0);
    assert!(budgets[&large] > budgets[&small]);
    assert_eq!(budgets.values().sum::<u32>(), 96);

    let mut reordered = original.clone();
    let mut definitions: Vec<_> = reordered.method_defs.drain().collect();
    definitions.reverse();
    reordered.method_defs.extend(definitions);
    assert_eq!(
        original.optimizer_method_budgets(96),
        reordered.optimizer_method_budgets(96)
    );

    let mut original_fuel = OptFuel::new(96);
    let mut reordered_fuel = OptFuel::new(96);
    original.opt(&mut original_fuel);
    reordered.opt(&mut reordered_fuel);
    assert_eq!(original_fuel, reordered_fuel);
    assert_eq!(original.method_defs, reordered.method_defs);
    assert_eq!(original.nodes.values(), reordered.nodes.values());
    assert_eq!(original.roots.values(), reordered.roots.values());
}

#[test]
fn dce_keeps_method_metadata_types_and_override_targets() {
    let mut asm = Assembly::default();
    let owner = asm.main_module();
    let void_sig = asm.sig([], Type::Void);
    let ret = asm.alloc_root(CILRoot::VoidRet);

    let attribute_name = asm.alloc_string("OnlyMethodMetadataKeepsMe");
    let attribute = asm
        .class_def(ClassDef::new(
            attribute_name,
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

    let base_name = asm.alloc_string("metadata_base");
    let base = asm.new_method(MethodDef::new(
        Access::Private,
        owner,
        base_name,
        void_sig,
        MethodKind::Virtual,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        },
        vec![],
    ));

    let exported_name = asm.alloc_string("metadata_export");
    let exported = MethodDef::new(
        Access::Extern,
        owner,
        exported_name,
        void_sig,
        MethodKind::Virtual,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        },
        vec![],
    )
    .with_override(base.0)
    .with_custom_attributes(
        vec![super::class::CustomAttrDef::new(
            attribute.0,
            vec![],
            vec![],
        )],
        vec![],
        vec![],
    );
    asm.new_method(exported);

    asm.eliminate_dead_code();
    assert!(asm.method_defs().contains_key(&base));
    assert!(asm.class_defs().contains_key(&attribute));
}

#[test]
fn dce_drops_member_accessors_with_a_dead_owner() {
    let mut asm = Assembly::default();
    let owner_name = asm.alloc_string("DeadMemberOwner");
    let owner = asm
        .class_def(ClassDef::new(
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
        ))
        .unwrap();
    let void_sig = asm.sig([], Type::Void);
    let void_ret = asm.alloc_root(CILRoot::VoidRet);
    let void_body = || MethodImpl::MethodBody {
        blocks: vec![super::BasicBlock::new(vec![void_ret], 0, None)],
        locals: vec![],
    };
    let add_name = asm.alloc_string("add_Changed");
    let add = asm.new_method(MethodDef::new(
        Access::Private,
        owner,
        add_name,
        void_sig,
        MethodKind::Instance,
        void_body(),
        vec![],
    ));
    let remove_name = asm.alloc_string("remove_Changed");
    let remove = asm.new_method(MethodDef::new(
        Access::Private,
        owner,
        remove_name,
        void_sig,
        MethodKind::Instance,
        void_body(),
        vec![],
    ));
    let getter_sig = asm.sig([], Type::Int(Int::I32));
    let zero = asm.alloc_node(Const::I32(0));
    let getter_ret = asm.alloc_root(CILRoot::Ret(zero));
    let getter_name = asm.alloc_string("get_Value");
    let getter = asm.new_method(MethodDef::new(
        Access::Private,
        owner,
        getter_name,
        getter_sig,
        MethodKind::Instance,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![getter_ret], 0, None)],
            locals: vec![],
        },
        vec![],
    ));
    let event_name = asm.alloc_string("Changed");
    asm.class_mut(owner).add_event(EventDef::new(
        event_name,
        Type::PlatformObject,
        add.0,
        remove.0,
    ));
    let property_name = asm.alloc_string("Value");
    asm.class_mut(owner).add_property(PropertyDef::new(
        property_name,
        Type::Int(Int::I32),
        Some(getter.0),
        None,
    ));

    asm.eliminate_dead_code();

    assert!(!asm.class_defs().contains_key(&owner));
    assert!(!asm.method_defs().contains_key(&add));
    assert!(!asm.method_defs().contains_key(&remove));
    assert!(!asm.method_defs().contains_key(&getter));
}

#[test]
fn dce_keeps_member_accessors_for_an_owner_reached_through_a_live_signature() {
    let mut asm = Assembly::default();
    let owner_name = asm.alloc_string("SignatureMemberOwner");
    let owner = asm
        .class_def(ClassDef::new(
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
        ))
        .unwrap();
    let getter_sig = asm.sig([], Type::Int(Int::I32));
    let zero = asm.alloc_node(Const::I32(0));
    let getter_ret = asm.alloc_root(CILRoot::Ret(zero));
    let getter_name = asm.alloc_string("get_Value");
    let getter = asm.new_method(MethodDef::new(
        Access::Private,
        owner,
        getter_name,
        getter_sig,
        MethodKind::Instance,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![getter_ret], 0, None)],
            locals: vec![],
        },
        vec![],
    ));
    let property_name = asm.alloc_string("Value");
    asm.class_mut(owner).add_property(PropertyDef::new(
        property_name,
        Type::Int(Int::I32),
        Some(getter.0),
        None,
    ));

    let exported_sig = asm.sig([Type::ClassRef(owner.0)], Type::Void);
    let exported_ret = asm.alloc_root(CILRoot::VoidRet);
    let exported_name = asm.alloc_string("use_signature_owner");
    let main = asm.main_module();
    asm.new_method(MethodDef::new(
        Access::Extern,
        main,
        exported_name,
        exported_sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![exported_ret], 0, None)],
            locals: vec![],
        },
        vec![None],
    ));

    asm.eliminate_dead_code();

    assert!(asm.class_defs().contains_key(&owner));
    assert!(asm.method_defs().contains_key(&getter));
    assert_eq!(asm.class_defs()[&owner].properties().len(), 1);
}

#[test]
fn dce_follows_nested_generics_and_external_method_reference_metadata() {
    fn property_owner(asm: &mut Assembly, name: &str) -> (ClassDefIdx, MethodDefIdx) {
        let name = asm.alloc_string(name);
        let owner = asm
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
        let getter_name = asm.alloc_string("get_Value");
        let getter_sig = asm.sig([Type::ClassRef(owner.0)], Type::Int(Int::I32));
        let zero = asm.alloc_node(Const::I32(0));
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        let getter = asm.new_method(MethodDef::new(
            Access::Private,
            owner,
            getter_name,
            getter_sig,
            MethodKind::Instance,
            MethodImpl::MethodBody {
                blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            },
            vec![None],
        ));
        let property_name = asm.alloc_string("Value");
        asm.class_mut(owner).add_property(PropertyDef::new(
            property_name,
            Type::Int(Int::I32),
            Some(getter.0),
            None,
        ));
        (owner, getter)
    }

    let mut asm = Assembly::default();
    let (owner_payload, owner_getter) = property_owner(&mut asm, "OwnerGenericPayload");
    let (method_payload, method_getter) = property_owner(&mut asm, "MethodGenericPayload");
    let (signature_payload, signature_getter) = property_owner(&mut asm, "SignaturePayload");

    let external_assembly = asm.alloc_string("External.Api");
    let envelope_name = asm.alloc_string("External.Envelope`1");
    let envelope = asm.alloc_class_ref(ClassRef::new(
        envelope_name,
        Some(external_assembly),
        false,
        [Type::ClassRef(owner_payload.0)].into(),
    ));
    let api_name = asm.alloc_string("External.GenericApi`1");
    let api = asm.alloc_class_ref(ClassRef::new(
        api_name,
        Some(external_assembly),
        false,
        [Type::ClassRef(envelope)].into(),
    ));
    let invoke_name = asm.alloc_string("Invoke");
    let invoke_sig = asm.sig([], Type::Void);
    let invoke = asm.alloc_methodref(MethodRef::new(
        api,
        invoke_name,
        invoke_sig,
        MethodKind::Static,
        [Type::ClassRef(method_payload.0)].into(),
    ));
    let call = asm.call_root(invoke, &[] as &[Interned<CILNode>], IsPure::NOT);

    let capture_name = asm.alloc_string("Capture");
    let capture_sig = asm.sig([Type::ClassRef(signature_payload.0)], Type::Void);
    let capture = asm.alloc_methodref(MethodRef::new(
        api,
        capture_name,
        capture_sig,
        MethodKind::Static,
        [].into(),
    ));
    let function = asm.ld_ftn(capture);
    let pop_function = asm.alloc_root(CILRoot::Pop(function));
    let ret = asm.alloc_root(CILRoot::VoidRet);
    let exported_name = asm.alloc_string("exercise_external_metadata");
    let exported_sig = asm.sig([], Type::Void);
    let main = asm.main_module();
    asm.new_method(MethodDef::new(
        Access::Extern,
        main,
        exported_name,
        exported_sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(
                vec![call, pop_function, ret],
                0,
                None,
            )],
            locals: vec![],
        },
        vec![],
    ));

    asm.eliminate_dead_code();

    for (owner, getter) in [
        (owner_payload, owner_getter),
        (method_payload, method_getter),
        (signature_payload, signature_getter),
    ] {
        assert!(asm.class_defs().contains_key(&owner));
        assert!(asm.method_defs().contains_key(&getter));
        assert_eq!(asm.class_defs()[&owner].properties().len(), 1);
    }

    let ready = asm.verify_for_export().unwrap();
    let (image, _) = ready
        .try_render_pe(&test_pe_options("semantic-reachability"))
        .unwrap();
    assert_eq!(&image[..2], b"MZ");
}

#[test]
fn final_verifier_rejects_only_missing_methods_that_survive_dce() {
    let mut asm = Assembly::default().prepared();
    let owner = asm.main_module();
    let sig = asm.sig([], Type::Void);
    let dead_name = asm.alloc_string("dead_missing");
    asm.new_method(MethodDef::new(
        Access::Private,
        owner,
        dead_name,
        sig,
        MethodKind::Static,
        MethodImpl::Missing,
        vec![],
    ));
    asm.eliminate_dead_code();
    assert!(asm.verify_for_export().is_ok());

    let mut asm = Assembly::default().prepared();
    let owner = asm.main_module();
    let sig = asm.sig([], Type::Void);
    let live_name = asm.alloc_string("live_missing");
    asm.new_method(MethodDef::new(
        Access::Extern,
        owner,
        live_name,
        sig,
        MethodKind::Static,
        MethodImpl::Missing,
        vec![],
    ));
    asm.eliminate_dead_code();
    let Err(VerificationFailure::AssemblyInvariant { message }) = asm.verify_for_export() else {
        panic!("a reachable missing method must fail the final verifier")
    };
    assert!(message.contains("live_missing"), "{message}");
}

#[cfg(test)]
fn add_cfg_test_missing(asm: &mut Assembly, name: &str) -> MethodDefIdx {
    let owner = asm.main_module();
    let sig = asm.sig([], Type::Void);
    let name = asm.alloc_string(name);
    asm.new_method(MethodDef::new(
        Access::Private,
        owner,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::Missing,
        vec![],
    ))
}

#[cfg(test)]
fn add_cfg_test_export(
    asm: &mut Assembly,
    name: &str,
    inputs: impl Into<Box<[Type]>>,
    blocks: Vec<super::BasicBlock>,
) -> MethodDefIdx {
    let owner = asm.main_module();
    let sig = asm.sig(inputs, Type::Void);
    let name = asm.alloc_string(name);
    asm.new_method(MethodDef::new(
        Access::Extern,
        owner,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks,
            locals: vec![],
        },
        vec![],
    ))
}

#[test]
fn mandatory_cfg_cleanup_drops_roots_after_an_unconditional_transfer() {
    let mut asm = Assembly::default();
    let missing = add_cfg_test_missing(&mut asm, "dead_post_terminator_missing");
    let to_live = asm.branch(2, 0, None);
    let unreachable_default = asm.branch(1, 0, None);
    let call = asm.call_root(missing.0, &[] as &[Interned<CILNode>], IsPure::NOT);
    let ret = asm.alloc_root(CILRoot::VoidRet);
    let caller = add_cfg_test_export(
        &mut asm,
        "post_terminator_cfg",
        [],
        vec![
            super::BasicBlock::new(vec![to_live, unreachable_default], 0, None),
            super::BasicBlock::new(vec![call, ret], 1, None),
            super::BasicBlock::new(vec![ret], 2, None),
        ],
    );

    asm.eliminate_dead_code();

    assert!(!asm.method_defs().contains_key(&missing));
    let blocks = asm.method_defs()[&caller]
        .implementation()
        .blocks()
        .unwrap();
    assert_eq!(
        blocks
            .iter()
            .map(super::BasicBlock::block_id)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(blocks[0].roots(), &[to_live]);
}

#[test]
fn mandatory_cfg_cleanup_folds_const_false_before_call_graph_dce() {
    let mut asm = Assembly::default();
    let missing = add_cfg_test_missing(&mut asm, "const_false_missing");
    let false_node = asm.alloc_node(Const::Bool(false));
    let dead_branch = asm.branch(1, 0, Some(super::BranchCond::True(false_node)));
    let live_branch = asm.branch(2, 0, None);
    let call = asm.call_root(missing.0, &[] as &[Interned<CILNode>], IsPure::NOT);
    let ret = asm.alloc_root(CILRoot::VoidRet);
    add_cfg_test_export(
        &mut asm,
        "const_false_cfg",
        [],
        vec![
            super::BasicBlock::new(vec![dead_branch, live_branch], 0, None),
            super::BasicBlock::new(vec![call, ret], 1, None),
            super::BasicBlock::new(vec![ret], 2, None),
        ],
    );

    asm.eliminate_dead_code();
    assert!(!asm.method_defs().contains_key(&missing));
}

#[test]
fn mandatory_cfg_cleanup_folds_const_true_and_preserves_the_live_call() {
    let mut asm = Assembly::default();
    let missing = add_cfg_test_missing(&mut asm, "const_true_missing");
    let true_node = asm.alloc_node(Const::Bool(true));
    let live_branch = asm.branch(1, 0, Some(super::BranchCond::True(true_node)));
    let dead_default = asm.branch(2, 0, None);
    let call = asm.call_root(missing.0, &[] as &[Interned<CILNode>], IsPure::NOT);
    let ret = asm.alloc_root(CILRoot::VoidRet);
    add_cfg_test_export(
        &mut asm,
        "const_true_cfg",
        [],
        vec![
            super::BasicBlock::new(vec![live_branch, dead_default], 0, None),
            super::BasicBlock::new(vec![call, ret], 1, None),
            super::BasicBlock::new(vec![ret], 2, None),
        ],
    );

    asm.eliminate_dead_code();
    assert!(asm.method_defs().contains_key(&missing));
}

#[test]
fn mandatory_cfg_cleanup_retains_both_edges_of_a_dynamic_branch() {
    let mut asm = Assembly::default();
    let missing = add_cfg_test_missing(&mut asm, "dynamic_branch_missing");
    let dynamic = asm.alloc_node(CILNode::LdArg(0));
    let conditional = asm.branch(1, 0, Some(super::BranchCond::True(dynamic)));
    let fallthrough = asm.branch(2, 0, None);
    let call = asm.call_root(missing.0, &[] as &[Interned<CILNode>], IsPure::NOT);
    let ret = asm.alloc_root(CILRoot::VoidRet);
    add_cfg_test_export(
        &mut asm,
        "dynamic_cfg",
        [Type::Bool],
        vec![
            super::BasicBlock::new(vec![conditional, fallthrough], 0, None),
            super::BasicBlock::new(vec![call, ret], 1, None),
            super::BasicBlock::new(vec![ret], 2, None),
        ],
    );

    asm.eliminate_dead_code();
    assert!(asm.method_defs().contains_key(&missing));
}

#[test]
fn mandatory_cfg_cleanup_drops_an_unrooted_block_cycle() {
    let mut asm = Assembly::default();
    let missing = add_cfg_test_missing(&mut asm, "dead_cycle_missing");
    let entry_to_exit = asm.branch(3, 0, None);
    let one_to_two = asm.branch(2, 0, None);
    let two_to_one = asm.branch(1, 0, None);
    let call = asm.call_root(missing.0, &[] as &[Interned<CILNode>], IsPure::NOT);
    let ret = asm.alloc_root(CILRoot::VoidRet);
    let caller = add_cfg_test_export(
        &mut asm,
        "dead_cycle_cfg",
        [],
        vec![
            super::BasicBlock::new(vec![entry_to_exit], 0, None),
            super::BasicBlock::new(vec![call, one_to_two], 1, None),
            super::BasicBlock::new(vec![two_to_one], 2, None),
            super::BasicBlock::new(vec![ret], 3, None),
        ],
    );

    asm.eliminate_dead_code();

    assert!(!asm.method_defs().contains_key(&missing));
    let blocks = asm.method_defs()[&caller]
        .implementation()
        .blocks()
        .unwrap();
    assert_eq!(
        blocks
            .iter()
            .map(super::BasicBlock::block_id)
            .collect::<Vec<_>>(),
        vec![0, 3]
    );
}

#[test]
fn mandatory_cfg_cleanup_preserves_live_exception_region_graphs() {
    let mut asm = Assembly::default();
    let owner = asm.main_module();
    let sig = asm.sig([], Type::Void);
    let to_exit = asm.branch(2, 0, None);
    let cleanup_next = asm.branch(11, 0, None);
    let ret = asm.alloc_root(CILRoot::VoidRet);
    let rethrow = asm.alloc_root(CILRoot::ReThrow);
    let name = asm.alloc_string("region_cfg");
    let method = asm.new_method(MethodDef::new(
        Access::Extern,
        owner,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::RegionBody {
            blocks: vec![
                super::BasicBlock::new(vec![to_exit], 0, None),
                super::BasicBlock::new(vec![ret], 1, None),
                super::BasicBlock::new(vec![ret], 2, None),
            ],
            cleanup_blocks: vec![
                super::BasicBlock::new(vec![cleanup_next], 10, None),
                super::BasicBlock::new(vec![rethrow], 11, None),
                super::BasicBlock::new(vec![rethrow], 20, None),
            ],
            exception_regions: vec![
                super::ExceptionRegion::new(0, 10),
                super::ExceptionRegion::new(1, 20),
            ],
            locals: vec![],
        },
        vec![],
    ));

    asm.canonicalize_control_flow();

    let MethodImpl::RegionBody {
        blocks,
        cleanup_blocks,
        exception_regions,
        ..
    } = asm.method_defs()[&method].implementation()
    else {
        panic!("region body changed representation")
    };
    assert_eq!(
        blocks
            .iter()
            .map(super::BasicBlock::block_id)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(
        cleanup_blocks
            .iter()
            .map(super::BasicBlock::block_id)
            .collect::<Vec<_>>(),
        vec![10, 11]
    );
    assert_eq!(exception_regions, &[super::ExceptionRegion::new(0, 10)]);
}

#[test]
fn dead_type_elimination_prunes_unreachable_external_declarations() {
    let mut asm = Assembly::default();
    let external_assembly = asm.alloc_string("System.Runtime.Intrinsics");
    let dead_name = asm.alloc_string("System.Runtime.Intrinsics.Vector512");
    let dead_ref = asm.alloc_class_ref(ClassRef::new(
        dead_name,
        Some(external_assembly),
        true,
        [].into(),
    ));
    let dead_def = ClassDef::new(
        dead_name,
        true,
        0,
        None,
        vec![],
        vec![],
        Access::Extern,
        None,
        None,
        false,
    );
    asm.class_defs.insert(ClassDefIdx(dead_ref), dead_def);

    let kept_name = asm.alloc_string("ExportedType");
    let kept_ref = asm.alloc_class_ref(ClassRef::new(kept_name, None, true, [].into()));
    asm.class_defs.insert(
        ClassDefIdx(kept_ref),
        ClassDef::new(
            kept_name,
            true,
            0,
            None,
            vec![],
            vec![],
            Access::Extern,
            None,
            None,
            false,
        ),
    );

    let reachable_name = asm.alloc_string("System.Runtime.Intrinsics.Vector128");
    let reachable_ref = asm.alloc_class_ref(ClassRef::new(
        reachable_name,
        Some(external_assembly),
        true,
        [].into(),
    ));
    asm.class_defs.insert(
        ClassDefIdx(reachable_ref),
        ClassDef::new(
            reachable_name,
            true,
            0,
            None,
            vec![],
            vec![],
            Access::Extern,
            None,
            None,
            false,
        ),
    );
    let main = asm.main_module();
    let method_name = asm.alloc_string("uses_vector128");
    let sig = asm.sig([], Type::ClassRef(reachable_ref));
    asm.new_method(MethodDef::new(
        Access::Public,
        main,
        method_name,
        sig,
        MethodKind::Static,
        MethodImpl::Missing,
        vec![],
    ));

    asm.eliminate_dead_types();
    assert!(!asm.class_defs.contains_key(&ClassDefIdx(dead_ref)));
    assert!(asm.class_defs.contains_key(&ClassDefIdx(kept_ref)));
    assert!(asm.class_defs.contains_key(&ClassDefIdx(reachable_ref)));
}

#[test]
fn facade_dce_drops_unrelated_exported_types() {
    let mut asm = Assembly::default();
    let dead_name = asm.alloc_string("DeadRuntimeType");
    let dead = asm
        .class_def(ClassDef::new(
            dead_name,
            true,
            0,
            None,
            vec![],
            vec![],
            Access::Extern,
            None,
            None,
            false,
        ))
        .unwrap();
    let facade_name = asm.alloc_string("Game.Exports");
    let facade = asm
        .class_def(ClassDef::new(
            facade_name,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Extern,
            None,
            None,
            true,
        ))
        .unwrap();
    let dto_name = asm.alloc_string("Game.Snapshot");
    let dto = asm
        .class_def(ClassDef::new(
            dto_name,
            false,
            0,
            None,
            vec![],
            vec![],
            Access::Extern,
            None,
            None,
            true,
        ))
        .unwrap();
    let ctor_name = asm.alloc_string(".ctor");
    let ctor_sig = asm.sig([], Type::Void);
    let ctor_ret = asm.alloc_root(CILRoot::VoidRet);
    let ctor = asm.new_method(MethodDef::new(
        Access::Extern,
        dto,
        ctor_name,
        ctor_sig,
        MethodKind::Constructor,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![ctor_ret], 0, None)],
            locals: vec![],
        },
        vec![],
    ));
    let getter_name = asm.alloc_string("get_Score");
    let getter_sig = asm.sig([], Type::Int(Int::I32));
    let score = asm.alloc_node(Const::I32(7));
    let getter_ret = asm.alloc_root(CILRoot::Ret(score));
    let getter = asm.new_method(MethodDef::new(
        Access::Extern,
        dto,
        getter_name,
        getter_sig,
        MethodKind::Instance,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![getter_ret], 0, None)],
            locals: vec![],
        },
        vec![],
    ));
    let property_name = asm.alloc_string("Score");
    asm.class_mut(dto).add_property(PropertyDef::new(
        property_name,
        Type::Int(Int::I32),
        Some(getter.0),
        None,
    ));
    let method_name = asm.alloc_string("answer");
    let sig = asm.sig([], Type::Int(Int::I32));
    let answer = asm.alloc_node(Const::I32(42));
    let ret = asm.alloc_root(CILRoot::Ret(answer));
    asm.new_method(MethodDef::new(
        Access::Extern,
        facade,
        method_name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        },
        vec![],
    ));

    asm.eliminate_dead_code_after_facade_projection("Game.Exports");

    assert!(!asm.class_defs.contains_key(&dead));
    assert!(asm.class_defs.contains_key(&facade));
    assert!(asm.class_defs.contains_key(&dto));
    assert!(asm.method_defs.contains_key(&ctor));
    assert!(asm.method_defs.contains_key(&getter));
    assert_eq!(asm.method_defs.len(), 3);
}

#[test]
fn unity_legacy_128_helpers_are_well_typed_and_internal() {
    let mut asm = Assembly::default();
    asm.install_unity_legacy_128_types();

    assert_eq!(asm.typecheck_with_policy(true, true), 0);
    for type_name in ["System.Int128", "System.UInt128"] {
        let class = asm
            .class_defs()
            .values()
            .find(|class| &asm[class.name()] == type_name)
            .expect("compatibility type must exist");
        assert_eq!(*class.access(), Access::Private);
        assert_eq!(
            class.explict_size().map(std::num::NonZeroU32::get),
            Some(16)
        );
        assert!(
            class
                .methods()
                .iter()
                .all(|method| *asm[*method].access() == Access::Assembly)
        );
    }
    let before = asm.class_defs().len();
    asm.install_unity_legacy_128_types();
    assert_eq!(asm.class_defs().len(), before, "installation is idempotent");
}

#[test]
fn main_module_visibility_keeps_only_explicit_exports_public() {
    let mut asm = Assembly::default();
    let main = asm.main_module();
    let sig = asm.sig([], Type::Void);
    for (name, access) in [
        ("linked_helper", Access::Public),
        ("ordinary_rust", Access::Assembly),
        ("managed_export", Access::Extern),
    ] {
        let name = asm.alloc_string(name);
        asm.new_method(MethodDef::new(
            access,
            main,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![],
        ));
    }

    assert_eq!(asm.hide_main_module_implementation_details(), 1);
    let access_by_name: FxHashMap<_, _> = asm
        .method_defs()
        .values()
        .map(|method| (asm[method.name()].to_string(), *method.access()))
        .collect();
    assert_eq!(access_by_name["linked_helper"], Access::Assembly);
    assert_eq!(access_by_name["ordinary_rust"], Access::Assembly);
    assert_eq!(access_by_name["managed_export"], Access::Extern);
}

#[test]
fn public_facade_contains_only_explicit_exports() {
    let mut asm = Assembly::default();
    let main = asm.main_module();
    let sig = asm.sig([], Type::Int(Int::I32));
    for (name, access) in [
        ("runtime_helper_with_newer_bcl_types", Access::Public),
        ("sample_value", Access::Extern),
    ] {
        let name = asm.alloc_string(name);
        asm.new_method(MethodDef::new(
            access,
            main,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![],
        ));
    }

    assert_eq!(asm.hide_main_module_implementation_details(), 1);
    assert_eq!(
        asm.project_main_module_exports("Rust.Unity.Sample.Exports"),
        1
    );

    let mut methods: Vec<_> = asm
        .method_defs()
        .values()
        .map(|method| {
            (
                asm[asm[method.class()].name()].to_string(),
                asm[method.name()].to_string(),
                *method.access(),
                matches!(method.implementation(), MethodImpl::MethodBody { .. }),
            )
        })
        .collect();
    methods.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
    assert_eq!(
        methods,
        vec![
            (
                "MainModule".to_string(),
                "runtime_helper_with_newer_bcl_types".to_string(),
                Access::Assembly,
                false,
            ),
            (
                "MainModule".to_string(),
                "sample_value".to_string(),
                Access::Assembly,
                false,
            ),
            (
                "Rust.Unity.Sample.Exports".to_string(),
                "sample_value".to_string(),
                Access::Extern,
                true,
            ),
        ]
    );
}

#[test]
fn optimizer_method_schedule_uses_semantic_order() {
    let mut asm = Assembly::default();
    let main = asm.main_module();
    let sig = asm.sig([], Type::Void);
    for name in ["zeta", "alpha", "middle"] {
        let name = asm.alloc_string(name);
        asm.new_method(MethodDef::new(
            Access::Private,
            main,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![],
        ));
    }

    let names: Vec<_> = asm
        .stable_method_def_idxs()
        .into_iter()
        .map(|method| asm[asm[method].name()].to_string())
        .collect();
    assert_eq!(names, ["alpha", "middle", "zeta"]);
}

#[test]
fn constructed_generic_overload_order_is_total_and_reproducible() {
    fn assembly(reverse: bool) -> Assembly {
        let mut asm = Assembly::default();
        let external = asm.alloc_string("System.Collections");
        let list_name = asm.alloc_string("System.Collections.Generic.List`1");
        let list_i32 = asm.alloc_class_ref(ClassRef::new(
            list_name,
            Some(external),
            false,
            [Type::Int(Int::I32)].into(),
        ));
        let list_string = asm.alloc_class_ref(ClassRef::new(
            list_name,
            Some(external),
            false,
            [Type::PlatformString].into(),
        ));
        let nop = asm.alloc_root(CILRoot::Nop);
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let name = asm.alloc_string("same_name");
        let owner = asm.main_module();
        let mut overloads = vec![(list_i32, 2_usize), (list_string, 5_usize)];
        if reverse {
            overloads.reverse();
        }
        for (parameter, root_count) in overloads {
            let sig = asm.sig([Type::ClassRef(parameter)], Type::Void);
            let mut roots = vec![nop; root_count - 1];
            roots.push(ret);
            asm.new_method(MethodDef::new(
                Access::Private,
                owner,
                name,
                sig,
                MethodKind::Static,
                MethodImpl::MethodBody {
                    blocks: vec![super::BasicBlock::new(roots, 0, None)],
                    locals: vec![],
                },
                vec![None],
            ));
        }
        asm
    }

    fn semantic_budgets(asm: &Assembly, total: u32) -> std::collections::BTreeMap<Vec<u8>, u32> {
        asm.optimizer_method_budgets(total)
            .into_iter()
            .map(|(method, budget)| (asm.method_semantic_key(method), budget))
            .collect()
    }

    let mut forward = assembly(false);
    let mut reverse = assembly(true);
    assert_eq!(
        semantic_budgets(&forward, 73),
        semantic_budgets(&reverse, 73)
    );

    let mut forward_fuel = OptFuel::new(73);
    let mut reverse_fuel = OptFuel::new(73);
    forward.opt(&mut forward_fuel);
    reverse.opt(&mut reverse_fuel);
    assert_eq!(forward_fuel, reverse_fuel);
    assert_eq!(forward.arena_counts(), reverse.arena_counts());

    let forward = forward
        .verify_for_export()
        .unwrap()
        .try_render_pe(&test_pe_options("generic-overload-order"))
        .unwrap();
    let reverse = reverse
        .verify_for_export()
        .unwrap()
        .try_render_pe(&test_pe_options("generic-overload-order"))
        .unwrap();
    assert_eq!(forward, reverse);
}

#[test]
fn fixed_array_layout_verifier_rejects_representation_expanded_elements() {
    let mut asm = Assembly::default();
    let element_name = asm.alloc_string("ExpandedElement");
    let element = asm.alloc_class_ref(ClassRef::new(element_name, None, true, [].into()));
    asm.class_def(ClassDef::new(
        element_name,
        true,
        0,
        None,
        vec![],
        vec![],
        Access::Public,
        std::num::NonZeroU32::new(24),
        std::num::NonZeroU32::new(8),
        false,
    ))
    .unwrap();

    let array = ClassRef::fixed_array_with_layout(Type::ClassRef(element), 2, 32, 8, &mut asm);
    let array_name = asm.class_ref(array).name();
    asm.class_def(
        ClassDef::new(
            array_name,
            true,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            std::num::NonZeroU32::new(32),
            std::num::NonZeroU32::new(8),
            true,
        )
        .with_fixed_array_layout(FixedArrayLayout::new(
            Type::ClassRef(element),
            2,
            32,
            32,
            8,
        )),
    )
    .unwrap();

    let error = asm.validate_fixed_array_layouts().unwrap_err();
    assert!(error.contains("representation-expanded element"));
    assert!(matches!(
        asm.verify_for_export(),
        Err(VerificationFailure::AssemblyInvariant { .. })
    ));
}

#[test]
fn fixed_array_layout_validation_is_link_order_independent() {
    fn owner_shard() -> Assembly {
        let mut asm = Assembly::default();
        let element_name = asm.alloc_string("CrossShardExpandedElement");
        let element = asm.alloc_class_ref(ClassRef::new(element_name, None, true, [].into()));
        asm.class_def(ClassDef::new(
            element_name,
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
        let array = ClassRef::fixed_array_with_layout(Type::ClassRef(element), 2, 32, 8, &mut asm);
        let array_name = asm.class_ref(array).name();
        asm.class_def(
            ClassDef::new(
                array_name,
                true,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                std::num::NonZeroU32::new(32),
                std::num::NonZeroU32::new(8),
                true,
            )
            .with_fixed_array_layout(FixedArrayLayout::new(
                Type::ClassRef(element),
                2,
                32,
                32,
                8,
            )),
        )
        .unwrap();
        asm
    }

    fn definition_shard() -> Assembly {
        let mut asm = Assembly::default();
        let name = asm.alloc_string("CrossShardExpandedElement");
        asm.alloc_class_ref(ClassRef::new(name, None, true, [].into()));
        asm.class_def(ClassDef::new(
            name,
            true,
            0,
            None,
            vec![],
            vec![],
            Access::Public,
            std::num::NonZeroU32::new(24),
            std::num::NonZeroU32::new(8),
            false,
        ))
        .unwrap();
        asm
    }

    for linked in [
        owner_shard().link(definition_shard()),
        definition_shard().link(owner_shard()),
    ] {
        let error = linked.validate_fixed_array_layouts().unwrap_err();
        assert!(error.contains("representation-expanded element"));
    }
}

#[test]
fn final_export_verification_returns_structured_failure() {
    let mut asm = Assembly::default().prepared();
    let bad_method = add_deliberately_ill_typed_method(&mut asm);
    let Err(error) = asm.verify_for_export() else {
        panic!("an ill-typed method must not receive an export-ready seal");
    };
    let VerificationFailure::Method {
        method,
        method_name,
        error,
    } = error
    else {
        panic!("expected a method verification failure")
    };
    assert_eq!(method, bad_method);
    assert_eq!(method_name, "deliberately_ill_typed");
    assert!(matches!(error, TypeCheckError::LocalAssigementWrong { .. }));
}

#[test]
fn direct_pe_render_reverifies_before_releasing_artifacts() {
    let ready = Assembly::default().prepared().verify_for_export().unwrap();
    let result = ready.render_with_reverification(|asm| {
        add_deliberately_ill_typed_method(asm);
        (vec![0x4d, 0x5a], Vec::<u8>::new())
    });
    assert!(
        result.is_err(),
        "post-render invalidation must discard PE bytes"
    );
}

#[test]
fn direct_pe_preflight_returns_structured_error_before_rendering() {
    use std::cell::Cell;

    let mut asm = Assembly::default().prepared();
    let input = asm.alloc_node(Const::I32(1));
    let unsupported = asm.alloc_node(CILNode::IntCast {
        input,
        target: Int::I128,
        extend: ExtendKind::SignExtend,
    });
    let pop = asm.alloc_root(CILRoot::Pop(unsupported));
    let ret = asm.alloc_root(CILRoot::VoidRet);
    let main = asm.main_module();
    let name = asm.alloc_string("retained_unsupported_cast");
    let sig = asm.sig([], Type::Void);
    asm.new_method(MethodDef::new(
        Access::Private,
        main,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(vec![pop, ret], 0, None)],
            locals: vec![],
        },
        vec![],
    ));
    let ready = asm.verify_for_export().unwrap();
    let rendered = Cell::new(false);
    let result = ready.render_with_reverification(|_| rendered.set(true));
    assert!(matches!(result, Err(PeEmissionError::Target(_))));
    assert!(
        !rendered.get(),
        "unsupported IR must not enter the byte writer"
    );
}

#[test]
fn direct_pe_preflight_ignores_unreachable_interned_constructs() {
    let mut asm = Assembly::default().prepared();
    let input = asm.alloc_node(Const::I32(1));
    asm.alloc_node(CILNode::IntCast {
        input,
        target: Int::I128,
        extend: ExtendKind::SignExtend,
    });
    asm.sig([], Type::Float(super::Float::F128));

    let (image, _) = asm
        .verify_for_export()
        .unwrap()
        .try_render_pe(&test_pe_options("dead-preflight-ir"))
        .unwrap();
    assert_eq!(&image[..2], b"MZ");
}

#[test]
fn missing_method_resolution_reaches_fixed_point() {
    use std::{cell::Cell, rc::Rc};

    fn void_body(asm: &mut Assembly, before_return: Option<Interned<CILRoot>>) -> MethodImpl {
        let mut roots = Vec::new();
        roots.extend(before_return);
        roots.push(asm.alloc_root(CILRoot::VoidRet));
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(roots, 0, None)],
            locals: vec![],
        }
    }

    let mut asm = Assembly::default();
    let first = Interned::<MethodRef>::builtin(&mut asm, "first", &[], Type::Void);
    let first_name = asm.alloc_string("first");
    let second_name = asm.alloc_string("second");
    let third_name = asm.alloc_string("third");
    let first_calls = Rc::new(Cell::new(0));
    let second_calls = Rc::new(Cell::new(0));
    let third_calls = Rc::new(Cell::new(0));

    let mut overrides = MissingMethodPatcher::default();
    let calls = Rc::clone(&first_calls);
    overrides.insert(
        first_name,
        Box::new(move |_, asm| {
            calls.set(calls.get() + 1);
            let second = Interned::<MethodRef>::builtin(asm, "second", &[], Type::Void);
            let call = asm.call_root(second, &[] as &[Interned<CILNode>], IsPure::NOT);
            void_body(asm, Some(call))
        }),
    );
    let calls = Rc::clone(&second_calls);
    overrides.insert(
        second_name,
        Box::new(move |_, asm| {
            calls.set(calls.get() + 1);
            let third = Interned::<MethodRef>::builtin(asm, "third", &[], Type::Void);
            let call = asm.call_root(third, &[] as &[Interned<CILNode>], IsPure::NOT);
            void_body(asm, Some(call))
        }),
    );
    let calls = Rc::clone(&third_calls);
    overrides.insert(
        third_name,
        Box::new(move |_, asm| {
            calls.set(calls.get() + 1);
            void_body(asm, None)
        }),
    );

    let externs: FxHashMap<&str, String> = FxHashMap::default();
    let modifies_errno: FxHashSet<&str> = FxHashSet::default();
    let stats = asm.resolve_missing_methods(&externs, &modifies_errno, &overrides);

    assert_eq!(stats.method_refs_processed, 3);
    assert_eq!(stats.method_refs_added, 2);
    assert_eq!(stats.overrides_applied, 3);
    assert_eq!(stats.unresolved_missing_methods, 0);
    assert_eq!(first_calls.get(), 1);
    assert_eq!(second_calls.get(), 1);
    assert_eq!(third_calls.get(), 1);
    assert!(matches!(
        asm[asm.method_ref_to_def(first).unwrap()].implementation(),
        MethodImpl::MethodBody { .. }
    ));
}

#[test]
fn missing_method_resolution_only_aliases_allowlisted_rustc_runtime_symbols() {
    fn void_body(asm: &mut Assembly) -> MethodImpl {
        MethodImpl::MethodBody {
            blocks: vec![super::BasicBlock::new(
                vec![asm.alloc_root(CILRoot::VoidRet)],
                0,
                None,
            )],
            locals: vec![],
        }
    }

    let mut asm = Assembly::default();
    let mangled_alloc = "_RNvCsk5I9pfA249o_7___rustc12___rust_alloc";
    assert_eq!(
        RuntimeService::classify(mangled_alloc),
        Some(RuntimeService::Alloc),
        "unexpected demangling: {}",
        rustc_demangle::demangle(mangled_alloc)
    );
    let alloc_ref = Interned::<MethodRef>::builtin(&mut asm, mangled_alloc, &[], Type::Void);
    let posix_write = Interned::<MethodRef>::builtin(&mut asm, "write", &[], Type::Void);
    let rust_write = Interned::<MethodRef>::builtin(&mut asm, "core::fmt::write", &[], Type::Void);

    let write_name = asm.alloc_string("write");
    let mut overrides = MissingMethodPatcher::default();
    overrides.insert_runtime_service(
        &mut asm,
        RuntimeService::Alloc,
        Box::new(|_, asm| void_body(asm)),
    );
    overrides.insert(write_name, Box::new(|_, asm| void_body(asm)));

    let stats =
        asm.resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &overrides);

    assert_eq!(stats.overrides_applied, 2);
    assert_eq!(stats.allocator_shims_synthesized, 1);
    assert!(matches!(
        asm[asm.method_ref_to_def(alloc_ref).unwrap()].implementation(),
        MethodImpl::MethodBody { .. }
    ));
    assert!(matches!(
        asm[asm.method_ref_to_def(posix_write).unwrap()].implementation(),
        MethodImpl::MethodBody { .. }
    ));
    assert!(matches!(
        asm[asm.method_ref_to_def(rust_write).unwrap()].implementation(),
        MethodImpl::Missing
    ));
}

#[test]
fn panic_runtime_service_classification_is_exact_and_covers_the_pinned_set() {
    for kind in PanicKind::ALL {
        assert_eq!(
            RuntimeService::classify(kind.canonical_symbol()),
            Some(RuntimeService::Panic(kind)),
            "unclassified pinned panic service {}",
            kind.canonical_symbol()
        );
    }

    // Exact symbols observed from the pinned toolchain's direct-rustc fixtures. Alternate
    // demangling removes the crate disambiguator but retains the complete module path.
    assert_eq!(
        RuntimeService::classify("_RNvNtCsh6vCWCGbp4W_4core9panicking18panic_bounds_check"),
        Some(RuntimeService::Panic(PanicKind::BoundsCheck))
    );
    assert_eq!(
        RuntimeService::classify("_RNvNtCs5bnaVnPOCY4_4core9panicking5panic"),
        Some(RuntimeService::Panic(PanicKind::Explicit))
    );
    assert_eq!(
        RuntimeService::classify(
            "_RNvNtNtCs5bnaVnPOCY4_4core9panicking11panic_const23panic_const_rem_by_zero"
        ),
        Some(RuntimeService::Panic(PanicKind::RemByZero))
    );

    for unrelated in [
        "my_crate::panic_bounds_check",
        "my_crate::panic_const::panic_const_rem_by_zero",
        "core::fmt::panic_bounds_check",
        "core::panicking::panic_const::panic_const_rem_by_zero_suffix",
    ] {
        assert_eq!(RuntimeService::classify(unrelated), None, "{unrelated}");
    }

    assert_eq!(
        RuntimeService::classify("_Unwind_FindEnclosingFunction"),
        Some(RuntimeService::UnwindFindEnclosingFunction)
    );
    assert_eq!(
        RuntimeService::classify("_Unwind_GetCFA"),
        Some(RuntimeService::UnwindGetCfa)
    );
    assert_eq!(
        RuntimeService::classify("_Unwind_GetIP"),
        Some(RuntimeService::UnwindGetIp)
    );
    assert_eq!(
        RuntimeService::classify("_Unwind_Backtrace"),
        Some(RuntimeService::UnwindBacktrace)
    );
    for unrelated in [
        "my_crate::_Unwind_FindEnclosingFunction",
        "_Unwind_FindEnclosingFunction_suffix",
        "prefix_Unwind_FindEnclosingFunction",
        "_RNvC1234_8my_crate29_Unwind_FindEnclosingFunction",
        "my_crate::_Unwind_GetCFA",
        "_Unwind_GetCFA_suffix",
        "prefix_Unwind_GetCFA",
        "my_crate::_Unwind_GetIP",
        "_Unwind_GetIP_suffix",
        "prefix_Unwind_GetIP",
        "my_crate::_Unwind_Backtrace",
        "_Unwind_Backtrace_suffix",
        "prefix_Unwind_Backtrace",
    ] {
        assert_eq!(RuntimeService::classify(unrelated), None, "{unrelated}");
    }

    let pointer_guard =
        "_RNvNvMNtNtCs5bnaVnPOCY4_4core3ptr9const_ptrPp20offset_from_unsigned18precondition_check";
    assert_eq!(
        format!("{:#}", rustc_demangle::demangle(pointer_guard)),
        "<*const _>::offset_from_unsigned::precondition_check"
    );
    assert_eq!(
        RuntimeService::classify(pointer_guard),
        Some(RuntimeService::CoreUbPrecondition)
    );
    let integer_guard = "_RNvNvMs9_NtCs5bnaVnPOCY4_4core3numj13unchecked_add18precondition_check";
    assert_eq!(
        format!("{:#}", rustc_demangle::demangle(integer_guard)),
        "<usize>::unchecked_add::precondition_check"
    );
    assert_eq!(
        RuntimeService::classify(integer_guard),
        Some(RuntimeService::CoreUbPrecondition)
    );
    assert_eq!(
        RuntimeService::classify("<u8>::unchecked_add::precondition_check"),
        Some(RuntimeService::CoreUbPrecondition)
    );
    assert_eq!(
        RuntimeService::classify("core::slice::raw::from_raw_parts::precondition_check"),
        Some(RuntimeService::CoreUbPrecondition)
    );
    for unrelated in [
        "my_crate::offset_from_unsigned::precondition_check",
        "<bool>::unchecked_add::precondition_check",
        "<u8>::checked_add::precondition_check",
        "<u8>::unchecked_add::precondition_check_suffix",
        "<u8 as my_crate::Unchecked>::unchecked_add::precondition_check",
        "core::slice::raw::from_raw_parts::not_the_precondition_check",
        "precondition_check",
    ] {
        assert_eq!(RuntimeService::classify(unrelated), None, "{unrelated}");
    }
}

#[cfg(test)]
fn rust_c_void_pointer(asm: &mut Assembly) -> Type {
    let name = asm.alloc_string("core.ffi.c_void.tid_1c1f13fc4e744fd85b767c73c88e69aa");
    let class = asm
        .class_def(ClassDef::new(
            name,
            true,
            0,
            None,
            vec![],
            vec![],
            Access::Assembly,
            std::num::NonZeroU32::new(1),
            std::num::NonZeroU32::new(1),
            true,
        ))
        .unwrap();
    let pointee = asm.alloc_type(Type::ClassRef(class.0));
    Type::Ptr(pointee)
}

#[cfg(test)]
fn named_test_pointer(
    asm: &mut Assembly,
    name: &str,
    is_valuetype: bool,
    assembly: Option<&str>,
    generics: Vec<Type>,
) -> Type {
    let name = asm.alloc_string(name);
    let assembly = assembly.map(|assembly| asm.alloc_string(assembly));
    let class = asm.alloc_class_ref(ClassRef::new(
        name,
        assembly,
        is_valuetype,
        generics.into_boxed_slice(),
    ));
    let pointee = asm.alloc_type(Type::ClassRef(class));
    Type::Ptr(pointee)
}

#[cfg(test)]
fn rust_unwind_reason_code(asm: &mut Assembly) -> Type {
    rust_unwind_reason_code_named(
        asm,
        "std.backtrace_rs.backtrace.libunwind.uw._Unwind_Reason_Code.tid_3ddc8e5b4f31b46b53e19243bd2cfa51",
    )
}

#[cfg(test)]
fn rust_unwind_reason_code_named(asm: &mut Assembly, name: &str) -> Type {
    let name = asm.alloc_string(name);
    let class = asm
        .class_def(ClassDef::new(
            name,
            true,
            0,
            None,
            vec![],
            vec![],
            Access::Assembly,
            std::num::NonZeroU32::new(4),
            std::num::NonZeroU32::new(4),
            true,
        ))
        .unwrap();
    Type::ClassRef(class.0)
}

#[cfg(test)]
fn unwind_backtrace_method(asm: &mut Assembly) -> Interned<MethodRef> {
    let context = asm.nptr(Type::Void);
    let argument = rust_c_void_pointer(asm);
    let reason = rust_unwind_reason_code(asm);
    let callback = asm.sig([context, argument], reason);
    Interned::<MethodRef>::builtin(
        asm,
        "_Unwind_Backtrace",
        &[Type::FnPtr(callback), argument],
        reason,
    )
}

#[test]
fn managed_unwind_symbol_address_capability_is_typed_identity_cil() {
    let mut asm = Assembly::default();
    let pointer = rust_c_void_pointer(&mut asm);
    let method = Interned::<MethodRef>::builtin(
        &mut asm,
        "_Unwind_FindEnclosingFunction",
        &[pointer],
        pointer,
    );
    let mut patcher = MissingMethodPatcher::default();
    super::builtins::unwind::find_enclosing_function(&mut asm, &mut patcher);

    let stats = asm
        .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
        .unwrap();

    assert_eq!(stats.unwind_shims_synthesized, 1);
    assert_eq!(stats.overrides_applied, 0);
    assert_eq!(stats.unresolved_missing_methods, 0);
    let definition = &asm[asm.method_ref_to_def(method).unwrap()];
    assert_eq!(definition.sig(), asm[method].sig());
    let MethodImpl::MethodBody { blocks, locals } = definition.implementation() else {
        panic!("managed unwind identity capability did not produce a method body")
    };
    assert!(locals.is_empty());
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].roots().len(), 1);
    let CILRoot::Ret(value) = &asm[blocks[0].roots()[0]] else {
        panic!("managed unwind identity capability must return its argument")
    };
    assert!(matches!(asm[*value], CILNode::LdArg(0)));

    let (image, _) = asm
        .prepared()
        .verify_for_export()
        .unwrap()
        .try_render_pe(&test_pe_options("unwind-symbol-address-identity"))
        .unwrap();
    assert_eq!(&image[..2], b"MZ");
}

#[test]
fn managed_unwind_symbol_address_requires_an_exact_abi_and_capability() {
    fn assert_signature_rejected(make: impl FnOnce(&mut Assembly) -> Interned<MethodRef>) {
        let mut asm = Assembly::default();
        let method = make(&mut asm);
        let mut patcher = MissingMethodPatcher::default();
        super::builtins::unwind::find_enclosing_function(&mut asm, &mut patcher);
        let error = asm
            .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
            .unwrap_err();
        assert!(matches!(
            error,
            MissingMethodResolutionError::RuntimeServiceSignatureMismatch {
                service: RuntimeService::UnwindFindEnclosingFunction,
                ..
            }
        ));
        assert!(asm.method_ref_to_def(method).is_none());
    }

    assert_signature_rejected(|asm| {
        let pointer = rust_c_void_pointer(asm);
        Interned::<MethodRef>::builtin(asm, "_Unwind_FindEnclosingFunction", &[], pointer)
    });
    assert_signature_rejected(|asm| {
        let pointer = rust_c_void_pointer(asm);
        Interned::<MethodRef>::builtin(
            asm,
            "_Unwind_FindEnclosingFunction",
            &[pointer],
            Type::Int(Int::ISize),
        )
    });
    assert_signature_rejected(|asm| {
        let pointer = asm.nptr(Type::Void);
        Interned::<MethodRef>::builtin(asm, "_Unwind_FindEnclosingFunction", &[pointer], pointer)
    });
    assert_signature_rejected(|asm| {
        Interned::<MethodRef>::builtin(
            asm,
            "_Unwind_FindEnclosingFunction",
            &[Type::Int(Int::ISize)],
            Type::Int(Int::ISize),
        )
    });
    for malformed in [
        "core.ffi.c_void",
        "core.ffi.c_void.tid_short",
        "core.ffi.c_void.tid_1C1F13FC4E744FD85B767C73C88E69AA",
        "core.ffi.c_void.tid_1c1f13fc4e744fd85b767c73c88e69ag",
        "core.ffi.c_void.tid_1c1f13fc4e744fd85b767c73c88e69aa_suffix",
    ] {
        assert_signature_rejected(|asm| {
            let pointer = named_test_pointer(asm, malformed, true, None, vec![]);
            Interned::<MethodRef>::builtin(
                asm,
                "_Unwind_FindEnclosingFunction",
                &[pointer],
                pointer,
            )
        });
    }
    assert_signature_rejected(|asm| {
        let pointer = named_test_pointer(
            asm,
            "core.ffi.c_void.tid_1c1f13fc4e744fd85b767c73c88e69aa",
            false,
            None,
            vec![],
        );
        Interned::<MethodRef>::builtin(asm, "_Unwind_FindEnclosingFunction", &[pointer], pointer)
    });
    assert_signature_rejected(|asm| {
        let pointer = named_test_pointer(
            asm,
            "core.ffi.c_void.tid_1c1f13fc4e744fd85b767c73c88e69aa",
            true,
            Some("foreign"),
            vec![],
        );
        Interned::<MethodRef>::builtin(asm, "_Unwind_FindEnclosingFunction", &[pointer], pointer)
    });
    assert_signature_rejected(|asm| {
        let pointer = named_test_pointer(
            asm,
            "core.ffi.c_void.tid_1c1f13fc4e744fd85b767c73c88e69aa",
            true,
            None,
            vec![Type::Int(Int::I32)],
        );
        Interned::<MethodRef>::builtin(asm, "_Unwind_FindEnclosingFunction", &[pointer], pointer)
    });
    assert_signature_rejected(|asm| {
        let pointer = rust_c_void_pointer(asm);
        let signature = asm.sig([pointer], pointer);
        let main = *asm.main_module();
        asm.new_methodref(
            main,
            "_Unwind_FindEnclosingFunction",
            signature,
            MethodKind::Instance,
            [],
        )
    });
    assert_signature_rejected(|asm| {
        let pointer = rust_c_void_pointer(asm);
        let signature = asm.sig([pointer], pointer);
        let main = *asm.main_module();
        asm.new_methodref(
            main,
            "_Unwind_FindEnclosingFunction",
            signature,
            MethodKind::Static,
            [Type::Int(Int::I32)],
        )
    });

    let mut asm = Assembly::default();
    let pointer = rust_c_void_pointer(&mut asm);
    let method = Interned::<MethodRef>::builtin(
        &mut asm,
        "_Unwind_FindEnclosingFunction",
        &[pointer],
        pointer,
    );
    let error = asm
        .try_resolve_missing_methods(
            &FxHashMap::default(),
            &FxHashSet::default(),
            &MissingMethodPatcher::default(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        MissingMethodResolutionError::UnsupportedRuntimeService {
            service: RuntimeService::UnwindFindEnclosingFunction,
            ..
        }
    ));
    assert!(asm.method_ref_to_def(method).is_none());
}

#[test]
fn linked_unwind_symbol_address_definition_wins_over_the_managed_fallback() {
    let mut asm = Assembly::default();
    let pointer = rust_c_void_pointer(&mut asm);
    let method = Interned::<MethodRef>::builtin(
        &mut asm,
        "_Unwind_FindEnclosingFunction",
        &[pointer],
        pointer,
    );
    let marker = asm.alloc_node(CILNode::LdArg(0));
    let marker = asm.alloc_root(CILRoot::Ret(marker));
    let body = MethodImpl::MethodBody {
        blocks: vec![super::BasicBlock::new(vec![marker], 0, None)],
        locals: vec![],
    };
    let definition = asm[method].clone().into_def(body, Access::Public, &asm);
    asm.new_method(definition);
    let mut patcher = MissingMethodPatcher::default();
    super::builtins::unwind::find_enclosing_function(&mut asm, &mut patcher);

    let stats = asm
        .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
        .unwrap();
    assert_eq!(stats.already_defined, 1);
    assert_eq!(stats.unwind_shims_synthesized, 0);
    let MethodImpl::MethodBody { blocks, .. } =
        asm[asm.method_ref_to_def(method).unwrap()].implementation()
    else {
        panic!("linked unwind definition changed implementation kind")
    };
    assert_eq!(blocks[0].roots(), &[marker]);
}

#[test]
fn managed_unwind_cfa_capability_reports_unavailable_with_exact_abi() {
    let mut asm = Assembly::default();
    let context = asm.nptr(Type::Void);
    let method = Interned::<MethodRef>::builtin(
        &mut asm,
        "_Unwind_GetCFA",
        &[context],
        Type::Int(Int::USize),
    );
    let mut patcher = MissingMethodPatcher::default();
    super::builtins::unwind::get_cfa(&mut asm, &mut patcher);

    let stats = asm
        .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
        .unwrap();
    assert_eq!(stats.unwind_shims_synthesized, 1);
    assert_eq!(stats.unwind_cfa_shims_synthesized, 1);
    assert_eq!(stats.unwind_backtrace_shims_synthesized, 0);
    let definition = &asm[asm.method_ref_to_def(method).unwrap()];
    let MethodImpl::MethodBody { blocks, locals } = definition.implementation() else {
        panic!("managed unwind CFA capability did not produce a method body")
    };
    assert!(locals.is_empty());
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].roots().len(), 1);
    let CILRoot::Ret(value) = &asm[blocks[0].roots()[0]] else {
        panic!("managed unwind CFA capability must return a value")
    };
    let CILNode::Const(value) = &asm[*value] else {
        panic!("managed unwind CFA capability must return a constant")
    };
    assert_eq!(value.as_ref(), &Const::USize(0));
    assert_eq!(asm.typecheck(), 0);
}

#[test]
fn managed_unwind_cfa_rejects_near_miss_abis_and_requires_capability() {
    fn assert_rejected(make: impl FnOnce(&mut Assembly) -> Interned<MethodRef>) {
        let mut asm = Assembly::default();
        let method = make(&mut asm);
        let mut patcher = MissingMethodPatcher::default();
        super::builtins::unwind::get_cfa(&mut asm, &mut patcher);
        let error = asm
            .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
            .unwrap_err();
        assert!(matches!(
            error,
            MissingMethodResolutionError::RuntimeServiceSignatureMismatch {
                service: RuntimeService::UnwindGetCfa,
                ..
            }
        ));
        assert!(asm.method_ref_to_def(method).is_none());
    }

    assert_rejected(|asm| {
        Interned::<MethodRef>::builtin(asm, "_Unwind_GetCFA", &[], Type::Int(Int::USize))
    });
    assert_rejected(|asm| {
        let context = asm.nptr(Type::Void);
        Interned::<MethodRef>::builtin(asm, "_Unwind_GetCFA", &[context], Type::Int(Int::ISize))
    });
    assert_rejected(|asm| {
        let context = rust_c_void_pointer(asm);
        Interned::<MethodRef>::builtin(asm, "_Unwind_GetCFA", &[context], Type::Int(Int::USize))
    });
    assert_rejected(|asm| {
        let context = asm.nptr(Type::Void);
        let signature = asm.sig([context], Type::Int(Int::USize));
        let main = *asm.main_module();
        asm.new_methodref(main, "_Unwind_GetCFA", signature, MethodKind::Instance, [])
    });
    assert_rejected(|asm| {
        let context = asm.nptr(Type::Void);
        let signature = asm.sig([context], Type::Int(Int::USize));
        let main = *asm.main_module();
        asm.new_methodref(
            main,
            "_Unwind_GetCFA",
            signature,
            MethodKind::Static,
            [Type::Int(Int::I32)],
        )
    });
    assert_rejected(|asm| {
        let owner_name = asm.alloc_string("NotMainModule");
        let owner = asm
            .class_def(ClassDef::new(
                owner_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Assembly,
                None,
                None,
                true,
            ))
            .unwrap();
        let context = asm.nptr(Type::Void);
        let signature = asm.sig([context], Type::Int(Int::USize));
        asm.new_methodref(owner.0, "_Unwind_GetCFA", signature, MethodKind::Static, [])
    });

    let mut asm = Assembly::default();
    let context = asm.nptr(Type::Void);
    let method = Interned::<MethodRef>::builtin(
        &mut asm,
        "_Unwind_GetCFA",
        &[context],
        Type::Int(Int::USize),
    );
    let error = asm
        .try_resolve_missing_methods(
            &FxHashMap::default(),
            &FxHashSet::default(),
            &MissingMethodPatcher::default(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        MissingMethodResolutionError::UnsupportedRuntimeService {
            service: RuntimeService::UnwindGetCfa,
            ..
        }
    ));
    assert!(asm.method_ref_to_def(method).is_none());
}

#[test]
fn managed_unwind_ip_capability_reports_unavailable_with_exact_abi() {
    let mut asm = Assembly::default();
    let context = asm.nptr(Type::Void);
    let method = Interned::<MethodRef>::builtin(
        &mut asm,
        "_Unwind_GetIP",
        &[context],
        Type::Int(Int::USize),
    );
    let mut patcher = MissingMethodPatcher::default();
    super::builtins::unwind::get_ip(&mut asm, &mut patcher);

    let stats = asm
        .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
        .unwrap();
    assert_eq!(stats.unwind_shims_synthesized, 1);
    assert_eq!(stats.unwind_cfa_shims_synthesized, 0);
    assert_eq!(stats.unwind_ip_shims_synthesized, 1);
    assert_eq!(stats.unwind_backtrace_shims_synthesized, 0);
    let definition = &asm[asm.method_ref_to_def(method).unwrap()];
    let MethodImpl::MethodBody { blocks, locals } = definition.implementation() else {
        panic!("managed unwind IP capability did not produce a method body")
    };
    assert!(locals.is_empty());
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].roots().len(), 1);
    let CILRoot::Ret(value) = &asm[blocks[0].roots()[0]] else {
        panic!("managed unwind IP capability must return a value")
    };
    let CILNode::Const(value) = &asm[*value] else {
        panic!("managed unwind IP capability must return a constant")
    };
    assert_eq!(value.as_ref(), &Const::USize(0));
    assert_eq!(asm.typecheck(), 0);
}

#[test]
fn managed_unwind_ip_rejects_near_miss_abis_and_requires_capability() {
    fn assert_rejected(make: impl FnOnce(&mut Assembly) -> Interned<MethodRef>) {
        let mut asm = Assembly::default();
        let method = make(&mut asm);
        let mut patcher = MissingMethodPatcher::default();
        super::builtins::unwind::get_ip(&mut asm, &mut patcher);
        let error = asm
            .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
            .unwrap_err();
        assert!(matches!(
            error,
            MissingMethodResolutionError::RuntimeServiceSignatureMismatch {
                service: RuntimeService::UnwindGetIp,
                ..
            }
        ));
        assert!(asm.method_ref_to_def(method).is_none());
    }

    assert_rejected(|asm| {
        Interned::<MethodRef>::builtin(asm, "_Unwind_GetIP", &[], Type::Int(Int::USize))
    });
    assert_rejected(|asm| {
        let context = asm.nptr(Type::Void);
        Interned::<MethodRef>::builtin(asm, "_Unwind_GetIP", &[context], Type::Int(Int::ISize))
    });
    assert_rejected(|asm| {
        let context = rust_c_void_pointer(asm);
        Interned::<MethodRef>::builtin(asm, "_Unwind_GetIP", &[context], Type::Int(Int::USize))
    });
    assert_rejected(|asm| {
        let context = asm.nptr(Type::Void);
        let signature = asm.sig([context], Type::Int(Int::USize));
        let main = *asm.main_module();
        asm.new_methodref(main, "_Unwind_GetIP", signature, MethodKind::Instance, [])
    });

    let mut asm = Assembly::default();
    let context = asm.nptr(Type::Void);
    let method = Interned::<MethodRef>::builtin(
        &mut asm,
        "_Unwind_GetIP",
        &[context],
        Type::Int(Int::USize),
    );
    let error = asm
        .try_resolve_missing_methods(
            &FxHashMap::default(),
            &FxHashSet::default(),
            &MissingMethodPatcher::default(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        MissingMethodResolutionError::UnsupportedRuntimeService {
            service: RuntimeService::UnwindGetIp,
            ..
        }
    ));
    assert!(asm.method_ref_to_def(method).is_none());
}

#[test]
fn managed_unwind_backtrace_capability_returns_end_of_stack() {
    let mut asm = Assembly::default();
    let method = unwind_backtrace_method(&mut asm);
    let mut patcher = MissingMethodPatcher::default();
    super::builtins::unwind::backtrace_end_of_stack(&mut asm, &mut patcher);

    let stats = asm
        .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
        .unwrap();
    assert_eq!(stats.unwind_shims_synthesized, 1);
    assert_eq!(stats.unwind_cfa_shims_synthesized, 0);
    assert_eq!(stats.unwind_backtrace_shims_synthesized, 1);
    let definition = &asm[asm.method_ref_to_def(method).unwrap()];
    let MethodImpl::MethodBody { blocks, locals } = definition.implementation() else {
        panic!("managed unwind backtrace capability did not produce a method body")
    };
    assert_eq!(locals.len(), 1);
    assert_eq!(asm[locals[0].1], *asm[asm[method].sig()].output());
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].roots().len(), 2);
    let CILRoot::StInd(store) = &asm[blocks[0].roots()[0]] else {
        panic!("managed unwind backtrace capability must initialize its enum result")
    };
    let (_, value, stored_type, volatile) = store.as_ref();
    assert_eq!(*stored_type, Type::Int(Int::I32));
    assert!(!*volatile);
    let CILNode::Const(value) = &asm[*value] else {
        panic!("managed unwind backtrace result must use a constant reason code")
    };
    assert_eq!(value.as_ref(), &Const::I32(5));
    let CILRoot::Ret(value) = &asm[blocks[0].roots()[1]] else {
        panic!("managed unwind backtrace capability must return its enum local")
    };
    assert!(matches!(asm[*value], CILNode::LdLoc(0)));
    assert_eq!(asm.typecheck(), 0);
}

#[test]
fn managed_unwind_backtrace_rejects_near_miss_abis_and_requires_capability() {
    fn assert_rejected(make: impl FnOnce(&mut Assembly) -> Interned<MethodRef>) {
        let mut asm = Assembly::default();
        let method = make(&mut asm);
        let mut patcher = MissingMethodPatcher::default();
        super::builtins::unwind::backtrace_end_of_stack(&mut asm, &mut patcher);
        let error = asm
            .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
            .unwrap_err();
        assert!(matches!(
            error,
            MissingMethodResolutionError::RuntimeServiceSignatureMismatch {
                service: RuntimeService::UnwindBacktrace,
                ..
            }
        ));
        assert!(asm.method_ref_to_def(method).is_none());
    }

    assert_rejected(|asm| {
        let argument = rust_c_void_pointer(asm);
        let reason = rust_unwind_reason_code(asm);
        Interned::<MethodRef>::builtin(asm, "_Unwind_Backtrace", &[argument], reason)
    });
    assert_rejected(|asm| {
        let argument = rust_c_void_pointer(asm);
        let reason = rust_unwind_reason_code(asm);
        let callback = asm.sig([argument, argument], reason);
        Interned::<MethodRef>::builtin(
            asm,
            "_Unwind_Backtrace",
            &[Type::FnPtr(callback), argument],
            reason,
        )
    });
    assert_rejected(|asm| {
        let context = asm.nptr(Type::Void);
        let argument = rust_c_void_pointer(asm);
        let reason = rust_unwind_reason_code(asm);
        let callback = asm.sig([context, argument], Type::Int(Int::I32));
        Interned::<MethodRef>::builtin(
            asm,
            "_Unwind_Backtrace",
            &[Type::FnPtr(callback), argument],
            reason,
        )
    });
    assert_rejected(|asm| {
        let context = asm.nptr(Type::Void);
        let reason = rust_unwind_reason_code(asm);
        let callback = asm.sig([context, context], reason);
        Interned::<MethodRef>::builtin(
            asm,
            "_Unwind_Backtrace",
            &[Type::FnPtr(callback), context],
            reason,
        )
    });
    assert_rejected(|asm| {
        let context = asm.nptr(Type::Void);
        let argument = rust_c_void_pointer(asm);
        let reason = rust_unwind_reason_code(asm);
        let callback = asm.sig([context, argument], reason);
        Interned::<MethodRef>::builtin(
            asm,
            "_Unwind_Backtrace",
            &[Type::FnPtr(callback), argument],
            Type::Int(Int::I32),
        )
    });
    assert_rejected(|asm| {
        let context = asm.nptr(Type::Void);
        let argument = rust_c_void_pointer(asm);
        let reason = rust_unwind_reason_code_named(
            asm,
            "std.backtrace_rs.backtrace.libunwind.uw._Unwind_Reason_Code.tid_short",
        );
        let callback = asm.sig([context, argument], reason);
        Interned::<MethodRef>::builtin(
            asm,
            "_Unwind_Backtrace",
            &[Type::FnPtr(callback), argument],
            reason,
        )
    });

    let mut asm = Assembly::default();
    let method = unwind_backtrace_method(&mut asm);
    let error = asm
        .try_resolve_missing_methods(
            &FxHashMap::default(),
            &FxHashSet::default(),
            &MissingMethodPatcher::default(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        MissingMethodResolutionError::UnsupportedRuntimeService {
            service: RuntimeService::UnwindBacktrace,
            ..
        }
    ));
    assert!(asm.method_ref_to_def(method).is_none());
}

#[test]
fn linked_unwind_context_services_win_over_managed_fallbacks() {
    let mut asm = Assembly::default();
    let context = asm.nptr(Type::Void);
    let get_cfa = Interned::<MethodRef>::builtin(
        &mut asm,
        "_Unwind_GetCFA",
        &[context],
        Type::Int(Int::USize),
    );
    let get_cfa_value = asm.alloc_node(Const::USize(17));
    let get_cfa_marker = asm.alloc_root(CILRoot::Ret(get_cfa_value));
    let body = MethodImpl::MethodBody {
        blocks: vec![super::BasicBlock::new(vec![get_cfa_marker], 0, None)],
        locals: vec![],
    };
    asm.new_method(asm[get_cfa].clone().into_def(body, Access::Public, &asm));

    let get_ip = Interned::<MethodRef>::builtin(
        &mut asm,
        "_Unwind_GetIP",
        &[context],
        Type::Int(Int::USize),
    );
    let get_ip_value = asm.alloc_node(Const::USize(23));
    let get_ip_marker = asm.alloc_root(CILRoot::Ret(get_ip_value));
    let body = MethodImpl::MethodBody {
        blocks: vec![super::BasicBlock::new(vec![get_ip_marker], 0, None)],
        locals: vec![],
    };
    asm.new_method(asm[get_ip].clone().into_def(body, Access::Public, &asm));

    let backtrace = unwind_backtrace_method(&mut asm);
    let reason = *asm[asm[backtrace].sig()].output();
    let reason = asm.alloc_type(reason);
    let value = asm.alloc_node(CILNode::LdLoc(0));
    let backtrace_marker = asm.alloc_root(CILRoot::Ret(value));
    let body = MethodImpl::MethodBody {
        blocks: vec![super::BasicBlock::new(vec![backtrace_marker], 0, None)],
        locals: vec![(None, reason)],
    };
    asm.new_method(asm[backtrace].clone().into_def(body, Access::Public, &asm));

    let mut patcher = MissingMethodPatcher::default();
    super::builtins::unwind::get_cfa(&mut asm, &mut patcher);
    super::builtins::unwind::get_ip(&mut asm, &mut patcher);
    super::builtins::unwind::backtrace_end_of_stack(&mut asm, &mut patcher);
    let stats = asm
        .try_resolve_missing_methods(&FxHashMap::default(), &FxHashSet::default(), &patcher)
        .unwrap();
    assert_eq!(stats.already_defined, 3);
    assert_eq!(stats.unwind_cfa_shims_synthesized, 0);
    assert_eq!(stats.unwind_ip_shims_synthesized, 0);
    assert_eq!(stats.unwind_backtrace_shims_synthesized, 0);
    let MethodImpl::MethodBody { blocks, .. } =
        asm[asm.method_ref_to_def(get_cfa).unwrap()].implementation()
    else {
        panic!("linked GetCFA definition changed implementation kind")
    };
    assert_eq!(blocks[0].roots(), &[get_cfa_marker]);
    let MethodImpl::MethodBody { blocks, .. } =
        asm[asm.method_ref_to_def(get_ip).unwrap()].implementation()
    else {
        panic!("linked GetIP definition changed implementation kind")
    };
    assert_eq!(blocks[0].roots(), &[get_ip_marker]);
    let MethodImpl::MethodBody { blocks, .. } =
        asm[asm.method_ref_to_def(backtrace).unwrap()].implementation()
    else {
        panic!("linked Backtrace definition changed implementation kind")
    };
    assert_eq!(blocks[0].roots(), &[backtrace_marker]);
}

#[cfg(test)]
fn thrown_exception_name(asm: &Assembly, method: Interned<MethodRef>) -> String {
    let definition = &asm[asm.method_ref_to_def(method).expect("resolved method")];
    let MethodImpl::MethodBody { blocks, locals } = definition.implementation() else {
        panic!("panic service did not resolve to a method body")
    };
    assert!(locals.is_empty());
    assert_eq!(blocks.len(), 1);
    let root = blocks[0].roots().last().expect("panic service terminal");
    let CILRoot::Throw(exception) = &asm[*root] else {
        panic!("panic service body must terminate with throw")
    };
    let CILNode::Call(call) = &asm[*exception] else {
        panic!("panic service must throw a constructed exception")
    };
    let (ctor, _, _) = call.as_ref();
    let ctor = &asm[*ctor];
    assert_eq!(&asm[ctor.name()], ".ctor");
    let class = asm.class_ref(ctor.class());
    assert!(
        class.asm().is_some(),
        "panic fallback must be an external managed exception, not a fabricated RustException"
    );
    asm[class.name()].to_string()
}

#[test]
fn absent_native_core_panics_resolve_to_signature_correct_typed_throws() {
    let mut asm = Assembly::default();
    let location = asm.nptr(Type::Void);
    let bounds = Interned::<MethodRef>::builtin(
        &mut asm,
        "_RNvNtCsh6vCWCGbp4W_4core9panicking18panic_bounds_check",
        &[Type::Int(Int::USize), Type::Int(Int::USize), location],
        Type::Void,
    );
    let remainder = Interned::<MethodRef>::builtin(
        &mut asm,
        "_RNvNtNtCs5bnaVnPOCY4_4core9panicking11panic_const23panic_const_rem_by_zero",
        &[location],
        Type::Void,
    );
    let resumed = Interned::<MethodRef>::builtin(
        &mut asm,
        PanicKind::AsyncFnResumed.canonical_symbol(),
        &[location],
        Type::Void,
    );
    let explicit = Interned::<MethodRef>::builtin(
        &mut asm,
        "_RNvNtCs5bnaVnPOCY4_4core9panicking5panic",
        &[Type::Int(Int::USize), location],
        Type::Void,
    );
    let cannot_unwind = Interned::<MethodRef>::builtin(
        &mut asm,
        PanicKind::CannotUnwind.canonical_symbol(),
        &[],
        Type::Void,
    );
    let pointer_guard = Interned::<MethodRef>::builtin(
        &mut asm,
        "_RNvNvMNtNtCs5bnaVnPOCY4_4core3ptr9const_ptrPp20offset_from_unsigned18precondition_check",
        &[location, location, location],
        Type::Void,
    );
    let integer_guard = Interned::<MethodRef>::builtin(
        &mut asm,
        "_RNvNvMs9_NtCs5bnaVnPOCY4_4core3numj13unchecked_add18precondition_check",
        &[Type::Int(Int::USize), Type::Int(Int::USize), location],
        Type::Void,
    );

    let stats = asm
        .try_resolve_missing_methods(
            &FxHashMap::default(),
            &FxHashSet::default(),
            &MissingMethodPatcher::default(),
        )
        .unwrap();

    assert_eq!(stats.panic_shims_synthesized, 5);
    assert_eq!(stats.core_ub_precondition_shims_synthesized, 2);
    assert_eq!(stats.missing_stubs_synthesized, 0);
    assert_eq!(stats.unresolved_missing_methods, 0);
    assert_eq!(
        thrown_exception_name(&asm, bounds),
        "System.IndexOutOfRangeException"
    );
    assert_eq!(
        thrown_exception_name(&asm, remainder),
        "System.DivideByZeroException"
    );
    assert_eq!(
        thrown_exception_name(&asm, resumed),
        "System.InvalidOperationException"
    );
    assert_eq!(
        thrown_exception_name(&asm, explicit),
        "System.InvalidOperationException"
    );
    assert_eq!(
        thrown_exception_name(&asm, cannot_unwind),
        "System.InvalidOperationException"
    );
    let MethodImpl::MethodBody { blocks, .. } =
        asm[asm.method_ref_to_def(cannot_unwind).unwrap()].implementation()
    else {
        panic!("nounwind panic did not resolve to a method body")
    };
    assert_eq!(blocks[0].roots().len(), 2);
    let CILRoot::Call(fail_fast) = &asm[blocks[0].roots()[0]] else {
        panic!("nounwind panic must call Environment.FailFast")
    };
    let fail_fast_ref = &asm[fail_fast.0];
    assert_eq!(&asm[fail_fast_ref.name()], "FailFast");
    assert_eq!(
        &asm[asm.class_ref(fail_fast_ref.class()).name()],
        "System.Environment"
    );

    // This helper performs a conditional check in native core. The fallback must return for a
    // valid pointer operation instead of unconditionally failing; only its optional UB diagnostic
    // is unavailable when direct-rustc fixtures link the native core artifact.
    let guard_definition = &asm[asm.method_ref_to_def(pointer_guard).unwrap()];
    assert_eq!(guard_definition.sig(), asm[pointer_guard].sig());
    let MethodImpl::MethodBody { blocks, locals } = guard_definition.implementation() else {
        panic!("core UB precondition did not resolve to a body")
    };
    assert!(locals.is_empty());
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].roots().len(), 1);
    assert!(matches!(&asm[blocks[0].roots()[0]], CILRoot::VoidRet));
    let MethodImpl::MethodBody { blocks, .. } =
        asm[asm.method_ref_to_def(integer_guard).unwrap()].implementation()
    else {
        panic!("integer UB precondition did not resolve to a body")
    };
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].roots().len(), 1);
    assert!(matches!(&asm[blocks[0].roots()[0]], CILRoot::VoidRet));

    // This is the strict verifier + direct-emitter boundary used by the linker. A throw is a valid
    // terminal for every retained panic signature, and the precondition helper remains a valid
    // void-returning method, including its hidden track-caller argument.
    let (image, _) = asm
        .prepared()
        .verify_for_export()
        .unwrap()
        .try_render_pe(&test_pe_options("typed-panic-fallbacks"))
        .unwrap();
    assert_eq!(&image[..2], b"MZ");
}

#[test]
fn linked_core_panic_definition_wins_over_the_native_core_fallback() {
    let mut asm = Assembly::default();
    let location = asm.nptr(Type::Void);
    let method = Interned::<MethodRef>::builtin(
        &mut asm,
        PanicKind::RemByZero.canonical_symbol(),
        &[location],
        Type::Void,
    );
    let real_body_marker = asm.alloc_root(CILRoot::VoidRet);
    let body = MethodImpl::MethodBody {
        blocks: vec![super::BasicBlock::new(vec![real_body_marker], 0, None)],
        locals: vec![],
    };
    let reference = asm[method].clone();
    let definition = reference.into_def(body, Access::Public, &asm);
    asm.new_method(definition);

    let stats = asm
        .try_resolve_missing_methods(
            &FxHashMap::default(),
            &FxHashSet::default(),
            &MissingMethodPatcher::default(),
        )
        .unwrap();

    assert_eq!(stats.already_defined, 1);
    assert_eq!(stats.panic_shims_synthesized, 0);
    let MethodImpl::MethodBody { blocks, .. } =
        asm[asm.method_ref_to_def(method).unwrap()].implementation()
    else {
        panic!("linked core definition changed implementation kind")
    };
    assert_eq!(blocks[0].roots(), &[real_body_marker]);
}

#[test]
fn rust_catch_unwind_rethrows_managed_panic_fallbacks_without_a_fake_payload() {
    let mut asm = Assembly::default();
    let mut patcher = MissingMethodPatcher::default();
    super::builtins::insert_exception(&mut asm, &mut patcher);
    let catch_name = asm.alloc_string("catch_unwind");
    let byte_ptr = asm.nptr(Type::Int(Int::U8));
    let catch_ref = Interned::<MethodRef>::builtin(
        &mut asm,
        "catch_unwind",
        &[byte_ptr, byte_ptr, byte_ptr],
        Type::Int(Int::I32),
    );
    let implementation = patcher.get(&catch_name).unwrap()(catch_ref, &mut asm);
    let MethodImpl::MethodBody { blocks, .. } = implementation else {
        panic!("catch_unwind patcher did not produce a body")
    };
    let handler = blocks[0].handler().expect("catch_unwind handler");
    assert!(
        handler
            .iter()
            .flat_map(super::BasicBlock::roots)
            .any(|root| matches!(asm[*root], CILRoot::ReThrow))
    );
    let checked_type = handler
        .iter()
        .flat_map(super::BasicBlock::roots)
        .find_map(|root| {
            let CILRoot::Branch(branch) = &asm[*root] else {
                return None;
            };
            let (_, _, Some(super::BranchCond::False(check))) = branch.as_ref() else {
                return None;
            };
            let CILNode::IsInst(_, checked_type) = asm[*check] else {
                return None;
            };
            Some(checked_type)
        })
        .expect("catch_unwind RustException type test");
    let Type::ClassRef(rust_exception) = asm[checked_type] else {
        panic!("catch_unwind must test a class reference")
    };
    assert_eq!(&asm[asm.class_ref(rust_exception).name()], "RustException");
    assert!(asm.class_ref(rust_exception).asm().is_none());

    let managed_fallback = PanicKind::Explicit.managed_exception(&mut asm);
    assert_eq!(
        &asm[asm.class_ref(managed_fallback).name()],
        "System.InvalidOperationException"
    );
    assert!(asm.class_ref(managed_fallback).asm().is_some());
    assert_ne!(managed_fallback, rust_exception);
}

#[test]
fn unsupported_runtime_service_is_a_structured_resolution_error() {
    let mut asm = Assembly::default();
    let alloc = Interned::<MethodRef>::builtin(&mut asm, "__rust_alloc", &[], Type::Void);
    let error = asm
        .try_resolve_missing_methods(
            &FxHashMap::default(),
            &FxHashSet::default(),
            &MissingMethodPatcher::default(),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        MissingMethodResolutionError::UnsupportedRuntimeService {
            service: RuntimeService::Alloc,
            ..
        }
    ));
    assert!(asm.method_ref_to_def(alloc).is_none());
}

#[test]
fn no_alloc_marker_is_a_typed_builtin_noop_capability() {
    let mut asm = Assembly::default();
    let marker = Interned::<MethodRef>::builtin(
        &mut asm,
        "__rustc::__rust_no_alloc_shim_is_unstable_v2",
        &[],
        Type::Void,
    );
    let stats = asm
        .try_resolve_missing_methods(
            &FxHashMap::default(),
            &FxHashSet::default(),
            &MissingMethodPatcher::default(),
        )
        .unwrap();

    assert_eq!(stats.no_alloc_shims_synthesized, 1);
    assert!(matches!(
        asm[asm.method_ref_to_def(marker).unwrap()].implementation(),
        MethodImpl::MethodBody { .. }
    ));
}

#[test]
fn declared_native_import_beats_the_legacy_extern_map_and_preserves_metadata() {
    let mut asm = Assembly::default();
    let imported = Interned::<MethodRef>::builtin(&mut asm, "rust_name", &[], Type::Void);
    asm.add_native_import(NativeImport {
        rust_symbol: "rust_name".into(),
        entry_point: "native_name".into(),
        library: "example-native".into(),
        call_conv: super::PInvokeCallConv::Stdcall,
        preserve_errno: true,
    });
    let mut externs = FxHashMap::default();
    externs.insert("rust_name", "wrong-library".to_owned());

    let stats = asm.resolve_missing_methods(
        &externs,
        &FxHashSet::default(),
        &MissingMethodPatcher::default(),
    );

    assert_eq!(stats.externs_synthesized, 1);
    let implementation = asm[asm.method_ref_to_def(imported).unwrap()].implementation();
    let MethodImpl::Extern {
        lib,
        entry_point,
        call_conv,
        preserve_errno,
    } = implementation
    else {
        panic!("declared native import did not become a P/Invoke method")
    };
    assert_eq!(&asm[*lib], "example-native");
    assert_eq!(&asm[entry_point.unwrap()], "native_name");
    assert_eq!(*call_conv, super::PInvokeCallConv::Stdcall);
    assert!(*preserve_errno);
}

#[test]
fn call_alias_adapter_explicitly_converts_pointer_and_native_int_boundaries() {
    let mut asm = Assembly::default();
    let void_ptr = asm.nptr(Type::Void);
    let sig = asm.sig([void_ptr], void_ptr);

    let argument = asm.adapt_call_argument(0, void_ptr, Type::Int(Int::USize));
    let argument = asm[argument].clone();
    assert_eq!(
        argument.typecheck(sig, &[], &mut asm).unwrap(),
        Type::Int(Int::USize)
    );

    let native_int = asm.alloc_node(Const::USize(1));
    let result = asm.adapt_call_result(native_int, Type::Int(Int::USize), void_ptr);
    let result = asm[result].clone();
    assert_eq!(result.typecheck(sig, &[], &mut asm).unwrap(), void_ptr);

    let zero = asm.alloc_node(Const::ISize(0));
    let pointer = asm.cast_ptr_to(zero, void_ptr);
    let native = asm.adapt_call_value(pointer, void_ptr, Type::Int(Int::USize));
    let native_node = asm[native].clone();
    assert_eq!(
        native_node.typecheck(sig, &[], &mut asm).unwrap(),
        Type::Int(Int::USize)
    );
    let pointer = asm.adapt_call_value(native, Type::Int(Int::USize), void_ptr);
    let pointer_node = asm[pointer].clone();
    assert_eq!(
        pointer_node.typecheck(sig, &[], &mut asm).unwrap(),
        void_ptr
    );
}

#[test]
fn missing_method_resolution_reports_runtime_stubs_but_not_abstract_placeholders() {
    let mut asm = Assembly::default();
    let missing = Interned::<MethodRef>::builtin(&mut asm, "unresolved", &[], Type::Void);

    let main_module = asm.main_module();
    let abstract_name = asm.alloc_string("AbstractPlaceholder");
    let abstract_sig = asm.sig([], Type::Void);
    asm.new_method(
        MethodDef::new(
            Access::Public,
            main_module,
            abstract_name,
            abstract_sig,
            MethodKind::Static,
            MethodImpl::Missing,
            vec![],
        )
        .with_abstract(),
    );

    let externs: FxHashMap<&str, String> = FxHashMap::default();
    let modifies_errno: FxHashSet<&str> = FxHashSet::default();
    let stats =
        asm.resolve_missing_methods(&externs, &modifies_errno, &MissingMethodPatcher::default());

    assert_eq!(stats.missing_stubs_synthesized, 1);
    assert_eq!(stats.unresolved_missing_methods, 1);
    assert!(matches!(
        asm[asm.method_ref_to_def(missing).unwrap()].implementation(),
        MethodImpl::Missing
    ));
}

#[test]
fn missing_method_resolution_rejects_dangling_interface_refs_structurally() {
    let mut asm = Assembly::default();
    let interface_name = asm.alloc_string("ITest");
    let interface = asm
        .class_def(
            ClassDef::new(
                interface_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                None,
                None,
                true,
            )
            .with_interface(),
        )
        .unwrap();
    let sig = asm.sig([], Type::Void);
    asm.new_methodref(*interface, "MissingMember", sig, MethodKind::Virtual, []);

    let externs: FxHashMap<&str, String> = FxHashMap::default();
    let modifies_errno: FxHashSet<&str> = FxHashSet::default();
    let error = asm
        .try_resolve_missing_methods(&externs, &modifies_errno, &MissingMethodPatcher::default())
        .unwrap_err();
    assert!(matches!(
        error,
        MissingMethodResolutionError::InterfaceMemberMismatch { .. }
    ));
}

config! {LINKER_RECOVER,bool,false}
