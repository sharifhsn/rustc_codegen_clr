use crate::assembly::MethodCompileCtx;
use cilly::{
    cilnode::ExtendKind, cilnode::IsPure, cilnode::MethodKind, ClassRef, Int, Interned, MethodRef,
    Type,
};
use rustc_codegen_clr_place::place_set;
use rustc_codegen_clr_type::GetTypeExt;
use rustc_middle::{mir::Place, ty::Instance};

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;

pub fn type_id<'tcx>(
    destination: &Place<'tcx>,
    call_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let tpe = ctx.monomorphize(
        call_instance.args[0]
            .as_type()
            .expect("needs_drop works only on types!"),
    );
    let tpe = ctx.type_from_cache(tpe);
    let type_type = ClassRef::type_type(ctx);
    let runtime_handle = ClassRef::runtime_type_hadle(ctx);
    let sig = ctx.sig([runtime_handle.into()], type_type);
    let gethash_sig = ctx.sig([type_type.into()], Type::Int(Int::I32));
    let op_implict = MethodRef::new(
        ClassRef::uint_128(ctx),
        ctx.alloc_string("op_Implicit"),
        ctx.sig([Type::Int(Int::U32)], Type::Int(Int::U128)),
        MethodKind::Static,
        vec![].into(),
    );
    let get_hash_code = MethodRef::new(
        ClassRef::object(ctx),
        ctx.alloc_string("GetHashCode"),
        gethash_sig,
        MethodKind::Virtual,
        vec![].into(),
    );
    let get_type_handle = MethodRef::new(
        type_type,
        ctx.alloc_string("GetTypeFromHandle"),
        sig,
        MethodKind::Static,
        vec![].into(),
    );
    let op_implict = ctx.alloc_methodref(op_implict);
    let get_hash_code = ctx.alloc_methodref(get_hash_code);
    let get_type_handle = ctx.alloc_methodref(get_type_handle);
    let type_token: Node = ctx.ld_type_token(tpe);
    let handle = ctx.call(get_type_handle, &[type_token], IsPure::NOT);
    let hash = ctx.call(get_hash_code, &[handle], IsPure::NOT);
    let hash_u32 = ctx.int_cast(hash, Int::U32, ExtendKind::ZeroExtend);
    let value_calc = ctx.call(op_implict, &[hash_u32], IsPure::NOT);
    place_set(destination, value_calc, ctx)
}
