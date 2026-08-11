use cilly::{
    Assembly, ClassRef, Int, Interned, MethodRef, Type,
    cilnode::{IsPure, MethodKind},
};

type Node = Interned<cilly::ir::CILNode>;

#[derive(Clone, Copy)]
enum AtomicRmwOp {
    Add,
    Or,
    Xor,
    And,
    Nand,
    Min,
    Max,
}

impl AtomicRmwOp {
    const fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::And => "and",
            Self::Nand => "nand",
            Self::Min => "min",
            Self::Max => "max",
        }
    }

    const fn direct_interlocked_name(self) -> Option<&'static str> {
        match self {
            Self::Or => Some("Or"),
            Self::And => Some("And"),
            Self::Add | Self::Xor | Self::Nand | Self::Min | Self::Max => None,
        }
    }

    const fn supports_bool(self) -> bool {
        !matches!(self, Self::Add)
    }
}

/// Lowers one integer read-modify-write operation through the same operation/type matrix that
/// cilly registers in `builtins::atomics`. The only direct BCL fast path is an exact .NET 10
/// `Interlocked.And`/`Or` overload; all other widths and operations use the generated CAS loop.
fn atomic_rmw(addr: Node, operand: Node, tpe: Type, op: AtomicRmwOp, asm: &mut Assembly) -> Node {
    match tpe {
        Type::Int(int) => {
            let int_type = Type::Int(int);
            let int_ref = asm.nref(int_type);
            let addr = asm.cast_ptr_to(addr, int_ref);

            if matches!(int, Int::U32 | Int::I32 | Int::U64 | Int::I64)
                && crate::config::native_subword_atomics()
                && let Some(method) = op.direct_interlocked_name()
            {
                let call_site = MethodRef::new(
                    ClassRef::interlocked(asm),
                    asm.alloc_string(method),
                    asm.sig([int_ref, int_type], int_type),
                    MethodKind::Static,
                    vec![].into(),
                );
                let call_site = asm.alloc_methodref(call_site);
                return asm.call(call_site, &[addr, operand], IsPure::NOT);
            }

            asm.call_static(
                &format!("atomic_{}_{}", op.name(), int.name()),
                [int_ref, int_type],
                int_type,
                &[addr, operand],
            )
        }
        Type::Bool if op.supports_bool() => {
            let u8_type = Type::Int(Int::U8);
            let u8_ref = asm.nref(u8_type);
            let addr = asm.cast_ptr_to(addr, u8_ref);
            let operand = asm.transmute_on_stack(Type::Bool, u8_type, operand);
            let call = asm.call_static(
                &format!("atomic_{}_{}", op.name(), Int::U8.name()),
                [u8_ref, u8_type],
                u8_type,
                &[addr, operand],
            );
            asm.transmute_on_stack(u8_type, Type::Bool, call)
        }
        Type::Ptr(_) => {
            let usize_type = Type::Int(Int::USize);
            let usize_ref = asm.nref(usize_type);
            let call_site = asm.static_mref(
                &format!("atomic_{}_{}", op.name(), Int::USize.name()),
                [usize_ref, usize_type],
                usize_type,
            );
            let addr = asm.cast_ptr_to(addr, usize_ref);
            let operand = asm.cast_ptr_to(operand, usize_type);
            let call = asm.call(call_site, &[addr, operand], IsPure::NOT);
            asm.cast_ptr_to(call, tpe)
        }
        _ => todo!("Can't atomic {} {tpe:?}", op.name()),
    }
}

pub fn atomic_add(addr: Node, addend: Node, tpe: Type, asm: &mut Assembly) -> Node {
    atomic_rmw(addr, addend, tpe, AtomicRmwOp::Add, asm)
}

pub fn atomic_or(addr: Node, operand: Node, tpe: Type, asm: &mut Assembly) -> Node {
    atomic_rmw(addr, operand, tpe, AtomicRmwOp::Or, asm)
}

pub fn atomic_xor(addr: Node, operand: Node, tpe: Type, asm: &mut Assembly) -> Node {
    atomic_rmw(addr, operand, tpe, AtomicRmwOp::Xor, asm)
}

pub fn atomic_and(addr: Node, operand: Node, tpe: Type, asm: &mut Assembly) -> Node {
    atomic_rmw(addr, operand, tpe, AtomicRmwOp::And, asm)
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

pub fn atomic_nand(addr: Node, operand: Node, tpe: Type, asm: &mut Assembly) -> Node {
    atomic_rmw(addr, operand, tpe, AtomicRmwOp::Nand, asm)
}

pub fn atomic_min(addr: Node, operand: Node, tpe: Type, asm: &mut Assembly) -> Node {
    atomic_rmw(addr, operand, tpe, AtomicRmwOp::Min, asm)
}

pub fn atomic_max(addr: Node, operand: Node, tpe: Type, asm: &mut Assembly) -> Node {
    atomic_rmw(addr, operand, tpe, AtomicRmwOp::Max, asm)
}
