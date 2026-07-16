using System;
using System.Runtime.InteropServices;

public static class NativeProbe
{
    [DllImport("unity_native_sample", EntryPoint = "rust_native_multiply")]
    private static extern int RustNativeMultiply(int left, int right);

    public static int Main()
    {
        int value = RustNativeMultiply(6, 7);
        Console.WriteLine($"UNITY_MONO_NATIVE_RUST={value}");
        return value == 42 ? 0 : 1;
    }
}
