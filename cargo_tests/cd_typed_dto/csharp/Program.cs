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

var ratePointType = typeof(RatePoint);
Check(ratePointType.IsValueType, "#[dotnet_value] did not emit a CLR value type");
Check(ratePointType.GetProperties().All(property => property.CanRead && property.CanWrite),
    "value-type properties are not readable/writable");
var ratePoint = default(RatePoint);
ratePoint.TenorDays = 730;
ratePoint.Rate = 0.08;
Check(ratePoint.TenorDays == 730 && ratePoint.Rate == 0.08,
    "value-type constructor/properties are wrong");
Check(InvoiceFacade.AnnualizedRate(ratePoint) == 0.04,
    "value type did not cross from C# into managed Rust");
var secondRatePoint = default(RatePoint);
secondRatePoint.TenorDays = 365;
secondRatePoint.Rate = 0.12;
var ratePoints = new[] { ratePoint, secondRatePoint };
Check(InvoiceFacade.SumRatePoints(ratePoints) == 0.20,
    "managed value-type array was not read correctly in Rust");
Check(ReferenceEquals(InvoiceFacade.EchoRatePoints(ratePoints), ratePoints),
    "managed value-type array identity was not preserved");
var invoices = new[] { withDate, withoutDate, blank };
Check(InvoiceFacade.CountInvoices(invoices) == 3,
    "managed reference DTO array length was not observed in Rust");
Check(ReferenceEquals(InvoiceFacade.EchoInvoices(invoices), invoices),
    "managed reference DTO array identity was not preserved");
Check(InvoiceFacade.SumReadonlyRates(ratePoints) == 0.20,
    "IReadOnlyList<RatePoint> projection was not read correctly in Rust");
Check(ReferenceEquals(InvoiceFacade.EchoReadonlyInvoices(invoices), invoices),
    "IReadOnlyList<InvoiceDto> projection did not preserve the implementation object");
var retainedRatesArray = new[] { 0.01, 0.02, 0.03 };
ReadOnlyMemory<double> retainedRates = retainedRatesArray.AsMemory(1, 2);
Check(InvoiceFacade.SumReadonlyMemory(retainedRates) == 0.05,
    "ReadOnlyMemory<double> was not consumed through its sliced view");
var echoedRates = InvoiceFacade.EchoReadonlyMemory(retainedRates);
Check(echoedRates.Span.SequenceEqual(retainedRates.Span),
    "ReadOnlyMemory<double> contents did not round-trip");
Check(System.Runtime.InteropServices.MemoryMarshal.TryGetArray(echoedRates, out var echoedSegment)
      && ReferenceEquals(echoedSegment.Array, retainedRatesArray)
      && echoedSegment.Offset == 1 && echoedSegment.Count == 2,
    "ReadOnlyMemory<double> backing array/view identity was not preserved");
var mutableValuesArray = new[] { 1, 2, 3, 4 };
Memory<int> mutableValues = mutableValuesArray.AsMemory(1, 2);
var filledValues = InvoiceFacade.FillMemory(mutableValues, 17);
Check(mutableValuesArray.SequenceEqual(new[] { 1, 17, 17, 4 }),
    "Memory<int> mutation did not reach the caller-owned backing array");
Check(filledValues.Span.SequenceEqual(new[] { 17, 17 }),
    "returned Memory<int> view is incorrect");
IList<int> mutableList = new List<int> { 1, 2 };
var updatedList = InvoiceFacade.UpdateList(mutableList);
Check(ReferenceEquals(updatedList, mutableList) && mutableList.SequenceEqual(new[] { 41, 2, 42 }),
    "IList<int> projection did not mutate and preserve the caller implementation");
IDictionary<int, double> mutableDictionary = new Dictionary<int, double> { [7] = 1.25 };
var updatedDictionary = InvoiceFacade.UpdateDictionary(mutableDictionary);
Check(ReferenceEquals(updatedDictionary, mutableDictionary) && mutableDictionary[8] == 2.5,
    "IDictionary<int,double> projection did not mutate and preserve the caller implementation");
IEnumerable<int> producedSequence = InvoiceFacade.ProduceSequence();
Check(producedSequence.SequenceEqual(new[] { 2, 3, 5, 7 }),
    "Rust-produced IEnumerable<int> has the wrong values");
Check(InvoiceFacade.SumSequence(Enumerable.Range(1, 4)) == 10,
    "arbitrary C# IEnumerable<int> implementation was not consumed");
Check(InvoiceFacade.HasInvoice(withDate) && !InvoiceFacade.HasInvoice(null),
    "nullable managed reference presence was not projected correctly");
Check(ReferenceEquals(InvoiceFacade.EchoOptionalInvoice(withDate), withDate)
      && InvoiceFacade.EchoOptionalInvoice(null) is null
      && InvoiceFacade.AbsentInvoice() is null,
    "nullable managed reference did not round-trip as the underlying CLR type/null");
ReadOnlySpan<int> scopedInput = stackalloc[] { 3, 4, 5 };
Check(InvoiceFacade.SumSpan(scopedInput) == 12,
    "ReadOnlySpan<int> did not project as a scoped Rust slice");
Span<double> scopedOutput = stackalloc[] { 1.5, 2.5 };
InvoiceFacade.ScaleSpan(scopedOutput, 2.0);
Check(scopedOutput[0] == 3.0 && scopedOutput[1] == 5.0,
    "Span<double> mutation did not write through stack-allocated caller memory");

var recordType = typeof(RiskScenario);
var recordCtor = recordType.GetConstructors().Single();
Check(recordCtor.GetParameters().Length == 6, "record-shaped DTO has no primary constructor");
Check(recordType.GetConstructor(Type.EmptyTypes) is null,
    "record-shaped DTO unexpectedly has a parameterless constructor");
Check(recordType.GetProperties().All(property => property.CanRead && !property.CanWrite),
    "record-shaped DTO properties are not getter-only");
var scenario = InvoiceFacade.CreateScenario();
Check(scenario.ScenarioId == Guid.Parse("7eb9b72f-4f65-4ce6-9fbb-ea40680f75a8"),
    "record Guid is wrong");
Check(scenario.Name == "rate-up", "record name is wrong");
Check(scenario.AsOf == DateTimeOffset.Parse("2026-07-15T12:30:00-04:00"),
    "record DateTimeOffset is wrong");
Check(scenario.CalculatedAt == DateTime.Parse("2026-07-15T16:31:00Z"),
    "record DateTime is wrong");
Check(scenario.ShockPercent == 1.25, "record shock is wrong");
Check(scenario.HorizonDays == 30, "record horizon is wrong");
var scenarioCopy = new RiskScenario(
    scenario.ScenarioId,
    scenario.Name,
    scenario.AsOf,
    scenario.CalculatedAt,
    scenario.ShockPercent,
    scenario.HorizonDays);
var differentScenario = new RiskScenario(
    scenario.ScenarioId,
    scenario.Name,
    scenario.AsOf,
    scenario.CalculatedAt,
    scenario.ShockPercent + 1.0,
    scenario.HorizonDays);
Check(scenario.Equals(scenarioCopy) && scenario.Equals((object)scenarioCopy),
    "record field-wise equality failed");
Check(!scenario.Equals(differentScenario) && !scenario.Equals("not a scenario"),
    "record equality did not reject a different field value/runtime type");
Check(scenario == scenarioCopy && scenario != differentScenario,
    "record equality operators are not value-based");
RiskScenario? absentScenario = null;
Check(scenario != absentScenario && !(scenario == absentScenario) && absentScenario == null,
    "record equality operators are not null-safe");
Check(scenario.GetHashCode() == scenarioCopy.GetHashCode(),
    "equal records produced different hash codes");
Check(typeof(IEquatable<RiskScenario>).IsAssignableFrom(recordType),
    "record does not implement IEquatable<RiskScenario>");
var renderedScenario = scenario.ToString();
Check(renderedScenario.StartsWith("RiskScenario { ")
      && renderedScenario.Contains("ScenarioId = 7eb9b72f-4f65-4ce6-9fbb-ea40680f75a8")
      && renderedScenario.Contains("Name = rate-up")
      && renderedScenario.Contains("ShockPercent = 1.25")
      && renderedScenario.EndsWith(" }"),
    $"record ToString is not diagnostic and field-shaped: {renderedScenario}");
var deconstruct = recordType.GetMethod("Deconstruct", BindingFlags.Public | BindingFlags.Instance);
Check(deconstruct is not null
      && deconstruct.GetParameters().Length == 6
      && deconstruct.GetParameters().All(parameter => parameter.IsOut),
    "record Deconstruct metadata is missing or does not expose out parameters");
var (scenarioId, scenarioName, asOf, calculatedAt, shockPercent, horizonDays) = scenario;
Check(scenarioId == scenario.ScenarioId
      && scenarioName == scenario.Name
      && asOf == scenario.AsOf
      && calculatedAt == scenario.CalculatedAt
      && shockPercent == scenario.ShockPercent
      && horizonDays == scenario.HorizonDays,
    "record positional deconstruction returned incorrect values");
var nanScenario = new RiskScenario(
    scenario.ScenarioId, null!, scenario.AsOf, scenario.CalculatedAt, double.NaN, scenario.HorizonDays);
var nanScenarioCopy = new RiskScenario(
    scenario.ScenarioId, null!, scenario.AsOf, scenario.CalculatedAt, double.NaN, scenario.HorizonDays);
Check(nanScenario == nanScenarioCopy && nanScenario.GetHashCode() == nanScenarioCopy.GetHashCode(),
    "record equality/hash did not follow CLR null and floating-point comparer semantics");

Check(typeof(IDisposable).IsAssignableFrom(typeof(NativeResource)),
    "declarative lifecycle class does not implement IDisposable");
var nativeResource = NativeResource.Create();
using (nativeResource)
{
    Check(!nativeResource.IsDisposed(), "new Rust-owned resource is already disposed");
    nativeResource.Dispose();
    Check(nativeResource.IsDisposed(), "Dispose did not invalidate the Rust-owned resource");
    nativeResource.Dispose();
}
Check(NativeResource.DisposedCount() == 1,
    "Rust-owned resource was not dropped exactly once across using/repeated Dispose");
Check(typeof(IAsyncDisposable).IsAssignableFrom(typeof(NativeResource)),
    "declarative lifecycle class does not implement IAsyncDisposable");
await using (var asyncNativeResource = NativeResource.Create())
{
    Check(!asyncNativeResource.IsDisposed(), "new async Rust-owned resource is already disposed");
}
Check(NativeResource.DisposedCount() == 2,
    "await using did not await ValueTask-backed Rust async cleanup exactly once");

using var cancellation = new CancellationTokenSource();
Check(!InvoiceFacade.ObserveCancellation(cancellation.Token),
    "fresh token unexpectedly reports cancellation");
cancellation.Cancel();
Check(InvoiceFacade.ObserveCancellation(cancellation.Token),
    "canceled token was not observed in Rust");
Check(InvoiceFacade.RegisterCanceledCallback(cancellation.Token) == 1,
    "owned Rust cancellation callback did not run exactly once");
try
{
    InvoiceFacade.ThrowIfCanceled(cancellation.Token);
    throw new InvalidOperationException("ThrowIfCanceled did not throw");
}
catch (OperationCanceledException)
{
}
var reported = 0;
var progress = new ImmediateProgress<int>(value => reported = value);
InvoiceFacade.ReportProgress(progress, 73);
Check(reported == 73, "Rust did not report through IProgress<int>");

Console.WriteLine("typed DTO assertions passed");

sealed class ImmediateProgress<T>(Action<T> report) : IProgress<T>
{
    public void Report(T value) => report(value);
}
