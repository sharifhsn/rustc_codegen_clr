using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Design;

namespace CdEfCore;

/// <summary>
/// Design-time factory so `dotnet ef migrations add` (run from `csharp_helper/`) can construct an
/// `InvestorDbContext` without going through <see cref="InvestorDbContext.CreateContext"/>'s
/// keep-alive shared-cache connection (which needs a live, already-open `SqliteConnection` --
/// nothing the EF tooling process wants to manage). The connection string here is only used to
/// build the model for scaffolding the migration; it is never opened by the tool.
/// </summary>
public class InvestorDbContextFactory : IDesignTimeDbContextFactory<InvestorDbContext>
{
    public InvestorDbContext CreateDbContext(string[] args)
    {
        var options = new DbContextOptionsBuilder<InvestorDbContext>()
            .UseSqlite("Data Source=design_time_only.db")
            .Options;
        return new InvestorDbContext(options);
    }
}
