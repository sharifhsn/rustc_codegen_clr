using System;
using System.Collections.Generic;
using System.ComponentModel.DataAnnotations.Schema;
using System.Linq;

namespace CdEfCore;

/// <summary>
/// Fintech-shaped entity, modeled after primary-offerings' ConfigurationEntity
/// (Guid Id, string field, nullable Guid FK, DateTime CreatedAt).
/// </summary>
public class Investor
{
    public Guid Id { get; set; }
    public string Name { get; set; } = string.Empty;
    public Guid? PartnerId { get; set; }
    public DateTime CreatedAt { get; set; }

    /// EF6: standard one-to-many navigation (FK convention: `Subscription.InvestorId`). Populated
    /// only when a query opts in via `.Include(i => i.Subscriptions)`
    /// (see `QueryRunner.InvestorsWithSubscriptions`) -- otherwise EF leaves it an empty list rather
    /// than lazy-loading (this project doesn't enable EF's lazy-loading proxies).
    public List<Subscription> Subscriptions { get; set; } = new();

    /// A plain array mirror of <see cref="Subscriptions"/>, NOT mapped to the database (EF would
    /// otherwise try -- and fail -- to interpret an array-of-entity-type property as its own
    /// navigation). Exists purely so Rust can read the nested collection through the SAME
    /// `RustcCLRInteropManagedArray` intrinsics (`ld_len`/`ld_elem_ref`) already proven against
    /// `Investor[]`/`QueryResult.Rows` in Stage 1, instead of needing a new `List<T>` interop path.
    [NotMapped]
    public Subscription[] SubscriptionsArray => Subscriptions.ToArray();
}
