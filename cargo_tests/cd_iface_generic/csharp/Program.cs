using System;

// C# classes implementing TWO DIFFERENT instantiations of the Rust-defined GENERIC interface
// `IBox<T>`. This only COMPILES if `IBox`1` is a genuine generic .NET interface definition
// (backtick-arity TypeDef + a GenericParam row + ET_VAR member signatures): a monomorphized fake
// or a non-generic interface couldn't be closed over `int` AND `string` at once.
class IntBox : IBox<int>
{
    int v;
    public int Get() => v;
    public void Put(int x) => v = x;
    public int Count() => 1;
}

class StrBox : IBox<string>
{
    string s = "";
    public string Get() => s;
    public void Put(string x) => s = x;
    public int Count() => 2;
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
        // Reflection over the OPEN definition: it must be an interface AND a genuine generic
        // type definition, with the declared parameter name surviving into metadata.
        Check("typeof(IBox<>) is an interface", typeof(IBox<>).IsInterface);
        Check("typeof(IBox<>) is a generic type DEFINITION", typeof(IBox<>).IsGenericTypeDefinition);
        Check("GetGenericArguments()[0].Name == \"T\"", typeof(IBox<>).GetGenericArguments()[0].Name == "T");
        Check("Get()'s return type is the generic parameter (ET_VAR)",
              typeof(IBox<>).GetMethod("Get").ReturnType.IsGenericParameter);

        // Closed instantiation: implementor assignability + genuine interface dispatch.
        var intBox = new IntBox();
        Check("new IntBox() is IBox<int>", intBox is IBox<int>);
        IBox<int> b = intBox;
        b.Put(42);
        Check("interface dispatch: Put(42) then Get() == 42", b.Get() == 42);
        Check("non-generic member through the interface: Count() == 1", b.Count() == 1);

        // TRUE genericity: a generic C# helper constrained only on IBox<T> works for a SECOND,
        // reference-typed instantiation (string) — no monomorphized fake could satisfy both.
        Check("generic helper over IBox<string>: Roundtrip(strBox, \"hi\") == \"hi\"",
              Roundtrip(new StrBox(), "hi") == "hi");

        // Rust-side surface: `MainModule.take_box(IBox<int>)` — its parameter is an
        // INSTANTIATION of the assembly's own generic interface (an in-assembly GENERICINST
        // TypeSpec in the exported signature), and an IntBox is assignable to it.
        Check("MainModule.take_box(IBox<int>) == 7", MainModule.take_box(intBox) == 7);

        Console.WriteLine($"cd_iface_generic: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }

    // Knows nothing about IntBox/StrBox — pure IBox<T> polymorphism.
    static T Roundtrip<T>(IBox<T> box, T value)
    {
        box.Put(value);
        return box.Get();
    }
}
