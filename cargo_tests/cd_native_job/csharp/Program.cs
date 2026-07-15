using System.Collections.Concurrent;
using System.Diagnostics;

static void Check(bool condition, string message)
{
    if (!condition) throw new InvalidOperationException(message);
}

static void WaitUntil(Func<bool> condition, string description)
{
    var timeout = Stopwatch.StartNew();
    while (!condition())
    {
        Check(timeout.Elapsed < TimeSpan.FromSeconds(5), $"timed out waiting for {description}");
        Thread.Sleep(1);
    }
}

var successProgress = new ConcurrentQueue<int>();
Console.WriteLine("native job: success scenario");
using (var succeeded = ManagedNativeJob.Start(
           new ImmediateProgress<int>(successProgress.Enqueue),
           3,
           0,
           false))
{
    WaitUntil(() =>
    {
        succeeded.PumpProgress();
        return succeeded.Status() == ManagedJobStatus.Succeeded;
    }, "native job success");
    succeeded.PumpProgress();
    WaitUntil(() => successProgress.Contains(3), "terminal progress delivery");
    Check(successProgress.Contains(1) && successProgress.Contains(3),
        "IProgress<int> did not receive retained native callback values");
    Check(succeeded.TakeResult() == 3 && succeeded.TakeResult() == int.MinValue,
        "native job result was not available exactly once");
    Check(succeeded.IsRegistered(), "completed job lost its registration before explicit stop");
    Check(succeeded.TryStop(), "completed native job did not stop quiescently");
    Check(!succeeded.IsRegistered() && succeeded.Status() == ManagedJobStatus.Succeeded,
        "stopping a completed job discarded its terminal result state");
}

Console.WriteLine("native job: failure scenario");
using (var failed = ManagedNativeJob.Start(
           new ImmediateProgress<int>(_ => { }),
           0,
           0,
           false))
{
    Check(failed.Fail(2), "managed native adapter did not set the operation error");
    WaitUntil(() => failed.Status() == ManagedJobStatus.Failed, "native job failure");
    Check(failed.TakeError() == 2 && failed.TakeError() == int.MinValue,
        "native job error was not available exactly once");
    Check(failed.TryStop(), "failed native job did not stop quiescently");
    Check(failed.Status() == ManagedJobStatus.Failed,
        "stopping a failed job discarded its terminal error state");
}

Console.WriteLine("native job: managed cancellation scenario");
using var cancellation = new CancellationTokenSource();
var cancelProgress = new ConcurrentQueue<int>();
using (var canceled = ManagedNativeJob.Start(
           new ImmediateProgress<int>(cancelProgress.Enqueue),
           0,
           0,
           true))
{
    using var cancellationRegistration =
        cancellation.Token.Register(canceled.RequestCancellation);
    WaitUntil(() => ManagedNativeJob.LiveWorkers() > 0, "native worker before cancellation");
    cancellation.Cancel();
    WaitUntil(canceled.IsCancellationRequested, "managed CancellationToken forwarding");

    Check(!canceled.TryStop(), "first retryable native stop unexpectedly succeeded");
    Check(canceled.IsRegistered() && canceled.LastStopError() == 1,
        "failed stop did not preserve the live registration and native error");
    Check(canceled.TryStop(), "retryable native stop did not succeed on retry");
    Check(!canceled.IsRegistered() && canceled.Status() == ManagedJobStatus.Stopped,
        "successful retry did not leave a stopped, unregistered job");
}

Console.WriteLine("native job: explicit cancellation scenario");
using (var explicitlyCanceled = ManagedNativeJob.Start(
           new ImmediateProgress<int>(_ => { }),
           0,
           0,
           false))
{
    explicitlyCanceled.RequestCancellation();
    Check(explicitlyCanceled.IsCancellationRequested(),
        "explicit cooperative cancellation was not observable");
    Check(explicitlyCanceled.TryStop(), "explicitly canceled job did not stop");
}

Check(typeof(IDisposable).IsAssignableFrom(typeof(ManagedNativeJob)),
    "managed native job does not implement IDisposable");
var publicConstructors = typeof(ManagedNativeJob).GetConstructors(
    System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Instance);
var internalConstructors = typeof(ManagedNativeJob).GetConstructors(
    System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance);
Check(publicConstructors.Length == 0,
    "managed native job exposes a forgeable public state-ID constructor");
Check(internalConstructors.Length == 1 && internalConstructors[0].IsAssembly,
    "managed native job factory constructor is not assembly-internal");
Check(ManagedNativeJob.LiveWorkers() == 0,
    "native worker survived managed job disposal/quiescence");
Console.WriteLine("managed native job assertions passed");

sealed class ImmediateProgress<T>(Action<T> report) : IProgress<T>
{
    public void Report(T value) => report(value);
}
