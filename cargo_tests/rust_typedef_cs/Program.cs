// WF-8b — C# instantiates a Rust-defined managed class and calls its virtual method.
//
// `RustObj` is declared in Rust via `dotnet_typedef!` (see cargo_tests/rust_typedef) and emitted as a
// real managed class by the comptime interpreter. WF-8b adds a constructor, so C# can `new RustObj()`;
// `get_value()` is a virtual method that aliases an ordinary Rust fn returning 42.

using System;

public static class Program
{
    public static int Main()
    {
        RustObj o = new RustObj();
        int v = o.get_value();
        Console.WriteLine("C# new RustObj().get_value() = " + v);
        bool ok = v == 42;
        Console.WriteLine(ok ? "PASS" : "FAIL");
        return ok ? 0 : 1;
    }
}
