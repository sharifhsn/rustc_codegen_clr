// C# consumes a Rust type through a GENERIC BCL interface, `System.IEquatable<int>`.
//
// `cd_generic_iface.dll` (produced from the Rust crate `cd_generic_iface`) contains the managed
// class `IntBox`, which the backend emitted with an `implements class [System.Runtime]'IEquatable`1'
// <int32>` clause (a real closed generic instantiation, not the unbound open `IEquatable`1`) — see
// `rustc_codegen_clr_add_generic_interface_impl`. If that metadata were wrong (an unbound open
// generic, or a plain non-generic TypeRef missing the arity suffix entirely), the CLR would reject
// `IntBox` at load time with a `TypeLoadException` the moment this program starts — a clean run
// through `IEquatable<int>` is itself the proof the generic interface binding is correct.
//
// This program never uses the concrete `IntBox` API to check equality: it upcasts to
// `IEquatable<int>` and calls `Equals` only through the interface.

using System;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        IntBox five = new IntBox(5);
        IEquatable<int> eq = five; // upcast to the generic interface (TypeLoadException here if unbound)

        Check("eq.Equals(5)", eq.Equals(5), true, ref pass, ref total);
        Check("eq.Equals(6)", eq.Equals(6), false, ref pass, ref total);
        Check("five.Value()", five.Value(), 5, ref pass, ref total);

        // TRUE polymorphism: a method that knows only `IEquatable<int>`, never `IntBox`.
        Check("MatchesZero(five)", MatchesZero(five), false, ref pass, ref total);
        Check("MatchesZero(zero)", MatchesZero(new IntBox(0)), true, ref pass, ref total);

        // `is`/`as` pattern-based interface test also holds.
        object boxed = new IntBox(42);
        Check("boxed is IEquatable<int>", boxed is IEquatable<int>, true, ref pass, ref total);
        Check("(boxed as IEquatable<int>).Equals(42)", ((IEquatable<int>)boxed).Equals(42), true, ref pass, ref total);

        Console.WriteLine($"cd_generic_iface: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    // Knows nothing but the interface — genuine polymorphic use of the Rust implementation.
    private static bool MatchesZero(IEquatable<int> x) => x.Equals(0);

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok)
            pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
