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

        Console.WriteLine($"cd_interface: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }
}
