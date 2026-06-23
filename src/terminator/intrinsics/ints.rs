use crate::assembly::MethodCompileCtx;
use cilly::cilnode::{ExtendKind, IsPure};
use cilly::{
    Assembly, BinOp, Int, Interned, Type,
    {cilnode::MethodKind, ClassRef, MethodRef},
};
use rustc_codegen_clr_place::place_set;
use rustc_codegen_clr_type::GetTypeExt;
use rustc_codgen_clr_operand::handle_operand;
use rustc_middle::{
    mir::{Operand, Place},
    ty::Instance,
};
use rustc_span::Spanned;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

fn ctpop_small_int(asm: &mut cilly::Assembly, operand: Node, int: Int) -> Node {
    assert!(int.size().is_none_or(|size| size <= 8));
    let mref = MethodRef::new(
        ClassRef::bit_operations(asm),
        asm.alloc_string("PopCount"),
        asm.sig([Type::Int(int)], Type::Int(Int::I32)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    let call = asm.call(mref, &[operand], IsPure::NOT);
    asm.int_cast(call, Int::U32, ExtendKind::ZeroExtend)
}
pub fn ctpop<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,

    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        1,
        "The intrinsic `ctpop` MUST take in exactly 1 argument!"
    );
    let tpe = ctx.type_from_cache(
        ctx.monomorphize(
            call_instance.args[0]
                .as_type()
                .expect("needs_drop works only on types!"),
        ),
    );
    let operand = handle_operand(&args[0].node, ctx);
    let value = match tpe {
        Type::Int(Int::U64) => ctpop_small_int(ctx, operand, Int::U64),
        Type::Int(Int::I64) => {
            let operand = ctx.int_cast(operand, Int::U64, ExtendKind::ZeroExtend);
            ctpop_small_int(ctx, operand, Int::U64)
        }
        Type::Int(Int::U32) => ctpop_small_int(ctx, operand, Int::U32),
        Type::Int(Int::U8 | Int::U16 | Int::I8 | Int::I16 | Int::I32) => {
            let operand = ctx.int_cast(operand, Int::U32, ExtendKind::ZeroExtend);
            ctpop_small_int(ctx, operand, Int::U32)
        }
        Type::Int(Int::USize) => ctpop_small_int(ctx, operand, Int::USize),
        Type::Int(Int::ISize) => {
            let operand = ctx.int_cast(operand, Int::ISize, ExtendKind::SignExtend);
            ctpop_small_int(ctx, operand, Int::USize)
        }
        Type::Int(Int::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("PopCount"),
                ctx.sig([Type::Int(Int::U128)], Type::Int(Int::U128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let call = ctx.call(mref, &[operand], IsPure::NOT);
            crate::casts::int_to_int(Type::Int(Int::U128), Type::Int(Int::U32), call, ctx)
        }
        Type::Int(Int::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("PopCount"),
                ctx.sig([Type::Int(Int::I128)], Type::Int(Int::I128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let call = ctx.call(mref, &[operand], IsPure::NOT);
            crate::casts::int_to_int(Type::Int(Int::I128), Type::Int(Int::U32), call, ctx)
        }
        _ => todo!("Unsported pop count type {tpe:?}"),
    };
    place_set(destination, value, ctx)
}
pub fn ctlz<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        1,
        "The intrinsic `ctlz` MUST take in exactly 1 argument!"
    );

    let tpe = ctx.type_from_cache(
        ctx.monomorphize(
            call_instance.args[0]
                .as_type()
                .expect("needs_drop works only on types!"),
        ),
    );
    // TODO: this assumes a 64 bit system!
    let sub = match tpe {
        Type::Int(int @ (Int::ISize | Int::USize)) => {
            let mref = MethodRef::new(
                ClassRef::bit_operations(ctx),
                ctx.alloc_string("LeadingZeroCount"),
                ctx.sig([Type::Int(int)], Type::Int(Int::I32)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let arg = handle_operand(&args[0].node, ctx);
            let call = ctx.call(mref, &[arg], IsPure::NOT);
            let value = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            return place_set(destination, value, ctx);
        }
        Type::Ptr(_) => {
            let mref = MethodRef::new(
                ClassRef::bit_operations(ctx),
                ctx.alloc_string("LeadingZeroCount"),
                ctx.sig([Type::Int(Int::USize)], Type::Int(Int::I32)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let arg = handle_operand(&args[0].node, ctx);
            let call = ctx.call(mref, &[arg], IsPure::NOT);
            let value = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            return place_set(destination, value, ctx);
        }
        Type::Int(Int::I64 | Int::U64) => ctx.alloc_node(0_i32),
        Type::Int(Int::I32 | Int::U32) => ctx.alloc_node(32_i32),
        Type::Int(Int::I16 | Int::U16) => ctx.alloc_node(48_i32),
        Type::Int(Int::I8 | Int::U8) => ctx.alloc_node(56_i32),
        Type::Int(Int::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("LeadingZeroCount"),
                ctx.sig([Type::Int(Int::I128)], Type::Int(Int::I128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let arg = handle_operand(&args[0].node, ctx);
            let call = ctx.call(mref, &[arg], IsPure::NOT);
            let value = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            return place_set(destination, value, ctx);
        }
        Type::Int(Int::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("LeadingZeroCount"),
                ctx.sig([Type::Int(Int::U128)], Type::Int(Int::U128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let arg = handle_operand(&args[0].node, ctx);
            let call = ctx.call(mref, &[arg], IsPure::NOT);
            let value = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            return place_set(destination, value, ctx);
        }
        _ => todo!("Can't `ctlz`  type {tpe:?} yet!"),
    };
    let mref = MethodRef::new(
        ClassRef::bit_operations(ctx),
        ctx.alloc_string("LeadingZeroCount"),
        ctx.sig([Type::Int(Int::U64)], Type::Int(Int::I32)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = ctx.alloc_methodref(mref);
    let arg = handle_operand(&args[0].node, ctx);
    let arg = ctx.int_cast(arg, Int::U64, ExtendKind::ZeroExtend);
    let call = ctx.call(mref, &[arg], IsPure::NOT);
    let diff = ctx.biop(call, sub, BinOp::Sub);
    let value = ctx.int_cast(diff, Int::U32, ExtendKind::ZeroExtend);
    place_set(destination, value, ctx)
}
pub fn cttz<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        1,
        "The intrinsic `ctlz` MUST take in exactly 1 argument!"
    );
    let bit_operations = ClassRef::bit_operations(ctx);
    let tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    let tpe = ctx.type_from_cache(tpe);
    let operand = handle_operand(&args[0].node, ctx);
    match tpe {
        Type::Int(Int::I8) => {
            let ttc = MethodRef::new(
                bit_operations,
                ctx.alloc_string("TrailingZeroCount"),
                ctx.sig([Type::Int(Int::I32)], Type::Int(Int::I32)),
                MethodKind::Static,
                vec![].into(),
            );
            let ttc = ctx.alloc_methodref(ttc);
            let operand = ctx.int_cast(operand, Int::I32, ExtendKind::SignExtend);
            let call = ctx.call(ttc, &[operand], IsPure::NOT);
            let value_calc = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            let min = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Min"),
                ctx.sig(
                    [Type::Int(Int::U32), Type::Int(Int::U32)],
                    Type::Int(Int::U32),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let min = ctx.alloc_methodref(min);
            let bits = ctx.alloc_node(i8::BITS);
            let value = ctx.call(min, &[value_calc, bits], IsPure::NOT);
            place_set(destination, value, ctx)
        }
        Type::Int(Int::I16) => {
            let mref = MethodRef::new(
                bit_operations,
                ctx.alloc_string("TrailingZeroCount"),
                ctx.sig([Type::Int(Int::I32)], Type::Int(Int::I32)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let operand = ctx.int_cast(operand, Int::I32, ExtendKind::SignExtend);
            let call = ctx.call(mref, &[operand], IsPure::NOT);
            let value_calc = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            let min = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Min"),
                ctx.sig(
                    [Type::Int(Int::U32), Type::Int(Int::U32)],
                    Type::Int(Int::U32),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let min = ctx.alloc_methodref(min);
            let bits = ctx.alloc_node(i16::BITS);
            let value = ctx.call(min, &[value_calc, bits], IsPure::NOT);
            place_set(destination, value, ctx)
        }
        Type::Int(Int::U8) => {
            let mref = MethodRef::new(
                bit_operations,
                ctx.alloc_string("TrailingZeroCount"),
                ctx.sig([Type::Int(Int::U32)], Type::Int(Int::I32)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let operand = ctx.int_cast(operand, Int::U32, ExtendKind::ZeroExtend);
            let call = ctx.call(mref, &[operand], IsPure::NOT);
            let value_calc = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            let min = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Min"),
                ctx.sig(
                    [Type::Int(Int::U32), Type::Int(Int::U32)],
                    Type::Int(Int::U32),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let min = ctx.alloc_methodref(min);
            let bits = ctx.alloc_node(u8::BITS);
            let value = ctx.call(min, &[value_calc, bits], IsPure::NOT);
            place_set(destination, value, ctx)
        }
        Type::Int(Int::U16) => {
            let mref = MethodRef::new(
                bit_operations,
                ctx.alloc_string("TrailingZeroCount"),
                ctx.sig([Type::Int(Int::U32)], Type::Int(Int::I32)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let operand = ctx.int_cast(operand, Int::U32, ExtendKind::ZeroExtend);
            let call = ctx.call(mref, &[operand], IsPure::NOT);
            let value_calc = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            let min = MethodRef::new(
                ClassRef::math(ctx),
                ctx.alloc_string("Min"),
                ctx.sig(
                    [Type::Int(Int::U32), Type::Int(Int::U32)],
                    Type::Int(Int::U32),
                ),
                MethodKind::Static,
                vec![].into(),
            );
            let min = ctx.alloc_methodref(min);
            let bits = ctx.alloc_node(u16::BITS);
            let value = ctx.call(min, &[value_calc, bits], IsPure::NOT);
            place_set(destination, value, ctx)
        }
        Type::Int(Int::I128) => {
            let mref = MethodRef::new(
                ClassRef::int_128(ctx),
                ctx.alloc_string("TrailingZeroCount"),
                ctx.sig([Type::Int(Int::I128)], Type::Int(Int::I128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let arg = handle_operand(&args[0].node, ctx);
            let call = ctx.call(mref, &[arg], IsPure::NOT);
            let value_calc = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            place_set(destination, value_calc, ctx)
        }
        Type::Int(Int::U128) => {
            let mref = MethodRef::new(
                ClassRef::uint_128(ctx),
                ctx.alloc_string("TrailingZeroCount"),
                ctx.sig([Type::Int(Int::U128)], Type::Int(Int::U128)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let arg = handle_operand(&args[0].node, ctx);
            let call = ctx.call(mref, &[arg], IsPure::NOT);
            let value_calc = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            place_set(destination, value_calc, ctx)
        }
        _ => {
            let mref = MethodRef::new(
                bit_operations,
                ctx.alloc_string("TrailingZeroCount"),
                ctx.sig([tpe], Type::Int(Int::I32)),
                MethodKind::Static,
                vec![].into(),
            );
            let mref = ctx.alloc_methodref(mref);
            let call = ctx.call(mref, &[operand], IsPure::NOT);
            let value_calc = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
            place_set(destination, value_calc, ctx)
        }
    }
}
pub fn rotate_left<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        2,
        "The  `rotate_left` MUST take in exactly 2 arguments!"
    );
    let val_tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    let val_tpe = ctx.type_from_cache(val_tpe);
    let val = handle_operand(&args[0].node, ctx);
    let rot = handle_operand(&args[1].node, ctx);
    match val_tpe {
        Type::Int(
            int @ (Int::U8
            | Int::I8
            | Int::U16
            | Int::I16
            | Int::U32
            | Int::I32
            | Int::U64
            | Int::I64
            | Int::U128
            | Int::I128
            | Int::USize
            | Int::ISize),
        ) => {
            let rot = ctx.int_cast(rot, Int::I32, ExtendKind::SignExtend);
            let value = rol_int(val, rot, int, ctx);
            place_set(destination, value, ctx)
        }
        _ => todo!("Can't ror {val_tpe:?}"),
    }
}
pub fn rol_int(val: Node, rot: Node, int: Int, asm: &mut cilly::Assembly) -> Node {
    let mref = MethodRef::new(
        int.class(asm),
        asm.alloc_string("RotateLeft"),
        asm.sig([Type::Int(int), Type::Int(Int::I32)], Type::Int(int)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    asm.call(mref, &[val, rot], IsPure::NOT)
}
pub fn ror_int(val: Node, rot: Node, int: Int, asm: &mut cilly::Assembly) -> Node {
    let mref = MethodRef::new(
        int.class(asm),
        asm.alloc_string("RotateRight"),
        asm.sig([Type::Int(int), Type::Int(Int::I32)], Type::Int(int)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    asm.call(mref, &[val, rot], IsPure::NOT)
}
pub fn rotate_right<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        2,
        "The  `rotate_right` MUST take in exactly 2 arguments!"
    );
    let val_tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    let val_tpe = ctx.type_from_cache(val_tpe);
    let val = handle_operand(&args[0].node, ctx);
    let rot = handle_operand(&args[1].node, ctx);
    match val_tpe {
        Type::Int(
            int @ (Int::U8
            | Int::I8
            | Int::U16
            | Int::I16
            | Int::U32
            | Int::I32
            | Int::U64
            | Int::I64
            | Int::U128
            | Int::I128
            | Int::USize
            | Int::ISize),
        ) => {
            let rot = ctx.int_cast(rot, Int::I32, ExtendKind::SignExtend);
            let value = ror_int(val, rot, int, ctx);
            place_set(destination, value, ctx)
        }
        _ => todo!("Can't ror {val_tpe:?}"),
    }
}
pub fn bitreverse_u8(byte: Node, asm: &mut Assembly) -> Node {
    let byte = asm.int_cast(byte, Int::U64, ExtendKind::ZeroExtend);
    let lhs_rhs = asm.alloc_node(0x0002_0202_0202_u64);
    let mul = asm.biop(byte, lhs_rhs, BinOp::Mul);
    let mask = asm.alloc_node(0x0108_8442_2010_u64);
    let and = asm.biop(mul, mask, BinOp::And);
    let divisor = asm.alloc_node(1023_u64);
    let rem = asm.biop(and, divisor, BinOp::RemUn);
    asm.int_cast(rem, Int::U8, ExtendKind::ZeroExtend)
}
fn bitreverse_u16(ushort: Node, asm: &mut Assembly) -> Node {
    let low = bitreverse_u8(asm.int_cast(ushort, Int::U8, ExtendKind::ZeroExtend), asm);
    let low = asm.int_cast(low, Int::U16, ExtendKind::ZeroExtend);
    let scale = asm.alloc_node(256_u16);
    let low_scaled = asm.biop(low, scale, BinOp::Mul);
    let divisor = asm.alloc_node(256_u16);
    let high_div = asm.biop(ushort, divisor, BinOp::Div);
    let high_byte = asm.int_cast(high_div, Int::U8, ExtendKind::ZeroExtend);
    let high = bitreverse_u8(high_byte, asm);
    let high = asm.int_cast(high, Int::U16, ExtendKind::ZeroExtend);
    asm.biop(low_scaled, high, BinOp::Add)
}
pub fn bitreverse_int(val: Node, int: Int, asm: &mut cilly::Assembly) -> Node {
    let mref = MethodRef::new(
        *asm.main_module(),
        asm.alloc_string(format!("bitreverse_{}", int.as_unsigned().name())),
        asm.sig([Type::Int(int.as_unsigned())], Type::Int(int.as_unsigned())),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    let arg = crate::casts::int_to_int(int.into(), int.as_unsigned().into(), val, asm);
    let call = asm.call(mref, &[arg], IsPure::NOT);
    crate::casts::int_to_int(int.as_unsigned().into(), int.into(), call, asm)
}
pub fn bitreverse<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
) -> Root {
    debug_assert_eq!(
        args.len(),
        1,
        "The  `bitreverse` MUST take in exactly 1 argument!"
    );
    let val_tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    let val_tpe = ctx.type_from_cache(val_tpe);
    let val = handle_operand(&args[0].node, ctx);
    let value = match val_tpe {
        Type::Int(Int::U8) => bitreverse_u8(val, ctx),
        Type::Int(Int::I8) => {
            let rev = bitreverse_u8(val, ctx);
            ctx.int_cast(rev, Int::I8, ExtendKind::SignExtend)
        }
        Type::Int(Int::U16) => bitreverse_u16(val, ctx),
        Type::Int(Int::I16) => {
            let val = ctx.int_cast(val, Int::U16, ExtendKind::ZeroExtend);
            let rev = bitreverse_u16(val, ctx);
            ctx.int_cast(rev, Int::I16, ExtendKind::SignExtend)
        }
        Type::Int(int @ (Int::I32 | Int::U32 | Int::I64 | Int::U64 | Int::U128 | Int::I128)) => {
            bitreverse_int(val, int, ctx)
        }
        // usize/isize: assume a 64-bit target (the same convention used by `ctlz` and the
        // saturating isize arms). Widen to u64, reuse the tested `bitreverse_u64` helper, then
        // narrow back. This avoids needing a separate `bitreverse_usize` patcher body.
        Type::Int(Int::USize) => {
            let widened = ctx.int_cast(val, Int::U64, ExtendKind::ZeroExtend);
            let rev = bitreverse_int(widened, Int::U64, ctx);
            ctx.int_cast(rev, Int::USize, ExtendKind::ZeroExtend)
        }
        Type::Int(Int::ISize) => {
            let widened = ctx.int_cast(val, Int::U64, ExtendKind::ZeroExtend);
            let rev = bitreverse_int(widened, Int::U64, ctx);
            ctx.int_cast(rev, Int::ISize, ExtendKind::ZeroExtend)
        }
        _ => todo!("can't yet bitreverse {val_tpe:?}"),
    };
    place_set(destination, value, ctx)
}
