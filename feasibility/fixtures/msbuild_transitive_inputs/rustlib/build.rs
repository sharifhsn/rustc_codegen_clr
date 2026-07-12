use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=build-input.txt");
    let value = fs::read_to_string("build-input.txt").expect("read build-input.txt");
    let value = value.trim();
    assert!(value.bytes().all(|byte| byte.is_ascii_digit()));
    println!("cargo:rustc-env=FIXTURE_BUILD_INPUT={value}");
}
