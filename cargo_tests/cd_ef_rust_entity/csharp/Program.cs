// Regression proof: can EF Core's model builder reflect over a MANAGED CLASS DEFINED IN RUST
// (`Widget`, from `#[dotnet_class]` in ../rustlib/src/lib.rs) and treat it as a real entity?
//
// `Widget` has an `int id` field and a `MString name` field, exposed via `#[dotnet_class]`'s
// `properties = true`: real `Id`/`Name` `.NET` properties (backed by `get_Id`/`set_Id`/`get_Name`/
// `set_Name` accessors, `SpecialName` + linked into a genuine §II.22.34 `Property` row), a
// field-initializing primary ctor `Widget(int, string)`, AND a parameterless ctor `Widget()` (from
// `default_ctor = true`). Before the fix, `#[dotnet_class]`'s only field-accessor option was
// `field_setters = true`, which emits plain `MethodDef`s (`read_id`/`set_id`, no `PropertyDef` row)
// — invisible to `Type.GetProperties()`, which is what EF Core's default entity-type discovery
// convention scans, so `GetProperties()` returned zero results and EF refused to treat `Widget` as
// a valid entity. This program drives the real EF Core model builder against a real Sqlite provider
// and reports exactly what happens: whether the C# compiles against `Widget` (DbSet<Widget> needs
// no special shape at compile time), what `context.Model.FindEntityType(typeof(Widget))` reports
// (null vs an entity type with N properties), and what exception (if any) EnsureCreated()/
// materialization throws.

using System;
using System.Linq;
using System.Reflection;
using Microsoft.Data.Sqlite;
using Microsoft.EntityFrameworkCore;

public class WidgetDbContext : DbContext
{
    public DbSet<Widget> Widgets => Set<Widget>();

    private readonly SqliteConnection _connection;

    public WidgetDbContext(SqliteConnection connection)
    {
        _connection = connection;
    }

    protected override void OnConfiguring(DbContextOptionsBuilder options)
    {
        options.UseSqlite(_connection);
    }
}

public static class Program
{
    public static int Main()
    {
        Console.WriteLine("=== cd_ef_rust_entity: can a Rust-defined #[dotnet_class] serve as a real EF Core entity? ===");

        // ---- Step 0: raw reflection over Widget, independent of EF — ground truth for what
        // Type.GetProperties()/GetFields() actually see on the Rust-emitted type. ----
        Type widgetType = typeof(Widget);
        Console.WriteLine($"\n[Step 0] typeof(Widget) = {widgetType.FullName}, assembly = {widgetType.Assembly.GetName().Name}");

        var props = widgetType.GetProperties(BindingFlags.Public | BindingFlags.Instance);
        Console.WriteLine($"[Step 0] Type.GetProperties() (public instance) => {props.Length} propert{(props.Length == 1 ? "y" : "ies")}");
        foreach (var p in props)
        {
            Console.WriteLine($"           - {p.PropertyType} {p.Name} (CanRead={p.CanRead}, CanWrite={p.CanWrite})");
        }

        var methods = widgetType.GetMethods(BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly);
        Console.WriteLine($"[Step 0] Type.GetMethods() (public instance, declared-only) => {methods.Length} method(s):");
        foreach (var m in methods)
        {
            Console.WriteLine($"           - {m.Name}({string.Join(", ", m.GetParameters().Select(p => p.ParameterType.Name))}) : {m.ReturnType}");
        }

        var ctors = widgetType.GetConstructors();
        Console.WriteLine($"[Step 0] Type.GetConstructors() => {ctors.Length}:");
        foreach (var c in ctors)
        {
            Console.WriteLine($"           - .ctor({string.Join(", ", c.GetParameters().Select(p => p.ParameterType.Name))})");
        }

        var connection = new SqliteConnection("Data Source=file:cd_ef_rust_entity_mem?mode=memory&cache=shared");
        connection.Open();

        int exitCode = 0;

        // ---- Step 1: can EF's model builder even build a model that includes Widget? ----
        Console.WriteLine("\n[Step 1] Building WidgetDbContext and inspecting context.Model ...");
        try
        {
            using var ctx = new WidgetDbContext(connection);
            var entityType = ctx.Model.FindEntityType(typeof(Widget));
            if (entityType == null)
            {
                Console.WriteLine("[Step 1] RESULT: context.Model.FindEntityType(typeof(Widget)) == null");
                Console.WriteLine("           -> EF's model builder did NOT register Widget as an entity type at all");
                Console.WriteLine("           -> (DbSet<Widget> exposed a property EF silently ignored, OR model build deferred the error)");
            }
            else
            {
                var efProps = entityType.GetProperties().ToList();
                Console.WriteLine($"[Step 1] RESULT: Widget IS a registered entity type with {efProps.Count} mapped propert{(efProps.Count == 1 ? "y" : "ies")}:");
                foreach (var p in efProps)
                {
                    Console.WriteLine($"           - {p.Name} : {p.ClrType}");
                }
                var key = entityType.FindPrimaryKey();
                Console.WriteLine($"[Step 1] Primary key: {(key == null ? "NONE" : string.Join(",", key.Properties.Select(p => p.Name)))}");
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Step 1] EXCEPTION building/inspecting the model: {ex.GetType().FullName}");
            Console.WriteLine($"           message: {ex.Message}");
            exitCode = 1;
        }

        // ---- Step 2: try the real end-to-end path — EnsureCreated() (schema generation) then a
        // round-trip insert + query. Expected to fail somewhere if Step 1 already found 0 properties
        // (no columns => EF should throw about a missing key before ever touching Sqlite), but drive
        // it anyway to see the EXACT failure point / exception type, not just predict it. ----
        Console.WriteLine("\n[Step 2] Attempting EnsureCreated() + insert + query round-trip ...");
        try
        {
            using var ctx = new WidgetDbContext(connection);
            bool created = ctx.Database.EnsureCreated();
            Console.WriteLine($"[Step 2] EnsureCreated() returned {created} (schema created)");

            var w = new Widget(7, "hello-from-csharp");
            ctx.Widgets.Add(w);
            int saved = ctx.SaveChanges();
            Console.WriteLine($"[Step 2] SaveChanges() persisted {saved} row(s)");

            var back = ctx.Widgets.First();
            Console.WriteLine($"[Step 2] Round-trip read: Id={back.Id}, Name={back.Name}");
            Console.WriteLine("[Step 2] RESULT: full EF Core round-trip over a Rust-defined entity SUCCEEDED");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Step 2] EXCEPTION: {ex.GetType().FullName}");
            Console.WriteLine($"           message: {ex.Message}");
            if (ex.InnerException != null)
            {
                Console.WriteLine($"           inner: {ex.InnerException.GetType().FullName}: {ex.InnerException.Message}");
            }
            exitCode = 1;
        }

        Console.WriteLine($"\n=== cd_ef_rust_entity: done, exitCode={exitCode} ===");
        return exitCode;
    }
}
