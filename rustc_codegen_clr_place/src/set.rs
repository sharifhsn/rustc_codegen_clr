use rustc_codegen_clr_ctx::MethodCompileCtx;
use rustc_codegen_clr_type::{
    GetTypeExt,
    adt::{enum_field_descriptor, field_descrptor},
    r#type::fat_ptr_to,
    utilis::pointer_to_is_fat,
};

use crate::{PlaceTy, pointed_type};
use cilly::{
    Assembly, BinOp, Interned, IntoAsmIndex, Type,
    cilnode::{ExtendKind, IsPure},
    {ClassRef, FieldDesc, Int, MethodRef, cilnode::MethodKind},
};
use rustc_middle::{
    mir::PlaceElem,
    ty::{FloatTy, IntTy, Ty, TyKind, UintTy},
};
pub fn local_set(
    local: usize,
    method: &rustc_middle::mir::Body,
    tree: Interned<cilly::ir::CILNode>,
    asm: &mut Assembly,
) -> Interned<cilly::ir::CILRoot> {
    if let Some(spread_arg) = method.spread_arg
        && local == spread_arg.as_usize()
    {
        return asm.st_loc(
            (method.local_decls.len() - method.arg_count)
                .try_into()
                .unwrap(),
            tree,
        );
    }
    if local == 0 {
        asm.st_loc(0, tree)
    } else if local > method.arg_count {
        asm.st_loc(u32::try_from(local - method.arg_count).unwrap(), tree)
    } else {
        asm.st_arg(u32::try_from(local - 1).unwrap(), tree)
    }
}
pub fn place_elem_set<'a>(
    place_elem: &PlaceElem<'a>,
    curr_type: PlaceTy<'a>,
    ctx: &mut MethodCompileCtx<'a, '_>,
    addr_calc: Interned<cilly::ir::CILNode>,
    value_calc: Interned<cilly::ir::CILNode>,
) -> Interned<cilly::ir::CILRoot> {
    match place_elem {
        PlaceElem::Deref => {
            let pointed_type = pointed_type(curr_type);

            ptr_set_op(pointed_type.into(), ctx, addr_calc, value_calc)
        }
        PlaceElem::Field(field_index, _field_type) => match curr_type {
            PlaceTy::Ty(curr_type) => {
                let curr_type = ctx.monomorphize(curr_type);
                let field_desc = field_descrptor(curr_type, (*field_index).into(), ctx);
                ctx.set_field(field_desc, addr_calc, value_calc)
            }
            super::PlaceTy::EnumVariant(enm, var_idx) => {
                let enm = ctx.monomorphize(enm);
                let field_desc = enum_field_descriptor(enm, field_index.as_u32(), var_idx, ctx);

                ctx.set_field(field_desc, addr_calc, value_calc)
            }
        },
        PlaceElem::Index(index) => {
            let curr_ty = curr_type
                .as_ty()
                .expect("INVALID PLACE: Indexing into enum variant???");
            let index = crate::get::local_get(index.as_usize(), ctx.body(), ctx);

            match curr_ty.kind() {
                TyKind::Slice(inner) => {
                    let inner = ctx.monomorphize(*inner);
                    let inner_type = ctx.type_from_cache(inner);
                    let inner_ptr = ctx.nptr(inner_type);
                    let slice = fat_ptr_to(Ty::new_slice(ctx.tcx(), inner), ctx);
                    let desc = FieldDesc::new(
                        slice,
                        ctx.alloc_string(cilly::DATA_PTR),
                        ctx.nptr(Type::Void),
                    );
                    let desc = ctx.alloc_field(desc);
                    let field_val = ctx.ld_field(addr_calc, desc);
                    let size = ctx.size_of(inner_type).into_idx(ctx);
                    let size = ctx.alloc_node(cilly::CILNode::IntCast {
                        input: size,
                        target: Int::USize,
                        extend: cilly::cilnode::ExtendKind::ZeroExtend,
                    });
                    let offset = ctx.biop(index, size, BinOp::Mul);
                    let field_val = ctx.cast_ptr(field_val, inner_ptr);
                    let addr_calc = ctx.biop(field_val, offset, BinOp::Add);
                    ptr_set_op(super::PlaceTy::Ty(inner), ctx, addr_calc, value_calc)
                }
                TyKind::Array(element, _length) => {
                    let element = ctx.monomorphize(*element);
                    let array_type = ctx.type_from_cache(curr_ty);
                    let element_type = ctx.type_from_cache(element);

                    let array_dotnet = array_type.as_class_ref().expect("Non array type");
                    let arr_ref = ctx.nref(array_type);
                    let mref = MethodRef::new(
                        array_dotnet,
                        ctx.alloc_string("set_Item"),
                        ctx.sig([arr_ref, Type::Int(Int::USize), element_type], Type::Void),
                        MethodKind::Instance,
                        vec![].into(),
                    );
                    let mref = ctx.alloc_methodref(mref);
                    ctx.call_root(mref, &[addr_calc, index, value_calc], IsPure::NOT)
                }
                _ => {
                    rustc_middle::ty::print::with_no_trimmed_paths! { todo!("Can't index into {curr_ty}!")}
                }
            }
        }
        PlaceElem::ConstantIndex {
            offset,
            min_length,
            from_end,
        } => {
            let _ = min_length;
            let curr_ty = curr_type
                .as_ty()
                .expect("INVALID PLACE: Indexing into enum variant???");
            let index = ctx.alloc_node(*offset);
            assert!(!from_end, "Indexing slice form end");

            match curr_ty.kind() {
                TyKind::Slice(inner) => {
                    let inner = ctx.monomorphize(*inner);

                    let inner_type = ctx.type_from_cache(inner);
                    let slice = fat_ptr_to(Ty::new_slice(ctx.tcx(), inner), ctx);
                    let desc = FieldDesc::new(
                        slice,
                        ctx.alloc_string(cilly::DATA_PTR),
                        ctx.nptr(Type::Void),
                    );
                    let metadata = FieldDesc::new(
                        slice,
                        ctx.alloc_string(cilly::METADATA),
                        Type::Int(Int::USize),
                    );
                    let mref = MethodRef::new(
                        *ctx.main_module(),
                        ctx.alloc_string("bounds_check"),
                        ctx.sig(
                            [Type::Int(Int::USize), Type::Int(Int::USize)],
                            Type::Int(Int::USize),
                        ),
                        MethodKind::Static,
                        vec![].into(),
                    );
                    let desc = ctx.alloc_field(desc);
                    let metadata = ctx.alloc_field(metadata);
                    let inner_ptr = ctx.nptr(inner_type);

                    let base = ctx.ld_field(addr_calc, desc);
                    let base = ctx.cast_ptr(base, inner_ptr);

                    let index_us = ctx.int_cast(index, Int::USize, ExtendKind::ZeroExtend);
                    let meta_val = ctx.ld_field(addr_calc, metadata);
                    let mref = ctx.alloc_methodref(mref);
                    let checked = ctx.call(mref, &[index_us, meta_val], IsPure::NOT);

                    let stride = ctx.size_of(inner_type).into_idx(ctx);
                    let stride = ctx.int_cast(stride, Int::USize, ExtendKind::ZeroExtend);
                    let scaled = ctx.biop(checked, stride, BinOp::Mul);
                    let addr = ctx.biop(base, scaled, BinOp::Add);
                    ptr_set_op(super::PlaceTy::Ty(inner), ctx, addr, value_calc)
                }
                TyKind::Array(element, _length) => {
                    //println!("WARNING: ConstantIndex has required min_length of {min_length}, but bounds checking on const access not supported yet!");
                    let element = ctx.monomorphize(*element);
                    let element = ctx.type_from_cache(element);
                    let array_type = ctx.type_from_cache(curr_ty);
                    let array_dotnet = array_type.as_class_ref().expect("Non array type");
                    let arr_ref = ctx.nref(array_type);
                    let mref = MethodRef::new(
                        array_dotnet,
                        ctx.alloc_string("set_Item"),
                        ctx.sig([arr_ref, Type::Int(Int::USize), element], Type::Void),
                        MethodKind::Instance,
                        vec![].into(),
                    );
                    let mref = ctx.alloc_methodref(mref);
                    let index_us = ctx.int_cast(index, Int::USize, ExtendKind::ZeroExtend);
                    ctx.call_root(mref, &[addr_calc, index_us, value_calc], IsPure::NOT)
                }
                _ => {
                    rustc_middle::ty::print::with_no_trimmed_paths! { todo!("Can't index into {curr_ty}!")}
                }
            }
        }
        _ => todo!("Can't handle porojection {place_elem:?} in set"),
    }
}
/// Returns a set of instructons to set a pointer to a `pointed_type` to a value from the stack.
pub fn ptr_set_op<'tcx>(
    pointed_type: PlaceTy<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    addr_calc: Interned<cilly::ir::CILNode>,
    value_calc: Interned<cilly::ir::CILNode>,
) -> Interned<cilly::ir::CILRoot> {
    if let PlaceTy::Ty(pointed_type) = pointed_type {
        match pointed_type.kind() {
            TyKind::Int(int_ty) => match int_ty {
                IntTy::I8 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I8), false),
                IntTy::I16 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I16), false),
                IntTy::I32 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I32), false),
                IntTy::I64 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I64), false),
                IntTy::Isize => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::ISize), false),
                IntTy::I128 => {
                    let tpe = ClassRef::int_128(ctx).into();
                    ctx.st_ind(addr_calc, value_calc, tpe, false)
                }
            },
            TyKind::Uint(int_ty) => match int_ty {
                UintTy::U8 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I8), false),
                UintTy::U16 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I16), false),
                UintTy::U32 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I32), false),
                UintTy::U64 => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I64), false),
                UintTy::Usize => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::ISize), false),
                UintTy::U128 => {
                    let tpe = ClassRef::uint_128(ctx).into();
                    ctx.st_ind(addr_calc, value_calc, tpe, false)
                }
            },
            TyKind::Float(float_ty) => match float_ty {
                FloatTy::F32 => ctx.st_ind(addr_calc, value_calc, Type::Float(cilly::Float::F32), false),
                FloatTy::F64 => ctx.st_ind(addr_calc, value_calc, Type::Float(cilly::Float::F64), false),
                FloatTy::F128 => {
                    ctx.st_ind(addr_calc, value_calc, Type::Float(cilly::Float::F128), false)
                }
                FloatTy::F16 => {
                    ctx.st_ind(addr_calc, value_calc, Type::Float(cilly::Float::F16), false)
                }
            },
            // Both Rust bool and a managed bool are 1 byte wide. .NET bools are 4 byte wide only in the context of Marshaling/PInvoke,
            // due to historic reasons(BOOL was an alias for int in early Windows, and it stayed this way.) - FractalFir
            TyKind::Bool => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I8), false),
            // always 4 bytes wide: https://doc.rust-lang.org/std/primitive.char.html#representation
            TyKind::Char => ctx.st_ind(addr_calc, value_calc, Type::Int(Int::I32), false),
            TyKind::Adt(_, _)
            | TyKind::Tuple(_)
            | TyKind::Array(_, _)
            | TyKind::Closure(_, _)
            | TyKind::Coroutine(_, _) => {
                let pointed_type = ctx.type_from_cache(pointed_type);
                ctx.st_ind(addr_calc, value_calc, pointed_type, false)
            }
            TyKind::Ref(_, inner, _) => {
                if pointer_to_is_fat(*inner, ctx.tcx(), ctx.instance()) {
                    let tpe = ctx.type_from_cache(pointed_type);
                    ctx.st_ind(addr_calc, value_calc, tpe, false)
                } else {
                    let inner = ctx.type_from_cache(*inner);
                    let ptr = ctx.nptr(inner);
                    ctx.st_ind(addr_calc, value_calc, ptr, false)
                }
            }
            TyKind::RawPtr(ty, _) => {
                if pointer_to_is_fat(*ty, ctx.tcx(), ctx.instance()) {
                    let tpe = ctx.type_from_cache(pointed_type);
                    ctx.st_ind(addr_calc, value_calc, tpe, false)
                } else {
                    let inner = ctx.type_from_cache(*ty);
                    let ptr = ctx.nptr(inner);
                    ctx.st_ind(addr_calc, value_calc, ptr, false)
                }
            }
            _ => todo!(" can't deref type {pointed_type:?} yet"),
        }
    } else {
        todo!("Can't set the value behind a poitner to an enum variant!");
    }
}
