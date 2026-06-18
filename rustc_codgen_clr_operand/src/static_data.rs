

use crate::constant::static_ty;
use cilly::{
    Access, CILRoot, Const, FnSig, Int, Interned, MethodDef, MethodDefIdx, MethodRef,
    StaticFieldDesc, Type,
    cilnode::MethodKind,
    utilis::encode,
    ir::{BasicBlock, CILNode},
};

type Root = Interned<cilly::ir::CILRoot>;
use rustc_codegen_clr_call::CallInfo;
pub use rustc_codegen_clr_ctx::MethodCompileCtx;
use rustc_codegen_clr_ctx::function_name;
use rustc_codegen_clr_type::{GetTypeExt, align_of, r#type::fixed_array};
use rustc_middle::{
    mir::interpret::{AllocId, Allocation, GlobalAlloc},
    ty::{Instance, List, TypingEnv},
};
use rustc_span::def_id::DefId;

pub fn add_static(def_id: DefId, ctx: &mut MethodCompileCtx<'_, '_>) -> Interned<CILNode> {
    let main_module_id = ctx.main_module();
    let alloc = ctx.tcx().eval_static_initializer(def_id).unwrap();
    let attrs = ctx.tcx().codegen_fn_attrs(def_id);

    let thread_local = attrs
        .flags
        .contains(rustc_middle::middle::codegen_fn_attrs::CodegenFnAttrFlags::THREAD_LOCAL);
    let align = alloc.0.align.bytes().max(1);
    let ty = static_ty(def_id, ctx.tcx());
    let tpe = ctx.type_from_cache(ty);
    assert_eq!(align, align_of(ty, ctx.tcx()));
    assert!(ty.is_sized(ctx.tcx(), TypingEnv::fully_monomorphized()));
    let symbol: String = ctx
        .tcx()
        .symbol_name(Instance::new_raw(def_id, List::empty()))
        .to_string();

    let sfld = ctx.add_static(
        tpe,
        symbol.clone(),
        thread_local,
        main_module_id,
        None,
        false,
    );
    let ptr = ctx.alloc_node(CILNode::LdStaticFieldAddress(sfld));
    let ptr = ctx.cast_ptr(ptr, Int::U8);
    let ptr = ptr;
    let initialzer = allocation_initializer_method(&alloc.0, &symbol, ctx, ptr, true);
    let root = ctx.alloc_root(cilly::CILRoot::call(*initialzer, []));

    if thread_local {
        ctx.add_tcctor(&[root]);
    } else {
        ctx.add_cctor(&[root]);
    }

    ptr
}
fn alloc_default_type(alloc_id: u64, ctx: &mut MethodCompileCtx<'_, '_>) -> Type {
    let alloc = match ctx
        .tcx()
        .global_alloc(AllocId(alloc_id.try_into().expect("0 alloc id?")))
    {
        GlobalAlloc::Memory(alloc) => alloc,
        GlobalAlloc::Static(def_id) => return ctx.type_from_cache(static_ty(def_id, ctx.tcx())),
        GlobalAlloc::VTable(..) => {
            todo!()
        }
        GlobalAlloc::Function { .. } => {
            todo!()
        }
        GlobalAlloc::TypeId{..}=>todo!(),
    };
    let tpe = match alloc.0.0.align.bytes() {
        ..1 => Int::U8,
        ..2 => Int::U16,
        ..4 => Int::U32,
        ..8 => Int::U64,
        _ => {
            ctx.tcx().dcx().span_warn(
                ctx.span(),
                format!(
                    "Alloc of align {} required, but that can't be guranteed!",
                    alloc.0.0.align.bytes()
                ),
            );
            Int::U64
        }
    };
    let arr_size = alloc.0.len() as u64;
    if arr_size == 0 {
        return Type::Void;
    }
    let size = tpe.size().unwrap_or(8) as u64;
    let tpe = fixed_array(
        ctx,
        Type::Int(tpe),
        arr_size.div_ceil(size),
        arr_size.next_multiple_of(size),
        tpe.size().unwrap_or(8) as u64,
    );
    Type::ClassRef(tpe)
}
/// Adds a static field and initialized for allocation represented by `alloc_id`.
pub fn add_allocation(
    alloc_id: u64,
    ctx: &mut MethodCompileCtx<'_, '_>,
    tpe: Interned<Type>,
) -> Interned<CILNode> {
    let uint8_ptr = ctx.nptr(Type::Int(Int::U8));
    let main_module_id = ctx.main_module();
    let const_allocation = match ctx
        .tcx()
        .global_alloc(AllocId(alloc_id.try_into().expect("0 alloc id?")))
    {
        GlobalAlloc::Memory(alloc) => alloc,
        GlobalAlloc::Static(def_id) => return add_static(def_id, ctx),
        GlobalAlloc::VTable(..) => {
            //TODO: handle VTables
            let field_desc = ctx.add_static(
                uint8_ptr,
                format!("v_{alloc_id:x}"),
                false,
                main_module_id,
                None,
                false,
            );
            return ctx.load_static(field_desc);
        }
        GlobalAlloc::Function { .. } => {
            //TODO: handle constant functions
            let alloc_fld = format!("f_{alloc_id:x}");
            let field_desc =
                ctx.add_static(uint8_ptr, alloc_fld, false, main_module_id, None, false);

            return ctx.load_static(field_desc);
            //todo!("Function/Vtable allocation.");
        }
        GlobalAlloc::TypeId{..} => todo!(),
    };

    let const_allocation = const_allocation.inner();

    let bytes: &[u8] =
        const_allocation.inspect_with_uninit_and_ptr_outside_interpreter(0..const_allocation.len());
    let align = const_allocation.align.bytes().max(1);
    if const_allocation.len() == 0 {
        return ctx.alloc_node(Const::USize(align));
    }
    // Check if const literal can be used
    if const_allocation.provenance().ptrs().is_empty() && align <= 1 {
        return ctx.bytebuffer(bytes, Int::U8);
    }
    // Alloc ids are *not* unique across all crates. Adding the hash here ensures we don't overwrite allocations during linking
    // TODO:consider using something better here / making the hashes stable.
    let byte_hash = calculate_hash(&bytes);
    match (align, bytes.len()) {
        _ => {
            let alloc_name = format!(
                "al_{}_{}_{}_{}",
                encode(alloc_id),
                encode(byte_hash),
                encode(tpe.inner().into()),
                const_allocation.len()
            );
            let name = ctx.alloc_string(alloc_name.clone());
            let field_desc = StaticFieldDesc::new(*ctx.main_module(), name, ctx[tpe]);
            // Currently, all static fields are in one module. Consider spliting them up.

            let main_module = ctx.class_mut(main_module_id);

            if main_module.has_static_field(name, field_desc.tpe()) {
                return ctx.static_addr(field_desc).into();
            }
            let tpe = ctx[tpe].clone();
            ctx.add_static(tpe, &*alloc_name, false, main_module_id, None, false);

            let ptr = ctx.static_addr(field_desc);
            let ptr = ctx.cast_ptr(ptr, Int::U8);

            let initialzer: MethodDefIdx =
                allocation_initializer_method(const_allocation, &alloc_name, ctx, ptr.into(), true);

            // Calls the static initialzer, and sets the static field to the returned pointer.
            let root = ctx.alloc_root(cilly::CILRoot::call(*initialzer, []));
            ctx.add_cctor(&[root]);

            ctx.static_addr(field_desc)
        }
    }
}
pub fn add_const_value(asm: &mut cilly::Assembly, bytes: u128) -> StaticFieldDesc {
    let uint8_ptr = Type::Int(Int::U128);
    let main_module_id = asm.main_module();
    let alloc_fld = format!("a_{bytes:x}");
    let alloc_fld_name = asm.alloc_string(alloc_fld.clone());

    let field_desc = StaticFieldDesc::new(*asm.main_module(), alloc_fld_name, Type::Int(Int::U128));

    let main_module = asm.class_mut(main_module_id);
    if main_module.has_static_field(alloc_fld_name, field_desc.tpe()) {
        return field_desc;
    }
    asm.add_static(uint8_ptr, alloc_fld, false, main_module_id, None, false);

    let field = asm.alloc_sfld(field_desc);
    let val = asm.alloc_node(Const::U128(bytes));
    let set = asm.alloc_root(cilly::CILRoot::SetStaticField { field, val });

    asm.add_cctor(&[set]);

    field_desc
}
fn allocation_initializer_method(
    const_allocation: &Allocation,
    name: &str,
    ctx: &mut MethodCompileCtx<'_, '_>,
    ptr: Interned<CILNode>,
    void_ret: bool,
) -> MethodDefIdx {
    let bytes: &[u8] =
        const_allocation.inspect_with_uninit_and_ptr_outside_interpreter(0..const_allocation.len());
    let ptrs = const_allocation.provenance().ptrs();
    let mut trees: Vec<Root> = Vec::new();

    // Emit the static-initialization roots directly.
    // STLoc(0, ptr)
    trees.push(ctx.alloc_root(CILRoot::StLoc(0, ptr)));
    // CpBlk(dst = LdLoc(0), src = bytebuffer, len = const)
    {
        let dst = ctx.alloc_node(CILNode::LdLoc(0));
        let src = ctx.bytebuffer(bytes, Int::U8);
        let len = ctx.alloc_node(Const::USize(bytes.len() as u64));
        let cpblk = ctx.cp_blk(dst, src, len);
        trees.push(cpblk);
    }

    if !ptrs.is_empty() {
        for (offset, prov) in ptrs.iter() {
            let offset = u32::try_from(offset.bytes_usize()).unwrap();
            // Check if this allocation is a function
            let reloc_target_alloc = ctx.tcx().global_alloc(prov.alloc_id());
            if let GlobalAlloc::Function {
                instance: finstance,
            } = reloc_target_alloc
            {
                // If it is a function, patch its pointer up.
                let mut ctx = MethodCompileCtx::new(ctx.tcx(), None, finstance, ctx);
                let call_info = CallInfo::sig_from_instance_(finstance, &mut ctx);
                let function_name = function_name(ctx.tcx().symbol_name(finstance));
                let mref = MethodRef::new(
                    *ctx.main_module(),
                    ctx.alloc_string(function_name),
                    ctx.alloc_sig(call_info.sig().clone()),
                    MethodKind::Static,
                    vec![].into(),
                );
                // addr = (LdLoc(0) + offset) cast to *usize
                let ld_loc = ctx.alloc_node(CILNode::LdLoc(0));
                let off = ctx.alloc_node(Const::USize(offset.into()));
                let addr = ctx.biop(ld_loc, off, cilly::BinOp::Add);
                let usize_ptr = ctx.nptr(Type::Int(Int::USize));
                let addr = ctx.cast_ptr_to(addr, usize_ptr);
                // val = LdFtn(mref) cast to usize
                let mref = ctx.alloc_methodref(mref);
                let ftn = ctx.ld_ftn(mref);
                let val = ctx.cast_ptr_to(ftn, Type::Int(Int::USize));
                trees.push(ctx.alloc_root(CILRoot::StInd(Box::new((
                    addr,
                    val,
                    Type::Int(Int::ISize),
                    false,
                )))));
            } else {
                let tpe = alloc_default_type(prov.alloc_id().0.into(), ctx);
                let tpe = ctx.alloc_type(tpe);
                let ptr_alloc = add_allocation(prov.alloc_id().0.into(), ctx, tpe);

                // addr = (LdLoc(0) + offset) cast to *usize
                let ld_loc = ctx.alloc_node(CILNode::LdLoc(0));
                let off = ctx.alloc_node(Const::USize(offset.into()));
                let addr = ctx.biop(ld_loc, off, cilly::BinOp::Add);
                let usize_ptr = ctx.nptr(Type::Int(Int::USize));
                let addr = ctx.cast_ptr_to(addr, usize_ptr);
                // val = ptr_alloc cast to usize
                let val = ctx.cast_ptr_to(ptr_alloc, Type::Int(Int::USize));
                trees.push(ctx.alloc_root(CILRoot::StInd(Box::new((
                    addr,
                    val,
                    Type::Int(Int::ISize),
                    false,
                )))));
            }
        }
    }
    if void_ret {
        trees.push(ctx.alloc_root(CILRoot::VoidRet));
    } else {
        let ld_loc = ctx.alloc_node(CILNode::LdLoc(0));
        trees.push(ctx.alloc_root(CILRoot::Ret(ld_loc)));
    }
    let uint8_ptr = ctx.nptr(Type::Int(Int::U8));
    let ret = if void_ret { Type::Void } else { uint8_ptr };
    let uint8_ptr_idx = ctx.alloc_type(uint8_ptr);
    let alloc_ptr_name = ctx.alloc_string("alloc_ptr");
    let sig = ctx.alloc_sig(FnSig::new([], ret));
    let main_module_id = ctx.main_module();
    let init_method = MethodDef::from_blocks(
        Access::Private,
        main_module_id,
        &format!("init_{name}"),
        sig,
        MethodKind::Static,
        vec![BasicBlock::new(trees, 0, None)],
        vec![(Some(alloc_ptr_name), uint8_ptr_idx)],
        vec![],
        ctx,
    );
    ctx.new_method(init_method)
}
fn calculate_hash<T: std::hash::Hash>(t: &T) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
