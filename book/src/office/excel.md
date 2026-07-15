# Use managed Rust from Excel

The supported Office path today is a 64-bit Windows Excel-DNA add-in. Excel-DNA owns worksheet
registration and Excel's special cell model; your calculation code is an ordinary Rust library
compiled into a .NET 10 assembly. This keeps the Rust API reusable from C#, WinUI, services, and
tests instead of coupling it to Excel objects.

This is not VSTO and it is not a macOS `.xll` claim. VSTO remains tied to .NET Framework 4.8. For
cross-platform Office, use an Office web add-in with managed Rust behind an ASP.NET service or local
companion.

## Prerequisites

- Windows x64 with 64-bit desktop Excel;
- the Rust and .NET 10 SDK prerequisites from [installation](../installation.md); and
- the .NET 10 Desktop Runtime on the Excel machine.

## Create and build the add-in

```powershell
cargo dotnet new .\risk-engine --excel
cd .\risk-engine\excel
dotnet build -c Release
```

The C# build invokes the Rust build through `RustDotnet.targets`; no separate terminal build is
required. The packed 64-bit `.xll` is written below:

```text
excel\bin\Release\net10.0-windows\publish
```

Open the packed `.xll` in Excel. The scaffold exposes:

```text
=RUST.STATUS()
=RUST.PORTFOLIO_FV(1000, 7, 10)
=RUST.PORTFOLIO_FV_TABLE(A2:C20)
=RUST.PORTFOLIO_STRESS_ASYNC(1000, 7, 10, 250000)
```

The table input has exactly three columns: principal, annual rate percent, and years. It spills one
result per input row. Invalid shapes and conversions become visible `#RUST!` cells, with the row
number and message, rather than crashing Excel or passing JSON/pointers through worksheets.

The stress function is a cancellable Excel-DNA `Task<T>` UDF. Excel-DNA supplies a hidden final
`CancellationToken` and signals it if the formula is deleted while calculation is outstanding. The
generated C# function copies only the four scalar arguments into `Task.Run`; managed Rust polls the
token during its deterministic scenario sweep. Normal validation failures become `#RUST!` values,
while cancellation remains cancellation instead of being cached as a fake error string.

## Where code belongs

Keep the Excel-specific boundary small:

```text
Excel cells (`object[,]` and Excel errors)
    -> generated C# Excel-DNA functions (validation/conversion)
    -> typed managed Rust exports (`double`, `int`, DTOs, arrays)
    -> optional private native Rust kernel (P/Invoke)
```

Edit `rustlib/src/lib.rs` for reusable calculations. Edit `excel/Functions.cs` only for worksheet
names, descriptions, Excel cell conversion, and Excel-specific threading/error policy.

If a calculation needs a native high-performance kernel, keep its C ABI behind the managed Rust
layer. Add a NuGet native asset with `cargo dotnet add-native`, or a local RID-qualified binary with
`cargo dotnet add-native-file`. The Excel-facing API should remain typed managed .NET so native
ownership and pointers never reach worksheet users.

## Deployment and safety

- Distribute the packed x64 `.xll` and test it on a clean Windows machine before sharing it.
- Match Excel bitness; the scaffold intentionally does not emit a 32-bit add-in.
- Do not call the Excel COM object model from calculation worker threads. Keep worksheet UDFs pure
  and bounded. The generated async UDF captures only scalar values and a cancellation token.
- If a command, ribbon callback, or completed background operation must deliberately change Excel,
  call `ExcelAsyncUtil.QueueAsMacro`. It waits until Excel is ready and runs the action on Excel's
  main thread. Do not capture `Range`, `ExcelReference`, `ExcelDnaUtil.Application`, or another
  Excel C-API/COM object in `Task.Run`.
- The scaffold marks only its pure synchronous functions `IsThreadSafe = true`; that opt-in means
  Excel may execute them concurrently during multithreaded recalculation.
- Treat macro/backend/toolchain upgrades as rebuilds. `cargo dotnet` fingerprints the backend,
  linker, SDK sources, runtime profile, and private sysroot to avoid reusing stale assemblies.

The repository's compile-and-pack acceptance is
`feasibility/excel_dna_acceptance.sh`. A real Excel process launch on a Windows runner remains the
final host proof before this path graduates from preview to fully supported.
