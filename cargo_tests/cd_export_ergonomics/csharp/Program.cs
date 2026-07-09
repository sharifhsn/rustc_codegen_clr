using System;
using System.Linq;

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

// #[dotnet_export] Vec<T> param now marshals inbound too (via a RustVec<int> handle) — built here
// with the raw rcl_vec_* exports directly (same handle shape RustDotnet.RustVec<T> wraps).
unsafe
{
    nuint handle = MainModule.rcl_vec_new((nuint)sizeof(int));
    foreach (int x in new[] { 1, 2, 3, 4 })
    {
        MainModule.rcl_vec_push(handle, (byte*)&x);
    }
    Check("sum_vec([1,2,3,4])", MainModule.sum_vec(handle), 10);
    MainModule.rcl_vec_free(handle);
}

// #[dotnet_methods] instance method taking &str/String directly (no manual MString).
Greeting g = Greeting.make(2);
Check("g.greet(\"World\")", g.greet("World"), "Hi, World! Hi, World! ");

// #[dotnet_methods] instance method taking Option<i32> directly.
Check("g.add_bonus(5)", g.add_bonus(5), 7);
Check("g.add_bonus(null)", g.add_bonus(null), 2);

Console.WriteLine(pass == total ? "PASS" : $"FAIL ({pass}/{total})");
return pass == total ? 0 : 1;
