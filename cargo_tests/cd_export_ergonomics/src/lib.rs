//! Proof for the two `#[dotnet_export]`/`#[dotnet_methods]` ergonomics expansions:
//!   1. `#[dotnet_export]` now marshals `Option<T>` (both directions) and `Vec<T>` params (of a
//!      passthrough primitive `T`), not just returns.
//!   2. `#[dotnet_methods]` instance/static methods now go through the SAME marshalling table, so a
//!      class method can take `&str`/`String`/`Option<T>`/`Vec<T>` directly instead of hand-marshalling
//!      `MString`/`Nullable<T>`/a `RustVec<T>` handle.
//!   3. A C# `Action`/`Func`/`Comparison` parameter crosses as its real managed delegate handle and
//!      is reconstructed as an invokable `mycorrhiza::delegate` wrapper inside Rust.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::{dotnet_class, dotnet_enum, dotnet_export, dotnet_methods};
use mycorrhiza::delegate::{Action1, Action2, Action3, Comparison, Func1, Func2, Func3};
use mycorrhiza::system::{DotNetString, MString};

// Needed for the `Vec<T>` param arm's `rcl_vec_len`/`rcl_vec_get` calls (both param- and
// method-side), matching the existing `Vec<T>` RETURN arm's requirement.
mycorrhiza::export_rust_containers!();

// ---- Rust-defined enum -> genuine CLR enum + automatic export marshalling ----------------

#[dotnet_enum(name = "Status")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Status {
    Pending = 0,
    Ready = 4,
    Done,
}

#[dotnet_export(enums(Status))]
pub fn roundtrip_status(status: Status) -> Status {
    status
}

#[dotnet_export(enums(Status))]
pub fn is_terminal(status: Status) -> bool {
    status == Status::Done
}

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

// ---- C# delegates imported as invokable Rust wrappers ------------------------------------

#[dotnet_export]
pub fn invoke_action1(callback: Action1<i32>, value: i32) -> i32 {
    callback.invoke(value);
    value
}

#[dotnet_export]
pub fn invoke_action2(callback: Action2<i32, i32>, left: i32, right: i32) -> i32 {
    callback.invoke(left, right);
    left + right
}

#[dotnet_export]
pub fn invoke_action3(
    callback: Action3<i32, i32, i32>,
    first: i32,
    second: i32,
    third: i32,
) -> i32 {
    callback.invoke(first, second, third);
    first + second + third
}

#[dotnet_export]
pub fn invoke_func1(callback: Func1<i32, i32>, value: i32) -> i32 {
    callback.invoke(value)
}

#[dotnet_export]
pub fn invoke_func2(callback: Func2<i32, i32, i32>, left: i32, right: i32) -> i32 {
    callback.invoke(left, right)
}

#[dotnet_export]
pub fn invoke_func3(
    callback: Func3<i32, i32, i32, i32>,
    first: i32,
    second: i32,
    third: i32,
) -> i32 {
    callback.invoke(first, second, third)
}

#[dotnet_export]
pub fn invoke_string_func(callback: Func1<MString, i32>, value: String) -> i32 {
    callback.invoke(MString::from(value.as_str()))
}

#[dotnet_export]
pub fn invoke_comparison(callback: Comparison<i32>, left: i32, right: i32) -> i32 {
    callback.invoke(left, right)
}

// ---- Portable-PDB consumer proof: C# -> exported Rust -> managed stack trace ------------

#[inline(never)]
fn rust_pdb_leaf() -> String {
    let debugger_probe_local =
        DotNetString::from_handle(mycorrhiza::System::Environment::get_stack_trace())
            .to_rust_string();
    std::hint::black_box(&debugger_probe_local);
    debugger_probe_local
}

/// Return the managed stack captured inside an ordinary Rust frame. A C# host uses this to prove
/// the sidecar Portable PDB resolves the exported Rust library back to this `lib.rs` file and line.
#[dotnet_export]
pub fn rust_pdb_stack_trace() -> String {
    rust_pdb_leaf()
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

    /// Instance-method mirror of the free-function delegate import path.
    pub fn apply(this: GreetingHandle, callback: Func1<i32, i32>, value: i32) -> i32 {
        let times: i32 = this.instance0::<"read_times", i32>();
        callback.invoke(value) + times
    }
}
