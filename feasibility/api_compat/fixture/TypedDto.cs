namespace ApiCompatFixture;

public enum InvoiceState : int
{
    Draft = 1,
    Posted = 2,
}

public sealed class InvoiceDto
{
    public InvoiceDto(decimal amount, DateOnly? date, string? memo)
    {
        Amount = amount;
        Date = date;
        Memo = memo;
    }

    public decimal Amount { get; }
    public DateOnly? Date { get; }
    public string? Memo { get; }
    public InvoiceState State { get; init; }

    public string Format(string? prefix) => $"{prefix}{Amount}";
}
