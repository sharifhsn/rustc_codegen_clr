//! H2 backend-correctness REPRO: interior `&mut` into a large heap struct.
//! soak_miniz_oxide builds but NREs at runtime in compress_normal, which takes
//! `&mut d.field` into a big Box<CompressorOxide>. This isolates the smallest
//! failing shape: forming an interior mutable reference into a (heap / large /
//! nested) struct and mutating through it across a call.
//!
//! Each candidate prints a marker BEFORE the suspect call so the last marker
//! printed pinpoints which interior-&mut shape NREs. Panic-safe: no unwraps,
//! no indexing that can go OOB.

// A helper that mutates through an interior &mut. #[inline(never)] forces a real
// call boundary (the byref must survive being passed as an argument).
#[inline(never)]
fn bump(c: &mut u32) {
    *c = c.wrapping_add(1);
}

#[inline(never)]
fn bump64(c: &mut u64) {
    *c = c.wrapping_add(1);
}

// A nested inner struct (so we can test &mut b.nested.x).
struct Inner {
    x: u32,
    y: u32,
    pad: [u32; 8],
}

// HuffmanOxide analogue: a struct with array fields, lives behind a Box.
struct Huff {
    count: [[u16; 16]; 2],
    total: u32,
}
impl Huff {
    fn new() -> Self {
        Huff { count: [[0; 16]; 2], total: 0 }
    }
}

// record_literal analogue: takes &mut Huff (the DEREF of the box).
#[inline(never)]
fn record(h: &mut Huff, lit: u8) {
    h.total += 1;
    h.count[0][(lit as usize) & 15] += 1;
}

// A big struct, miniz_oxide-CompressorOxide-shaped: many fields, a nested
// struct, and large arrays. Large enough that the optimizer/layout treats it
// as a real heap object, not something it can scalar-replace.
struct Big {
    counter: u32,
    other: u64,
    nested: Inner,
    arr: [u32; 64],
    big_arr: [u8; 4096],
    tail: u32,
}

impl Big {
    fn new() -> Self {
        Big {
            counter: 0,
            other: 0,
            nested: Inner { x: 0, y: 0, pad: [0; 8] },
            arr: [0; 64],
            big_arr: [0; 4096],
            tail: 0,
        }
    }
}

// ---- contrast: a SMALL stack struct (expected to WORK) ----
struct Small {
    counter: u32,
    other: u32,
}

#[inline(never)]
fn small_test() {
    let mut s = Small { counter: 10, other: 20 };
    // interior &mut into a small stack struct
    bump(&mut s.counter);
    println!("  small.counter = {} (want 11)", s.counter);
}

// ---- candidate A: &mut b.field on a LARGE STACK struct ----
#[inline(never)]
fn large_stack_test() {
    let mut b = Big::new();
    println!("A0 large-stack: before bump(&mut b.counter)");
    bump(&mut b.counter);
    println!("A1 large-stack b.counter = {} (want 1)", b.counter);
    println!("A2 before bump(&mut b.nested.x)");
    bump(&mut b.nested.x);
    println!("A3 large-stack b.nested.x = {} (want 1)", b.nested.x);
    println!("A4 before bump(&mut b.arr[3])");
    bump(&mut b.arr[3]);
    println!("A5 large-stack b.arr[3] = {} (want 1)", b.arr[3]);
}

// ---- candidate B: &mut b.field on a Box<Big> (HEAP) ----
#[inline(never)]
fn box_field_test() {
    let mut b: Box<Big> = Box::new(Big::new());
    println!("B0 box-field: before bump(&mut b.counter)");
    bump(&mut b.counter);
    println!("B1 box b.counter = {} (want 1)", b.counter);
}

#[inline(never)]
fn box_other64_test() {
    let mut b: Box<Big> = Box::new(Big::new());
    println!("C0 box-other64: before bump64(&mut b.other)");
    bump64(&mut b.other);
    println!("C1 box b.other = {} (want 1)", b.other);
}

// ---- candidate D: nested field on a Box<Big> (&mut b.nested.x) ----
#[inline(never)]
fn box_nested_test() {
    let mut b: Box<Big> = Box::new(Big::new());
    println!("D0 box-nested: before bump(&mut b.nested.x)");
    bump(&mut b.nested.x);
    println!("D1 box b.nested.x = {} (want 1)", b.nested.x);
}

// ---- candidate E: array element on a Box<Big> (&mut b.arr[i]) ----
#[inline(never)]
fn box_array_test() {
    let mut b: Box<Big> = Box::new(Big::new());
    let i = 5usize;
    println!("E0 box-array: before bump(&mut b.arr[i])");
    bump(&mut b.arr[i]);
    println!("E1 box b.arr[5] = {} (want 1)", b.arr[i]);
}

// ---- candidate F: &mut b.field as *mut _, then write through it ----
#[inline(never)]
fn box_rawptr_test() {
    let mut b: Box<Big> = Box::new(Big::new());
    println!("F0 box-rawptr: before (&mut b.counter) as *mut u32 write");
    let p: *mut u32 = &mut b.counter;
    unsafe { *p = 42; }
    println!("F1 box b.counter = {} (want 42)", b.counter);
}

// ---- candidate G: take the &mut once, mutate through it across a call ----
#[inline(never)]
fn use_ref(r: &mut u32) {
    bump(r);
    bump(r);
}
#[inline(never)]
fn box_passed_ref_test() {
    let mut b: Box<Big> = Box::new(Big::new());
    println!("G0 box-passed-ref: before use_ref(&mut b.counter)");
    use_ref(&mut b.counter);
    println!("G1 box b.counter = {} (want 2)", b.counter);
}

// ---- candidate H: deeply nested &mut on a heap struct via explicit pointer ----
// Mirrors miniz: a function takes `d: &mut Big` and forms interior &mut from it.
#[inline(never)]
fn process(d: &mut Big) {
    println!("H0 process: before bump(&mut d.counter)");
    bump(&mut d.counter);
    println!("H1 d.counter = {} (want 1)", d.counter);
    println!("H2 before bump(&mut d.nested.x)");
    bump(&mut d.nested.x);
    println!("H3 d.nested.x = {} (want 1)", d.nested.x);
}
#[inline(never)]
fn box_via_param_test() {
    let mut b: Box<Big> = Box::new(Big::new());
    println!("H_ box-via-param: calling process(&mut *b)");
    process(&mut b);
    println!("H4 box b.counter = {} b.nested.x = {}", b.counter, b.nested.x);
}

// ============================================================================
// miniz_oxide-shaped candidates: Box<[T; N]> array fields inside nested structs,
// reached via a `&mut` to the outer struct. This is the exact shape of
// HashBuffers { dict: Box<[u8; N]>, ... } inside DictOxide inside CompressorOxide,
// mutated in compress_normal(d: &mut CompressorOxide).
// ============================================================================

// HashBuffers analogue (0.7.4 layout): INLINE arrays (not boxed). The Box is one
// level up, at the Dict level (b: Box<Buffers>).
struct Buffers {
    dict: [u8; 4096],
    next: [u16; 1024],
    hash: [u16; 1024],
}
impl Buffers {
    fn new() -> Self {
        Buffers {
            dict: [0u8; 4096],
            next: [0u16; 1024],
            hash: [0u16; 1024],
        }
    }
}

// DictOxide analogue (0.7.4): the HashBuffers are BEHIND A BOX: b: Box<Buffers>.
struct Dict {
    max_probes: [u32; 2],
    b: Box<Buffers>,
    size: usize,
    lookahead_size: usize,
    lookahead_pos: usize,
}
impl Dict {
    fn new() -> Self {
        Dict {
            max_probes: [0; 2],
            b: Box::new(Buffers::new()),
            size: 0,
            lookahead_size: 0,
            lookahead_pos: 0,
        }
    }
}

// CompressorOxide analogue: holds the Dict + a Box<NestedStruct> field (huff).
struct Compressor {
    counter: u32,
    dict: Dict,
    huff: Box<Huff>,
}
impl Compressor {
    fn new() -> Self {
        Compressor {
            counter: 0,
            dict: Dict::new(),
            huff: Box::new(Huff::new()),
        }
    }
}

// ---- candidate I: index a Box<[u8;N]> field directly (simplest) ----
#[inline(never)]
fn boxed_array_field_test() {
    let mut buf = Buffers::new();
    println!("I0 boxed-array-field: before buf.dict[10] = 7");
    buf.dict[10] = 7;
    println!("I1 buf.dict[10] = {} (want 7)", buf.dict[10]);
}

// ---- candidate J: EXACT compress_normal shape ----
//      `let dictb = &mut d.dict.b;` where d.dict.b is Box<Buffers>, then index
//      dictb.dict[..] (deref the box, index inline array) inside a loop, while
//      also writing back scalar fields of d.dict (d.dict.size, lookahead_*).
#[inline(never)]
fn process_dict(d: &mut Compressor) {
    let mut lookahead_size = d.dict.lookahead_size;
    let lookahead_pos = d.dict.lookahead_pos;
    println!("J0 nested-&mut-box<inline-array>: before dictb loop");
    {
        // `let dictb = &mut d.dict.b;` -> &mut Box<Buffers>
        let dictb = &mut d.dict.b;
        let input: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        for &c in input.iter() {
            let dst_pos = (lookahead_pos + lookahead_size) & 4095;
            dictb.dict[dst_pos] = c;                 // write through box deref + inline-array index
            let ins_pos = lookahead_pos + lookahead_size;
            dictb.next[ins_pos & 1023] = dictb.hash[(c as usize) & 1023];
            dictb.hash[(c as usize) & 1023] = ins_pos as u16;
            lookahead_size += 1;
        }
    }
    // write back scalars to d.dict AFTER the &mut dictb borrow ends
    d.dict.size = lookahead_size.min(4096 - lookahead_size);
    d.dict.lookahead_size = lookahead_size;
    println!("J1 dictb.dict[0] = {} (want 1), d.dict.size = {}", d.dict.b.dict[0], d.dict.size);
}
#[inline(never)]
fn nested_boxed_array_test() {
    let mut c = Compressor::new();
    println!("J_ calling process_dict(&mut c)");
    process_dict(&mut c);
    println!("J2 c.dict.b.dict[0] = {} (want 1)", c.dict.b.dict[0]);
}

// ---- candidate K: index Box<[u8;N]> field through a &mut struct WITHOUT the
//      intermediate `let dictb` rebinding (direct d.dict.b.dict[i]) ----
#[inline(never)]
fn process_direct(d: &mut Compressor) {
    println!("K0 direct d.dict.b.dict[i] = c (box<inline array> field of by-ref struct)");
    d.dict.b.dict[20] = 33;
    println!("K1 d.dict.b.dict[20] = {} (want 33)", d.dict.b.dict[20]);
}
#[inline(never)]
fn direct_boxed_array_test() {
    let mut c = Compressor::new();
    process_direct(&mut c);
    println!("K2 c.dict.b.dict[20] = {} (want 33)", c.dict.b.dict[20]);
}

// ---- candidate L: &mut to a FIELD of the boxed Huff (d.huff.total) ----
#[inline(never)]
fn process_huff(d: &mut Compressor) {
    println!("L0 before bump(&mut d.huff.total)");
    bump(&mut d.huff.total);
    println!("L1 d.huff.total = {} (want 1)", d.huff.total);
}
#[inline(never)]
fn box_field_in_byref_test() {
    let mut c = Compressor::new();
    process_huff(&mut c);
    println!("L2 c.huff.total = {} (want 1)", c.huff.total);
}

// ---- candidate N: EXACT record_literal shape ----
//      `record(&mut d.huff, lit)` where d.huff: Box<Huff> — the &mut Box deref-
//      coerces to &mut Huff (the box's data pointer). Callee derefs it.
#[inline(never)]
fn process_record(d: &mut Compressor) {
    println!("N0 before record(&mut d.huff, 5)  [&mut *box-field of by-ref struct]");
    record(&mut d.huff, 5);
    record(&mut d.huff, 5);
    println!("N1 d.huff.total = {} (want 2), count[0][5] = {} (want 2)", d.huff.total, d.huff.count[0][5]);
}
#[inline(never)]
fn deref_box_field_arg_test() {
    let mut c = Compressor::new();
    process_record(&mut c);
    println!("N2 c.huff.total = {} (want 2)", c.huff.total);
}

// ---- candidate O: the FULL miniz mix — record + dictb in one fn, like
//      compress_normal: &mut d.huff, &mut d.lz-style, d.dict.b.dict, writebacks ----
struct Lz {
    codes: [u8; 64],
    code_position: usize,
    total_bytes: u32,
}
impl Lz {
    fn new() -> Self { Lz { codes: [0; 64], code_position: 1, total_bytes: 0 } }
    fn write_code(&mut self, v: u8) { self.codes[self.code_position & 63] = v; self.code_position += 1; }
}
struct Compressor2 {
    lz: Lz,
    dict: Dict,
    huff: Box<Huff>,
}
impl Compressor2 {
    fn new() -> Self { Compressor2 { lz: Lz::new(), dict: Dict::new(), huff: Box::new(Huff::new()) } }
}
#[inline(never)]
fn record2(h: &mut Huff, lz: &mut Lz, lit: u8) {
    lz.total_bytes += 1;
    lz.write_code(lit);
    h.count[0][(lit as usize) & 15] += 1;
}
#[inline(never)]
fn compress_like(d: &mut Compressor2) {
    let input: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let mut lookahead_size = d.dict.lookahead_size;
    let lookahead_pos = d.dict.lookahead_pos;
    for &c in input.iter() {
        let dst_pos = (lookahead_pos + lookahead_size) & 4095;
        d.dict.b.dict[dst_pos] = c;
        lookahead_size += 1;
        // pass &mut d.huff (deref box) AND &mut d.lz (interior) to a fn
        record2(&mut d.huff, &mut d.lz, c);
    }
    d.dict.lookahead_size = lookahead_size;
    d.dict.size = lookahead_size;
    println!("O1 huff.count[0][1]={} (want 1), lz.total = {} (want 8), dict[0]={} (want 1)",
        d.huff.count[0][1], d.lz.total_bytes, d.dict.b.dict[0]);
}
#[inline(never)]
fn full_mix_test() {
    let mut d = Compressor2::new();
    println!("O0 calling compress_like(&mut d)");
    compress_like(&mut d);
    println!("O2 d.huff.count[0][1] = {} (want 1)", d.huff.count[0][1]);
}

// ---- candidate P: O but only the huff arg (no &mut d.lz), in a loop ----
#[inline(never)]
fn compress_p(d: &mut Compressor2) {
    let input: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    for &c in input.iter() {
        record(&mut d.huff, c);   // single deref-box-field arg, in a loop
    }
    println!("P1 huff.total = {} (want 8)", d.huff.total);
}
#[inline(never)]
fn loop_huff_only_test() {
    let mut d = Compressor2::new();
    compress_p(&mut d);
    println!("P2 d.huff.total = {} (want 8)", d.huff.total);
}

// ---- candidate Q: O but no dict write (huff + lz, in a loop) ----
#[inline(never)]
fn compress_q(d: &mut Compressor2) {
    let input: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    for &c in input.iter() {
        record2(&mut d.huff, &mut d.lz, c);
    }
    println!("Q1 huff.count[0][1]={} (want 1), lz.total = {} (want 8)", d.huff.count[0][1], d.lz.total_bytes);
}
#[inline(never)]
fn loop_huff_lz_test() {
    let mut d = Compressor2::new();
    compress_q(&mut d);
    println!("Q2 d.huff.count[0][1] = {} (want 1)", d.huff.count[0][1]);
}

// ---- candidate R: huff + lz, NO loop (straight-line, two calls) ----
#[inline(never)]
fn compress_r(d: &mut Compressor2) {
    record2(&mut d.huff, &mut d.lz, 1);
    record2(&mut d.huff, &mut d.lz, 2);
    println!("R1 huff.count[0][1]={} (want 1), count[0][2]={} (want 1), lz.total = {} (want 2)", d.huff.count[0][1], d.huff.count[0][2], d.lz.total_bytes);
}
#[inline(never)]
fn straight_huff_lz_test() {
    let mut d = Compressor2::new();
    compress_r(&mut d);
    println!("R2 d.huff.count[0][1] = {} (want 1)", d.huff.count[0][1]);
}

// ---- candidate S: &mut d.huff + a SEPARATE LOCAL lz (not an interior ref of d) ----
#[inline(never)]
fn compress_s(d: &mut Compressor2) {
    let mut local_lz = Lz::new();
    record2(&mut d.huff, &mut local_lz, 1);
    record2(&mut d.huff, &mut local_lz, 2);
    println!("S1 huff.count[0][1]={} (want 1), local_lz.total = {} (want 2)", d.huff.count[0][1], local_lz.total_bytes);
}
#[inline(never)]
fn huff_plus_local_test() {
    let mut d = Compressor2::new();
    compress_s(&mut d);
    println!("S2 d.huff.count[0][1] = {} (want 1)", d.huff.count[0][1]);
}

// ---- candidate T: TWO box-field derefs of the SAME struct (huff twice) ----
//      record2 takes (&mut Huff, &mut Lz). Here pass &mut d.huff and &mut d.lz
//      but ALSO read d.huff right after, to see if the box value is stale.
#[inline(never)]
fn compress_t(d: &mut Compressor2) {
    // Just ONE call, then read back immediately. record2 writes count[0][lit&15].
    record2(&mut d.huff, &mut d.lz, 1);
    println!("T1 (single call) huff.count[0][1] = {} (want 1), huff.total = {} (want 0)", d.huff.count[0][1], d.huff.total);
}
#[inline(never)]
fn single_call_huff_lz_test() {
    let mut d = Compressor2::new();
    compress_t(&mut d);
    println!("T2 d.huff.count[0][1] = {} (want 1)", d.huff.count[0][1]);
}

// ---- candidate U: order swapped — &mut d.lz FIRST arg, &mut d.huff SECOND ----
#[inline(never)]
fn record2_swapped(lz: &mut Lz, h: &mut Huff, lit: u8) {
    lz.total_bytes += 1;
    h.count[0][(lit as usize) & 15] += 1;
    h.total += 1;
}
#[inline(never)]
fn compress_u(d: &mut Compressor2) {
    record2_swapped(&mut d.lz, &mut d.huff, 1);
    println!("U1 huff.total = {} (want 1), lz.total = {} (want 1)", d.huff.total, d.lz.total_bytes);
}
#[inline(never)]
fn swapped_args_test() {
    let mut d = Compressor2::new();
    compress_u(&mut d);
    println!("U2 d.huff.total = {} (want 1)", d.huff.total);
}

// ---- candidate V: print the raw pointer the callee receives for each arg ----
#[inline(never)]
fn show_ptrs(h: &mut Huff, lz: &mut Lz, _lit: u8) {
    let hp = h as *mut Huff as usize;
    let lp = lz as *mut Lz as usize;
    println!("V_callee: h_ptr={:#x} lz_ptr={:#x}", hp, lp);
    h.total += 1;
}
#[inline(never)]
fn ptr_probe(d: &mut Compressor2) {
    // What is the box's real data pointer (via a working single-arg path)?
    let real = (&mut *d.huff) as *mut Huff as usize;
    println!("V_caller: real box data ptr = {:#x}", real);
    show_ptrs(&mut d.huff, &mut d.lz, 1);
    println!("V_after: d.huff.total = {} (want 1)", d.huff.total);
}
#[inline(never)]
fn ptr_probe_test() {
    let mut d = Compressor2::new();
    ptr_probe(&mut d);
}

// ============================================================================
// candidate W: THE ACTUAL BUG — a field whose byte offset exceeds 65535.
// FieldOffsetIterator clamps offsets > u16::MAX to 0, aliasing fields. This is
// why CompressorOxide (.size 65688) has params/huff/dict all at offset 0.
// ============================================================================
// >64KiB so the trailing scalar field lands past offset 65535.
struct Big64k {
    blob: [u8; 70000],
    a: u32,   // its offset is ~70000 > 65535 -> clamped to 0 -> aliases blob[0..4]
    b: u32,
}
impl Big64k {
    fn new() -> Self { Big64k { blob: [0; 70000], a: 0, b: 0 } }
}
#[inline(never)]
fn write_a(d: &mut Big64k, v: u32) {
    d.a = v;          // stfld at the (wrong) clamped offset
}
#[inline(never)]
fn over_64k_offset_test() {
    let mut d = Box::new(Big64k::new());
    d.blob[0] = 111;
    d.blob[1] = 112;
    write_a(&mut d, 0xAABBCCDD);
    // If `a`'s offset is correct (~70000), blob[0..2] stay 111,112 and a==0xAABBCCDD.
    // If `a` is clamped to offset 0, writing a CLOBBERS blob[0..4].
    println!("W1 blob[0]={} (want 111), blob[1]={} (want 112), a={:#x} (want 0xaabbccdd)",
        d.blob[0], d.blob[1], d.a);
    let ok = d.blob[0] == 111 && d.blob[1] == 112 && d.a == 0xAABBCCDD;
    println!("W2 over-64k-offset OK = {} (want true)", ok);
}

// ---- candidate M: the WHOLE compressor on the heap (Box<Compressor>), as
//      compress_to_vec_inner does, then call a by-ref fn on it. ----
#[inline(never)]
fn boxed_compressor_test() {
    let mut c: Box<Compressor> = Box::new(Compressor::new());
    println!("M_ calling process_dict(&mut c) on Box<Compressor>");
    process_dict(&mut c);
    println!("M2 c.dict.b.dict[10] = {} (want 7)", c.dict.b.dict[10]);
}

fn main() {
    println!("== pal_heap start ==");

    println!("[small]");
    small_test();

    println!("[A large_stack]");
    large_stack_test();

    println!("[B box_field]");
    box_field_test();

    println!("[C box_other64]");
    box_other64_test();

    println!("[D box_nested]");
    box_nested_test();

    println!("[E box_array]");
    box_array_test();

    println!("[F box_rawptr]");
    box_rawptr_test();

    println!("[G box_passed_ref]");
    box_passed_ref_test();

    println!("[H box_via_param]");
    box_via_param_test();

    println!("[I boxed_array_field]");
    boxed_array_field_test();

    println!("[J nested_boxed_array]");
    nested_boxed_array_test();

    println!("[K direct_boxed_array]");
    direct_boxed_array_test();

    println!("[L box_field_in_byref]");
    box_field_in_byref_test();

    println!("[M boxed_compressor]");
    boxed_compressor_test();

    println!("[N deref_box_field_arg]");
    deref_box_field_arg_test();

    println!("[O full_mix]");
    full_mix_test();

    println!("[P loop_huff_only]");
    loop_huff_only_test();

    println!("[Q loop_huff_lz]");
    loop_huff_lz_test();

    println!("[R straight_huff_lz]");
    straight_huff_lz_test();

    println!("[S huff_plus_local]");
    huff_plus_local_test();

    println!("[T single_call_huff_lz]");
    single_call_huff_lz_test();

    println!("[U swapped_args]");
    swapped_args_test();

    println!("[V ptr_probe]");
    ptr_probe_test();

    println!("[W over_64k_offset]");
    over_64k_offset_test();

    println!("== pal_heap done ==");
}
