use dotnet_macros::dotnet_export;

#[dotnet_export(rename = "ParsePosition")]
pub fn parse_position() -> i32 {
    1
}

fn main() {}
