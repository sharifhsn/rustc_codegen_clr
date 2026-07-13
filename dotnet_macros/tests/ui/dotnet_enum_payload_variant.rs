use dotnet_macros::dotnet_enum;

#[dotnet_enum(name = "Example.Status")]
#[repr(i32)]
pub enum Status {
    Ready(i32),
}

fn main() {}
