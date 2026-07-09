// Standalone C# smoke test: confirm EF Core + Sqlite actually resolves and runs in this
// environment BEFORE any Rust-side effort is spent. Run with `dotnet run`.
using System;
using System.Linq;
using System.Linq.Expressions;
using CdEfCore;
using Microsoft.EntityFrameworkCore;

using var ctx = InvestorDbContext.CreateContext();

Console.WriteLine("=== Smoke test: seeded rows ===");
foreach (var inv in ctx.Investors.OrderBy(i => i.CreatedAt))
{
    Console.WriteLine($"{inv.Id} {inv.Name} {inv.PartnerId} {inv.CreatedAt:o}");
}

// Build an Expression<Func<Investor,bool>> exactly the shape Rust will build via
// mycorrhiza::linq -- a member access on a real property compared to a constant.
var param = Expression.Parameter(typeof(Investor), "i");
var member = Expression.PropertyOrField(param, "Name");
var constant = Expression.Constant("Acme", typeof(string));
var body = Expression.Equal(member, constant);
var predicate = Expression.Lambda<Func<Investor, bool>>(body, param);

var query = ctx.Investors.Where(predicate);
var sql = query.ToQueryString();
Console.WriteLine("=== Translated SQL ===");
Console.WriteLine(sql);

var results = query.ToList();
Console.WriteLine($"=== Materialized rows: {results.Count} ===");
foreach (var r in results)
{
    Console.WriteLine($"{r.Name} {r.CreatedAt:o}");
}

bool sqlOk = sql.Contains("WHERE", StringComparison.OrdinalIgnoreCase) && sql.Contains("Acme");
bool countOk = results.Count == 2;
Console.WriteLine(sqlOk && countOk ? "SMOKE TEST PASS" : "SMOKE TEST FAIL");

