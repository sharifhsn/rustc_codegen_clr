// A REAL Microsoft.Extensions.Hosting generic host, driving Rust-implemented `IHostedService`s
// through its actual lifecycle (`AddHostedService<T>()` -> `host.StartAsync()`/`StopAsync()`), not
// a hand-rolled call to the interface members.
//
// `SyncWorker` (from `cd_bgservice.dll`) implements `IHostedService` directly (no `BackgroundService`
// base class) and does synchronous work before returning `Task.CompletedTask` -- proving a Rust
// hosted service can do real per-lifecycle-callback work without touching the async ceiling.
//
// `LoopWorker` does the same, but `StartAsync` additionally spins a background OS thread running a
// blocking `loop { ...; thread::sleep(..) }` -- a genuinely long-running worker, sidestepping
// `async`/`await` entirely by staying synchronous-but-blocking on its own thread.

using System;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;

public static class Program
{
    public static async Task<int> Main()
    {
        int pass = 0, total = 0;
        void Check(string name, bool ok)
        {
            total++;
            if (ok) pass++;
            Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}");
        }

        // ---- Build a REAL generic host with both Rust-implemented IHostedServices registered ----
        // `services.AddHostedService<T>()` -- the idiomatic, GENERIC-constrained registration
        // (`where THostedService : class, IHostedService`) -- now compiles here. It used to fail
        // with CS0311/CS0012 because the backend's `is_bcl_assembly()` mis-stamped
        // `Microsoft.Extensions.Hosting.Abstractions`'s `AssemblyRef` with CoreLib's own ECMA
        // public-key token instead of its real one; see the (now historical) writeup at the
        // bottom of this file and `docs/RUST_PARITY_ROADMAP.md` Tier-0 item 3 for the fix.
        IHost host = Host.CreateDefaultBuilder()
            .ConfigureServices(services =>
            {
                services.AddHostedService<SyncWorker>();
                services.AddHostedService<LoopWorker>();
            })
            .Build();

        Check("typeof(SyncWorker) implements IHostedService (runtime reflection)",
              typeof(IHostedService).IsAssignableFrom(typeof(SyncWorker)));
        Check("typeof(LoopWorker) implements IHostedService (runtime reflection)",
              typeof(IHostedService).IsAssignableFrom(typeof(LoopWorker)));

        Check("StartCount == 0 before host starts", SyncWorker.StartCount() == 0);
        Check("LoopWorker not running before host starts", !LoopWorker.IsRunning());

        // The host lifecycle itself invokes StartAsync on every registered IHostedService.
        await host.StartAsync();

        Check("SyncWorker.StartAsync ran via host.StartAsync()", SyncWorker.StartCount() == 1);
        Check("LoopWorker.StartAsync ran via host.StartAsync()", LoopWorker.IsRunning());

        // Give the background thread LoopWorker.StartAsync spun a moment to actually tick --
        // proof the loop is a real running OS thread, not just a flag flip.
        int ticksBeforeWait = LoopWorker.Ticks();
        await Task.Delay(200);
        int ticksAfterWait = LoopWorker.Ticks();
        Check($"LoopWorker background thread ticked ({ticksBeforeWait} -> {ticksAfterWait})",
              ticksAfterWait > ticksBeforeWait);

        // The host lifecycle invokes StopAsync on shutdown, in reverse registration order.
        await host.StopAsync();

        Check("SyncWorker.StopAsync ran via host.StopAsync()", SyncWorker.StopCount() == 1);
        Check("LoopWorker.StopAsync signaled the loop to stop", LoopWorker.IsRunning() == false
              || WaitUntilStopped());

        Console.WriteLine($"cd_bgservice: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    // LoopWorker.StopAsync only *signals* the loop (doesn't join it); give the background thread a
    // short grace window to actually observe the flag and exit before failing the check.
    private static bool WaitUntilStopped()
    {
        for (int i = 0; i < 50; i++)
        {
            if (!LoopWorker.IsRunning()) return true;
            Thread.Sleep(20);
        }
        return !LoopWorker.IsRunning();
    }
}

// ============================================================================================
// HISTORY: why `services.AddHostedService<SyncWorker>()` USED TO NOT COMPILE (now fixed)
// ============================================================================================
//
// `AddHostedService<THostedService>()` has a `where THostedService : class, IHostedService`
// constraint, so Roslyn must prove `SyncWorker : IHostedService` at compile time by reading
// `cd_bgservice.dll`'s own metadata (its `InterfaceImpl` row naming
// `[Microsoft.Extensions.Hosting.Abstractions]Microsoft.Extensions.Hosting.IHostedService`).
//
// The backend used to resolve that `implements = "[Asm]Ns.Type"` string into an `AssemblyRef` row
// via `is_bcl_assembly(name)` (`cilly/src/ir/pe_exporter/tables.rs`, ported from
// `cilly/src/ir/il_exporter/mod.rs`), which treated ANY assembly name starting with `"Microsoft"`
// as part of the shared framework and stamped it with CoreLib's own ECMA public-key token
// (`B0 3F 5F 7F 11 D5 0A 3A`) and the target `.ver` triplet (`8:0:0:0`).
//
// That heuristic was WRONG for `Microsoft.Extensions.*` NuGet packages: this repo's own probe
// (`typeof(IHostedService).Assembly.FullName` against the real `Microsoft.Extensions.Hosting`
// 8.0.0 package) prints:
//
//   Microsoft.Extensions.Hosting.Abstractions, Version=8.0.0.0, Culture=neutral,
//   PublicKeyToken=adb9793829ddae60
//
// -- a DIFFERENT public-key token (`adb9793829ddae60`, Microsoft's "extensions/aspnetcore"
// signing key, not `b03f5f7f11d50a3a`, CoreLib's ECMA token). So `cd_bgservice.dll` used to carry
// an `AssemblyRef` for `Microsoft.Extensions.Hosting.Abstractions` with the WRONG identity, and
// Roslyn -- which resolves types by exact (name, version, culture, public-key-token) tuple, not by
// name alone -- treated it as a *different, unresolved* assembly:
//
//   error CS0311: The type 'SyncWorker' cannot be used as type parameter 'THostedService' ...
//     There is no implicit reference conversion from 'SyncWorker' to
//     'Microsoft.Extensions.Hosting.IHostedService'.
//   error CS0012: The type 'IHostedService' is defined in an assembly that is not referenced.
//     You must add a reference to assembly 'Microsoft.Extensions.Hosting.Abstractions,
//     Version=8.0.0.0, Culture=neutral, PublicKeyToken=b03f5f7f11d50a3a'.
//
// (Note the error named the WRONG token as the "missing" one -- proof Roslyn was looking for the
// mis-stamped identity the backend emitted, not the real package.)
//
// FIX (`cilly/src/ir/il_exporter/mod.rs::bcl_public_key_token` +
// `cilly/src/ir/pe_exporter/tables.rs::bcl_public_key_token`): `is_bcl_assembly`'s blanket
// `name.starts_with("Microsoft")` heuristic is replaced with a small, verified table --
// `System.*`/CoreLib still get the ECMA token, but the `Microsoft.Extensions.*`/
// `Microsoft.AspNetCore.*`/`Microsoft.EntityFrameworkCore*` family now gets its real
// `adb9793829ddae60` token instead. A third-party NuGet package that merely happens to be named
// `Microsoft.Foo` (outside both families) now correctly falls through to a name-only extern
// instead of being mis-stamped with a key it was never signed with. See
// `docs/RUST_PARITY_ROADMAP.md` Tier-0 item 3.
//
// The formerly-necessary WORKAROUND (registering via the non-generic
// `AddSingleton(typeof(IHostedService), typeof(SyncWorker))` overload, which has no
// `where T : IHostedService` constraint and so never needed Roslyn to prove the interface
// relationship at compile time) is no longer needed and is not used above; kept here only as
// context for readers of this history section.
