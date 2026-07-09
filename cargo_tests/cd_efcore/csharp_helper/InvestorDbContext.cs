using System;
using System.Linq;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;

namespace CdEfCore;

/// <summary>
/// Primary-constructor-style DbContext (mirrors primary-offerings' convention):
/// `class MyDbContext(DbContextOptions&lt;MyDbContext&gt; options) : DbContext(options)`.
/// </summary>
public class InvestorDbContext(DbContextOptions<InvestorDbContext> options) : DbContext(options)
{
    public DbSet<Investor> Investors => Set<Investor>();
    public DbSet<Subscription> Subscriptions => Set<Subscription>();

    /// <summary>
    /// The ONLY entry point Rust should ever call. All fluent DbContextOptionsBuilder wiring
    /// stays on the C# side -- Rust never touches DbContextOptionsBuilder directly. Uses a
    /// real (file-backed, not in-memory-provider) SQLite database via a keep-alive connection
    /// so the schema/data persist for the lifetime of the returned context.
    /// </summary>
    public static InvestorDbContext CreateContext()
    {
        // A shared-cache in-memory SQLite database (real Sqlite provider, real SQL translation --
        // NOT the EF "InMemory" provider). The connection must stay open for the DB to persist;
        // EF owns the connection lifetime here since we pass a live SqliteConnection.
        var connection = new SqliteConnection("Data Source=file:cd_efcore_mem?mode=memory&cache=shared");
        connection.Open();

        var options = new DbContextOptionsBuilder<InvestorDbContext>()
            .UseSqlite(connection)
            .Options;

        var ctx = new InvestorDbContext(options);
        // Real migration flow: apply the recorded migration history (Migrations/*.cs, generated via
        // `dotnet ef migrations add InitialCreate`) instead of syncing the schema straight from the
        // model (`EnsureCreated()`). Stays synchronous (no `MigrateAsync`) per this project's ceiling
        // around `Task<T>`/GC-refs-across-await from Rust.
        ctx.Database.Migrate();

        if (!ctx.Investors.Any())
        {
            var partnerId = Guid.NewGuid();
            var acme1 = new Investor { Id = Guid.NewGuid(), Name = "Acme", PartnerId = partnerId, CreatedAt = new DateTime(2024, 1, 15) };
            var globex = new Investor { Id = Guid.NewGuid(), Name = "Globex", PartnerId = null, CreatedAt = new DateTime(2023, 6, 1) };
            var acme2 = new Investor { Id = Guid.NewGuid(), Name = "Acme", PartnerId = null, CreatedAt = new DateTime(2025, 3, 10) };
            var initech = new Investor { Id = Guid.NewGuid(), Name = "Initech", PartnerId = partnerId, CreatedAt = new DateTime(2022, 11, 20) };
            ctx.Investors.AddRange(acme1, globex, acme2, initech);

            // EF6 seed data: a handful of related Subscription rows (one-to-many off Investor),
            // exercised via `.Include(i => i.Subscriptions)` in `QueryRunner.InvestorsWithSubscriptions`.
            ctx.Subscriptions.AddRange(
                new Subscription { Id = Guid.NewGuid(), InvestorId = acme1.Id, Kind = "Seed", SubscribedAt = new DateTime(2024, 1, 20) },
                new Subscription { Id = Guid.NewGuid(), InvestorId = acme1.Id, Kind = "SeriesA", SubscribedAt = new DateTime(2024, 8, 5) },
                new Subscription { Id = Guid.NewGuid(), InvestorId = globex.Id, Kind = "SeriesA", SubscribedAt = new DateTime(2023, 7, 1) },
                new Subscription { Id = Guid.NewGuid(), InvestorId = initech.Id, Kind = "Seed", SubscribedAt = new DateTime(2022, 12, 1) }
                // `acme2` intentionally has ZERO subscriptions -- proves the Include path handles an
                // empty nested collection correctly, not just the non-empty case.
            );

            ctx.SaveChanges();
        }

        return ctx;
    }
}
