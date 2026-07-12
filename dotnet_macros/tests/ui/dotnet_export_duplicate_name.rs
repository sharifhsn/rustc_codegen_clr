use dotnet_macros::dotnet_export;

#[dotnet_export(name = "ParsePosition", name = "ParseAgain")]
pub fn parse_position() -> i32 {
    1
}

fn main() {}
