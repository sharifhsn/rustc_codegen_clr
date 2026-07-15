use dotnet_macros::dotnet_class;

#[dotnet_class(constructor_visibility = "protected")]
struct FactoryOwned {
    id: usize,
}

fn main() {}
