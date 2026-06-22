// J3 — the C# consumption side of a Rust library built by the REAL `cargo dotnet` flow.
//
// `cd_interop.dll` is the .NET class-library assembly produced from the Rust `cdylib` crate
// `cd_interop` (target_os=dotnet, build-std with panic_unwind). It is named after its crate AND
// emits proper `.assembly extern` BCL identities, so C# references it directly and calls its
// `#[no_mangle] pub extern "C"` functions as ordinary static methods on `MainModule` — no P/Invoke,
// no marshalling attributes, no reflection. They are managed calls, because the Rust was compiled to
// managed CIL.
//
// Tier 1 marshalling: primitives + UTF-8 (ptr, len) strings + a de-mangled struct value-type + an
// inbound slice. Every assertion checks the C#-observed result against what Rust computes.

using System;
using System.Text;

public static class Program
{
    public static unsafe int Main()
    {
        int pass = 0, total = 0;

        // ---- primitives (direct typed managed call) ----
        Check("rust_add(2,3)", MainModule.rust_add(2, 3), 5, ref pass, ref total);

        // ---- string marshalling (UTF-8 ptr+len, out-buffer round-trip) ----
        byte[] utf8 = Encoding.UTF8.GetBytes("World");
        fixed (byte* np = utf8)
        {
            byte[] outbuf = new byte[256];
            fixed (byte* op = outbuf)
            {
                // C# string -> Rust &str (inbound), Rust String -> C# string (outbound).
                nuint n = MainModule.greet(np, (nuint)utf8.Length, op, (nuint)outbuf.Length);
                string greeting = Encoding.UTF8.GetString(outbuf, 0, (int)n);
                Check("greet(\"World\")", greeting, "Hello, World, from Rust!", ref pass, ref total);
            }
        }

        // ---- struct marshalling (de-mangled `cd_interop.Point` value-type) ----
        // The Rust struct is referenced directly by its clean name; the backend-synthesized public
        // constructor + per-field getters make it constructible and readable from C#.
        cd_interop.Point p = new cd_interop.Point(2, 3);              // inbound: C# value-type -> Rust
        Check("point_sum(new Point(2,3))", MainModule.point_sum(p), 5, ref pass, ref total);
        cd_interop.Point q = MainModule.make_point(4, 5);            // outbound: Rust -> C# value-type
        Check("make_point(4,5).get_x()", q.get_x(), 4, ref pass, ref total);
        Check("make_point(4,5).get_y()", q.get_y(), 5, ref pass, ref total);

        // ---- collection marshalling (inbound slice / "Vec sum") ----
        int[] nums = { 1, 2, 3, 4 };
        fixed (int* sp = nums)                                       // inbound: C# int[] -> Rust &[i32]
        {
            Check("sum_slice([1,2,3,4])", MainModule.sum_slice(sp, (nuint)nums.Length), 10, ref pass, ref total);
        }

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
