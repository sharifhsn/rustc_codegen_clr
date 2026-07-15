# A clean C# -> managed Rust -> native Rust example

This example is a useful shape for a WinUI application: C# asks for a report about a potentially
large file, managed Rust owns validation and presentation policy, and an ordinary native Rust
`cdylib` owns the tight byte-processing loop. Progress travels back as a normal C# delegate.

The public surface stays managed:

```text
WinUI / C#
    MainModule.AnalyzeFile(string, AnalysisMode, ulong?, Func<int, bool>) -> string
        managed Rust compiled to a .NET assembly
            safe Rust facade
                small C ABI
                    native Rust cdylib
```

The JSON return is intentional in this first example. `#[dotnet_export]` supports strings,
primitive nullable values, delegates, enums, and primitive vectors today, but not arbitrary Rust
structs as managed DTOs. A tiny C# record gives the result a conventional application-facing type.

## 1. Native Rust: the high-performance kernel

This crate is ordinary stable Rust and builds as a platform-native dynamic library.

```toml
# native-file-inspector/Cargo.toml
[package]
name = "native-file-inspector"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]
```

```rust
// native-file-inspector/src/lib.rs
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeReport {
    pub bytes_examined: u64,
    pub zero_percent: f64,
    pub ascii_percent: f64,
    pub entropy_bits: f64,
}

pub type ProgressCallback =
    Option<unsafe extern "C" fn(context: *mut c_void, percent: i32) -> i32>;

/// Returns 0 on success, 1 for invalid arguments, or 2 when the callback cancels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rfi_analyze(
    bytes: *const u8,
    len: usize,
    stride: usize,
    callback: ProgressCallback,
    context: *mut c_void,
    out_report: *mut NativeReport,
) -> i32 {
    if bytes.is_null() || out_report.is_null() || stride == 0 {
        return 1;
    }

    let bytes = unsafe { std::slice::from_raw_parts(bytes, len) };
    let mut histogram = [0_u64; 256];
    let mut examined = 0_u64;
    let mut last_progress = -1;

    for (index, &byte) in bytes.iter().enumerate().step_by(stride) {
        histogram[byte as usize] += 1;
        examined += 1;

        let progress = ((index + 1) * 100 / len.max(1)) as i32;
        if progress != last_progress {
            last_progress = progress;
            if let Some(callback) = callback {
                if unsafe { callback(context, progress) } == 0 {
                    return 2;
                }
            }
        }
    }

    let ratio = |count: u64| 100.0 * count as f64 / examined.max(1) as f64;
    let ascii_count: u64 = histogram[0x20..=0x7e].iter().sum();
    let entropy_bits = histogram
        .iter()
        .copied()
        .filter(|&count| count != 0)
        .map(|count| {
            let probability = count as f64 / examined.max(1) as f64;
            -probability * probability.log2()
        })
        .sum();

    unsafe {
        out_report.write(NativeReport {
            bytes_examined: examined,
            zero_percent: ratio(histogram[0]),
            ascii_percent: ratio(ascii_count),
            entropy_bits,
        });
    }
    0
}
```

## 2. Managed Rust: private ABI plus safe facade

Only this module knows that pointers or status integers exist.

```rust
// managed-backend/src/native.rs
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeReport {
    pub bytes_examined: u64,
    pub zero_percent: f64,
    pub ascii_percent: f64,
    pub entropy_bits: f64,
}

pub type ProgressCallback =
    Option<unsafe extern "C" fn(*mut c_void, i32) -> i32>;

#[link(name = "native_file_inspector")]
unsafe extern "C" {
    pub fn rfi_analyze(
        bytes: *const u8,
        len: usize,
        stride: usize,
        callback: ProgressCallback,
        context: *mut c_void,
        out_report: *mut NativeReport,
    ) -> i32;
}
```

The facade uses `rust-dotnet-pinvoke` to keep callback storage heap-stable, contain callback panics,
and expose the out value only after the native status policy accepts it.

```rust
// managed-backend/src/file_inspector.rs
use rust_dotnet_pinvoke::{
    Callback, NativeStatusError, callback_trampoline_return, status_zero, try_out,
};

use crate::native;

callback_trampoline_return! {
    unsafe extern "C" fn progress_trampoline(percent: i32) -> i32;
    on_panic = 0; // ask native code to cancel; never unwind through the C ABI
}

pub fn analyze(
    bytes: &[u8],
    stride: usize,
    mut progress: impl FnMut(i32) -> bool + 'static,
) -> Result<native::NativeReport, NativeStatusError> {
    let mut callback = Callback::new(move |(percent,)| {
        i32::from(progress(percent)) // false asks native code to cancel
    });

    unsafe {
        try_out(|out_report| {
            status_zero(native::rfi_analyze(
                bytes.as_ptr(),
                bytes.len(),
                stride,
                Some(progress_trampoline),
                callback.context(),
                out_report,
            ))
        })
    }
}
```

The exported layer is ordinary Rust. It reads the file, applies the optional limit, turns an enum
into a kernel policy, invokes the C# delegate, and maps `Result::Err` to a catchable C# exception.

```rust
// managed-backend/src/lib.rs
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

mod file_inspector;
mod native;

use dotnet_macros::{dotnet_enum, dotnet_export};
use mycorrhiza::delegate::Func1;

#[dotnet_enum(name = "AnalysisMode")]
#[derive(Clone, Copy)]
#[repr(i32)]
pub enum AnalysisMode {
    Fast = 0,
    Thorough = 1,
}

/// Inspect a file in native Rust and return a JSON report.
#[dotnet_export(enums(AnalysisMode), error = "exception", name = "AnalyzeFile")]
pub fn analyze_file(
    path: &str,
    mode: AnalysisMode,
    max_bytes: Option<u64>,
    progress: Func1<i32, bool>,
) -> Result<String, String> {
    let mut bytes = std::fs::read(path).map_err(|error| format!("Could not read {path}: {error}"))?;
    if let Some(limit) = max_bytes {
        bytes.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    }

    let stride = match mode {
        AnalysisMode::Fast => (bytes.len() / 4_000_000).max(1),
        AnalysisMode::Thorough => 1,
    };
    let report = file_inspector::analyze(&bytes, stride, move |percent| progress.invoke(percent))
    .map_err(|error| format!("Native analysis failed: {error}"))?;

    Ok(format!(
        concat!(
            "{{\"bytesExamined\":{},",
            "\"zeroPercent\":{:.4},",
            "\"asciiPercent\":{:.4},",
            "\"entropyBits\":{:.4}}}"
        ),
        report.bytes_examined,
        report.zero_percent,
        report.ascii_percent,
        report.entropy_bits,
    ))
}
```

The managed crate needs the normal managed-Rust dependencies plus the safe P/Invoke facade:

```toml
# managed-backend/Cargo.toml (relevant part)
[lib]
crate-type = ["cdylib"]

[dependencies]
dotnet_macros = "0.0.1"
mycorrhiza = "0.0.1"
rust-dotnet-pinvoke = "0.0.1"
```

Stage one native binary per target RID with `cargo dotnet add-native-file`; applications then load
the library beside the managed assembly without application-level `DllImport` declarations.

## 3. Ordinary C#: an application-facing service

The rest of the C# application does not know P/Invoke exists. It sees a generated CLR enum, a
nullable primitive, a `Func<int, bool>`, a string, and normal exceptions.

```csharp
// FileAnalysisService.cs
using System.Text.Json;

public sealed record FileAnalysis(
    ulong BytesExamined,
    double ZeroPercent,
    double AsciiPercent,
    double EntropyBits);

public sealed class FileAnalysisService
{
    private static readonly JsonSerializerOptions JsonOptions =
        new(JsonSerializerDefaults.Web);

    public async Task<FileAnalysis> AnalyzeAsync(
        string path,
        AnalysisMode mode,
        ulong? maxBytes,
        IProgress<int>? progress = null,
        CancellationToken cancellationToken = default)
    {
        string json;
        try
        {
            json = await Task.Run(() => MainModule.AnalyzeFile(
                path,
                mode,
                maxBytes,
                percent =>
                {
                    progress?.Report(percent);
                    return !cancellationToken.IsCancellationRequested;
                }));
        }
        catch (Exception) when (cancellationToken.IsCancellationRequested)
        {
            throw new OperationCanceledException(cancellationToken);
        }

        return JsonSerializer.Deserialize<FileAnalysis>(json, JsonOptions)
            ?? throw new InvalidDataException("Rust returned an empty report.");
    }
}
```

A WinUI page can use that service without any unsafe code or interop ceremony:

```csharp
// MainPage.xaml.cs (inside a page with ProgressBar AnalysisProgress and TextBlock Summary)
private readonly FileAnalysisService _analysis = new();

private async void AnalyzeButton_Click(object sender, RoutedEventArgs e)
{
    var picker = new Windows.Storage.Pickers.FileOpenPicker();
    WinRT.Interop.InitializeWithWindow.Initialize(
        picker,
        WinRT.Interop.WindowNative.GetWindowHandle(App.MainWindow));
    picker.FileTypeFilter.Add("*");

    var file = await picker.PickSingleFileAsync();
    if (file is null)
        return;

    try
    {
        var progress = new Progress<int>(value => AnalysisProgress.Value = value);
        FileAnalysis report = await _analysis.AnalyzeAsync(
            file.Path,
            AnalysisMode.Thorough,
            maxBytes: 512UL * 1024 * 1024,
            progress);

        Summary.Text =
            $"{report.BytesExamined:N0} bytes; " +
            $"entropy {report.EntropyBits:F2} bits/byte; " +
            $"ASCII {report.AsciiPercent:F1}%";
    }
    catch (Exception error)
    {
        Summary.Text = error.Message; // includes Rust/native failure context
    }
}
```

## If the native API retains the callback

The example above is scoped: native code finishes using the callback before `rfi_analyze` returns.
If a native API starts a worker and retains the callback, replace `Callback` with
`CallbackRegistration`. The registration closure hands native code the generated context pointer;
the guard then owns that context until native unregistration has stopped future calls and waited
for in-flight calls to finish:

```rust
thread_safe_callback_trampoline_return! {
    unsafe extern "C" fn retained_progress(percent: i32) -> i32;
    on_panic = 0;
}

let registration = unsafe {
    CallbackRegistration::register(
        move |(percent,)| {
            shared_progress.store(percent, Ordering::Release);
            1
        },
        |context| try_out(|token| {
            status_zero(native::start_worker(Some(retained_progress), context, token))
        }),
        unregister_and_join,
    )
}?;

// Keep `registration` in the managed job object. Explicit stop is retryable on failure.
registration.try_unregister()?;
```

`unregister_and_join` is API-specific and must guarantee quiescence. If implicit drop cannot prove
that native code has stopped calling, `CallbackRegistration` deliberately leaks the context rather
than free memory that native code may still use.
