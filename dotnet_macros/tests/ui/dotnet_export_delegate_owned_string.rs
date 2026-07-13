use dotnet_macros::dotnet_export;

struct Func1<T, R>(T, R);

#[dotnet_export]
fn unsupported_callback(callback: Func1<String, i32>) -> i32 {
    0
}

fn main() {}
