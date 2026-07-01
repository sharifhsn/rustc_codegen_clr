// C# consumes a Rust type THROUGH a C#-defined interface.
//
// `cd_iface.dll` (produced from the Rust crate `cd_iface`) contains the managed class `Greeter`, which
// the backend emitted with an `implements [Contracts]Contracts.IGreeter` clause. Its two virtual
// methods — `Greet(string)` and `Priority()` — are Rust functions that satisfy the interface members.
//
// This program never uses the concrete `Greeter` API to invoke behaviour: it upcasts to `IGreeter`
// and calls everything through the interface. If the interface binding were missing, the CLR would
// throw a TypeLoadException the moment `Greeter` loads — so a clean run is itself the proof that a
// Rust type genuinely implements a C# interface and dispatches polymorphically.

using System;
using Contracts;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        // Construct the concrete Rust-defined class, then immediately view it ONLY as the interface.
        Greeter g = new Greeter(10);
        IGreeter ig = g; // upcast to the C#-defined interface (TypeLoadException here if unbound)

        // Interface-dispatched string method (string in, string out — full marshalling both ways).
        Check("ig.Greet(\"world\")", ig.Greet("world"), "Hello, world! (priority 10)", ref pass, ref total);
        Check("ig.Greet(\"Rust\")", ig.Greet("Rust"), "Hello, Rust! (priority 10)", ref pass, ref total);

        // Interface-dispatched int method (base_priority + 1).
        Check("ig.Priority()", ig.Priority(), 11, ref pass, ref total);

        // Distinct instance -> distinct field-backed result, still through the interface.
        IGreeter ig2 = new Greeter(41);
        Check("ig2.Priority()", ig2.Priority(), 42, ref pass, ref total);
        Check("ig2.Greet(\"x\")", ig2.Greet("x"), "Hello, x! (priority 41)", ref pass, ref total);

        // TRUE polymorphism: pass the Rust object to a method that only knows the interface, and to a
        // heterogeneous IGreeter[] loop — the call site has zero knowledge of the concrete type.
        Check("Describe(ig)", Describe(ig), "[11] Hello, poly! (priority 10)", ref pass, ref total);

        IGreeter[] greeters = { new Greeter(1), new Greeter(2), new Greeter(3) };
        int sum = 0;
        foreach (IGreeter x in greeters)
            sum += x.Priority();
        Check("sum of priorities via interface[]", sum, (1 + 1) + (2 + 1) + (3 + 1), ref pass, ref total);

        // `is`/`as` pattern-based interface test also holds.
        object boxed = new Greeter(7);
        Check("boxed is IGreeter", boxed is IGreeter, true, ref pass, ref total);
        Check("(boxed as IGreeter).Priority()", ((IGreeter)boxed).Priority(), 8, ref pass, ref total);

        Console.WriteLine($"cd_iface: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    // Knows nothing but the interface — genuine polymorphic use of the Rust implementation.
    private static string Describe(IGreeter x) => $"[{x.Priority()}] {x.Greet("poly")}";

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok)
            pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
