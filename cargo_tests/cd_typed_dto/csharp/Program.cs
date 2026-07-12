using System.Reflection;

static void Check(bool condition, string message)
{
    if (!condition) throw new InvalidOperationException(message);
}

var type = typeof(InvoiceDto);
Check(type.GetConstructor(Type.EmptyTypes) is not null, "DTO has no parameterless constructor");
Check(type.GetProperty("Amount")?.PropertyType == typeof(decimal), "Amount is not System.Decimal");
Check(type.GetProperty("Date")?.PropertyType == typeof(DateOnly?),
    "Date is not System.Nullable<System.DateOnly>");
Check(type.GetProperty("Memo")?.PropertyType == typeof(string), "Memo is not System.String");

var withDate = InvoiceFacade.CreateWithDate(new DateOnly(2025, 3, 14).DayNumber);
Check(withDate.Amount == 123.4500m, "Amount value did not cross the facade");
Check((decimal.GetBits(withDate.Amount)[3] >> 16 & 0x7f) == 4, "Decimal scale was not preserved");
Check(withDate.Date == new DateOnly(2025, 3, 14), "DateOnly value did not cross the facade");
Check(withDate.Memo == "from-rust", "String property value did not cross the facade");

var withoutDate = InvoiceFacade.CreateWithoutDate();
Check(withoutDate.Date is null, "nullable DateOnly should be absent");
Check(withoutDate.Amount == 7.00m, "second DTO amount is wrong");
Check((decimal.GetBits(withoutDate.Amount)[3] >> 16 & 0x7f) == 2, "second decimal scale was not preserved");

var blank = new InvoiceDto();
blank.Amount = 9.10m;
blank.Date = null;
blank.Memo = "set-in-csharp";
Check(blank.Amount == 9.10m && blank.Date is null && blank.Memo == "set-in-csharp",
    "generated CLR properties are not writable/readable");

Console.WriteLine("typed DTO assertions passed");
