using System;
using System.Reflection;

// A C# class implementing the Rust-defined `IVolume` interface with AUTO-PROPERTIES. This only
// COMPILES if `Volume`/`Name` are genuine .NET properties on the interface (Property rows +
// MethodSemantics binding the abstract get_/set_ accessors) — with plain abstract methods csc
// would demand explicit `get_Volume`/`set_Volume`/`get_Name` implementations (CS0535) and reject
// `int Volume { get; set; }` as an implementation. So the compile itself is the zeroth check.
class Speaker : IVolume
{
    public int Id() => 7;
    public int Volume { get; set; } = 11;
    public string Name => "a speaker";
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
        var speaker = new Speaker();
        IVolume v = speaker;

        // Read THROUGH THE INTERFACE reference (virtual dispatch to the auto-property).
        Check("Volume read through interface == 11", v.Volume == 11);

        // Write THROUGH THE INTERFACE reference, observe on the concrete instance.
        v.Volume = 42;
        Check("Volume write through interface observed on instance", speaker.Volume == 42);
        Check("Volume re-read through interface == 42", v.Volume == 42);

        // The get-only property has no setter — enforced at compile time by the absence of a
        // `set_Name` MethodSemantics row (nothing to runtime-check beyond it existing and
        // reading correctly).
        Check("Name (get-only) through interface", v.Name == "a speaker");

        // Reflection: the interface declares real PropertyInfo rows with the right value type
        // and Can{Read,Write} flags, and the accessors are genuine abstract virtual members.
        PropertyInfo volProp = typeof(IVolume).GetProperty("Volume");
        Check("typeof(IVolume).GetProperty(\"Volume\") != null", volProp != null);
        Check("Volume.PropertyType == typeof(int)", volProp != null && volProp.PropertyType == typeof(int));
        Check("Volume.CanRead && Volume.CanWrite", volProp != null && volProp.CanRead && volProp.CanWrite);

        PropertyInfo nameProp = typeof(IVolume).GetProperty("Name");
        Check("typeof(IVolume).GetProperty(\"Name\") != null", nameProp != null);
        Check("Name.CanRead && !Name.CanWrite (get-only)", nameProp != null && nameProp.CanRead && !nameProp.CanWrite);

        MethodInfo getVol = typeof(IVolume).GetMethod("get_Volume");
        Check("get_Volume accessor is abstract && virtual",
              getVol != null && getVol.IsAbstract && getVol.IsVirtual);
        MethodInfo setVol = typeof(IVolume).GetMethod("set_Volume");
        Check("set_Volume accessor is abstract && virtual",
              setVol != null && setVol.IsAbstract && setVol.IsVirtual);

        // The plain abstract member coexists with the properties on the same interface.
        Check("Id() == 7 through the interface", v.Id() == 7);

        Console.WriteLine($"cd_iface_prop: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }
}
