#![crate_type = "lib"]

#[inline]
pub fn instance_suffix_probe(value: u32) -> u32 {
    value.wrapping_add(1)
}

#[inline]
pub fn instance_unreachable_probe(valid: bool) -> u32 {
    if valid {
        7
    } else {
        // The function is never executed by this fixture. Its purpose is to ensure that the
        // backend's diagnostic payload for an `Unreachable` terminator is identical when rustc
        // emits the same upstream MIR in both the defining and consuming crates.
        unsafe { core::hint::unreachable_unchecked() }
    }
}

// Force the defining crate to emit its own copy as well as exporting inline MIR to consumers.
pub static DEFINING_POINTER: fn(u32) -> u32 = instance_suffix_probe;
pub static DEFINING_UNREACHABLE_POINTER: fn(bool) -> u32 = instance_unreachable_probe;

#[unsafe(no_mangle)]
pub extern "C" fn defining_crate_probe(value: u32) -> u32 {
    DEFINING_POINTER(value)
}
