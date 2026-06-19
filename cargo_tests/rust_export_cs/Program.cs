// WF-7 Phase 1 — the C# consumption side.
//
// This is the proof that C# can call INTO a Rust module compiled by this backend. The Rust crate
// `rust_export` is compiled to a managed .NET assembly (rust_export.dll); its #[no_mangle] functions
// land as `public static` methods on the `MainModule` class. Here we load that assembly and invoke
// them — initially via reflection (robust to the assembly's current identity name), upgraded to
// direct typed calls once the assembly is named after the crate.

using System;
using System.Reflection;

public static class Program
{
    public static int Main(string[] args)
    {
        string dllPath = args.Length > 0 ? args[0] : "rust_export.dll";
        Assembly asm = Assembly.LoadFrom(dllPath);
        Type mm = asm.GetType("MainModule");
        if (mm == null) { Console.WriteLine("FAIL: no MainModule type"); return 1; }

        int ok = 0, total = 0;
        ok += Check(mm, "rust_fib", new object[] { 10 }, 55, ref total);
        ok += Check(mm, "rust_add", new object[] { 2, 3 }, 5, ref total);
        ok += Check(mm, "rust_mul", new object[] { 4, 5 }, 20, ref total);
        ok += Check(mm, "rust_add_f64", new object[] { 1.5, 2.25 }, 3.75, ref total);

        Console.WriteLine(ok == total ? "PASS" : "FAIL (" + ok + "/" + total + ")");
        return ok == total ? 0 : 1;
    }

    static int Check(Type mm, string name, object[] argv, object expected, ref int total)
    {
        total++;
        MethodInfo mi = mm.GetMethod(name);
        if (mi == null) { Console.WriteLine("  " + name + ": MISSING (inlined away?)"); return 0; }
        object got = mi.Invoke(null, argv);
        bool pass = got.Equals(expected);
        Console.WriteLine("  C# -> Rust " + name + " = " + got + (pass ? " ✓" : " ✗ expected " + expected));
        return pass ? 1 : 0;
    }
}
