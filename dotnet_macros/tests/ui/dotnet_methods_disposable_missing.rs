use dotnet_macros::dotnet_methods;

struct Resource;

#[dotnet_methods(disposable)]
impl Resource {
    pub fn status() -> i32 {
        1
    }
}

fn main() {}
