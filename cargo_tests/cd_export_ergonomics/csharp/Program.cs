using System;
using System.IO;
using System.Linq;
using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;
using System.Reflection.PortableExecutable;
using System.Text;
using System.Text.Json;

int pass = 0, total = 0;
void Check(string label, object? got, object? expected)
{
    total++;
    bool ok = Equals(got, expected);
    Console.WriteLine("  " + label + " = " + (got?.ToString() ?? "null") + (ok ? " (ok)" : $" (FAIL, want {expected})"));
    if (ok) pass++;
}

// #[dotnet_export] Option<T> param now marshals inbound too.
Check("double_if_present(5)", MainModule.double_if_present(5), 10);
Check("double_if_present(null)", MainModule.double_if_present(null), null);

// #[dotnet_enum]: genuine enum metadata, ordinary C# enum syntax, and typed export seams.
Check("Status is enum", typeof(Status).IsEnum, true);
Check("Status underlying type", Enum.GetUnderlyingType(typeof(Status)), typeof(int));
Check("Status names", string.Join(",", Enum.GetNames<Status>()), "Pending,Ready,Done");
Check("Status.Ready raw value", (int)Status.Ready, 4);
Check("roundtrip_status", MainModule.roundtrip_status(Status.Ready), Status.Ready);
Check("is_terminal", MainModule.is_terminal(Status.Done), true);
Check("enum switch", Status.Done switch { Status.Pending => 0, Status.Ready => 1, Status.Done => 2, _ => -1 }, 2);
Check("typed enum parameter", typeof(MainModule).GetMethod("roundtrip_status")!.GetParameters()[0].ParameterType, typeof(Status));

// #[dotnet_export] Vec<T> is an ordinary managed T[] by default.
Check("sum_vec([1,2,3,4])", MainModule.sum_vec(new[] { 1, 2, 3, 4 }), 10);

// C# -> Rust delegate import. Rust receives the real managed delegate handle, wraps it as the
// matching mycorrhiza delegate, and invokes it through Delegate.Invoke.
int action1Total = 0;
Check("invoke_action1 return", MainModule.invoke_action1(x => action1Total += x, 7), 7);
Check("invoke_action1 callback", action1Total, 7);

int action2Total = 0;
Check("invoke_action2 return", MainModule.invoke_action2((a, b) => action2Total = a * b, 6, 5), 11);
Check("invoke_action2 callback", action2Total, 30);
Check("invoke_func1", MainModule.invoke_func1(x => x * 3, 7), 21);
Check("invoke_func2", MainModule.invoke_func2((a, b) => a * 10 + b, 4, 2), 42);
int action3Total = 0;
Check("invoke_action3 return", MainModule.invoke_action3((a, b, c) => action3Total = a * b * c, 2, 3, 4), 9);
Check("invoke_action3 callback", action3Total, 24);
Check("invoke_func3", MainModule.invoke_func3((a, b, c) => a * 100 + b * 10 + c, 4, 2, 7), 427);
string callbackText = "héllo 🦀";
Check("invoke_string_func", MainModule.invoke_string_func(s => s.Length, callbackText), callbackText.Length);
Check("invoke_comparison", MainModule.invoke_comparison((a, b) => a.CompareTo(b), 3, 9), -1);

// #[dotnet_methods] instance method taking &str/String directly (no manual MString).
Greeting g = Greeting.make(2);
Check("g.greet(\"World\")", g.greet("World"), "Hi, World! Hi, World! ");

// #[dotnet_methods] instance method taking Option<i32> directly.
Check("g.add_bonus(5)", g.add_bonus(5), 7);
Check("g.add_bonus(null)", g.add_bonus(null), 2);
Check("g.apply(Func<int,int>)", g.apply(x => x + 1, 39), 42);

// C# -> Rust debugger/source proof. The call enters an exported Rust method, captures a managed
// stack there, and relies on the adjacent Portable PDB to resolve the Rust frame to lib.rs:line N.
string rustTrace = MainModule.rust_pdb_stack_trace();
Console.WriteLine("  Rust managed trace:\n" + rustTrace);
Check("Rust PDB trace names lib.rs", rustTrace.Contains("lib.rs"), true);
Check("Rust PDB trace has file:line", rustTrace.Contains(".rs:line"), true);
Check("Rust PDB trace names leaf", rustTrace.Contains("rust_pdb_leaf"), true);

string rustAssembly = typeof(MainModule).Assembly.Location;
string rustPdb = Path.ChangeExtension(rustAssembly, ".pdb");
Check("Rust sidecar PDB exists", File.Exists(rustPdb), true);
using (FileStream peStream = File.OpenRead(rustAssembly))
using (PEReader peReader = new PEReader(peStream))
{
    foreach (DebugDirectoryEntry entry in peReader.ReadDebugDirectory())
    {
        if (entry.Type == DebugDirectoryEntryType.CodeView)
        {
            Console.WriteLine("  Rust PE CodeView path: " + peReader.ReadCodeViewDebugDirectoryData(entry).Path);
        }
    }
}
using (FileStream pdbStream = File.OpenRead(rustPdb))
using (MetadataReaderProvider provider = MetadataReaderProvider.FromPortablePdbStream(pdbStream))
{
    MetadataReader reader = provider.GetMetadataReader();
    Check("Rust Portable PDB has documents", reader.Documents.Count > 0, true);
    string[] documentNames = reader.Documents
        .Select(document => reader.GetString(reader.GetDocument(document).Name))
        .ToArray();
    Check("Rust PDB uses logical consumer path", documentNames.Contains("/_/consumer/src/lib.rs"), true);
    string checkoutPath = Environment.GetEnvironmentVariable("RustCheckoutPath") ?? "<missing>";
    Check("Rust PDB hides checkout path", documentNames.All(name => !name.StartsWith(checkoutPath)), true);
    Check("Rust Portable PDB has local scopes", reader.LocalScopes.Count > 0, true);
    string[] localNames = reader.LocalScopes
        .SelectMany(scope => reader.GetLocalScope(scope).GetLocalVariables())
        .Select(variable => reader.GetString(reader.GetLocalVariable(variable).Name))
        .ToArray();
    Console.WriteLine("  Rust Portable PDB named locals: " + localNames.Length);
    if (Environment.GetEnvironmentVariable("RustProfile") == "debug")
    {
        Check("Rust Portable PDB names debugger local", localNames.Contains("debugger_probe_local"), true);
    }
    else
    {
        Check("Rust release PDB retains named locals", localNames.Length > 0, true);
    }

    const string sourceLinkKind = "cc110556-a091-4d38-9fec-25ab9a351a6a";
    string? sourceLinkJson = reader
        .GetCustomDebugInformation(MetadataTokens.EntityHandle(0x00000001))
        .Select(handle => reader.GetCustomDebugInformation(handle))
        .Where(info => reader.GetGuid(info.Kind).ToString() == sourceLinkKind)
        .Select(info => Encoding.UTF8.GetString(reader.GetBlobBytes(info.Value)))
        .SingleOrDefault();
    Check("Rust Portable PDB has Source Link", sourceLinkJson is not null, true);
    string? sourceLinkUrl = sourceLinkJson is null
        ? null
        : JsonDocument.Parse(sourceLinkJson).RootElement
            .GetProperty("documents")
            .GetProperty("/_/consumer/*")
            .GetString();
    Check(
        "Rust Source Link maps logical consumer documents",
        sourceLinkUrl,
        "https://example.invalid/rust-dotnet-fixture/*"
    );
}

if (pass == 37 && total == 37)
{
    Console.WriteLine("PASS");
    Console.WriteLine("== cd_export_ergonomics done ==");
    return 0;
}

Console.WriteLine($"FAIL ({pass}/{total}, expected 37/37)");
return 1;
