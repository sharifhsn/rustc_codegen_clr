// C# consumes Rust functions exported ergonomically via `#[dotnet_export]`.
//
// `cd_export.dll` is the .NET class library produced from the Rust crate `cd_export`, whose functions
// carry `#[dotnet_export]`. The macro generated the seam shims; C# calls them as ordinary typed static
// methods on `MainModule` — string parameters/returns are real managed `System.String`, so there is
// NO `(ptr, len)` marshalling here at all (contrast cd_interop/Program.cs, which pins buffers by hand).
//
// Two further return-type arms (docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md Tier C §6):
//   * Case A: `Task`/`Task<T>` returns — `rustlib/src/lib.rs`'s `delayed_ping()`/`compute_answer()`
//     build and emit the correct CIL (`System.Threading.Tasks.Task`/`Task<int>`, verified via
//     monodis/ilspycmd against the produced cd_export.dll), but the C# SIDE below is left
//     UNEXERCISED for now: a separate, pre-existing gap in `cilly/src/ir/il_exporter/mod.rs`'s
//     `ref_assembly_name_for_type` (the CS0012 ref-vs-impl-assembly table) doesn't cover
//     `System.Threading.Tasks.Task`/`Task<T>` the way it already does for `System.Threading`'s
//     `SemaphoreSlim`/etc., so a separately-compiled C# project sees `CS0012` on these two exports.
//     Fixing that table is a `cilly/src` change outside this macro work's scope — left as a follow-up
//     rather than forced. See the task's final report for detail; do not re-enable an `await` call on
//     `delayed_ping`/`compute_answer` here until that lands (it will make this project fail to build).
//   * Case B: `Vec<T>` -> `RustVec<T>` returns — `range()`/`squares()` below, consumed via
//     `foreach`/LINQ `.Sum()` exactly like the hand-built `cd_rustvec` wrapper. Fully verified.

using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;

/// A minimal local `RustVec<T>` wrapper — same shape as `cd_rustvec`'s and
/// `RustDotnet.Containers.cs`'s `RustVec<T>`, kept inline here so this probe stays dependency-free
/// (no `UseRustDotnetContainers`). Backs the `#[dotnet_export]` `Vec<T>` -> seam `usize` handle arm:
/// the Rust shim already built the `RustVec` core (via `rcl_vec_new`/`rcl_vec_push`) and handed back
/// its opaque handle, so C# only needs to wrap that handle — never call `rcl_vec_new` itself for
/// these two functions.
public unsafe struct RustVec<T> : IDisposable, IEnumerable<T> where T : unmanaged
{
    private nuint _handle;

    public static RustVec<T> FromHandle(nuint handle) => new RustVec<T> { _handle = handle };

    public int Count => (int)MainModule.rcl_vec_len(_handle);

    public T Get(int idx)
    {
        T v = default;
        if (!MainModule.rcl_vec_get(_handle, (nuint)idx, (byte*)&v))
            throw new IndexOutOfRangeException();
        return v;
    }

    public void Dispose()
    {
        if (_handle != 0)
        {
            MainModule.rcl_vec_free(_handle);
            _handle = 0;
        }
    }

    public Enumerator GetEnumerator() => new Enumerator(this);
    IEnumerator<T> IEnumerable<T>.GetEnumerator() => GetEnumerator();
    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

    public struct Enumerator : IEnumerator<T>
    {
        private RustVec<T> _vec;
        private int _index;
        private T _current;

        internal Enumerator(RustVec<T> vec)
        {
            _vec = vec;
            _index = -1;
            _current = default;
        }

        public T Current => _current;
        object IEnumerator.Current => _current;

        public bool MoveNext()
        {
            int next = _index + 1;
            if (next >= _vec.Count)
                return false;
            _current = _vec.Get(next);
            _index = next;
            return true;
        }

        public void Reset()
        {
            _index = -1;
            _current = default;
        }

        public void Dispose() { }
    }
}

public static class Program
{
    public static int Main()
    {
        return MainAsync().GetAwaiter().GetResult();
    }

    private static async Task<int> MainAsync()
    {
        int pass = 0, total = 0;

        // string greet(string) — inbound &str, outbound String, both as managed string.
        Check("greet(\"World\")", MainModule.greet("World"), "Hello, World, from Rust!", ref pass, ref total);
        Check("greet(\"\")", MainModule.greet(""), "Hello, , from Rust!", ref pass, ref total);

        // Primitive passthrough.
        Check("add(2,3)", MainModule.add(2, 3), 5, ref pass, ref total);
        Check("add(-4,10)", MainModule.add(-4, 10), 6, ref pass, ref total);

        // bool return.
        Check("is_even(4)", MainModule.is_even(4L), true, ref pass, ref total);
        Check("is_even(7)", MainModule.is_even(7L), false, ref pass, ref total);

        // mixed float/int.
        Check("scale(2.5, 4)", MainModule.scale(2.5, 4), 10.0, ref pass, ref total);

        // String (owned) inbound, String outbound.
        Check("shout(\"hi\")", MainModule.shout("hi"), "HI!", ref pass, ref total);

        // &str inbound, primitive return — proves the string content crossed intact.
        Check("str_len(\"héllo\")", MainModule.str_len("héllo"), 6, ref pass, ref total); // é is 2 UTF-8 bytes

        // no params, &'static str return.
        Check("version()", MainModule.version(), "cd_export 1.0", ref pass, ref total);

        // void export — just prove it links and is callable.
        MainModule.ping();
        Check("ping() callable", true, true, ref pass, ref total);

        // Panic-safety: a #[dotnet_export] fn that panics must surface as a genuine, catchable
        // managed exception (NOT a process abort via Environment.FailFast), carrying the panic
        // message, and the process must remain healthy afterward (a following call still works).
        {
            total++;
            bool caught = false;
            string message = null;
            try
            {
                MainModule.boom("division safety check failed");
            }
            catch (Exception e)
            {
                caught = true;
                message = e.Message;
            }
            bool ok = caught && message != null && message.Contains("boom: division safety check failed");
            if (ok) pass++;
            Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] boom(...) panics as catchable exception: caught={caught}, message=\"{message}\"");
        }

        // Process-health check: a normal call after the panic must still succeed (proves the panic
        // didn't corrupt or abort the runtime — it took an ordinary managed-exception control path).
        Check("add(2,3) after boom() panic", MainModule.add(2, 3), 5, ref pass, ref total);

        // ---- Case A: Task/Task<T> returns — NOT YET EXERCISED here, see the file-header comment ---
        //
        // `MainModule.delayed_ping()` / `MainModule.compute_answer()` exist and build correctly on
        // the Rust side (confirmed via metadata inspection: the shims' declared return types really
        // are `System.Threading.Tasks.Task` / `Task<int>`), but calling them from this separately-
        // compiled C# project currently hits CS0012 — a pre-existing gap in the exporter's ref-vs-
        // impl-assembly table (`cilly/src/ir/il_exporter/mod.rs`'s `ref_assembly_name_for_type`,
        // which already covers `System.Threading`'s `SemaphoreSlim`/etc. but not `System.Threading.
        // Tasks.Task`/`Task<T>`). That's a `cilly/src` fix, out of scope for this macro-only change —
        // left as a documented follow-up rather than forced. Uncomment once it lands:
        //
        // await MainModule.delayed_ping();
        // int got = await MainModule.compute_answer();
        // Check("await compute_answer()", got, 42, ref pass, ref total);
        await Task.CompletedTask; // keep `await` reachable so MainAsync stays a real async method.

        // ---- Case B: Vec<T> -> RustVec<T> returns — foreach/LINQ over an exported Rust Vec<T> -----

        // RustVec<int> range(int, int).
        using (var r = RustVec<int>.FromHandle(MainModule.range(1, 6)))
        {
            Check("range(1,6) Count", r.Count, 5, ref pass, ref total);
            Check("range(1,6) values", string.Join(",", r), "1,2,3,4,5", ref pass, ref total);
            Check("range(1,6) LINQ Sum", r.Sum(), 15, ref pass, ref total);
        }

        // RustVec<long> squares(int) — a second element type (i64), proving the arm generalizes.
        using (var s = RustVec<long>.FromHandle(MainModule.squares(5)))
        {
            Check("squares(5) Count", s.Count, 5, ref pass, ref total);
            Check("squares(5) values", string.Join(",", s), "0,1,4,9,16", ref pass, ref total);
            Check("squares(5) LINQ Sum", s.Sum(), 30L, ref pass, ref total);
        }

        Console.WriteLine($"cd_export: {pass}/{total} checks passed");
        return pass == total ? 0 : 1;
    }

    private static void Check<T>(string name, T got, T want, ref int pass, ref int total)
    {
        total++;
        bool ok = Equals(got, want);
        if (ok) pass++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}: got {got}, want {want}");
    }
}
