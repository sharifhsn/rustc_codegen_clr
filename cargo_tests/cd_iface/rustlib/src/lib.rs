//! A Rust type IMPLEMENTING a .NET interface defined in another (C#) assembly.
//!
//! `#[dotnet_class(implements = "[Contracts]Contracts.IGreeter")]` declares that the managed class
//! `Greeter` (synthesized from this Rust struct) implements the interface `Contracts.IGreeter`, which
//! lives in the separate `Contracts.dll`. The backend emits an `implements` clause on the class, and
//! the two virtual methods a `#[dotnet_methods]` block adds — `Greet(string) -> string` and
//! `Priority() -> int` — satisfy the interface members by name+signature (CLR binds them implicitly,
//! so no explicit `.override` is needed).
//!
//! A C# consumer then uses `Greeter` **only through `IGreeter`** — the exact shape of dropping a Rust
//! implementation behind one of an existing C# codebase's interfaces (DI / strategy / plugin).
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::{dotnet_class, dotnet_methods};
use mycorrhiza::system::{DotNetString, MString};

/// A Rust-defined managed reference type implementing `Contracts.IGreeter`. Its single field
/// `base_priority` backs both interface methods (via the backend-synthesized `read_base_priority`
/// accessor the primary ctor emits).
#[dotnet_class(implements = "[Contracts]Contracts.IGreeter")]
pub struct Greeter {
    base_priority: i32,
}

#[dotnet_methods]
impl Greeter {
    /// Implements `IGreeter.Greet(string) -> string`. The managed method name (`Greet`) and signature
    /// (`instance string Greet(string)`) match the interface member exactly, so it binds implicitly.
    pub fn Greet(this: GreeterHandle, name: MString) -> MString {
        let name = DotNetString::from_handle(name).to_rust_string();
        let prio = this.instance0::<"read_base_priority", i32>();
        DotNetString::from(format!("Hello, {name}! (priority {prio})").as_str()).handle()
    }

    /// Implements `IGreeter.Priority() -> int`. Returns `base_priority + 1`, so the consumer can tell
    /// the Rust body actually ran (not a default).
    pub fn Priority(this: GreeterHandle) -> i32 {
        this.instance0::<"read_base_priority", i32>() + 1
    }
}
