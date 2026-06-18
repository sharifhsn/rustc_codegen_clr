#![allow(clippy::module_name_repetitions)]
#![feature(iter_intersperse, pattern)]

pub use crate::v2::*;
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
pub mod v2;
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
