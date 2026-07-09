using System;
using System.Linq;
using System.Linq.Expressions;
using Microsoft.EntityFrameworkCore;

namespace CdEfCorePg;

/// <summary>
/// Mirrors `cargo_tests/cd_efcore/csharp_helper/QueryRunner.cs` exactly (same entry points, same
/// shapes) -- only the underlying provider (Npgsql/Postgres vs Sqlite) differs, which is invisible
/// at this layer since all provider wiring lives in `InvestorDbContext`.
/// </summary>
public class QueryResult
{
    public string Sql { get; set; } = string.Empty;
    public Investor[] Rows { get; set; } = Array.Empty<Investor>();
}

public class WriteResult
{
    public string NewInvestorId { get; set; } = string.Empty;
    public string NewInvestorName { get; set; } = string.Empty;
    public int PersistedCount { get; set; }
}

public class IncludeResult
{
    public string Sql { get; set; } = string.Empty;
    public Investor[] Rows { get; set; } = Array.Empty<Investor>();
}

public static class QueryRunner
{
    public static Investor[] AllInvestors(InvestorDbContext ctx) => ctx.Investors.ToArray();

    /// The real proof: run a Rust-built predicate through EF's REAL Npgsql/Postgres provider.
    /// `ToQueryString()` reflects Postgres SQL (double-quoted identifiers, `$1`-style params get
    /// inlined as literals by `ToQueryString()` same as the Sqlite proof) -- NOT expected to be
    /// byte-identical to the Sqlite-translated SQL, just well-formed Postgres SQL.
    public static QueryResult Run(InvestorDbContext ctx, Expression<Func<Investor, bool>> predicate)
    {
        var q = ctx.Investors.Where(predicate);
        var sql = q.ToQueryString();
        var rows = q.ToArray();
        return new QueryResult { Sql = sql, Rows = rows };
    }

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

    public static IncludeResult InvestorsWithSubscriptions(InvestorDbContext ctx)
    {
        var q = ctx.Investors.Include(i => i.Subscriptions).OrderBy(i => i.CreatedAt);
        var sql = q.ToQueryString();
        var rows = q.ToArray();
        return new IncludeResult { Sql = sql, Rows = rows };
    }

    public static string FirstAppliedMigrationName(InvestorDbContext ctx) =>
        ctx.Database.GetAppliedMigrations().FirstOrDefault() ?? string.Empty;

    public static int AppliedMigrationCount(InvestorDbContext ctx) =>
        ctx.Database.GetAppliedMigrations().Count();

    /// Postgres-only reset helper: this proof runs against a real, PERSISTENT Postgres server
    /// (unlike the Sqlite proof's shared-cache in-memory db, which starts empty every process run).
    /// Truncates both tables (and resets the migrations history) so repeated `cargo dotnet run`
    /// invocations against the same Postgres instance re-seed and re-count deterministically,
    /// exactly like the Sqlite proof's implicit "fresh db per run" behavior.
    public static void ResetDatabase(InvestorDbContext ctx)
    {
        ctx.Database.ExecuteSqlRaw("TRUNCATE TABLE \"Subscriptions\", \"Investors\" RESTART IDENTITY CASCADE;");
    }
}
