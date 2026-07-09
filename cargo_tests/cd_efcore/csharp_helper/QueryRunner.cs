using System;
using System.Linq;
using System.Linq.Expressions;
using System.Threading.Tasks;
using Microsoft.EntityFrameworkCore;

namespace CdEfCore;

/// <summary>
/// The result of running a Rust-built predicate through a real EF Core query: the SQL the
/// provider translated the expression tree to (via ToQueryString), plus the materialized rows.
/// A plain array (not List&lt;T&gt;) so Rust can read it via the array-element intrinsics.
/// </summary>
public class QueryResult
{
    public string Sql { get; set; } = string.Empty;
    public Investor[] Rows { get; set; } = Array.Empty<Investor>();
}

/// <summary>
/// EF4: proof that a write Rust drove (Add + SaveChanges, entirely synchronous -- no
/// SaveChangesAsync, see <see cref="QueryRunner.AddInvestorAndVerify"/>) actually persisted.
/// `PersistedCount` comes from re-querying a **brand-new** `InvestorDbContext` (a second
/// `CreateContext()` call, same shared-cache Sqlite connection string) so this isn't just reading
/// back EF's in-memory change tracker -- it proves the row survived to a fresh context/session.
/// </summary>
public class WriteResult
{
    public string NewInvestorId { get; set; } = string.Empty;
    public string NewInvestorName { get; set; } = string.Empty;
    public int PersistedCount { get; set; }
}

/// <summary>
/// EF6: the result of an eager-loaded (`Include`) query -- SQL translation (proves a single
/// round-trip JOIN, no N+1) plus the materialized `Investor` rows, each with its
/// `Subscriptions` navigation collection populated.
/// </summary>
public class IncludeResult
{
    public string Sql { get; set; } = string.Empty;
    public Investor[] Rows { get; set; } = Array.Empty<Investor>();
}

/// <summary>
/// Entry points Rust calls. Every method here is NON-GENERIC on the C# side (the entity type
/// `Investor` is concrete/closed) -- the only generic-method production Rust itself has to do is
/// building the `Expression&lt;Func&lt;Investor,bool&gt;&gt;` value handed to <see cref="Run"/>,
/// mirroring `mycorrhiza::linq::Expr::typed_pred`'s int-specialized version for this entity.
/// </summary>
public static class QueryRunner
{
    /// De-risking step (Stage 1, item 2): materialize ALL rows with no filter, so Rust can prove
    /// it can read a string property off a REAL EF-materialized entity before the predicate
    /// pipeline is built at all.
    public static Investor[] AllInvestors(InvestorDbContext ctx) => ctx.Investors.ToArray();

    /// The real proof: run a Rust-built predicate through EF's Sqlite provider. `Where` on `ctx.Investors`
    /// (an `IQueryable<Investor>`) TRANSLATES the tree -- it does not evaluate it client-side -- so
    /// `ToQueryString()` on the result reflects what the provider actually generated.
    public static QueryResult Run(InvestorDbContext ctx, Expression<Func<Investor, bool>> predicate)
    {
        var q = ctx.Investors.Where(predicate);
        var sql = q.ToQueryString();
        var rows = q.ToArray();
        return new QueryResult { Sql = sql, Rows = rows };
    }

    /// EF4: the ONLY write entry point Rust calls. Constructs a new `Investor` (Rust hands in just
    /// the `Name`, matching the "Rust calls a thin C# helper method" shape -- Rust never touches
    /// `ctx.Investors.Add`/`SaveChanges` or the change tracker directly), adds it to the tracked
    /// `DbSet`, and calls the SYNCHRONOUS `SaveChanges()` (never `SaveChangesAsync`: this project has
    /// a documented ceiling around producing `Task&lt;T&gt;`/GC-refs-across-await from Rust, so this
    /// helper -- and everything it calls -- stays fully sync top to bottom).
    ///
    /// Re-queries via a **second, brand-new** `InvestorDbContext` (own `CreateContext()` call) to
    /// prove the row is actually durable in the shared-cache Sqlite database, not just visible
    /// through the same context's in-memory change tracker.
    public static WriteResult AddInvestorAndVerify(InvestorDbContext ctx, string name)
    {
        var investor = new Investor
        {
            Id = Guid.NewGuid(),
            Name = name,
            PartnerId = null,
            CreatedAt = new DateTime(2026, 7, 8, 0, 0, 0, DateTimeKind.Utc),
        };
        ctx.Investors.Add(investor);
        ctx.SaveChanges();

        using var freshCtx = InvestorDbContext.CreateContext();
        var persisted = freshCtx.Investors.Where(i => i.Name == name).ToArray();

        return new WriteResult
        {
            NewInvestorId = investor.Id.ToString(),
            NewInvestorName = investor.Name,
            PersistedCount = persisted.Length,
        };
    }

    /// EF6: eager-load `Investor.Subscriptions` via `Include` -- a single round-trip query (the
    /// translated SQL is a JOIN, not N+1 separate SELECTs per investor). The `Include(...)` lambda
    /// lives entirely here on the C# side; Rust only requests the eager-load by calling this method
    /// and then reads the nested `Subscriptions` collection off each materialized `Investor`.
    public static IncludeResult InvestorsWithSubscriptions(InvestorDbContext ctx)
    {
        var q = ctx.Investors.Include(i => i.Subscriptions).OrderBy(i => i.CreatedAt);
        var sql = q.ToQueryString();
        var rows = q.ToArray();
        return new IncludeResult { Sql = sql, Rows = rows };
    }

    /// EF-migrations proof: the concrete name of the (single) applied migration --
    /// `Database.GetAppliedMigrations()` reads the `__EFMigrationsHistory` table that
    /// `Database.Migrate()` populates, so a non-empty name here is a real signal the schema came
    /// from migration application, not `EnsureCreated()`'s model-sync (which never writes that
    /// table at all). Returns the first (only) applied migration's name, or "" if none.
    public static string FirstAppliedMigrationName(InvestorDbContext ctx) =>
        ctx.Database.GetAppliedMigrations().FirstOrDefault() ?? string.Empty;

    /// Total count of applied migrations, for a simpler numeric Rust-side assertion.
    public static int AppliedMigrationCount(InvestorDbContext ctx) =>
        ctx.Database.GetAppliedMigrations().Count();

    // ---- EF7 (async): the counterpart of `Run`, but genuinely asynchronous end to end --
    // `ToListAsync()`, not `ToArray()`. Returns a plain `Task<int>` (a fully CLOSED generic --
    // `Task`1<int32>`, no unbound `!0`/`!!0` -- since neither `QueryRunner` nor this method is
    // itself generic over the element type), so producing/consuming it from Rust is the SAME
    // concrete-generic-return shape already proven for every other `Task<T>` in `mycorrhiza::task`
    // -- just row-count instead of the full entity array, to keep the interop surface simple. The
    // real point of this method existing is that a RUST `async fn` awaits it (see
    // `cd_efcore_async`), not that it does anything C# couldn't already do synchronously.
    public static async Task<int> RunAsyncCount(InvestorDbContext ctx, Expression<Func<Investor, bool>> predicate)
    {
        var rows = await ctx.Investors.Where(predicate).ToListAsync();
        return rows.Count;
    }

    /// EF8 (async): the asynchronous counterpart of `AddInvestorAndVerify` -- `SaveChangesAsync`,
    /// not `SaveChanges`, and the durability re-check also goes through `ToListAsync`. Returns the
    /// persisted-count `int`, matching `AddInvestorAndVerify`'s `WriteResult.PersistedCount` field
    /// but as a bare `Task<int>` so a Rust `async fn` can `await_task` it directly.
    public static async Task<int> AddInvestorAndVerifyAsync(InvestorDbContext ctx, string name)
    {
        var investor = new Investor
        {
            Id = Guid.NewGuid(),
            Name = name,
            PartnerId = null,
            CreatedAt = new DateTime(2026, 7, 8, 0, 0, 0, DateTimeKind.Utc),
        };
        ctx.Investors.Add(investor);
        await ctx.SaveChangesAsync();

        using var freshCtx = InvestorDbContext.CreateContext();
        var persisted = await freshCtx.Investors.Where(i => i.Name == name).ToListAsync();
        return persisted.Count;
    }
}
