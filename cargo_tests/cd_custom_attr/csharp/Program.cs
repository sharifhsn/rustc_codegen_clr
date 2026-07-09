// C# reads back general ECMA-335 CustomAttribute rows DEFINED IN RUST via
// `#[dotnet_class(attr(...))]`.
//
// `cd_custom_attr.dll` is the .NET class library produced from the Rust crate `cd_custom_attr`,
// whose only content is four `#[dotnet_class(attr(...))]` structs. This program uses ordinary
// .NET reflection (`Type.GetCustomAttributes()`) to prove every attribute round-trips: the right
// TYPE, the right constructor arguments, and the right named property arguments.

using System;
using System.Linq;

public static class Program
{
    public static int Main()
    {
        int pass = 0, total = 0;

        // --- NoArgClass: a bare [Obsolete] (no-arg ctor shape). ---
        var noArgAttrs = typeof(NoArgClass).GetCustomAttributes(typeof(ObsoleteAttribute), false)
            .Cast<ObsoleteAttribute>().ToArray();
        Check("NoArgClass: exactly one ObsoleteAttribute", noArgAttrs.Length, 1, ref pass, ref total);
        if (noArgAttrs.Length == 1)
        {
            Check("NoArgClass: Message is null", noArgAttrs[0].Message, null, ref pass, ref total);
            Check("NoArgClass: IsError is false", noArgAttrs[0].IsError, false, ref pass, ref total);
        }

        // --- MessageClass: [Obsolete("...")] (single positional string ctor arg). ---
        var msgAttrs = typeof(MessageClass).GetCustomAttributes(typeof(ObsoleteAttribute), false)
            .Cast<ObsoleteAttribute>().ToArray();
        Check("MessageClass: exactly one ObsoleteAttribute", msgAttrs.Length, 1, ref pass, ref total);
        if (msgAttrs.Length == 1)
        {
            Check(
                "MessageClass: Message text",
                msgAttrs[0].Message,
                "This type is deprecated; use FullClass instead",
                ref pass, ref total);
            Check("MessageClass: IsError is false", msgAttrs[0].IsError, false, ref pass, ref total);
        }

        // --- FullClass: [Obsolete("...", true)] + named DiagnosticId/UrlFormat properties. ---
        // Looked up by NAME (not `typeof(FullClass)`) because IsError=true makes referencing the
        // type directly a genuine (unsuppressable) Roslyn ERROR — proof in itself that
        // DiagnosticId/IsError round-tripped correctly: `dotnet build` on this very file fails
        // with `error RCC0001` (our own custom DiagnosticId, not the default CS0619) unless this
        // indirection is used instead. See the `fullClassType != null` check below.
        Type fullClassType = typeof(NoArgClass).Assembly.GetType("FullClass");
        Check("FullClass: resolved by name", fullClassType != null, true, ref pass, ref total);
        var fullAttrs = fullClassType?.GetCustomAttributes(typeof(ObsoleteAttribute), false)
            .Cast<ObsoleteAttribute>().ToArray() ?? Array.Empty<ObsoleteAttribute>();
        Check("FullClass: exactly one ObsoleteAttribute", fullAttrs.Length, 1, ref pass, ref total);
        if (fullAttrs.Length == 1)
        {
            Check("FullClass: Message text", fullAttrs[0].Message, "Use the new API", ref pass, ref total);
            Check("FullClass: IsError is true", fullAttrs[0].IsError, true, ref pass, ref total);
            Check("FullClass: DiagnosticId (named prop arg)", fullAttrs[0].DiagnosticId, "RCC0001", ref pass, ref total);
            Check(
                "FullClass: UrlFormat (named prop arg)",
                fullAttrs[0].UrlFormat,
                "https://example.invalid/diagnostics/{0}",
                ref pass, ref total);
        }

        // --- MultiAttrClass: TWO distinct attribute instances accumulate (don't overwrite). ---
        var multiAttrs = typeof(MultiAttrClass).GetCustomAttributes(typeof(ObsoleteAttribute), false)
            .Cast<ObsoleteAttribute>().ToArray();
        Check("MultiAttrClass: exactly two ObsoleteAttributes", multiAttrs.Length, 2, ref pass, ref total);
        if (multiAttrs.Length == 2)
        {
            bool hasNoArg = multiAttrs.Any(a => a.Message == null);
            bool hasSecond = multiAttrs.Any(a => a.Message == "second attribute on the same type");
            Check("MultiAttrClass: has the no-arg instance", hasNoArg, true, ref pass, ref total);
            Check("MultiAttrClass: has the message instance", hasSecond, true, ref pass, ref total);
        }

        // --- CustomAttributeData: lower-level API, cross-check the ctor argument SHAPE directly
        // (types + count), independent of ObsoleteAttribute's own property getters. ---
        var cad = fullClassType == null
            ? null
            : System.Reflection.CustomAttributeData
                .GetCustomAttributes(fullClassType)
                .FirstOrDefault(d => d.AttributeType == typeof(ObsoleteAttribute));
        if (cad != null)
        {
            Check("FullClass CAD: 2 ctor args", cad.ConstructorArguments.Count, 2, ref pass, ref total);
            Check("FullClass CAD: 2 named args", cad.NamedArguments.Count, 2, ref pass, ref total);
        }
        else
        {
            Check("FullClass CAD: found", false, true, ref pass, ref total);
        }

        Console.WriteLine($"cd_custom_attr: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok)
            pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
