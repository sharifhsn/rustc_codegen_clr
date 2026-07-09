// THROWAWAY probe: try to instantiate the Rust `RustBgService` (extends BackgroundService,
// .overrides ExecuteAsync) and run it through a real host, to see WHERE (if anywhere) it fails --
// Rust/backend compile, C# compile, CLR type load, or host lifecycle.
//
// RESULT (recorded here rather than a separate report file, per repo convention of keeping
// findings next to the repro): `dotnet bin/Debug/net8.0/probe.dll` exits with code 139 (SIGSEGV)
// and prints NOTHING -- not even the very first `Console.WriteLine` inside the first `try` block.
// CoreCLR crashes natively while resolving/loading `RustBgService` (extending the real
// `Microsoft.Extensions.Hosting.BackgroundService` and `.override`-ing its abstract
// `ExecuteAsync`), before any of this program's own code runs. This empirically confirms
// `rustc_codegen_clr_mark_last_method_override`'s own doc warning that "general base-class
// wrapping (a framework type with a non-trivial constructor, protected members, ...) is a
// separate, larger, unaddressed problem" -- and shows the failure mode is worse than a graceful
// `TypeLoadException`: it's a hard native crash. NOT attempted further (out of scope for this
// investigation, which stays confined to `cargo_tests/cd_bgservice/`).
using System;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;

try
{
    var t = typeof(RustBgService);
    Console.WriteLine($"typeof(RustBgService) resolved: {t.FullName}, BaseType={t.BaseType}");
    var instance = Activator.CreateInstance(t);
    Console.WriteLine($"Activator.CreateInstance succeeded: {instance}");
}
catch (Exception e)
{
    Console.WriteLine($"FAILED at type-load/instantiate: {e.GetType().Name}: {e.Message}");
    return;
}

try
{
    IHost host = Host.CreateDefaultBuilder()
        .ConfigureServices(services =>
        {
            services.AddSingleton(typeof(IHostedService), typeof(RustBgService));
        })
        .Build();
    await host.StartAsync();
    Console.WriteLine("host.StartAsync() with RustBgService SUCCEEDED");
    await host.StopAsync();
    Console.WriteLine("host.StopAsync() SUCCEEDED");
}
catch (Exception e)
{
    Console.WriteLine($"FAILED at host lifecycle: {e.GetType().Name}: {e.Message}");
}
