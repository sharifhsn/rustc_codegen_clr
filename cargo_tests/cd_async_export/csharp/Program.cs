using System.Runtime.CompilerServices;
using System.Collections.Concurrent;
using Mycorrhiza.Interop.Helpers;

Task<int> scoreTask = MainModule.ComputeScoreAsync(21);
int score = await scoreTask;
await MainModule.WarmUpAsync();
var reported = 0;
var progress = new ImmediateProgress<int>(value => reported = value);
using var cancellation = new CancellationTokenSource();
bool canceled = MainModule.ReportAndObserve(cancellation.Token, progress);
bool asyncCanceled = await MainModule.ReportAndObserveAsync(cancellation.Token, progress);
int uncanceledScore = await MainModule.CancelableScoreAsync(cancellation.Token, 9);
await MainModule.CancelableUnitAsync(cancellation.Token);
using var canceledSource = new CancellationTokenSource();
canceledSource.Cancel();
Task<int> canceledScoreTask = MainModule.CancelableScoreAsync(canceledSource.Token, 9);
Task canceledUnitTask = MainModule.CancelableUnitAsync(canceledSource.Token);
if (!canceledScoreTask.IsCanceled || canceledScoreTask.IsFaulted
    || !canceledUnitTask.IsCanceled || canceledUnitTask.IsFaulted)
    throw new InvalidOperationException("Rust cancellation did not produce genuine canceled Tasks");
try
{
    await canceledScoreTask;
    throw new InvalidOperationException("awaiting canceled Task<int> unexpectedly succeeded");
}
catch (OperationCanceledException)
{
}
try
{
    await canceledUnitTask;
    throw new InvalidOperationException("awaiting canceled Task unexpectedly succeeded");
}
catch (OperationCanceledException)
{
}
var retainedValues = new[] { 4, 5, 6, 7 };
int retainedSum = await MainModule.SumReadOnlyMemoryAsync(
    new ReadOnlyMemory<int>(retainedValues, 1, 2));
var mutableValues = new[] { 10, 20, 30, 40 };
int filledLength = await MainModule.FillMemoryAsync(new Memory<int>(mutableValues, 1, 2), 99);
IList<int> asyncList = new List<int> { 3, 5 };
IDictionary<int, int> asyncDictionary = new Dictionary<int, int>();
int collectionCount = await MainModule.UpdateCollectionsAsync(asyncList, asyncDictionary);
int sequenceSum = await MainModule.SumSequenceAsync(Enumerable.Range(3, 4));
bool hasText = await MainModule.HasTextAsync("rooted");
bool hasNullText = await MainModule.HasTextAsync(null);

var streamedScores = new List<int>();
await foreach (int value in MainModule.ScoresAsync(4, 2))
    streamedScores.Add(value);
await WaitForStreamProducersAsync();
Console.WriteLine("async stream normal completion passed");

// The producer starts immediately, but a capacity-one channel prevents it from running ahead of
// an idle consumer. Breaking await-foreach invokes DisposeAsync and stops the Rust producer.
IAsyncEnumerable<int> earlyBreakStream = MainModule.ScoresAsync(100, 1);
await Task.Delay(50);
if (MainModule.StreamEmittedCount() > 1)
    throw new InvalidOperationException("Rust async stream did not preserve one-item backpressure");
await foreach (int value in earlyBreakStream)
{
    if (value != 10)
        throw new InvalidOperationException($"Unexpected early-break value {value}");
    break;
}
await WaitForStreamProducersAsync();
bool secondEnumerationRejected = false;
try
{
    await foreach (int _ in earlyBreakStream)
    {
    }
}
catch (InvalidOperationException error) when (error.Message.Contains("single-consumer", StringComparison.Ordinal))
{
    secondEnumerationRejected = true;
}
Console.WriteLine("async stream early disposal passed");

await CreateAndAbandonStreamAsync();
GC.Collect();
GC.WaitForPendingFinalizers();
await WaitForStreamProducersAsync();
Console.WriteLine("async stream abandonment cleanup passed");

using var streamCancellation = new CancellationTokenSource();
streamCancellation.CancelAfter(20);
bool streamWasCanceled = false;
try
{
    await foreach (int _ in MainModule.ScoresAsync(1000, 2)
        .WithCancellation(streamCancellation.Token))
    {
    }
}
catch (OperationCanceledException)
{
    streamWasCanceled = true;
}
await WaitForStreamProducersAsync();
Console.WriteLine("async stream cancellation passed");

bool streamFaultObserved = false;
try
{
    await foreach (int _ in MainModule.FaultingScoresAsync())
    {
    }
}
catch (Exception error) when (error.Message.Contains("stream boom from Rust", StringComparison.Ordinal))
{
    streamFaultObserved = true;
}
await WaitForStreamProducersAsync();
Console.WriteLine("async stream fault propagation passed");

int expectedUiThread = Environment.CurrentManagedThreadId;
var pumpContext = new PumpSynchronizationContext();
SynchronizationContext.SetSynchronizationContext(pumpContext);
try
{
    IRustUiDispatcher uiDispatcher = SynchronizationContextUiDispatcher.CaptureCurrent();
    if (!MainModule.StartUiDispatch(uiDispatcher, 314, false))
        throw new InvalidOperationException("Rust UI-dispatch worker did not start");
    PumpUntil(
        pumpContext,
        () => MainModule.UiDispatchMarker() == 314 && MainModule.ActiveUiDispatches() == 0,
        "accepted Rust UI dispatch did not complete");
    if (MainModule.UiDispatchThread() != expectedUiThread)
        throw new InvalidOperationException(
            $"UI callback ran on managed thread {MainModule.UiDispatchThread()}, expected {expectedUiThread}");
    Console.WriteLine("UI dispatch main-thread execution passed");

    if (!MainModule.StartUiDispatch(uiDispatcher, 2718, true))
        throw new InvalidOperationException("Rust panicking UI-dispatch worker did not start");
    PumpUntil(
        pumpContext,
        () => MainModule.UiDispatchMarker() == 2718 && MainModule.ActiveUiDispatches() == 0,
        "panicking Rust UI dispatch did not release its lease");
    Console.WriteLine("UI dispatch panic containment passed");

    var rejectingDispatcher = new DelegateUiDispatcher(
        checkAccess: static () => false,
        tryDispatch: static _ => false);
    if (!MainModule.StartUiDispatch(rejectingDispatcher, 1, false))
        throw new InvalidOperationException("Rust rejected-dispatch worker did not start");
    SpinUntil(
        () => MainModule.ActiveUiDispatches() == 0,
        "rejected Rust UI dispatch retained its closure");
    if (MainModule.UiDispatchMarker() != 0)
        throw new InvalidOperationException("Rejected UI work unexpectedly executed");
    Console.WriteLine("UI dispatch rejection cleanup passed");

    var abandoningDispatcher = new DelegateUiDispatcher(
        checkAccess: static () => false,
        tryDispatch: static _ => true);
    if (!MainModule.StartUiDispatch(abandoningDispatcher, 2, false))
        throw new InvalidOperationException("Rust abandoned-dispatch worker did not start");
    SpinUntil(
        () => MainModule.ActiveUiDispatches() != 0,
        "abandoned Rust UI dispatch never became active");
    ForceGcUntil(
        () => MainModule.ActiveUiDispatches() == 0,
        "finalization did not release abandoned Rust UI work");
    if (MainModule.UiDispatchMarker() != 0)
        throw new InvalidOperationException("Abandoned UI work unexpectedly executed");
    Console.WriteLine("UI dispatch abandonment cleanup passed");
}
finally
{
    SynchronizationContext.SetSynchronizationContext(null);
}

Console.WriteLine($"async export score={score}, task={scoreTask.GetType().FullName}");
if (score != 42 || uncanceledScore != 18 || canceled || asyncCanceled || reported != 100 || retainedSum != 11
    || filledLength != 2 || !mutableValues.SequenceEqual(new[] { 10, 99, 99, 40 })
    || collectionCount != 3 || asyncList[^1] != 8 || asyncDictionary[8] != 3
    || sequenceSum != 18 || !hasText || hasNullText
    || !streamedScores.SequenceEqual(new[] { 10, 20, 30, 40 })
    || !secondEnumerationRejected || !streamWasCanceled || !streamFaultObserved
    || MainModule.ActiveStreamProducers() != 0)
    Environment.Exit(1);

static async Task CreateAndAbandonStreamAsync()
{
    IAsyncEnumerable<int> abandoned = MainModule.ScoresAsync(1000, 1);
    var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
    while (MainModule.ActiveStreamProducers() == 0 && DateTime.UtcNow < deadline)
        await Task.Delay(10);
    if (MainModule.ActiveStreamProducers() == 0)
        throw new TimeoutException("Rust async-stream producer did not start");
    GC.KeepAlive(abandoned);
}

static async Task WaitForStreamProducersAsync()
{
    var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
    while (MainModule.ActiveStreamProducers() != 0 && DateTime.UtcNow < deadline)
        await Task.Delay(10);
    if (MainModule.ActiveStreamProducers() != 0)
        throw new TimeoutException("Rust async-stream producer did not stop after managed disposal");
}

static void PumpUntil(
    PumpSynchronizationContext context,
    Func<bool> condition,
    string failure)
{
    var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
    while (!condition() && DateTime.UtcNow < deadline)
        context.PumpOne(TimeSpan.FromMilliseconds(20));
    if (!condition())
        throw new TimeoutException(failure);
}

static void SpinUntil(Func<bool> condition, string failure)
{
    var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
    while (!condition() && DateTime.UtcNow < deadline)
        Thread.Sleep(10);
    if (!condition())
        throw new TimeoutException(failure);
}

static void ForceGcUntil(Func<bool> condition, string failure)
{
    var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
    while (!condition() && DateTime.UtcNow < deadline)
    {
        GC.Collect();
        GC.WaitForPendingFinalizers();
        Thread.Sleep(10);
    }
    if (!condition())
        throw new TimeoutException(failure);
}

sealed class ImmediateProgress<T>(Action<T> report) : IProgress<T>
{
    public void Report(T value) => report(value);
}

sealed class PumpSynchronizationContext : SynchronizationContext
{
    private readonly ConcurrentQueue<(SendOrPostCallback Callback, object? State)> _queue = new();
    private readonly AutoResetEvent _ready = new(initialState: false);

    public override void Post(SendOrPostCallback callback, object? state)
    {
        ArgumentNullException.ThrowIfNull(callback);
        _queue.Enqueue((callback, state));
        _ready.Set();
    }

    public bool PumpOne(TimeSpan timeout)
    {
        if (!_queue.TryDequeue(out var work))
        {
            _ready.WaitOne(timeout);
            if (!_queue.TryDequeue(out work))
                return false;
        }

        work.Callback(work.State);
        return true;
    }
}
