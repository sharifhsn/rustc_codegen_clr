use dotnet_macros::dotnet_export;

pub struct PositionDto {
    pub amount: i64,
}

#[dotnet_export]
pub fn bad_return() -> PositionDto {
    PositionDto { amount: 42 }
}

fn main() {}
