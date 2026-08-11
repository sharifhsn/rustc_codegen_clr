#![feature(
    lang_items,
    adt_const_params,
    associated_type_defaults,
    core_intrinsics,
    unsized_const_params,
    unsize,
    coerce_unsized
)]
#![allow(internal_features, incomplete_features, unused_variables, dead_code)]
include!("../common.rs");
struct Coercable<'a, T: ?Sized> {
    rf: &'a T,
    next: usize,
}

#[repr(C)]
struct PointerInTheMiddle<'a, T: ?Sized> {
    prefix: u16,
    pointer: &'a T,
    suffix: u32,
}

#[repr(C)]
struct NestedWrapper<'a, T: ?Sized> {
    prefix: u8,
    nested: PointerInTheMiddle<'a, T>,
    suffix: u64,
}

const CONSTANT_ARRAY: &[i32; 4] = &[101, 202, 303, 404];

fn main() {
    let arr: [i32; 8] = [0, 1, 2, 4, 5, 6, 7, 8];
    let coercable: Coercable<'_, [i32; 8]> = black_box(Coercable {
        rf: &arr,
        next: usize::MAX,
    });
    let coerced = coercable as Coercable<'_, [i32]>;
    test_eq!(coerced.next, usize::MAX);

    let nested = black_box(NestedWrapper {
        prefix: 0x5a,
        nested: PointerInTheMiddle {
            prefix: 0x1234,
            pointer: &arr,
            suffix: 0x89ab_cdef,
        },
        suffix: 0x0123_4567_89ab_cdef,
    });
    let nested = nested as NestedWrapper<'_, [i32]>;
    test_eq!(nested.prefix, 0x5a);
    test_eq!(nested.nested.prefix, 0x1234);
    test_eq!(nested.nested.suffix, 0x89ab_cdef);
    test_eq!(nested.suffix, 0x0123_4567_89ab_cdef);
    test_eq!(nested.nested.pointer.len(), arr.len());
    test_eq!(nested.nested.pointer[3], 4);

    // Keep the source as an Operand::Constant so the plan's staging-address path is exercised.
    let constant_slice = CONSTANT_ARRAY as &[i32];
    test_eq!(constant_slice.len(), 4);
    test_eq!(constant_slice[2], 303);

    // Standard NonNull and Pin wrappers add custom CoerceUnsized layers around actual pointer
    // leaves, proving repeated rustc-selected field identities are followed rather than assuming
    // a particular field index or offset. Stack backing keeps this oracle independent of allocator
    // shims.
    let mut non_null_data = [7, 14, 21, 28];
    let non_null: core::ptr::NonNull<[i32; 4]> = core::ptr::NonNull::from(&mut non_null_data);
    let non_null: core::ptr::NonNull<[i32]> = non_null;
    test_eq!(unsafe { non_null.as_ref() }.len(), 4);
    test_eq!(unsafe { non_null.as_ref() }[1], 14);

    let mut pin_data = [9, 18, 27, 36];
    let pinned: core::pin::Pin<&mut [i32; 4]> =
        unsafe { core::pin::Pin::new_unchecked(&mut pin_data) };
    let pinned: core::pin::Pin<&mut [i32]> = pinned;
    test_eq!(pinned.len(), 4);
    test_eq!(pinned[3], 36);

    let cell = std::cell::RefCell::new([11_i32, 22, 33, 44]);
    {
        let borrow = match cell.try_borrow_mut() {
            Ok(borrow) => borrow,
            Err(_) => core::intrinsics::abort(),
        };
        let borrow: std::cell::RefMut<'_, [i32]> = borrow;
        test_eq!(borrow.len(), 4);
        test_eq!(borrow[2], 33);
    }
    test_eq!(cell.try_borrow_mut().is_ok(), true);
}
impl<'a, 'b, A: ?Sized, B: core::marker::Unsize<A> + ?Sized>
    core::ops::CoerceUnsized<Coercable<'a, A>> for Coercable<'a, B>
{
}

impl<'a, 'b, A: ?Sized, B: core::marker::Unsize<A> + ?Sized>
    core::ops::CoerceUnsized<PointerInTheMiddle<'a, A>> for PointerInTheMiddle<'a, B>
{
}

impl<'a, 'b, A: ?Sized, B: core::marker::Unsize<A> + ?Sized>
    core::ops::CoerceUnsized<NestedWrapper<'a, A>> for NestedWrapper<'a, B>
where
    PointerInTheMiddle<'a, B>: core::ops::CoerceUnsized<PointerInTheMiddle<'a, A>>,
{
}
