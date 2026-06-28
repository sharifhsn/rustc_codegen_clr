// 3-way performance harness — C# peer baseline (.NET 8), logic mirroring ../rust/src/main.rs.
// Idiomatic-fast C# (manual loops, arrays, Dictionary, StringBuilder) = the .NET ceiling the
// Rust-via-backend numbers are measured against. Prints:
//     RESULT <name> <best_ns> <gc_bytes> <gen0>
// gc_bytes = managed bytes allocated during one run (GC.GetTotalAllocatedBytes), gen0 = gen-0
// collections during it. These are the .NET-managed view of allocation (NOT directly comparable to
// the Rust counting-allocator columns, but the per-column trend is informative).
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;

static class Bench
{
    static ulong IntArith(ulong n)
    {
        ulong acc = 0;
        for (ulong i = 0; i < n; i++) acc = unchecked(acc + ((i * i) ^ (i >> 3)));
        return acc;
    }

    static ulong FloatArith(ulong n)
    {
        double acc = 0;
        for (ulong i = 0; i < n; i++)
        {
            double x = i * 1e-6;
            acc += Math.Sqrt(x * x + 1.0) - x;
        }
        return (ulong)BitConverter.DoubleToInt64Bits(acc);
    }

    // C# has no zero-cost iterator; the fast idiom is the manual loop, so iter_sum == iter_indexed
    // here (both = the .NET ceiling). The interesting delta is Rust-internal (iter_sum vs indexed).
    static ulong IterSum(ulong n) => IntArith(n);
    static ulong IterIndexed(ulong n) => IntArith(n);

    static ulong IterZip(ulong[] a, ulong[] b, int reps)
    {
        ulong acc = 0;
        for (int r = 0; r < reps; r++)
        {
            ulong s = 0;
            int len = Math.Min(a.Length, b.Length);
            for (int i = 0; i < len; i++) s = unchecked(s + a[i] * b[i]);
            acc = unchecked(acc + s);
        }
        return acc;
    }

    static ulong VecChurn(int iters, int k)
    {
        ulong acc = 0;
        for (int it = 0; it < iters; it++)
        {
            var v = new ulong[k];               // GC-heap allocation
            for (int j = 0; j < k; j++) v[j] = (ulong)j;
            for (int j = 0; j < k; j++) acc = unchecked(acc + v[j]);
        }
        return acc;
    }

    sealed class Boxed { public ulong V; }
    static ulong BoxChurn(int iters)
    {
        ulong acc = 0;
        for (int i = 0; i < iters; i++)
        {
            var b = new Boxed { V = (ulong)i };
            acc = unchecked(acc + b.V);
        }
        return acc;
    }

    static ulong HashMap(ulong n)
    {
        var m = new Dictionary<ulong, ulong>();
        for (ulong i = 0; i < n; i++) m[i] = unchecked(i * 3);
        ulong acc = 0;
        for (ulong i = 0; i < n; i++) acc = unchecked(acc + (m.TryGetValue(i, out var v) ? v : 0));
        return acc;
    }

    static ulong StringBuild(int n)
    {
        var s = new StringBuilder();
        for (int i = 0; i < n; i++) s.Append(i % 2 == 0 ? "ab" : "cde");
        return (ulong)s.Length;
    }

    static ulong SliceFill(int reps, int k)
    {
        var v = new byte[k];
        ulong acc = 0;
        for (int r = 0; r < reps; r++)
        {
            Array.Fill(v, (byte)(r & 0xff));
            acc = unchecked(acc + v[k - 1]);
        }
        return acc;
    }

    static ulong SortInts(int n)
    {
        var v = new ulong[n];
        for (int i = 0; i < n; i++) v[i] = unchecked((ulong)i * 2654435761UL) & 0xffff;
        Array.Sort(v);
        ulong acc = 0;
        for (int i = 0; i < n; i++) acc = unchecked(acc + v[i]);
        return acc;
    }

    static ulong Fib(ulong n) => n < 2 ? n : unchecked(Fib(n - 1) + Fib(n - 2));

    static void Run(string name, int warmup, int reps, Func<ulong> f)
    {
        for (int i = 0; i < warmup; i++) GC.KeepAlive(f());
        long b0 = GC.GetTotalAllocatedBytes(true);
        int g0 = GC.CollectionCount(0);
        ulong sink = f();
        long gcBytes = GC.GetTotalAllocatedBytes(true) - b0;
        int gen0 = GC.CollectionCount(0) - g0;
        long best = long.MaxValue;
        for (int r = 0; r < reps; r++)
        {
            var sw = Stopwatch.StartNew();
            ulong rr = f();
            sw.Stop();
            GC.KeepAlive(rr);
            long ns = sw.ElapsedTicks * 1_000_000_000L / Stopwatch.Frequency;
            if (ns < best) best = ns;
        }
        Console.Error.WriteLine($"# {name} sink={sink}");
        Console.WriteLine($"RESULT {name} {best} {gcBytes} {gen0}");
    }

    static void Main()
    {
        var zipA = new ulong[4096];
        var zipB = new ulong[4096];
        for (int i = 0; i < 4096; i++) { zipA[i] = (ulong)i; zipB[i] = (ulong)i ^ 0x55; }

        Run("int_arith", 2, 3, () => IntArith(200_000_000));
        Run("float_arith", 2, 3, () => FloatArith(100_000_000));
        Run("iter_sum", 2, 3, () => IterSum(200_000_000));
        Run("iter_indexed", 2, 3, () => IterIndexed(200_000_000));
        Run("iter_zip", 2, 3, () => IterZip(zipA, zipB, 50_000));
        Run("vec_churn", 2, 3, () => VecChurn(200_000, 512));
        Run("box_churn", 2, 3, () => BoxChurn(20_000_000));
        Run("hashmap", 2, 3, () => HashMap(2_000_000));
        Run("string_build", 2, 3, () => StringBuild(5_000_000));
        Run("slice_fill", 2, 3, () => SliceFill(2_000_000, 4096));
        Run("sort_ints", 2, 3, () => SortInts(2_000_000));
        Run("fib_rec", 2, 3, () => Fib(35));
    }
}
