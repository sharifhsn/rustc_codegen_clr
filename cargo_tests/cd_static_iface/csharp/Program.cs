using System;
using System.Reflection;

// A C# class implementing the Rust-defined `IParse` interface, whose `Make`/`Add` members are
// STATIC ABSTRACT (.NET 7+ static virtual members in interfaces). This only COMPILES if the PE
// writer emitted them with the exact Roslyn shape (Public|Static|Virtual|HideBySig|Abstract, no
// NewSlot, SIG_DEFAULT sig) — csc's static-virtual feature gate and the CoreCLR loader are both
// stricter about this member kind than about instance abstracts.
class X : IParse
{
    public static int Make() => 42;
    public static int Add(int a, int b) => a + b;
    public int Describe() => 7;
}

class Program
{
    static int checks = 0, passed = 0;
    static void Check(string name, bool ok)
    {
        checks++;
        if (ok) passed++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}");
    }

    // Constrained generic dispatch THROUGH the static abstract member — the whole point of the
    // feature (the `INumber<T>` generic-math pattern).
    static int CallMake<T>() where T : IParse => T.Make();
    static int CallAdd<T>(int a, int b) where T : IParse => T.Add(a, b);

    static void Main()
    {
        Check("generic dispatch: CallMake<X>() == 42", CallMake<X>() == 42);
        Check("parameterized: CallAdd<X>(3, 4) == 7", CallAdd<X>(3, 4) == 7);
        Check("direct call on the implementer: X.Make() == 42", X.Make() == 42);

        // Reflection: the member really is a static abstract on a genuine interface.
        var make = typeof(IParse).GetMethod("Make", BindingFlags.Public | BindingFlags.Static);
        Check(
            "typeof(IParse).IsInterface && Make is static+abstract+virtual",
            typeof(IParse).IsInterface && make != null && make.IsStatic && make.IsAbstract && make.IsVirtual
        );

        // An INSTANCE member on the same interface still dispatches normally.
        Check("mixed member: ((IParse)new X()).Describe() == 7", ((IParse)new X()).Describe() == 7);

        Console.WriteLine($"cd_static_iface: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }
}
