// Round-2 reusable C#→Rust containers, end-to-end, with ZERO hand-written interop:
//   * the Rust side is three macro lines (see ../rustlib/src/lib.rs) — export_rust_hashmap!() +
//     export_rust_string!() (+ export_rust_containers!());
//   * the C# side is the shipped RustDotnet.RustHashMap<K,V> / RustDotnet.RustString (auto-included by
//     RustDotnet.targets because <UseRustDotnetContainers>true</UseRustDotnetContainers>).
// This file only USES them.

using System;
using RustDotnet;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        // ---- RustHashMap<K,V> : size-erased Rust HashMap keyed by raw key bytes ----
        using (var m = RustHashMap<int, long>.New())
        {
            Check("empty Count", m.Count, 0, ref pass, ref total);
            Check("insert new -> false", m.Insert(1, 100L), false, ref pass, ref total);
            Check("insert new -> false (2)", m.Insert(2, 200L), false, ref pass, ref total);
            Check("Count after 2", m.Count, 2, ref pass, ref total);
            Check("insert dup -> true", m.Insert(1, 111L), true, ref pass, ref total);
            Check("overwrote value", m[1], 111L, ref pass, ref total);
            Check("indexer get", m[2], 200L, ref pass, ref total);

            m[3] = 300L; // indexer set
            Check("indexer set", m[3], 300L, ref pass, ref total);
            Check("Count after 3", m.Count, 3, ref pass, ref total);

            Check("ContainsKey present", m.ContainsKey(2), true, ref pass, ref total);
            Check("ContainsKey absent", m.ContainsKey(99), false, ref pass, ref total);

            long got;
            Check("TryGetValue present", m.TryGetValue(3, out got), true, ref pass, ref total);
            Check("TryGetValue value", got, 300L, ref pass, ref total);
            Check("TryGetValue absent", m.TryGetValue(99, out got), false, ref pass, ref total);

            Check("Remove present", m.Remove(2), true, ref pass, ref total);
            Check("Remove absent", m.Remove(2), false, ref pass, ref total);
            Check("Count after remove", m.Count, 2, ref pass, ref total);
            Check("ContainsKey after remove", m.ContainsKey(2), false, ref pass, ref total);
        }

        // A map over a struct key (raw-byte identity).
        using (var m2 = RustHashMap<Coord, int>.New())
        {
            m2[new Coord { X = 1, Y = 2 }] = 12;
            m2[new Coord { X = 3, Y = 4 }] = 34;
            Check("struct-key Count", m2.Count, 2, ref pass, ref total);
            Check("struct-key get", m2[new Coord { X = 3, Y = 4 }], 34, ref pass, ref total);
            Check("struct-key contains", m2.ContainsKey(new Coord { X = 1, Y = 2 }), true, ref pass, ref total);
            Check("struct-key miss", m2.ContainsKey(new Coord { X = 9, Y = 9 }), false, ref pass, ref total);
        }

        // ---- RustString : Rust-owned mutable UTF-8 buffer, marshals to/from managed string ----
        using (var s = RustString.New())
        {
            Check("empty Length", s.Length, 0, ref pass, ref total);
            Check("empty ToString", s.ToString(), "", ref pass, ref total);

            s.Append("Hello");
            s.Append(", ");
            s.Append("world");
            Check("appended ToString", s.ToString(), "Hello, world", ref pass, ref total);
            Check("byte Length", s.Length, 12, ref pass, ref total);

            s.Clear();
            Check("Length after Clear", s.Length, 0, ref pass, ref total);
            Check("ToString after Clear", s.ToString(), "", ref pass, ref total);
        }

        // Non-ASCII round-trips as UTF-8 (byte length != char length).
        using (var s2 = RustString.From("café éé")) // "café éé"
        {
            Check("utf8 ToString", s2.ToString(), "café éé", ref pass, ref total);
            // 'c','a','f' = 3 bytes, 'é' = 2, ' ' = 1, 'é' = 2, 'é' = 2  => 10 bytes
            Check("utf8 byte Length", s2.Length, 10, ref pass, ref total);
        }

        Console.WriteLine($"cd_containers2: {pass}/{total} checks passed");
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

public struct Coord
{
    public int X;
    public int Y;
}
