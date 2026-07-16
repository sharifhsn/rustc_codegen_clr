#[unsafe(no_mangle)]
pub extern "C" fn rust_native_multiply(left: i32, right: i32) -> i32 {
    left * right
}
