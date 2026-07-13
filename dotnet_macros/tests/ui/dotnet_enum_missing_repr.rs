use dotnet_macros::dotnet_enum;

#[dotnet_enum(name = "Example.Status")]
pub enum Status {
    Ready = 1,
}

fn main() {}
