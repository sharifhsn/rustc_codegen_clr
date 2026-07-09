using System;

namespace CdLinqGroup;

/// The "many" side of `Investor`'s navigation. `Kind` is the `GroupBy` key; `Amount` lets the
/// materialized-result proof check a real aggregate (sum per group), not just a count.
public class Subscription
{
    public Guid Id { get; set; }
    public Guid InvestorId { get; set; }

    /// The `Join` key — see `Investor.Code`'s doc comment. NOT a real FK (`InvestorId` is); purely an
    /// `int` mirror of it so the `Join` proof's key selectors stay `i32`-typed.
    public int InvestorCode { get; set; }

    public string Kind { get; set; } = string.Empty;
    public int Amount { get; set; }

    public Investor? Investor { get; set; }
}
