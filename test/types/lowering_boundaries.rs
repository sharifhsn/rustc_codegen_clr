#![feature(
    lang_items,
    adt_const_params,
    associated_type_defaults,
    core_intrinsics,
    unsized_const_params
)]
#![allow(
    internal_features,
    incomplete_features,
    unused_variables,
    dead_code,
    improper_ctypes_definitions
)]

include!("../common.rs");

#[derive(Clone, Copy)]
struct Zst;

#[derive(Clone, Copy)]
#[repr(C, align(8))]
struct AlignedZst {
    unit: (),
}

#[inline(never)]
fn leading_zst(_: Zst, value: u32) -> u32 {
    value.wrapping_add(1)
}

#[inline(never)]
fn middle_zst(left: u32, _: Zst, right: u32) -> u32 {
    left.wrapping_add(right)
}

static STATIC_MIDDLE: fn(u32, Zst, u32) -> u32 = middle_zst;

#[track_caller]
#[inline(never)]
fn tracked_leading_zst(_: Zst, value: u32) -> u32 {
    value.wrapping_add(3)
}

#[repr(C)]
struct Inner {
    byte: u8,
    zst: Zst,
}

#[repr(C)]
struct Outer {
    word: u32,
    inner: Inner,
}

#[repr(C, u8)]
enum EnumWithZst {
    Active { payload: AlignedZst },
    Other(u16),
}

// `repr(C, u8)` represents each variant as a C struct whose leading field is the u8 tag. This
// mirror lets the test derive the payload offset independently through the ordinary struct path.
#[repr(C)]
struct ActiveVariantLayout {
    tag: u8,
    payload: AlignedZst,
}

fn test_zst_sequence_addresses() {
    let array = [AlignedZst { unit: () }; 4];
    let array_base = core::ptr::addr_of!(array) as *const AlignedZst as usize;

    let array_indexed = core::ptr::addr_of!(array[3]) as usize;
    let array_intermediate = core::ptr::addr_of!(array[3].unit) as usize;
    test_eq!(array_indexed, array_base);
    test_eq!(array_intermediate, array_base);

    let [array_first, .., array_last] = &array;
    test_eq!(array_first as *const AlignedZst as usize, array_base);
    test_eq!(array_last as *const AlignedZst as usize, array_base);

    let slice: &[AlignedZst] = black_box(&array[..]);
    let slice_base = slice.as_ptr() as usize;
    test_eq!(slice_base, array_base);

    let [slice_first, .., slice_last] = slice else {
        core::intrinsics::abort()
    };
    test_eq!(slice_first as *const AlignedZst as usize, slice_base);
    test_eq!(slice_last as *const AlignedZst as usize, slice_base);

    let [.., AlignedZst { unit: slice_unit }] = slice else {
        core::intrinsics::abort()
    };
    test_eq!(slice_unit as *const () as usize, slice_base);

    let [_, _, tail @ ..] = slice else {
        core::intrinsics::abort()
    };
    test_eq!(tail.as_ptr() as usize, slice_base);
}

fn test_enum_zst_addresses() {
    let value = EnumWithZst::Active {
        payload: AlignedZst { unit: () },
    };
    let enum_base = core::ptr::addr_of!(value);
    let mirror = enum_base.cast::<ActiveVariantLayout>();
    let expected_payload = unsafe { core::ptr::addr_of!((*mirror).payload) } as usize;

    let (payload, unit) = match &value {
        EnumWithZst::Active { payload } => (
            payload as *const AlignedZst as usize,
            core::ptr::addr_of!(payload.unit) as usize,
        ),
        EnumWithZst::Other(_) => core::intrinsics::abort(),
    };
    test_eq!(payload, expected_payload);
    test_eq!(unit, expected_payload);
}

#[repr(C, u8)]
enum ProjectionEnum {
    Active { marker: AlignedZst, value: u32 },
    Other(u16),
}

#[inline(never)]
fn invoke_rust_call<F>(function: F, left: u32, right: u32) -> u32
where
    F: FnOnce(Zst, u32, Zst, u32) -> u32,
{
    function(Zst, left, Zst, right)
}

fn test_projection_reads_writes() -> u32 {
    let mut array = [3_u32, 5, 7, 11, 13];
    let index = black_box(2_usize);
    let array_base = array.as_mut_ptr() as usize;
    let indexed = core::ptr::addr_of_mut!(array[index]) as usize;
    test_eq!(indexed, array_base + index * core::mem::size_of::<u32>());
    test_eq!(array[index], 7);
    array[index] = 17;

    let slice: &mut [u32] = black_box(&mut array[..]);
    let slice_index = black_box(3_usize);
    let slice_indexed = core::ptr::addr_of_mut!(slice[slice_index]) as usize;
    test_eq!(
        slice_indexed,
        slice.as_mut_ptr() as usize + slice_index * core::mem::size_of::<u32>()
    );
    test_eq!(slice[slice_index], 11);
    slice[slice_index] = 19;

    let [first, middle @ .., last] = slice else {
        core::intrinsics::abort()
    };
    test_eq!(*first, 3);
    test_eq!(*last, 13);
    test_eq!(middle[1], 17);
    middle[0] = 23;
    middle[2] = 29;

    let mut projected_enum = ProjectionEnum::Active {
        marker: AlignedZst { unit: () },
        value: 31,
    };
    let enum_base = core::ptr::addr_of!(projected_enum) as usize;
    let (marker_address, enum_value) = match &mut projected_enum {
        ProjectionEnum::Active { marker, value } => {
            *value = value.wrapping_add(6);
            (marker as *mut AlignedZst as usize, *value)
        }
        ProjectionEnum::Other(_) => core::intrinsics::abort(),
    };
    test!(marker_address >= enum_base);
    test_eq!(enum_value, 37);

    array.iter().copied().sum::<u32>().wrapping_add(enum_value)
}

// These deliberately reproduce backend-reserved names and even the private intrinsic marker in a
// different crate. They are ordinary Rust items and must never be intercepted as mycorrhiza APIs.
mod ordinary_lookalikes {
    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    pub struct RustcCLRInteropManagedClass {
        pub value: u32,
    }

    #[doc = "__rustc_codegen_clr_intrinsic_v1"]
    #[inline(never)]
    pub fn rustc_clr_interop_managed_ld_len(value: u32) -> u32 {
        value.wrapping_add(10)
    }
}

#[inline(never)]
fn rustc_codegen_clr_comptime_entrypoint() -> u32 {
    77
}

fn main() {
    let leading: fn(Zst, u32) -> u32 = leading_zst;
    let middle: fn(u32, Zst, u32) -> u32 = middle_zst;
    let tracked: fn(Zst, u32) -> u32 = tracked_leading_zst;
    let closure: fn(Zst, u32) -> u32 = |_, value| value.wrapping_add(4);
    let multi_zst_closure: fn(Zst, u32, Zst, u32) -> u32 =
        |_, left, _, right| left.wrapping_mul(2).wrapping_add(right);

    test_eq!(black_box(leading)(Zst, 10), 11);
    test_eq!(black_box(middle)(20, Zst, 22), 42);
    test_eq!(black_box(tracked)(Zst, 30), 33);
    test_eq!(black_box(closure)(Zst, 40), 44);
    test_eq!(black_box(STATIC_MIDDLE)(50, Zst, 8), 58);
    test_eq!(black_box(multi_zst_closure)(Zst, 9, Zst, 4), 22);

    let captured = black_box(7_u32);
    let rust_call = invoke_rust_call(
        |_, left, _, right| captured.wrapping_add(left).wrapping_add(right),
        12,
        23,
    );
    test_eq!(rust_call, 42);

    let outer = Outer {
        word: 0x1234_5678,
        inner: Inner { byte: 9, zst: Zst },
    };
    let outer_ptr = black_box(&outer as *const Outer);
    let projected = unsafe { core::ptr::addr_of!((*outer_ptr).inner.zst) } as usize;
    let expected = (outer_ptr as usize)
        .wrapping_add(core::mem::offset_of!(Outer, inner))
        .wrapping_add(core::mem::offset_of!(Inner, zst));
    test_eq!(projected, expected);

    test_zst_sequence_addresses();
    test_enum_zst_addresses();
    test_eq!(test_projection_reads_writes(), 122);
    let lookalike = ordinary_lookalikes::RustcCLRInteropManagedClass { value: 5 };
    test_eq!(lookalike.value, 5);
    test_eq!(ordinary_lookalikes::rustc_clr_interop_managed_ld_len(9), 19);
    test_eq!(rustc_codegen_clr_comptime_entrypoint(), 77);
}
