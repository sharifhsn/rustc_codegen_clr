using System;

namespace CdEfCorePg;

/// <summary>
/// Mirrors `cargo_tests/cd_efcore/csharp_helper/Subscription.cs` -- the "many" side of a standard
/// one-to-many navigation off <see cref="Investor"/>, resolved by EF Core's default FK convention.
/// </summary>
public class Subscription
{
    public Guid Id { get; set; }
    public Guid InvestorId { get; set; }
    public string Kind { get; set; } = string.Empty;
    public DateTime SubscribedAt { get; set; }

    public Investor? Investor { get; set; }
}
