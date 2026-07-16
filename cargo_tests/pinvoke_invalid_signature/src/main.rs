#[link(name = "unsupported_native")]
unsafe extern "C" {
    #[cfg(feature = "reference")]
    #[link_name = "native_complex_operation"]
    fn complex_operation(value: &str) -> bool;

    #[cfg(feature = "callback-abi")]
    #[link_name = "native_register_callback"]
    fn register_callback(callback: unsafe extern "Rust" fn(i32) -> i32);
}

fn main() {}
