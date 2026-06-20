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
use cilly::{Access, ClassDef, ClassRef, MethodDef, MethodImpl, MethodRef, Type};
use rustc_codegen_clr_call::CallInfo;
use rustc_codegen_clr_ctx::{function_name, MethodCompileCtx};
use rustc_codegen_clr_type::r#type::get_type;
use rustc_codegen_clr_type::utilis::garg_to_string;
use rustc_middle::mir::{Rvalue, StatementKind, TerminatorKind};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{Instance, TyKind, TypingEnv};

use crate::utilis::garag_to_bool;

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
                    let is_value_type = garag_to_bool(subst_ref[1], ctx.tcx());
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
    let def = ClassDef::new(
        name,
        class.is_value_type,
        0,
        extends,
        fields,
        vec![],
        Access::Public,
        None,
        None,
        true,
    );
    let class_idx = ctx
        .class_def(def)
        .expect("comptime: layout error registering interop class");

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
}
