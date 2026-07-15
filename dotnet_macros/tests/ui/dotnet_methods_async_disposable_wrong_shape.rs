use dotnet_macros::dotnet_methods;

struct Resource;
struct ResourceHandle;

#[dotnet_methods(async_disposable)]
impl Resource {
    pub fn dispose_async(_this: ResourceHandle) {}
}

fn main() {}
