using System;

// A C# class implementing the Rust-defined `ISpeaker` interface. This only COMPILES if `ISpeaker`
// is a genuine .NET interface (a plain class/abstract-class would need `: base()`/`override` and
// couldn't be satisfied this way) — so the compile itself is the first proof the PE writer emitted
// the Interface+Abstract TypeDef correctly.
class Parrot : ISpeaker
{
    public void Speak() => Console.WriteLine("Squawk!");
    public int Volume() => 11;
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
        Check("`is ISpeaker` holds on the implementor", s is ISpeaker);

        // Reflection: the type really is an interface, and Parrot reports implementing it.
        Check("typeof(ISpeaker).IsInterface", typeof(ISpeaker).IsInterface);
        var implemented = Array.Exists(typeof(Parrot).GetInterfaces(), i => i == typeof(ISpeaker));
        Check("Parrot.GetInterfaces() lists ISpeaker", implemented);

        Console.WriteLine($"cd_interface: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }
}
