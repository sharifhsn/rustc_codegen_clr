using System.Reflection;
using ExcelDna.Integration;

if (args.Length != 1)
    throw new ArgumentException("expected the backend-built managed Rust DLL path");

Assembly rust = Assembly.LoadFrom(Path.GetFullPath(args[0]));
MethodInfo method = rust.GetType("MainModule", throwOnError: true)!
    .GetMethod("RustEngineInfo", BindingFlags.Public | BindingFlags.Static)
    ?? throw new MissingMethodException("MainModule", "RustEngineInfo");
ExcelFunctionAttribute function = method.GetCustomAttribute<ExcelFunctionAttribute>()
    ?? throw new InvalidOperationException("RustEngineInfo lacks ExcelFunctionAttribute");
ExcelArgumentAttribute argument = method.GetParameters().Single()
    .GetCustomAttribute<ExcelArgumentAttribute>()
    ?? throw new InvalidOperationException("topic lacks ExcelArgumentAttribute");

if (function.Name != "RUST.ENGINE_INFO" ||
    function.Category != "Rust on .NET" ||
    function.IsThreadSafe != true ||
    argument.Name != "topic" ||
    argument.Description != "status, runtime, or profile")
{
    throw new InvalidOperationException("Excel-DNA metadata values do not match the scaffold contract");
}

Console.WriteLine("managed Rust ExcelFunction/ExcelArgument metadata OK");
