using System;
using System.Linq;
using Microsoft.EntityFrameworkCore;

namespace CdLinqGroup;

/// One summarized `GroupBy` group — the materialized shape Rust reads back (`Kind`/`Count`/`Sum`)
/// after Rust builds the `GroupBy` key-selector expression and drives `Queryable.GroupBy` itself
/// (see `mycorrhiza::linq::group_by`, called directly from `main.rs`).
public class GroupSummary
{
    public string Kind { get; set; } = string.Empty;
    public int Count { get; set; }
    public int Sum { get; set; }
}

/// `Sql` + the materialized `GroupSummary[]` — same "translated SQL + materialized rows" pairing
/// `cd_efcore`'s `QueryResult` uses for `Where`.
public class GroupSummaryResult
{
    public string Sql { get; set; } = string.Empty;
    public GroupSummary[] Groups { get; set; } = Array.Empty<GroupSummary>();
}

/// `Sql` + the materialized `Subscription[]` rows — reused for BOTH the `Join` and `SelectMany`
/// proofs (their result shape, after this crate's simplification for `Join`'s `resultSelector` —
/// see `main.rs` — is `IQueryable<Subscription>` in both cases).
public class SubscriptionQueryResult
{
    public string Sql { get; set; } = string.Empty;
    public Subscription[] Rows { get; set; } = Array.Empty<Subscription>();
}

/// Thin C# helpers Rust calls into: (1) fetch the two real `IQueryable<T>` sources Rust's own
/// `Queryable.GroupBy`/`Join`/`SelectMany` calls consume, and (2) materialize + read
/// `ToQueryString()` off the `IQueryable` RESULT Rust hands back — mirrors `cd_efcore`'s
/// `QueryRunner`. The actual `GroupBy`/`Join`/`SelectMany` calls themselves happen on the RUST side
/// (`mycorrhiza::linq::{group_by,join,select_many}`), unlike `cd_efcore`'s `Where`, which is issued
/// from C# (`QueryRunner.Run`) — this crate exists specifically to prove the Rust-driven path.
public static class QueryHelpers
{
    public static IQueryable<Investor> InvestorsQuery(GroupDbContext ctx) => ctx.Investors;

    public static IQueryable<Subscription> SubscriptionsQuery(GroupDbContext ctx) => ctx.Subscriptions;

    public static GroupSummaryResult SummarizeGroups(IQueryable<IGrouping<string, Subscription>> groups)
    {
        // `groups.ToQueryString()` alone (before any aggregate projection) is NOT what actually
        // translates to a `GROUP BY` — EF Core only lowers `GroupBy` to real relational `GROUP BY`
        // once it sees the aggregate shape consuming it (`Count()`/`Sum()`/...); a bare, unconsumed
        // `IQueryable<IGrouping<K,V>>` instead compiles to a plain `ORDER BY key` stream that EF
        // groups client-side. So capture the SQL off the AGGREGATE-PROJECTED query (the one Rust's
        // `Queryable.GroupBy` call feeds into here), which is what actually contains `GROUP BY`.
        var projected = groups
            .Select(g => new GroupSummary { Kind = g.Key, Count = g.Count(), Sum = g.Sum(s => s.Amount) })
            .OrderBy(g => g.Kind);
        var sql = projected.ToQueryString();
        var summarized = projected.ToArray();
        return new GroupSummaryResult { Sql = sql, Groups = summarized };
    }

    public static SubscriptionQueryResult SummarizeSubscriptions(IQueryable<Subscription> q)
    {
        var sql = q.ToQueryString();
        var rows = q.OrderBy(s => s.Amount).ToArray();
        return new SubscriptionQueryResult { Sql = sql, Rows = rows };
    }
}
