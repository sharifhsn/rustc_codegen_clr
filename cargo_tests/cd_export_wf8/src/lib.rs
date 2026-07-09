//! Proof for the two WF-8 marshalling gaps closed this session: `Option<T>` return -> a real
//! `System.Nullable<T>`/`T?`, and `Vec<T>`/array return -> a real `T[]` (not the opaque `RustVec<T>`
//! handle the existing `Vec<T>` arm produces).
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::dotnet_export;
use mycorrhiza::intrinsics::{
    rustc_clr_interop_managed_new_arr, rustc_clr_interop_managed_set_elem,
    RustcCLRInteropManagedArray,
};
use mycorrhiza::nullable::Nullable;

/// `int? maybe_positive(int)` — internal `Option<i32>`, converted at the boundary via `.into()`.
#[dotnet_export]
pub fn maybe_positive(n: i32) -> Nullable<i32> {
    let opt: Option<i32> = if n > 0 { Some(n * 2) } else { None };
    opt.into()
}

/// `int[] first_n_squares(int)` — a real managed array, built via the array-construction
/// intrinsics that already existed but were never wired into #[dotnet_export]'s return path.
#[dotnet_export]
pub fn first_n_squares(n: i32) -> RustcCLRInteropManagedArray<i32, 1> {
    let arr = rustc_clr_interop_managed_new_arr::<i32>(n);
    let mut i = 0;
    while i < n {
        rustc_clr_interop_managed_set_elem(arr, i, (i + 1) * (i + 1));
        i += 1;
    }
    arr
}
