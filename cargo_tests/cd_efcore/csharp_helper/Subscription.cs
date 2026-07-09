using System;

namespace CdEfCore;

/// <summary>
/// EF6: the "many" side of a standard one-to-many navigation off <see cref="Investor"/>
/// (`Investor.Id` &lt;-&gt; `Subscription.InvestorId`, resolved by EF Core's default FK
/// convention -- no explicit `OnModelCreating` fluent config needed).
/// </summary>
public class Subscription
{
    public Guid Id { get; set; }
    public Guid InvestorId { get; set; }
    public string Kind { get; set; } = string.Empty;
    public DateTime SubscribedAt { get; set; }

    public Investor? Investor { get; set; }
}
