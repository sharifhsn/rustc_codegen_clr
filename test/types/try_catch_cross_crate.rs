#![allow(dead_code)]

use mycorrhiza::error::try_managed;
use mycorrhiza::{
    cancellation::{Cancellation, CancellationToken},
    memory::{Memory, MemoryHandle},
};

// `try_managed` is generic and therefore instantiated in this downstream crate. Its private
// implementation details must still reach the public-hidden magic declaration through rustc
// metadata; otherwise the aborting Rust placeholder becomes a reachable Missing MethodDef.
#[unsafe(no_mangle)]
pub fn try_catch_cross_crate_probe() -> bool {
    matches!(try_managed(|| 41_u32), Ok(41))
}

// Cancellation and Memory intentionally root CLR value types through the same public-hidden box
// intrinsics. Their generic methods are likewise instantiated downstream, so they guard against
// reintroducing private, metadata-invisible copies of those declarations.
#[unsafe(no_mangle)]
pub fn cancellation_cross_crate_probe(token: CancellationToken) -> bool {
    let mut cancellation = Cancellation::from_token(token);
    cancellation.can_be_canceled()
}

#[unsafe(no_mangle)]
pub fn memory_cross_crate_probe(handle: MemoryHandle<u8>) -> i32 {
    Memory::from_handle(handle).len()
}
