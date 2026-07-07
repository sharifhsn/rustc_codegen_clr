// C# observes a Rust-defined managed class explicitly `.override`ing System.Object.ToString().
//
// `cd_override.dll` (produced from the Rust crate `cd_override`) contains the managed class
// `Greeter`, emitted with a real ECMA-335 `.override` clause (a MethodImpl row) naming
// `System.Object.ToString()` as the base virtual — not an ordinary new-slot virtual method, and
// not `implements=` interface binding (there is no interface here at all).
//
// The decisive proof: call ToString() through an `Object`-typed reference, not just the concrete
// type. A same-name/signature virtual WITHOUT an explicit `.override` would create a NEW vtable
// slot (a shadow method) — calling it through `object` would still dispatch to the BCL's own
// `Object.ToString()` (the type's full name), not this override. Landing in Object's OWN slot is
// what makes this a genuine override, and only an Object-typed call site can distinguish the two.

using System;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        Greeter g = new Greeter(7);

        // Direct call on the concrete type -- would pass even for an accidental shadow method.
        Check("g.ToString()", g.ToString(), "Greeter #7", ref pass, ref total);

        // THE decisive check: upcast to Object, call ToString() through Object's own vtable slot.
        object boxed = g;
        Check("((object)g).ToString()", boxed.ToString(), "Greeter #7", ref pass, ref total);

        // Same check via a plain Object-typed local, and via string interpolation (which also
        // calls ToString() through the compile-time-static Object-ish path for a boxed value).
        Object asObject = g;
        Check("asObject.ToString()", asObject.ToString(), "Greeter #7", ref pass, ref total);
        Check("$\"{g}\"", $"{g}", "Greeter #7", ref pass, ref total);

        // A distinct instance -> distinct field-backed result, still through Object.
        object other = new Greeter(42);
        Check("other.ToString()", other.ToString(), "Greeter #42", ref pass, ref total);

        Console.WriteLine($"cd_override: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok)
            pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
