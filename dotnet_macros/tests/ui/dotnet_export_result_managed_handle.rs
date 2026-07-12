use dotnet_macros::dotnet_export;

pub struct TaskT<T>(T);

#[dotnet_export(error = "exception")]
pub fn fallible_task() -> Result<TaskT<i32>, String> {
    Ok(TaskT(42))
}

fn main() {}
