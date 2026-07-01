// Span<T> interop probe. Span<T> is a `ref struct` (a generic value type) whose real interop value is
// zero-copy between Rust memory and .NET: `new Span<T>(void* ptr, int len)` over a Rust slice, then
// hand it to a .NET API that reads/writes it in place.
//
// Exercises: value-type-generic ctor with concrete (void*, int) args; `get_Length()` and `Fill(T)`
// (value-type instance methods); and the zero-copy proof (a .NET `Fill` mutates the Rust buffer).
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use mycorrhiza::gen;
use mycorrhiza::intrinsics::{
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2,
    rustc_clr_interop_generic_ctor2, RustcCLRInteropByRef, RustcCLRInteropManagedGenericStruct, RustcCLRInteropTypeGeneric,
};
use mycorrhiza::system::console::Console;

const CORELIB: &str = "System.Private.CoreLib";
const SPAN: &str = "System.Span";

// Span<T> is two words (a byref/pointer + an int length). SIZE is a Rust-side placeholder (CLR-sized).
type SpanI32 = RustcCLRInteropManagedGenericStruct<CORELIB, SPAN, 16, (i32,)>;

// new Span<i32>(void* pointer, int length) — a value-type-generic ctor; the params are concrete
// (void*, int32), NOT generic. We pass a Rust slice pointer, so the Span views Rust memory.
fn span_from_ptr(ptr: *mut i32, len: i32) -> SpanI32 {
    rustc_clr_interop_generic_ctor2::<
        CORELIB, SPAN, true, (i32,),
        ((), *mut (), i32),
        SpanI32, *mut (), i32,
    >(ptr as *mut (), len)
}
// Span<T>.get_Length() -> int32 (concrete return; value-type instance method, receiver by &).
fn span_len(s: &SpanI32) -> i32 {
    rustc_clr_interop_generic_call1::<
        CORELIB, SPAN, true, "get_Length", 1, (i32,), (i32,), i32, &SpanI32,
    >(s)
}
// Span<T>.Fill(T value) -> void — value-type instance method taking `!0`.
fn span_fill(s: &SpanI32, value: i32) {
    rustc_clr_interop_generic_call2::<
        CORELIB, SPAN, true, "Fill", 1, (i32,), ((), gen!(0)), (), &SpanI32, i32,
    >(s, value)
}
// Span<T>.Clear() -> void — zero every element.
fn span_clear(s: &SpanI32) {
    rustc_clr_interop_generic_call1::<
        CORELIB, SPAN, true, "Clear", 1, (i32,), ((),), (), &SpanI32,
    >(s)
}
// Span<T>.get_Item(int) -> ref T (the byref indexer). Returns a managed byref `!0&`; we take it as a
// raw pointer and read through it. (For a Rust-backed span the byref is a plain native pointer.)
fn span_get_ref(s: &SpanI32, idx: i32) -> *mut i32 {
    rustc_clr_interop_generic_call2::<
        CORELIB, SPAN, true, "get_Item", 1, (i32,),
        (RustcCLRInteropByRef<RustcCLRInteropTypeGeneric<0>>, i32), // Sig: ref !0 get_Item(int32)
        *mut i32, &SpanI32, i32,
    >(s, idx)
}
fn span_get(s: &SpanI32, idx: i32) -> i32 {
    unsafe { *span_get_ref(s, idx) }
}

static mut PASS: u32 = 0;
static mut TOTAL: u32 = 0;
fn chk(id: u32, got: i64, want: i64) {
    unsafe {
        TOTAL += 1;
        if got == want {
            PASS += 1;
        } else {
            Console::writeln_u64(90_000_000 + id as u64);
            Console::writeln_u64(got as u64);
        }
    }
}

fn main() -> std::process::ExitCode {
    let mut buf = [0i32; 5];
    let span = span_from_ptr(buf.as_mut_ptr(), 5);
    chk(1, span_len(&span) as i64, 5);

    // Zero-copy: a .NET Fill on the Span writes through to the Rust buffer.
    span_fill(&span, 7);
    chk(2, buf[0] as i64, 7);
    chk(3, buf[2] as i64, 7);
    chk(4, buf[4] as i64, 7);

    // Clear zeroes it (still the same Rust memory).
    span_clear(&span);
    chk(5, buf[0] as i64, 0);
    chk(6, buf[4] as i64, 0);

    // Byref indexer: read elements through `get_Item(int) -> ref T`.
    let vals = [10i32, 20, 30, 40, 50];
    let mut buf2 = vals;
    let span2 = span_from_ptr(buf2.as_mut_ptr(), 5);
    chk(7, span_get(&span2, 0) as i64, 10);
    chk(8, span_get(&span2, 2) as i64, 30);
    chk(9, span_get(&span2, 4) as i64, 50);
    // Write through the byref, then observe it in the Rust buffer.
    unsafe { *span_get_ref(&span2, 1) = 99 };
    chk(10, buf2[1] as i64, 99);

    // ---- Ergonomic mycorrhiza::span::{Span, ReadOnlySpan} over a Rust slice ----
    {
        use mycorrhiza::span::{ReadOnlySpan, Span};
        let mut data = [1i32, 2, 3, 4];
        let mut sp = Span::from_slice(&mut data);
        chk(11, sp.len() as i64, 4);
        chk(12, sp.get(2).unwrap() as i64, 3);
        sp.set(0, 100);
        sp.fill(0); // then set index 3
        sp.set(3, 42);
        drop(sp);
        chk(13, data[3] as i64, 42);
        chk(14, data[0] as i64, 0); // fill zeroed it

        let ro = [7i32, 8, 9];
        let ros = ReadOnlySpan::from_slice(&ro);
        chk(15, ros.len() as i64, 3);
        chk(16, ros.is_empty() as i64, 0); // not empty
        let _ = ros.handle(); // materialisable to hand to a .NET API
    }

    unsafe {
        Console::writeln_u64(PASS as u64);
        Console::writeln_u64(TOTAL as u64);
        if PASS == TOTAL {
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        }
    }
}
