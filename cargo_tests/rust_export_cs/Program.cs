// WF-7 — the C# consumption side.
//
// Loads the Rust-produced .NET assembly (a `cdylib` named `rust_export`) and calls its `#[no_mangle]`
// functions. These are ordinary managed calls (the Rust was compiled to managed CIL), invoked here via
// reflection: the assembly's BCL references carry version 0.0.0.0, which the C# *compiler* rejects for a
// direct typed reference (CS0012) but the *runtime* resolves fine — so reflection is the portable harness
// until the assembly emits proper BCL reference versions (WF-8 packaging).
//
// P1: primitives. P2: string marshalling via the UTF-8 (ptr, len) convention — `rust_strlen` (C# string
// -> Rust &str) and `greet` (Rust String -> C# string via a caller-provided out-buffer).

using System;
using System.Reflection;
using System.Text;

public static class Program
{
    public static unsafe int Main(string[] args)
    {
        string dllPath = args.Length > 0 ? args[0] : "rust_export.dll";
        Assembly asm = Assembly.LoadFrom(dllPath);
        Type mm = asm.GetType("MainModule");
        if (mm == null) { Console.WriteLine("FAIL: no MainModule type"); return 1; }

        int pass = 0, total = 0;

        // ---- P1: primitives ----
        Check("rust_add(2,3)", Call(mm, "rust_add", 2, 3), 5, ref pass, ref total);
        Check("rust_mul(4,5)", Call(mm, "rust_mul", 4, 5), 20, ref pass, ref total);
        Check("rust_fib(10)", Call(mm, "rust_fib", 10), 55, ref pass, ref total);
        Check("rust_add_f64(1.5,2.25)", Call(mm, "rust_add_f64", 1.5, 2.25), 3.75, ref pass, ref total);

        // ---- P2: string marshalling (UTF-8 ptr+len) ----
        byte[] utf8 = Encoding.UTF8.GetBytes("World");
        fixed (byte* np = utf8)
        {
            // C# string -> Rust &str (inbound).
            object slen = mm.GetMethod("rust_strlen").Invoke(null,
                new object[] { Pointer.Box(np, typeof(byte*)), (UIntPtr)utf8.Length });
            Check("rust_strlen(\"World\")", slen, 5, ref pass, ref total);

            // Rust String -> C# string (outbound, via a caller-provided out-buffer).
            byte[] outbuf = new byte[256];
            fixed (byte* op = outbuf)
            {
                object n = mm.GetMethod("greet").Invoke(null, new object[]
                {
                    Pointer.Box(np, typeof(byte*)), (UIntPtr)utf8.Length,
                    Pointer.Box(op, typeof(byte*)), (UIntPtr)outbuf.Length,
                });
                int written = (int)(ulong)(UIntPtr)n;
                string greeting = Encoding.UTF8.GetString(outbuf, 0, written);
                Check("greet(\"World\")", greeting, "Hello, World, from Rust!", ref pass, ref total);
            }
        }

        Console.WriteLine(pass == total ? "PASS" : $"FAIL ({pass}/{total})");
        return pass == total ? 0 : 1;
    }

    static object Call(Type mm, string name, params object[] argv) =>
        mm.GetMethod(name).Invoke(null, argv);

    static void Check(string label, object got, object expected, ref int pass, ref int total)
    {
        total++;
        bool ok = got.Equals(expected);
        Console.WriteLine("  C# -> Rust " + label + " = " + got + (ok ? " ✓" : " ✗ expected " + expected));
        if (ok) pass++;
    }
}
