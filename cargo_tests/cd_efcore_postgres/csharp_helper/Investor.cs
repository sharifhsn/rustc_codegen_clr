using System;
using System.Collections.Generic;
using System.ComponentModel.DataAnnotations.Schema;
using System.Linq;

namespace CdEfCorePg;

/// <summary>
/// Same fintech-shaped entity as `cargo_tests/cd_efcore/csharp_helper/Investor.cs`, mirrored here
/// verbatim (shape-for-shape) but targeting Npgsql/PostgreSQL instead of Sqlite -- see that file's
/// doc comment for the primary-offerings rationale.
/// </summary>
public class Investor
{
    public Guid Id { get; set; }
    public string Name { get; set; } = string.Empty;
    public Guid? PartnerId { get; set; }
    public DateTime CreatedAt { get; set; }

    public List<Subscription> Subscriptions { get; set; } = new();

    [NotMapped]
    public Subscription[] SubscriptionsArray => Subscriptions.ToArray();
}
