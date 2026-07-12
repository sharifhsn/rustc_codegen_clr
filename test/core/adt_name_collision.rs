#![feature(
    lang_items,
    adt_const_params,
    associated_type_defaults,
    core_intrinsics,
    let_chains,
    never_type,
    unsized_const_params,
    pointer_is_aligned_to
)]
#![allow(internal_features, incomplete_features, unused_variables, dead_code)]

include!("../common.rs");

#[inline(never)]
fn same_parent_local_layouts() -> (u64, u64, u64) {
    // Blocks do not add a readable DefPath component. These three definitions therefore share the
    // same parent and display path; only rustc's definition identity/disambiguator separates them.
    let a = {
        struct SameDisplayName {
            value: u16,
        }
        u64::from(SameDisplayName { value: 16 }.value)
    };
    let b = {
        struct SameDisplayName {
            value: u32,
        }
        u64::from(SameDisplayName { value: 32 }.value)
    };
    let c = {
        struct SameDisplayName {
            value: u64,
        }
        SameDisplayName { value: 64 }.value
    };
    (a, b, c)
}

#[inline(never)]
fn generic_instantiation_layouts() -> (u64, u64) {
    // One DefId instantiated with different field widths must also produce distinct physical CLR
    // classes; the internal identity includes the fully instantiated Ty, not only the ADT DefId.
    struct SameGeneric<T> {
        value: T,
    }
    let a = SameGeneric { value: 7_u16 };
    let b = SameGeneric { value: 9_u64 };
    (u64::from(a.value), b.value)
}

fn main() {
    test_eq!(same_parent_local_layouts(), (16, 32, 64));
    test_eq!(generic_instantiation_layouts(), (7, 9));
}
