// Investigation: can a Rust-defined type/function participate in ASP.NET Core hosting?
//
// This is a REAL minimal-API ASP.NET Core host (`WebApplication.CreateBuilder`, `app.MapGet`,
// `app.Run()`) whose HTTP handler bodies call into Rust-defined logic:
//   * `MainModule.add`/`MainModule.greet` — plain `#[dotnet_export]` functions.
//   * `Calculator` — a `#[dotnet_class]` managed type constructed and driven from a handler body.
//
// Tier 2 probe: `/mul2` attempts to pass a Rust-defined STATIC method (`Calculator.multiply`)
// directly as the route handler delegate (method-group conversion), with no C# lambda wrapper —
// exactly like `app.MapGet("/foo", SomeCSharpClass.SomeMethod)` would work for an ordinary
// C#-defined static method.

var builder = WebApplication.CreateBuilder(args);
var app = builder.Build();

// --- Tier 1: handler bodies call Rust logic -------------------------------------------------

app.MapGet("/add/{a:int}/{b:int}", (int a, int b) =>
{
    int result = MainModule.add(a, b);
    return Results.Ok(new { a, b, result, computedBy = "rust:#[dotnet_export] add" });
});

app.MapGet("/greet/{name}", (string name) =>
{
    string msg = MainModule.greet(name);
    return Results.Text(msg);
});

app.MapGet("/calc/{start:int}/add/{n:int}", (int start, int n) =>
{
    // Construct a Rust-defined managed class instance and call an instance method on it,
    // from inside a real ASP.NET Core minimal-API handler.
    Calculator calc = new Calculator(start);
    int total = calc.add_to(n);
    return Results.Ok(new { start, n, total, computedBy = "rust:#[dotnet_class] Calculator.add_to" });
});

// --- Tier 2 (was BLOCKED, now FIXED — see docs/RUST_PARITY_ROADMAP.md Tier 0 item 4): ----------
// `app.MapGet("/mul2/{a:int}/{b:int}", Calculator.multiply)` passes a Rust-defined STATIC method
// directly as the route handler delegate (method-group conversion, no C# lambda wrapper). This
// used to throw `System.ArgumentException: An item with the same key has already been added.
// Key: ` at first-request time — ASP.NET's `RequestDelegateFactory` binds route parameters by
// `ParameterInfo.Name`, and every Rust-exported method's parameters reflected with `Name == ""`,
// so >=2 params collided. Root-caused to `src/comptime.rs`'s `finish_type`, which hardcoded
// `vec![None; ...]` for parameter names on every comptime-synthesized class-method alias instead
// of reading the aliased Rust fn's own MIR debug info (the plain `#[dotnet_export]`/`add_fn` path
// was already correct — only the `#[dotnet_class]`/`#[dotnet_methods]`/`#[dotnet_interface]`
// aliasing path had the bug). Fixed via `src/assembly.rs::carrier_arg_names`.
app.MapGet("/mul2/{a:int}/{b:int}", Calculator.multiply);

app.Run("http://127.0.0.1:5289");
