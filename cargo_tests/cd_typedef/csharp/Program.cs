// C# consumes a managed class DEFINED IN RUST via `#[dotnet_class]` + `#[dotnet_methods]`.
//
// `cd_typedef.dll` is the .NET class library produced from the Rust crate `cd_typedef`, whose only
// content is a `#[dotnet_class] struct Counter` and a `#[dotnet_methods] impl Counter`. The backend
// synthesized the managed class `Counter : System.Object` with:
//   * a parameterized primary constructor `Counter(int, long)` and a parameterless `Counter()`
//     (multiple, overloaded ctors);
//   * `read_value()`/`read_step()` accessors AND `set_value(int)`/`set_step(long)` mutators (property-
//     like getters/setters at the method level);
//   * a STATIC method `Counter.make(int, long)` and an INSTANCE method `sum()` — both Rust fns.
// so this C# program can drive the full surface and check every result.

using System;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        // --- Primary ctor: `new Counter(value, step)` stores each arg into its field. ---
        Counter c = new Counter(5, 100);
        Check("c.read_value()", c.read_value(), 5, ref pass, ref total);
        Check("c.read_step()", c.read_step(), 100L, ref pass, ref total);

        // A second instance with a negative i32 and a >32-bit i64, to be sure each ctor call is
        // independent and the widths round-trip.
        Counter c2 = new Counter(-1, 9_000_000_000L);
        Check("c2.read_value()", c2.read_value(), -1, ref pass, ref total);
        Check("c2.read_step()", c2.read_step(), 9_000_000_000L, ref pass, ref total);

        // --- Parameterless ctor: `new Counter()` gives zero-initialized fields (a SECOND ctor). ---
        Counter d = new Counter();
        Check("new Counter().read_value()", d.read_value(), 0, ref pass, ref total);
        Check("new Counter().read_step()", d.read_step(), 0L, ref pass, ref total);

        // --- Field setters: `set_value`/`set_step` mutate the fields; `read_*` observes the change. ---
        d.set_value(42);
        d.set_step(7L);
        Check("after set_value(42)", d.read_value(), 42, ref pass, ref total);
        Check("after set_step(7)", d.read_step(), 7L, ref pass, ref total);

        // --- Static method: `Counter.make(value, step)` builds a Counter (Rust-side newobj). ---
        Counter m = Counter.make(11, 22L);
        Check("Counter.make -> read_value", m.read_value(), 11, ref pass, ref total);
        Check("Counter.make -> read_step", m.read_step(), 22L, ref pass, ref total);

        // --- Instance method: `sum()` = value + step (widened). ---
        Check("c.sum()", c.sum(), 105L, ref pass, ref total);           // 5 + 100
        Check("c2.sum()", c2.sum(), 8_999_999_999L, ref pass, ref total); // -1 + 9_000_000_000
        Check("Counter.make(11,22).sum()", m.sum(), 33L, ref pass, ref total);

        // --- Managed-type fields: `Pair` holds two `Counter` references. ---
        Pair p = new Pair(new Counter(1, 2), new Counter(3, 4));
        Check("p.read_left().sum()", p.read_left().sum(), 3L, ref pass, ref total);   // 1 + 2
        Check("p.read_right().sum()", p.read_right().sum(), 7L, ref pass, ref total); // 3 + 4
        Check("p.total()", p.total(), 10L, ref pass, ref total);                      // 3 + 7

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
