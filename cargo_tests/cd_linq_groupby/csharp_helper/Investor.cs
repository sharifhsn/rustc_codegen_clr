using System;
using System.Collections.Generic;

namespace CdLinqGroup;

/// Mirrors `cd_efcore`'s `Investor` shape (see that crate's `csharp_helper/Investor.cs`) — kept
/// fully independent (own namespace/assembly `LinqGroupHelper`) per this crate's own EF-backed
/// `IQueryable<Investor>`/`IQueryable<Subscription>` schema.
public class Investor
{
    public Guid Id { get; set; }
    public string Name { get; set; } = string.Empty;

    /// A small `int` surrogate key, used ONLY for the `Join` proof's key selectors — keeps the
    /// Rust-built `Join` key-selector `Expression<Func<Investor,int>>`/`Expression<Func<Subscription,
    /// int>>` types plain `i32` (already the type this crate's `mycorrhiza::linq` machinery is
    /// proven against, e.g. `IntQuery`) instead of needing a new `Guid`-keyed
    /// `Expression<Func<..,Guid>>` type-alias family. `Id` remains the real EF primary key.
    public int Code { get; set; }

    /// Standard one-to-many navigation (FK convention: `Subscription.InvestorId`) — the source
    /// collection `Queryable.SelectMany` flattens.
    public List<Subscription> Subscriptions { get; set; } = new();
}
