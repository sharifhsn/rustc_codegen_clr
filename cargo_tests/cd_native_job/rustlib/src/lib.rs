#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::dotnet_native_job;
use pinvoke_async_callback::{Registration, live_workers};
use rust_dotnet_pinvoke::{NativeJobController, NativeStatusError};

fn stop_error_code(error: NativeStatusError) -> i32 {
    error.code()
}

/// The only API-specific part of the managed job: connect its native callback to the standard
/// controller. `#[dotnet_native_job]` generates the C# class, lifecycle registry, dispatcher-safe
/// progress queue, cancellation/result/error surface, retryable stop, and `IDisposable` contract.
#[dotnet_native_job(
    class = ManagedNativeJob,
    status = ManagedJobStatus,
    registration = Registration,
    result = i32,
    error = i32,
    progress = i32,
    result_empty = i32::MIN,
    error_empty = i32::MIN,
    stop_error = stop_error_code,
    live_workers = live_workers,
)]
fn start_native_job(
    controller: NativeJobController<i32, i32, i32>,
    complete_at: i32,
    fail_at: i32,
    fail_first_stop: bool,
) -> Result<Registration, NativeStatusError> {
    Registration::start(
        move |value| {
            if controller.is_cancellation_requested() {
                return 1;
            }
            if fail_at > 0 && value >= fail_at {
                let _ = controller.fail(value);
                return 1;
            }
            controller.report_progress(value);
            if complete_at > 0 && value >= complete_at {
                let _ = controller.complete(value);
                return 1;
            }
            0
        },
        fail_first_stop,
    )
}
