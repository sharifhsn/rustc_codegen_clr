using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Design;

namespace CdEfCorePg;

/// <summary>
/// Design-time factory so `dotnet ef migrations add` (run from `csharp_helper/`) can construct an
/// `InvestorDbContext` against the Npgsql provider. Uses the same `CD_EFCORE_PG_CONNSTR` env var
/// (or localhost fallback) as `InvestorDbContext.CreateContext()` -- the connection is only used to
/// build the model for scaffolding, `dotnet ef migrations add` never opens it, but `dotnet ef
/// database update` (not used by this proof -- `Database.Migrate()` at runtime does the same job)
/// would.
/// </summary>
public class InvestorDbContextFactory : IDesignTimeDbContextFactory<InvestorDbContext>
{
    public InvestorDbContext CreateDbContext(string[] args)
    {
        var options = new DbContextOptionsBuilder<InvestorDbContext>()
            .UseNpgsql(InvestorDbContext.ConnectionString)
            .Options;
        return new InvestorDbContext(options);
    }
}
