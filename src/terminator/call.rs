use crate::abi::AbiPlan;
use crate::fn_ctx::fn_name_for_instance;
use crate::operand::{handle_operand, operand_address};
use crate::place::{place_address, place_set};
use crate::r#type::{
    GetTypeExt,
    utilis::{garg_to_string, garg_to_usize},
};
use crate::{
    assembly::MethodCompileCtx,
    interop::AssemblyRef,
    utilis::{
        CTOR_FN_NAME, MANAGED_CALL_FN_NAME, MANAGED_CALL_VIRT_FN_NAME, MagicFn, classify_magic_fn,
        garg_to_bool,
    },
};
use cilly::tpe::GenericKind;
use cilly::{
    Access, BasicBlock, BinOp, CILNode, CILRoot, ClassDef, ClassRef, Const, FieldDesc, FnSig,
    IString, Int, Interned, IntoAsmIndex, MethodDef, MethodImpl,
    cilnode::{ExtendKind, IsPure, MethodKind, PtrCastRes},
};
use cilly::{MethodRef, Type};
use rustc_middle::ty::InstanceKind;
use rustc_middle::{
    mir::{Operand, Place},
    ty::{GenericArg, Instance, Ty, TyKind},
};
use rustc_span::Spanned;

type Root = Interned<cilly::ir::CILRoot>;

fn emit_call<'tcx>(
    mref: Interned<MethodRef>,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    output: Type,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let call_args: Vec<_> = args
        .iter()
        .map(|arg| handle_operand(&arg.node, ctx))
        .collect();
    if output == Type::Void {
        ctx.call_root(mref, &call_args, IsPure::NOT)
    } else {
        let node = ctx.call(mref, &call_args, IsPure::NOT);
        place_set(destination, node, ctx)
    }
}

fn emit_constructor_call<'tcx>(
    class: Interned<ClassRef>,
    explicit_inputs: Vec<Type>,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let mut inputs = Vec::with_capacity(explicit_inputs.len() + 1);
    inputs.push(Type::ClassRef(class));
    inputs.extend(explicit_inputs);
    let sig = ctx.sig(inputs, Type::Void);
    let ctor = MethodRef::new(
        class,
        ctx.alloc_string(".ctor"),
        sig,
        MethodKind::Constructor,
        vec![].into(),
    );
    let ctor = ctx.alloc_methodref(ctor);
    let call_args: Vec<_> = args
        .iter()
        .map(|arg| handle_operand(&arg.node, ctx))
        .collect();
    let node = ctx.call(ctor, &call_args, IsPure::NOT);
    place_set(destination, node, ctx)
}

/// Emit the shared managed `Invoke` method used by the function-pointer and closure delegate
/// shims. The callers only differ in which fields they load and which arguments they prepend;
/// method signature, return handling, and argument metadata are identical.
fn emit_delegate_invoke<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    shim_def: cilly::ir::class::ClassDefIdx,
    shim_cref: Interned<ClassRef>,
    inputs: &[Type],
    output: Type,
    call_sig: Interned<FnSig>,
    fnptr_val: Interned<CILNode>,
    invoke_call_args: Vec<Interned<CILNode>>,
) {
    let invoke_name = ctx.alloc_string("Invoke");
    let mut invoke_sig_inputs = Vec::with_capacity(inputs.len() + 1);
    invoke_sig_inputs.push(Type::ClassRef(shim_cref));
    invoke_sig_inputs.extend(inputs.iter().copied());
    let invoke_sig = ctx.sig(invoke_sig_inputs, output);
    let invoke_body = if output == Type::Void {
        let call = ctx.call_indirect_root(call_sig, fnptr_val, invoke_call_args);
        let ret = ctx.alloc_root(CILRoot::VoidRet);
        vec![call, ret]
    } else {
        let call = ctx.call_indirect(call_sig, fnptr_val, invoke_call_args);
        let ret = ctx.alloc_root(CILRoot::Ret(call));
        vec![ret]
    };
    let mut invoke_arg_names = vec![None];
    invoke_arg_names.extend((0..inputs.len()).map(|_| None));
    ctx.new_method(MethodDef::new(
        Access::Public,
        shim_def,
        invoke_name,
        invoke_sig,
        MethodKind::Instance,
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(invoke_body, 0, None)],
            locals: vec![],
        },
        invoke_arg_names,
    ));
}

fn finish_delegate<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    asm: Option<Interned<IString>>,
    class_name: Interned<IString>,
    is_valuetype: bool,
    class_generics: Vec<Type>,
    shim_cref: Interned<ClassRef>,
    shim_ctor_inputs: Vec<Type>,
    shim_ctor_args: Vec<Interned<CILNode>>,
    inputs: &[Type],
    output: Type,
    destination: &Place<'tcx>,
) -> Root {
    let mut shim_sig_inputs = Vec::with_capacity(shim_ctor_inputs.len() + 1);
    shim_sig_inputs.push(Type::ClassRef(shim_cref));
    shim_sig_inputs.extend(shim_ctor_inputs);
    let shim_ctor = MethodRef::new(
        shim_cref,
        ctx.alloc_string(".ctor"),
        ctx.sig(shim_sig_inputs, Type::Void),
        MethodKind::Constructor,
        vec![].into(),
    );
    let shim_ctor = ctx.alloc_methodref(shim_ctor);
    let shim_obj = ctx.call(shim_ctor, &shim_ctor_args, IsPure::NOT);

    let mut invoke_sig_inputs = Vec::with_capacity(inputs.len() + 1);
    invoke_sig_inputs.push(Type::ClassRef(shim_cref));
    invoke_sig_inputs.extend(inputs.iter().copied());
    let invoke_sig = ctx.sig(invoke_sig_inputs, output);
    let shim_invoke = MethodRef::new(
        shim_cref,
        ctx.alloc_string("Invoke"),
        invoke_sig,
        MethodKind::Instance,
        vec![].into(),
    );
    let shim_invoke = ctx.alloc_methodref(shim_invoke);
    let invoke_ftn = ctx.ld_ftn(shim_invoke);
    let invoke_ftn = ctx.alloc_node(CILNode::PtrCast(invoke_ftn, Box::new(PtrCastRes::ISize)));

    let delegate_cref = ctx.alloc_class_ref(ClassRef::new(
        class_name,
        asm,
        is_valuetype,
        class_generics.into(),
    ));
    let delegate_ctor_sig = ctx.sig(
        [
            Type::ClassRef(delegate_cref),
            Type::PlatformObject,
            Type::Int(Int::ISize),
        ],
        Type::Void,
    );
    let delegate_ctor = MethodRef::new(
        delegate_cref,
        ctx.alloc_string(".ctor"),
        delegate_ctor_sig,
        MethodKind::Constructor,
        vec![].into(),
    );
    let delegate_ctor = ctx.alloc_methodref(delegate_ctor);
    let delegate = ctx.call(delegate_ctor, &[shim_obj, invoke_ftn], IsPure::NOT);
    place_set(destination, delegate, ctx)
}

fn argc_from_fn_name(function_name: &str, prefix: &str) -> u32 {
    let argc_start = function_name.find(prefix).unwrap() + (prefix.len());
    let argc_end = argc_start + function_name[argc_start..].find('_').unwrap();
    let argument_count = &function_name[argc_start..argc_end];
    argument_count.parse::<u32>().unwrap()
}
/// The common `<ASSEMBLY, CLASS_PATH, IS_VALUETYPE>` prefix shared by every interop magic-fn's
/// generic-argument list (`subst[0..3]`). Every managed-call/ctor path — `call_managed`,
/// `callvirt_managed`, `call_generic`, `ctor_generic`, `call_ctor` — names the target .NET class the
/// same way, so this header is decoded once instead of repeating the position-0/1/2 reads in each (the
/// off-by-one-prone manual indexing was duplicated nearly verbatim five times). The per-fn trailing
/// positional reads (`subst[3..]`) stay where they are.
struct InteropHeader {
    /// The containing assembly, or `None` when the class lives in the assembly being compiled.
    asm: Option<Interned<IString>>,
    /// The interned, demangled .NET class path (e.g. `System.Collections.Generic.List`).
    class_name: Interned<IString>,
    /// Whether the target is a value type (`true`) or a reference type (`false`).
    is_vt: bool,
}
impl InteropHeader {
    /// Decode `subst[0]`=assembly, `subst[1]`=class path, `subst[2]`=is-valuetype.
    fn decode<'tcx>(subst: &[GenericArg<'tcx>], ctx: &mut MethodCompileCtx<'tcx, '_>) -> Self {
        let asm = AssemblyRef::decode_assembly_ref(subst[0], ctx.tcx());
        let asm = asm.name().map(|name| ctx.alloc_string(name));
        let class_name = garg_to_string(subst[1], ctx.tcx());
        let class_name = ctx.alloc_string(class_name);
        let is_vt = garg_to_bool(subst[2], ctx.tcx());
        Self {
            asm,
            class_name,
            is_vt,
        }
    }
}
/// Shared lowering for the two managed-call magic-fn families. Their only semantic difference is
/// how a non-static reference-type receiver is dispatched (`instance` for `managedN`, `virtual`
/// for `managed_virtN`); argument decoding and result placement are identical.
fn emit_managed_call<'tcx>(
    header: InteropHeader,
    managed_fn_name: Interned<IString>,
    argc: u32,
    is_static: bool,
    virtual_path: bool,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    fn_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = header;
    let signature = AbiPlan::from_instance(fn_instance, ctx).signature().clone();
    let output = *signature.output();
    let (sig, kind) = if argc == 0 {
        // Keep the ABI's real return type for zero-argument getters; using Void loses managed
        // references before they reach the destination.
        (ctx.sig([], output), MethodKind::Static)
    } else {
        let kind = if is_static {
            MethodKind::Static
        } else if virtual_path || !is_valuetype {
            // Reference receivers use callvirt; unboxed value types require a plain instance call.
            MethodKind::Virtual
        } else {
            MethodKind::Instance
        };
        (ctx.alloc_sig(signature), kind)
    };
    let owner = ctx.alloc_class_ref(ClassRef::new(class_name, asm, is_valuetype, [].into()));
    let mref = ctx.alloc_methodref(MethodRef::new(
        owner,
        managed_fn_name,
        sig,
        kind,
        vec![].into(),
    ));
    emit_call(mref, args, destination, output, ctx)
}

/// Calls a non-virtual managed function (used for interop).
fn call_managed<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    function_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    fn_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let argc = argc_from_fn_name(function_name, MANAGED_CALL_FN_NAME);
    assert!(args.len() == argc as usize);
    let header = InteropHeader::decode(subst_ref, ctx);
    let managed_fn_name = garg_to_string(subst_ref[3], ctx.tcx());
    let managed_fn_name = ctx.alloc_string(managed_fn_name);
    let is_static = argc == 0 || garg_to_bool(subst_ref[4], ctx.tcx());
    emit_managed_call(
        header,
        managed_fn_name,
        argc,
        is_static,
        false,
        args,
        destination,
        fn_instance,
        ctx,
    )
}

/// Lowers `rustc_clr_interop_managed_get_field` to a typed `ldfld` without synthesizing or
/// resolving an accessor method. This is the stable primitive behind generated CLR value objects:
/// their field access must not depend on comptime accessor registration order.
fn managed_get_field<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    assert_eq!(
        args.len(),
        1,
        "managed field reads require exactly one owner"
    );
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);
    let owner = ctx.alloc_class_ref(ClassRef::new(class_name, asm, is_valuetype, [].into()));
    let field_name = garg_to_string(subst_ref[3], ctx.tcx());
    let field_name = ctx.alloc_string(field_name);
    let field_type = ctx.type_from_cache(
        ctx.monomorphize(subst_ref[4])
            .as_type()
            .expect("managed field return must be a type"),
    );
    let descriptor = ctx.alloc_field(FieldDesc::new(owner, field_name, field_type));
    let object = handle_operand(&args[0].node, ctx);
    let value = ctx.ld_field(object, descriptor);
    place_set(destination, value, ctx)
}
/// Calls a virtual managed function (used for interop).
fn callvirt_managed<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    function_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    fn_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let argc = argc_from_fn_name(function_name, MANAGED_CALL_VIRT_FN_NAME);
    //assert!(subst_ref.len() as u32 == argc + 3 || subst_ref.len() as u32 == argc + 4);
    assert!(u32::try_from(args.len()).expect("More than 2^32 function arguments.") == argc);
    let header = InteropHeader::decode(subst_ref, ctx);
    let managed_fn_garg = ctx.monomorphize(subst_ref[3]);
    let managed_fn_name = garg_to_string(managed_fn_garg, ctx.tcx());
    let managed_fn_name = ctx.alloc_string(managed_fn_name);
    let is_static = argc == 0 || garg_to_bool(subst_ref[4], ctx.tcx());
    emit_managed_call(
        header,
        managed_fn_name,
        argc,
        is_static,
        true,
        args,
        destination,
        fn_instance,
        ctx,
    )
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
/// Method generics `!!N` (CallGeneric) are validated the same way against the concrete
/// `method_generics` (the type arguments carried on the generic-method call — see `call_gmethod`).
fn check_generic_marker<'tcx>(
    sig_ty: Type,
    runtime_ty: Type,
    class_generics: &[Type],
    method_generics: &[Type],
    role: &str,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) {
    match sig_ty {
        // Leaf: a class generic `!N` resolves via `class_generics`; a method generic `!!N` via
        // `method_generics`. Either must resolve to EXACTLY the runtime type.
        Type::PlatformGeneric(n, kind) => {
            let (gens, prefix, which) = match kind {
                GenericKind::TypeGeneric => (class_generics, "!", "class"),
                // `!!N` — the `RustcCLRInteropMethodGeneric` marker lowers to `CallGeneric`; treat the
                // legacy `MethodGeneric` variant the same (both are method type parameters).
                GenericKind::CallGeneric | GenericKind::MethodGeneric => {
                    (method_generics, "!!", "method")
                }
            };
            match gens.get(n as usize) {
                Some(&resolved) if resolved == runtime_ty => {}
                Some(&resolved) => ctx.tcx().dcx().span_fatal(
                    ctx.span(),
                    format!(
                        "WF-9 generic interop: the `{prefix}{n}` {role} resolves to {which} generic {n} = {resolved:?}, but the declared runtime type is {runtime_ty:?}. The binding is inconsistent and would silently miscompile (CoreCLR runs unverified)."
                    ),
                ),
                None => ctx.tcx().dcx().span_fatal(
                    ctx.span(),
                    format!(
                        "WF-9 generic interop: a `{prefix}{n}` {role} references {which} generic {n}, but only {} {which} generic argument(s) were provided.",
                        gens.len()
                    ),
                ),
            }
        }
        // Nested generic: a def-shape type like `Dictionary<K,V>.KeyCollection<!0,!1>`,
        // `Comparison<!0>`, or `Task<!0>`. When the runtime type is the SAME open generic (same
        // name/assembly/valuetype and arity), recurse pairwise into the generic arguments so every
        // nested `!N` is proven to resolve to exactly the runtime argument in that position — the same
        // codegen-time proof the bare-`!N` leaf gets. This is what makes the `is_assignable_to`
        // nested-ClassRef relaxation *precisely* sound rather than merely trusted.
        Type::ClassRef(sig_cref) => {
            let Type::ClassRef(rt_cref) = runtime_ty else {
                return;
            };
            let (same_open, sig_gen, rt_gen) = {
                let s = ctx.class_ref(sig_cref);
                let r = ctx.class_ref(rt_cref);
                let same = s.name() == r.name()
                    && s.asm() == r.asm()
                    && s.is_valuetype() == r.is_valuetype()
                    && s.generics().len() == r.generics().len();
                (same, s.generics().to_vec(), r.generics().to_vec())
            };
            if same_open {
                for (sg, rg) in sig_gen.into_iter().zip(rt_gen) {
                    check_generic_marker(sg, rg, class_generics, method_generics, role, ctx);
                }
            }
        }
        // A pointer/byref to a marker — e.g. `Span<T>.get_Item` returns `!0&` (a `Ptr(!0)`), produced
        // into a concrete `*mut T`. Recurse into the pointees so the nested `!N` is proven consistent.
        Type::Ptr(sig_inner) | Type::Ref(sig_inner) => {
            let Some(rt_inner) = runtime_ty.pointed_to() else {
                return;
            };
            let inner_sig = ctx[sig_inner];
            let inner_rt = ctx[rt_inner];
            check_generic_marker(
                inner_sig,
                inner_rt,
                class_generics,
                method_generics,
                role,
                ctx,
            );
        }
        // A definition-shape array such as `!0[]` must bind element-for-element to the concrete
        // runtime array (`i32[]`, etc.). Rank mismatches are rejected by normal type checking; for
        // equal-rank arrays recurse so the same exact-binding proof covers the nested marker.
        Type::PlatformArray {
            elem: sig_elem,
            dims: sig_dims,
        } => {
            let Type::PlatformArray {
                elem: rt_elem,
                dims: rt_dims,
            } = runtime_ty
            else {
                return;
            };
            if sig_dims == rt_dims {
                check_generic_marker(
                    ctx[sig_elem],
                    ctx[rt_elem],
                    class_generics,
                    method_generics,
                    role,
                    ctx,
                );
            }
        }
        _ => {}
    }
}
/// Lower a magic-fn type-parameter `subst_ref[i]` (always a real type) to its .NET type.
fn garg_ty_to_type<'tcx>(garg: GenericArg<'tcx>, ctx: &mut MethodCompileCtx<'tcx, '_>) -> Type {
    let ty = ctx.monomorphize(
        garg.as_type()
            .expect("WF-9 generic interop: expected a type parameter"),
    );
    ctx.type_from_cache(ty)
}

/// Whether `tpe` is represented by a direct CLR object reference rather than a boxed value.
///
/// `ManagedBox*` stores the value behind a `GCHandle`. CLR arrays, strings, and `System.Object`
/// are references just like non-value `ClassRef`s: applying `box`/`unbox.any` to them is invalid
/// IL. Keep this exact instead of using `Type::is_gcref`, whose conservative generic-parameter
/// case is intentionally allowed to report false positives.
fn is_direct_managed_reference(tpe: Type, ctx: &MethodCompileCtx<'_, '_>) -> bool {
    match tpe {
        Type::ClassRef(class) => !ctx[class].is_valuetype(),
        Type::PlatformArray { .. } | Type::PlatformObject | Type::PlatformString => true,
        _ => false,
    }
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
    call_generic_inner(
        subst_ref,
        args,
        destination,
        ctx,
        "generic interop",
        6,
        7,
        vec![],
    )
}
/// WF-9 — calls a *generic method* (`!!N`), i.e. a method that itself takes type arguments, e.g.
/// `Activator.CreateInstance<T>()`, `JsonSerializer.Deserialize<T>(s)`, `provider.GetService<T>()`.
/// Mirrors [`call_generic`], but the methodref *carries the method's concrete type arguments* (so the
/// exporter renders `Method<int32>`) and the signature may use `!!N` markers (resolved via the method
/// generics) in addition to `!N` (the class generics). subst layout inserts a `MethodGenerics` tuple
/// after `ClassGenerics`:
///   `[0]`=assembly `[1]`=class `[2]`=is-vt `[3]`=method `[4]`=KIND `[5]`=ClassGenerics
///   `[6]`=MethodGenerics `[7]`=Sig `[8]`=Ret `[9..]`=runtime arg types.
fn call_gmethod<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    // The method's own concrete type arguments (e.g. the `(int32,)` of `CreateInstance<int32>`).
    let method_generics = tuple_garg_to_types(subst_ref[6], ctx);
    assert!(
        !method_generics.is_empty(),
        "WF-9 generic method: a generic method call must carry at least one method type argument"
    );
    call_generic_inner(
        subst_ref,
        args,
        destination,
        ctx,
        "generic method",
        7,
        8,
        method_generics,
    )
}

/// Shared WF-9 lowering for calls whose target is a generic class, with an optional concrete
/// method-generic argument list. The two magic-fn layouts differ only in the signature/return
/// tuple offsets; keeping the validation, receiver dispatch, and `MethodRef` construction here
/// prevents those layouts from drifting apart.
fn call_generic_inner<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    family: &str,
    signature_index: usize,
    runtime_return_index: usize,
    method_generics: Vec<Type>,
) -> Root {
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);
    let managed_fn_name = garg_to_string(subst_ref[3], ctx.tcx());
    let managed_fn_name = ctx.alloc_string(managed_fn_name);
    let kind = garg_to_usize(subst_ref[4], ctx.tcx());
    let class_generics = tuple_garg_to_types(subst_ref[5], ctx);
    let mut sig_types = tuple_garg_to_types(subst_ref[signature_index], ctx);
    assert!(
        !sig_types.is_empty(),
        "WF-9 {family}: the signature tuple must carry at least a return type"
    );
    let output = sig_types.remove(0);

    let ret_ty = garg_ty_to_type(subst_ref[runtime_return_index], ctx);
    check_generic_marker(
        output,
        ret_ty,
        &class_generics,
        &method_generics,
        "return",
        ctx,
    );
    let recv_offset = usize::from(kind != 0);
    for (j, &sig_in) in sig_types.iter().enumerate() {
        let arg_ty = garg_ty_to_type(subst_ref[runtime_return_index + 1 + recv_offset + j], ctx);
        check_generic_marker(
            sig_in,
            arg_ty,
            &class_generics,
            &method_generics,
            "argument",
            ctx,
        );
    }

    let this = ctx.alloc_class_ref(ClassRef::new(
        class_name,
        asm,
        is_valuetype,
        class_generics.into(),
    ));
    let mut inputs = Vec::with_capacity(sig_types.len() + 1);
    let mkind = match kind {
        0 => MethodKind::Static,
        1 => {
            inputs.push(if is_valuetype {
                ctx.nref(Type::ClassRef(this))
            } else {
                Type::ClassRef(this)
            });
            MethodKind::Instance
        }
        2 => {
            inputs.push(Type::ClassRef(this));
            MethodKind::Virtual
        }
        _ => panic!("WF-9 {family}: invalid call KIND {kind}"),
    };
    inputs.extend(sig_types);
    let sig = ctx.sig(inputs, output);
    let mref = MethodRef::new(this, managed_fn_name, sig, mkind, method_generics.into());
    let mref = ctx.alloc_methodref(mref);
    emit_call(mref, args, destination, output, ctx)
}
/// WF-9 — constructs a managed object of a *generic* .NET instantiation (e.g. `new List<i32>()`).
fn ctor_generic<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);
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
        check_generic_marker(sig_in, arg_ty, &class_generics, &[], "argument", ctx);
    }

    let this = ctx.alloc_class_ref(ClassRef::new(
        class_name,
        asm,
        is_valuetype,
        class_generics.into(),
    ));
    emit_constructor_call(this, explicit_inputs, args, destination, ctx)
}
/// Delegates & callbacks — wrap a Rust `extern` fn pointer into a managed .NET delegate instance
/// (`Action<..>` / `Func<.., R>`), so a Rust callback can be passed to any .NET API that takes a
/// delegate (`List.ForEach`, a sort comparator, LINQ, an event `add_*`).
///
/// A managed delegate must be constructed via `ldftn <managed method>; newobj Delegate::.ctor(object,
/// native int)` — the `native int` has to be the address of a *managed* method whose signature matches
/// the delegate's `Invoke`, NOT a raw native pointer. Our callback arrives as a native `FnPtr` (a
/// capture-less closure / `fn` item is coerced to one before it reaches here), so we synthesise a small
/// managed **shim** class per concrete signature, holding the native pointer in a field, whose `Invoke`
/// method `calli`s it. Then `newobj shim(fnptr)` → `ldftn shim::Invoke` → `newobj Delegate::.ctor`.
/// This is the exact dance `insert_dotnet_thread_spawn` performs for `ThreadStart`, generalised to any
/// arity and to the generic `Func`/`Action` families.
///
/// The shim's `Invoke` signature is the *concrete* lowered signature (from the `Sig` tuple), which by
/// construction equals the delegate's instantiated `Invoke` — so `newobj Func`N<T..>::.ctor(object,
/// native int)` binding `ldftn shim::Invoke` is sound (this is exactly what C#'s
/// `new Func<..>(obj.Invoke)` compiles to). Keeping the shim `Invoke` concrete is why the delegate type
/// mapping stays exact: the class generics on the delegate are the concrete `ClassGenerics`, and every
/// runtime value crosses with its ordinary Rust type.
///
/// subst layout (mirrors the WF-9 generic family header + a fn-ptr tail):
///   `[0]`=assembly `[1]`=delegate class path `[2]`=is-valuetype(false)
///   `[3]`=`ClassGenerics` tuple (concrete delegate type args, e.g. `(i32, bool)` for `Func<i32,bool>`)
///   `[4]`=`Sig` tuple `(Ret, In0, In1, …)` — the *concrete* signature the pointer is called with
///   `[5]`=`FnPtrTy` (the fn-ptr type of the argument; unused, the value carries the pointer)
fn delegate_from_fnptr<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    assert_eq!(
        args.len(),
        1,
        "rustc_clr_interop_delegate takes exactly one argument (the fn pointer)"
    );
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);
    // Concrete .NET type arguments of the delegate instantiation (e.g. the `(i32, bool)` of
    // `Func<i32, bool>`). May be empty for a non-generic delegate (rare; e.g. plain `Action`).
    let class_generics = tuple_garg_to_types(subst_ref[3], ctx);
    // The concrete signature the native pointer is invoked with: `(Ret, In0, In1, …)`.
    let mut sig_types = tuple_garg_to_types(subst_ref[4], ctx);
    assert!(
        !sig_types.is_empty(),
        "rustc_clr_interop_delegate: the signature tuple must carry at least a return type"
    );
    let output = sig_types.remove(0);
    let inputs = sig_types;

    // --- The native `fn`-pointer signature the shim `calli`s and its field type. ---
    let shim_fn_sig = ctx.sig(inputs.clone(), output);
    let shim_fn_ptr_ty = Type::FnPtr(shim_fn_sig);

    // --- Build (once) the monomorphic shim class holding the native pointer. ---
    // Name it uniquely by the concrete signature so distinct delegate shapes get distinct shims; a
    // second delegate of the *same* shape reuses the memoised class (re-defining a class name panics).
    let shim_name = format!("RustDelegateShim_{}", shim_fn_ptr_ty.mangle(ctx));
    let shim_name = ctx.alloc_string(shim_name);
    let shim_cref = ctx.alloc_class_ref(ClassRef::new(shim_name, None, false, [].into()));
    let fnptr_field_name = ctx.alloc_string("fnptr");
    let fnptr_field = ctx.alloc_field(FieldDesc::new(shim_cref, fnptr_field_name, shim_fn_ptr_ty));
    if !ctx
        .class_defs()
        .contains_key(&cilly::ir::class::ClassDefIdx(shim_cref))
    {
        let object = ClassRef::object(ctx);
        let shim_def = ctx
            .class_def(ClassDef::new(
                shim_name,
                false,
                0,
                Some(object),
                vec![(shim_fn_ptr_ty, fnptr_field_name, None)],
                vec![],
                // Extern: keep the linker's dead-code pass from pruning a shim whose only reference is
                // the `ldftn` inside the delegate `newobj` (matches `UnmanagedThreadStart`).
                Access::Extern,
                None,
                None,
                true,
            ))
            .expect("rustc_clr_interop_delegate: shim class layout check failed");

        // ---- shim `.ctor(this, fnptr)` : stores the pointer into the field ----
        let ctor_name = ctx.alloc_string(".ctor");
        let ctor_this = ctx.alloc_node(cilly::CILNode::LdArg(0));
        let ctor_arg = ctx.alloc_node(cilly::CILNode::LdArg(1));
        let set_field = ctx.alloc_root(cilly::CILRoot::SetField(Box::new((
            fnptr_field,
            ctor_this,
            ctor_arg,
        ))));
        let ctor_ret = ctx.alloc_root(cilly::CILRoot::VoidRet);
        let ctor_sig = ctx.sig([Type::ClassRef(shim_cref), shim_fn_ptr_ty], Type::Void);
        ctx.new_method(MethodDef::new(
            Access::Public,
            shim_def,
            ctor_name,
            ctor_sig,
            MethodKind::Constructor,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![set_field, ctor_ret], 0, None)],
                locals: vec![],
            },
            vec![None, Some(fnptr_field_name)],
        ));

        // The `Invoke` receiver is arg 0; the explicit inputs are args 1..=N.
        let mut invoke_call_args = Vec::with_capacity(inputs.len());
        for (i, _in_ty) in inputs.iter().enumerate() {
            let a = ctx.alloc_node(cilly::CILNode::LdArg(
                u32::try_from(i + 1).expect("delegate shim: too many arguments"),
            ));
            invoke_call_args.push(a);
        }
        let invoke_this = ctx.alloc_node(cilly::CILNode::LdArg(0));
        let fnptr_val = ctx.ld_field(invoke_this, fnptr_field);
        // The methodref receiver goes at sig position 0 (cilly convention); the shim `calli` sig is the
        // *native* pointer sig (no receiver), so a separate value list is used for the indirect call.
        emit_delegate_invoke(
            ctx,
            shim_def,
            shim_cref,
            &inputs,
            output,
            shim_fn_sig,
            fnptr_val,
            invoke_call_args,
        );
    }

    // --- Emit: newobj shim(fnptr) ; ldftn shim::Invoke ; newobj Delegate::.ctor(object, native int) ---
    let fnptr_arg = handle_operand(&args[0].node, ctx);
    // The incoming pointer is a `FnPtr` value; normalise it to the shim ctor's declared `FnPtr` param
    // type (a `ReifyFnPointer`/`ClosureFnPointer` coercion already produced a `FnPtr`, but its concrete
    // sig may differ in receiver-elision). Cast directly to `FnPtr` — NOT `cast_ptr`, which would wrap
    // it in a `Ptr(FnPtr)` (that mismatch was `CallArgTypeWrong got p1i32v expected 1i32v`).
    let fnptr_arg = ctx.alloc_node(cilly::CILNode::PtrCast(
        fnptr_arg,
        Box::new(cilly::cilnode::PtrCastRes::FnPtr(shim_fn_sig)),
    ));

    finish_delegate(
        ctx,
        asm,
        class_name,
        is_valuetype,
        class_generics,
        shim_cref,
        vec![shim_fn_ptr_ty],
        vec![fnptr_arg],
        &inputs,
        output,
        destination,
    )
}
/// Delegates & callbacks — wrap a **capturing** Rust closure into a managed delegate. Unlike
/// [`delegate_from_fnptr`] (a capture-less `fn`), the closure has an environment, so the caller (the
/// mycorrhiza `from_closure`) boxes it to a thin `*mut ()` and hands us BOTH that env pointer and a
/// monomorphic trampoline `extern "C" fn(env, In..) -> Ret` that reconstructs the closure and calls it.
/// The synthesised shim holds two fields (env + trampoline) and its `Invoke(this, In..)` loads the env
/// field, prepends it to the args, and `calli`s the trampoline — so the closure's captured state rides
/// along on the .NET side.
///
/// subst layout: `[0]`=assembly `[1]`=delegate class `[2]`=is-vt `[3]`=`ClassGenerics`
///   `[4]`=`Sig` `(Ret, In0, …)` (the delegate's Invoke signature, NO env) `[5]`=`EnvTy` (`*mut ()`)
///   `[6]`=`FnPtrTy` (the trampoline's type; unused, the value carries it). args: `[0]`=env `[1]`=trampoline.
fn delegate_from_closure<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    use cilly::cilnode::PtrCastRes;
    assert_eq!(
        args.len(),
        2,
        "rustc_clr_interop_delegate_closure takes two arguments (env pointer, trampoline fn pointer)"
    );
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);
    let class_generics = tuple_garg_to_types(subst_ref[3], ctx);
    let mut sig_types = tuple_garg_to_types(subst_ref[4], ctx);
    assert!(
        !sig_types.is_empty(),
        "rustc_clr_interop_delegate_closure: the signature tuple must carry at least a return type"
    );
    let output = sig_types.remove(0);
    let inputs = sig_types;
    let env_ty = garg_ty_to_type(subst_ref[5], ctx);

    // The trampoline is invoked as `(env, In0, …) -> Ret` — env prepended to the delegate's inputs.
    let mut tramp_inputs = vec![env_ty];
    tramp_inputs.extend(inputs.iter().copied());
    let tramp_fn_sig = ctx.sig(tramp_inputs, output);
    let tramp_fn_ptr_ty = Type::FnPtr(tramp_fn_sig);

    // Memoised per (env, trampoline-sig) shape.
    let shim_name = format!("RustClosureShim_{}", tramp_fn_ptr_ty.mangle(ctx));
    let shim_name = ctx.alloc_string(shim_name);
    let shim_cref = ctx.alloc_class_ref(ClassRef::new(shim_name, None, false, [].into()));
    let env_field_name = ctx.alloc_string("env");
    let env_field = ctx.alloc_field(FieldDesc::new(shim_cref, env_field_name, env_ty));
    let fnptr_field_name = ctx.alloc_string("fnptr");
    let fnptr_field = ctx.alloc_field(FieldDesc::new(shim_cref, fnptr_field_name, tramp_fn_ptr_ty));
    if !ctx
        .class_defs()
        .contains_key(&cilly::ir::class::ClassDefIdx(shim_cref))
    {
        let object = ClassRef::object(ctx);
        let shim_def = ctx
            .class_def(ClassDef::new(
                shim_name,
                false,
                0,
                Some(object),
                vec![
                    (env_ty, env_field_name, None),
                    (tramp_fn_ptr_ty, fnptr_field_name, None),
                ],
                vec![],
                Access::Extern,
                None,
                None,
                true,
            ))
            .expect("rustc_clr_interop_delegate_closure: shim class layout check failed");

        // ---- shim `.ctor(this, env, fnptr)` ----
        let ctor_name = ctx.alloc_string(".ctor");
        let ctor_this = ctx.alloc_node(cilly::CILNode::LdArg(0));
        let ctor_env = ctx.alloc_node(cilly::CILNode::LdArg(1));
        let ctor_fnptr = ctx.alloc_node(cilly::CILNode::LdArg(2));
        let set_env = ctx.alloc_root(cilly::CILRoot::SetField(Box::new((
            env_field, ctor_this, ctor_env,
        ))));
        let ctor_this2 = ctx.alloc_node(cilly::CILNode::LdArg(0));
        let set_fnptr = ctx.alloc_root(cilly::CILRoot::SetField(Box::new((
            fnptr_field,
            ctor_this2,
            ctor_fnptr,
        ))));
        let ctor_ret = ctx.alloc_root(cilly::CILRoot::VoidRet);
        let ctor_sig = ctx.sig(
            [Type::ClassRef(shim_cref), env_ty, tramp_fn_ptr_ty],
            Type::Void,
        );
        ctx.new_method(MethodDef::new(
            Access::Public,
            shim_def,
            ctor_name,
            ctor_sig,
            MethodKind::Constructor,
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![set_env, set_fnptr, ctor_ret], 0, None)],
                locals: vec![],
            },
            vec![None, Some(env_field_name), Some(fnptr_field_name)],
        ));

        let invoke_this = ctx.alloc_node(cilly::CILNode::LdArg(0));
        let env_val = ctx.ld_field(invoke_this, env_field);
        let mut invoke_call_args = Vec::with_capacity(inputs.len() + 1);
        invoke_call_args.push(env_val);
        for (i, _in_ty) in inputs.iter().enumerate() {
            let a = ctx.alloc_node(cilly::CILNode::LdArg(
                u32::try_from(i + 1).expect("closure shim: too many arguments"),
            ));
            invoke_call_args.push(a);
        }
        let invoke_this2 = ctx.alloc_node(cilly::CILNode::LdArg(0));
        let fnptr_val = ctx.ld_field(invoke_this2, fnptr_field);
        emit_delegate_invoke(
            ctx,
            shim_def,
            shim_cref,
            &inputs,
            output,
            tramp_fn_sig,
            fnptr_val,
            invoke_call_args,
        );
    }

    // --- Emit: newobj shim(env, trampoline) ; ldftn shim::Invoke ; newobj Delegate::.ctor(obj, ftn) ---
    let env_arg = handle_operand(&args[0].node, ctx);
    let tramp_arg = handle_operand(&args[1].node, ctx);
    let tramp_arg = ctx.alloc_node(cilly::CILNode::PtrCast(
        tramp_arg,
        Box::new(PtrCastRes::FnPtr(tramp_fn_sig)),
    ));

    finish_delegate(
        ctx,
        asm,
        class_name,
        is_valuetype,
        class_generics,
        shim_cref,
        vec![env_ty, tramp_fn_ptr_ty],
        vec![env_arg, tramp_arg],
        &inputs,
        output,
        destination,
    )
}
/// Creates a new managed object, and places a reference to it in destination
fn call_ctor<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    function_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let argc = argc_from_fn_name(function_name, CTOR_FN_NAME);
    // Current SDK intrinsics carry an explicit return type after the three identity parameters so
    // value-type constructors can return their real transport type. Keep accepting the historical
    // header-only shape used by direct compiler regression fixtures.
    let input_start = match subst_ref.len() {
        len if len == argc as usize + 4 => 4,
        len if len == argc as usize + 3 => 3,
        len => panic!(
            "managed ctor generic arity mismatch: got {len}, expected {} (legacy) or {}",
            argc as usize + 3,
            argc as usize + 4
        ),
    };
    // Check that a proper number of arguments is used
    assert!(args.len() == argc as usize);
    // Decode the `<assembly, class path, is-valuetype>` header (subst[0..3]):
    // - the assembly the constructed object resides in,
    // - the name of the constructed object,
    // - whether the constructed object is a valuetype. TODO: this may be unnecesary. Are valuetpes constructed using newobj?
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);
    let tpe = ClassRef::new(class_name, asm, is_valuetype, [].into());
    let tpe = ctx.alloc_class_ref(tpe);
    let inputs: Vec<_> = subst_ref[input_start..]
        .iter()
        .map(|ty| {
            ctx.type_from_cache(
                ctx.monomorphize(*ty)
                    .as_type()
                    .expect("Expceted generic type but got something that was not a type!"),
            )
        })
        .collect();
    emit_constructor_call(tpe, inputs, args, destination, ctx)
}

/// Lower the small set of LLVM intrinsics that can appear in the managed sysroot.
///
/// These symbols are registered as typed managed fallbacks by the linker (see
/// `cilly::builtins::x86`).  Keeping the lowering here, before normal ABI construction, is
/// important: LLVM intrinsics intentionally have no Rust `FnAbi` for rustc to expose.
fn handle_llvm_intrinsic<'tcx>(
    function_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Vec<Root> {
    match function_name {
        "llvm.x86.xgetbv" => {
            assert_eq!(args.len(), 1, "llvm.x86.xgetbv expects one argument");
            let xcr_no = handle_operand(&args[0].node, ctx);
            let value = ctx.call_static(
                "llvm.x86.xgetbv",
                [Type::Int(Int::U32)],
                Type::Int(Int::I64),
                &[xcr_no],
            );
            vec![place_set(destination, value, ctx)]
        }
        "llvm.x86.avx.vzeroupper" | "llvm.x86.sse2.pause" => {
            assert!(
                args.is_empty(),
                "{function_name} expects no arguments, got {}",
                args.len()
            );
            vec![ctx.call_static_root(function_name, [], Type::Void, &[])]
        }
        other => panic!("unsupported LLVM intrinsic in CLR backend: {other}"),
    }
}

/// Dispatches a resolved MIR call: vtable calls for `InstanceKind::Virtual`, no-ops for drop
/// glue on types with nothing to drop, then plain function calls — except when `instance` is one of
/// the magic interop fns [`classify_magic_fn`] recognizes, each of which is a distinct hand-written
/// call shape for a mycorrhiza/interop intrinsic rather than a real MIR function. Classification is by
/// exact `DefId`, not by matching the mangled call-site name, so (unlike the old substring-based
/// dispatch) branch order here no longer matters.
pub fn call_inner<'tcx>(
    _fn_type: Ty<'tcx>,
    instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    source_info: rustc_middle::mir::SourceInfo,
) -> Vec<Root> {
    if super::drop_glue_is_noop(instance, ctx.tcx()) {
        return vec![ctx.alloc_root(CILRoot::Nop)];
    }
    if let rustc_middle::ty::InstanceKind::Virtual(_def, fn_idx) = instance.def {
        assert!(!args.is_empty());

        let mut fat_ptr_address = operand_address(&args[0].node, ctx);
        let fat_ptr_dyn = ctx.alloc_string("FatPtrn3Dyn");
        let fat_ptr_dyn_cref =
            ctx.alloc_class_ref(ClassRef::new(fat_ptr_dyn, None, true, [].into()));
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

        let vtable_index =
            ctx.alloc_node(i32::try_from(fn_idx).expect("More tahn 2^31 functions in a vtable!"));
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
        let abi = AbiPlan::from_instance(instance, ctx);

        let mut signature = abi.signature().clone();
        signature.inputs_mut()[0] = ctx.nptr(Type::Void);
        let mut call_args = abi.lower_call_args(args, source_info, ctx);
        call_args[0] = obj_ptr;
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

    // LLVM intrinsics do not have a Rust ABI.  Asking rustc for their `FnAbi` is an ICE on
    // current nightlies (`fn_abi_of_instance should not be called on LLVM intrinsics`), and they
    // are intended to be expanded by the backend at the call site instead.  Keep this dispatch
    // before `AbiPlan::from_instance`; the ordinary `#[rustc_intrinsic]` path is similarly a
    // backend-owned lowering and must not be treated as a normal function call first.
    let function_name = match instance.def {
        InstanceKind::LlvmIntrinsic(def_id) => ctx
            .tcx()
            .codegen_fn_attrs(def_id)
            .symbol_name
            .expect("LLVM intrinsic is missing its symbol name")
            .to_string(),
        _ => fn_name_for_instance(ctx.tcx(), instance),
    };
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
    if matches!(instance.def, InstanceKind::LlvmIntrinsic(_)) {
        return handle_llvm_intrinsic(&function_name, args, destination, ctx);
    }

    let abi = AbiPlan::from_instance(instance, ctx);
    let mut signature = abi.signature().clone();
    // Checks if function is "magic" — classified by exact `DefId`, not by matching the mangled
    // `function_name`; see `classify_magic_fn`'s doc comment for why that's the safer mechanism.
    // `function_name` is still threaded into several arms below (`call_ctor`, `callvirt_managed`,
    // `call_managed`) because *those* still parse the concrete arity digit back out of it via
    // `argc_from_fn_name` — that's a self-contained detail of decoding a compiler-mangled name, not a
    // magic-fn-identification hazard.
    if let Some(magic) = classify_magic_fn(ctx.tcx(), instance.def_id()) {
        match magic {
            MagicFn::GenericCtor => {
                assert!(
                    !abi.is_rust_call(),
                    "Generic constructors may not use the `rust_call` calling convention!"
                );
                // WF-9: `new List<i32>()` and friends.
                return vec![ctor_generic(instance.args, args, destination, ctx)];
            }
            MagicFn::GenericMethodCall => {
                assert!(
                    !abi.is_rust_call(),
                    "Generic method calls may not use the `rust_call` calling convention!"
                );
                // WF-9: `Activator.CreateInstance<T>()`, `Deserialize<T>(…)`, `GetService<T>()` and friends.
                return vec![call_gmethod(instance.args, args, destination, ctx)];
            }
            MagicFn::GenericCall => {
                assert!(
                    !abi.is_rust_call(),
                    "Generic managed calls may not use the `rust_call` calling convention!"
                );
                // WF-9: `List<i32>::Add(…)` and friends.
                return vec![call_generic(instance.args, args, destination, ctx)];
            }
            MagicFn::DelegateClosure => {
                assert!(
                    !abi.is_rust_call(),
                    "Closure delegate construction may not use the `rust_call` calling convention!"
                );
                return vec![delegate_from_closure(instance.args, args, destination, ctx)];
            }
            MagicFn::Delegate => {
                assert!(
                    !abi.is_rust_call(),
                    "Delegate construction may not use the `rust_call` calling convention!"
                );
                // Delegates & callbacks: wrap a Rust `extern` fn pointer into a managed `Action`/`Func`.
                return vec![delegate_from_fnptr(instance.args, args, destination, ctx)];
            }
            MagicFn::Throw => {
                // `rustc_clr_interop_throw::<MSG>()` raises a managed `System.Exception(MSG)` directly (via
                // the `throw` IL op), so a .NET caller can `catch` it. Unlike a Rust `panic!` — which goes
                // through the unwinder and faults when it reaches a managed frame — this is an ordinary
                // managed throw. The fn returns `!`, so there is no destination; `throw` is a terminal op
                // (the caller appends the usual "diverging call returned" guard after it, as for `panic!`).
                let msg = garg_to_string(instance.args[0], ctx.tcx());
                return vec![ctx.throw_msg(&msg)];
            }
            MagicFn::EnumReprTransmute => {
                assert_eq!(
                    args.len(),
                    1,
                    "enum representation conversion must have exactly one value argument"
                );
                let source_ty = ctx.monomorphize(args[0].node.ty(ctx.body(), ctx.tcx()));
                let destination_ty = ctx.monomorphize(destination.ty(ctx.body(), ctx.tcx()).ty);
                let source_is_int = matches!(
                    source_ty.kind(),
                    rustc_middle::ty::TyKind::Int(_) | rustc_middle::ty::TyKind::Uint(_)
                );
                let destination_is_int = matches!(
                    destination_ty.kind(),
                    rustc_middle::ty::TyKind::Int(_) | rustc_middle::ty::TyKind::Uint(_)
                );
                let source_is_managed =
                    crate::managed_storage::is_raw_managed_struct(source_ty, ctx);
                let destination_is_managed =
                    crate::managed_storage::is_raw_managed_struct(destination_ty, ctx);
                assert!(
                    (source_is_int && destination_is_managed)
                        || (source_is_managed && destination_is_int),
                    "enum representation conversion requires exactly one integer and one raw managed-struct marker; got {source_ty:?} -> {destination_ty:?}"
                );
                assert_eq!(
                    ctx.layout_of(source_ty).size.bytes(),
                    ctx.layout_of(destination_ty).size.bytes(),
                    "enum representation conversion must preserve byte width"
                );
                let source_type = ctx.type_from_cache(source_ty);
                let destination_type = ctx.type_from_cache(destination_ty);
                let value = handle_operand(&args[0].node, ctx);
                let converted = ctx.transmute_on_stack(source_type, destination_type, value);
                return vec![place_set(destination, converted, ctx)];
            }
            MagicFn::Ctor => {
                assert!(
                    !abi.is_rust_call(),
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
            }
            MagicFn::ManagedCallVirt => {
                assert!(
                    !abi.is_rust_call(),
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
            }
            MagicFn::ManagedCall => {
                assert!(
                    !abi.is_rust_call(),
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
            }
            MagicFn::ManagedGetField => {
                assert!(
                    !abi.is_rust_call(),
                    "Managed field reads may not use the `rust_call` calling convention!"
                );
                return vec![managed_get_field(instance.args, args, destination, ctx)];
            }
            MagicFn::LdLen => {
                assert!(
                    !abi.is_rust_call(),
                    "Managed calls may not use the `rust_call` calling convention!"
                );
                // Not-Virtual (for interop)
                let arr = handle_operand(&args[0].node, ctx);
                let len = ctx.ld_len(arr);
                return vec![place_set(destination, len, ctx)];
            }
            MagicFn::LdNull => {
                assert!(
                    !abi.is_rust_call(),
                    "Managed calls may not use the `rust_call` calling convention!"
                );
                // Not-Virtual (for interop)
                let tpe = ctx
                    .type_from_cache(instance.args[0].as_type().unwrap())
                    .as_class_ref()
                    .unwrap();

                let node = ctx.alloc_node(Const::Null(tpe));
                return vec![place_set(destination, node, ctx)];
            }
            MagicFn::IsNull => {
                assert!(
                    !abi.is_rust_call(),
                    "Managed calls may not use the `rust_call` calling convention!"
                );
                let tpe = ctx
                    .type_from_cache(instance.args[0].as_type().unwrap())
                    .as_class_ref()
                    .expect("managed null checks require a reference type");
                assert!(
                    !ctx[tpe].is_valuetype(),
                    "managed null checks require a reference type"
                );
                let value = handle_operand(&args[0].node, ctx);
                let null = ctx.alloc_node(Const::Null(tpe));
                let is_null = ctx.alloc_node(CILNode::BinOp(value, null, BinOp::Eq));
                return vec![place_set(destination, is_null, ctx)];
            }
            MagicFn::CheckedCast => {
                let tpe = ctx
                    .type_from_cache(instance.args[0].as_type().unwrap())
                    .as_class_ref()
                    .unwrap();
                let input = handle_operand(&args[0].node, ctx);
                // Not-Virtual (for interop)
                let node = ctx.checked_cast(input, tpe);
                return vec![place_set(destination, node, ctx)];
            }
            MagicFn::IsInst => {
                let tpe = ctx
                    .type_from_cache(instance.args[0].as_type().unwrap())
                    .as_class_ref()
                    .unwrap();
                let input = handle_operand(&args[0].node, ctx);
                // Not-Virtual (for interop)
                let node = ctx.is_inst(input, tpe);
                return vec![place_set(destination, node, ctx)];
            }
            MagicFn::Box => {
                // Boxes the value of type `T` (the intrinsic's type generic) into `System.Object` (`box T`).
                // The typechecker enforces that `T` is a value type.
                let tpe = ctx.type_from_cache(instance.args[0].as_type().unwrap());
                let tpe = ctx.alloc_type(tpe);
                let value = handle_operand(&args[0].node, ctx);
                let node = ctx.box_value(value, tpe);
                return vec![place_set(destination, node, ctx)];
            }
            MagicFn::ManagedBoxNew => {
                let tpe = ctx.type_from_cache(instance.args[0].as_type().unwrap());
                let value = handle_operand(&args[0].node, ctx);
                let object = if is_direct_managed_reference(tpe, ctx) {
                    value
                } else {
                    let tpe = ctx.alloc_type(tpe);
                    ctx.box_value(value, tpe)
                };
                let handle = ctx[object].clone().ref_to_handle(ctx);
                let handle = ctx.alloc_node(handle);
                let void = ctx.alloc_type(Type::Void);
                let handle =
                    ctx.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
                return vec![place_set(destination, handle, ctx)];
            }
            MagicFn::ManagedBoxGet | MagicFn::ManagedBoxTake => {
                let take = matches!(magic, MagicFn::ManagedBoxTake);
                let tpe = ctx.type_from_cache(instance.args[0].as_type().unwrap());
                let handle = handle_operand(&args[0].node, ctx);
                let handle = ctx.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::ISize)));
                let main_module = *ctx.main_module();
                let handle_to_obj_name = ctx.alloc_string("handle_to_obj");
                let handle_to_obj = ctx.class_ref(main_module).clone().static_mref(
                    &[Type::Int(Int::ISize)],
                    Type::PlatformObject,
                    handle_to_obj_name,
                    ctx,
                );
                let object = ctx.alloc_node(CILNode::call(handle_to_obj, [handle]));
                let value = if is_direct_managed_reference(tpe, ctx) {
                    let tpe = ctx.alloc_type(tpe);
                    ctx.alloc_node(CILNode::CheckedCast(object, tpe))
                } else {
                    let tpe = ctx.alloc_type(tpe);
                    ctx.unbox_any(object, tpe)
                };
                let store = place_set(destination, value, ctx);

                if !take {
                    return vec![store];
                }

                let handle_free_name = ctx.alloc_string("handle_free");
                let handle_free = ctx.class_ref(main_module).clone().static_mref(
                    &[Type::Int(Int::ISize)],
                    Type::Void,
                    handle_free_name,
                    ctx,
                );
                let free = ctx.alloc_root(CILRoot::call(handle_free, [handle]));
                return vec![store, free];
            }
            MagicFn::ManagedBoxFree => {
                let handle = handle_operand(&args[0].node, ctx);
                let handle = ctx.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::ISize)));
                let main_module = *ctx.main_module();
                let handle_free_name = ctx.alloc_string("handle_free");
                let handle_free = ctx.class_ref(main_module).clone().static_mref(
                    &[Type::Int(Int::ISize)],
                    Type::Void,
                    handle_free_name,
                    ctx,
                );
                return vec![ctx.alloc_root(CILRoot::call(handle_free, [handle]))];
            }
            MagicFn::ManagedDefault => {
                let tpe = ctx.type_from_cache(instance.args[0].as_type().unwrap());
                let tpe = ctx.alloc_type(tpe);
                let destination = place_address(destination, ctx);
                return vec![ctx.init_obj(destination, tpe)];
            }
            MagicFn::LdElemRef => {
                assert!(
                    !abi.is_rust_call(),
                    "Managed calls may not use the `rust_call` calling convention!"
                );
                // Not-Virtual (for interop)
                let arr = handle_operand(&args[0].node, ctx);
                let idx = handle_operand(&args[1].node, ctx);
                let node = ctx.ld_elem_ref(arr, idx);
                return vec![place_set(destination, node, ctx)];
            }
            MagicFn::LdElem => {
                assert!(
                    !abi.is_rust_call(),
                    "Managed calls may not use the `rust_call` calling convention!"
                );
                let elem = ctx.type_from_cache(instance.args[0].as_type().unwrap());
                let elem = ctx.alloc_type(elem);
                let arr = handle_operand(&args[0].node, ctx);
                let idx = handle_operand(&args[1].node, ctx);
                let node = ctx.ld_elem(arr, idx, elem);
                return vec![place_set(destination, node, ctx)];
            }
            MagicFn::NewArr => {
                assert!(
                    !abi.is_rust_call(),
                    "Managed calls may not use the `rust_call` calling convention!"
                );
                // Allocates a managed 1-D array of the (primitive) element type `T` with `len` elements.
                // The element type is the first generic argument of the intrinsic.
                let elem = ctx.type_from_cache(instance.args[0].as_type().unwrap());
                let elem = ctx.alloc_type(elem);
                let len = handle_operand(&args[0].node, ctx);
                let node = ctx.new_arr(elem, len);
                return vec![place_set(destination, node, ctx)];
            }
            MagicFn::SetElem => {
                assert!(
                    !abi.is_rust_call(),
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
            }
            MagicFn::TryCatch => {
                assert!(
                    !abi.is_rust_call(),
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
        }
    }
    let call_args = abi.lower_call_args(args, source_info, ctx);
    if abi.is_c_variadic() {
        let mut inputs: Vec<_> = args
            .iter()
            .map(|operand| {
                ctx.type_from_cache(ctx.monomorphize(operand.node.ty(ctx.body(), ctx.tcx())))
            })
            .collect();
        if let Some(slot) = abi.caller_location_slot() {
            inputs.push(signature.inputs()[slot]);
        }
        signature.set_inputs(inputs);
    }
    let is_void = matches!(signature.output(), cilly::Type::Void);
    //rustc_middle::ty::print::with_no_trimmed_paths! {call.push(CILOp::Comment(format!("Calling {instance:?}").into()))};
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
/// Resolves `fn_type` to an `Instance` and hands off to `call_inner` for the actual dispatch
/// (virtual/interop/plain-call branching). Entry point for MIR `Call` terminators; intrinsics
/// are routed separately, before reaching here, via `handle_intrinsic`.
pub fn call<'tcx>(
    fn_type: Ty<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    source_info: rustc_middle::mir::SourceInfo,
) -> Vec<Root> {
    let fn_type = ctx.monomorphize(fn_type);
    let instance = if let TyKind::FnDef(def_id, subst_ref) = fn_type.kind() {
        let subst = subst_ref
            .no_bound_vars()
            .expect("function definition had bound generic arguments");
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
