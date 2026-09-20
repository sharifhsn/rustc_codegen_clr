use cilly::Type;
use cilly::cilnode::{ExtendKind, IsPure, MethodKind};
use cilly::{Assembly, Float, Int, Interned};

type Node = Interned<cilly::ir::CILNode>;

fn call_128(
    asm: &mut Assembly,
    int: Int,
    operation: &'static str,
    src: Type,
    target: Type,
    operand: Node,
) -> Node {
    let sig = asm.sig([src], target);
    let class = int.class(asm);
    let method = asm.new_methodref(class, operation, sig, MethodKind::Static, []);
    asm.call(method, &[operand], IsPure::NOT)
}

/// Casts from intiger type `src` to target `target`
pub fn int_to_int(src: Type, target: Type, operand: Node, asm: &mut Assembly) -> Node {
    if src == target {
        return operand;
    }
    match (&src, &target) {
        // Unsigned-to-signed casts must zero-extend to the target width before applying the
        // target's signed interpretation.
        (
            Type::Int(Int::U32 | Int::U16 | Int::U8 | Int::U64 | Int::USize),
            Type::Int(target @ (Int::ISize | Int::I64 | Int::I32 | Int::I16 | Int::I8)),
        ) => {
            let unsigned_target = target.as_unsigned();
            let value = asm.int_cast(operand, unsigned_target, ExtendKind::ZeroExtend);
            asm.int_cast(value, *target, ExtendKind::SignExtend)
        }
        //
        (Type::Int(Int::ISize | Int::U32), Type::Int(Int::I128)) => {
            call_128(asm, Int::I128, "op_Implicit", src, target, operand)
        }
        (Type::Int(Int::ISize), Type::Int(Int::U128)) => {
            let arg = asm.int_cast(operand, Int::I64, ExtendKind::SignExtend);
            call_128(
                asm,
                Int::U128,
                "op_Explicit",
                Type::Int(Int::I64),
                target,
                arg,
            )
        }
        (Type::Bool, Type::Int(Int::U128)) => {
            let arg = asm.int_cast(operand, Int::I32, ExtendKind::SignExtend);
            call_128(
                asm,
                Int::U128,
                "op_Explicit",
                Type::Int(Int::I32),
                target,
                arg,
            )
        }
        (Type::Bool, Type::Int(Int::I128)) => {
            let arg = asm.int_cast(operand, Int::I32, ExtendKind::SignExtend);
            call_128(
                asm,
                Int::I128,
                "op_Implicit",
                Type::Int(Int::I32),
                target,
                arg,
            )
        }
        // Fixes sign casts
        (
            Type::Int(Int::I64 | Int::I32 | Int::I16 | Int::I8),
            Type::Int(target @ (Int::USize | Int::U64)),
        ) => asm.int_cast(operand, *target, ExtendKind::SignExtend),
        // i128 bit casts
        (Type::Int(Int::U128), Type::Int(Int::I128))
        | (Type::Int(Int::I8 | Int::I16 | Int::I32 | Int::I64), Type::Int(Int::U128)) => {
            call_128(asm, Int::U128, "op_Explicit", src, target, operand)
        }
        // pointer -> 128-bit: cast the pointer to usize first, then widen usize -> 128-bit via
        // op_Explicit (no direct Ptr -> 128 operator exists). Must precede the generic
        // (_, I128) / (_, U128) arms, which would otherwise match a Ptr source and emit a
        // malformed op_Implicit(Ptr) -> 128.
        (Type::Ptr(_), Type::Int(int @ (Int::U128 | Int::I128))) => {
            let us = asm.int_cast(operand, Int::USize, ExtendKind::ZeroExtend);
            call_128(asm, *int, "op_Explicit", Type::Int(Int::USize), target, us)
        }
        (_, Type::Int(Int::I128)) => call_128(asm, Int::I128, "op_Implicit", src, target, operand),
        (Type::Int(Int::I128), Type::Int(Int::U128)) => {
            call_128(asm, Int::I128, "op_Explicit", src, target, operand)
        }
        (_, Type::Int(Int::U128)) => call_128(asm, Int::U128, "op_Implicit", src, target, operand),
        // 128-bit <-> pointer: there is no op_Explicit(Int128/UInt128) -> Ptr operator in the
        // BCL (nor a C macro), so route through usize: 128-bit -> usize via op_Explicit, then
        // usize -> Ptr via cast_ptr (mirrors `to_int`'s Ptr arm). Must precede the generic
        // (I128, _) / (U128, _) arms below so Ptr targets are caught here.
        (Type::Int(int @ (Int::I128 | Int::U128)), Type::Ptr(tpe)) => {
            let us = call_128(
                asm,
                *int,
                "op_Explicit",
                src,
                Type::Int(Int::USize),
                operand,
            );
            asm.cast_ptr(us, *tpe)
        }
        (Type::Int(int @ (Int::I128 | Int::U128)), _) => {
            call_128(asm, *int, "op_Explicit", src, target, operand)
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
        Type::Int(int @ (Int::I128 | Int::U128)) => {
            call_128(asm, int, "op_Explicit", src, target, operand)
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
            let prefix = match src {
                Type::Float(Float::F32) => "cast_f32_",
                Type::Float(Float::F64) => "cast_f64_",
                _ => panic!("Non-float type!"),
            };
            let name = format!("{prefix}{}", int.name());
            asm.call_static(&name, [src], Type::Int(int), &[operand])
        }
        _ => to_int(target, operand, asm),
    }

    //call uint64 [System.Runtime]System.Int128::op_Explicit(valuetype [System.Runtime]System.Int128)
    //
}
/// Returns CIL ops required to convert to intiger of type `target`
fn to_int(target: Type, operand: Node, asm: &mut Assembly) -> Node {
    match target {
        Type::Int(
            int @ (Int::I8
            | Int::I16
            | Int::I32
            | Int::I64
            | Int::ISize
            | Int::U8
            | Int::U16
            | Int::U32
            | Int::U64
            | Int::USize),
        ) => asm.int_cast(
            operand,
            int,
            if int.is_signed() {
                ExtendKind::SignExtend
            } else {
                ExtendKind::ZeroExtend
            },
        ),
        Type::Ptr(tpe) => {
            let us = asm.int_cast(operand, Int::USize, ExtendKind::ZeroExtend);
            asm.cast_ptr(us, tpe)
        }
        _ => todo!("Can't cast to {target:?} yet!"),
    }
}
/// Returns CIL ops required to casts from intiger type `src` to `target` MOVE TO CILLY
pub fn int_to_float(src: Type, target: Type, parrent: Node, asm: &mut Assembly) -> Node {
    if matches!(src, Type::Int(Int::I128)) && matches!(target, Type::Float(Float::F32)) {
        asm.call_static("__floattisf", [src], target, &[parrent])
    } else if matches!(src, Type::Int(Int::U128)) && matches!(target, Type::Float(Float::F32)) {
        asm.call_static("__floatuntisf", [src], target, &[parrent])
    } else if let Type::Int(int @ (Int::I128 | Int::U128)) = src {
        call_128(asm, int, "op_Explicit", src, target, parrent)
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
            (Type::Int(Int::U64), Type::Float(Float::F32)) => {
                asm.call_static("__floatundisf", [src], target, &[parrent])
            }
            (Type::Int(Int::USize), Type::Float(Float::F32)) => {
                let u = asm.int_cast(parrent, Int::U64, ExtendKind::ZeroExtend);
                asm.call_static("__floatundisf", [Type::Int(Int::U64)], target, &[u])
            }
            (Type::Int(Int::U32), Type::Float(Float::F32)) => {
                let un = asm.float_cast(parrent, Float::F64, false);
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
