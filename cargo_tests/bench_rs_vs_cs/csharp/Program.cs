// Hand-written C# side of the head-to-head vs Rust-via-rustc_codegen_clr, both on .NET 8.
// Logic is byte-identical to ../rust/src/main.rs so the only variable is the toolchain.
//   numeric     : tight integer loop, zero allocation.
//   alloc_churn : new ulong[k] (GC heap) per iteration, fill + sum, left to the GC.
using System;
using System.Diagnostics;

static class Bench
{
    static ulong Numeric(ulong n)
    {
        ulong acc = 0;
        for (ulong i = 0; i < n; i++)
            acc = unchecked(acc + ((i * i) ^ (i >> 3)));
        return acc;
    }

    static ulong AllocChurn(int iters, int k)
    {
        ulong acc = 0;
        for (int it = 0; it < iters; it++)
        {
            var v = new ulong[k];               // GC-heap allocation
            for (int j = 0; j < k; j++) v[j] = (ulong)j;
            for (int j = 0; j < k; j++) acc = unchecked(acc + v[j]);
            // v becomes garbage -> collected later (GC pressure)
        }
        return acc;
    }

    static void Run(string name, int runs, Func<ulong> f)
    {
        long best = long.MaxValue; ulong sink = 0;
        for (int r = 0; r < runs; r++)
        {
            var sw = Stopwatch.StartNew();
            sink = f();
            sw.Stop();
            long us = sw.ElapsedTicks * 1_000_000L / Stopwatch.Frequency;
            if (us < best) best = us;
        }
        Console.WriteLine($"{name}: best={best}us (sink={sink})");
    }

    static void Main()
    {
        _ = Numeric(5_000_000);
        _ = AllocChurn(10_000, 256);
        Run("numeric", 3, () => Numeric(300_000_000));
        Run("alloc_churn", 3, () => AllocChurn(300_000, 512));
    }
}
