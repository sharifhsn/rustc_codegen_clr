// Step 3 of the async campaign: a genuinely separate C# console app calling and `await`ing a Rust
// `async fn` that itself performs a real, two-await, EF Core workflow (`ToListAsync` then
// `SaveChangesAsync`), exposed via `#[dotnet_export]` as an ordinary `Task<int>`-returning method —
// the same seam shape `cd_export`'s `compute_answer()` proved for a trivial `async { 42 }`, here
// backing a real, stateful, I/O-bound Rust `async fn` (`cd_efcore_async/rustlib/src/lib.rs`,
// `async_investor_workflow`).
//
// Expected value: `acme_count * 1000 + persisted_count`. The seeded EfHelper fixture has 2 "Acme"
// investors (see cargo_tests/cd_efcore/csharp_helper/InvestorDbContext.cs's seed data), and the
// write-then-reread-from-a-fresh-context always finds exactly the 1 row it just wrote (a fresh
// process/database each run), so the expected result is `2 * 1000 + 1 == 2001`.

using System;
using System.Threading.Tasks;

public static class Program
{
    public static int Main()
    {
        return MainAsync().GetAwaiter().GetResult();
    }

    private static async Task<int> MainAsync()
    {
        Console.WriteLine("== cd_efcore_async (C# host) start ==");

        int result = await MainModule.run_investor_workflow();

        Console.WriteLine($"run_investor_workflow() => {result}");

        bool ok = result == 2001;
        Console.WriteLine(ok
            ? "[OK] async EF Core workflow (ToListAsync + sync transform + SaveChangesAsync), driven from a Rust async fn, round-tripped correctly"
            : $"[FAIL] expected 2001, got {result}");

        Console.WriteLine("== cd_efcore_async (C# host) done ==");
        return ok ? 0 : 1;
    }
}
