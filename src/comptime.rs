//! Comptime interpreter — the `.NET → Rust` *type-export* path (WF-7 P3).
//!
//! `dotnet_typedef!` (see `test/types/interop_typedef.rs`) expands to a function
//! `rustc_codegen_clr_comptime_entrypoint` that, at the MIR level, calls four "magic" intrinsics in
//! sequence to describe a .NET class:
//!   * `rustc_codegen_clr_new_typedef::<NAME, IS_VALUETYPE, INHERITS_ASM, INHERITS>() -> ClassDef`
//!   * `rustc_codegen_clr_add_field_def::<FieldTy, FNAME>(class) -> ClassDef`
//!   * `rustc_codegen_clr_add_method_def::<VIS, MODIFIERS, FNAME, FnTy>(class, fnptr) -> ClassDef`
//!   * `rustc_codegen_clr_finish_type(class)`
//! The intrinsic bodies `abort()` — they are never executed; instead this interpreter *reads their MIR*
//! (the const-generic args carry the metadata) and, as a side effect, registers a real `ClassDef` into
//! the assembly. So a Rust source declaration becomes a managed .NET class whose virtual methods alias
//! ordinary (separately codegen'd) Rust functions.
//!
//! Methods can only be attached to a class that is already registered, so we accumulate the class shape
//! as plain data while walking the MIR and build + register everything in one shot at `finish_type`.

use cilly::cilnode::MethodKind;
use cilly::{
    Access, BasicBlock, CILNode, CILRoot, ClassDef, ClassRef, FieldDesc, Interned, MethodDef,
    MethodImpl, MethodRef, Type,
};
use cilly::{Float, Int};
use rustc_codegen_clr_call::CallInfo;
use rustc_codegen_clr_ctx::{function_name, MethodCompileCtx};
use rustc_codegen_clr_type::r#type::get_type;
use rustc_codegen_clr_type::utilis::garg_to_string;
use rustc_middle::mir::{Mutability, Rvalue, StatementKind, TerminatorKind};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{Instance, TyKind, TypingEnv};

use crate::utilis::garg_to_bool;

/// A `.NET` class being described, accumulated as plain data (no interning) until `finish_type`.
#[derive(Clone)]
struct PendingClass<'tcx> {
    name: String,
    is_value_type: bool,
    /// `(assembly, class_name)` of the superclass, if any.
    superclass: Option<(String, String)>,
    /// `(field_type, field_name)`.
    fields: Vec<(Type, String)>,
    /// `(managed_method_name, target_rust_fn)` — the virtual method aliases the Rust fn.
    methods: Vec<(String, Instance<'tcx>)>,
    /// `(managed_method_name, target_rust_fn)` — a `static` method aliasing the Rust fn (no receiver;
    /// the fn's signature is used verbatim).
    static_methods: Vec<(String, Instance<'tcx>)>,
    /// `(interface_assembly, interface_name, generic_args)` — managed interfaces this class
    /// implements. The virtual methods above satisfy them by name+signature (implicit interface
    /// implementation). `generic_args` is empty for a non-generic interface, or a single
    /// `(assembly, type_name, is_valuetype)` external-type reference for the one-generic-parameter
    /// case (see `rustc_codegen_clr_add_generic_interface_impl`) — never derived from a Rust type,
    /// so `is_valuetype` must come from the caller (there is no Rust type to infer it from).
    interfaces: Vec<(String, String, Vec<(String, String, bool)>)>,
    /// Synthesize a field-initializing primary ctor `.ctor(field0, field1, …)` (in field order) so a
    /// managed caller can `new <Name>(…)` and get an instance with its fields set.
    has_primary_ctor: bool,
    /// Also synthesize a parameterless `.ctor()` (overloading the primary ctor) so a managed caller
    /// can `new <Name>()` and get a default-initialized instance.
    has_default_ctor: bool,
    /// Also synthesize a `set_<field>(value)` mutator per field, paired with the `read_<field>`
    /// accessor.
    has_field_setters: bool,
    /// `managed_method_name -> (base_asm, base_type)` — an explicit ECMA-335 `.override` target
    /// for a virtual method already registered in `methods` above (see
    /// `rustc_codegen_clr_mark_last_method_override`'s doc). The base method's own name is
    /// assumed identical to the overriding method's name (the only shape this narrow spike
    /// supports — see `MethodDef::with_override`'s doc for the intentionally small scope), and
    /// its signature is assumed identical too (definitionally true for a valid override), so no
    /// separate base-signature needs to be carried here.
    method_overrides: std::collections::HashMap<String, (String, String)>,
    /// `managed_method_name -> (event_name, is_add)` — links a virtual method already registered
    /// in `methods` above to a `.NET` event's `add_*`/`remove_*` half (see
    /// `rustc_codegen_clr_mark_last_method_event_add`'s doc). `is_add = true` for the `add_*`
    /// method, `false` for `remove_*`. Both halves of the same `event_name` must be present by the
    /// time `finish_type` runs, or building the `EventDef` panics with a clear message.
    event_bindings: std::collections::HashMap<String, (String, bool)>,
    /// `accessor_method_name -> (property_name, is_getter)` — links an abstract interface member
    /// already registered in `abstract_methods` below to a `.NET` property's getter/setter half
    /// (see `rustc_codegen_clr_mark_last_abstract_property_get`'s doc). The property's value
    /// type is inferred from the accessor's own carrier signature at `finish_type` (getter:
    /// return type; setter: the single non-receiver parameter) — the two must agree or
    /// `finish_type` panics. Only valid on `is_interface` classes.
    property_bindings: std::collections::HashMap<String, (String, bool)>,
    /// This class is a genuine ECMA-335 `interface` `TypeDef` (from `#[dotnet_interface]` on a Rust
    /// trait), not an ordinary class — registered via `ClassDef::with_interface()`. Its members are
    /// all in `abstract_methods` (never `methods`), and it has no base type / no ctors.
    is_interface: bool,
    /// `(managed_method_name, signature_carrier_fn, out_param_sequences, generic_param_names)` —
    /// abstract (no-body) interface members. The carrier is used ONLY to extract the member's
    /// signature (like `methods`' targets), but it is NOT aliased: the emitted `MethodDef` is
    /// `MethodImpl::Missing` + `.with_abstract()` (RVA=0). `out_param_sequences` holds the
    /// 1-based, receiver-stripped positions of `#[dotnet_out]`-marked parameters (see
    /// `rustc_codegen_clr_mark_last_abstract_method_out_params`) — empty for most members.
    /// `generic_param_names` is the declared type-parameter name list of a generic method
    /// DEFINITION (`fn Echo<T>(&self, value: T) -> T` via
    /// `rustc_codegen_clr_add_generic_abstract_method_def` — the METHOD-generic dual of
    /// `type_generics` below; the carrier spells each `T` position as the
    /// `RustcCLRInteropMethodGeneric<N>` / `!!N` marker) — empty for non-generic members.
    abstract_methods: Vec<(String, Instance<'tcx>, Vec<u16>, Vec<String>)>,
    /// `(managed_method_name, target_rust_fn)` — **default interface methods** (DIM, CoreCLR
    /// 3.0+): virtual, NON-abstract interface members with a real body. Unlike
    /// `abstract_methods`' signature-only carriers, the target here is a REAL codegen'd fn (the
    /// lifted default body from `#[dotnet_interface]`) that the member aliases exactly like a
    /// class virtual in `methods` — `MethodKind::Virtual` + `MethodImpl::AliasFor`, no
    /// `.with_abstract()`, so Pass 4 of the PE writer assembles a body and the member's RVA is
    /// non-zero. Only valid on `is_interface` classes.
    default_methods: Vec<(String, Instance<'tcx>)>,
    /// `(managed_method_name, signature_carrier_fn)` — **`static abstract`** interface members
    /// (.NET 7+ static virtual members in interfaces, from a `#[dotnet_interface]` trait fn with
    /// no `self` receiver). Like `abstract_methods` the carrier is signature-only, but it carries
    /// NO receiver: the emitted `MethodDef` is `MethodKind::Static` + `MethodImpl::Missing` +
    /// `.with_abstract()` (RVA=0, sig used verbatim). Only valid on `is_interface` classes.
    static_abstract_methods: Vec<(String, Instance<'tcx>)>,
    /// The declared generic-parameter names (`["T"]`, `["K", "V"]`, …) of a GENERIC type
    /// definition, in declaration order — from `rustc_codegen_clr_set_type_generics` (emitted by
    /// `#[dotnet_interface]` on a `trait IFoo<T>`, whose `name` above already carries the CLS
    /// backtick-arity suffix `IFoo`1`). Empty for every non-generic type. Only valid on
    /// `is_interface` classes (asserted in `finish_type` — generic CLASS definitions stay
    /// walled: the no-explicit-layout-on-.NET-generics ban applies to classes, not interfaces).
    type_generics: Vec<String>,
}

#[derive(Clone)]
enum ComptimeLocalVar<'tcx> {
    NotSet,
    Void,
    Class(PendingClass<'tcx>),
}

impl<'tcx> ComptimeLocalVar<'tcx> {
    fn as_class(&self) -> &PendingClass<'tcx> {
        match self {
            Self::Class(v) => v,
            _ => panic!("comptime: expected a ClassDef local in interop type definition"),
        }
    }
}

pub fn interpret<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    body: &'tcx rustc_middle::mir::Body<'tcx>,
) {
    let mut block_id = rustc_middle::mir::BasicBlock::from_usize(0);
    let mut locals = vec![ComptimeLocalVar::NotSet; body.local_decls.len()];

    loop {
        let block_data = &body.basic_blocks[block_id];
        assert!(
            !block_data.is_cleanup,
            "comptime: can't interpret a cleanup block"
        );

        // Statements: we only need to thread the ClassDef value between locals (the magic calls return
        // it and the next call takes it). Everything else (storage markers, fn-pointer reifications) is
        // irrelevant to building the type.
        for statement in &block_data.statements {
            if let StatementKind::Assign(bx) = &statement.kind {
                let (target, rvalue) = bx.as_ref();
                if let Rvalue::Use(src, _) = rvalue {
                    if let (Some(src_local), Some(tgt_local)) = (
                        src.place().and_then(|p| p.as_local()),
                        target.as_local(),
                    ) {
                        locals[usize::from(tgt_local)] = locals[usize::from(src_local)].clone();
                    }
                }
                // Rvalue::Cast(ReifyFnPointer, ..) and others: ignored — the method's fn is read from
                // the call's generic args, not from a tracked local.
                let _ = PointerCoercion::ReifyFnPointer;
            }
        }

        let Some(term) = &block_data.terminator else {
            return;
        };
        match &term.kind {
            TerminatorKind::Call {
                func,
                args,
                destination,
                target,
                ..
            } => {
                let func_ty = ctx.monomorphize(func.ty(body, ctx.tcx()));
                let TyKind::FnDef(def_id, subst_ref) = func_ty.kind() else {
                    return;
                };
                let subst_ref = ctx.monomorphize(*subst_ref);
                let env = TypingEnv::fully_monomorphized();
                let call_instance = Instance::try_resolve(ctx.tcx(), env, *def_id, subst_ref)
                    .expect("comptime: invalid function def")
                    .expect("comptime: could not resolve callee instance");
                let fname = function_name(ctx.tcx().symbol_name(call_instance));

                let dest_local = destination
                    .as_local()
                    .expect("comptime: unsupported call destination in interop type definition");

                let result = if fname.contains("rustc_codegen_clr_new_typedef") {
                    let name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let is_value_type = garg_to_bool(subst_ref[1], ctx.tcx());
                    let superclass_asm = garg_to_string(subst_ref[2], ctx.tcx()).replace("::", ".");
                    let superclass_name = garg_to_string(subst_ref[3], ctx.tcx()).replace("::", ".");
                    let superclass = if superclass_name.is_empty() {
                        None
                    } else {
                        Some((superclass_asm, superclass_name))
                    };
                    ComptimeLocalVar::Class(PendingClass {
                        name,
                        is_value_type,
                        superclass,
                        fields: vec![],
                        methods: vec![],
                        static_methods: vec![],
                        interfaces: vec![],
                        has_primary_ctor: false,
                        has_default_ctor: false,
                        has_field_setters: false,
                        method_overrides: std::collections::HashMap::new(),
                        event_bindings: std::collections::HashMap::new(),
                        property_bindings: std::collections::HashMap::new(),
                        is_interface: false,
                        abstract_methods: vec![],
                        default_methods: vec![],
                        static_abstract_methods: vec![],
                        type_generics: vec![],
                    })
                } else if fname.contains("rustc_codegen_clr_add_field_def") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    let field_ty = ctx.monomorphize(subst_ref[0].as_type().unwrap());
                    let tpe = get_type(field_ty, ctx);
                    let field_name = garg_to_string(subst_ref[1], ctx.tcx());
                    class.fields.push((tpe, field_name));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_method_def") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    let method_name = garg_to_string(subst_ref[2], ctx.tcx()).replace("::", ".");
                    let fn_ty = ctx.monomorphize(subst_ref[3].as_type().unwrap());
                    let TyKind::FnDef(fdef, fsubst) = fn_ty.kind() else {
                        panic!("comptime: method target is not a function definition");
                    };
                    let fsubst = ctx.monomorphize(*fsubst);
                    let target = Instance::try_resolve(ctx.tcx(), env, *fdef, fsubst)
                        .expect("comptime: invalid method target")
                        .expect("comptime: could not resolve method target instance");
                    class.methods.push((method_name, target));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_abstract_method_def") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const FNAME, FnType> — so FNAME is [0], the signature-carrier fn
                    // type is [1]. The carrier is resolved to an `Instance` only to read its
                    // signature (like `add_method_def`), never aliased/codegen'd.
                    let method_name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let fn_ty = ctx.monomorphize(subst_ref[1].as_type().unwrap());
                    let TyKind::FnDef(fdef, fsubst) = fn_ty.kind() else {
                        panic!("comptime: abstract method signature carrier is not a function definition");
                    };
                    let fsubst = ctx.monomorphize(*fsubst);
                    let carrier = Instance::try_resolve(ctx.tcx(), env, *fdef, fsubst)
                        .expect("comptime: invalid abstract method signature carrier")
                        .expect("comptime: could not resolve abstract method signature carrier instance");
                    class.abstract_methods.push((method_name, carrier, vec![], vec![]));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_generic_abstract_method_def") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const FNAME, const GENERIC_PARAMS, FnType> — the abstract-member
                    // shape of `add_abstract_method_def` plus the declared type-parameter NAME
                    // list of a generic method DEFINITION (`;`-separated, declaration order —
                    // the same `;`-list convention as `set_type_generics`). Substring-dispatch
                    // safety: "add_generic_abstract_method_def" neither contains nor is contained
                    // by "add_method_def", "add_abstract_method_def", "add_static_method_def",
                    // "add_static_abstract_method_def", "add_default_method_def",
                    // "add_generic_interface_impl", or "set_type_generics" (the
                    // `generic_abstract_` infix breaks every containment; audited against the
                    // whole chain), so the chain order cannot misdispatch.
                    let method_name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let names_arg = garg_to_string(subst_ref[1], ctx.tcx());
                    let generic_names: Vec<String> = names_arg
                        .split(';')
                        .map(str::trim)
                        .map(String::from)
                        .collect();
                    assert!(
                        !generic_names.is_empty() && generic_names.iter().all(|n| !n.is_empty()),
                        "comptime: rustc_codegen_clr_add_generic_abstract_method_def for `{method_name}` \
                         got a malformed GENERIC_PARAMS list ({names_arg:?}) — a generic method \
                         definition must declare at least one non-empty parameter name \
                         (unreachable from the #[dotnet_interface] macro, which builds the list \
                         from parsed idents)"
                    );
                    let fn_ty = ctx.monomorphize(subst_ref[2].as_type().unwrap());
                    let TyKind::FnDef(fdef, fsubst) = fn_ty.kind() else {
                        panic!("comptime: generic abstract method signature carrier is not a function definition");
                    };
                    let fsubst = ctx.monomorphize(*fsubst);
                    let carrier = Instance::try_resolve(ctx.tcx(), env, *fdef, fsubst)
                        .expect("comptime: invalid generic abstract method signature carrier")
                        .expect("comptime: could not resolve generic abstract method signature carrier instance");
                    class
                        .abstract_methods
                        .push((method_name, carrier, vec![], generic_names));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_mark_last_abstract_method_out_params") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const OUT_PARAMS: &'static str> — a CSV of 1-based,
                    // receiver-stripped parameter positions (e.g. "1,3"). Marks the LAST-added
                    // abstract member (same "mark the LAST member" contract as
                    // `mark_last_method_override`). Substring-dispatch safety: this needle
                    // neither contains nor is contained by any sibling intrinsic's
                    // (`mark_last_abstract_method` vs `mark_last_method_*` — the `abstract_`
                    // infix breaks every containment; audited against the whole chain).
                    let csv = garg_to_string(subst_ref[0], ctx.tcx());
                    let out_params: Vec<u16> = csv
                        .split(',')
                        .map(|s| {
                            s.trim().parse::<u16>().unwrap_or_else(|_| {
                                panic!(
                                    "comptime: rustc_codegen_clr_mark_last_abstract_method_out_params \
                                     got a malformed OUT_PARAMS entry {s:?} (expected a CSV of \
                                     1-based positions like \"1,3\")"
                                )
                            })
                        })
                        .collect();
                    let last = class.abstract_methods.last_mut().expect(
                        "comptime: rustc_codegen_clr_mark_last_abstract_method_out_params called \
                         with no preceding add_abstract_method_def in this entrypoint",
                    );
                    last.2 = out_params;
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_mark_last_abstract_property_get") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const PROP_NAME: &'static str>. Marks the LAST-added ABSTRACT
                    // member as this property's getter (same "mark the LAST member" contract as
                    // `mark_last_abstract_method_out_params`). Substring-dispatch safety:
                    // `mark_last_abstract_property_get` neither contains nor is contained by any
                    // sibling needle (`…_method_out_params` diverges at `method_`/`property_`;
                    // `mark_last_method_*` diverges at `abstract_`; `_get` vs `_set` diverge at
                    // the final token; audited against the whole chain).
                    let prop_name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let (method_name, ..) = class.abstract_methods.last().expect(
                        "comptime: rustc_codegen_clr_mark_last_abstract_property_get called with \
                         no preceding add_abstract_method_def in this entrypoint",
                    );
                    class
                        .property_bindings
                        .insert(method_name.clone(), (prop_name, true));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_mark_last_abstract_property_set") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    let prop_name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let (method_name, ..) = class.abstract_methods.last().expect(
                        "comptime: rustc_codegen_clr_mark_last_abstract_property_set called with \
                         no preceding add_abstract_method_def in this entrypoint",
                    );
                    class
                        .property_bindings
                        .insert(method_name.clone(), (prop_name, false));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_default_method_def") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const FNAME, FnType> — same shape as `add_abstract_method_def`,
                    // but the resolved fn is a REAL codegen'd target (the lifted default body)
                    // the member will alias, not a signature-only carrier. Substring-dispatch
                    // safety: "add_default_method_def" neither contains nor is contained by
                    // "add_method_def", "add_abstract_method_def", "add_static_method_def",
                    // "add_static_abstract_method_def", or "add_default_ctor" (the `default_`
                    // infix / `method_def` tail break every containment; audited against the
                    // whole chain), so the chain order cannot misdispatch.
                    let method_name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let fn_ty = ctx.monomorphize(subst_ref[1].as_type().unwrap());
                    let TyKind::FnDef(fdef, fsubst) = fn_ty.kind() else {
                        panic!("comptime: default interface method target is not a function definition");
                    };
                    let fsubst = ctx.monomorphize(*fsubst);
                    let target = Instance::try_resolve(ctx.tcx(), env, *fdef, fsubst)
                        .expect("comptime: invalid default interface method target")
                        .expect("comptime: could not resolve default interface method target instance");
                    class.default_methods.push((method_name, target));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_static_abstract_method_def") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const FNAME, FnType> — same shape as `add_abstract_method_def`,
                    // but the carrier has NO receiver (a `static abstract` member's signature is
                    // its C#-visible parameter list verbatim). Substring-dispatch safety: this
                    // name neither contains nor is contained by `…add_method_def`,
                    // `…add_abstract_method_def`, or `…add_static_method_def` (the `static_`
                    // infix breaks every containment), so the chain order cannot misdispatch.
                    let method_name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let fn_ty = ctx.monomorphize(subst_ref[1].as_type().unwrap());
                    let TyKind::FnDef(fdef, fsubst) = fn_ty.kind() else {
                        panic!("comptime: static abstract method signature carrier is not a function definition");
                    };
                    let fsubst = ctx.monomorphize(*fsubst);
                    let carrier = Instance::try_resolve(ctx.tcx(), env, *fdef, fsubst)
                        .expect("comptime: invalid static abstract method signature carrier")
                        .expect("comptime: could not resolve static abstract method signature carrier instance");
                    class.static_abstract_methods.push((method_name, carrier));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_mark_last_method_override") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const BASE_ASM, const BASE_TYPE>. Must follow the
                    // `add_method_def` call for the overriding method in the same entrypoint's
                    // MIR sequence (see `rustc_codegen_clr_mark_last_method_override`'s doc) — the
                    // base method's own name is assumed identical to the last-registered virtual
                    // method's name, the only shape this narrow spike supports.
                    let base_asm = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let base_type = garg_to_string(subst_ref[1], ctx.tcx()).replace("::", ".");
                    let (method_name, _) = class
                        .methods
                        .last()
                        .expect(
                            "comptime: rustc_codegen_clr_mark_last_method_override called with no \
                             preceding add_method_def in this entrypoint",
                        )
                        .clone();
                    class.method_overrides.insert(method_name, (base_asm, base_type));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_mark_last_method_event_add") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    let event_name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    // On an interface (`#[dotnet_interface]` + `#[dotnet_event]`) the accessor was
                    // just pushed by `add_abstract_method_def` (interface members have no bodies);
                    // on a class, by `add_method_def`. Same "mark the LAST-added method" contract.
                    let method_name = if class.is_interface {
                        class.abstract_methods.last().map(|(name, ..)| name.clone())
                    } else {
                        class.methods.last().map(|(name, _)| name.clone())
                    }
                    .expect(
                        "comptime: rustc_codegen_clr_mark_last_method_event_add called with no \
                         preceding add_method_def/add_abstract_method_def in this entrypoint",
                    );
                    class.event_bindings.insert(method_name, (event_name, true));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_mark_last_method_event_remove") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    let event_name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let method_name = if class.is_interface {
                        class.abstract_methods.last().map(|(name, ..)| name.clone())
                    } else {
                        class.methods.last().map(|(name, _)| name.clone())
                    }
                    .expect(
                        "comptime: rustc_codegen_clr_mark_last_method_event_remove called with \
                         no preceding add_method_def/add_abstract_method_def in this entrypoint",
                    );
                    class.event_bindings.insert(method_name, (event_name, false));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_static_method_def") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const FNAME, FnType> — so FNAME is [0], FnType is [1].
                    let method_name = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let fn_ty = ctx.monomorphize(subst_ref[1].as_type().unwrap());
                    let TyKind::FnDef(fdef, fsubst) = fn_ty.kind() else {
                        panic!("comptime: static method target is not a function definition");
                    };
                    let fsubst = ctx.monomorphize(*fsubst);
                    let target = Instance::try_resolve(ctx.tcx(), env, *fdef, fsubst)
                        .expect("comptime: invalid static method target")
                        .expect("comptime: could not resolve static method target instance");
                    class.static_methods.push((method_name, target));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_generic_interface_impl") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const IFACE_ASM, const IFACE, const GENERIC_ASM, const GENERIC_TYPE,
                    // const GENERIC_IS_VALUETYPE>.
                    let iface_asm = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let iface_name = garg_to_string(subst_ref[1], ctx.tcx()).replace("::", ".");
                    let generic_asm = garg_to_string(subst_ref[2], ctx.tcx()).replace("::", ".");
                    let generic_type = garg_to_string(subst_ref[3], ctx.tcx()).replace("::", ".");
                    let generic_is_valuetype = garg_to_bool(subst_ref[4], ctx.tcx());
                    class.interfaces.push((
                        iface_asm,
                        iface_name,
                        vec![(generic_asm, generic_type, generic_is_valuetype)],
                    ));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_interface_impl") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const IFACE_ASM, const IFACE>.
                    let iface_asm = garg_to_string(subst_ref[0], ctx.tcx()).replace("::", ".");
                    let iface_name = garg_to_string(subst_ref[1], ctx.tcx()).replace("::", ".");
                    class.interfaces.push((iface_asm, iface_name, vec![]));
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_primary_ctor") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    class.has_primary_ctor = true;
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_default_ctor") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    class.has_default_ctor = true;
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_add_field_setters") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    class.has_field_setters = true;
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_mark_interface") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    class.is_interface = true;
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_set_type_generics") {
                    let src = operand_local(&args[0].node);
                    let mut class = locals[src].as_class().clone();
                    // Generics: <const NAMES: &'static str> — `;`-separated declared parameter
                    // names in declaration order (`"T"`, `"K;V"`), same separator convention as
                    // `implements=`. Substring-dispatch safety: "set_type_generics" neither
                    // contains nor is contained by any sibling intrinsic's needle (audited
                    // against the whole contains() chain).
                    let names = garg_to_string(subst_ref[0], ctx.tcx());
                    class.type_generics = names
                        .split(';')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect();
                    assert!(
                        !class.type_generics.is_empty(),
                        "comptime: rustc_codegen_clr_set_type_generics got an empty NAMES list \
                         ({names:?}) — a generic type definition must declare at least one \
                         parameter name"
                    );
                    ComptimeLocalVar::Class(class)
                } else if fname.contains("rustc_codegen_clr_finish_type") {
                    let src = operand_local(&args[0].node);
                    let class = locals[src].as_class().clone();
                    finish_type(ctx, &class);
                    ComptimeLocalVar::Void
                } else {
                    // black_box, into(), and any other incidental call: irrelevant to the type shape.
                    ComptimeLocalVar::NotSet
                };
                locals[usize::from(dest_local)] = result;

                let Some(next) = target else {
                    return;
                };
                block_id = *next;
            }
            TerminatorKind::Return => return,
            // Be lenient: a diverging/odd terminator just ends interpretation.
            _ => return,
        }
    }
}

/// An operand, as the local index holding the (threaded) `ClassDef` value.
fn operand_local(op: &rustc_middle::mir::Operand<'_>) -> usize {
    usize::from(
        op.place()
            .expect("comptime: unsupported operand in interop type definition")
            .as_local()
            .expect("comptime: unsupported operand in interop type definition"),
    )
}

/// Maps a well-known CLR primitive's bare type name (e.g. `"System.Int32"`) to cilly's native
/// [`Type`] variant, so it encodes via its dedicated ECMA-335 element-type code
/// (`ELEMENT_TYPE_I4`, …) instead of the generic `VALUETYPE <TypeDefOrRef>`/`CLASS <TypeDefOrRef>`
/// form a string-referenced external [`ClassRef`] produces by construction. Returns `None` for
/// anything else (a user-defined struct/class, `DateTime`, `Guid`, …), which genuinely has no
/// compact element-type code and must stay a `ClassRef`.
///
/// Only reachable from a generic-interface-argument position today (see
/// `rustc_codegen_clr_add_generic_interface_impl`'s caller in `finish_type` below) — proven
/// necessary there, not theoretical: a hand-assembled ilasm repro AND this exporter's own PE
/// writer both independently produced spec-valid `implements class System.IEquatable`1<valuetype
/// [System.Runtime]System.Int32>` metadata (confirmed correct via `System.Reflection.Metadata`,
/// byte-for-byte matching ECMA-335 §II.23.2.12's `GenericInst` grammar) that a real `csc` rejects
/// with `CS0648: '' is a type not supported by the language` — while the IDENTICAL shape with a
/// REFERENCE-typed argument (`System.String`) compiles cleanly, isolating the failure to
/// value-type PRIMITIVES specifically wrapped as `VALUETYPE <TypeRef>` rather than encoded via
/// their dedicated element-type byte.
fn well_known_primitive_type(name: &str) -> Option<Type> {
    Some(match name {
        "System.Boolean" => Type::Bool,
        "System.Char" => Int::U16.into(),
        "System.SByte" => Int::I8.into(),
        "System.Byte" => Int::U8.into(),
        "System.Int16" => Int::I16.into(),
        "System.UInt16" => Int::U16.into(),
        "System.Int32" => Int::I32.into(),
        "System.UInt32" => Int::U32.into(),
        "System.Int64" => Int::I64.into(),
        "System.UInt64" => Int::U64.into(),
        "System.IntPtr" => Int::ISize.into(),
        "System.UIntPtr" => Int::USize.into(),
        "System.Single" => Type::Float(Float::F32),
        "System.Double" => Type::Float(Float::F64),
        _ => return None,
    })
}

/// Rewrites an interface-member signature carrier's lowered signature so every Rust `&mut T`
/// parameter becomes a **managed byref** (`Type::Ref` => `ELEMENT_TYPE_BYREF`, C# `ref T`/`out T`)
/// instead of the raw pointer (`Type::Ptr` => `T*`, C# `int*` — unsafe-only, and NOT
/// name+signature-matched by a C# `void M(ref int)` implementor) that the frontend's uniform
/// `TyKind::Ref | TyKind::RawPtr => nptr` lowering produces.
///
/// Deliberately a TARGETED comptime-layer rewrite of the carrier's already-lowered signature, not
/// a change to `get_type` (which would alter every `&mut` in all of codegen): the decision is
/// driven by the carrier's **Rust-level** parameter types (`TyKind::Ref(_, _, Mut)`), so it is
/// authoritative even through type aliases the `#[dotnet_interface]` macro can't see
/// syntactically. Raw pointers (`*mut T`/`*const T`) are untouched — they keep today's `T*`
/// meaning as the documented escape hatch. Shared `&T` parameters and reference RETURNS are
/// rejected here with a panic: the macro already rejects both when spelled literally, so reaching
/// this code with one means it was hidden behind a type alias — emitting the frontend's `T*`
/// lowering there would be a silently-different surface than the documented reject (C# `in T`
/// would need `modreq(InAttribute)`; `ref`-returns are a different metadata shape), so fail
/// loudly instead.
///
/// `skip` is the number of leading signature inputs that are NOT user-visible parameters (1 for
/// an instance member's `_this` receiver handle, 0 for a `static abstract` member).
///
/// A `&mut` to an UNSIZED pointee (`&mut str`/`&mut [T]`/`&mut dyn Trait`) lowers to a fat-ptr
/// struct, not `Type::Ptr` — no managed-byref equivalent exists, so it panics loudly (the
/// fail-loudly comptime idiom; the macro can't catch alias-hidden cases, this backstop can).
fn byref_interface_sig<'tcx>(
    ctx: &mut MethodCompileCtx<'tcx, '_>,
    method_name: &str,
    carrier: Instance<'tcx>,
    mut sig: cilly::FnSig,
    skip: usize,
) -> cilly::FnSig {
    let tcx = ctx.tcx();
    let fn_ty = carrier.ty(tcx, TypingEnv::fully_monomorphized());
    let rust_sig = tcx.instantiate_bound_regions_with_erased(fn_ty.fn_sig(tcx));
    let rust_inputs = rust_sig.inputs();
    // The carrier is a plain (non-`rust_call`) fn generated by `#[dotnet_interface]`, so its
    // Rust-level inputs are 1:1 parallel with the lowered signature's.
    assert_eq!(
        rust_inputs.len(),
        sig.inputs().len(),
        "comptime: interface member `{method_name}`'s carrier signature is not parallel to its \
         lowered signature — unsupported carrier shape"
    );
    let mut inputs = sig.inputs().to_vec();
    for (i, input) in inputs.iter_mut().enumerate().skip(skip) {
        match rust_inputs[i].kind() {
            TyKind::Ref(_, _, Mutability::Mut) => (),
            // The macro rejects a literally-spelled `&T`; only a type alias can smuggle one this
            // far. Emitting `T*` here would silently contradict that documented reject.
            TyKind::Ref(_, _, Mutability::Not) => panic!(
                "comptime: interface member `{method_name}`, parameter {i}: shared-reference \
                 (`&T`) parameters are not supported on `#[dotnet_interface]` members (C# `in T` \
                 would need `modreq(InAttribute)`) — this one is hidden behind a type alias, \
                 which the macro cannot see. Use `&mut T` (C# `ref T`), pass by value, or use a \
                 raw pointer (`*const T` => C# `T*`)"
            ),
            _ => continue,
        }
        match input {
            Type::Ptr(inner) => *input = Type::Ref(*inner),
            _ => panic!(
                "comptime: interface member `{method_name}`, parameter {i}: `&mut` to an unsized \
                 type (`&mut str`, `&mut [T]`, `&mut dyn Trait`, …) has no managed-byref (`ref`) \
                 equivalent and is not supported on `#[dotnet_interface]` members — pass a thin \
                 `&mut T` or a raw pointer instead"
            ),
        }
    }
    // Reference RETURNS: the macro rejects `-> &T`/`-> &mut T` spelled literally; an alias-hidden
    // one would otherwise ship the frontend's `T*` return (C# `ref`-returns are a different
    // metadata shape than anything emitted here). Same fail-loudly backstop as the params above.
    assert!(
        !matches!(rust_sig.output().kind(), TyKind::Ref(..)),
        "comptime: interface member `{method_name}`: reference returns (`-> &T` / `-> &mut T`) \
         are not supported on `#[dotnet_interface]` members — this one is hidden behind a type \
         alias, which the macro cannot see"
    );
    sig.set_inputs(inputs);
    sig
}

/// Build and register the managed class, then attach its virtual methods (which alias the Rust fns).
fn finish_type<'tcx>(ctx: &mut MethodCompileCtx<'tcx, '_>, class: &PendingClass<'tcx>) {
    // Superclass reference (e.g. [System.Runtime]System.Object).
    let extends = class.superclass.as_ref().map(|(asm_name, cls_name)| {
        let cls = ctx.alloc_string(cls_name.clone());
        let asm = if asm_name.is_empty() {
            None
        } else {
            Some(ctx.alloc_string(asm_name.clone()))
        };
        ctx.alloc_class_ref(ClassRef::new(cls, asm, false, [].into()))
    });

    // Fields: (type, interned name, no explicit offset — let the runtime lay the class out).
    let fields: Vec<_> = class
        .fields
        .iter()
        .map(|(tpe, name)| (*tpe, ctx.alloc_string(name.clone()), None))
        .collect();

    let name = ctx.alloc_string(class.name.clone());
    // Idempotent registration: a class may be described by more than one comptime entrypoint (e.g. the
    // `#[dotnet_class]` struct decl plus a `#[dotnet_methods]` impl block re-opening it). `class_def`
    // panics on a duplicate name, so if this class is already registered, reuse its `ClassDefIdx` and
    // just append this entrypoint's methods/ctors. Codegen-unit ordering is NOT guaranteed, so a
    // re-opening entrypoint (which carries no fields) can register the bare class first; when the
    // field-carrying struct entrypoint then runs, it MERGES its fields into the existing def (adding
    // only those not already present). This keeps field/method emission order-independent: whichever
    // entrypoint runs last, the final classdef has every field, so the `read_*`/`set_*` accessors
    // typecheck.
    let self_cref = ctx.alloc_class_ref(ClassRef::new(name, None, class.is_value_type, [].into()));
    let class_idx = if let Some(existing) = ctx.class_ref_to_def(self_cref) {
        if !fields.is_empty() {
            let def = ctx.class_mut(existing);
            let existing_fields = def.fields_mut();
            for field in &fields {
                if !existing_fields.iter().any(|(_, n, _)| *n == field.1) {
                    existing_fields.push(*field);
                }
            }
        }
        existing
    } else {
        // Generic type DEFINITIONS (`rustc_codegen_clr_set_type_generics`, from
        // `#[dotnet_interface] trait IFoo<T>`) are interface-only: an interface has no layout,
        // so the historical no-explicit-layout-on-.NET-generics ban does not apply — but it DOES
        // apply to classes, which stay walled loudly here rather than emitting a generic class
        // TypeDef whose layout the CLR controls.
        assert!(
            class.type_generics.is_empty() || class.is_interface,
            "comptime: type generics are only supported on #[dotnet_interface] interfaces \
             (class '{}' declares {:?})",
            class.name,
            class.type_generics
        );
        let generics = u32::try_from(class.type_generics.len())
            .expect("comptime: generic arity over u32");
        let mut def = ClassDef::new(
            name,
            class.is_value_type,
            generics,
            extends,
            fields,
            vec![],
            Access::Public,
            None,
            None,
            true,
        );
        // `#[dotnet_interface]`: a genuine ECMA-335 `interface` TypeDef (no base, Interface+Abstract
        // flags). An interface is always registered fresh here (a trait is defined by exactly one
        // entrypoint — no `#[dotnet_methods]`-style re-opening), so the idempotent-reuse branch
        // above never applies to it.
        if class.is_interface {
            def = def.with_interface();
        }
        if !class.type_generics.is_empty() {
            let names = class
                .type_generics
                .iter()
                .map(|n| ctx.alloc_string(n.clone()))
                .collect();
            def = def.with_type_generic_names(names);
        }
        ctx.class_def(def)
            .expect("comptime: layout error registering interop class")
    };

    // Attach implemented interfaces. Each is a ClassRef into the interface's declaring assembly; the
    // virtual methods emitted below satisfy them by name+signature (implicit interface implementation).
    // A non-empty `generic_args` builds one external-type ClassRef per arg (the same construction as
    // the interface reference itself, never derived from a Rust type) and attaches it as the
    // interface ClassRef's own generic argument list.
    for (iface_asm, iface_name, generic_args) in &class.interfaces {
        let iface_cls = ctx.alloc_string(iface_name.clone());
        let iface_asm_ref = if iface_asm.is_empty() {
            None
        } else {
            Some(ctx.alloc_string(iface_asm.clone()))
        };
        let generics: Vec<Type> = generic_args
            .iter()
            .map(|(gen_asm, gen_name, gen_is_valuetype)| {
                // Well-known CLR primitives (Int32, Boolean, …) MUST use their dedicated
                // ECMA-335 element-type code (`ELEMENT_TYPE_I4`, …) even as a GENERICINST
                // argument, not the generic `VALUETYPE <TypeDefOrRef>` encoding a string-named
                // external ClassRef produces by construction — see `well_known_primitive_type`'s
                // doc for the empirical proof (a hand-assembled ilasm repro AND this exporter's
                // own PE writer both independently produced spec-valid `VALUETYPE
                // [System.Runtime]System.Int32` metadata that a real `csc` rejects with CS0648
                // for a class implementing `IEquatable<int>`, while the IDENTICAL shape with a
                // REFERENCE-typed argument like `System.String` compiles fine — isolating the
                // failure to the value-type-primitive case specifically).
                if let Some(tpe) = well_known_primitive_type(gen_name) {
                    return tpe;
                }
                let gen_cls = ctx.alloc_string(gen_name.clone());
                let gen_asm_ref = if gen_asm.is_empty() {
                    None
                } else {
                    Some(ctx.alloc_string(gen_asm.clone()))
                };
                Type::ClassRef(ctx.alloc_class_ref(ClassRef::new(
                    gen_cls,
                    gen_asm_ref,
                    *gen_is_valuetype,
                    [].into(),
                )))
            })
            .collect();
        let iface_ref =
            ctx.alloc_class_ref(ClassRef::new(iface_cls, iface_asm_ref, false, generics.into()));
        ctx.class_mut(class_idx).add_interface(iface_ref);
    }

    // Accumulates each event's `add`/`remove` `MethodRef` + delegate `Type` as the method loops
    // below encounter them (in whatever order `class.methods`/`class.abstract_methods` happen to
    // hold them) — see `rustc_codegen_clr_mark_last_method_event_add`'s doc. Built into real
    // `EventDef`s once the loops finish and every event has both halves. A `BTreeMap` (NOT
    // `HashMap`) so a multi-event class emits its `Event` rows in a deterministic (name-sorted)
    // order — the PE writer requires deterministic row emission.
    let mut pending_events: std::collections::BTreeMap<
        String,
        (Option<Interned<MethodRef>>, Option<Interned<MethodRef>>, Option<Type>),
    > = std::collections::BTreeMap::new();

    // Each virtual method aliases an ordinary Rust fn (codegen'd separately). The Rust fn takes the
    // receiver as its first explicit arg, so its signature matches the virtual method's.
    for (method_name, target) in &class.methods {
        let call_info = CallInfo::sig_from_instance_(*target, ctx);
        let fn_sig = call_info.sig().clone();
        // The exporter requires one arg name per signature input (the receiver included for a virtual).
        let arg_names = vec![None; fn_sig.inputs().len()];
        let sig = ctx.alloc_sig(fn_sig);
        let target_name = function_name(ctx.tcx().symbol_name(*target));
        let target_name = ctx.alloc_string(target_name);
        let main_module = *ctx.main_module();
        let target_mref = MethodRef::new(main_module, target_name, sig, MethodKind::Static, [].into());
        let target_ref = ctx.alloc_methodref(target_mref);
        let mname = ctx.alloc_string(method_name.clone());
        // `Access::Extern` marks this as a dead-code-elimination ROOT — a Rust-defined managed class is
        // an exported surface with no internal caller, so (like `#[no_mangle]` exports) its methods must
        // be roots or the whole class would be culled. The DCE also follows the `AliasFor` edge to keep
        // the target Rust fn alive (see `Assembly::eliminate_dead_fns`).
        let mut mdef = MethodDef::new(
            Access::Extern,
            class_idx,
            mname,
            sig,
            MethodKind::Virtual,
            MethodImpl::AliasFor(target_ref),
            arg_names,
        );
        if let Some((base_asm, base_type)) = class.method_overrides.get(method_name) {
            let base_cls = ctx.alloc_string(base_type.clone());
            let base_asm_ref = if base_asm.is_empty() {
                None
            } else {
                Some(ctx.alloc_string(base_asm.clone()))
            };
            let base_class_ref =
                ctx.alloc_class_ref(ClassRef::new(base_cls, base_asm_ref, false, [].into()));
            let base_mref =
                MethodRef::new(base_class_ref, mdef.name(), sig, MethodKind::Virtual, [].into());
            let base_mref = ctx.alloc_methodref(base_mref);
            mdef = mdef.with_override(base_mref);
        }
        if let Some((event_name, is_add)) = class.event_bindings.get(method_name) {
            // The delegate type is the method's own second signature input (index 0 is the
            // receiver) — the value being subscribed/unsubscribed — never a separately-spelled
            // string (see `rustc_codegen_clr_mark_last_method_event_add`'s doc).
            let delegate_ty = ctx[sig].inputs()[1];
            let mref = ctx.alloc_methodref(mdef.ref_to());
            let entry = pending_events.entry(event_name.clone()).or_insert((None, None, None));
            if *is_add {
                entry.0 = Some(mref);
            } else {
                entry.1 = Some(mref);
            }
            entry.2 = Some(delegate_ty);
        }
        ctx.new_method(mdef);
    }

    // Accumulates each property's accessor `MethodRef`s + value `Type` as the abstract-member
    // loop below encounters them — see `rustc_codegen_clr_mark_last_abstract_property_get`'s
    // doc. A declaration-ordered Vec, NOT a HashMap: the PE writer requires deterministic row
    // emission (MVID/output stability), and a hash-ordered iteration here would reorder
    // `Property` rows between builds. Built into real `PropertyDef`s once the loop finishes.
    // (getter_tpe/setter_tpe are kept separately so the type-agreement check below can name
    // both sides in its panic message.)
    #[allow(clippy::type_complexity)]
    let mut pending_properties: Vec<(
        String,
        (
            Option<(Interned<MethodRef>, Type)>, // getter + its return type
            Option<(Interned<MethodRef>, Type)>, // setter + its value-parameter type
        ),
    )> = Vec::new();
    assert!(
        class.property_bindings.is_empty() || class.is_interface,
        "comptime: properties are only supported on a #[dotnet_interface] (class '{}' is not an \
         interface)",
        class.name
    );

    // Abstract (no-body) interface members (`#[dotnet_interface]`). The signature comes from a
    // carrier fn (like a virtual method's target), but the member is emitted as `MethodImpl::
    // Missing` + `.with_abstract()` — NO `AliasFor`, so nothing is codegen'd for it and its
    // `MethodDef.RVA` stays 0 (§II.22.26). The receiver is the carrier's first input (the interface
    // handle), sliced off in the C#-visible declared signature exactly like a virtual method.
    for (method_name, carrier, out_params, generic_names) in &class.abstract_methods {
        let call_info = CallInfo::sig_from_instance_(*carrier, ctx);
        // `&mut T` parameters => managed byrefs (C# `ref T`) — see `byref_interface_sig`'s doc.
        // `skip = 1`: input 0 is the `_this` receiver handle.
        let fn_sig =
            byref_interface_sig(ctx, method_name, *carrier, call_info.sig().clone(), 1);
        // `#[dotnet_out]` positions (1-based among the receiver-stripped params, so sequence `s`
        // is signature input `s` here — the receiver occupies index 0). The macro already
        // guarantees each is a `&mut T` parameter; this is the backend's defense-in-depth assert,
        // not a user-facing error.
        for seq in out_params {
            let input = fn_sig.inputs().get(usize::from(*seq));
            assert!(
                matches!(input, Some(Type::Ref(_))),
                "comptime: interface member `{method_name}`'s `#[dotnet_out]` parameter {seq} \
                 did not lower to a managed byref (got {input:?}) — unreachable from the \
                 `#[dotnet_interface]` macro, which only accepts `#[dotnet_out]` on `&mut T`"
            );
        }
        let arg_names = vec![None; fn_sig.inputs().len()];
        let sig = ctx.alloc_sig(fn_sig);
        let mname = ctx.alloc_string(method_name.clone());
        let mut mdef = MethodDef::new(
            Access::Extern,
            class_idx,
            mname,
            sig,
            MethodKind::Virtual,
            MethodImpl::Missing,
            arg_names,
        )
        .with_abstract();
        if !out_params.is_empty() {
            mdef = mdef.with_out_params(out_params.clone());
        }
        // A generic method DEFINITION (`rustc_codegen_clr_add_generic_abstract_method_def`):
        // attach the declared type-parameter names — the PE writer (export.rs Pass 3) stamps
        // `SIG_GENERIC` + `GenParamCount` on the signature blob, emits one method-owned
        // `GenericParam` row per name, and asserts every `!!N` marker in the signature is in
        // range of this list.
        if !generic_names.is_empty() {
            mdef = mdef.with_generic_params(
                generic_names.iter().map(|n| ctx.alloc_string(n.clone())).collect(),
            );
        }
        // An abstract accessor of an INTERFACE event (`#[dotnet_event]` inside
        // `#[dotnet_interface]`) — same binding block as the virtual (class) loop above: the
        // delegate type is the accessor's own second signature input (index 0 is the receiver).
        if let Some((event_name, is_add)) = class.event_bindings.get(method_name) {
            let inputs = ctx[sig].inputs();
            assert_eq!(
                inputs.len(),
                2,
                "comptime: interface event accessor `{method_name}` must take exactly (receiver, \
                 delegate) — the `#[dotnet_interface]` macro guarantees this shape"
            );
            let delegate_ty = inputs[1];
            let mref = ctx.alloc_methodref(mdef.ref_to());
            let entry = pending_events.entry(event_name.clone()).or_insert((None, None, None));
            if *is_add {
                entry.0 = Some(mref);
            } else {
                entry.1 = Some(mref);
            }
            entry.2 = Some(delegate_ty);
        }
        // An abstract accessor of an INTERFACE property (`#[dotnet_property]` inside
        // `#[dotnet_interface]`): the property's value type comes from the accessor's own
        // signature — getter: the return type; setter: the single non-receiver parameter
        // (index 0 is the receiver). The macro guarantees the accessor shapes; the asserts here
        // are the backend's defense-in-depth for a hand-rolled entrypoint, phrased loudly.
        if let Some((prop_name, is_getter)) = class.property_bindings.get(method_name) {
            let accessor_sig = &ctx[sig];
            let (mref_tpe, slot_is_getter) = if *is_getter {
                let tpe = *accessor_sig.output();
                assert!(
                    tpe != Type::Void,
                    "comptime: property '{prop_name}' getter `{method_name}` returns void — a \
                     property getter must return the property's value"
                );
                (tpe, true)
            } else {
                assert_eq!(
                    accessor_sig.inputs().len(),
                    2,
                    "comptime: property '{prop_name}' setter `{method_name}` must take exactly \
                     (receiver, value) — got {} signature input(s)",
                    accessor_sig.inputs().len()
                );
                (accessor_sig.inputs()[1], false)
            };
            let mref = ctx.alloc_methodref(mdef.ref_to());
            let entry = match pending_properties
                .iter_mut()
                .find(|(name, _)| name == prop_name)
            {
                Some((_, entry)) => entry,
                None => {
                    pending_properties.push((prop_name.clone(), (None, None)));
                    &mut pending_properties
                        .last_mut()
                        .expect("just pushed")
                        .1
                }
            };
            let slot = if slot_is_getter { &mut entry.0 } else { &mut entry.1 };
            assert!(
                slot.is_none(),
                "comptime: property '{prop_name}' declares two {}s — unreachable from the \
                 #[dotnet_interface] macro, which rejects duplicate accessor names",
                if slot_is_getter { "getter" } else { "setter" }
            );
            *slot = Some((mref, mref_tpe));
        }
        ctx.new_method(mdef);
    }

    // Build the accumulated property halves into real `PropertyDef`s, in declaration order.
    // Fail-loudly boundary (the loud comptime-failure precedent is the event both-halves panic
    // below): a getter/setter TYPE disagreement and a write-only property are both clean panics
    // naming the property, never silently-wrong metadata.
    for (prop_name, (getter, setter)) in pending_properties {
        if let (Some((_, get_tpe)), Some((_, set_tpe))) = (&getter, &setter) {
            assert!(
                get_tpe == set_tpe,
                "comptime: property '{prop_name}' getter returns {get_tpe:?} but its setter \
                 takes {set_tpe:?} — both accessors must agree on the property's value type"
            );
        }
        let (tpe, getter_ref, setter_ref) = match (getter, setter) {
            (Some((g, tpe)), setter) => (tpe, Some(g), setter.map(|(s, _)| s)),
            (None, Some(_)) => panic!(
                "comptime: property '{prop_name}' has a set_* accessor but no get_* — \
                 write-only properties are not supported; add get_{prop_name}"
            ),
            (None, None) => unreachable!("a pending property always has at least one accessor"),
        };
        let name = ctx.alloc_string(prop_name);
        ctx.class_mut(class_idx)
            .add_property(cilly::class::PropertyDef::new(name, tpe, getter_ref, setter_ref));
    }

    // `static abstract` interface members (.NET 7+ static virtual members in interfaces, from a
    // `#[dotnet_interface]` trait fn with no `self` receiver). Same `MethodImpl::Missing` +
    // `.with_abstract()` no-body shape as the instance loop above, but `MethodKind::Static`: the
    // carrier has NO receiver, so its signature is used VERBATIM (nothing to slice), and the PE
    // writer stamps Roslyn's exact `Public|Static|Virtual|HideBySig|Abstract` (0x4D6) flags — see
    // `MetadataBuilder::mark_method_static_abstract`.
    assert!(
        class.is_interface || class.static_abstract_methods.is_empty(),
        "comptime: static abstract members are only valid on a #[dotnet_interface] (class '{}' \
         is not an interface)",
        class.name
    );
    for (method_name, carrier) in &class.static_abstract_methods {
        let call_info = CallInfo::sig_from_instance_(*carrier, ctx);
        // `&mut T` parameters => managed byrefs, exactly like the instance loop above — a static
        // abstract's C# implementor writes `public static … M(ref T x)` and the CLR matches it by
        // name+signature, so the byref mapping must be consistent across both member kinds.
        // `skip = 0`: a static carrier has no receiver input.
        let fn_sig =
            byref_interface_sig(ctx, method_name, *carrier, call_info.sig().clone(), 0);
        let arg_names = vec![None; fn_sig.inputs().len()];
        let sig = ctx.alloc_sig(fn_sig);
        let mname = ctx.alloc_string(method_name.clone());
        let mdef = MethodDef::new(
            Access::Extern,
            class_idx,
            mname,
            sig,
            MethodKind::Static,
            MethodImpl::Missing,
            arg_names,
        )
        .with_abstract();
        ctx.new_method(mdef);
    }

    // **Default interface methods** (DIM, CoreCLR 3.0+): virtual, NON-abstract members with a
    // real body, on the interface `TypeDef` itself. Byte-for-byte the same emission as a class
    // virtual in the `methods` loop above (`MethodKind::Virtual` + `MethodImpl::AliasFor` a
    // MainModule static — the lifted default body), minus override/event handling (the macro
    // rejects both on a defaulted member): Pass 3 of the PE writer stamps `Virtual|NewSlot`
    // WITHOUT `Abstract`, and Pass 4 assembles the alias target's body, so the member's RVA is
    // non-zero — exactly Roslyn's DIM shape (§II.23.1.10 permits non-abstract virtual bodies on
    // an interface; the CLR dispatches to them when the implementing class omits the member).
    assert!(
        class.default_methods.is_empty() || class.is_interface,
        "comptime: default interface methods are only valid on a #[dotnet_interface] (class \
         '{}' is not an interface)",
        class.name
    );
    for (method_name, target) in &class.default_methods {
        // SEMANTIC backstop behind the macro's syntactic reject of reference parameters/returns
        // on defaulted members: the macro only sees spelled-out `&`/`&mut` types, so an alias
        // (`type Slot<'a> = &'a mut i32;`) slips past it. The declared member would then carry
        // the frontend's `T*` lowering — inconsistent with the managed-byref (`ref T`) surface
        // an identically-typed ABSTRACT sibling gets from `byref_interface_sig`, and a violation
        // of the documented "no reference params on a default body" contract. Decide from the
        // lifted fn's RUST-level types (authoritative through aliases) and fail loudly.
        {
            let tcx = ctx.tcx();
            let fn_ty = target.ty(tcx, TypingEnv::fully_monomorphized());
            let rust_sig = tcx.instantiate_bound_regions_with_erased(fn_ty.fn_sig(tcx));
            // Input 0 is the `this: <Name>Handle` receiver the macro synthesized — never a Ref.
            for (i, input) in rust_sig.inputs().iter().enumerate().skip(1) {
                assert!(
                    !matches!(input.kind(), TyKind::Ref(..)),
                    "comptime: default interface method `{method_name}`, parameter {i}: \
                     reference parameters (`&T`/`&mut T`) are not supported on a method with a \
                     default body — this one is hidden behind a type alias, which the \
                     `#[dotnet_interface]` macro cannot see. Pass by value or drop the default \
                     body"
                );
            }
            assert!(
                !matches!(rust_sig.output().kind(), TyKind::Ref(..)),
                "comptime: default interface method `{method_name}`: reference returns \
                 (`-> &T` / `-> &mut T`) are not supported — this one is hidden behind a type \
                 alias, which the `#[dotnet_interface]` macro cannot see"
            );
        }
        let call_info = CallInfo::sig_from_instance_(*target, ctx);
        let fn_sig = call_info.sig().clone();
        let arg_names = vec![None; fn_sig.inputs().len()];
        let sig = ctx.alloc_sig(fn_sig);
        let target_name = function_name(ctx.tcx().symbol_name(*target));
        let target_name = ctx.alloc_string(target_name);
        let main_module = *ctx.main_module();
        let target_mref =
            MethodRef::new(main_module, target_name, sig, MethodKind::Static, [].into());
        let target_ref = ctx.alloc_methodref(target_mref);
        let mname = ctx.alloc_string(method_name.clone());
        // `Access::Extern` = DCE root, and the `AliasFor` edge keeps the lifted Rust fn alive —
        // same rationale as the class-virtual loop.
        let mdef = MethodDef::new(
            Access::Extern,
            class_idx,
            mname,
            sig,
            MethodKind::Virtual,
            MethodImpl::AliasFor(target_ref),
            arg_names,
        );
        ctx.new_method(mdef);
    }

    // Build the accumulated event halves (from BOTH loops above — class virtuals and interface
    // abstracts) into real `EventDef`s. Both halves are required; the macros make a lone half
    // unrepresentable, so a panic here means a hand-rolled entrypoint got it wrong.
    for (event_name, (add, remove, delegate_ty)) in pending_events {
        let add = add.unwrap_or_else(|| {
            panic!("comptime: event '{event_name}' has a remove_* method but no add_* — both halves are required")
        });
        let remove = remove.unwrap_or_else(|| {
            panic!("comptime: event '{event_name}' has an add_* method but no remove_* — both halves are required")
        });
        let delegate_ty = delegate_ty.expect("comptime: event delegate type unset (unreachable — set alongside add/remove)");
        let name = ctx.alloc_string(event_name);
        ctx.class_mut(class_idx)
            .add_event(cilly::class::EventDef::new(name, delegate_ty, add, remove));
    }

    // Each static method aliases an ordinary Rust fn, but unlike a virtual it has NO receiver — its
    // signature is the Rust fn's verbatim, and it is emitted as `MethodKind::Static` (so `MainModule`
    // — actually `<Class>` — exposes it as `static <Ret> FNAME(<inputs>)` and C# calls it as
    // `<Class>.FNAME(…)`).
    for (method_name, target) in &class.static_methods {
        let call_info = CallInfo::sig_from_instance_(*target, ctx);
        let fn_sig = call_info.sig().clone();
        let arg_names = vec![None; fn_sig.inputs().len()];
        let sig = ctx.alloc_sig(fn_sig);
        let target_name = function_name(ctx.tcx().symbol_name(*target));
        let target_name = ctx.alloc_string(target_name);
        let main_module = *ctx.main_module();
        let target_mref = MethodRef::new(main_module, target_name, sig, MethodKind::Static, [].into());
        let target_ref = ctx.alloc_methodref(target_mref);
        let mname = ctx.alloc_string(method_name.clone());
        let mdef = MethodDef::new(
            Access::Extern,
            class_idx,
            mname,
            sig,
            MethodKind::Static,
            MethodImpl::AliasFor(target_ref),
            arg_names,
        );
        ctx.new_method(mdef);
    }

    // Emit constructors for reference types so C# can `new` the class. Value types have no base
    // constructor to chain to and are created differently, so skip them. A class may request several
    // ctors (overloaded by arity): a field-initializing *primary* ctor `.ctor(field0, …)` and/or a
    // parameterless *default* ctor `.ctor()`. If neither is requested we still emit the parameterless
    // ctor (the historical default — so `new <Name>()` always works).
    if !class.is_value_type {
        if let Some(base) = extends {
            let self_ty = Type::ClassRef(*class_idx);
            // Reference to the base class's `.ctor` (e.g. System.Object::.ctor). Chaining to a base
            // constructor is a plain `call instance void …::.ctor()`, so this methodref is `Instance`
            // kind, NOT `Constructor` — the latter is for `newobj` and is rejected as a CIL-root call.
            // The base ctor's `this` param is typed as the DERIVED class (the actual `this` at the
            // call). The methodref still targets the base's `.ctor` (its `class` is `base`), so the IL
            // is `call instance void <base>::.ctor()`; typing the param as the derived type just lets
            // the inheritance-unaware CIL checker accept the `this` argument, which is a sound upcast
            // (a derived reference IS-A base reference).
            let base_ctor_sig = ctx.sig([self_ty], Type::Void);
            let base_ctor_name = ctx.alloc_string(".ctor");
            let base_ctor = ctx.alloc_methodref(MethodRef::new(
                base,
                base_ctor_name,
                base_ctor_sig,
                MethodKind::Instance,
                [].into(),
            ));

            // Field-initializing primary ctor: `.ctor(this, field0, field1, …)`:
            // `ldarg.0; call base::.ctor(); [ldarg.0; ldarg.{i+1}; stfld field_i;]* ret`.
            if class.has_primary_ctor {
                let mut inputs = vec![self_ty];
                inputs.extend(class.fields.iter().map(|(tpe, _)| *tpe));
                let n_inputs = inputs.len();
                let ctor_sig = ctx.sig(inputs, Type::Void);
                let this = ctx.alloc_node(CILNode::LdArg(0));
                let call_base = ctx.alloc_root(CILRoot::call(base_ctor, [this]));
                let mut roots = vec![call_base];
                for (idx, (tpe, fname)) in class.fields.iter().enumerate() {
                    let fname = ctx.alloc_string(fname.clone());
                    let fdesc = ctx.alloc_field(FieldDesc::new(*class_idx, fname, *tpe));
                    let obj = ctx.alloc_node(CILNode::LdArg(0));
                    let value = ctx.alloc_node(CILNode::LdArg(
                        u32::try_from(idx + 1).expect("comptime: too many ctor fields"),
                    ));
                    roots.push(ctx.alloc_root(CILRoot::SetField(Box::new((fdesc, obj, value)))));
                }
                roots.push(ctx.alloc_root(CILRoot::VoidRet));
                let ctor_name = ctx.alloc_string(".ctor");
                let ctor_def = MethodDef::new(
                    Access::Extern,
                    class_idx,
                    ctor_name,
                    ctor_sig,
                    MethodKind::Constructor,
                    MethodImpl::MethodBody {
                        blocks: vec![BasicBlock::new(roots, 0, None)],
                        locals: vec![],
                    },
                    vec![None; n_inputs],
                );
                ctx.new_method(ctor_def);
            }

            // Parameterless default ctor `.ctor(this)`: `ldarg.0; call base::.ctor(); ret`. Emitted
            // when explicitly requested, or implicitly whenever no primary ctor exists (so every
            // reference class has at least one ctor). When BOTH exist they overload by arity.
            if class.has_default_ctor || !class.has_primary_ctor {
                let ctor_sig = ctx.sig([self_ty], Type::Void);
                let this = ctx.alloc_node(CILNode::LdArg(0));
                let call_base = ctx.alloc_root(CILRoot::call(base_ctor, [this]));
                let ret = ctx.alloc_root(CILRoot::VoidRet);
                let ctor_name = ctx.alloc_string(".ctor");
                let ctor_def = MethodDef::new(
                    Access::Extern,
                    class_idx,
                    ctor_name,
                    ctor_sig,
                    MethodKind::Constructor,
                    MethodImpl::MethodBody {
                        blocks: vec![BasicBlock::new(vec![call_base, ret], 0, None)],
                        locals: vec![],
                    },
                    vec![None],
                );
                ctx.new_method(ctor_def);
            }

            // For a primary-ctor (record-like) class, also emit a public accessor per field —
            // `read_<field>(this) -> field_ty` = `ldarg.0; ldfld field; ret` — so a managed caller can
            // observe the ctor-initialized state (the fields themselves are private).
            if class.has_primary_ctor {
                for (tpe, fname) in &class.fields {
                    let fname_str = ctx.alloc_string(fname.clone());
                    let fdesc = ctx.alloc_field(FieldDesc::new(*class_idx, fname_str, *tpe));
                    let this = ctx.alloc_node(CILNode::LdArg(0));
                    let load = ctx.ld_field(this, fdesc);
                    let ret = ctx.alloc_root(CILRoot::Ret(load));
                    let getter_sig = ctx.sig([self_ty], *tpe);
                    let getter_name = ctx.alloc_string(format!("read_{fname}"));
                    let getter_def = MethodDef::new(
                        Access::Extern,
                        class_idx,
                        getter_name,
                        getter_sig,
                        MethodKind::Virtual,
                        MethodImpl::MethodBody {
                            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                            locals: vec![],
                        },
                        vec![None],
                    );
                    ctx.new_method(getter_def);
                }
            }

            // Field setters: `set_<field>(this, value)` = `ldarg.0; ldarg.1; stfld field; ret`, paired
            // with the `read_<field>` accessor so a managed caller can update the field state too.
            if class.has_field_setters {
                for (tpe, fname) in &class.fields {
                    let fname_str = ctx.alloc_string(fname.clone());
                    let fdesc = ctx.alloc_field(FieldDesc::new(*class_idx, fname_str, *tpe));
                    let obj = ctx.alloc_node(CILNode::LdArg(0));
                    let value = ctx.alloc_node(CILNode::LdArg(1));
                    let store = ctx.alloc_root(CILRoot::SetField(Box::new((fdesc, obj, value))));
                    let ret = ctx.alloc_root(CILRoot::VoidRet);
                    let setter_sig = ctx.sig([self_ty, *tpe], Type::Void);
                    let setter_name = ctx.alloc_string(format!("set_{fname}"));
                    let setter_def = MethodDef::new(
                        Access::Extern,
                        class_idx,
                        setter_name,
                        setter_sig,
                        MethodKind::Virtual,
                        MethodImpl::MethodBody {
                            blocks: vec![BasicBlock::new(vec![store, ret], 0, None)],
                            locals: vec![],
                        },
                        vec![None, None],
                    );
                    ctx.new_method(setter_def);
                }
            }
        }
    }
}
