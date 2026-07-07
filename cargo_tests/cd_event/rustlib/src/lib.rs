//! Proof of the `#[dotnet_event]` macro wiring on the DEFAULT `DIRECT_PE=1` path — the hand-rolled
//! PE writer now emits the ECMA-335 `EventMap`/`Event`/`MethodSemantics` metadata rows directly
//! (`docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md` Tier C finding #5). No `ilasm` in the loop.
//!
//! `#[dotnet_event("Changed")]` links `add_Changed`/`remove_Changed` into a genuine `System.Action`
//! event — verified from C#: `n.Changed += handler` compiles as a real event subscription (a plain
//! method pair would need `n.add_Changed(handler)`), and `typeof(Notifier).GetEvent("Changed")`
//! reflects a real `EventInfo`. The bodies are deliberately trivial (metadata-linking is the point,
//! not multicast semantics — a production event would `Delegate.Combine` onto a backing field with
//! `Interlocked.CompareExchange`, out of scope for this proof).

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::{dotnet_class, dotnet_methods};
use mycorrhiza::intrinsics::RustcCLRInteropManagedClass;

/// `System.Action` (non-generic, zero-arg) — the delegate type `Changed` subscribers must match.
type ActionHandle = RustcCLRInteropManagedClass<"System.Runtime", "System.Action">;

#[dotnet_class]
pub struct Notifier {}

#[dotnet_methods]
impl Notifier {
    #[dotnet_event("Changed")]
    pub fn add_Changed(_this: NotifierHandle, _value: ActionHandle) {}

    #[dotnet_event("Changed")]
    pub fn remove_Changed(_this: NotifierHandle, _value: ActionHandle) {}
}
