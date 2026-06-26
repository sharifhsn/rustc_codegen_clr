#![allow(clippy::module_name_repetitions)]
#![feature(iter_intersperse, pattern)]

pub use crate::ir::*;
use fxhash::FxHasher;

pub type IString = Box<str>;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AsmString(u64);

pub fn calculate_hash<T: std::hash::Hash>(t: &T) -> u64 {
    use std::hash::Hasher;
    let mut s = FxHasher::default();
    t.hash(&mut s);
    s.finish()
}

use serde::{Deserialize, Serialize};

#[derive(Hash, PartialEq, Eq, Copy, Clone, Debug, Serialize, Deserialize)]
pub enum Access {
    Extern,
    Public,
    Private,
}

impl Access {
    /// Returns `true` if the access is [`Extern`].
    ///
    /// [`Extern`]: Access::Extern
    #[must_use]
    pub fn is_extern(&self) -> bool {
        matches!(self, Self::Extern)
    }
}

pub mod entrypoint;
pub mod libc_fns;

pub mod utilis;
pub mod ir;
/// The metadata of a slice
pub const METADATA: &str = "m";
/// The data pointer of a slice
pub const DATA_PTR: &str = "d";
/// The tag of an enum
pub const ENUM_TAG: &str = "v";
#[macro_export]
macro_rules! config {
    ($name:ident,bool,$default:expr) => {
        pub static $name: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            std::env::vars()
                .find_map(|(key, value)| {
                    if key == stringify!($name) {
                        Some(value)
                    } else {
                        None
                    }
                })
                .map(|value| match value.as_ref() {
                    "0" | "false" | "False" | "FALSE" => false,
                    "1" | "true" | "True" | "TRUE" => true,
                    _ => panic!(
                        "Boolean enviroment variable {} has invalid value {}",
                        stringify!($name),
                        value
                    ),
                })
                .unwrap_or($default)
        });
    };
    ($name:ident,bool,$default:expr,$comment:literal) => {
        #[doc = $comment]
        pub static $name: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
            std::env::vars()
                .find_map(|(key, value)| {
                    if key == stringify!($name) {
                        Some(value)
                    } else {
                        None
                    }
                })
                .map(|value| match value.as_ref() {
                    "0" | "false" | "False" | "FALSE" => false,
                    "1" | "true" | "True" | "TRUE" => true,
                    _ => panic!(
                        "Boolean enviroment variable {} has invalid value {}",
                        stringify!($name),
                        value
                    ),
                })
                .unwrap_or($default)
        });
    };
    ($name:ident,$tpe:ty,$default:expr) => {
        pub static $name: std::sync::LazyLock<$tpe> = std::sync::LazyLock::new(|| {
            std::env::vars()
                .find_map(|(key, value)| {
                    if key == stringify!($name) {
                        Some(value)
                    } else {
                        None
                    }
                })
                .map(|value| value.parse().unwrap())
                .unwrap_or($default)
        });
    };
}
config! {DEAD_CODE_ELIMINATION,bool,true}

/// Debug tooling: when set, every method whose mangled **or** demangled name contains this substring
/// is printed as a deterministic, type-annotated IR dump during the type-verifier pass (see
/// [`crate::ir::dump`]). Empty/unset disables it. Read directly because the dump fires in both the
/// backend (`join_codegen`) and the `linker` process.
pub static DUMP_FN: std::sync::LazyLock<Option<String>> =
    std::sync::LazyLock::new(|| std::env::var("DUMP_FN").ok().filter(|s| !s.is_empty()));
/// The active `DUMP_FN` substring filter, if any.
#[must_use]
pub fn dump_fn_filter() -> Option<&'static str> {
    DUMP_FN.as_deref()
}

// --- Type-verifier wiring (Phase P1 of docs/ABSOLUTE_CORRECTNESS_PLAN.md) -------------------------
// These three flags gate `Assembly::typecheck` (cilly/src/ir/asm.rs). They mirror the same-named
// flags in the backend's `src/config.rs`; cilly reads the env directly because the typecheck runs in
// both the backend (`join_codegen`) and the `linker` process. Defaults are chosen to preserve the
// historical behaviour exactly — run the checker, but only *warn* — so wiring them in is a no-op
// until the operator opts into stricter modes (or the project flips `ALLOW_MISCOMPILATIONS` off).
config! {TYPECHECK_CIL,bool,true,"Run the CIL type-verifier over every emitted method. Default on."}
config! {VERIFY_METHODS,bool,true,"Alias-style enable for the per-method type-verifier. Default on; either this or TYPECHECK_CIL being set runs the checker."}
config! {ALLOW_MISCOMPILATIONS,bool,false,"If true, a type-verifier violation is a warning and codegen continues. If false (default — Phase P1 of the absolute-correctness plan, invariant I1), any violation ABORTS the build: an ill-typed method is never emitted. The default was flipped to false once the verifier was proven sound (zero false positives across the full ::stable build + std/probe/soak corpus) and the gate stayed green under the fatal checker. Set ALLOW_MISCOMPILATIONS=1 to opt back into the historical advisory behaviour."}

#[must_use]
pub fn mem_checks() -> bool {
    false
}
#[must_use]
pub fn debig_sfi() -> bool {
    *crate::DEBUG_SFI
}
config!(
    DEBUG_SFI,
    bool,
    false,
    "Tells codegen to display source file info when executing each statement."
);

#[derive(Copy, Clone)]
pub struct DepthSetting(u32);
impl DepthSetting {
    pub fn with_pading() -> Self {
        Self(0)
    }
    pub fn no_pading() -> Self {
        Self(u32::MAX)
    }
    pub fn pad(&self, out: &mut impl std::fmt::Write) -> std::fmt::Result {
        writeln!(out)?;
        if self.0 == u32::MAX {
            return Ok(());
        }
        for _ in 0..self.0 {
            write!(out, " ")?;
        }
        Ok(())
    }
    pub fn incremented(self) -> Self {
        if self.0 == u32::MAX {
            self
        } else {
            Self(self.0 + 1)
        }
    }
}

pub fn escape_type_name(name: &str) -> String {
    name.replace(['.', ' '], "_")
        .replace('<', "lt")
        .replace('>', "gt")
        .replace('$', "ds")
        .replace(',', "cm")
        .replace('{', "bs")
        .replace('}', "be")
        .replace('+', "ps")
}
#[macro_export]
macro_rules! source_info {
    () => {
        CILRoot::source_info(
            file!(),
            (line!() as u64)..(line!() as u64),
            (column!() as u64)..(column!() as u64 + 1),
        )
        .into()
    };
}
