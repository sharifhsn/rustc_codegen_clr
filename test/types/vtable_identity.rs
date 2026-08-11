#![feature(adt_const_params, core_intrinsics, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

include!("../common.rs");

struct SharedConcrete;

trait First: Sync {
    fn value(&self) -> u32;
}

trait Second: Sync {
    fn value(&self) -> u32;
}

impl First for SharedConcrete {
    #[inline(never)]
    fn value(&self) -> u32 {
        11
    }
}

impl Second for SharedConcrete {
    #[inline(never)]
    fn value(&self) -> u32 {
        29
    }
}

const FIRST: &(dyn First + Sync) = &SharedConcrete;
const SECOND: &(dyn Second + Sync) = &SharedConcrete;

fn main() {
    test_eq!(black_box(FIRST).value(), 11);
    test_eq!(black_box(SECOND).value(), 29);
}
