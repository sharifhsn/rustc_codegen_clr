// C# consumes two new #[dotnet_class] capabilities: real static fields, and real operator
// overloads (SpecialName-bound +/==/!=).
using System;

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

        Console.WriteLine(pass == total ? "PASS" : $"FAIL ({pass}/{total})");
        return pass == total ? 0 : 1;
    }
}
