use crate::{
    assembly::MethodCompileCtx,
    interop::AssemblyRef,
    utilis::{
        classify_magic_fn, garg_to_bool, MagicFn, CTOR_FN_NAME, MANAGED_CALL_FN_NAME,
        MANAGED_CALL_VIRT_FN_NAME,
    },
};
use cilly::{
    cilnode::{ExtendKind, IsPure, MethodKind},
    BinOp, ClassRef, Const, FieldDesc, FnSig, IString, Int, Interned, IntoAsmIndex,
};
use cilly::tpe::GenericKind;
use cilly::{MethodRef, Type};
use crate::call_info::CallInfo;
use crate::fn_ctx::fn_name;
use crate::operand::{handle_operand, operand_address};
use crate::place::place_set;
use crate::r#type::{
    utilis::{garg_to_usize, garg_to_string},
    GetTypeExt,
};
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
/// Calls a non-virtual managed function(used for interop)
fn call_managed<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    function_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    fn_instance: Instance<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let argc = argc_from_fn_name(function_name, MANAGED_CALL_FN_NAME);
    //FIXME: figure out the proper argc.
    //assert!(subst_ref.len() as u32 == argc + 3 || subst_ref.len() as u32 == argc + 4);
    assert!(args.len() == argc as usize);
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);
    let managed_fn_name = garg_to_string(subst_ref[3], ctx.tcx());
    let tpe = ClassRef::new(class_name, asm, is_valuetype, [].into());

    //eprintln!("tpe:{tpe:?}");
    let signature = crate::function_sig::sig_from_instance_(fn_instance, ctx)
        .expect("Can't get the function signature");

    if argc == 0 {
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
        let is_static = garg_to_bool(subst_ref[4], ctx.tcx());

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
    let argc = argc_from_fn_name(function_name, MANAGED_CALL_VIRT_FN_NAME);
    //assert!(subst_ref.len() as u32 == argc + 3 || subst_ref.len() as u32 == argc + 4);
    assert!(u32::try_from(args.len()).expect("More than 2^32 function arguments.") == argc);
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);

    let managed_fn_garg = &subst_ref[3];
    let managed_fn_garg = ctx.monomorphize(*managed_fn_garg);
    let managed_fn_name = garg_to_string(managed_fn_garg, ctx.tcx());

    let tpe = ClassRef::new(class_name, asm, is_valuetype, [].into());
    let signature = crate::function_sig::sig_from_instance_(fn_instance, ctx)
        .expect("Can't get the function signature");
    if argc == 0 {
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
        let is_static = garg_to_bool(subst_ref[4], ctx.tcx());

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
            check_generic_marker(inner_sig, inner_rt, class_generics, method_generics, role, ctx);
        }
        _ => {}
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
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);
    let managed_fn_name = garg_to_string(subst_ref[3], ctx.tcx());
    let managed_fn_name = ctx.alloc_string(managed_fn_name);
    let kind = garg_to_usize(subst_ref[4], ctx.tcx());
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
    check_generic_marker(output, ret_ty, &class_generics, &[], "return", ctx);
    let recv_offset = if kind == 0 { 0 } else { 1 };
    for (j, &sig_in) in explicit_inputs.iter().enumerate() {
        let arg_ty = garg_ty_to_type(subst_ref[8 + recv_offset + j], ctx);
        check_generic_marker(sig_in, arg_ty, &class_generics, &[], "argument", ctx);
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
            // Instance method reached with `call instance` (a non-virtual slot). A **value-type**
            // receiver's `this` is a *managed pointer* to the unboxed valuetype (`valuetype Foo&`),
            // exactly as the non-generic `RustcCLRInteropManagedStruct::vt_instance*` path in
            // `call_managed` passes `&self` — so the wrapper hands us the address and we type the
            // receiver slot as a ref. A reference-type receiver is the object reference itself.
            if is_valuetype {
                let this_ref = ctx.nref(Type::ClassRef(this));
                inputs.push(this_ref);
            } else {
                inputs.push(Type::ClassRef(this));
            }
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
    let InteropHeader {
        asm,
        class_name,
        is_vt: is_valuetype,
    } = InteropHeader::decode(subst_ref, ctx);
    let managed_fn_name = garg_to_string(subst_ref[3], ctx.tcx());
    let managed_fn_name = ctx.alloc_string(managed_fn_name);
    let kind = garg_to_usize(subst_ref[4], ctx.tcx());
    let class_generics = tuple_garg_to_types(subst_ref[5], ctx);
    // The method's own concrete type arguments (e.g. the `(int32,)` of `CreateInstance<int32>`).
    let method_generics = tuple_garg_to_types(subst_ref[6], ctx);
    assert!(
        !method_generics.is_empty(),
        "WF-9 generic method: a generic method call must carry at least one method type argument"
    );
    let mut sig_types = tuple_garg_to_types(subst_ref[7], ctx);
    assert!(
        !sig_types.is_empty(),
        "WF-9 generic method: the signature tuple must carry at least a return type"
    );
    let output = sig_types.remove(0);
    let explicit_inputs = sig_types;

    // Loud-fail on an inconsistent binding — `!N` against class generics, `!!N` against method generics.
    let ret_ty = garg_ty_to_type(subst_ref[8], ctx);
    check_generic_marker(output, ret_ty, &class_generics, &method_generics, "return", ctx);
    let recv_offset = if kind == 0 { 0 } else { 1 };
    for (j, &sig_in) in explicit_inputs.iter().enumerate() {
        let arg_ty = garg_ty_to_type(subst_ref[9 + recv_offset + j], ctx);
        check_generic_marker(sig_in, arg_ty, &class_generics, &method_generics, "argument", ctx);
    }

    let this = ctx.alloc_class_ref(ClassRef::new(
        class_name,
        asm,
        is_valuetype,
        class_generics.into(),
    ));
    let mut inputs = Vec::with_capacity(explicit_inputs.len() + 1);
    let mkind = match kind {
        0 => MethodKind::Static,
        1 => {
            if is_valuetype {
                let this_ref = ctx.nref(Type::ClassRef(this));
                inputs.push(this_ref);
            } else {
                inputs.push(Type::ClassRef(this));
            }
            MethodKind::Instance
        }
        2 => {
            inputs.push(Type::ClassRef(this));
            MethodKind::Virtual
        }
        _ => panic!("WF-9 generic method: invalid call KIND {kind}"),
    };
    inputs.extend(explicit_inputs);
    let sig = ctx.sig(inputs, output);
    // The KEY difference from `call_generic`: the methodref carries the method's concrete type args, so
    // the exporter emits `Method<..>` and the CLR binds the right instantiation.
    let mref = MethodRef::new(this, managed_fn_name, sig, mkind, method_generics.into());
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
    use cilly::cilnode::PtrCastRes;
    use cilly::{Access, BasicBlock, ClassDef, MethodDef, MethodImpl};
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
        let ctor_sig = ctx.sig(
            [Type::ClassRef(shim_cref), shim_fn_ptr_ty],
            Type::Void,
        );
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

        // ---- shim `Invoke(this, In0, …) -> Ret` : loads args, loads the field, `calli`s it ----
        let invoke_name = ctx.alloc_string("Invoke");
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
        let mut invoke_sig_inputs = vec![Type::ClassRef(shim_cref)];
        invoke_sig_inputs.extend(inputs.iter().copied());
        let invoke_sig = ctx.sig(invoke_sig_inputs, output);
        let invoke_body = if output == Type::Void {
            let call = ctx.call_indirect_root(shim_fn_sig, fnptr_val, invoke_call_args);
            let ret = ctx.alloc_root(cilly::CILRoot::VoidRet);
            vec![call, ret]
        } else {
            let call = ctx.call_indirect(shim_fn_sig, fnptr_val, invoke_call_args);
            let ret = ctx.alloc_root(cilly::CILRoot::Ret(call));
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

    let shim_ctor_sig = ctx.sig([Type::ClassRef(shim_cref), shim_fn_ptr_ty], Type::Void);
    let shim_ctor = MethodRef::new(
        shim_cref,
        ctx.alloc_string(".ctor"),
        shim_ctor_sig,
        MethodKind::Constructor,
        vec![].into(),
    );
    let shim_ctor = ctx.alloc_methodref(shim_ctor);
    let shim_obj = ctx.call(shim_ctor, &[fnptr_arg], IsPure::NOT);

    // ldftn shim::Invoke  (an instance method: methodref receiver is sig position 0)
    let mut invoke_sig_inputs = vec![Type::ClassRef(shim_cref)];
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
    // `ldftn` yields `native int`; the delegate `.ctor`'s second param is `native int`. Normalise.
    let invoke_ftn = ctx.alloc_node(cilly::CILNode::PtrCast(invoke_ftn, Box::new(PtrCastRes::ISize)));

    // newobj DelegateClass<ClassGenerics..>::.ctor(object, native int)
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
    use cilly::{Access, BasicBlock, ClassDef, MethodDef, MethodImpl};
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

        // ---- shim `Invoke(this, In0, …) -> Ret` : ldfld env ; ldargs ; ldfld fnptr ; calli(env, In..) ----
        let invoke_name = ctx.alloc_string("Invoke");
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
        let mut invoke_sig_inputs = vec![Type::ClassRef(shim_cref)];
        invoke_sig_inputs.extend(inputs.iter().copied());
        let invoke_sig = ctx.sig(invoke_sig_inputs, output);
        let invoke_body = if output == Type::Void {
            let call = ctx.call_indirect_root(tramp_fn_sig, fnptr_val, invoke_call_args);
            let ret = ctx.alloc_root(cilly::CILRoot::VoidRet);
            vec![call, ret]
        } else {
            let call = ctx.call_indirect(tramp_fn_sig, fnptr_val, invoke_call_args);
            let ret = ctx.alloc_root(cilly::CILRoot::Ret(call));
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

    // --- Emit: newobj shim(env, trampoline) ; ldftn shim::Invoke ; newobj Delegate::.ctor(obj, ftn) ---
    let env_arg = handle_operand(&args[0].node, ctx);
    let tramp_arg = handle_operand(&args[1].node, ctx);
    let tramp_arg = ctx.alloc_node(cilly::CILNode::PtrCast(
        tramp_arg,
        Box::new(PtrCastRes::FnPtr(tramp_fn_sig)),
    ));

    let shim_ctor_sig = ctx.sig(
        [Type::ClassRef(shim_cref), env_ty, tramp_fn_ptr_ty],
        Type::Void,
    );
    let shim_ctor = MethodRef::new(
        shim_cref,
        ctx.alloc_string(".ctor"),
        shim_ctor_sig,
        MethodKind::Constructor,
        vec![].into(),
    );
    let shim_ctor = ctx.alloc_methodref(shim_ctor);
    let shim_obj = ctx.call(shim_ctor, &[env_arg, tramp_arg], IsPure::NOT);

    let mut invoke_sig_inputs = vec![Type::ClassRef(shim_cref)];
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
    let invoke_ftn = ctx.alloc_node(cilly::CILNode::PtrCast(invoke_ftn, Box::new(PtrCastRes::ISize)));

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
/// Creates a new managed object, and places a reference to it in destination
fn call_ctor<'tcx>(
    subst_ref: &[GenericArg<'tcx>],
    function_name: &str,
    args: &[Spanned<Operand<'tcx>>],
    destination: &Place<'tcx>,
    ctx: &mut MethodCompileCtx<'tcx, '_>,
) -> Root {
    let argc = argc_from_fn_name(function_name, CTOR_FN_NAME);
    // Check that there are enough function path and argument specifers
    assert!(subst_ref.len() == argc as usize + 3);
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
    // If no arguments, inputs don't have to be handled, so a simpler call handling is used.
    if argc == 0 {
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
/// Dispatches a resolved MIR call: vtable calls for `InstanceKind::Virtual`, no-ops for drop
/// glue on types with nothing to drop, then plain function calls — except when `instance` is one of
/// the magic interop fns [`classify_magic_fn`] recognizes, each of which is a distinct hand-written
/// call shape for a mycorrhiza/interop intrinsic rather than a real MIR function. Classification is by
/// exact `DefId`, not by matching the mangled call-site name, so (unlike the old substring-based
/// dispatch) branch order here no longer matters.
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

    let function_name = fn_name(ctx.tcx().symbol_name(instance));
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
                    !call_info.split_last_tuple(),
                    "Generic constructors may not use the `rust_call` calling convention!"
                );
                // WF-9: `new List<i32>()` and friends.
                return vec![ctor_generic(instance.args, args, destination, ctx)];
            }
            MagicFn::GenericMethodCall => {
                assert!(
                    !call_info.split_last_tuple(),
                    "Generic method calls may not use the `rust_call` calling convention!"
                );
                // WF-9: `Activator.CreateInstance<T>()`, `Deserialize<T>(…)`, `GetService<T>()` and friends.
                return vec![call_gmethod(instance.args, args, destination, ctx)];
            }
            MagicFn::GenericCall => {
                assert!(
                    !call_info.split_last_tuple(),
                    "Generic managed calls may not use the `rust_call` calling convention!"
                );
                // WF-9: `List<i32>::Add(…)` and friends.
                return vec![call_generic(instance.args, args, destination, ctx)];
            }
            MagicFn::DelegateClosure => {
                assert!(
                    !call_info.split_last_tuple(),
                    "Closure delegate construction may not use the `rust_call` calling convention!"
                );
                return vec![delegate_from_closure(instance.args, args, destination, ctx)];
            }
            MagicFn::Delegate => {
                assert!(
                    !call_info.split_last_tuple(),
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
            MagicFn::Ctor => {
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
            }
            MagicFn::ManagedCallVirt => {
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
            }
            MagicFn::ManagedCall => {
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
            }
            MagicFn::LdLen => {
                assert!(
                    !call_info.split_last_tuple(),
                    "Managed calls may not use the `rust_call` calling convention!"
                );
                // Not-Virtual (for interop)
                let arr = handle_operand(&args[0].node, ctx);
                let len = ctx.ld_len(arr);
                return vec![place_set(destination, len, ctx)];
            }
            MagicFn::LdNull => {
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
            MagicFn::LdElemRef => {
                assert!(
                    !call_info.split_last_tuple(),
                    "Managed calls may not use the `rust_call` calling convention!"
                );
                // Not-Virtual (for interop)
                let arr = handle_operand(&args[0].node, ctx);
                let idx = handle_operand(&args[1].node, ctx);
                let node = ctx.ld_elem_ref(arr, idx);
                return vec![place_set(destination, node, ctx)];
            }
            MagicFn::NewArr => {
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
            }
            MagicFn::SetElem => {
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
            }
            MagicFn::TryCatch => {
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
        }
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
