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
using System.Runtime.InteropServices;

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

/// A growable list of ANY `T` (managed reference types, structs holding references, anything),
/// backed by the SAME size-erased Rust vector — but each element is a `GCHandle` rooting the managed
/// object, and the vector stores only the pointer-sized handle. This is the managed-`T` arm of the
/// two-mode bridge (docs/TRANSLATION_STATUS §7): works for any `T` at the cost of a box + GC root per
/// element (vs `RustVec<T : unmanaged>`'s near-zero-cost byte copy). Rust never sees the object — only
/// the opaque handle — so the same Rust monomorphization serves both modes. Reference identity is
/// preserved: `Get` returns the very object that was `Push`ed.
public unsafe struct RustBoxVec<T> : IDisposable
{
    private nuint _handle;

    public static RustBoxVec<T> New() =>
        new RustBoxVec<T> { _handle = MainModule.rcl_vec_new((nuint)IntPtr.Size) };

    public int Count => (int)MainModule.rcl_vec_len(_handle);

    public void Push(T value)
    {
        // A normal (strong) GCHandle roots the object so the GC keeps it alive while it sits in the
        // Rust-side vector (which holds only the handle's integer value, never a managed reference).
        GCHandle gh = GCHandle.Alloc(value);
        IntPtr p = GCHandle.ToIntPtr(gh);
        MainModule.rcl_vec_push(_handle, (byte*)&p);
    }

    public T Get(int idx)
    {
        IntPtr p = default;
        if (!MainModule.rcl_vec_get(_handle, (nuint)idx, (byte*)&p))
            throw new IndexOutOfRangeException();
        return (T)GCHandle.FromIntPtr(p).Target;
    }

    public void Set(int idx, T value)
    {
        IntPtr old = default;
        if (!MainModule.rcl_vec_get(_handle, (nuint)idx, (byte*)&old))
            throw new IndexOutOfRangeException();
        GCHandle.FromIntPtr(old).Free(); // release the replaced element's root
        GCHandle gh = GCHandle.Alloc(value);
        IntPtr p = GCHandle.ToIntPtr(gh);
        MainModule.rcl_vec_set(_handle, (nuint)idx, (byte*)&p);
    }

    public void Dispose()
    {
        if (_handle != 0)
        {
            int n = Count;
            for (int i = 0; i < n; i++)
            {
                IntPtr p = default;
                MainModule.rcl_vec_get(_handle, (nuint)i, (byte*)&p);
                GCHandle.FromIntPtr(p).Free(); // free every rooted element
            }
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

// A managed reference type, to exercise the GCHandle-boxed path (reference identity + a field).
public sealed class Thing
{
    public string Name;
    public int Id;
}

// A struct with INTERNAL PADDING (byte at 0, then 7 bytes padding, long at 8 → 16 bytes). The
// byte-erased core memcpys sizeof(T) bytes verbatim, so the padding round-trips with the fields.
public struct Padded
{
    public byte B;
    public long L;
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

        // ---- large count: force the backing Rust Vec to grow/realloc many times ----
        using (var vbig = RustVec<int>.New())
        {
            for (int i = 0; i < 1000; i++)
                vbig.Push(i * 3);
            Check("big Count", vbig.Count, 1000, ref pass, ref total);
            Check("big[0]", vbig.Get(0), 0, ref pass, ref total);
            Check("big[500]", vbig.Get(500), 1500, ref pass, ref total);
            Check("big[999]", vbig.Get(999), 2997, ref pass, ref total);
        }

        // ---- RustVec<byte>: a 1-byte element ----
        using (var vb = RustVec<byte>.New())
        {
            vb.Push(255);
            vb.Push(0);
            vb.Push(128);
            Check("byte Count", vb.Count, 3, ref pass, ref total);
            Check("byte[0]", vb.Get(0), (byte)255, ref pass, ref total);
            Check("byte[2]", vb.Get(2), (byte)128, ref pass, ref total);
        }

        // ---- RustVec<double>: 8-byte float, bit-exact round-trip ----
        using (var vd = RustVec<double>.New())
        {
            vd.Push(3.141592653589793);
            vd.Push(-1.5e300);
            Check("double Count", vd.Count, 2, ref pass, ref total);
            Check("double[0]", vd.Get(0), 3.141592653589793, ref pass, ref total);
            Check("double[1]", vd.Get(1), -1.5e300, ref pass, ref total);
        }

        // ---- RustVec<Padded>: a 16-byte struct WITH internal padding (memcpy preserves it) ----
        using (var vpad = RustVec<Padded>.New())
        {
            vpad.Push(new Padded { B = 7, L = 1L << 50 });
            Padded got = vpad.Get(0);
            Check("Padded.B", got.B, (byte)7, ref pass, ref total);
            Check("Padded.L", got.L, 1L << 50, ref pass, ref total);
        }

        // ---- Set sweep: overwrite every index, then read back ----
        using (var vs = RustVec<int>.New())
        {
            for (int i = 0; i < 10; i++)
                vs.Push(0);
            for (int i = 0; i < 10; i++)
                vs.Set(i, i * i);
            bool sweepOk = true;
            for (int i = 0; i < 10; i++)
                sweepOk &= vs.Get(i) == i * i;
            Check("Set sweep", sweepOk, true, ref pass, ref total);
        }

        // ====================== managed-T mode (RustBoxVec<T>, GCHandle-boxed) ======================

        // ---- RustBoxVec<string>: managed reference type — value + reference identity ----
        using (var vstr = RustBoxVec<string>.New())
        {
            string s0 = new string('a', 3); // a distinct (non-interned) instance
            vstr.Push(s0);
            vstr.Push("beta");
            Check("string Count", vstr.Count, 2, ref pass, ref total);
            Check("string[0] value", vstr.Get(0), "aaa", ref pass, ref total);
            Check("string[0] identity", ReferenceEquals(vstr.Get(0), s0), true, ref pass, ref total);
            Check("string[1] value", vstr.Get(1), "beta", ref pass, ref total);
        }

        // ---- RustBoxVec<Thing>: a managed class — field round-trip + reference identity + Set ----
        using (var vt = RustBoxVec<Thing>.New())
        {
            var t0 = new Thing { Name = "zero", Id = 0 };
            vt.Push(t0);
            vt.Push(new Thing { Name = "one", Id = 1 });
            Check("Thing Count", vt.Count, 2, ref pass, ref total);
            Check("Thing[1].Name", vt.Get(1).Name, "one", ref pass, ref total);
            Check("Thing[1].Id", vt.Get(1).Id, 1, ref pass, ref total);
            Check("Thing[0] identity", ReferenceEquals(vt.Get(0), t0), true, ref pass, ref total);
            var t2 = new Thing { Name = "two", Id = 2 };
            vt.Set(0, t2); // overwrite (frees t0's GCHandle)
            Check("Thing[0].Id after Set", vt.Get(0).Id, 2, ref pass, ref total);
            Check("Thing[0] identity after Set", ReferenceEquals(vt.Get(0), t2), true, ref pass, ref total);
        }

        // ---- RustBoxVec<int[]>: a managed ARRAY element — identity + contents ----
        using (var va = RustBoxVec<int[]>.New())
        {
            int[] arr = { 5, 6, 7 };
            va.Push(arr);
            Check("array Count", va.Count, 1, ref pass, ref total);
            Check("array identity", ReferenceEquals(va.Get(0), arr), true, ref pass, ref total);
            Check("array[0][2]", va.Get(0)[2], 7, ref pass, ref total);
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
