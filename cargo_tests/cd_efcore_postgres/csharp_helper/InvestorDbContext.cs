using System;
using System.Linq;
using Microsoft.EntityFrameworkCore;

namespace CdEfCorePg;

/// <summary>
/// Postgres counterpart of `cargo_tests/cd_efcore/csharp_helper/InvestorDbContext.cs`. Same shape
/// (primary-constructor DbContext, `CreateContext()` is the only Rust-visible entry point), but
/// wired to `Npgsql.EntityFrameworkCore.PostgreSQL` against a REAL Postgres server instead of
/// SQLite. The connection string comes from the `CD_EFCORE_PG_CONNSTR` env var (set by the test
/// runner to point at a real `postgres:16` instance) with a localhost fallback for interactive use.
/// </summary>
public class InvestorDbContext(DbContextOptions<InvestorDbContext> options) : DbContext(options)
{
    public DbSet<Investor> Investors => Set<Investor>();
    public DbSet<Subscription> Subscriptions => Set<Subscription>();

    public static string ConnectionString =>
        Environment.GetEnvironmentVariable("CD_EFCORE_PG_CONNSTR")
        ?? "Host=localhost;Port=55433;Database=cd_efcore_pg;Username=postgres;Password=cdpass";

    /// The ONLY entry point Rust should ever call. All fluent DbContextOptionsBuilder wiring stays
    /// on the C# side. Unlike the Sqlite proof (which uses a shared-cache in-memory connection kept
    /// alive by EF), Postgres is a real out-of-process server, so each `CreateContext()` call opens
    /// its own connection via the connection string -- no keep-alive object needed for the DB itself
    /// to persist between contexts, which is a MORE realistic story for the write-durability proof
    /// below (no possibility of accidentally reading the same in-memory backing store).
    public static InvestorDbContext CreateContext()
    {
        var options = new DbContextOptionsBuilder<InvestorDbContext>()
            .UseNpgsql(ConnectionString)
            .Options;

        var ctx = new InvestorDbContext(options);
        // Real migration flow, same as the Sqlite proof: apply the recorded migration history
        // (Migrations/*.cs, generated via `dotnet ef migrations add InitialCreate` against the
        // Npgsql provider) instead of `EnsureCreated()`'s model-sync. Stays synchronous.
        ctx.Database.Migrate();

        if (!ctx.Investors.Any())
        {
            var partnerId = Guid.NewGuid();
            var acme1 = new Investor { Id = Guid.NewGuid(), Name = "Acme", PartnerId = partnerId, CreatedAt = new DateTime(2024, 1, 15, 0, 0, 0, DateTimeKind.Utc) };
            var globex = new Investor { Id = Guid.NewGuid(), Name = "Globex", PartnerId = null, CreatedAt = new DateTime(2023, 6, 1, 0, 0, 0, DateTimeKind.Utc) };
            var acme2 = new Investor { Id = Guid.NewGuid(), Name = "Acme", PartnerId = null, CreatedAt = new DateTime(2025, 3, 10, 0, 0, 0, DateTimeKind.Utc) };
            var initech = new Investor { Id = Guid.NewGuid(), Name = "Initech", PartnerId = partnerId, CreatedAt = new DateTime(2022, 11, 20, 0, 0, 0, DateTimeKind.Utc) };
            ctx.Investors.AddRange(acme1, globex, acme2, initech);

            ctx.Subscriptions.AddRange(
                new Subscription { Id = Guid.NewGuid(), InvestorId = acme1.Id, Kind = "Seed", SubscribedAt = new DateTime(2024, 1, 20, 0, 0, 0, DateTimeKind.Utc) },
                new Subscription { Id = Guid.NewGuid(), InvestorId = acme1.Id, Kind = "SeriesA", SubscribedAt = new DateTime(2024, 8, 5, 0, 0, 0, DateTimeKind.Utc) },
                new Subscription { Id = Guid.NewGuid(), InvestorId = globex.Id, Kind = "SeriesA", SubscribedAt = new DateTime(2023, 7, 1, 0, 0, 0, DateTimeKind.Utc) },
                new Subscription { Id = Guid.NewGuid(), InvestorId = initech.Id, Kind = "Seed", SubscribedAt = new DateTime(2022, 12, 1, 0, 0, 0, DateTimeKind.Utc) }
                // `acme2` intentionally has ZERO subscriptions, same as the Sqlite proof.
            );

            ctx.SaveChanges();
        }

        return ctx;
    }
}
