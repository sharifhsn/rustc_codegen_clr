use dotnet_macros::dotnet_export;

#[dotnet_export]
pub fn fallible() -> Result<i32, &'static str> {
    Ok(42)
}

fn main() {}
