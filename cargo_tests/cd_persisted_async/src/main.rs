// Proof for Wall 1 (the "managed ref inside a coroutine's saved state" wall documented in
// mycorrhiza/src/task.rs): a `System.Runtime.InteropServices.GCHandle` is itself a .NET *value type*
// wrapping a plain `IntPtr` — NOT a GC reference. `mycorrhiza::class::Class<ASSEMBLY, CLASS_PATH>`
// already wraps exactly that GCHandle (see mycorrhiza/src/class.rs), so a `Class<..>` value has no
// gcref field and should be legal to hold *across* an `.await` point inside a real Rust `async fn`
// (unlike a raw `RustcCLRInteropManagedClass` handle, which IS a gcref and is rejected by
// `cilly`'s `layout_check` — see cilly/src/ir/class.rs, `ManagedRefInOverlapingField`).
//
// This test builds a managed `System.Text.StringBuilder`, wraps it in `Class<..>` (GCHandle-backed),
// and *holds that Class value across two separate `.await` points* inside one `async fn` body —
// appending text on both sides of each await, then reads the final string back through a second
// managed round trip (`ToString()` -> `DotNetString::to_rust_string()`). If this compiles (passes
// `layout_check`) and produces the right string, Wall 1 is closed for this GCHandle-newtype pattern.
#![allow(dead_code)]

use mycorrhiza::class::Class;
use mycorrhiza::prelude::*;
use mycorrhiza::system::console::Console;
use mycorrhiza::system::DotNetString;
use mycorrhiza::System;

type SB = Class<"System.Private.CoreLib", "System.Text.StringBuilder">;

// The critical shape: `sb` (a `Class<..>`, GCHandle-backed) is declared BEFORE the first `.await`
// and used again AFTER the second `.await` — i.e. it is genuinely part of the coroutine's saved
// state across two suspend points, not just a same-segment temporary.
// NOTE: the `Future::Output` is a plain Rust `std::string::String`, not a raw managed handle —
// returning a bare `RustcCLRInteropManagedClass` (gcref) directly from an `async fn` hits a SEPARATE,
// pre-existing layout limitation (the coroutine's internal `Poll<Output>` scaffolding rejects a gcref
// `Output`, unrelated to whether anything is held *across* an await). That is not the wall this test
// is targeting; converting to an owned Rust `String` before returning sidesteps it and isolates the
// actual claim under test: can a `Class<..>` (GCHandle-backed) LOCAL survive being held across two
// `.await` points inside the coroutine's own saved state.
async fn build_message() -> std::string::String {
    let sb: SB = SB::ctor0();

    // segment 0: append "hello" before the first await
    {
        let naked: System::Text::StringBuilder = unsafe { sb.get_naked_ref() };
        naked.append(MString::from("hello "));
    }

    await_unit(Task::delay(5)).await; // suspend point 1 -- `sb` must survive this

    // segment 1: append more text between the two awaits, still using the SAME persisted handle
    {
        let naked: System::Text::StringBuilder = unsafe { sb.get_naked_ref() };
        naked.append(MString::from("async "));
    }

    await_unit(Task::delay(5)).await; // suspend point 2 -- `sb` must survive this too

    // segment 2: after both awaits, append the last piece and read the final value back
    let naked: System::Text::StringBuilder = unsafe { sb.get_naked_ref() };
    naked.append(MString::from("world"));
    DotNetString::from_handle(naked.to_string()).to_rust_string()
}

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    println!("== cd_persisted_async start ==");

    // ---------- 1. a Class<..> (GCHandle-backed) value held across TWO `.await` points ----------
    {
        let got: std::string::String = block_on(build_message());
        chk!(got, "hello async world".to_string());
    }

    // ---------- 2. a bare Class<..> alone (no method calls) also survives an await ----------
    {
        let survived: bool = block_on(async {
            let handle: SB = SB::ctor0();
            await_unit(Task::delay(5)).await;
            // if `handle`'s GCHandle field got corrupted/collected, get_naked_ref + a call here
            // would crash or return garbage; instead confirm it's a live, callable object.
            let naked: System::Text::StringBuilder = unsafe { handle.get_naked_ref() };
            naked.get_length() >= 0
        });
        chk!(survived, true);
    }

    // ---------- 3. a Class<..> held across `await_task` on a real Task<i32> (isolates whether the
    // hazard is specific to `await_task`'s TaskT<T> consumption path, vs the plain `await_unit`
    // path already proven in test 1/2 above). ----------
    {
        let combined: i32 = block_on(async {
            let handle: SB = SB::ctor0();
            {
                let naked: System::Text::StringBuilder = unsafe { handle.get_naked_ref() };
                naked.append(MString::from("x"));
            }
            let t: TaskT<i32> = future_to_task(async { 7 });
            let got: i32 = await_task(t).await; // suspend point -- `handle` must survive
            let naked: System::Text::StringBuilder = unsafe { handle.get_naked_ref() };
            naked.get_length() + got
        });
        chk!(combined, 8); // len("x") == 1, + 7 == 8
    }

    // ---------- 4. a Class<..> held across TWO `await_task`s on real Task<i32>s (matches the
    // cd_efcore_async shape: two separate awaited TaskT<i32> productions, same persisted handle). --
    {
        let combined: i32 = block_on(async {
            let handle: SB = SB::ctor0();
            let got1: i32 = {
                let naked: System::Text::StringBuilder = unsafe { handle.get_naked_ref() };
                naked.append(MString::from("x"));
                let t: TaskT<i32> = future_to_task(async { 7 });
                await_task(t).await // suspend point 1
            };
            let got2: i32 = {
                let naked: System::Text::StringBuilder = unsafe { handle.get_naked_ref() };
                naked.append(MString::from("y"));
                let t: TaskT<i32> = future_to_task(async { 11 });
                await_task(t).await // suspend point 2
            };
            let naked: System::Text::StringBuilder = unsafe { handle.get_naked_ref() };
            naked.get_length() as i32 + got1 + got2
        });
        chk!(combined, 20); // len("xy") == 2, + 7 + 11 == 20
    }

    println!("== cd_persisted_async done ==");
    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
