using System;
using System.Linq;
using System.Reflection;

public static class ManagedProbe
{
    public static int Main()
    {
        var assembly = Assembly.LoadFrom("Rust.Unity.Sample.dll");
        var method = assembly.GetTypes()
            .SelectMany(type => type.GetMethods(BindingFlags.Public | BindingFlags.Static))
            .Single(candidate => candidate.Name == "sample_value");
        var value = (int)method.Invoke(null, null);
        Console.WriteLine("UNITY_MONO_MANAGED_RUST=" + value);
        return value == 42 ? 0 : 1;
    }
}
