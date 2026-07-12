use dotnet_macros::dotnet_export;

pub struct PositionDto {
    pub amount: i64,
}

// Wave 0 pins the current typed-DTO blocker. Wave 2 changes this fixture to compile-pass.
#[dotnet_export]
pub fn bad_export(x: PositionDto) -> i32 {
    x.amount as i32
}

fn main() {}
