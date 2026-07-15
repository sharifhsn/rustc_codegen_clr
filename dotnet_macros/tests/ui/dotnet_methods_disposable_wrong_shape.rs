use dotnet_macros::dotnet_methods;

struct Resource;

#[dotnet_methods(disposable)]
impl Resource {
    pub fn dispose(value: i32) -> i32 {
        value
    }
}

fn main() {}
