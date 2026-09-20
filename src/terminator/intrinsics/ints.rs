use crate::assembly::MethodCompileCtx;
use crate::operand::handle_operand;
use crate::place::place_set;
use crate::r#type::GetTypeExt;
use cilly::cilnode::{ExtendKind, IsPure};
use cilly::{
    Assembly, BinOp, Int, Interned, Type,
    {ClassRef, MethodRef, cilnode::MethodKind},
};
use rustc_middle::{
    mir::{Operand, Place},
    ty::Instance,
};
use rustc_span::Spanned;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

fn bit_count_call(asm: &mut cilly::Assembly, operand: Node, input: Type, method: &str) -> Node {
    let mref = MethodRef::new(
        ClassRef::bit_operations(asm),
        asm.alloc_string(method),
        asm.sig([input], Type::Int(Int::I32)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    let call = asm.call(mref, &[operand], IsPure::NOT);
    asm.int_cast(call, Int::U32, ExtendKind::ZeroExtend)
}

fn ctpop_small_int(asm: &mut cilly::Assembly, operand: Node, int: Int) -> Node {
    assert!(int.size().is_none_or(|size| size <= 8));
    bit_count_call(asm, operand, Type::Int(int), "PopCount")
}

fn cttz_narrow(
    operand: Node,
    cast: Int,
    extend: ExtendKind,
    bits: u32,
    ctx: &mut MethodCompileCtx<'_, '_>,
) -> Node {
    let mref = MethodRef::new(
        ClassRef::bit_operations(ctx),
        ctx.alloc_string("TrailingZeroCount"),
        ctx.sig([Type::Int(cast)], Type::Int(Int::I32)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = ctx.alloc_methodref(mref);
    let operand = ctx.int_cast(operand, cast, extend);
    let call = ctx.call(mref, &[operand], IsPure::NOT);
    let value = ctx.int_cast(call, Int::U32, ExtendKind::ZeroExtend);
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
    let bits = ctx.alloc_node(bits);
    ctx.call(min, &[value, bits], IsPure::NOT)
}

fn wide_bit_count(
    ctx: &mut MethodCompileCtx<'_, '_>,
    operand: Node,
    int: Int,
    method: &'static str,
) -> Node {
    let tpe = Type::Int(int);
    let class = if int.is_signed() {
        ClassRef::int_128(ctx)
    } else {
        ClassRef::uint_128(ctx)
    };
    let mref = MethodRef::new(
        class,
        ctx.alloc_string(method),
        ctx.sig([tpe], tpe),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = ctx.alloc_methodref(mref);
    let call = ctx.call(mref, &[operand], IsPure::NOT);
    crate::casts::int_to_int(tpe, Type::Int(Int::U32), call, ctx)
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
        Type::Int(int @ (Int::U128 | Int::I128)) => wide_bit_count(ctx, operand, int, "PopCount"),
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
        Type::Int(Int::ISize | Int::USize) | Type::Ptr(_) => {
            let input_type = match tpe {
                Type::Int(int) => Type::Int(int),
                Type::Ptr(_) => Type::Int(Int::USize),
                _ => unreachable!(),
            };
            let arg = handle_operand(&args[0].node, ctx);
            let value = bit_count_call(ctx, arg, input_type, "LeadingZeroCount");
            return place_set(destination, value, ctx);
        }
        Type::Int(Int::I64 | Int::U64) => ctx.alloc_node(0_i32),
        Type::Int(Int::I32 | Int::U32) => ctx.alloc_node(32_i32),
        Type::Int(Int::I16 | Int::U16) => ctx.alloc_node(48_i32),
        Type::Int(Int::I8 | Int::U8) => ctx.alloc_node(56_i32),
        Type::Int(int @ (Int::I128 | Int::U128)) => {
            let arg = handle_operand(&args[0].node, ctx);
            let value = wide_bit_count(ctx, arg, int, "LeadingZeroCount");
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
    let tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    let tpe = ctx.type_from_cache(tpe);
    let operand = handle_operand(&args[0].node, ctx);
    let value = match tpe {
        Type::Int(int @ (Int::I8 | Int::I16 | Int::U8 | Int::U16)) => {
            let (cast, extend, bits) = match int {
                Int::I8 => (Int::I32, ExtendKind::SignExtend, i8::BITS),
                Int::I16 => (Int::I32, ExtendKind::SignExtend, i16::BITS),
                Int::U8 => (Int::U32, ExtendKind::ZeroExtend, u8::BITS),
                Int::U16 => (Int::U32, ExtendKind::ZeroExtend, u16::BITS),
                _ => unreachable!(),
            };
            cttz_narrow(operand, cast, extend, bits, ctx)
        }
        Type::Int(int @ (Int::I128 | Int::U128)) => {
            wide_bit_count(ctx, operand, int, "TrailingZeroCount")
        }
        _ => bit_count_call(ctx, operand, tpe, "TrailingZeroCount"),
    };
    place_set(destination, value, ctx)
}
fn rotate_intrinsic<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    call_instance: Instance<'tcx>,
    intrinsic: &str,
    method: &str,
) -> Root {
    debug_assert_eq!(
        args.len(),
        2,
        "The rotate intrinsic MUST take in exactly 2 arguments!"
    );
    let val_tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    let val_tpe = ctx.type_from_cache(val_tpe);
    let int = match val_tpe {
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
        ) => int,
        _ => todo!("Can't {intrinsic} {val_tpe:?}"),
    };
    let val = handle_operand(&args[0].node, ctx);
    let rot = handle_operand(&args[1].node, ctx);
    let rot = ctx.int_cast(rot, Int::I32, ExtendKind::SignExtend);
    let value = rotate_int(val, rot, int, method, ctx);
    place_set(destination, value, ctx)
}

macro_rules! rotate_intrinsic_export {
    ($name:ident, $intrinsic:literal, $method:literal) => {
        pub fn $name<'tcx>(
            args: &[Spanned<Operand<'tcx>>],
            destination: &Place<'tcx>,
            ctx: &mut MethodCompileCtx<'tcx, '_>,
            call_instance: Instance<'tcx>,
        ) -> Root {
            rotate_intrinsic(args, destination, ctx, call_instance, $intrinsic, $method)
        }
    };
}

rotate_intrinsic_export!(rotate_left, "rotate_left", "RotateLeft");

fn rotate_int(val: Node, rot: Node, int: Int, method: &str, asm: &mut cilly::Assembly) -> Node {
    let mref = MethodRef::new(
        int.class(asm),
        asm.alloc_string(method),
        asm.sig([Type::Int(int), Type::Int(Int::I32)], Type::Int(int)),
        MethodKind::Static,
        vec![].into(),
    );
    let mref = asm.alloc_methodref(mref);
    asm.call(mref, &[val, rot], IsPure::NOT)
}

rotate_intrinsic_export!(rotate_right, "rotate_right", "RotateRight");
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
    let mref = asm.static_mref(
        &format!("bitreverse_{}", int.as_unsigned().name()),
        [Type::Int(int.as_unsigned())],
        Type::Int(int.as_unsigned()),
    );
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
        Type::Int(int @ (Int::USize | Int::ISize)) => {
            let physical = if ctx.target_layout().pointer_bits() == 32 {
                Int::U32
            } else {
                Int::U64
            };
            let widened = ctx.int_cast(val, physical, ExtendKind::ZeroExtend);
            let rev = bitreverse_int(widened, physical, ctx);
            ctx.int_cast(rev, int, ExtendKind::ZeroExtend)
        }
        _ => todo!("can't yet bitreverse {val_tpe:?}"),
    };
    place_set(destination, value, ctx)
}
