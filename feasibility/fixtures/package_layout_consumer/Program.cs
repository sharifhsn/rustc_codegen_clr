using System.Reflection;

if (args.Length != 2)
{
    Console.Error.WriteLine("usage: PackageLayoutConsumer <assembly> <static-method>");
    return 2;
}

Assembly assembly = Assembly.Load(args[0]);
Type module = assembly.GetType("MainModule", throwOnError: true)!;
MethodInfo method = module.GetMethod(args[1], BindingFlags.Public | BindingFlags.Static)
    ?? throw new MissingMethodException(module.FullName, args[1]);
object? result = method.Invoke(null, null);
Console.WriteLine($"{args[0]}.{args[1]}={result}");
return result is int value && value == 42 ? 0 : 1;
