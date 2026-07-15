use dotnet_macros::dotnet_native_job;

#[dotnet_native_job(
    class = ManagedJob,
    status = JobStatus,
    registration = Registration,
    result = i32,
    error = i32,
    progress = i32,
    result_empty = i32::MIN,
    error_empty = i32::MIN,
    stop_error = stop_error,
)]
fn start(controller: NativeJobController<i32, i32, i32>) -> Result<Registration, StartError> {
    loop {}
}

fn main() {}
