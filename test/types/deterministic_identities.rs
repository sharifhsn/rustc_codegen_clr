#![crate_type = "lib"]

pub static READ_ONLY_BYTES: [u8; 4] = [3, 1, 4, 1];
pub static READ_ONLY_REF: &[u8; 4] = &READ_ONLY_BYTES;
pub static FUNCTION_POINTER: fn(u32) -> u32 = identity_target;
pub static mut MUTABLE_COUNTER: usize = 0;

#[cfg(identity_perturb)]
#[unsafe(no_mangle)]
pub extern "C" fn unrelated_promotion() -> *const [u64; 2] {
    &[0x1122_3344_5566_7788, 0x8877_6655_4433_2211]
}

fn identity_target(value: u32) -> u32 {
    value.wrapping_add(u32::from(READ_ONLY_REF[0]))
}

#[unsafe(no_mangle)]
pub extern "C" fn deterministic_identity_probe(value: u32) -> u32 {
    unsafe {
        MUTABLE_COUNTER = MUTABLE_COUNTER.wrapping_add(1);
    }
    FUNCTION_POINTER(value)
}

#[unsafe(no_mangle)]
pub extern "C" fn mutable_counter_probe() -> usize {
    unsafe {
        MUTABLE_COUNTER = MUTABLE_COUNTER.wrapping_add(1);
        MUTABLE_COUNTER
    }
}
