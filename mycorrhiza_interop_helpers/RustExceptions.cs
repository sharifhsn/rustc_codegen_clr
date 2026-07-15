using System;
using System.IO;

namespace Mycorrhiza.Interop.Helpers;

/// <summary>Common diagnostics carried by exceptions projected from Rust errors.</summary>
public interface IRustExceptionDetails
{
    bool HasNativeStatus { get; }
    int NativeStatus { get; }
}

internal static class RustExceptionStatus
{
    internal const int Missing = int.MinValue;
}

public sealed class RustException(string message, int nativeStatus)
    : Exception(message), IRustExceptionDetails
{
    public bool HasNativeStatus => nativeStatus != RustExceptionStatus.Missing;
    public int NativeStatus => nativeStatus;
}

public sealed class RustArgumentException(string message, int nativeStatus)
    : ArgumentException(message), IRustExceptionDetails
{
    public bool HasNativeStatus => nativeStatus != RustExceptionStatus.Missing;
    public int NativeStatus => nativeStatus;
}

public sealed class RustInvalidOperationException(string message, int nativeStatus)
    : InvalidOperationException(message), IRustExceptionDetails
{
    public bool HasNativeStatus => nativeStatus != RustExceptionStatus.Missing;
    public int NativeStatus => nativeStatus;
}

public sealed class RustIOException(string message, int nativeStatus)
    : IOException(message), IRustExceptionDetails
{
    public bool HasNativeStatus => nativeStatus != RustExceptionStatus.Missing;
    public int NativeStatus => nativeStatus;
}

public sealed class RustTimeoutException(string message, int nativeStatus)
    : TimeoutException(message), IRustExceptionDetails
{
    public bool HasNativeStatus => nativeStatus != RustExceptionStatus.Missing;
    public int NativeStatus => nativeStatus;
}

public sealed class RustNotSupportedException(string message, int nativeStatus)
    : NotSupportedException(message), IRustExceptionDetails
{
    public bool HasNativeStatus => nativeStatus != RustExceptionStatus.Missing;
    public int NativeStatus => nativeStatus;
}
