// The reusable C#→Rust generic container, end-to-end, with ZERO hand-written interop:
//   * the Rust side is one line — `mycorrhiza::export_rust_containers!()` (see ../rustlib/src/lib.rs);
//   * the C# side is the shipped `RustDotnet.RustVec<T>` / `RustBoxVec<T>` (auto-included by
//     RustDotnet.targets because <UseRustDotnetContainers>true</UseRustDotnetContainers>).
// This file only USES them.

using System;
using RustDotnet;

public struct Point
{
    public int X;
    public int Y;
}

public sealed class Thing
{
    public string Name;
    public int Id;
}

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        // ---- RustVec<T : unmanaged> : raw-byte storage, near-zero-cost ----
        using (var vi = RustVec<int>.New())
        {
            vi.Push(10);
            vi.Push(20);
            vi.Push(30);
            Check("int Count", vi.Count, 3, ref pass, ref total);
            Check("int[2]", vi.Get(2), 30, ref pass, ref total);
            vi.Set(0, 99);
            Check("int[0] after Set", vi.Get(0), 99, ref pass, ref total);
        }
        using (var vp = RustVec<Point>.New())
        {
            vp.Push(new Point { X = 1, Y = 2 });
            vp.Push(new Point { X = 3, Y = 4 });
            Check("Point Count", vp.Count, 2, ref pass, ref total);
            Point p = vp.Get(1);
            Check("Point[1].X", p.X, 3, ref pass, ref total);
            Check("Point[1].Y", p.Y, 4, ref pass, ref total);
        }

        // ---- RustBoxVec<T> : GCHandle-boxed, works for ANY managed T, reference identity ----
        using (var vstr = RustBoxVec<string>.New())
        {
            string s0 = new string('a', 3);
            vstr.Push(s0);
            vstr.Push("beta");
            Check("string Count", vstr.Count, 2, ref pass, ref total);
            Check("string[0] value", vstr.Get(0), "aaa", ref pass, ref total);
            Check("string[0] identity", ReferenceEquals(vstr.Get(0), s0), true, ref pass, ref total);
            Check("string[1]", vstr.Get(1), "beta", ref pass, ref total);
        }
        using (var vt = RustBoxVec<Thing>.New())
        {
            var t0 = new Thing { Name = "zero", Id = 0 };
            vt.Push(t0);
            vt.Push(new Thing { Name = "one", Id = 1 });
            Check("Thing Count", vt.Count, 2, ref pass, ref total);
            Check("Thing[1].Name", vt.Get(1).Name, "one", ref pass, ref total);
            Check("Thing[0] identity", ReferenceEquals(vt.Get(0), t0), true, ref pass, ref total);
        }

        Console.WriteLine($"cd_containers: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok) pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
