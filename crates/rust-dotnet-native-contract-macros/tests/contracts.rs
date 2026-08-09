use rust_dotnet_native_contract_macros::native_export;

#[native_export]
pub fn sum(values: &[i32], multiplier: i32) -> Result<i64, i32> {
    Ok(values
        .iter()
        .map(|value| i64::from(*value) * i64::from(multiplier))
        .sum())
}

#[native_export]
pub fn increment(values: &mut [i32]) -> Result<i32, i32> {
    for value in values {
        *value += 1;
    }
    Ok(7)
}

#[native_export]
pub fn failure(_value: i32) -> Result<i32, i32> {
    Err(42)
}

#[native_export]
pub fn panics(_value: i32) -> Result<i32, i32> {
    panic!("contained by export shim")
}

#[native_export]
pub fn greet(name: &str) -> Result<String, i32> {
    Ok(format!("Hello, {name}!"))
}

#[native_export]
pub fn doubled(values: &[i32]) -> Result<Vec<i32>, i32> {
    Ok(values.iter().map(|value| value * 2).collect())
}

#[test]
fn exported_slice_and_scalar_call_is_safe_for_implementation() {
    let values = [2, 3, 5];
    let mut output = 0_i64;
    let status =
        unsafe { __rust_dotnet_native_export_sum(values.as_ptr(), values.len(), 4, &mut output) };
    assert_eq!(status, 0);
    assert_eq!(output, 40);
}

#[test]
fn empty_null_slice_is_accepted_but_nonempty_null_slice_is_not() {
    let mut output = 99_i64;
    let empty_status =
        unsafe { __rust_dotnet_native_export_sum(core::ptr::null(), 0, 1, &mut output) };
    assert_eq!(empty_status, 0);
    assert_eq!(output, 0);

    let invalid_status =
        unsafe { __rust_dotnet_native_export_sum(core::ptr::null(), 2, 1, &mut output) };
    assert_ne!(invalid_status, 0);
}

#[test]
fn invalid_output_and_misaligned_input_are_rejected_before_safe_rust_runs() {
    let values = [2_i32, 3];
    assert_ne!(
        unsafe {
            __rust_dotnet_native_export_sum(values.as_ptr(), values.len(), 1, core::ptr::null_mut())
        },
        0
    );

    let bytes = [0_u8; 16];
    let misaligned = unsafe { bytes.as_ptr().add(1).cast::<i32>() };
    let mut output = 0_i64;
    assert_ne!(
        unsafe { __rust_dotnet_native_export_sum(misaligned, 1, 1, &mut output) },
        0
    );
}

#[test]
fn mutable_slice_is_mutated_without_exposing_unsafe_to_implementation() {
    let mut values = [10, 20];
    let mut output = 0_i32;
    let status = unsafe {
        __rust_dotnet_native_export_increment(values.as_mut_ptr(), values.len(), &mut output)
    };
    assert_eq!(status, 0);
    assert_eq!(output, 7);
    assert_eq!(values, [11, 21]);
}

#[test]
fn status_and_panic_do_not_cross_the_abi() {
    let mut output = 0_i32;
    assert_eq!(
        unsafe { __rust_dotnet_native_export_failure(1, &mut output) },
        42
    );
    assert_ne!(
        unsafe { __rust_dotnet_native_export_panics(1, &mut output) },
        0
    );
}

#[test]
fn utf8_inputs_and_owned_strings_are_safe_and_freed_by_the_exporter() {
    let name = "Ferris 🦀";
    let mut pointer = core::ptr::null_mut();
    let mut length = 0;
    let mut capacity = 0;
    let status = unsafe {
        __rust_dotnet_native_export_greet(
            name.as_ptr(),
            name.len(),
            &mut pointer,
            &mut length,
            &mut capacity,
        )
    };
    assert_eq!(status, 0);
    let value = unsafe { core::slice::from_raw_parts(pointer, length) };
    assert_eq!(str::from_utf8(value).unwrap(), "Hello, Ferris 🦀!");
    unsafe { __rust_dotnet_native_free_greet(pointer, length, capacity) };

    let invalid = [0xff_u8];
    assert_ne!(
        unsafe {
            __rust_dotnet_native_export_greet(
                invalid.as_ptr(),
                invalid.len(),
                &mut pointer,
                &mut length,
                &mut capacity,
            )
        },
        0
    );
}

#[test]
fn owned_vectors_are_copied_then_freed_by_the_matching_exporter() {
    let values = [2, -3, 5];
    let mut pointer = core::ptr::null_mut();
    let mut length = 0;
    let mut capacity = 0;
    let status = unsafe {
        __rust_dotnet_native_export_doubled(
            values.as_ptr(),
            values.len(),
            &mut pointer,
            &mut length,
            &mut capacity,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(
        unsafe { core::slice::from_raw_parts(pointer, length) },
        [4, -6, 10]
    );
    unsafe { __rust_dotnet_native_free_doubled(pointer, length, capacity) };
}
