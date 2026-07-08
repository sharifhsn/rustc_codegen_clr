using System;
using System.Linq;
using System.Reflection;

// A C# class implementing the Rust-defined interface whose members include GENERIC METHOD
// DEFINITIONS. This only COMPILES if `T Echo<T>(T value)` is a genuine generic method definition
// (SIG_GENERIC sig blob + a method-owned GenericParam row + ET_MVAR positions): csc matches the
// implementing method's arity + signature against the interface member's metadata exactly.
class Conv : IConverter
{
    public int Describe() => 7;
    public T Echo<T>(T value) => value;
    public K First<K, V>(K key, V value) => key;
}

// Mixed namespaces: the owning interface's T (`!0`) and the method's own U (`!!0`) in one
// signature — `U Pick<U>(int a, U b)` once closed over IPicker<int>.
class IntPicker : IPicker<int>
{
    public U Pick<U>(int a, U b) => b;
    public int Base() => 5;
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
        IConverter c = new Conv();

        // Interface dispatch at a VALUE-type and a REFERENCE-type instantiation — genuine
        // genericity (a monomorphized fake couldn't satisfy both from one definition).
        Check("c.Echo(42) == 42 (value-type instantiation)", c.Echo(42) == 42);
        Check("c.Echo(\"hi\") == \"hi\" (reference-type instantiation)", c.Echo("hi") == "hi");
        Check("c.First(3, \"x\") == 3 (two generic parameters)", c.First(3, "x") == 3);
        Check("non-generic member unchanged: c.Describe() == 7", c.Describe() == 7);

        // Reflection over the DEFINITION: the GenericParam rows and SIG_GENERIC blob must
        // round-trip as a generic method definition with the declared parameter names.
        MethodInfo echo = typeof(IConverter).GetMethod("Echo");
        Check("Echo is a generic method DEFINITION", echo.IsGenericMethodDefinition);
        Check("Echo declares exactly one generic parameter named \"T\"",
              echo.GetGenericArguments().Length == 1 && echo.GetGenericArguments()[0].Name == "T");
        Check("Echo's return type is the METHOD's generic parameter (ET_MVAR, not ET_VAR)",
              echo.ReturnType.IsGenericMethodParameter);
        MethodInfo first = typeof(IConverter).GetMethod("First");
        Check("First declares [\"K\", \"V\"] in order",
              first.GetGenericArguments().Select(a => a.Name).SequenceEqual(new[] { "K", "V" }));

        // The hardest loader-side validation: close the definition over int via reflection and
        // invoke it — the runtime type loader fully validates GenParamCount vs the owned
        // GenericParam rows and every MVAR index when instantiating.
        Check("MakeGenericMethod(typeof(int)).Invoke(c, {7}) == 7",
              (int)echo.MakeGenericMethod(typeof(int)).Invoke(c, new object[] { 7 }) == 7);

        // Mixed namespaces: a generic method ON a generic interface (`!0` and `!!0` in one
        // signature), through an interface-typed variable.
        IPicker<int> p = new IntPicker();
        Check("p.Pick(1, \"s\") == \"s\" (interface T + method U mixed)", p.Pick(1, "s") == "s");
        Check("type-generic-only member alongside: p.Base() == 5", p.Base() == 5);
        MethodInfo pick = typeof(IPicker<>).GetMethod("Pick");
        Check("Pick: param a is the INTERFACE's parameter, param b the METHOD's",
              pick.GetParameters()[0].ParameterType.IsGenericTypeParameter
              && pick.GetParameters()[1].ParameterType.IsGenericMethodParameter);

        // Rust-side surface probe (parity with the sibling cd_iface_* crates).
        Check("MainModule.genmethod_probe() == 11", MainModule.genmethod_probe() == 11);

        Console.WriteLine($"cd_iface_genmethod: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }
}
