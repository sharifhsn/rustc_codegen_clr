// Differential interop sample for the FULL method-wrapper surface emitted by the `spinacz`
// binding generator (mycorrhiza::bindings, re-exported at `mycorrhiza::System::*`).
//
// Unlike the raw-intrinsic probe in test/std/interop_slice_methods.rs (which open-codes the
// staticN/instanceN/virt0/ctorN calls), this sample calls the GENERATED wrappers directly, so it
// validates the real emitted surface end-to-end: generated `pub fn` -> magic-fn -> emitted .NET
// call. It covers ~12 methods across three namespaces and all four dispatch kinds:
//
//   static  (System.Math, System.String) : sqrt/pow/max/abs/floor, compare, is_null_or_empty
//   ctor    (System.Text.StringBuilder)  : new()
//   instance(StringBuilder, String)      : append, get_length (property getter -> callvirt)
//   virtual (String)                     : to_upper (returns a new String), get_hash_code
//
// Each result is printed as an integer/float marker via the managed Console, to be diffed against
// the expected native .NET values listed in the trailing comments.
//
// NOTE: the wrapper overloads `spinacz` picked are the FIRST faithful overload per (name, arity):
//   Math::max/min -> the `u8` overloads, Math::abs -> the `i16` overload (see bindings.rs ~9930).
//   StringBuilder::append -> Append(String) (the i32 overload is shadowed by the String one).
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, unused_variables)]

use mycorrhiza::system::console::Console;
use mycorrhiza::system::MString;
use mycorrhiza::System::Math;
use mycorrhiza::System::String as MStr;
use mycorrhiza::System::Text::StringBuilder;

fn main() {
    // ----- System.Math: static methods (static1 / static2) ----------------------------------
    Console::writeln_f64(Math::sqrt(144.0)); // expect 12
    Console::writeln_f64(Math::pow(2.0, 10.0)); // expect 1024
    Console::writeln_f64(Math::floor(3.9)); // expect 3
    Console::writeln_u64(Math::max(3u8, 7u8) as u64); // expect 7  (u8 overload)
    Console::writeln_u64(Math::abs(-5i16) as u64); // expect 5  (i16 overload)

    // ----- System.Text.StringBuilder: ctor + instance + property getter ----------------------
    // new() -> append("hello") -> get_Length == 5
    let sb = StringBuilder::new();
    let hello: MString = "hello".into();
    let sb = sb.append(hello); // instance1::<"Append", System::String, StringBuilder>
    Console::writeln_u64(sb.get_length() as u64); // expect 5  (get_Length, reference-type -> callvirt)

    // append another piece -> "hello, world" has length 12
    let rest: MString = ", world".into();
    let sb = sb.append(rest);
    Console::writeln_u64(sb.get_length() as u64); // expect 12

    // ----- System.String: static + instance + virtual ----------------------------------------
    let s: MString = "abc".into();
    Console::writeln_u64(s.get_length() as u64); // expect 3 (instance0 property getter)

    // ToUpper() returns a new managed String; its length is unchanged (3).
    let up = s.to_upper(); // instance0::<"ToUpper", System::String>
    Console::writeln_u64(up.get_length() as u64); // expect 3

    // String.IsNullOrEmpty("") -> true (static1, bool); IsNullOrEmpty("abc") -> false.
    let empty: MString = "".into();
    Console::writeln_u64(MStr::is_null_or_empty(empty) as u64); // expect 1 (true)
    Console::writeln_u64(MStr::is_null_or_empty(s) as u64); // expect 0 (false)

    // String.Compare("abc","abd") < 0 ; Compare("abc","abc") == 0. Print Compare-equal result.
    let a: MString = "abc".into();
    let b: MString = "abc".into();
    Console::writeln_u64(MStr::compare(a, b) as u64); // expect 0 (equal)

    // GetHashCode of a String is virtual (virt0); we only check it is deterministic by hashing
    // two equal strings and printing whether the hashes match (1 == match). This exercises the
    // virtual-dispatch path without depending on the (runtime-specific) hash value.
    let h1: MString = "stable".into();
    let h2: MString = "stable".into();
    let m1 = h1.get_hash_code();
    let m2 = h2.get_hash_code();
    Console::writeln_u64((m1 == m2) as u64); // expect 1 (equal strings hash equally)
}
