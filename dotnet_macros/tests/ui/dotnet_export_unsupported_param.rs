use dotnet_macros::dotnet_export;

// `Vec<i32>` is not in the marshalled type set; the macro already rejected this with a clear
// message pre-fix — pinned here as a regression guard for that existing good behavior.
#[dotnet_export]
pub fn bad_export(x: Vec<i32>) -> i32 {
    x.len() as i32
}

fn main() {}
