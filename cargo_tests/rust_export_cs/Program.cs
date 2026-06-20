// WF-7/WF-8 — the C# consumption side, using DIRECT TYPED calls (no reflection).
//
// `rust_export.dll` is the .NET class-library assembly produced from the Rust `cdylib` crate
// `rust_export`. It is named after its crate AND (WF-8a) emits proper `.assembly extern` BCL
// identities, so C# references it directly and calls its `#[no_mangle]` functions as ordinary static
// methods on `MainModule` — no P/Invoke, no marshalling attributes, no reflection. They are managed
// calls, because the Rust was compiled to managed CIL.
//
// P1: primitive signatures. P2: string marshalling — UTF-8 (ptr, len) in, out-buffer out.

using System;
using System.Text;

public static class Program
{
    public static unsafe int Main()
    {
        int pass = 0, total = 0;

        // ---- P1: primitives (direct typed managed calls) ----
        Check("rust_add(2,3)", MainModule.rust_add(2, 3), 5, ref pass, ref total);
        Check("rust_mul(4,5)", MainModule.rust_mul(4, 5), 20, ref pass, ref total);
        Check("rust_fib(10)", MainModule.rust_fib(10), 55, ref pass, ref total);
        Check("rust_add_f64(1.5,2.25)", MainModule.rust_add_f64(1.5, 2.25), 3.75, ref pass, ref total);

        // ---- P2: string marshalling (UTF-8 ptr+len) ----
        byte[] utf8 = Encoding.UTF8.GetBytes("World");
        fixed (byte* np = utf8)
        {
            // C# string -> Rust &str (inbound).
            Check("rust_strlen(\"World\")", MainModule.rust_strlen(np, (nuint)utf8.Length), 5, ref pass, ref total);

            // Rust String -> C# string (outbound, via a caller-provided out-buffer).
            byte[] outbuf = new byte[256];
            fixed (byte* op = outbuf)
            {
                nuint n = MainModule.greet(np, (nuint)utf8.Length, op, (nuint)outbuf.Length);
                string greeting = Encoding.UTF8.GetString(outbuf, 0, (int)n);
                Check("greet(\"World\")", greeting, "Hello, World, from Rust!", ref pass, ref total);
            }

            // Rust String -> C# string, returned DIRECTLY as a managed System.String (WF-8c).
            string gm = MainModule.greet_managed(np, (nuint)utf8.Length);
            Check("greet_managed(\"World\")", gm, "Hello, World, from Rust (managed)!", ref pass, ref total);
        }

        // ---- WF-8d: struct marshalling (de-mangled `rust_export.Point` value-type) ----
        // The Rust struct is referenced directly by its clean name; the backend-synthesized public
        // constructor + per-field getters make it constructible and readable from C#.
        rust_export.Point p = new rust_export.Point(2, 3); // inbound: C# value-type -> Rust
        Check("point_sum(new Point(2,3))", MainModule.point_sum(p), 5, ref pass, ref total);
        rust_export.Point q = MainModule.make_point(4, 5); // outbound: Rust -> C# value-type
        Check("make_point(4,5).get_x()", q.get_x(), 4, ref pass, ref total);
        Check("make_point(4,5).get_y()", q.get_y(), 5, ref pass, ref total);

        Console.WriteLine(pass == total ? "PASS" : $"FAIL ({pass}/{total})");
        return pass == total ? 0 : 1;
    }

    static void Check(string label, object got, object expected, ref int pass, ref int total)
    {
        total++;
        bool ok = got.Equals(expected);
        Console.WriteLine("  C# -> Rust " + label + " = " + got + (ok ? " ✓" : " ✗ expected " + expected));
        if (ok) pass++;
    }
}
