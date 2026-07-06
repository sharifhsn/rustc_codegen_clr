// C# consumes Rust functions exported ergonomically via `#[dotnet_export]`.
//
// `cd_export.dll` is the .NET class library produced from the Rust crate `cd_export`, whose functions
// carry `#[dotnet_export]`. The macro generated the seam shims; C# calls them as ordinary typed static
// methods on `MainModule` — string parameters/returns are real managed `System.String`, so there is
// NO `(ptr, len)` marshalling here at all (contrast cd_interop/Program.cs, which pins buffers by hand).

using System;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        // string greet(string) — inbound &str, outbound String, both as managed string.
        Check("greet(\"World\")", MainModule.greet("World"), "Hello, World, from Rust!", ref pass, ref total);
        Check("greet(\"\")", MainModule.greet(""), "Hello, , from Rust!", ref pass, ref total);

        // Primitive passthrough.
        Check("add(2,3)", MainModule.add(2, 3), 5, ref pass, ref total);
        Check("add(-4,10)", MainModule.add(-4, 10), 6, ref pass, ref total);

        // bool return.
        Check("is_even(4)", MainModule.is_even(4L), true, ref pass, ref total);
        Check("is_even(7)", MainModule.is_even(7L), false, ref pass, ref total);

        // mixed float/int.
        Check("scale(2.5, 4)", MainModule.scale(2.5, 4), 10.0, ref pass, ref total);

        // String (owned) inbound, String outbound.
        Check("shout(\"hi\")", MainModule.shout("hi"), "HI!", ref pass, ref total);

        // &str inbound, primitive return — proves the string content crossed intact.
        Check("str_len(\"héllo\")", MainModule.str_len("héllo"), 6, ref pass, ref total); // é is 2 UTF-8 bytes

        // no params, &'static str return.
        Check("version()", MainModule.version(), "cd_export 1.0", ref pass, ref total);

        // void export — just prove it links and is callable.
        MainModule.ping();
        Check("ping() callable", true, true, ref pass, ref total);

        // Panic-safety: a #[dotnet_export] fn that panics must surface as a genuine, catchable
        // managed exception (NOT a process abort via Environment.FailFast), carrying the panic
        // message, and the process must remain healthy afterward (a following call still works).
        {
            total++;
            bool caught = false;
            string message = null;
            try
            {
                MainModule.boom("division safety check failed");
            }
            catch (Exception e)
            {
                caught = true;
                message = e.Message;
            }
            bool ok = caught && message != null && message.Contains("boom: division safety check failed");
            if (ok) pass++;
            Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] boom(...) panics as catchable exception: caught={caught}, message=\"{message}\"");
        }

        // Process-health check: a normal call after the panic must still succeed (proves the panic
        // didn't corrupt or abort the runtime — it took an ordinary managed-exception control path).
        Check("add(2,3) after boom() panic", MainModule.add(2, 3), 5, ref pass, ref total);

        Console.WriteLine($"cd_export: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok) pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
