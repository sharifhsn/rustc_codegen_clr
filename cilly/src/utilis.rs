use std::fmt::Debug;

use crate::bimap::Interned;

use crate::Assembly;
use crate::cilnode::{ExtendKind, IsPure, MethodKind};
use crate::ir::{BasicBlock, CILNode};
use crate::{BinOp, BranchCond, CILRoot, MethodDef, Type};
use crate::{ClassRef, FnSig, Int, MethodRef, StaticFieldDesc};

pub fn argc_argv_init_method(asm: &mut Assembly) -> Interned<MethodRef> {
    let init_cs = MethodRef::new(
        *asm.main_module(),
        asm.alloc_string("argc_argv_init"),
        asm.sig([], Type::Void),
        MethodKind::Static,
        vec![].into(),
    );

    asm.alloc_methodref(init_cs)
}
pub fn mstring_to_utf8ptr(mstring: Interned<CILNode>, asm: &mut Assembly) -> Interned<CILNode> {
    let mref = MethodRef::new(
        ClassRef::marshal(asm),
        asm.alloc_string("StringToCoTaskMemUTF8"),
        asm.sig([Type::PlatformString], Type::Int(Int::ISize)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    let call = asm.call(mref, &[mstring], IsPure::NOT);
    // `StringToCoTaskMemUTF8` returns an `IntPtr` to a NUL-terminated UTF8 byte
    // buffer — semantically a `uint8*` (C `char*`). `cast_ptr(x, T)` yields `Ptr(T)`,
    // so the pointee is `u8`, NOT `uint8*` (which would over-pointer the result to
    // `uint8**` and mismatch the `uint8*` slots its callers store it into).
    asm.cast_ptr(call, Type::Int(Int::U8))
}

pub fn get_environ(asm: &mut Assembly) -> Interned<MethodRef> {
    let main_module = asm.main_module();
    let uint8_ptr = asm.nptr(Type::Int(Int::U8));
    let uint8_ptr_ptr = asm.nptr(uint8_ptr);
    let init_cs = MethodRef::new(
        *asm.main_module(),
        asm.alloc_string("get_environ"),
        asm.sig([], uint8_ptr_ptr),
        MethodKind::Static,
        vec![].into(),
    );
    let init_cs = asm.alloc_methodref(init_cs);
    if asm.method_def_from_ref(init_cs).is_some() {
        return init_cs;
    }
    // Local layout (mirrors the previous `add_local` order):
    //   0: dict, 1: envc, 2: arr_ptr, 3: idx, 4: iter, 5: keyval, 6: encoded_keyval
    let dictionary_local: u32 = 0;
    let envc: u32 = 1;
    let arr_ptr: u32 = 2;
    let idx: u32 = 3;
    let iter_local: u32 = 4;
    let keyval: u32 = 5;
    let encoded_keyval: u32 = 6;
    let keyval_tpe = ClassRef::dictionary_entry(asm);
    let i_dictionary_class = ClassRef::i_dictionary(asm);
    let dictionary_iterator = ClassRef::dictionary_iterator(asm);
    let string_class = ClassRef::string(asm);
    let locals = vec![
        (
            Some(asm.alloc_string("dict")),
            asm.alloc_type(Type::ClassRef(i_dictionary_class)),
        ),
        (
            Some(asm.alloc_string("envc")),
            asm.alloc_type(Type::Int(Int::I32)),
        ),
        (
            Some(asm.alloc_string("arr_ptr")),
            asm.alloc_type(uint8_ptr_ptr),
        ),
        (
            Some(asm.alloc_string("idx")),
            asm.alloc_type(Type::Int(Int::I32)),
        ),
        (
            Some(asm.alloc_string("iter")),
            asm.alloc_type(Type::ClassRef(dictionary_iterator)),
        ),
        (
            Some(asm.alloc_string("keyval")),
            asm.alloc_type(Type::ClassRef(keyval_tpe)),
        ),
        (
            Some(asm.alloc_string("encoded_keyval")),
            asm.alloc_type(Type::ClassRef(string_class)),
        ),
    ];
    // Block ids (mirror the `new_bb` order): 0 first_check, 1 init, 2 loop_body, 3 loop_end, 4 ret.
    let first_check_bb: u32 = 0;
    let init_bb: u32 = 1;
    let loop_body_bb: u32 = 2;
    let loop_end_bb: u32 = 3;
    let ret_bb: u32 = 4;
    // Environ static field load.
    let environ_descr = StaticFieldDesc::new(
        *asm.main_module(),
        asm.alloc_string("environ"),
        uint8_ptr_ptr,
    );
    let environ_fld = asm.alloc_sfld(environ_descr);
    let environ = asm.alloc_node(CILNode::LdStaticField(environ_fld));

    // ---- first_check block ----
    let mut first_check_roots = Vec::new();
    let zero_i32 = asm.alloc_node(0_i32);
    let zero_usize = asm.int_cast(zero_i32, Int::USize, ExtendKind::ZeroExtend);
    // `cast_ptr(x, T)` yields `Ptr(T)`, so to get a null `uint8**` (matching the
    // `environ` static) the pointee must be `uint8*` (`uint8_ptr`), not the full
    // `uint8**` (`uint8_ptr_ptr`). Passing the latter over-pointers to `uint8***`,
    // which made the null check compare `Ptr(uint8*)` against `Ptr(uint8**)` and
    // tripped the `CantCompareTypes` typecheck (the comparison is a plain null check).
    let zero_ptr = asm.cast_ptr(zero_usize, uint8_ptr);
    first_check_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
        ret_bb,
        0,
        Some(BranchCond::Ne(environ, zero_ptr)),
    )))));
    first_check_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((init_bb, 0, None)))));

    // ---- init block ----
    let mut init_roots = Vec::new();
    let i_dictionary = Type::ClassRef(i_dictionary_class);
    let mref = MethodRef::new(
        ClassRef::enviroment(asm),
        asm.alloc_string("GetEnvironmentVariables"),
        asm.sig([], i_dictionary),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    let call = asm.call(mref, &[] as &[Interned<CILNode>], IsPure::NOT);
    init_roots.push(asm.alloc_root(CILRoot::StLoc(dictionary_local, call)));
    let mref = MethodRef::new(
        ClassRef::i_collection(asm),
        asm.alloc_string("get_Count"),
        asm.sig([i_dictionary], Type::Int(Int::I32)),
        MethodKind::Virtual,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    let ld_dict = asm.ld_loc(dictionary_local);
    let call = asm.call(mref, &[ld_dict], IsPure::NOT);
    init_roots.push(asm.alloc_root(CILRoot::StLoc(envc, call)));
    // arr_size = zext_usize(envc + 1) * stride
    let ld_envc = asm.ld_loc(envc);
    let one_i32 = asm.alloc_node(1_i32);
    let element_count = asm.biop(ld_envc, one_i32, BinOp::Add);
    let element_count = asm.int_cast(element_count, Int::USize, ExtendKind::ZeroExtend);
    let stride = asm.size_of(uint8_ptr_ptr);
    let stride = asm.int_cast(stride, Int::USize, ExtendKind::ZeroExtend);
    let arr_size = asm.biop(element_count, stride, BinOp::Mul);
    let arr_align = asm.size_of(uint8_ptr_ptr);
    let arr_align = asm.int_cast(arr_align, Int::USize, ExtendKind::ZeroExtend);
    let aligned_alloc = MethodRef::aligned_alloc(asm);
    let aligned_alloc = asm.alloc_methodref(aligned_alloc);
    let alloc_call = asm.call(aligned_alloc, &[arr_size, arr_align], IsPure::NOT);
    // `cast_ptr(x, T)` yields `Ptr(T)`. `arr_ptr` is `uint8**`, so the pointee here is
    // `uint8*` (`uint8_ptr`); passing the full `uint8**` would over-pointer to
    // `uint8***` and fail the `StLoc(arr_ptr, …)` assignability check.
    let alloc_call = asm.cast_ptr(alloc_call, uint8_ptr);
    init_roots.push(asm.alloc_root(CILRoot::StLoc(arr_ptr, alloc_call)));
    let zero_i32 = asm.alloc_node(0_i32);
    init_roots.push(asm.alloc_root(CILRoot::StLoc(idx, zero_i32)));
    let mref = MethodRef::new(
        i_dictionary_class,
        asm.alloc_string("GetEnumerator"),
        asm.sig([i_dictionary], Type::ClassRef(dictionary_iterator)),
        MethodKind::Virtual,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    let ld_dict = asm.ld_loc(dictionary_local);
    let call = asm.call(mref, &[ld_dict], IsPure::NOT);
    init_roots.push(asm.alloc_root(CILRoot::StLoc(iter_local, call)));
    init_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((loop_body_bb, 0, None)))));

    // ---- ret block ----
    let ret_roots = vec![asm.alloc_root(CILRoot::Ret(environ))];

    // ---- loop_body block ----
    let mut loop_body_roots = Vec::new();
    let move_next = MethodRef::new(
        ClassRef::i_enumerator(asm),
        asm.alloc_string("MoveNext"),
        asm.sig([Type::ClassRef(dictionary_iterator)], Type::Bool),
        MethodKind::Virtual,
        vec![].into(),
    );
    let move_next = asm.alloc_methodref(move_next);
    let ld_iter = asm.ld_loc(iter_local);
    let move_next_call = asm.call(move_next, &[ld_iter], IsPure::NOT);
    loop_body_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
        loop_end_bb,
        0,
        Some(BranchCond::False(move_next_call)),
    )))));
    let get_current = MethodRef::new(
        ClassRef::i_enumerator(asm),
        asm.alloc_string("get_Current"),
        asm.sig([Type::ClassRef(dictionary_iterator)], Type::PlatformObject),
        MethodKind::Virtual,
        vec![].into(),
    );
    let get_current = asm.alloc_methodref(get_current);
    let ld_iter = asm.ld_loc(iter_local);
    let cur = asm.call(get_current, &[ld_iter], IsPure::NOT);
    let keyval_ty = asm.alloc_type(Type::ClassRef(keyval_tpe));
    let unboxed = asm.unbox_any(cur, keyval_ty);
    loop_body_roots.push(asm.alloc_root(CILRoot::StLoc(keyval, unboxed)));
    let keyval_tpe_ref = asm.nref(Type::ClassRef(keyval_tpe));
    let sig = asm.sig([keyval_tpe_ref], Type::PlatformObject);
    let get_key = MethodRef::new(
        keyval_tpe,
        asm.alloc_string("get_Key"),
        sig,
        MethodKind::Instance,
        vec![].into(),
    );
    let get_key = asm.alloc_methodref(get_key);
    let kv = asm.alloc_node(CILNode::LdLocA(keyval));
    let key = asm.call(get_key, &[kv], IsPure::NOT);
    let get_value = MethodRef::new(
        keyval_tpe,
        asm.alloc_string("get_Value"),
        sig,
        MethodKind::Instance,
        vec![].into(),
    );
    let get_value = asm.alloc_methodref(get_value);
    let kv = asm.alloc_node(CILNode::LdLocA(keyval));
    let value = asm.call(get_value, &[kv], IsPure::NOT);
    let concat = MethodRef::new(
        string_class,
        asm.alloc_string("Concat"),
        asm.sig(
            [
                Type::PlatformObject,
                Type::PlatformObject,
                Type::PlatformObject,
            ],
            Type::PlatformString,
        ),
        MethodKind::Static,
        vec![].into(),
    );
    let concat = asm.alloc_methodref(concat);
    let eq_str = asm.alloc_string("=");
    let eq_node = asm.alloc_node(crate::Const::PlatformString(eq_str));
    let concat_call = asm.call(concat, &[key, eq_node, value], IsPure::NOT);
    loop_body_roots.push(asm.alloc_root(CILRoot::StLoc(encoded_keyval, concat_call)));
    let ld_encoded = asm.ld_loc(encoded_keyval);
    let utf8_kval = mstring_to_utf8ptr(ld_encoded, asm);
    // addr = arr_ptr + zext_usize(idx * size_of(uint8_ptr_ptr))
    let ld_arr_ptr = asm.ld_loc(arr_ptr);
    let ld_idx = asm.ld_loc(idx);
    let stride_n = asm.size_of(uint8_ptr_ptr);
    let mul = asm.biop(ld_idx, stride_n, BinOp::Mul);
    let mul = asm.int_cast(mul, Int::USize, ExtendKind::ZeroExtend);
    let addr = asm.biop(ld_arr_ptr, mul, BinOp::Add);
    let u8_ptr_ty = asm.nptr(Type::Int(Int::U8));
    loop_body_roots.push(asm.alloc_root(CILRoot::StInd(Box::new((
        addr, utf8_kval, u8_ptr_ty, false,
    )))));
    let ld_idx = asm.ld_loc(idx);
    let one_i32 = asm.alloc_node(1_i32);
    let inc = asm.biop(ld_idx, one_i32, BinOp::Add);
    loop_body_roots.push(asm.alloc_root(CILRoot::StLoc(idx, inc)));
    loop_body_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((loop_body_bb, 0, None)))));

    // ---- loop_end block ----
    let mut loop_end_roots = Vec::new();
    let zero_i32 = asm.alloc_node(0_i32);
    let zero_usize = asm.int_cast(zero_i32, Int::USize, ExtendKind::ZeroExtend);
    // The array slots hold `uint8*` (the array itself is `uint8**`), and this writes
    // the NULL terminator into a slot. `cast_ptr(x, T)` yields `Ptr(T)`, so the
    // pointee for a `uint8*` null is `u8`, not `uint8_ptr` (which would over-pointer
    // to `uint8**`, mismatching the `u8_ptr_ty` store type used in the StInd below).
    let null_ptr = asm.cast_ptr(zero_usize, Type::Int(Int::U8));
    // addr = arr_ptr + zext_usize(envc * size_of(uint8_ptr_ptr))
    let ld_arr_ptr = asm.ld_loc(arr_ptr);
    let ld_envc = asm.ld_loc(envc);
    let stride_n = asm.size_of(uint8_ptr_ptr);
    let mul = asm.biop(ld_envc, stride_n, BinOp::Mul);
    let mul = asm.int_cast(mul, Int::USize, ExtendKind::ZeroExtend);
    let addr = asm.biop(ld_arr_ptr, mul, BinOp::Add);
    let u8_ptr_ty = asm.nptr(Type::Int(Int::U8));
    loop_end_roots
        .push(asm.alloc_root(CILRoot::StInd(Box::new((addr, null_ptr, u8_ptr_ty, false)))));
    let environ_descr2 = StaticFieldDesc::new(
        *asm.main_module(),
        asm.alloc_string("environ"),
        uint8_ptr_ptr,
    );
    let environ_fld2 = asm.alloc_sfld(environ_descr2);
    let ld_arr_ptr = asm.ld_loc(arr_ptr);
    loop_end_roots.push(asm.alloc_root(CILRoot::SetStaticField {
        field: environ_fld2,
        val: ld_arr_ptr,
    }));

    // Blocks must be in id order: first_check(0), init(1), loop_body(2), loop_end(3), ret(4).
    let blocks = vec![
        BasicBlock::new(first_check_roots, first_check_bb, None),
        BasicBlock::new(init_roots, init_bb, None),
        BasicBlock::new(loop_body_roots, loop_body_bb, None),
        BasicBlock::new(loop_end_roots, loop_end_bb, None),
        BasicBlock::new(ret_roots, ret_bb, None),
    ];
    let sig = asm.alloc_sig(FnSig::new([], uint8_ptr_ptr));
    let def = MethodDef::from_blocks(
        crate::Access::Extern,
        main_module,
        "get_environ",
        sig,
        MethodKind::Static,
        blocks,
        locals,
        vec![],
        asm,
    );
    asm.new_method(def);
    asm.add_static(uint8_ptr_ptr, "environ", true, main_module, None, false);
    init_cs
}
static CHARS: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i',
    'j', 'k', 'l', 'm', 'n', 'o', 'p', 'r', 's', 't', 'u', 'w', 'v', 'x', 'y', 'z', 'A', 'B', 'C',
    'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'R', 'S', 'T', 'U', 'W', 'V',
    'X', 'Y', 'Z', '_',
];
pub fn encode(mut int: u64) -> String {
    let mut res = String::new();
    while int != 0 {
        let curr = int % (CHARS.len() as u64);
        res.push(CHARS[curr as usize]);
        int /= CHARS.len() as u64;
    }
    res
}
/// Checks if all elements in a slice are truly unquie.
#[track_caller]
#[cfg_attr(not(debug_assertions), allow(unused_variables))] // `val`/`msg` are only read under the debug-only assert below.
pub fn assert_unique<T: std::hash::Hash + PartialEq + Eq>(val: &[T], msg: impl Debug) {
    #[cfg(debug_assertions)]
    {
        let mut set = std::collections::HashSet::new();
        set.extend(val.iter());
        assert_eq!(set.len(), val.len(), "{msg:?}");
    }
}
#[must_use]
pub fn escape_class_name(name: &str) -> String {
    name.replace("::", ".")
        .replace("..", ".")
        .replace('$', "_dsig_")
        .replace('<', "_lt_")
        .replace('\'', "_ap_")
        .replace(' ', "_spc_")
        .replace('>', "_gt_")
        .replace('(', "_lpar_")
        .replace(')', "_rpar")
        .replace('{', "_lbra_")
        .replace('}', "_rbra")
        .replace('[', "_lsbra_")
        .replace(']', "_rsbra_")
        .replace('+', "_pls_")
        .replace('-', "_hyp_")
        .replace(',', "_com_")
        .replace('*', "_ptr_")
        .replace('#', "_hsh_")
        .replace('&', "_ref_")
        .replace(';', "_scol_")
        .replace('!', "_excl_")
        .replace('\"', "_qt_")
}
/*
#[test]
fn argv() {
    let mut asm = Assembly::empty();
    argc_argv_init_method(&mut asm);
} */

#[test]
fn environ() {
    let mut asm = Assembly::default();
    get_environ(&mut asm);
}
#[test]
fn environ_serde_roundtrip() {
    let mut asm = Assembly::default();
    let _ = get_environ(&mut asm);
    let asm = asm.prepared();
    let bytes = postcard::to_stdvec(&asm).expect("serialize");
    let _decoded: Assembly = postcard::from_bytes(&bytes).expect("deserialize");
}
#[test]
fn test_escape_name() {
    assert_eq!(escape_class_name("SomeFunnyType"), "SomeFunnyType");
    assert_eq!(
        escape_class_name("MyNamespace::SomeFunnyType"),
        "MyNamespace.SomeFunnyType"
    );
    assert_eq!(
        escape_class_name("MyNamespace..SomeFunnyType"),
        "MyNamespace.SomeFunnyType"
    );
    assert_eq!(
        escape_class_name("SomeFunnyType<[Inner]>"),
        "SomeFunnyType_lt__lsbra_Inner_rsbra__gt_"
    );
}
