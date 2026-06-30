use crate::{
    assembly::MethodCompileCtx,
    interop::AssemblyRef,
    utilis::{
        garag_to_bool, CTOR_FN_NAME, GENERIC_CALL_FN_NAME, GENERIC_CTOR_FN_NAME,
        MANAGED_CALL_FN_NAME, MANAGED_CALL_VIRT_FN_NAME, MANAGED_CHECKED_CAST, MANAGED_IS_INST,
        MANAGED_LD_ELEM_REF, MANAGED_LD_LEN, MANAGED_LD_NULL, MANAGED_NEW_ARR, MANAGED_SET_ELEM,
        MANAGED_THROW, MANAGED_TRY_CATCH,
    },
};
use cilly::{
    cilnode::{ExtendKind, IsPure, MethodKind},
    BinOp, ClassRef, Const, FieldDesc, FnSig, Int, Interned, IntoAsmIndex,
};
use cilly::tpe::GenericKind;
use cilly::{MethodRef, Type};
use rustc_codegen_clr_call::CallInfo;
use rustc_codegen_clr_ctx::function_name;
use rustc_codegen_clr_place::place_set;
use rustc_codegen_clr_type::{
    utilis::{garag_to_usize, garg_to_string},
    GetTypeExt,
};
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
        // Use the REAL return type, not Void. A zero-arg managed getter (e.g. a static `get_Default`
        // returning a managed reference) must produce a value of its declared type; hardcoding Void
        // here made the call node Void, so storing it into the (correctly-typed) destination failed
        // typecheck (`LocalAssigementWrong got "v"`). (A zero explicit-arg call is always `static0` —
        // an instance receiver would be an explicit arg and take the branch below — so `Static` is
        // correct here.)
        let ret = *signature.output();
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
        // Use the REAL return type, not Void (see `call_managed`'s 0-arg branch) — a zero-arg managed
        // getter returning a managed reference was being typed Void, failing the destination store.
        let ret = *signature.output();
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
/// WF-9 generic interop bridge — decompose a tuple-typed generic argument into the lowered .NET
/// types of its elements. Used to pass a class's generic-argument list (`(i32,)` of `List<i32>`) or
/// a method's *definition-shape* signature (`(Output, In0, …)` with `!N`/`!!N` markers) as a single
/// type parameter.
fn tuple_garg_to_types<'tcx>(
    garg: GenericArg<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Vec<Type> {
    let ty = ctx.monomorphize(
        garg.as_type()
            .expect("WF-9 generic interop: expected a tuple type parameter"),
    );
    let elems: Vec<Ty<'tcx>> = match ty.kind() {
        TyKind::Tuple(elems) => elems.iter().collect(),
        _ => panic!("WF-9 generic interop: expected a tuple type, got {ty:?}"),
    };
    elems
        .into_iter()
        .map(|elem| {
            let elem = ctx.monomorphize(elem);
            ctx.type_from_cache(elem)
        })
        .collect()
}
/// WF-9 binding-consistency guard. A definition-shape signature position that is a class generic
/// marker `!N` MUST resolve, via the concrete `class_generics`, to the SAME concrete type as the
/// corresponding runtime value (the declared `Ret`/`ArgK` of the magic fn). If it doesn't, the
/// binding the caller wrote is inconsistent (e.g. declaring a `List<i64>` return as `i32`) and would
/// **silently miscompile** — CoreCLR runs UNVERIFIED, so RyuJIT narrows/widens rather than rejecting.
/// Failing loud at codegen here is what makes the `is_assignable_to` `!N`-vs-concrete relaxation
/// *precisely* sound (the `!N` value provably equals its concrete binding) rather than merely trusted.
/// Method generics `!!N` (CallGeneric) are not validated (the current bridge carries no method generics).
fn check_generic_marker<'tcx>(
    sig_ty: Type,
    runtime_ty: Type,
    class_generics: &[Type],
    role: &str,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) {
    let Type::PlatformGeneric(n, GenericKind::TypeGeneric) = sig_ty else {
        return;
    };
    match class_generics.get(n as usize) {
        Some(&resolved) if resolved == runtime_ty => {}
        Some(&resolved) => ctx.tcx().dcx().span_fatal(
            ctx.span(),
            format!(
                "WF-9 generic interop: the `!{n}` {role} resolves to class generic {n} = {resolved:?}, but the declared runtime type is {runtime_ty:?}. The binding is inconsistent and would silently miscompile (CoreCLR runs unverified)."
            ),
        ),
        None => ctx.tcx().dcx().span_fatal(
            ctx.span(),
            format!(
                "WF-9 generic interop: a `!{n}` {role} references class generic {n}, but only {} class generic argument(s) were provided.",
                class_generics.len()
            ),
        ),
    }
}
/// Lower a magic-fn type-parameter `subst_ref[i]` (always a real type) to its .NET type.
fn garg_ty_to_type<'tcx>(
    garg: GenericArg<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Type {
    let ty = ctx.monomorphize(
        garg.as_type()
            .expect("WF-9 generic interop: expected a type parameter"),
    );
    ctx.type_from_cache(ty)
}
/// WF-9 — calls a method on a *generic* .NET instantiation (e.g. `List<i32>::Add`). The target class
/// carries concrete generic arguments (so the `ClassRef` renders `` List`1<int32> ``) and the method
/// signature is given in *definition* shape (`!N`/`!!N` markers), which is what a methodref on a
/// generic instantiation must use. `KIND`: 0 = static, 1 = `call instance`, 2 = `callvirt`.
fn call_generic<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let asm = AssemblyRef::decode_assembly_ref(subst_ref[0], ctx.tcx());
    let asm = asm.name().map(|name| ctx.alloc_string(name));
    let class_name = garg_to_string(subst_ref[1], ctx.tcx());
    let class_name = ctx.alloc_string(class_name);
    let is_valuetype = garag_to_bool(subst_ref[2], ctx.tcx());
    let managed_fn_name = garg_to_string(subst_ref[3], ctx.tcx());
    let managed_fn_name = ctx.alloc_string(managed_fn_name);
    let kind = garag_to_usize(subst_ref[4], ctx.tcx());
    // Concrete .NET type arguments of the class instantiation (e.g. the `(i32,)` of `List<i32>`).
    let class_generics = tuple_garg_to_types(subst_ref[5], ctx);
    // Definition-shape method signature: `(output, explicit-input0, …)` with `!N`/`!!N` markers.
    let mut sig_types = tuple_garg_to_types(subst_ref[6], ctx);
    assert!(
        !sig_types.is_empty(),
        "WF-9 generic interop: the signature tuple must carry at least a return type"
    );
    let output = sig_types.remove(0);
    let explicit_inputs = sig_types;

    // Loud-fail on an inconsistent binding (see `check_generic_marker`). Runtime types come from the
    // magic fn's declared `Ret` (subst[7]) and runtime args (subst[8..], receiver-first for
    // instance/virtual). The Sig excludes the receiver, so explicit input `j` pairs with runtime arg
    // `recv_offset + j`.
    let ret_ty = garg_ty_to_type(subst_ref[7], ctx);
    check_generic_marker(output, ret_ty, &class_generics, "return", ctx);
    let recv_offset = if kind == 0 { 0 } else { 1 };
    for (j, &sig_in) in explicit_inputs.iter().enumerate() {
        let arg_ty = garg_ty_to_type(subst_ref[8 + recv_offset + j], ctx);
        check_generic_marker(sig_in, arg_ty, &class_generics, "argument", ctx);
    }

    let this = ctx.alloc_class_ref(ClassRef::new(
        class_name,
        asm,
        is_valuetype,
        class_generics.into(),
    ));
    // Build the methodref signature in the cilly convention (the receiver, if any, is `inputs[0]`).
    let mut inputs = Vec::with_capacity(explicit_inputs.len() + 1);
    let mkind = match kind {
        0 => MethodKind::Static,
        1 => {
            assert!(
                !is_valuetype,
                "WF-9 generic interop: instance calls on generic *value types* (e.g. Span<T>) are not yet supported; use a reference type"
            );
            inputs.push(Type::ClassRef(this));
            MethodKind::Instance
        }
        2 => {
            inputs.push(Type::ClassRef(this));
            MethodKind::Virtual
        }
        _ => panic!("WF-9 generic interop: invalid call KIND {kind}"),
    };
    inputs.extend(explicit_inputs);
    let sig = ctx.sig(inputs, output);
    let mref = MethodRef::new(this, managed_fn_name, sig, mkind, vec![].into());
    let mref = ctx.alloc_methodref(mref);

    let mut call_args = Vec::new();
    for arg in args {
        call_args.push(handle_operand(&arg.node, ctx));
    }
    if output == Type::Void {
        ctx.call_root(mref, &call_args, IsPure::NOT)
    } else {
        let node = ctx.call(mref, &call_args, IsPure::NOT);
        place_set(destination, node, ctx)
    }
}
/// WF-9 — constructs a managed object of a *generic* .NET instantiation (e.g. `new List<i32>()`).
fn ctor_generic<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let asm = AssemblyRef::decode_assembly_ref(subst_ref[0], ctx.tcx());
    let asm = asm.name().map(|name| ctx.alloc_string(name));
    let class_name = garg_to_string(subst_ref[1], ctx.tcx());
    let class_name = ctx.alloc_string(class_name);
    let is_valuetype = garag_to_bool(subst_ref[2], ctx.tcx());
    let class_generics = tuple_garg_to_types(subst_ref[3], ctx);
    // The ctor signature tuple is `(ignored-return, explicit-input0, …)`. A `.ctor` methodref returns
    // void; only the explicit inputs matter. (The first slot keeps the `Sig` tuple shape uniform with
    // the call path, where slot 0 is the genuine return type.)
    let mut sig_types = tuple_garg_to_types(subst_ref[4], ctx);
    assert!(
        !sig_types.is_empty(),
        "WF-9 generic interop: the ctor signature tuple must carry at least the (ignored) return slot"
    );
    let _ignored_ret = sig_types.remove(0);
    let explicit_inputs = sig_types;

    // Loud-fail on an inconsistent binding (see `check_generic_marker`). A ctor takes no receiver, so
    // explicit input `j` pairs with runtime arg `subst[6 + j]`.
    for (j, &sig_in) in explicit_inputs.iter().enumerate() {
        let arg_ty = garg_ty_to_type(subst_ref[6 + j], ctx);
        check_generic_marker(sig_in, arg_ty, &class_generics, "argument", ctx);
    }

    let this = ctx.alloc_class_ref(ClassRef::new(
        class_name,
        asm,
        is_valuetype,
        class_generics.into(),
    ));
    let mut inputs = vec![Type::ClassRef(this)];
    inputs.extend(explicit_inputs);
    let sig = ctx.sig(inputs, Type::Void);
    let ctor = MethodRef::new(
        this,
        ctx.alloc_string(".ctor"),
        sig,
        MethodKind::Constructor,
        vec![].into(),
    );
    let ctor = ctx.alloc_methodref(ctor);
    let mut call_args = Vec::new();
    for arg in args {
        call_args.push(handle_operand(&arg.node, ctx));
    }
    let node = ctx.call(ctor, &call_args, IsPure::NOT);
    place_set(destination, node, ctx)
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
    source_info: rustc_middle::mir::SourceInfo,
) -> Vec<Root> {
    if let rustc_middle::ty::InstanceKind::Virtual(_def, fn_idx) = instance.def {
        assert!(!args.is_empty());

        let mut fat_ptr_address = operand_address(&args[0].node, ctx);
        let fat_ptr_dyn = ctx.alloc_string("FatPtrn3Dyn");
        let fat_ptr_dyn_cref = ctx.alloc_class_ref(ClassRef::new(fat_ptr_dyn, None, true, [].into()));
        // The `m`/METADATA and `d`/DATA_PTR loads below read from the canonical erased fat-ptr class
        // `FatPtrn3Dyn`. When the virtual-call receiver is a `#[repr(transparent)]` ADT over the fat
        // pointer (e.g. `Pin<&mut dyn Future>`), `operand_address` yields a pointer whose pointee class
        // is the WRAPPER, so the loads would be `FieldOwnerMismatch` (futures' `LocalFutureObj::poll`).
        // The wrapper's storage IS the inner fat pointer (repr(transparent)), so reinterpret it as
        // `FatPtrn3Dyn`. A bare `&dyn`/`*mut dyn` receiver is already `FatPtrn3Dyn` (no cast); a
        // non-transparent by-value receiver (`Box<dyn _>` `self`) is excluded — it must not be cast.
        let recv_ty = args[0].node.ty(ctx.body(), ctx.tcx());
        let recv_ty = ctx.monomorphize(recv_ty);
        if matches!(recv_ty.kind(), TyKind::Adt(adt_def, _) if adt_def.repr().transparent()) {
            fat_ptr_address = ctx.cast_ptr(fat_ptr_address, Type::ClassRef(fat_ptr_dyn_cref));
        }
        let vtable_ptr_field_desc = FieldDesc::new(
            fat_ptr_dyn_cref,
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
            fat_ptr_dyn_cref,
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
            source_info,
            ctx,
        );
    }
    let mut signature = call_info.sig().clone();
    // Checks if function is "magic"
    if function_name.contains(GENERIC_CTOR_FN_NAME) {
        assert!(
            !call_info.split_last_tuple(),
            "Generic constructors may not use the `rust_call` calling convention!"
        );
        // WF-9: `new List<i32>()` and friends.
        return vec![ctor_generic(instance.args, args, destination, ctx)];
    } else if function_name.contains(GENERIC_CALL_FN_NAME) {
        assert!(
            !call_info.split_last_tuple(),
            "Generic managed calls may not use the `rust_call` calling convention!"
        );
        // WF-9: `List<i32>::Add(…)` and friends.
        return vec![call_generic(instance.args, args, destination, ctx)];
    } else if function_name.contains(MANAGED_THROW) {
        // `rustc_clr_interop_throw::<MSG>()` raises a managed `System.Exception(MSG)` directly (via the
        // `throw` IL op), so a .NET caller can `catch` it. Unlike a Rust `panic!` — which goes through
        // the unwinder and faults when it reaches a managed frame — this is an ordinary managed throw.
        // The fn returns `!`, so there is no destination; `throw` is a terminal op (the caller appends
        // the usual "diverging call returned" guard after it, exactly as for `panic!`).
        let msg = garg_to_string(instance.args[0], ctx.tcx());
        return vec![ctx.throw_msg(&msg)];
    } else if function_name.contains(CTOR_FN_NAME) {
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
    } else if function_name.contains(MANAGED_NEW_ARR) {
        assert!(
            !call_info.split_last_tuple(),
            "Managed calls may not use the `rust_call` calling convention!"
        );
        // Allocates a managed 1-D array of the (primitive) element type `T` with `len` elements.
        // The element type is the first generic argument of the intrinsic.
        let elem = ctx.type_from_cache(instance.args[0].as_type().unwrap());
        let elem = ctx.alloc_type(elem);
        let len = handle_operand(&args[0].node, ctx);
        let node = ctx.new_arr(elem, len);
        return vec![place_set(destination, node, ctx)];
    } else if function_name.contains(MANAGED_SET_ELEM) {
        assert!(
            !call_info.split_last_tuple(),
            "Managed calls may not use the `rust_call` calling convention!"
        );
        // Stores `val` into managed array `arr` at `idx`. Side-effecting; destination is unit.
        let elem = ctx.type_from_cache(instance.args[0].as_type().unwrap());
        let elem = ctx.alloc_type(elem);
        let arr = handle_operand(&args[0].node, ctx);
        let idx = handle_operand(&args[1].node, ctx);
        let val = handle_operand(&args[2].node, ctx);
        let root = ctx.st_elem(arr, idx, val, elem);
        let root = ctx.alloc_root(root);
        return vec![root];
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
        // The callee is `#[track_caller]`: rustc appends an implicit `&core::panic::Location` param
        // that the *call site* must supply (this is why `FnSig` ≠ `FnAbi` for track_caller fns).
        // Supply the correct caller location: if *we* are also track_caller, forward our own implicit
        // arg (so the location propagates up the chain to the real user site); otherwise, and after
        // accounting for any MIR-inlined track_caller frames, materialize it from the call-site span.
        // Previously this unconditionally materialized the local span, which both lost the propagation
        // and (under MIR inlining) reported the inlined body's span instead of the user's.
        let location = crate::terminator::get_caller_location(ctx, source_info);
        call_args.push(location);
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
    source_info: rustc_middle::mir::SourceInfo,
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
    call_inner(fn_type, instance, ctx, args, destination, source_info)
}
