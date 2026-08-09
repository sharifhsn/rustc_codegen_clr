use rust_dotnet_native_contract_macros::native_import;

// This test deliberately does not call the import, so it checks generated Rust signatures without
// requiring a platform native library at macro-crate test time.
native_import! {
    // libc exists on the supported developer hosts; the declared symbol is never invoked here.
    library = "c";
    pub fn mutate(values: &mut [i32], scale: i32) -> Result<(), i32>;
}

native_import! {
    library = "c";
    pub fn greet(name: &str) -> Result<String, i32>;
}

native_import! {
    library = "c";
    pub fn doubled(values: &[i32]) -> Result<Vec<i32>, i32>;
}

#[test]
fn imported_functions_have_safe_idiomatic_signatures() {
    let _: fn(&mut [i32], i32) -> Result<(), i32> = mutate;
    let _: fn(&str) -> Result<String, i32> = greet;
    let _: fn(&[i32]) -> Result<Vec<i32>, i32> = doubled;
}
