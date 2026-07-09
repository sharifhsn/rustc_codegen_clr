//! Proof for the two `#[dotnet_export]`/`#[dotnet_methods]` ergonomics expansions:
//!   1. `#[dotnet_export]` now marshals `Option<T>` (both directions) and `Vec<T>` params (of a
//!      passthrough primitive `T`), not just returns.
//!   2. `#[dotnet_methods]` instance/static methods now go through the SAME marshalling table, so a
//!      class method can take `&str`/`String`/`Option<T>`/`Vec<T>` directly instead of hand-marshalling
//!      `MString`/`Nullable<T>`/a `RustVec<T>` handle.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::{dotnet_class, dotnet_export, dotnet_methods};

// Needed for the `Vec<T>` param arm's `rcl_vec_len`/`rcl_vec_get` calls (both param- and
// method-side), matching the existing `Vec<T>` RETURN arm's requirement.
mycorrhiza::export_rust_containers!();

// ---- #[dotnet_export]: Option<T> now marshals INBOUND too (previously return-only) ----

#[dotnet_export]
pub fn double_if_present(n: Option<i32>) -> Option<i32> {
    n.map(|v| v * 2)
}

// ---- #[dotnet_export]: Vec<T> now marshals INBOUND too (previously return-only) ----

#[dotnet_export]
pub fn sum_vec(xs: Vec<i32>) -> i32 {
    xs.iter().sum()
}

// ---- #[dotnet_methods]: a class method taking &str/String — no manual MString needed ----

#[dotnet_class]
pub struct Greeting {
    times: i32,
}

#[dotnet_methods]
impl Greeting {
    pub fn make(times: i32) -> GreetingHandle {
        GreetingHandle::ctor1::<i32>(times)
    }

    /// Instance method taking `&str` directly (was previously only possible via `MString` by hand) —
    /// proves `#[dotnet_methods]` now shares `#[dotnet_export]`'s param marshalling table.
    pub fn greet(this: GreetingHandle, name: String) -> String {
        let times: i32 = this.instance0::<"read_times", i32>();
        let mut out = String::new();
        for _ in 0..times {
            out.push_str(&format!("Hi, {name}! "));
        }
        out
    }

    /// Instance method taking `Option<i32>` directly.
    pub fn add_bonus(this: GreetingHandle, bonus: Option<i32>) -> i32 {
        let times: i32 = this.instance0::<"read_times", i32>();
        times + bonus.unwrap_or(0)
    }
}
