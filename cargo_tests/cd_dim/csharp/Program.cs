using System;
using System.Reflection;

// A C# class implementing ONLY the abstract member of the Rust-defined `ICalc` interface. This
// only COMPILES if `Doubled`/`PlusN`/`Fixed`/`DoubledPlus` are genuine DEFAULT interface methods
// (virtual, non-abstract, with a real body): if they were abstract, csc would error CS0535
// ("does not implement interface member") right here — so the compile itself is the first proof.
class Minimal : ICalc
{
    public int Base() => 21;
}

// A class that DEFINES one of the defaulted members — its definition must win over the DIM.
class Overrider : ICalc
{
    public int Base() => 10;
    public int Doubled() => 999;
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
        // DIMs are only reachable through the interface reference (C# semantics).
        ICalc m = new Minimal();
        Check("abstract sibling: m.Base() == 21", m.Base() == 21);
        // The DIM body runs, and its `self.Base()` dispatches back into Minimal.Base.
        Check("DIM runs + self-call dispatches to the class: m.Doubled() == 42", m.Doubled() == 42);
        // A DIM with an argument.
        Check("DIM with an argument: m.PlusN(5) == 26", m.PlusN(5) == 26);
        // A self-free DIM on a class that implemented nothing but Base.
        Check("self-free DIM: m.Fixed() == 7", m.Fixed() == 7);
        // A DIM calling another DIM (which self-calls Base) — two dispatch levels.
        Check("DIM calling a DIM: m.DoubledPlus(1) == 43", m.DoubledPlus(1) == 43);

        ICalc o = new Overrider();
        // A class's own definition beats the DIM.
        Check("class override beats the DIM: o.Doubled() == 999", o.Doubled() == 999);
        // An inherited DIM still self-dispatches into the OVERRIDER's Base.
        Check("inherited DIM dispatches to Overrider.Base: o.PlusN(1) == 11", o.PlusN(1) == 11);
        // A DIM whose inner self-call (`self.Doubled()`) lands on the CLASS's override —
        // virtual dispatch from inside a default body.
        Check("DIM's inner self-call hits the class override: o.DoubledPlus(1) == 1000",
              o.DoubledPlus(1) == 1000);

        // Reflection: the metadata shape itself — a genuine DIM (flags 0x146: Virtual|NewSlot,
        // NOT Abstract) next to an unharmed abstract sibling.
        MethodInfo dim = typeof(ICalc).GetMethod("Doubled");
        Check("reflection: Doubled is !IsAbstract && IsVirtual (a genuine DIM)",
              !dim.IsAbstract && dim.IsVirtual);
        Check("reflection: Base stays IsAbstract (abstract sibling unharmed)",
              typeof(ICalc).GetMethod("Base").IsAbstract);

        Console.WriteLine($"cd_dim: {passed}/{checks} checks passed");
        if (passed != checks) Environment.Exit(1);
    }
}
