using System;

// A C# class implementing the DERIVED interface only. This only COMPILES if csc sees
// `interface IPet : IAnimal, ILoud` from the Rust-emitted metadata — the compiler then FORCES
// Impl to provide all three members (deleting Legs() or Volume() here is a CS0535), which is the
// compile-time half of the interface-inheritance proof.
class Impl : IPet
{
    public int Legs() => 4;
    public int Volume() => 11;
    public int Cuteness() => 5;
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
        // Reflection: IPet is a genuine interface and lists BOTH bases (multi-supertrait =>
        // one InterfaceImpl row each).
        Check("typeof(IPet).IsInterface", typeof(IPet).IsInterface);
        var bases = typeof(IPet).GetInterfaces();
        Check("IPet.GetInterfaces() contains IAnimal",
            Array.Exists(bases, i => i == typeof(IAnimal)));
        Check("IPet.GetInterfaces() contains ILoud and Length == 2",
            Array.Exists(bases, i => i == typeof(ILoud)) && bases.Length == 2);

        var impl = new Impl();
        IPet pet = impl;
        Check("Cuteness() == 5 through the IPet reference", pet.Cuteness() == 5);
        // Assignability through the BASE-interface references — genuine inheritance, not just
        // reflection metadata.
        IAnimal a = pet;
        Check("IAnimal a = (IPet)impl; a.Legs() == 4", a.Legs() == 4);
        ILoud l = impl;
        Check("ILoud l = impl; l.Volume() == 11", l.Volume() == 11);
        object o = impl;
        Check("object-typed impl `is IAnimal`", o is IAnimal);

        // A RUST implementor whose own TypeDef lists ONLY InterfaceImpl(IPet): the CLR computes
        // the transitive interface closure at load time from OUR emitted metadata.
        object dog = new Dog(4);
        Check("Rust Dog (implements only IPet) `is IAnimal`", dog is IAnimal);
        Check("((IAnimal)dog).Legs() == 4 (Rust body through the base interface)",
            ((IAnimal)dog).Legs() == 4);

        Console.WriteLine($"cd_iface_inherit: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }
}
