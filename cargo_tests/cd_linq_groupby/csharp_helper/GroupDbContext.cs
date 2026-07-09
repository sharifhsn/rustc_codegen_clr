using System;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;

namespace CdLinqGroup;

/// The ONLY entry point Rust calls into the C# side's fluent EF wiring — mirrors `cd_efcore`'s
/// `InvestorDbContext.CreateContext()`. Uses `EnsureCreated()` (not a migration history) since this
/// crate proves query SHAPES (`GroupBy`/`Join`/`SelectMany`), not the migration pipeline `cd_efcore`
/// already covers.
public class GroupDbContext(DbContextOptions<GroupDbContext> options) : DbContext(options)
{
    public DbSet<Investor> Investors => Set<Investor>();
    public DbSet<Subscription> Subscriptions => Set<Subscription>();

    public static GroupDbContext CreateContext()
    {
        // Real Sqlite provider (not EF's "InMemory" provider, which never translates to SQL) over a
        // shared-cache in-memory database — same pattern as `cd_efcore`, distinct cache name so the
        // two crates' processes never collide.
        var connection = new SqliteConnection("Data Source=file:cd_linq_groupby_mem?mode=memory&cache=shared");
        connection.Open();

        var options = new DbContextOptionsBuilder<GroupDbContext>()
            .UseSqlite(connection)
            .Options;

        var ctx = new GroupDbContext(options);
        ctx.Database.EnsureCreated();

        if (!ctx.Investors.Any())
        {
            var acme = new Investor { Id = Guid.NewGuid(), Name = "Acme", Code = 1 };
            var globex = new Investor { Id = Guid.NewGuid(), Name = "Globex", Code = 2 };
            ctx.Investors.AddRange(acme, globex);

            // 5 subscriptions across the 2 investors, 2 `Kind` buckets:
            //   SeriesA: Acme/100, Acme/150, Globex/300  -> count 3, sum 550
            //   SeriesB: Acme/200, Globex/50              -> count 2, sum 250
            ctx.Subscriptions.AddRange(
                new Subscription { Id = Guid.NewGuid(), InvestorId = acme.Id, InvestorCode = acme.Code, Kind = "SeriesA", Amount = 100 },
                new Subscription { Id = Guid.NewGuid(), InvestorId = acme.Id, InvestorCode = acme.Code, Kind = "SeriesA", Amount = 150 },
                new Subscription { Id = Guid.NewGuid(), InvestorId = acme.Id, InvestorCode = acme.Code, Kind = "SeriesB", Amount = 200 },
                new Subscription { Id = Guid.NewGuid(), InvestorId = globex.Id, InvestorCode = globex.Code, Kind = "SeriesA", Amount = 300 },
                new Subscription { Id = Guid.NewGuid(), InvestorId = globex.Id, InvestorCode = globex.Code, Kind = "SeriesB", Amount = 50 }
            );

            ctx.SaveChanges();
        }

        return ctx;
    }
}
