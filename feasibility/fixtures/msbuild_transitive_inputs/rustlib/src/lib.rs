#[unsafe(no_mangle)]
pub extern "C" fn fixture_value() -> i32 {
    let build_value = (env!("FIXTURE_BUILD_INPUT").as_bytes()[0] - b'0') as i32;
    fixture_dep::value() + build_value
}
