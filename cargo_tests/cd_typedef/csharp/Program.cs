// C# consumes a managed class DEFINED IN RUST via `#[dotnet_class]`.
//
// `cd_typedef.dll` is the .NET class library produced from the Rust crate `cd_typedef`, whose only
// content is a `#[dotnet_class] struct Counter`. The backend synthesized the managed class
// `Counter : System.Object` with a parameterized primary constructor and `read_value()`/`read_step()`
// accessors — so this C# program can `new Counter(...)` (proving the new parameterized-ctor capability)
// and read back the ctor-initialized fields.

using System;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        // Primary ctor: `new Counter(value, step)` stores each arg into its field.
        Counter c = new Counter(5, 100);
        Check("c.read_value()", c.read_value(), 5, ref pass, ref total);
        Check("c.read_step()", c.read_step(), 100L, ref pass, ref total);

        // A second instance with a negative i32 and a >32-bit i64, to be sure each ctor call is
        // independent and the widths round-trip.
        Counter c2 = new Counter(-1, 9_000_000_000L);
        Check("c2.read_value()", c2.read_value(), -1, ref pass, ref total);
        Check("c2.read_step()", c2.read_step(), 9_000_000_000L, ref pass, ref total);

        Console.WriteLine($"cd_typedef: {pass}/{total} checks passed");
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
