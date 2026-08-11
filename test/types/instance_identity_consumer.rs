#![crate_type = "lib"]

extern crate instance_identity_support;

// Taking an inline upstream function's address makes rustc emit a downstream-owned
// `GloballyShared { may_conflict: true }` copy. Native symbols add the consumer crate as a suffix;
// the managed backend deliberately gives both copies one semantic Instance name and lets strict
// body comparison decide whether they are actually mergeable.
#[unsafe(no_mangle)]
pub extern "C" fn consuming_crate_probe() -> fn(u32) -> u32 {
    instance_identity_support::instance_suffix_probe
}

#[unsafe(no_mangle)]
pub extern "C" fn consuming_unreachable_probe() -> fn(bool) -> u32 {
    instance_identity_support::instance_unreachable_probe
}
