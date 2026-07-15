// C# consumes Rust functions exported ergonomically via `#[dotnet_export]`.
//
// `cd_export.dll` is the .NET class library produced from the Rust crate `cd_export`, whose functions
// carry `#[dotnet_export]`. The macro generated the seam shims; C# calls them as ordinary typed static
// methods on `MainModule` — string parameters/returns are real managed `System.String`, so there is
// NO `(ptr, len)` marshalling here at all (contrast cd_interop/Program.cs, which pins buffers by hand).
//
// Two further return-type arms (docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md Tier C §6):
//   * Case A: `Task`/`Task<T>` returns — `rustlib/src/lib.rs`'s `delayed_ping()`/`compute_answer()`
//     build and emit the correct CIL (`System.Threading.Tasks.Task`/`Task<int>`). Consuming them
//     from this separately-compiled C# project used to hit CS0012 (a pre-existing gap in the
//     exporters' `ref_assembly_name_for_type` ref-vs-impl-assembly table, which covered
//     `System.Threading`'s `SemaphoreSlim`/etc. but not `System.Threading.Tasks.Task`/`Task<T>`) —
//     fixed in both `il_exporter` and `pe_exporter` (a `System.Threading.Tasks.Task` ->
//     `System.Threading.Tasks` entry, confirmed against the real net8.0 ref-pack DLL), so both
//     exports are now `await`ed directly below like any other async C# call.
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

        Check("checked_answer(true)", MainModule.checked_answer(true), 42, ref pass, ref total);
        {
            total++;
            try
            {
                MainModule.checked_answer(false);
                Console.WriteLine("  [FAIL] checked_answer(false) throws");
            }
            catch (Exception e)
            {
                bool ok = e.Message.Contains("checked answer failed");
                if (ok) pass++;
                Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] checked_answer(false) throws: {e.Message}");
            }
        }
        // ---- Case A: Task/Task<T> returns --------------------------------------------------------
        //
        // The CS0012 gap noted above is now fixed: `ref_assembly_name_for_type` (both exporters)
        // has a `System.Threading.Tasks.Task` -> `System.Threading.Tasks` entry alongside the
        // existing `System.Threading` ones, confirmed against the real net8.0 ref-pack DLL.
        await MainModule.delayed_ping();
        int got = await MainModule.compute_answer();
        Check("await compute_answer()", got, 42, ref pass, ref total);

        // ---- Case B: Vec<T> -> normal T[]; RustOwnedVec<T> -> explicit RustVec<T> -----------------
        int[] r = MainModule.range(1, 6);
        Check("range(1,6) Length", r.Length, 5, ref pass, ref total);
        Check("range(1,6) values", string.Join(",", r), "1,2,3,4,5", ref pass, ref total);
        Check("range(1,6) LINQ Sum", r.Sum(), 15, ref pass, ref total);

        long[] s = MainModule.squares(5);
        Check("squares(5) Length", s.Length, 5, ref pass, ref total);
        Check("squares(5) values", string.Join(",", s), "0,1,4,9,16", ref pass, ref total);
        Check("squares(5) LINQ Sum", s.Sum(), 30L, ref pass, ref total);

        using (var owned = RustVec<int>.FromHandle(MainModule.rust_owned_range(4)))
        {
            Check("rust_owned_range Count", owned.Count, 4, ref pass, ref total);
            Check("rust_owned_range values", string.Join(",", owned), "0,1,2,3", ref pass, ref total);
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
