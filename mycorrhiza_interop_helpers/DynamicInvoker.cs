// Raw dynamic-reflection invoke helper, backing `mycorrhiza::dynamic::invoke_dynamic`.
//
// Everything `mycorrhiza`/`spinacz` (`add-nuget`, the BCL bindings) normally does is STATIC binding:
// `spinacz` reflects a target assembly at BUILD time and `dotnet_macros`/`intrinsics.rs` emit a
// methodref for an exact, compile-time-known `(assembly, type, method)` triple — a real CIL `call`.
// That covers everything known at compile time, but not late-bound scenarios where the method to
// call is only known at RUNTIME (a plugin system, a method chosen by a config string, a truly dynamic
// API surface). This class is that escape hatch: it takes the whole `(assembly, type, method, args)`
// tuple as ordinary runtime VALUES (strings + a boxed `object?[]`) and resolves/dispatches the call
// itself, using `System.Reflection` (`Assembly.Load` / `Type.GetType` / `Type.GetMethod` /
// `MethodInfo.Invoke`) rather than a CIL `call`/`callvirt` emitted at Rust-compile time.
//
// This is intentionally invoked from Rust as a perfectly ordinary FIXED static method call (the
// `(assembly="Mycorrhiza.Interop.Helpers", type="Mycorrhiza.Reflection.DynamicInvoker",
// method="InvokeStatic")` triple IS known at Rust-compile time, via the normal
// `RustcCLRInteropManagedClass::static4` interop primitive) — the "dynamic" part is entirely inside
// this method's own body, not in how Rust reaches it.
namespace Mycorrhiza.Reflection;

using System;
using System.Reflection;

public static class DynamicInvoker
{
    /// Resolve `typeName` inside the assembly named `assemblyName` (loaded via `Assembly.Load`), find
    /// a public static method called `methodName` whose parameter types exactly match the runtime
    /// types of `args` (boxed values supply their own type; `null` matches `object`), invoke it, and
    /// return the (possibly boxed) result — `null` for a `void` method.
    ///
    /// Every failure mode here (assembly not found, type not found, no matching overload, the
    /// resolved overload throwing) surfaces as an ordinary .NET exception out of this method — the
    /// same as any other managed call. It is not a source of memory-unsafety; it is the normal
    /// "wrong string, wrong shape" failure mode of any reflection API, in any language.
    public static object? InvokeStatic(string assemblyName, string typeName, string methodName, object?[] args)
    {
        var assembly = Assembly.Load(assemblyName);
        var type = assembly.GetType(typeName, throwOnError: true)!;

        var argTypes = new Type[args.Length];
        for (var i = 0; i < args.Length; i++)
        {
            argTypes[i] = args[i]?.GetType() ?? typeof(object);
        }

        var method = type.GetMethod(
            methodName,
            BindingFlags.Public | BindingFlags.Static | BindingFlags.FlattenHierarchy,
            binder: null,
            types: argTypes,
            modifiers: null);
        if (method is null)
        {
            throw new MissingMethodException(
                $"{assemblyName}!{typeName}.{methodName}({string.Join(", ", Array.ConvertAll(argTypes, t => t.Name))})");
        }
        return method.Invoke(null, args);
    }
}
