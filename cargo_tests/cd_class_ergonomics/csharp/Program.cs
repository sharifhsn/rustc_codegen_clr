// C# consumes three new #[dotnet_class] capabilities: real static fields, real operator
// overloads (SpecialName-bound +/==/!=), and base constructors that take arguments.
using System;
using CdClassErgonomicsBase;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;
        void Check(string label, object got, object expected)
        {
            total++;
            bool ok = got.Equals(expected);
            Console.WriteLine("  " + label + " = " + got + (ok ? " (ok)" : $" (FAIL, want {expected})"));
            if (ok) pass++;
        }

        // ---- Static field: directly public, no accessor needed from C# ----
        Check("Counter.Count initial", Counter.Count, 0);
        Counter.Count = 5;
        Check("Counter.Count after direct set", Counter.Count, 5);
        Check("Counter.bump() (Rust reads/writes it too)", Counter.bump(), 6);
        Check("Counter.Count after Rust bump()", Counter.Count, 6);

        // ---- Real operator overloads ----
        Vector2 a = Vector2.make(1, 2);
        Vector2 b = Vector2.make(3, 4);
        Vector2 sum = a + b; // real `+` syntax, not a.op_Addition(b)
        Check("(a+b).get_x()", sum.get_x(), 4);
        Check("(a+b).get_y()", sum.get_y(), 6);

        Vector2 c = Vector2.make(1, 2);
        Check("a == c (real == syntax)", a == c, true);
        Check("a != b (real != syntax)", a != b, true);
        Check("a == b", a == b, false);

        // ---- Base constructors that take arguments ----
        Gadget g = Gadget.make_gadget(42, 7);
        Check("Gadget(42, 7).Seed (forwarded to Widget's base ctor)", ((Widget)g).Seed, 42);
        Check("Gadget(42, 7).get_tag() (own field)", g.get_tag(), 7);

        Console.WriteLine(pass == total ? "PASS" : $"FAIL ({pass}/{total})");
        return pass == total ? 0 : 1;
    }
}
