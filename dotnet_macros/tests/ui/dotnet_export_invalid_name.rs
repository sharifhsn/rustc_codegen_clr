use dotnet_macros::dotnet_export;

#[dotnet_export(name = "parse-position")]
pub fn parse_position() -> i32 {
    1
}

fn main() {}
