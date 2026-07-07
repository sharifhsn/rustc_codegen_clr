// Task 3 — the load-bearing cross-language SharedLock proof, PLUS a contrasting SharedMutex<T> proof.
//
// `cd_sharedlock.dll` is the .NET class library produced from the Rust crate `cd_sharedlock`. It
// exports `sharedlock_new()`, which constructs a `mycorrhiza::sync::SharedLock` (a `SemaphoreSlim(1,1)`)
// on the Rust side and hands back the RAW managed `System.Threading.SemaphoreSlim` object — a genuine
// typed managed reference, no P/Invoke, no serialization, no wrapper class of our own.
//
// This program then runs TWO concurrent threads against that SAME SemaphoreSlim, incrementing one
// shared counter (also exposed by the Rust side) under the lock:
//   - a C# thread calling `Wait()`/`Release()` DIRECTLY on the received SemaphoreSlim (no Rust
//     involved at all on this side beyond having produced the object), and
//   - a genuine Rust OS thread (spawned via `std::thread::spawn` inside
//     `sharedlock_spawn_rust_worker`, itself invoked from a second C# background thread so both sides
//     actually run concurrently) calling `SharedLock::lock()`.
//
// If the shared exclusion were NOT real (e.g. two independent semaphores, or a broken handle-sharing
// path), the interleaved, non-atomic read/increment/write in `bump_counter` would lose updates and the
// final count would fall short of `2 * ITERS`. This mirrors the 200,000-per-thread scale used in the
// research probe.
//
// ---- Scenario (b), below: SharedMutex<T>, for contrast ----
//
// The `sharedlock_*` scenario above is the genuinely irreducible case for `unsafe`: C# itself calls into
// Rust to perform the increment, timed by its own direct `Wait()`/`Release()` -- it is a real co-mutator
// of the same logical counter, and there is no mechanism to hand a `SharedMutexGuard` (a Rust-only RAII
// type tied to a Rust borrow) across the FFI boundary for C# to use itself.
//
// The `sharedmutex_*` section below demonstrates the OTHER case `SharedMutex<T>`'s docs describe as its
// correct fit: Rust owns and performs ALL the mutation (via two genuine Rust OS threads spawned inside
// `sharedmutex_spawn_two_workers`), and C# merely starts the work and reads back the final value. C#
// gets an opaque `isize` token, not a handle onto the protected `i64` -- it cannot read or write that
// memory itself, only ask Rust to. Zero `unsafe` appears in the Rust code path that performs the
// increments (see `cargo_tests/cd_sharedlock/rustlib/src/lib.rs`).

using System;
using System.Threading;

public static class Program
{
    // Same scale as the research probe / cd_sync's pure-Rust proof (check #7): 200,000 iterations per
    // side, large enough that a real race would reliably show up as a short final count.
    const long ITERS = 200_000;

    public static int Main()
    {
        int pass = 0, total = 0;

        // ---- Acquire the shared lock object from Rust: a genuine typed managed reference. ----
        System.Threading.SemaphoreSlim sem = MainModule.sharedlock_new();
        Check("sharedlock_new() returns a real SemaphoreSlim", sem != null, true, ref pass, ref total);

        MainModule.sharedlock_reset_counter();
        Check("counter starts at 0", MainModule.sharedlock_get_counter(), 0L, ref pass, ref total);

        // ---- C# thread: increments the counter by calling Wait()/Release() DIRECTLY on the handle,
        // with NO Rust-side locking involved on this side at all -- mutual exclusion for this thread's
        // increments is provided entirely by C#'s own use of the shared SemaphoreSlim. ----
        Thread csharpWorker = new Thread(() =>
        {
            for (long i = 0; i < ITERS; i++)
            {
                sem.Wait();
                try
                {
                    MainModule.sharedlock_bump_counter_unlocked();
                }
                finally
                {
                    sem.Release();
                }
            }
        });

        // ---- Rust thread: a second C# background thread calls into Rust, which itself spawns a real
        // std::thread and blocks THAT C# thread until the Rust thread finishes its ITERS increments. ----
        Thread rustWorkerHost = new Thread(() =>
        {
            MainModule.sharedlock_spawn_rust_worker(sem, ITERS);
        });

        csharpWorker.Start();
        rustWorkerHost.Start();
        csharpWorker.Join();
        rustWorkerHost.Join();

        long finalCount = MainModule.sharedlock_get_counter();
        long expected = ITERS * 2;
        bool exact = finalCount == expected;
        total++;
        if (exact) pass++;
        Console.WriteLine($"  [{(exact ? "OK" : "FAIL")}] cross-language SharedLock exclusion: final_count={finalCount}, expected={expected}");

        // ---- Scenario (b): SharedMutex<T> -- Rust does ALL the mutating; C# only starts the work and
        // reads the result back. C# never gets a handle onto the protected i64 itself (only an opaque
        // isize token), so unlike the SharedLock scenario above, there is no unsafe anywhere on the Rust
        // side that performs the increments. ----
        nint mutexHandle = MainModule.sharedmutex_new(0);
        Check("sharedmutex_new() returns a non-null handle", mutexHandle != 0, true, ref pass, ref total);
        Check("SharedMutex<i64> starts at 0", MainModule.sharedmutex_get(mutexHandle), 0L, ref pass, ref total);

        // Rust spawns two of its own OS threads internally and blocks this call until both finish --
        // C# is not a participant in the mutation at all, only the caller that kicks it off.
        MainModule.sharedmutex_spawn_two_workers(mutexHandle, ITERS);

        long mutexFinal = MainModule.sharedmutex_get(mutexHandle);
        long mutexExpected = ITERS * 2;
        bool mutexExact = mutexFinal == mutexExpected;
        total++;
        if (mutexExact) pass++;
        Console.WriteLine($"  [{(mutexExact ? "OK" : "FAIL")}] SharedMutex<T> all-Rust-side exclusion: final_count={mutexFinal}, expected={mutexExpected}");

        MainModule.sharedmutex_free(mutexHandle);

        Console.WriteLine($"cd_sharedlock: {pass}/{total} checks passed");
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
