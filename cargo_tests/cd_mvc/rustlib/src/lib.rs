//! Investigation crate: can a Rust-defined type/function participate in ASP.NET Core hosting?
//!
//! Tier 1: a real `WebApplication` minimal-API host (C# side, `csharp/Program.cs`) whose HTTP
//! handler bodies call into Rust-defined logic — both a plain `#[dotnet_export]` function and a
//! `#[dotnet_class]` instance method. This crate supplies that logic.
//!
//! Tier 2: whether a Rust-defined method can be passed directly as `app.MapGet("/foo", handler)`
//! via method-group-to-delegate conversion (no C# wrapper lambda). `Calculator::add` below is
//! shaped to be attempted as a direct handler (static, primitive params/return — the RequestDelegate
//! signature ASP.NET's minimal APIs expect for auto route-binding).
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::{dotnet_class, dotnet_export, dotnet_methods};

/// `int add(int, int)` — plain exported function, called from a minimal-API handler body.
#[dotnet_export]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// `string greet(string)` — plain exported function returning a computed managed string.
#[dotnet_export]
pub fn greet(name: &str) -> String {
    format!("Hello, {name}, from Rust (via ASP.NET Core)!")
}

/// A Rust-defined managed class with instance state, whose method is called from a minimal-API
/// handler body (Tier 1), and separately attempted as a direct `MapGet` handler (Tier 2).
#[dotnet_class(default_ctor = true, field_setters = true)]
pub struct Calculator {
    accumulator: i32,
}

#[dotnet_methods]
impl Calculator {
    pub fn make(start: i32) -> CalculatorHandle {
        CalculatorHandle::ctor1::<i32>(start)
    }

    /// Instance method: adds `n` to the accumulator and returns the new total.
    pub fn add_to(this: CalculatorHandle, n: i32) -> i32 {
        let acc: i32 = this.instance0::<"read_accumulator", i32>();
        acc + n
    }

    /// Static method with a signature that fits ASP.NET minimal-API's plain-parameter route
    /// binding (`static int multiply(int, int)`), for the Tier-2 direct-delegate-handler attempt.
    pub fn multiply(a: i32, b: i32) -> i32 {
        a * b
    }
}
