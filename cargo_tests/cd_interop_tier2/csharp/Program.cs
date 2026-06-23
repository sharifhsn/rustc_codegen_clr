// Tier-2 consumer: a Rust fn returning a managed System.String, and a Rust fn raising a .NET
// exception. Before the CS0012 fix this project FAILS TO COMPILE (System.String unresolved).
using System;
using System.Text;

public static class Program
{
    public static unsafe int Main()
    {
        int pass = 0, total = 0;

        Check("rust_add(2,3)", MainModule.rust_add(2, 3), 5, ref pass, ref total);

        byte[] utf8 = Encoding.UTF8.GetBytes("World");
        fixed (byte* np = utf8)
        {
            string gm = MainModule.greet_managed(np, (nuint)utf8.Length);
            Check("greet_managed(\"World\")", gm, "Hello, World, from Rust (managed)!", ref pass, ref total);
        }

        Check("try_div(10,2)", MainModule.try_div(10, 2), 5, ref pass, ref total);
        bool threw = false;
        try { MainModule.try_div(1, 0); }
        catch (Exception) { threw = true; }
        Check("try_div(1,0) -> C# catch", threw, true, ref pass, ref total);

        // Rust returns a first-class managed System.Int32[] (newarr + stelem), consumed as int[].
        int[] ints = MainModule.make_ints();
        Check("make_ints().Length", ints.Length, 3, ref pass, ref total);
        Check("make_ints()[0]", ints[0], 10, ref pass, ref total);
        Check("make_ints()[1]", ints[1], 20, ref pass, ref total);
        Check("make_ints()[2]", ints[2], 30, ref pass, ref total);

        Console.WriteLine(pass == total ? "PASS" : $"FAIL ({pass}/{total})");
        return pass == total ? 0 : 1;
    }

    static void Check(string label, object got, object expected, ref int pass, ref int total)
    {
        total++;
        bool ok = got.Equals(expected);
        Console.WriteLine("  C# -> Rust " + label + " = " + got + (ok ? " ok" : " FAIL expected " + expected));
        if (ok) pass++;
    }
}
