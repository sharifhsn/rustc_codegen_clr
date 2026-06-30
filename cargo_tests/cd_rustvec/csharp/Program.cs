// WF-9 Stage 2 — C# instantiates a *Rust generic* across the seam.
//
// `RustVec<T>` is a NORMAL C# generic (a thin handle-holder, no explicit layout — legal under CLI
// §9.5) backed by ONE Rust monomorphization (`cd_rustvec`'s size-erased `RustVec`, exposed as
// `MainModule.rcl_vec_*`). The wrapper marshals each `T` to/from raw bytes via `&value` + `sizeof(T)`
// and Rust stores them by element size — so the SAME Rust `.dll` serves `RustVec<int>`,
// `RustVec<Point>` (a C# struct Rust never saw), `RustVec<long>`, … for any `T : unmanaged`.
//
// This is the doc's "size-parameterized sharing" bridge for wall #1: near-zero-cost, layout-
// preserving functional generic interop, without ever emitting a Rust generic *.NET type definition*
// (which §9.5 forbids).

using System;

/// A growable list of unmanaged `T`, backed by a single size-erased Rust vector.
public unsafe struct RustVec<T> : IDisposable where T : unmanaged
{
    private nuint _handle;

    public static RustVec<T> New() =>
        new RustVec<T> { _handle = MainModule.rcl_vec_new((nuint)sizeof(T)) };

    public int Count => (int)MainModule.rcl_vec_len(_handle);

    public void Push(T value) => MainModule.rcl_vec_push(_handle, (byte*)&value);

    public T Get(int idx)
    {
        T v = default;
        if (!MainModule.rcl_vec_get(_handle, (nuint)idx, (byte*)&v))
            throw new IndexOutOfRangeException();
        return v;
    }

    public void Set(int idx, T value)
    {
        if (!MainModule.rcl_vec_set(_handle, (nuint)idx, (byte*)&value))
            throw new IndexOutOfRangeException();
    }

    /// Sum every element interpreted by Rust as a little-endian i32 (only meaningful for int).
    public long RustSumI32() => MainModule.rcl_vec_sum_i32(_handle);

    public void Dispose()
    {
        if (_handle != 0)
        {
            MainModule.rcl_vec_free(_handle);
            _handle = 0;
        }
    }
}

public struct Point
{
    public int X;
    public int Y;
}

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        // ---- RustVec<int>: a 4-byte primitive ----
        using (var vi = RustVec<int>.New())
        {
            vi.Push(10);
            vi.Push(20);
            vi.Push(30);
            Check("int Count", vi.Count, 3, ref pass, ref total);
            Check("int[0]", vi.Get(0), 10, ref pass, ref total);
            Check("int[2]", vi.Get(2), 30, ref pass, ref total);
            vi.Set(1, 99);
            Check("int[1] after Set", vi.Get(1), 99, ref pass, ref total);
            // Rust does real work over the stored bytes: 10 + 99 + 30 = 139.
            Check("Rust sum_i32", vi.RustSumI32(), 139L, ref pass, ref total);
        }

        // ---- RustVec<Point>: an 8-byte C#-defined struct Rust never saw ----
        using (var vp = RustVec<Point>.New())
        {
            vp.Push(new Point { X = 1, Y = 2 });
            vp.Push(new Point { X = 3, Y = 4 });
            Check("Point Count", vp.Count, 2, ref pass, ref total);
            Point p = vp.Get(1);
            Check("Point[1].X", p.X, 3, ref pass, ref total);
            Check("Point[1].Y", p.Y, 4, ref pass, ref total);
        }

        // ---- RustVec<long>: an 8-byte primitive, value beyond 32 bits ----
        using (var vl = RustVec<long>.New())
        {
            long big = 1L << 40;
            vl.Push(big);
            vl.Push(-7L);
            Check("long Count", vl.Count, 2, ref pass, ref total);
            Check("long[0]", vl.Get(0), big, ref pass, ref total);
            Check("long[1]", vl.Get(1), -7L, ref pass, ref total);
        }

        Console.WriteLine($"cd_rustvec: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok)
            pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
