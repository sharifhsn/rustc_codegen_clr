use dotnet_macros::dotnet_export;

#[dotnet_export]
pub async fn invalid_async_span(values: &[i32]) -> i32 {
    values.iter().sum()
}

fn main() {}
