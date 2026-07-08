using System;

// A C# class implementing the Rust-defined `ISpeaker` interface. This only COMPILES if `ISpeaker`
// is a genuine .NET interface (a plain class/abstract-class would need `: base()`/`override` and
// couldn't be satisfied this way) — so the compile itself is the first proof the PE writer emitted
// the Interface+Abstract TypeDef correctly.
class Parrot : ISpeaker
{
    int vol = 11;
    public void Speak() => Console.WriteLine("Squawk!");
    public int Volume() => vol;
    public int SetVolume(int level) { vol = level; return vol; }
    public int Mix(int a, int b) => a + b;
    public string Describe() => "a parrot at volume " + vol;
}

// A C# class implementing the Rust-defined `IRefCell` interface — its members use the `ref`/`out`
// KEYWORDS. This only compiles if the Rust `&mut i32` parameters were emitted as managed byrefs
// (ELEMENT_TYPE_BYREF) with `ParamAttributes.Out` where `#[dotnet_out]` was used: under the old
// `int32*` (pointer) encoding csc would demand `int*` + unsafe instead.
class Cell : IRefCell
{
    public void Fill(ref int slot) { slot = 42; }
    public void FillOut(out int slot) { slot = 7; }
    public int AddInto(int a, ref int acc) { acc += a; return acc; }
    // The `static abstract` member with a byref param (`&mut i32` on a receiver-less trait fn).
    public static void Reset(ref int v) { v = -1; }
}

class Program
{
    static int checks = 0, passed = 0;
    static void Check(string name, bool ok)
    {
        checks++;
        if (ok) passed++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}");
    }

    static void Main()
    {
        // Polymorphic use THROUGH the interface — genuine interface dispatch.
        ISpeaker s = new Parrot();
        s.Speak();
        Check("interface dispatch: Volume() == 11", s.Volume() == 11);
        // Parameterized member + mutation, still through the interface reference.
        Check("SetVolume(42) returns 42", s.SetVolume(42) == 42);
        Check("Volume() reflects the mutation (== 42)", s.Volume() == 42);
        // Multiple parameters.
        Check("Mix(3, 4) == 7", s.Mix(3, 4) == 7);
        // Managed (System.String) return type through the interface.
        Check("Describe() returns the right string", s.Describe() == "a parrot at volume 42");
        Check("`is ISpeaker` holds on the implementor", s is ISpeaker);

        // Reflection: the type really is an interface, and Parrot reports implementing it.
        Check("typeof(ISpeaker).IsInterface", typeof(ISpeaker).IsInterface);
        var implemented = Array.Exists(typeof(Parrot).GetInterfaces(), i => i == typeof(ISpeaker));
        Check("Parrot.GetInterfaces() lists ISpeaker", implemented);

        // --- ref/out parameters (IRefCell): `&mut i32` => `ref int`, #[dotnet_out] => `out int`.
        IRefCell c = new Cell();
        int v = 0;
        c.Fill(ref v); // through the interface reference — genuine interface dispatch of a byref member
        Check("Fill(ref v): implementor's write observed through the byref (v == 42)", v == 42);
        c.FillOut(out int o);
        Check("FillOut(out o): definite-assignment out param (o == 7)", o == 7);
        int acc = 10;
        int sum = c.AddInto(5, ref acc);
        Check("AddInto(5, ref acc): mixed value+ref params (returns 15, acc == 15)", sum == 15 && acc == 15);
        // Static abstract member with a byref param, dispatched generically via the constraint.
        int r = 99;
        ResetVia<Cell>(ref r);
        Check("static abstract Reset(ref v) via T.Reset (r == -1)", r == -1);

        // Reflection: the metadata shape itself. `ref` = ByRef type + Flags 0; `out` = ByRef + Out.
        var fillP = typeof(IRefCell).GetMethod("Fill").GetParameters()[0];
        var fillOutP = typeof(IRefCell).GetMethod("FillOut").GetParameters()[0];
        Check("Fill's param ParameterType.IsByRef", fillP.ParameterType.IsByRef);
        Check("FillOut's param IsOut (ParamAttributes.Out survived the linker)", fillOutP.IsOut);
        Check("Fill's param is NOT IsOut (plain `ref` keeps Flags == 0, matching csc)", !fillP.IsOut);

        Console.WriteLine($"cd_interface: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }

    // .NET 7+ static virtual dispatch: `T.Reset(ref v)` under the interface constraint.
    static void ResetVia<T>(ref int v) where T : IRefCell => T.Reset(ref v);
}
