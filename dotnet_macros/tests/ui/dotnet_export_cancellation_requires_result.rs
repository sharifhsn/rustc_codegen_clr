use dotnet_macros::dotnet_export;

#[dotnet_export(cancellation = "task")]
pub async fn not_a_cancellation_result() -> i32 {
    1
}

fn main() {}
