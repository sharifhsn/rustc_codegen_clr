use cilly::{
    Interned, MethodRef, Type,
    cilnode::{IsPure, MethodKind},
    {Assembly, ClassRef, Int},
};

type Node = Interned<cilly::ir::CILNode>;

pub fn atomic_add(addr: Node, addend: Node, tpe: Type, asm: &mut Assembly) -> Node {
    match tpe {
        Type::Int(int) => {
            let int_ref = asm.nref(Type::Int(int));
            // MIR atomic intrinsics expose their destination as `*mut T`, while the shared
            // Interlocked-style builtin signature uses a managed byref (`T&`). The bits are the
            // same address, but the verifier-visible boundary must be adapted explicitly.
            let addr = asm.cast_ptr_to(addr, int_ref);
            asm.call_static(
                &format!("atomic_add_{int}", int = int.name()),
                [int_ref, Type::Int(int)],
                Type::Int(int),
                &[addr, addend],
            )
        }

        Type::Ptr(_) => {
            let usize_ref = asm.nref(Type::Int(Int::USize));
            let mref = asm.static_mref(
                "atomic_add_usize",
                [usize_ref, Type::Int(Int::USize)],
                Type::Int(Int::USize),
            );
            let arg0 = asm.cast_ptr_to(addr, usize_ref);
            let arg1 = asm.cast_ptr_to(addend, Type::Int(Int::USize));
            let call = asm.call(mref, &[arg0, arg1], IsPure::NOT);
            asm.cast_ptr_to(call, tpe)
        }

        _ => todo!("Can't atomic add {tpe:?}"),
    }
}
pub fn atomic_or(addr: Node, addend: Node, tpe: Type, asm: &mut Assembly) -> Node {
    match tpe {
        Type::Int(Int::U64 | Int::I64) => {
            let u64_ref = asm.nref(Type::Int(Int::U64));
            let mref = MethodRef::new(
                ClassRef::interlocked(asm),
                asm.alloc_string("Or"),
                asm.sig([u64_ref, Type::Int(Int::U64)], Type::Int(Int::U64)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[addr, addend], IsPure::NOT)
        }
        Type::Int(Int::U32 | Int::I32) => {
            let u32_ref = asm.nref(Type::Int(Int::U32));
            let mref = MethodRef::new(
                ClassRef::interlocked(asm),
                asm.alloc_string("Or"),
                asm.sig([u32_ref, Type::Int(Int::U32)], Type::Int(Int::U32)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[addr, addend], IsPure::NOT)
        }
        Type::Int(Int::ISize | Int::USize | Int::U8 | Int::I8) | Type::Bool => {
            let int_ref = asm.nref(tpe);
            let name = format!("atomic_or_{}", tpe.mangle(asm));
            asm.call_static(&name, [int_ref, tpe], tpe, &[addr, addend])
        }

        Type::Ptr(inner) => {
            let int = Int::USize;
            let int_ref = asm.nref(Type::Int(int));
            let mref = asm.static_mref(
                &format!("atomic_or_{}", int.name()),
                [int_ref, Type::Int(int)],
                Type::Int(int),
            );
            let usize_ref = asm.nref(Type::Int(Int::USize));
            let arg0 = asm.cast_ptr_to(addr, usize_ref);
            let arg1 = asm.cast_ptr_to(addend, Type::Int(Int::USize));
            let cilnode = asm.call(mref, &[arg0, arg1], IsPure::NOT);
            let cilnode = asm.cast_ptr_to(cilnode, Type::Ptr(inner));
            asm.cast_ptr_to(cilnode, tpe)
        }
        _ => todo!("Can't atomic or {tpe:?}"),
    }
}
pub fn atomic_xor(addr: Node, addend: Node, tpe: Type, asm: &mut Assembly) -> Node {
    match tpe {
        Type::Bool
        | Type::Int(
            Int::U8 | Int::I8 | Int::U32 | Int::I32 | Int::U64 | Int::I64 | Int::USize | Int::ISize,
        ) => {
            let iref = asm.nref(tpe);
            let name = format!("atomic_xor_{}", tpe.mangle(asm));
            asm.call_static(&name, [iref, tpe], tpe, &[addr, addend])
        }

        Type::Ptr(inner) => {
            let int = Int::USize;
            let iref = asm.nref(Type::Int(int));
            let mref = asm.static_mref(
                &format!("atomic_xor_{}", int.name()),
                [iref, Type::Int(int)],
                Type::Int(int),
            );
            let usize_ref = asm.nref(Type::Int(Int::USize));
            let arg0 = asm.cast_ptr_to(addr, usize_ref);
            let arg1 = asm.cast_ptr_to(addend, Type::Int(Int::USize));
            let call = asm.call(mref, &[arg0, arg1], IsPure::NOT);
            asm.cast_ptr_to(call, Type::Ptr(inner))
        }
        _ => todo!("Can't atomic xor {tpe:?}"),
    }
}
pub fn atomic_and(addr: Node, addend: Node, tpe: Type, asm: &mut Assembly) -> Node {
    match tpe {
        Type::Int(Int::U64 | Int::I64) => {
            let u64_ref = asm.nref(Type::Int(Int::U64));
            let mref = MethodRef::new(
                ClassRef::interlocked(asm),
                asm.alloc_string("And"),
                asm.sig([u64_ref, Type::Int(Int::U64)], Type::Int(Int::U64)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[addr, addend], IsPure::NOT)
        }
        Type::Int(Int::U32 | Int::I32) => {
            let u32_ref = asm.nref(Type::Int(Int::U32));
            let mref = MethodRef::new(
                ClassRef::interlocked(asm),
                asm.alloc_string("And"),
                asm.sig([u32_ref, Type::Int(Int::U32)], Type::Int(Int::U32)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[addr, addend], IsPure::NOT)
        }
        Type::Int(Int::USize) => {
            let usize_ref = asm.nref(Type::Int(Int::USize));
            asm.call_static(
                "atomic_and_usize",
                [usize_ref, Type::Int(Int::USize)],
                Type::Int(Int::USize),
                &[addr, addend],
            )
        }
        Type::Int(Int::ISize) => {
            let usize_ref = asm.nref(Type::Int(Int::USize));
            let mref = asm.static_mref(
                "atomic_and_usize",
                [usize_ref, Type::Int(Int::USize)],
                Type::Int(Int::USize),
            );
            let usize_ref2 = asm.nref(Type::Int(Int::USize));
            let arg0 = asm.cast_ptr_to(addr, usize_ref2);
            let arg1 = asm.cast_ptr_to(addend, Type::Int(Int::USize));
            let cilnode = asm.call(mref, &[arg0, arg1], IsPure::NOT);
            asm.cast_ptr_to(cilnode, Type::Int(Int::ISize))
        }
        Type::Ptr(inner) => {
            let usize_ref = asm.nref(Type::Int(Int::USize));
            let mref = asm.static_mref(
                "atomic_and_usize",
                [usize_ref, Type::Int(Int::USize)],
                Type::Int(Int::USize),
            );
            let usize_ref2 = asm.nref(Type::Int(Int::USize));
            let arg0 = asm.cast_ptr_to(addr, usize_ref2);
            let arg1 = asm.cast_ptr_to(addend, Type::Int(Int::USize));
            let cilnode = asm.call(mref, &[arg0, arg1], IsPure::NOT);
            asm.cast_ptr_to(cilnode, Type::Ptr(inner))
        }
        Type::Bool | Type::Int(Int::U8 | Int::I8) => {
            let iref = asm.nref(tpe);
            let name = format!("atomic_and_{}", tpe.mangle(asm));
            asm.call_static(&name, [iref, tpe], tpe, &[addr, addend])
        }
        _ => todo!("Can't atomic and {tpe:?}"),
    }
}
pub fn compare_bytes(a: Node, b: Node, len: Node, asm: &mut Assembly) -> Node {
    let u8_ref = asm.nptr(Type::Int(Int::U8));
    asm.call_static(
        "memcmp",
        [u8_ref, u8_ref, Type::Int(Int::USize)],
        Type::Int(Int::I32),
        &[a, b, len],
    )
}
pub fn atomic_nand(addr: Node, addend: Node, tpe: Type, asm: &mut Assembly) -> Node {
    match tpe {
        Type::Int(int @ (Int::U32 | Int::I32 | Int::U64 | Int::I64 | Int::USize | Int::ISize)) => {
            let iref = asm.nref(Type::Int(int));
            asm.call_static(
                &format!("atomic_nand_{}", int.name()),
                [iref, Type::Int(int)],
                Type::Int(int),
                &[addr, addend],
            )
        }
        Type::Ptr(inner) => {
            let int = Int::USize;
            let iref = asm.nref(Type::Int(int));
            let mref = asm.static_mref(
                &format!("atomic_nand_{}", int.name()),
                [iref, Type::Int(int)],
                Type::Int(int),
            );
            let usize_ref = asm.nref(Type::Int(Int::USize));
            let arg0 = asm.cast_ptr_to(addr, usize_ref);
            let arg1 = asm.cast_ptr_to(addend, Type::Int(Int::USize));
            let call = asm.call(mref, &[arg0, arg1], IsPure::NOT);
            asm.cast_ptr_to(call, Type::Ptr(inner))
        }
        Type::Bool | Type::Int(Int::U8 | Int::I8) => {
            let iref = asm.nref(tpe);
            let name = format!("atomic_nand_{}", tpe.mangle(asm));
            asm.call_static(&name, [iref, tpe], tpe, &[addr, addend])
        }
        _ => todo!("Can't atomic nand {tpe:?}"),
    }
}
pub fn atomic_min(addr: Node, addend: Node, tpe: Type, asm: &mut Assembly) -> Node {
    match tpe {
        Type::Bool
        | Type::Int(
            Int::U8 | Int::I8 | Int::U32 | Int::I32 | Int::U64 | Int::I64 | Int::USize | Int::ISize,
        ) => {
            let iref = asm.nref(tpe);
            let name = format!("atomic_min_{}", tpe.mangle(asm));
            asm.call_static(&name, [iref, tpe], tpe, &[addr, addend])
        }
        Type::Ptr(inner) => {
            let int = Int::USize;
            let iref = asm.nref(Type::Int(int));
            let mref = asm.static_mref(
                &format!("atomic_min_{}", int.name()),
                [iref, Type::Int(int)],
                Type::Int(int),
            );
            let usize_ref = asm.nref(Type::Int(Int::USize));
            let arg0 = asm.cast_ptr_to(addr, usize_ref);
            let arg1 = asm.cast_ptr_to(addend, Type::Int(Int::USize));
            let call = asm.call(mref, &[arg0, arg1], IsPure::NOT);
            asm.cast_ptr_to(call, Type::Ptr(inner))
        }
        _ => todo!("Can't atomic min {tpe:?}"),
    }
}
pub fn atomic_max(addr: Node, addend: Node, tpe: Type, asm: &mut Assembly) -> Node {
    match tpe {
        Type::Bool
        | Type::Int(
            Int::U8 | Int::I8 | Int::U32 | Int::I32 | Int::U64 | Int::I64 | Int::USize | Int::ISize,
        ) => {
            let iref = asm.nref(tpe);
            let name = format!("atomic_max_{}", tpe.mangle(asm));
            asm.call_static(&name, [iref, tpe], tpe, &[addr, addend])
        }
        Type::Ptr(inner) => {
            let int = Int::USize;
            let iref = asm.nref(Type::Int(int));
            let mref = asm.static_mref(
                &format!("atomic_max_{}", int.name()),
                [iref, Type::Int(int)],
                Type::Int(int),
            );
            let usize_ref = asm.nref(Type::Int(Int::USize));
            let arg0 = asm.cast_ptr_to(addr, usize_ref);
            let arg1 = asm.cast_ptr_to(addend, Type::Int(Int::USize));
            let call = asm.call(mref, &[arg0, arg1], IsPure::NOT);
            asm.cast_ptr_to(call, Type::Ptr(inner))
        }
        _ => todo!("Can't atomic max {tpe:?}"),
    }
}
