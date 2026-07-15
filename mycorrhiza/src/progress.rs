//! Idiomatic reporting through the managed `IProgress<T>` contract.

use crate::class::GenericClass;
use crate::intrinsics::{RustcCLRInteropManagedGeneric, RustcCLRInteropTypeGeneric};

const CORELIB: &str = "System.Private.CoreLib";
const IPROGRESS: &str = "System.IProgress";

/// A genuine CLR `System.IProgress<T>` reference. C# can pass `Progress<T>` or any custom
/// implementation; Rust calls the familiar `report` method without handling delegates directly.
pub type Progress<T> = RustcCLRInteropManagedGeneric<CORELIB, IPROGRESS, (T,)>;

impl<T> Progress<T> {
    #[inline]
    pub fn report(self, value: T) {
        crate::intrinsics::rustc_clr_interop_generic_call2::<
            { CORELIB },
            { IPROGRESS },
            false,
            "Report",
            2u8,
            (T,),
            ((), RustcCLRInteropTypeGeneric<0>),
            (),
            Self,
            T,
        >(self, value);
    }
}

/// Coroutine-safe, GCHandle-rooted `IProgress<T>` reporter.
pub struct ProgressReporter<T> {
    root: GenericClass<CORELIB, IPROGRESS, (T,)>,
}

impl<T> Clone for ProgressReporter<T> {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
        }
    }
}

impl<T> ProgressReporter<T> {
    #[inline]
    pub fn from_raw(progress: Progress<T>) -> Self {
        Self {
            root: GenericClass::from_naked_ref(progress),
        }
    }

    #[inline(never)]
    pub fn report(&self, value: T) {
        let progress = unsafe { self.root.get_naked_ref() };
        progress.report(value);
    }
}
