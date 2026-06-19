use crate::{
    assembly::MethodCompileCtx,
    interop::AssemblyRef,
    utilis::{
        garag_to_bool, CTOR_FN_NAME, MANAGED_CALL_FN_NAME, MANAGED_CALL_VIRT_FN_NAME,
        MANAGED_CHECKED_CAST, MANAGED_IS_INST, MANAGED_LD_ELEM_REF, MANAGED_LD_LEN,
        MANAGED_LD_NULL, MANAGED_TRY_CATCH,
    },
};
use cilly::{
    cilnode::{ExtendKind, IsPure, MethodKind},
    BinOp, ClassRef, Const, FieldDesc, FnSig, Int, Interned, IntoAsmIndex,
};
use cilly::{MethodRef, Type};
use rustc_codegen_clr_call::CallInfo;
use rustc_codegen_clr_ctx::function_name;
use rustc_codegen_clr_place::place_set;
use rustc_codegen_clr_type::{utilis::garg_to_string, GetTypeExt};
use rustc_codgen_clr_operand::{handle_operand, operand_address};
use rustc_middle::ty::InstanceKind;
use rustc_middle::{
    mir::{Operand, Place},
    ty::{GenericArg, Instance, Ty, TyKind},
};
use rustc_span::Spanned;

type Node = Interned<cilly::ir::CILNode>;
type Root = Interned<cilly::ir::CILRoot>;
const EMPTY_ARGS: &[Node] = &[];

fn argc_from_fn_name(function_name: &str, prefix: &str) -> u32 {
    let argc_start = function_name.find(prefix).unwrap() + (prefix.len());
    let argc_end = argc_start + function_name[argc_start..].find('_').unwrap();
    let argument_count = &function_name[argc_start..argc_end];
    argument_count.parse::<u32>().unwrap()
}
/// Calls a non-virtual managed function(used for interop)
fn call_managed<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    function_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    fn_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let argument_count = argc_from_fn_name(function_name, MANAGED_CALL_FN_NAME);
    //FIXME: figure out the proper argc.
    //assert!(subst_ref.len() as u32 == argc + 3 || subst_ref.len() as u32 == argc + 4);
    assert!(args.len() == argument_count as usize);
    let asm = AssemblyRef::decode_assembly_ref(subst_ref[0], ctx.tcx());
    let asm = asm.name().map(|name| ctx.alloc_string(name));
    let class_name = garg_to_string(subst_ref[1], ctx.tcx());
    let class_name = ctx.alloc_string(class_name);
    let is_valuetype = garag_to_bool(subst_ref[2], ctx.tcx());
    let managed_fn_name = garg_to_string(subst_ref[3], ctx.tcx());
    let tpe = ClassRef::new(class_name, asm, is_valuetype, [].into());

    //eprintln!("tpe:{tpe:?}");
    let signature = crate::function_sig::sig_from_instance_(fn_instance, ctx)
        .expect("Can't get the function signature");

    if argument_count == 0 {
        let ret = cilly::Type::Void;
        let call_site = MethodRef::new(
            ctx.alloc_class_ref(tpe),
            ctx.alloc_string(managed_fn_name),
            ctx.sig([], ret),
            MethodKind::Static,
            vec![].into(),
        );
        let call_site = ctx.alloc_methodref(call_site);
        if *signature.output() == cilly::Type::Void {
            ctx.call_root(call_site, EMPTY_ARGS, IsPure::NOT)
        } else {
            let call = ctx.call(call_site, EMPTY_ARGS, IsPure::NOT);
            place_set(destination, call, ctx)
        }
    } else {
        let is_static = garag_to_bool(subst_ref[4], ctx.tcx());

        let mut call_args = Vec::new();
        for arg in args {
            call_args.push(handle_operand(&arg.node, ctx));
        }
        let call = MethodRef::new(
            ctx.alloc_class_ref(tpe),
            ctx.alloc_string(managed_fn_name),
            ctx.alloc_sig(signature.clone()),
            if is_static {
                MethodKind::Static
            } else if is_valuetype {
                // Value-type instance methods are non-virtual slots and must use `call instance`
                // (`callvirt` on an unboxed valuetype receiver is invalid IL).
                MethodKind::Instance
            } else {
                // Reference-type instance calls must be emitted as `callvirt`, not `call instance`:
                // many BCL "instance" methods reached through this non-virtual `instanceN` helper
                // are actually virtual/abstract slots (e.g. `MethodBase::GetParameters`, which is
                // abstract). Binding an abstract/virtual slot with a plain `call instance` is
                // invalid IL and the JIT rejects the whole method with "Bad IL format". `callvirt`
                // is the correct, universally-valid dispatch for a reference-type receiver (it works
                // for non-virtual instance methods too), mirroring the `callvirt_managed` path.
                MethodKind::Virtual
            },
            vec![].into(),
        );
        let call = ctx.alloc_methodref(call);
        if *signature.output() == cilly::Type::Void {
            ctx.call_root(call, &call_args, IsPure::NOT)
        } else {
            let node = ctx.call(call, &call_args, IsPure::NOT);
            place_set(destination, node, ctx)
        }
    }
}
/// Calls a virtual managed function(used for interop)
fn callvirt_managed<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    function_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    fn_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let argument_count = argc_from_fn_name(function_name, MANAGED_CALL_VIRT_FN_NAME);
    //assert!(subst_ref.len() as u32 == argc + 3 || subst_ref.len() as u32 == argc + 4);
    assert!(
        u32::try_from(args.len()).expect("More than 2^32 function arguments.") == argument_count
    );
    let asm = AssemblyRef::decode_assembly_ref(subst_ref[0], ctx.tcx());
    let asm = asm.name().map(|name| ctx.alloc_string(name));
    let class_name = garg_to_string(subst_ref[1], ctx.tcx());
    let class_name = ctx.alloc_string(class_name);
    let is_valuetype = garag_to_bool(subst_ref[2], ctx.tcx());

    let managed_fn_garg = &subst_ref[3];
    let managed_fn_garg = ctx.monomorphize(*managed_fn_garg);
    let managed_fn_name = garg_to_string(managed_fn_garg, ctx.tcx());

    let tpe = ClassRef::new(class_name, asm, is_valuetype, [].into());
    let signature = crate::function_sig::sig_from_instance_(fn_instance, ctx)
        .expect("Can't get the function signature");
    if argument_count == 0 {
        let ret = cilly::Type::Void;
        let call = MethodRef::new(
            ctx.alloc_class_ref(tpe),
            ctx.alloc_string(managed_fn_name),
            ctx.sig([], ret),
            MethodKind::Static,
            vec![].into(),
        );
        let call = ctx.alloc_methodref(call);
        if *signature.output() == cilly::Type::Void {
            ctx.call_root(call, EMPTY_ARGS, IsPure::NOT)
        } else {
            let node = ctx.call(call, EMPTY_ARGS, IsPure::NOT);
            place_set(destination, node, ctx)
        }
    } else {
        let is_static = garag_to_bool(subst_ref[4], ctx.tcx());

        let mut call_args = Vec::new();
        for arg in args {
            call_args.push(handle_operand(&arg.node, ctx));
        }
        let call = MethodRef::new(
            ctx.alloc_class_ref(tpe),
            ctx.alloc_string(managed_fn_name),
            ctx.alloc_sig(signature.clone()),
            // This is the *virtual* managed-call path (`virtN`). A non-static call must therefore
            // be emitted as `callvirt`, not `call` — calling a virtual/abstract slot (e.g.
            // `System.Type::get_FullName`) with a plain `call instance` is invalid IL and the JIT
            // rejects the whole method with "Bad IL format".
            if is_static {
                MethodKind::Static
            } else {
                MethodKind::Virtual
            },
            vec![].into(),
        );
        let call = ctx.alloc_methodref(call);
        if *signature.output() == cilly::Type::Void {
            ctx.call_root(call, &call_args, IsPure::NOT)
        } else {
            let node = ctx.call(call, &call_args, IsPure::NOT);
            place_set(destination, node, ctx)
        }
    }
}
/// Creates a new managed object, and places a reference to it in destination
fn call_ctor<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    function_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let argument_count = argc_from_fn_name(function_name, CTOR_FN_NAME);
    // Check that there are enough function path and argument specifers
    assert!(subst_ref.len() == argument_count as usize + 3);
    // Check that a proper number of arguments is used
    assert!(args.len() == argument_count as usize);
    // Get the name of the assembly the constructed object resides in
    let asm = AssemblyRef::decode_assembly_ref(subst_ref[0], ctx.tcx());
    let asm = asm.name().map(|name| ctx.alloc_string(name));
    // Get the name of the constructed object
    let class_name = garg_to_string(subst_ref[1], ctx.tcx());
    let class_name = ctx.alloc_string(class_name);
    // Check if the costructed object is valuetype. TODO: this may be unnecesary. Are valuetpes constructed using newobj?
    let is_valuetype = garag_to_bool(subst_ref[2], ctx.tcx());
    let tpe = ClassRef::new(class_name, asm, is_valuetype, [].into());
    let tpe = ctx.alloc_class_ref(tpe);
    // If no arguments, inputs don't have to be handled, so a simpler call handling is used.
    if argument_count == 0 {
        let mref = MethodRef::new(
            tpe,
            ctx.alloc_string(".ctor"),
            ctx.sig([Type::ClassRef(tpe)], Type::Void),
            MethodKind::Constructor,
            vec![].into(),
        );
        let mref = ctx.alloc_methodref(mref);
        let node = ctx.call(mref, EMPTY_ARGS, IsPure::NOT);
        place_set(destination, node, ctx)
    } else {
        let mut inputs: Vec<_> = subst_ref[3..]
            .iter()
            .map(|ty| {
                ctx.type_from_cache(
                    ctx.monomorphize(*ty)
                        .as_type()
                        .expect("Expceted generic type but got something that was not a type!"),
                )
            })
            .collect();
        inputs.insert(0, Type::ClassRef(tpe));
        let sig = ctx.sig(inputs, cilly::Type::Void);
        let mut call = Vec::new();
        for arg in args {
            call.push(handle_operand(&arg.node, ctx));
        }
        let ctor = MethodRef::new(
            tpe,
            ctx.alloc_string(".ctor"),
            sig,
            MethodKind::Constructor,
            vec![].into(),
        );
        let ctor = ctx.alloc_methodref(ctor);
        let node = ctx.call(ctor, &call, IsPure::NOT);
        place_set(destination, node, ctx)
    }
}
pub fn call_closure<'tcx>(
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    sig: FnSig,
    function_name: &str,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let last_arg = args
        .last()
        .expect("Closure must be called with at least 2 arguments(closure + arg tuple)");

    let other_args = &args[..args.len() - 1];
    let mut call_args = Vec::new();
    for arg in other_args {
        call_args.push(handle_operand(&arg.node, ctx));
    }
    // "Rust call" is wierd, and not at all optimized for .NET. Passing all the arguments in a tuple is bad for performance and simplicty. Thus, unpacking this tuple and forcing "Rust call" to be
    // "normal" is far easier and better for performance.
    let last_arg_type = ctx.monomorphize(last_arg.node.ty(ctx.body(), ctx.tcx()));
    match last_arg_type.kind() {
        TyKind::Tuple(elements) => {
            if elements.is_empty() {
            } else {
                let tuple_type = ctx.type_from_cache(last_arg_type);

                for (index, element) in elements.iter().enumerate() {
                    let element_type = ctx.type_from_cache(element);
                    if element_type == Type::Void {
                        let u = ctx.uninit_val(Type::Void);
                        call_args.push(u);
                        continue;
                    }
                    let tuple_element_name = format!("Item{}", index + 1);
                    let field_descriptor = FieldDesc::new(
                        tuple_type.as_class_ref().expect("Invalid tuple type"),
                        ctx.alloc_string(tuple_element_name),
                        element_type,
                    );
                    let desc = ctx.alloc_field(field_descriptor);
                    let obj = handle_operand(&last_arg.node, ctx);
                    let fld = ctx.ld_field(obj, desc);
                    call_args.push(fld);
                }

                //todo!("Can't unbox tupels yet!")
            }
        }
        _ => panic!("Can't unbox type {last_arg_type:?}!"),
    }
    //panic!("Last arg:{last_arg:?}last_arg_type:{last_arg_type:?}");
    //assert_eq!(args.len(),signature.inputs().len(),"CALL SIGNATURE ARG COUNT MISMATCH!");
    let is_void = matches!(sig.output(), cilly::Type::Void);

    let call = MethodRef::new(
        *ctx.main_module(),
        ctx.alloc_string(function_name),
        ctx.alloc_sig(sig),
        MethodKind::Static,
        vec![].into(),
    );
    // Hande the call itself
    let call = ctx.alloc_methodref(call);
    if is_void {
        ctx.call_root(call, &call_args, IsPure::NOT)
    } else {
        let node = ctx.call(call, &call_args, IsPure::NOT);
        place_set(destination, node, ctx)
    }
}
pub fn call_inner<'tcx>(
    fn_type: Ty<'tcx>,
    instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    span: rustc_span::Span,
) -> Vec<Root> {
    if let rustc_middle::ty::InstanceKind::Virtual(_def, fn_idx) = instance.def {
        assert!(!args.is_empty());

        let fat_ptr_address = operand_address(&args[0].node, ctx);
        let fat_ptr_dyn = ctx.alloc_string("FatPtrn3Dyn");
        let vtable_ptr_field_desc = FieldDesc::new(
            ctx.alloc_class_ref(ClassRef::new(fat_ptr_dyn, None, true, [].into())),
            ctx.alloc_string(crate::METADATA),
            Type::Int(Int::USize),
        );
        let vtable_ptr_field_desc = ctx.alloc_field(vtable_ptr_field_desc);
        let vtable_ptr = ctx.ld_field(fat_ptr_address, vtable_ptr_field_desc);

        let vtable_index = ctx
            .alloc_node(i32::try_from(fn_idx).expect("More tahn 2^31 functions in a vtable!"));
        let size = ctx.size_of(Int::ISize).into_idx(ctx);
        let vtable_offset = ctx.biop(vtable_index, size, BinOp::Mul);
        let vtable_offset = ctx.int_cast(vtable_offset, Int::USize, ExtendKind::ZeroExtend);
        // Get the address of the function ptr, and load it
        let obj_ptr_field_desc = FieldDesc::new(
            ctx.alloc_class_ref(ClassRef::new(fat_ptr_dyn, None, true, [].into())),
            ctx.alloc_string(crate::DATA_PTR),
            ctx.nptr(Type::Void),
        );
        // Get the addres of the object
        let obj_ptr_field_desc = ctx.alloc_field(obj_ptr_field_desc);
        let obj_ptr = ctx.ld_field(fat_ptr_address, obj_ptr_field_desc);
        // Get the call info
        let call_info = CallInfo::sig_from_instance_(instance, ctx);

        let mut signature = call_info.sig().clone();
        signature.inputs_mut()[0] = ctx.nptr(Type::Void);
        let mut call_args = [obj_ptr].to_vec();
        if call_info.split_last_tuple() {
            let last_arg = args
                .last()
                .expect("Closure must be called with at least 2 arguments(closure + arg tuple)");

            let other_args = &args[..args.len() - 1];
            for arg in other_args.iter().skip(1) {
                call_args.push(handle_operand(&arg.node, ctx));
            }
            // "Rust call" is weird, and not at all optimized for .NET. Passing all the arguments in a tuple is bad for performance and simplicty. Thus, unpacking this tuple and forcing "Rust call" to be
            // "normal" is far easier and better for performance.
            let last_arg_type = ctx.monomorphize(last_arg.node.ty(ctx.body(), ctx.tcx()));
            match last_arg_type.kind() {
                TyKind::Tuple(elements) => {
                    if elements.is_empty() {
                    } else {
                        let tuple_type = ctx.type_from_cache(last_arg_type);

                        for (index, element) in elements.iter().enumerate() {
                            let element_type = ctx.type_from_cache(element);
                            if element_type == Type::Void {
                                let u = ctx.uninit_val(Type::Void);
                                call_args.push(u);
                                continue;
                            }
                            let tuple_element_name = format!("Item{}", index + 1);
                            let field_descriptor = FieldDesc::new(
                                tuple_type.as_class_ref().expect("Invalid tuple type"),
                                ctx.alloc_string(tuple_element_name),
                                element_type,
                            );
                            let desc = ctx.alloc_field(field_descriptor);
                            let obj = handle_operand(&last_arg.node, ctx);
                            let fld = ctx.ld_field(obj, desc);
                            call_args.push(fld);
                        }
                    }
                }
                _ => panic!("Can't unbox type {last_arg_type:?}!"),
            }
        } else {
            for arg in args.iter().skip(1) {
                call_args.push(handle_operand(&arg.node, ctx));
            }
        }
        let sig = ctx.alloc_sig(signature.clone());
        let fn_ptr_addr = ctx.biop(vtable_ptr, vtable_offset, BinOp::Add);
        // `fn_ptr_addr` is the address of the vtable slot holding the function pointer, so it must
        // be cast to a pointer-to-`FnPtr` (one level of indirection) before loading the `FnPtr`.
        // `cast_ptr` already wraps its argument in a `Ptr`, so the pointee type passed here is the
        // bare `FnPtr(sig)` — NOT `nptr(FnPtr(sig))`, which would yield a `Ptr(Ptr(FnPtr))` and make
        // the subsequent `LdInd { tpe: FnPtr }` deref a data `Ptr` (the `DerfWrongPtr` / Bad IL bug).
        let fn_ptr_addr = ctx.cast_ptr(fn_ptr_addr, Type::FnPtr(sig));
        let fn_ptr = ctx.load(fn_ptr_addr, Type::FnPtr(sig));
        assert_eq!(
            signature.inputs().len(),
            call_args.len(),
            "sig:{signature:?} call_args:{call_args:?}"
        );
        let is_ret_void = matches!(signature.output(), cilly::Type::Void);
        return if is_ret_void {
            vec![ctx.call_indirect_root(sig, fn_ptr, call_args)]
        } else {
            let call = ctx.call_indirect(sig, fn_ptr, call_args);
            vec![place_set(destination, call, ctx)]
        };
    }
    let call_info = CallInfo::sig_from_instance_(instance, ctx);

    let function_name = function_name(ctx.tcx().symbol_name(instance));
    if matches!(instance.def, InstanceKind::Intrinsic(_)) {
        return super::intrinsics::handle_intrinsic(
            &function_name,
            args,
            destination,
            instance,
            span,
            ctx,
        );
    }
    let mut signature = call_info.sig().clone();
    // Checks if function is "magic"
    if function_name.contains(CTOR_FN_NAME) {
        assert!(
            !call_info.split_last_tuple(),
            "Constructors may not use the `rust_call` calling convention!"
        );
        // Constructor
        return vec![call_ctor(
            instance.args,
            &function_name,
            args,
            destination,
            ctx,
        )];
    } else if function_name.contains(MANAGED_CALL_VIRT_FN_NAME) {
        assert!(
            !call_info.split_last_tuple(),
            "Managed virtual calls may not use the `rust_call` calling convention!"
        );
        // Virtual (for interop)
        return vec![callvirt_managed(
            instance.args,
            &function_name,
            args,
            destination,
            instance,
            ctx,
        )];
    } else if function_name.contains(MANAGED_CALL_FN_NAME) {
        assert!(
            !call_info.split_last_tuple(),
            "Managed calls may not use the `rust_call` calling convention!"
        );
        // Not-Virtual (for interop)
        return vec![call_managed(
            instance.args,
            &function_name,
            args,
            destination,
            instance,
            ctx,
        )];
    } else if function_name.contains(MANAGED_LD_LEN) {
        assert!(
            !call_info.split_last_tuple(),
            "Managed calls may not use the `rust_call` calling convention!"
        );
        // Not-Virtual (for interop)
        let arr = handle_operand(&args[0].node, ctx);
        let len = ctx.ld_len(arr);
        return vec![place_set(destination, len, ctx)];
    } else if function_name.contains(MANAGED_LD_NULL) {
        assert!(
            !call_info.split_last_tuple(),
            "Managed calls may not use the `rust_call` calling convention!"
        );
        // Not-Virtual (for interop)
        let tpe = ctx
            .type_from_cache(instance.args[0].as_type().unwrap())
            .as_class_ref()
            .unwrap();

        let node = ctx.alloc_node(Const::Null(tpe));
        return vec![place_set(destination, node, ctx)];
    } else if function_name.contains(MANAGED_CHECKED_CAST) {
        let tpe = ctx
            .type_from_cache(instance.args[0].as_type().unwrap())
            .as_class_ref()
            .unwrap();
        let input = handle_operand(&args[0].node, ctx);
        // Not-Virtual (for interop)
        let node = ctx.checked_cast(input, tpe);
        return vec![place_set(destination, node, ctx)];
    } else if function_name.contains(MANAGED_IS_INST) {
        let tpe = ctx
            .type_from_cache(instance.args[0].as_type().unwrap())
            .as_class_ref()
            .unwrap();
        let input = handle_operand(&args[0].node, ctx);
        // Not-Virtual (for interop)
        let node = ctx.is_inst(input, tpe);
        return vec![place_set(destination, node, ctx)];
    } else if function_name.contains(MANAGED_LD_ELEM_REF) {
        assert!(
            !call_info.split_last_tuple(),
            "Managed calls may not use the `rust_call` calling convention!"
        );
        // Not-Virtual (for interop)
        let arr = handle_operand(&args[0].node, ctx);
        let idx = handle_operand(&args[1].node, ctx);
        let node = ctx.ld_elem_ref(arr, idx);
        return vec![place_set(destination, node, ctx)];
    } else if function_name.contains(MANAGED_TRY_CATCH) {
        assert!(
            !call_info.split_last_tuple(),
            "Managed calls may not use the `rust_call` calling convention!"
        );
        // `try_catch(try_fn, data, catch_fn) -> i32`: run `try_fn(data)` inside a CIL
        // try/catch that catches *any* .NET exception (the `interop_try_catch` builtin),
        // returning 0 on normal completion and 1 if an exception was caught (after running
        // `catch_fn(data)`). Unlike `catch_unwind`, this catches foreign/BCL exceptions.
        let try_fn = handle_operand(&args[0].node, ctx);
        let data_ptr = handle_operand(&args[1].node, ctx);
        let catch_fn = handle_operand(&args[2].node, ctx);
        let uint8_ptr = ctx.nptr(Type::Int(Int::U8));
        let try_ptr = ctx.sig([uint8_ptr], Type::Void);
        let catch_ptr = ctx.sig([uint8_ptr], Type::Void);
        let try_catch = MethodRef::new(
            *ctx.main_module(),
            ctx.alloc_string("interop_try_catch"),
            ctx.sig(
                [Type::FnPtr(try_ptr), uint8_ptr, Type::FnPtr(catch_ptr)],
                Type::Int(Int::I32),
            ),
            MethodKind::Static,
            vec![].into(),
        );
        let try_catch = ctx.alloc_methodref(try_catch);
        let node = ctx.call(try_catch, &[try_fn, data_ptr, catch_fn], IsPure::NOT);
        return vec![place_set(destination, node, ctx)];
    }
    if call_info.split_last_tuple() {
        return vec![call_closure(
            args,
            destination,
            signature,
            &function_name,
            ctx,
        )];
    }

    let mut call_args = Vec::new();
    for arg in args {
        let res_calc = handle_operand(&arg.node, ctx);
        call_args.push(res_calc);
    }
    if crate::function_sig::is_fn_variadic(fn_type, ctx.tcx()) {
        signature.set_inputs(
            args.iter()
                .map(|operand| {
                    ctx.type_from_cache(ctx.monomorphize(operand.node.ty(ctx.body(), ctx.tcx())))
                })
                .collect::<Box<_>>(),
        );
    }
    if args.len() < signature.inputs().len() {
        let tpe: cilly::Type = signature.inputs()[signature.inputs().len() - 1];
        // let arg_len = args.len();
        //assert_eq!(args.len() + 1,signature.inputs().len(),"ERROR: mismatched argument count. \nsignature inputs:{:?} \narguments:{args:?}\narg_len:{arg_len}\n",signature.inputs());
        // assert_eq!(signature.inputs()[signature.inputs().len() - 1],tpe);
        //FIXME:This assembles a panic location from uninitialized memory. This WILL lead to bugs once unwinding is added. The fields `file`,`col`, and `line` should be set there.
        let u = ctx.uninit_val(tpe);
        call_args.push(u);
        //panic!("Call with PanicLocation!");
    }
    //assert_eq!(args.len(),signature.inputs().len(),"CALL SIGNATURE ARG COUNT MISMATCH!");
    let is_void = matches!(signature.output(), cilly::Type::Void);
    //rustc_middle::ty::print::with_no_trimmed_paths! {call.push(CILOp::Comment(format!("Calling {instance:?}").into()))};
    if let InstanceKind::DropGlue(_def, None) = instance.def {
        return vec![ctx.alloc_root(cilly::CILRoot::Nop)];
    }
    let call_site = MethodRef::new(
        *ctx.main_module(),
        ctx.alloc_string(function_name),
        ctx.alloc_sig(signature),
        MethodKind::Static,
        vec![].into(),
    );
    // Handle
    let site = ctx.alloc_methodref(call_site);
    if is_void {
        vec![ctx.call_root(site, &call_args, IsPure::NOT)]
    } else {
        let res_calc = ctx.call(site, &call_args, IsPure::NOT);
        vec![place_set(destination, res_calc, ctx)]
    }
}
/// Calls `fn_type` with `args`, placing the return value in destination.
pub fn call<'tcx>(
    fn_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    span: rustc_span::Span,
) -> Vec<Root> {
    let fn_type = ctx.monomorphize(fn_type);
    let instance = if let TyKind::FnDef(def_id, subst_ref) = fn_type.kind() {
        let subst = ctx.monomorphize(*subst_ref);
        let env = rustc_middle::ty::TypingEnv::fully_monomorphized();
        let Some(instance) =
            Instance::try_resolve(ctx.tcx(), env, *def_id, subst).expect("Invalid function def")
        else {
            panic!("ERROR: Could not get function instance. fn type:{fn_type:?}")
        };

        instance
    } else {
        todo!("Trying to call a type which is not a function definition!");
    };
    call_inner(fn_type, instance, ctx, args, destination, span)
}
