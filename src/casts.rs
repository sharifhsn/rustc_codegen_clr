use cilly::cilnode::{ExtendKind, IsPure, MethodKind};
use cilly::Type;
use cilly::{Assembly, ClassRef, Float, Int, Interned, MethodRef};

type Node = Interned<cilly::ir::CILNode>;

/// Casts from intiger type `src` to target `target`
pub fn int_to_int(src: Type, target: Type, operand: Node, asm: &mut Assembly) -> Node {
    if src == target {
        return operand;
    }
    match (&src, &target) {
        // Unsinged casts are special
        (
            Type::Int(Int::U32 | Int::U16 | Int::U8 | Int::U64 | Int::USize),
            Type::Int(Int::ISize),
        ) => {
            let us = asm.int_cast(operand, Int::USize, ExtendKind::ZeroExtend);
            asm.int_cast(us, Int::ISize, ExtendKind::SignExtend)
        }
        (Type::Int(Int::U32 | Int::U16 | Int::U8 | Int::U64 | Int::USize), Type::Int(Int::I64)) => {
            let u = asm.int_cast(operand, Int::U64, ExtendKind::ZeroExtend);
            asm.int_cast(u, Int::I64, ExtendKind::SignExtend)
        }
        (Type::Int(Int::U32 | Int::U16 | Int::U8 | Int::U64 | Int::USize), Type::Int(Int::I32)) => {
            let u = asm.int_cast(operand, Int::U32, ExtendKind::ZeroExtend);
            asm.int_cast(u, Int::I32, ExtendKind::SignExtend)
        }
        (Type::Int(Int::U32 | Int::U16 | Int::U8 | Int::U64 | Int::USize), Type::Int(Int::I16)) => {
            let u = asm.int_cast(operand, Int::U16, ExtendKind::ZeroExtend);
            asm.int_cast(u, Int::I16, ExtendKind::SignExtend)
        }
        (Type::Int(Int::U32 | Int::U16 | Int::U8 | Int::U64 | Int::USize), Type::Int(Int::I8)) => {
            let u = asm.int_cast(operand, Int::U8, ExtendKind::ZeroExtend);
            asm.int_cast(u, Int::I8, ExtendKind::SignExtend)
        }
        //
        (Type::Int(Int::ISize), Type::Int(Int::I128)) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("op_Implicit"),
                asm.sig([Type::Int(Int::ISize)], Type::Int(Int::I128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        (Type::Int(Int::U32), Type::Int(Int::I128)) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("op_Implicit"),
                asm.sig([Type::Int(Int::U32)], Type::Int(Int::I128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        (Type::Int(Int::ISize), Type::Int(Int::U128)) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([Type::Int(Int::I64)], Type::Int(Int::U128)),
                MethodKind::Static,
                vec![].into(),
            );
            let arg = asm.int_cast(operand, Int::I64, ExtendKind::SignExtend);
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[arg], IsPure::NOT)
        }
        (Type::Bool, Type::Int(Int::U128)) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([Type::Int(Int::I32)], Type::Int(Int::U128)),
                MethodKind::Static,
                vec![].into(),
            );
            let arg = asm.int_cast(operand, Int::I32, ExtendKind::SignExtend);
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[arg], IsPure::NOT)
        }
        (Type::Bool, Type::Int(Int::I128)) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("op_Implicit"),
                asm.sig([Type::Int(Int::I32)], Type::Int(Int::I128)),
                MethodKind::Static,
                vec![].into(),
            );
            let arg = asm.int_cast(operand, Int::I32, ExtendKind::SignExtend);
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[arg], IsPure::NOT)
        }
        // Fixes sign casts
        (Type::Int(Int::I64 | Int::I32 | Int::I16 | Int::I8), Type::Int(Int::USize)) => {
            asm.int_cast(operand, Int::USize, ExtendKind::SignExtend)
        }
        (Type::Int(Int::I64 | Int::I32 | Int::I16 | Int::I8), Type::Int(Int::U64)) => {
            asm.int_cast(operand, Int::U64, ExtendKind::SignExtend)
        }
        // i128 bit casts
        (Type::Int(Int::U128), Type::Int(Int::I128))
        | (Type::Int(Int::I8 | Int::I16 | Int::I32 | Int::I64), Type::Int(Int::U128)) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([src], target),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        // pointer -> 128-bit: cast the pointer to usize first, then widen usize -> 128-bit via
        // op_Explicit (no direct Ptr -> 128 operator exists). Must precede the generic
        // (_, I128) / (_, U128) arms, which would otherwise match a Ptr source and emit a
        // malformed op_Implicit(Ptr) -> 128.
        (Type::Ptr(_), Type::Int(Int::U128)) => {
            let us = asm.int_cast(operand, Int::USize, ExtendKind::ZeroExtend);
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([Type::Int(Int::USize)], Type::Int(Int::U128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[us], IsPure::NOT)
        }
        (Type::Ptr(_), Type::Int(Int::I128)) => {
            let us = asm.int_cast(operand, Int::USize, ExtendKind::ZeroExtend);
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([Type::Int(Int::USize)], Type::Int(Int::I128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[us], IsPure::NOT)
        }
        (_, Type::Int(Int::I128)) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("op_Implicit"),
                asm.sig([src], target),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        (Type::Int(Int::I128), Type::Int(Int::U128)) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([src], target),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        (_, Type::Int(Int::U128)) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("op_Implicit"),
                asm.sig([src], target),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        // 128-bit <-> pointer: there is no op_Explicit(Int128/UInt128) -> Ptr operator in the
        // BCL (nor a C macro), so route through usize: 128-bit -> usize via op_Explicit, then
        // usize -> Ptr via cast_ptr (mirrors `to_int`'s Ptr arm). Must precede the generic
        // (I128, _) / (U128, _) arms below so Ptr targets are caught here.
        (Type::Int(Int::I128), Type::Ptr(tpe)) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([src], Type::Int(Int::USize)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            let us = asm.call(mref, &[operand], IsPure::NOT);
            asm.cast_ptr(us, *tpe)
        }
        (Type::Int(Int::U128), Type::Ptr(tpe)) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([src], Type::Int(Int::USize)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            let us = asm.call(mref, &[operand], IsPure::NOT);
            asm.cast_ptr(us, *tpe)
        }
        (Type::Int(Int::I128), _) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([src], target),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        (Type::Int(Int::U128), _) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([src], target),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        //todo!("Casting to 128 bit intiegers is not supported!"),
        _ => to_int(target, operand, asm),
    }
}
/// Returns CIL ops required to convert type src to target
pub fn float_to_int(src: Type, target: Type, operand: Node, asm: &mut Assembly) -> Node {
    // `f16` has no native CIL float, and no `cast_f16_*` builtins exist; widen f16 -> f32 via
    // `System.Half`'s explicit conversion operator first, then reuse the f32 -> int path.
    if matches!(src, Type::Float(Float::F16)) {
        let as_f32 = cilly::ir::builtins::f16::f16_to_float(asm, operand, Float::F32);
        return float_to_int(Type::Float(Float::F32), target, as_f32, asm);
    }
    if matches!(src, Type::Float(Float::F128)) {
        todo!(
            "f128 -> int casts are unsupported: .NET has no quadruple-precision float type, so this \
             would need softfloat emulation (f128 arithmetic works only in C mode, via libgcc). \
             src:{src:?} target:{target:?}"
        );
    }
    match target {
        Type::Int(Int::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([src], target),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        Type::Int(Int::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(asm),
                asm.alloc_string("op_Explicit"),
                asm.sig([src], target),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = asm.alloc_methodref(mref);
            asm.call(mref, &[operand], IsPure::NOT)
        }
        Type::Int(
            int @ (Int::U8
            | Int::U16
            | Int::U32
            | Int::U64
            | Int::USize
            | Int::ISize
            | Int::I8
            | Int::I16
            | Int::I32
            | Int::I64),
        ) => {
            let name = match (int, src) {
                (Int::U8, Type::Float(Float::F32)) => "cast_f32_u8",
                (Int::U8, Type::Float(Float::F64)) => "cast_f64_u8",
                (Int::U16, Type::Float(Float::F32)) => "cast_f32_u16",
                (Int::U16, Type::Float(Float::F64)) => "cast_f64_u16",
                (Int::U32, Type::Float(Float::F32)) => "cast_f32_u32",
                (Int::U32, Type::Float(Float::F64)) => "cast_f64_u32",
                (Int::U64, Type::Float(Float::F32)) => "cast_f32_u64",
                (Int::U64, Type::Float(Float::F64)) => "cast_f64_u64",
                (Int::USize, Type::Float(Float::F32)) => "cast_f32_usize",
                (Int::USize, Type::Float(Float::F64)) => "cast_f64_usize",
                (Int::ISize, Type::Float(Float::F32)) => "cast_f32_isize",
                (Int::ISize, Type::Float(Float::F64)) => "cast_f64_isize",
                (Int::I8, Type::Float(Float::F32)) => "cast_f32_i8",
                (Int::I8, Type::Float(Float::F64)) => "cast_f64_i8",
                (Int::I16, Type::Float(Float::F32)) => "cast_f32_i16",
                (Int::I16, Type::Float(Float::F64)) => "cast_f64_i16",
                (Int::I32, Type::Float(Float::F32)) => "cast_f32_i32",
                (Int::I32, Type::Float(Float::F64)) => "cast_f64_i32",
                (Int::I64, Type::Float(Float::F32)) => "cast_f32_i64",
                (Int::I64, Type::Float(Float::F64)) => "cast_f64_i64",
                _ => panic!("Non-float type!"),
            };
            asm.call_static(name, [src], Type::Int(int), &[operand])
        }
        _ => to_int(target, operand, asm),
    }

    //call uint64 [System.Runtime]System.Int128::op_Explicit(valuetype [System.Runtime]System.Int128)
    //
}
/// Returns CIL ops required to convert to intiger of type `target`
fn to_int(target: Type, operand: Node, asm: &mut Assembly) -> Node {
    match target {
        Type::Int(Int::I8) => asm.int_cast(operand, Int::I8, ExtendKind::SignExtend),
        Type::Int(Int::U8) => asm.int_cast(operand, Int::U8, ExtendKind::ZeroExtend),
        Type::Int(Int::I16) => asm.int_cast(operand, Int::I16, ExtendKind::SignExtend),
        Type::Int(Int::U16) => asm.int_cast(operand, Int::U16, ExtendKind::ZeroExtend),
        Type::Int(Int::U32) => asm.int_cast(operand, Int::U32, ExtendKind::ZeroExtend),
        Type::Int(Int::I32) => asm.int_cast(operand, Int::I32, ExtendKind::SignExtend),
        Type::Int(Int::I64) => asm.int_cast(operand, Int::I64, ExtendKind::SignExtend),
        Type::Int(Int::U64) => asm.int_cast(operand, Int::U64, ExtendKind::ZeroExtend),
        Type::Int(Int::ISize) => asm.int_cast(operand, Int::ISize, ExtendKind::SignExtend),
        Type::Int(Int::USize) => asm.int_cast(operand, Int::USize, ExtendKind::ZeroExtend),
        Type::Ptr(tpe) => {
            let us = asm.int_cast(operand, Int::USize, ExtendKind::ZeroExtend);
            asm.cast_ptr(us, tpe)
        }
        _ => todo!("Can't cast to {target:?} yet!"),
    }
}
/// Returns CIL ops required to casts from intiger type `src` to `target` MOVE TO CILLY
pub fn int_to_float(src: Type, target: Type, parrent: Node, asm: &mut Assembly) -> Node {
    if matches!(src, Type::Int(Int::I128)) {
        let mref = MethodRef::new(
            ClassRef::int_128(asm),
            asm.alloc_string("op_Explicit"),
            asm.sig([src], target),
            MethodKind::Static,
            vec![].into(),
        );
        let mref = asm.alloc_methodref(mref);
        asm.call(mref, &[parrent], IsPure::NOT)
        //todo!("Casting from 128 bit intiegers is not supported!")
    } else if matches!(src, Type::Int(Int::U128)) {
        let mref = MethodRef::new(
            ClassRef::uint_128(asm),
            asm.alloc_string("op_Explicit"),
            asm.sig([src], target),
            MethodKind::Static,
            vec![].into(),
        );
        let mref = asm.alloc_methodref(mref);
        asm.call(mref, &[parrent], IsPure::NOT)
    } else if matches!(target, Type::Int(Int::I128 | Int::U128)) {
        todo!("Casting to 128 bit intiegers is not supported!")
    } else if matches!(target, Type::Float(Float::F16)) {
        // `f16` has no native CIL float; go int -> f32 first, then narrow f32 -> f16 via
        // `System.Half`'s explicit conversion operators.
        let as_f32 = int_to_float(src, Type::Float(Float::F32), parrent, asm);
        cilly::ir::builtins::f16::float_to_f16(asm, as_f32, Float::F32)
    } else if matches!(target, Type::Float(Float::F128)) {
        todo!(
            "int -> f128 casts are unsupported: .NET has no quadruple-precision float type, so this \
             would need softfloat emulation (f128 arithmetic works only in C mode, via libgcc). \
             src:{src:?} target:{target:?}"
        )
    } else {
        match (&src, &target) {
            (Type::Int(Int::U32 | Int::U64), Type::Float(Float::F32)) => {
                let un = asm.float_cast(parrent, Float::F64, false);
                asm.float_cast(un, Float::F32, true)
            }
            (Type::Int(Int::USize), Type::Float(Float::F32)) => {
                let u = asm.int_cast(parrent, Int::U64, ExtendKind::ZeroExtend);
                let un = asm.float_cast(u, Float::F64, false);
                asm.float_cast(un, Float::F32, true)
            }
            (_, Type::Float(Float::F32)) => asm.float_cast(parrent, Float::F32, true),
            (Type::Int(Int::U32 | Int::U64), Type::Float(Float::F64)) => {
                asm.float_cast(parrent, Float::F64, false)
            }
            (Type::Int(Int::USize), Type::Float(Float::F64)) => {
                let u = asm.int_cast(parrent, Int::U64, ExtendKind::ZeroExtend);
                asm.float_cast(u, Float::F64, false)
            }
            (_, Type::Float(Float::F64)) => asm.float_cast(parrent, Float::F64, true),
            _ => todo!("Can't  cast {src:?} to {target:?} yet!"),
        }
    }
}
