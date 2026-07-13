// Span<T> interop probe. Span<T> is a `ref struct` (a generic value type) whose real interop value is
// zero-copy between Rust memory and .NET: `new Span<T>(void* ptr, int len)` over a Rust slice, then
// hand it to a .NET API that reads/writes it in place.
//
// Exercises: value-type-generic ctor with concrete (void*, int) args; `get_Length()` and `Fill(T)`
// (value-type instance methods); and the zero-copy proof (a .NET `Fill` mutates the Rust buffer).
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use mycorrhiza::r#gen;
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
        CORELIB, SPAN, true, "Fill", 1, (i32,), ((), r#gen!(0)), (), &SpanI32, i32,
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

    // ---- Deeper API audit: Slice / CopyTo / TryCopyTo / contains / index_of / as_slice ----
    {
        use mycorrhiza::span::{ReadOnlySpan, Span};

        // Span<T>.Slice(int,int): a real .NET call, zero-copy over the same Rust buffer.
        let mut data = [10i32, 20, 30, 40, 50];
        let mut sp = Span::from_slice(&mut data);
        let mut mid = sp.slice(1, 3);
        chk(17, mid.len() as i64, 3);
        chk(18, mid.get(0).unwrap() as i64, 20);
        chk(19, mid.get(2).unwrap() as i64, 40);
        // Writing through the sub-span is visible in the original buffer (still zero-copy).
        mid.set(1, 999);
        drop(mid);
        drop(sp);
        chk(20, data[2] as i64, 999);
        chk(21, data[0] as i64, 10); // untouched, outside the slice

        // Span<T>.CopyTo: copy from one Rust buffer to another via the real .NET method.
        let mut src = [1i32, 2, 3, 4];
        let mut dst = [0i32; 4];
        let src_span = Span::from_slice(&mut src);
        let mut dst_span = Span::from_slice(&mut dst);
        src_span.copy_to(&mut dst_span);
        drop(src_span);
        drop(dst_span);
        chk(22, dst[0] as i64, 1);
        chk(23, dst[3] as i64, 4);

        // Span<T>.TryCopyTo: succeeds when large enough, fails (no write / no panic) when too small.
        let mut src2 = [5i32, 6, 7];
        let mut big = [0i32; 5];
        let mut small = [0i32; 2];
        let src2_span = Span::from_slice(&mut src2);
        let mut big_span = Span::from_slice(&mut big);
        let mut small_span = Span::from_slice(&mut small);
        chk(24, src2_span.try_copy_to(&mut big_span) as i64, 1);
        chk(25, src2_span.try_copy_to(&mut small_span) as i64, 0);
        drop(src2_span);
        drop(big_span);
        drop(small_span);
        chk(26, big[0] as i64, 5);
        chk(27, big[2] as i64, 7);
        chk(28, small[0] as i64, 0); // untouched — TryCopyTo did not write on failure

        // Span<T>.copy_from_slice: reverse direction, driven through ReadOnlySpan<T>.CopyTo.
        let mut buf3 = [0i32; 4];
        let mut buf3_span = Span::from_slice(&mut buf3);
        buf3_span.copy_from_slice(&[9i32, 8, 7, 6]);
        drop(buf3_span);
        chk(29, buf3[0] as i64, 9);
        chk(30, buf3[3] as i64, 6);

        // as_slice / as_mut_slice: direct Rust-side reads/writes of the same memory the Span views.
        let mut buf4 = [1i32, 2, 3];
        let mut span4 = Span::from_slice(&mut buf4);
        chk(31, span4.as_slice()[1] as i64, 2);
        span4.as_mut_slice()[0] = 42;
        drop(span4);
        chk(32, buf4[0] as i64, 42);

        // contains / index_of on Span<T>.
        let mut buf5 = [3i32, 1, 4, 1, 5];
        let span5 = Span::from_slice(&mut buf5);
        chk(33, span5.contains(4) as i64, 1);
        chk(34, span5.contains(99) as i64, 0);
        chk(35, span5.index_of(4).unwrap() as i64, 2);
        chk(36, span5.index_of(1).unwrap() as i64, 1); // first match
        chk(37, span5.index_of(99).is_none() as i64, 1);

        // ReadOnlySpan<T>.Slice + CopyTo + contains/index_of/as_slice.
        let ro2 = [100i32, 200, 300, 400];
        let ros2 = ReadOnlySpan::from_slice(&ro2);
        let ros2_mid = ros2.slice(1, 2);
        chk(38, ros2_mid.len() as i64, 2);
        chk(39, ros2_mid.as_slice()[0] as i64, 200);
        chk(40, ros2_mid.as_slice()[1] as i64, 300);
        chk(41, ros2.contains(300) as i64, 1);
        chk(42, ros2.index_of(300).unwrap() as i64, 2);

        let mut dst2 = [0i32; 4];
        let mut dst2_span = Span::from_slice(&mut dst2);
        ros2.copy_to(&mut dst2_span);
        drop(dst2_span);
        chk(43, dst2[0] as i64, 100);
        chk(44, dst2[3] as i64, 400);

        let mut small2 = [0i32; 1];
        let mut small2_span = Span::from_slice(&mut small2);
        chk(45, ros2.try_copy_to(&mut small2_span) as i64, 0);
    }

    // ---- Unsafe fast path: get_unchecked/set_unchecked match the checked path for in-bounds indices ----
    {
        use mycorrhiza::span::Span;

        let mut data = [1i32, 2, 3, 4, 5];
        let mut sp = Span::from_slice(&mut data);
        // get_unchecked must match get() for every valid index.
        for i in 0..sp.len() {
            let checked = sp.get(i).unwrap();
            let unchecked = unsafe { sp.get_unchecked(i) };
            chk(46 + i as u32, unchecked as i64, checked as i64);
        }

        // set_unchecked must match set()'s effect for every valid index.
        let mut a = [0i32; 5];
        let mut b = [0i32; 5];
        let mut sp_a = Span::from_slice(&mut a);
        let mut sp_b = Span::from_slice(&mut b);
        for i in 0..sp_a.len() {
            sp_a.set(i, (i + 1) * 10);
            unsafe { sp_b.set_unchecked(i, (i + 1) * 10) };
        }
        drop(sp_a);
        drop(sp_b);
        chk(51, a[0] as i64, b[0] as i64);
        chk(52, a[4] as i64, b[4] as i64);
        chk(53, b[2] as i64, 30);
    }

    // ---- GC-owned Memory<T> / ReadOnlyMemory<T>: async-safe managed-array storage ----
    {
        use mycorrhiza::memory::{Memory, ReadOnlyMemory};

        let mut memory = Memory::from_slice(&[10i32, 20, 30, 40]);
        chk(54, memory.len() as i64, 4);
        chk(55, memory.is_empty() as i64, 0);
        chk(56, memory.get(2).unwrap() as i64, 30);
        chk(57, memory.get(99).is_none() as i64, 1);
        chk(58, memory.set(1, 25) as i64, 1);
        chk(59, memory.get(1).unwrap() as i64, 25);

        // A Memory<T>.Slice is a cheap view over the SAME managed array, not a copy.
        let mut tail = memory.slice(1, 2);
        chk(60, tail.len() as i64, 2);
        tail.set(1, 99);
        chk(61, memory.get(2).unwrap() as i64, 99);
        tail.fill(7);
        chk(62, memory.get(1).unwrap() as i64, 7);
        chk(63, memory.get(2).unwrap() as i64, 7);

        let ro = ReadOnlyMemory::from_slice(&[3i32, 1, 4]);
        chk(64, ro.len() as i64, 3);
        chk(65, ro.slice(1, 2).len() as i64, 2);
        let _ = ro.span_handle();

        // CopyTo is the real ReadOnlyMemory<T>.CopyTo(Memory<T>) member.
        let mut copied = Memory::from_slice(&[0i32, 0, 0]);
        ro.copy_to(&mut copied);
        chk(66, copied.get(0).unwrap() as i64, 3);
        chk(67, copied.get(2).unwrap() as i64, 4);
        chk(68, copied.to_vec()[1] as i64, 1);
    }

    unsafe {
        Console::writeln_u64(PASS as u64);
        Console::writeln_u64(TOTAL as u64);
        if PASS == TOTAL {
            println!("== cd_span done ==");
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        }
    }
}
