using System;
using System.ComponentModel;
using System.Threading;

namespace Mycorrhiza.Interop.Helpers;

/// <summary>
/// Host-neutral contract used by managed Rust to schedule work on a UI thread.
/// </summary>
/// <remarks>
/// WinUI and MAUI can adapt their native dispatchers with <see cref="DelegateUiDispatcher"/>.
/// Unity code should capture its synchronization context on the main thread with
/// <see cref="SynchronizationContextUiDispatcher.CaptureCurrent"/>.
/// </remarks>
public interface IRustUiDispatcher
{
    /// <summary>Whether the caller already has access to the dispatcher's UI thread.</summary>
    bool CheckAccess { get; }

    /// <summary>Attempts to enqueue <paramref name="callback"/> for UI-thread execution.</summary>
    /// <returns><see langword="false"/> when the host is shutting down or rejects new work.</returns>
    bool TryDispatch(Action callback);
}

/// <summary>
/// Adapts host-specific check/enqueue delegates without adding a WinUI, MAUI, or Unity dependency
/// to the shared helper assembly.
/// </summary>
public sealed class DelegateUiDispatcher : IRustUiDispatcher
{
    private readonly Func<bool> _checkAccess;
    private readonly Func<Action, bool> _tryDispatch;

    public DelegateUiDispatcher(Func<bool> checkAccess, Func<Action, bool> tryDispatch)
    {
        _checkAccess = checkAccess ?? throw new ArgumentNullException(nameof(checkAccess));
        _tryDispatch = tryDispatch ?? throw new ArgumentNullException(nameof(tryDispatch));
    }

    public bool CheckAccess => _checkAccess();

    public bool TryDispatch(Action callback)
    {
        if (callback is null)
        {
            throw new ArgumentNullException(nameof(callback));
        }
        return _tryDispatch(callback);
    }
}

/// <summary>
/// Dispatches through a synchronization context captured on its owning thread.
/// </summary>
public sealed class SynchronizationContextUiDispatcher : IRustUiDispatcher
{
    private readonly SynchronizationContext _context;
    private readonly int _threadId;

    public SynchronizationContextUiDispatcher(
        SynchronizationContext context,
        int owningManagedThreadId)
    {
        _context = context ?? throw new ArgumentNullException(nameof(context));
        _threadId = owningManagedThreadId;
    }

    /// <summary>
    /// Captures the current context. Call this on the host's UI thread after it installs its real
    /// synchronization context (for example from Unity <c>Awake</c> or <c>Start</c>).
    /// </summary>
    public static SynchronizationContextUiDispatcher CaptureCurrent()
    {
        SynchronizationContext context = SynchronizationContext.Current
            ?? throw new InvalidOperationException(
                "No SynchronizationContext is installed on the current thread. " +
                "Capture the dispatcher from the host UI thread after host initialization.");
        return new SynchronizationContextUiDispatcher(
            context,
            Environment.CurrentManagedThreadId);
    }

    public bool CheckAccess => Environment.CurrentManagedThreadId == _threadId;

    public bool TryDispatch(Action callback)
    {
        if (callback is null)
        {
            throw new ArgumentNullException(nameof(callback));
        }
        if (CheckAccess)
        {
            callback();
            return true;
        }

        try
        {
            _context.Post(static state => ((Action)state!).Invoke(), callback);
            return true;
        }
        catch (InvalidAsynchronousStateException)
        {
            return false;
        }
        catch (ObjectDisposedException)
        {
            return false;
        }
    }
}

/// <summary>
/// Owns one opaque Rust callback until it executes, is rejected, or becomes unreachable.
/// </summary>
public sealed class RustDispatchWork
{
    private readonly long _id;
    private Action<long, bool>? _complete;

    public RustDispatchWork(long id, Action<long, bool> complete)
    {
        if (id == 0)
        {
            throw new ArgumentOutOfRangeException(nameof(id));
        }

        _id = id;
        _complete = complete ?? throw new ArgumentNullException(nameof(complete));
    }

    public void Run() => Complete(execute: true);

    public void Cancel() => Complete(execute: false);

    private void Complete(bool execute)
    {
        Action<long, bool>? complete = Interlocked.Exchange(ref _complete, null);
        if (complete is null)
        {
            return;
        }

        GC.SuppressFinalize(this);
        complete(_id, execute);
    }

    ~RustDispatchWork()
    {
        try
        {
            Complete(execute: false);
        }
        catch
        {
            // Finalizers must never terminate the process. Generated Rust completions are
            // non-throwing; this protects against malformed external implementations.
        }
    }
}

/// <summary>Safe entry point used by the Rust wrapper.</summary>
public static class RustUiDispatch
{
    /// <summary>
    /// Runs inline when access is already available or attempts to enqueue the work. Any immediate
    /// rejection or host exception cancels the lease before returning <see langword="false"/>.
    /// </summary>
    public static bool TryDispatch(IRustUiDispatcher dispatcher, RustDispatchWork work)
    {
        if (dispatcher is null)
        {
            throw new ArgumentNullException(nameof(dispatcher));
        }
        if (work is null)
        {
            throw new ArgumentNullException(nameof(work));
        }

        try
        {
            if (dispatcher.CheckAccess)
            {
                work.Run();
                return true;
            }

            if (dispatcher.TryDispatch(work.Run))
            {
                return true;
            }
        }
        catch
        {
            // A host adapter is not allowed to strand native ownership because its queue is
            // shutting down or because user dispatch code threw.
        }

        work.Cancel();
        return false;
    }
}
