use crate::{
    branch_cond_to_name,
    utilis::{assert_unique, encode},
    MethodImpl,
};

use std::{io::Write, path::Path};

use super::{
    asm::{IlasmFlavour, ILASM_FLAVOUR, ILASM_PATH},
    bimap::Interned,
    cilnode::{ExtendKind, UnOp},
    cilroot::BranchCond,
    class::StaticFieldDef,
    method::LocalDef,
    tpe::simd::SIMDElem,
    Assembly, BinOp, CILIter, CILIterElem, CILNode, CILRoot, ClassRef, Const, Exporter, FnSig, Int,
    MethodDefIdx, Type,
};

mod partition;

pub struct ILExporter {
    flavour: IlasmFlavour,
    is_lib: bool,
    /// The .NET assembly name to emit in the `.assembly` directive. `None` keeps the legacy `_`
    /// placeholder (used for executables, where the assembly is loaded by file path via the native
    /// launcher and the name is irrelevant). A library passes its crate name here so C# can reference
    /// the produced `.dll` by a real assembly identity.
    asm_name: Option<String>,
    /// Monotonic counter handing out unique `tr_done_N` leave-target labels for each emitted
    /// `TerminateRegion` inner protected region. The interned root index is NOT usable as the label:
    /// a single interned `protected` root can be referenced from several block roots within one
    /// method (CSE/realloc share identical roots), and IL labels must be unique within a method —
    /// reusing the index produced `Duplicate label: 'tr_done_N'` ilasm errors. A fresh counter value
    /// per emission guarantees uniqueness even when the same root is emitted multiple times.
    terminate_region_label: std::cell::Cell<u64>,
    /// When `MainModule` exceeds CoreCLR's per-type method cap, holds the split of its methods across
    /// per-module classes (rebuilt per export in [`Self::export_to_write`]); `None` for normal-size
    /// assemblies, where `MainModule` is emitted as a single class exactly as before. Read at
    /// method-reference emission to redirect a `MainModule::m` call to the class `m` was emitted in.
    partition: std::cell::RefCell<Option<partition::ModulePartition>>,
}
impl ILExporter {
    #[must_use]
    pub fn new(flavour: IlasmFlavour, is_lib: bool, asm_name: Option<String>) -> Self {
        Self {
            flavour,
            is_lib,
            asm_name,
            terminate_region_label: std::cell::Cell::new(0),
            partition: std::cell::RefCell::new(None),
        }
    }

    fn export_to_write(&self, asm: &super::Assembly, out: &mut impl Write) -> std::io::Result<()> {
        let asm_mut = &mut asm.clone();
        match &self.asm_name {
            // A named assembly so C# can reference it by identity (quoted to allow any crate name).
            Some(name) => writeln!(out, ".assembly '{name}'{{}}")?,
            // Legacy placeholder for executables (loaded by path, name irrelevant).
            None => writeln!(out, ".assembly _{{}}")?,
        }
        // For a LIBRARY, emit `.assembly extern` headers with real BCL identities. Without these,
        // ilasm infers extern refs from the `[asm]Type` uses in the body and defaults them to version
        // 0.0.0.0 / null token, which a C# *compiler* rejects when the library is referenced directly
        // (CS0012). net8 ref assemblies are 8:0:0:0 with the ECMA token; CoreLib and mscorlib differ.
        // Executables are run directly (the runtime resolves refs leniently) and need no headers — so
        // they keep ilasm's defaults, leaving the `::stable` exe suite untouched.
        if self.is_lib {
            // The BCL `.ver` triplet tracks the target .NET version (8:0:0:0 / 9:0:0:0); the
            // public-key tokens are version-INVARIANT (verified identical on 8 and 9), and mscorlib
            // keeps its legacy 4:0:0:0. Single source: `DotnetVersion::assembly_ver`.
            let dv_ver = crate::ir::dotnet_version().assembly_ver();
            // Normalize each extern name to its public REFERENCE assembly (CoreLib/mscorlib ->
            // System.Runtime) BEFORE stamping, and de-dup so a CoreLib entry collapses into the
            // single `System.Runtime` extern instead of emitting both a phantom CoreLib header and
            // System.Runtime. Only the C#-visible METADATA is normalized (this extern table + the
            // base-type `extends` clause via `simple_class_ref`); method-body instruction operands
            // keep the impl-assembly name — `call instance [System.Runtime]System.String::m` is
            // "Bad IL format" on a real CoreLib String (see mycorrhiza/src/system/mod.rs), and a
            // C# compiler never reads method bodies anyway.
            let raw = asm.external_assembly_names();
            let mut externs: Vec<&str> = raw.iter().map(|e| ref_assembly_name(e)).collect();
            externs.sort();
            externs.dedup();
            for ext in externs {
                if is_bcl_assembly(ext) {
                    // CoreLib/mscorlib are now normalized to System.Runtime, so they fall through to
                    // `_`. All BCL assemblies share the ECMA public-key token and the runtime `.ver`.
                    let (ver, token) = (dv_ver, "B0 3F 5F 7F 11 D5 0A 3A");
                    writeln!(
                        out,
                        ".assembly extern '{ext}' {{ .ver {ver} .publickeytoken = ({token}) }}"
                    )?;
                } else {
                    // A NON-BCL assembly — a consumer's own C# library (e.g. an interface/contracts
                    // assembly a Rust type implements). A plain `dotnet build` produces it as
                    // version 1.0.0.0 with NO strong-name token, so stamping the BCL ver+token here
                    // makes the reference fail to bind (`CS0012`: the type is in an assembly that is
                    // not referenced, with a mismatched identity). Emit a simple-name reference so it
                    // resolves against whatever `Name.dll` the app probes at build/run time.
                    writeln!(out, ".assembly extern '{ext}' {{ }}")?;
                }
            }
        }
        // Const-data blobs live in FieldRVA statics (`.field static <T> c_X at I_X` + a `.data` blob).
        // The field's declared type must be SIZED to the blob: the JIT loads the whole contiguous
        // `.data` section so reading N bytes from `&c_X` works regardless of the field type, but
        // NativeAOT/ILC preserves only `sizeof(<T>)` bytes of FieldRVA data and zeros the rest — so a
        // `uint8` field over an N-byte blob silently becomes "first byte, then zeros" under AOT (this
        // is what broke every `format!`/`&str`-literal/`DEC_DIGITS_LUT`/const-`&[T]` under AOT). Emit
        // a value-type sized to each distinct blob length (the Roslyn `__StaticArrayInitTypeSize`
        // idiom) and type the field with it, so ILC keeps the full blob. Consumers take `&c_X`
        // (`ldsflda`), so the field type is otherwise transparent.
        let mut blob_sizes: Vec<usize> = asm.const_data.1.keys().map(|d| d.len().max(1)).collect();
        blob_sizes.sort_unstable();
        blob_sizes.dedup();
        for n in &blob_sizes {
            writeln!(out, ".class private explicit ansi sealed '__rcl_const_blob_{n}' extends [System.Runtime]System.ValueType {{ .pack 1 .size {n} }}")?;
        }
        for (const_data, idx) in asm.const_data.1.iter() {
            let encoded = encode(idx.inner() as u64);
            let n = const_data.len().max(1);
            let data: String = const_data.iter().map(|u| format!("{u:x} ")).collect();
            writeln!(out, " .data cil I_{encoded} = bytearray ({data})\n.field assembly static valuetype '__rcl_const_blob_{n}' c_{encoded} at I_{encoded}")?;
        }
        let mut c = 0;
        // If `MainModule` is too large for a single .NET type (CoreCLR caps a type at ~65k methods),
        // split its methods across per-module partition classes. A no-op for normal-size assemblies
        // (returns `None`), so ordinary builds and the `::stable` suite are byte-for-byte unchanged.
        {
            let main_module_methods: Vec<MethodDefIdx> = asm
                .iter_class_defs()
                .find(|cd| asm[cd.name()] == *super::asm::MAIN_MODULE)
                .map_or_else(Vec::new, |cd| cd.methods().to_vec());
            *self.partition.borrow_mut() = partition::build(asm, &main_module_methods);
        }
        // Iterate trough all types
        for class_def in asm.iter_class_defs() {
            let vis = match class_def.access() {
                crate::Access::Extern | crate::Access::Public => "public",
                crate::Access::Private => "private",
            };
            let sealed = if class_def.is_valuetype() {
                "sealed"
            } else {
                ""
            };
            let extends = if let Some(parrent) = class_def.extends() {
                simple_class_ref(parrent, asm)
            } else if class_def.is_valuetype() {
                "[System.Runtime]System.ValueType".into()
            } else {
                "[System.Runtime]System.Object".into()
            };
            let explicit = if class_def.has_explicit_layout() {
                "explicit"
            } else {
                "auto"
            };
            // Shorten over-long monomorphized names so the stricter CoreCLR `ilasm` (native
            // macOS/Windows) accepts them; within-limit names pass through unchanged (Linux/Docker
            // + ::stable unaffected). The matching reference sites apply the identical transform.
            // `implements I1, I2, …` clause for Rust-defined managed classes that implement a managed
            // interface. Empty (no clause) for the overwhelming majority of classes.
            let implements = if class_def.implements().is_empty() {
                String::new()
            } else {
                let list: String = class_def
                    .implements()
                    .iter()
                    .map(|iface| simple_class_ref(*iface, asm))
                    .intersperse(", ".to_string())
                    .collect();
                format!(" implements {list}")
            };
            let name = dotnet_class_name(&asm[class_def.name()]);
            // When `MainModule` is split across partition classes, its static fields are read by
            // methods that now live in sibling classes — widen them to `public` so the cross-class
            // `ldsfld`/`stsfld` is legal (default field accessibility is `private`).
            let main_partitioned =
                asm[class_def.name()] == *super::asm::MAIN_MODULE && self.partition.borrow().is_some();
            let field_vis = if main_partitioned { "public " } else { "" };
            // A genuine ECMA-335 interface `TypeDef` (§II.10.1.3) must NOT have an `extends`
            // clause at all — even the implicit `[System.Runtime]System.Object` this branch would
            // otherwise emit is illegal for `Interface`-flagged types and CoreCLR rejects it at
            // load time. See `ClassDef::with_interface`'s doc for the exact scope this covers.
            if class_def.is_interface() {
                // An interface `TypeDef` cannot carry instance fields (§II.10.1.3) — nothing
                // upstream enforces this (`ClassDef::with_interface` is a bare flag, see its doc),
                // so guard it here instead of emitting invalid IL that only CoreCLR would reject
                // at load time.
                assert!(
                    class_def.fields().is_empty(),
                    "interface '{name}' (ClassDef::with_interface) has instance fields, which \
                     ECMA-335 forbids on an interface TypeDef."
                );
                writeln!(
                    out,
                    ".class {vis} interface abstract ansi '{name}'{implements}{{"
                )?;
            } else {
                writeln!(
                    out,
                    ".class {vis} ansi {sealed} {explicit} '{name}' extends {extends}{implements}{{"
                )?;
            }
            // Export size
            if let Some(size) = class_def.explict_size() {
                writeln!(out, ".size {size}", size = size.get())?;
            }
            if let Some(align) = class_def.align() {
                writeln!(out, "//align {align}", align = align.get())?;
            }
            // Export all fields
            for (tpe, name, offset) in class_def.fields() {
                let name = &asm[*name];
                let tpe = non_void_type_il(tpe, asm);
                if let Some(offset) = offset {
                    writeln!(out, ".field [{offset}] {tpe} '{name}'")
                } else {
                    writeln!(out, ".field {tpe} '{name}'")
                }?;
            }
            assert_unique(
                class_def.static_fields(),
                format!(
                    "The class {} contains a duplicate static field",
                    &asm[class_def.name()]
                ),
            );
            // Export all static fields
            for StaticFieldDef {
                tpe,
                name,
                is_tls,
                is_const,
                default_value,
            } in class_def.static_fields()
            {
                let name = &asm[*name];
                let tpe = non_void_type_il(tpe, asm);
                let is_const = if *is_const { "initonly" } else { "" };
                let default_value = if let Some(default_value) = default_value {
                    match default_value {
                        Const::Bool(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int8({})", *b as u8)?;
                            format!(" at C_{c}")
                        }
                        Const::U64(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int64({})", *b)?;
                            format!(" at C_{c}")
                        }
                        Const::U32(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int32({})", *b)?;
                            format!(" at C_{c}")
                        }
                        Const::U16(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int16({})", *b)?;
                            format!(" at C_{c}")
                        }
                        Const::U8(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int8({})", *b)?;
                            format!(" at C_{c}")
                        }
                        Const::U128(b) => {
                            c += 1;
                            writeln!(
                                out,
                                ".data cil C_{c} = bytearray({})",
                                // ILAsm `bytearray(...)` expects HEX byte pairs — match the I128
                                // arm below. (Was decimal `{v}`, a latent wrong-bytes bug for any
                                // >u64 unsigned static-field default; no producer drives it today.)
                                b.to_le_bytes()
                                    .iter()
                                    .map(|v| format!(" {v:02x}"))
                                    .collect::<String>()
                            )?;
                            format!(" at C_{c}")
                        }
                        // Signed ints share the byte-slot encoding of their unsigned siblings;
                        // emit via the matching width directive (ILAsm `intN(...)` accepts signed
                        // decimals). Latent until a future interop const-field producer drives it.
                        Const::I8(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int8({b})")?;
                            format!(" at C_{c}")
                        }
                        Const::I16(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int16({b})")?;
                            format!(" at C_{c}")
                        }
                        Const::I32(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int32({b})")?;
                            format!(" at C_{c}")
                        }
                        Const::I64(b) | Const::ISize(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int64({b})")?;
                            format!(" at C_{c}")
                        }
                        Const::USize(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = int64({b})")?;
                            format!(" at C_{c}")
                        }
                        Const::I128(b) => {
                            c += 1;
                            writeln!(
                                out,
                                ".data cil C_{c} = bytearray({})",
                                b.to_le_bytes()
                                    .iter()
                                    .map(|v| format!(" {v:02x}"))
                                    .collect::<String>()
                            )?;
                            format!(" at C_{c}")
                        }
                        Const::F32(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = float32({})", **b)?;
                            format!(" at C_{c}")
                        }
                        Const::F64(b) => {
                            c += 1;
                            writeln!(out, ".data cil C_{c} = float64({})", **b)?;
                            format!(" at C_{c}")
                        }
                        // Non-static-field-shaped variants (PlatformString/Null/ByteBuffer) have
                        // no `.data` rendering; refuse loudly per the project's clean-wall idiom.
                        other => panic!(
                            "static-field default value of kind {other:?} is unsupported on the .NET target"
                        ),
                    }
                } else {
                    "".into()
                };
                writeln!(
                    out,
                    ".field {field_vis}static {is_const} {tpe} '{name}'{default_value}"
                )?;
                if *is_tls {
                    writeln!(out,".custom instance void [System.Runtime]System.ThreadStaticAttribute::.ctor() = (01 00 00 00)")?;
                };
            }
            // Debug check
            let mut ensure_unqiue: std::collections::HashSet<MethodDefIdx> =
                std::collections::HashSet::new();
            // Export all methods

            // Export all methods. When `MainModule` overflows a single .NET type, split its
            // methods across per-module partition classes (see `partition`); otherwise emit the one
            // class as before. Assignment keys on the method NAME, matching the reference redirect.
            if &asm[class_def.name()] == super::asm::MAIN_MODULE && self.partition.borrow().is_some() {
                let (residual, extras): (Vec<MethodDefIdx>, Vec<(String, Vec<MethodDefIdx>)>) = {
                    let guard = self.partition.borrow();
                    let part = guard.as_ref().unwrap();
                    (
                        part.residual_methods().to_vec(),
                        part.extra_classes()
                            .map(|(c, m)| (c.to_string(), m.to_vec()))
                            .collect(),
                    )
                };
                for method_id in &residual {
                    self.emit_one_method(asm, asm_mut, out, *method_id, &mut ensure_unqiue)?;
                }
                writeln!(out, "}}")?; // close the `MainModule` residual class
                for (cname, method_ids) in &extras {
                    writeln!(
                        out,
                        ".class public ansi auto '{cname}' extends [System.Runtime]System.Object{{"
                    )?;
                    for method_id in method_ids {
                        self.emit_one_method(asm, asm_mut, out, *method_id, &mut ensure_unqiue)?;
                    }
                    writeln!(out, "}}")?;
                }
                continue;
            }
            for method_id in class_def.methods() {
                self.emit_one_method(asm, asm_mut, out, *method_id, &mut ensure_unqiue)?;
            }
            // `.event`/`.addon`/`.removeon` (ECMA-335 §II.22.13/§II.15.4.3): the `add_`/`remove_`
            // bodies are ordinary instance methods (already emitted above by the per-method loop —
            // an `EventDef` only links their names into an Event/MethodSemantics-shaped IL block,
            // it doesn't introduce new invocation semantics). `ilasm` computes the real
            // EventMap/Event/MethodSemantics metadata rows itself from this text; the hand-rolled
            // PE writer has no equivalent yet (see `ClassDef::add_event`'s doc for scope).
            for ev in class_def.events() {
                let ev_name = asm_mut[ev.name()].to_string();
                let delegate_ty = non_void_type_il(&ev.delegate(), asm_mut);
                let add_text = self.method_ref_operand_text(asm_mut, ev.add());
                let remove_text = self.method_ref_operand_text(asm_mut, ev.remove());
                writeln!(
                    out,
                    ".event {delegate_ty} '{ev_name}' {{ .addon {add_text} .removeon {remove_text} }}"
                )?;
            }
            writeln!(out, "}}")?;
        }

        Ok(())
    }
    /// Builds the `<class>::'<name>'(<params>)` text ECMA-335 uses to reference a method by full
    /// signature (as opposed to `.override`'s bare `<class>::<name>`) — the shape `.addon`/
    /// `.removeon` need. Factored out of `export_node`'s `CILNode::Call` arm, which builds the
    /// identical text for a `call`/`callvirt` operand (just prefixed with the call opcode there).
    fn method_ref_operand_text(
        &self,
        asm: &mut super::Assembly,
        mref: Interned<super::MethodRef>,
    ) -> String {
        let mref = &asm[mref];
        let sig = &asm[mref.sig()];
        let output = type_il(sig.output(), asm);
        let inputs = match mref.kind() {
            crate::cilnode::MethodKind::Static => sig.inputs(),
            crate::cilnode::MethodKind::Instance
            | crate::cilnode::MethodKind::Virtual
            | crate::cilnode::MethodKind::Constructor => &sig.inputs()[1..],
        };
        let inputs: String = inputs
            .iter()
            .map(|tpe| non_void_type_il(tpe, asm))
            .intersperse(",".to_owned())
            .collect();
        let name = &asm[mref.name()];
        let class = self.partitioned_class(mref.class(), mref.name(), asm);
        format!("instance {output} {class}::'{name}'({inputs})")
    }
    /// Emit one `.method … { … }` definition. Factored out of the class loop so an over-large
    /// `MainModule` can spread its methods across several partition classes (see [`partition`]).
    fn emit_one_method(
        &self,
        asm: &super::Assembly,
        asm_mut: &mut super::Assembly,
        out: &mut impl Write,
        method_id: MethodDefIdx,
        ensure_unqiue: &mut std::collections::HashSet<MethodDefIdx>,
    ) -> std::io::Result<()> {
        let method = asm.method_def(method_id);
        // Under an active `MainModule` partition its methods call one another across class
        // boundaries, so they must be cross-class accessible — force `public`. `class_of` is `Some`
        // for every `MainModule` method (residual ones included); a same-named data-type method
        // being widened too is harmless. Non-partitioned builds keep the source visibility.
        let force_public = self
            .partition
            .borrow()
            .as_ref()
            .is_some_and(|p| p.class_of(method.name()).is_some());
        let vis = if force_public {
            "public"
        } else {
            match method.access() {
                crate::Access::Extern | crate::Access::Public => "public",
                crate::Access::Private => "private",
            }
        };
        let kind = match method.kind() {
            crate::cilnode::MethodKind::Static => "static",
            crate::cilnode::MethodKind::Instance => "instance",
            // An interface member (`MethodDef::is_abstract`) needs the `newslot abstract` flags
            // ahead of `virtual instance` — `newslot` because an interface member never overrides
            // an existing vtable slot, `abstract` because it has no body (RVA=0, §II.15.4.2.2).
            crate::cilnode::MethodKind::Virtual if method.is_abstract() => {
                "newslot abstract virtual instance"
            }
            crate::cilnode::MethodKind::Virtual => "virtual instance",
            // A constructor is an instance method (the `instance` calling-convention keyword
            // must come LAST, right before the return type — like `virtual instance` above —
            // not interspersed with the `specialname`/`rtspecialname` attributes).
            crate::cilnode::MethodKind::Constructor => "specialname rtspecialname instance",
        };
        let pinvoke = if let MethodImpl::Extern {
            lib,
            preserve_errno,
        } = method.implementation()
        {
            let lib = &asm[*lib];
            if *preserve_errno {
                format!("pinvokeimpl(\"{lib}\" cdecl lasterr)")
            } else {
                format!("pinvokeimpl(\"{lib}\" cdecl)")
            }
        } else {
            String::new()
        };
        let name = &asm[method.name()];
        let sig = &asm[method.sig()];
        // `_signature` variants: this `.method` header line is the ONE place a method's own
        // return/parameter types are declared — C#-visible metadata a separately-compiled consumer
        // resolves a call against, exactly like the `extends`/`.assembly extern` cases
        // `ref_assembly_name` already covers (see that fn's doc). Every other `type_il`/
        // `non_void_type_il` call site in this file (body instructions, calli signatures, field
        // declarations, locals) stays on the plain impl-assembly-qualified path: those are either
        // invisible to a C# compiler (bodies are never read) or would be genuinely rejected by the
        // JIT if impl-assembly types like `System.String`/`System.Object` were re-qualified there
        // (see `class_ref`'s doc comment on why body positions must keep the impl-assembly name).
        let ret = type_il_signature(sig.output(), asm);
        assert_eq!(method.arg_names().len(), sig.inputs().len(), "{name:?}");
        let inputs = match method.kind() {
            crate::cilnode::MethodKind::Static => sig.inputs(),
            crate::cilnode::MethodKind::Instance
            | crate::cilnode::MethodKind::Virtual
            | crate::cilnode::MethodKind::Constructor => &sig.inputs()[1..],
        };

        let inputs: String = inputs
            .iter()
            .zip(method.arg_names())
            .map(|(tpe, name)| match name {
                Some(name) => {
                    format!("{} '{}'", non_void_type_il_signature(tpe, asm_mut), &asm_mut[*name])
                }
                None => non_void_type_il_signature(tpe, asm_mut),
            })
            .intersperse(",".to_string())
            .collect();
        let preservesig = if method.implementation().is_extern() {
            "preservesig"
        } else {
            ""
        };
        // Layer 3 (help RyuJIT): hint the JIT to inline small, straight-line leaf methods — the
        // monomorphized closure bodies / iterator-adapter `next`/`fold` wrappers that Rust's
        // zero-cost abstractions lower to, plus small branchy-but-call-free leaves like the
        // saturating float->int cast helpers. RyuJIT won't inline across these by default
        // (struct-by-value returns + its size heuristic), so the per-element call survives;
        // `aggressiveinlining` (MethodImplOptions.AggressiveInlining) tells it to. Heuristic shared
        // with `pe_exporter` via `MethodImpl::should_hint_aggressive_inline` (see that method's doc)
        // so the two exporters can't drift out of parity on this again. Pure JIT hint — cannot
        // affect correctness. `PDB_FRAMES=1` suppresses only this hint so debug/PDB runs can keep
        // user frames visible in managed stack traces; default-off preserves current RyuJIT
        // behaviour.
        let aggrinline = if !*crate::PDB_FRAMES
            && method.implementation().should_hint_aggressive_inline(asm_mut)
        {
            "aggressiveinlining "
        } else {
            ""
        };
        writeln!(
            out,
            ".method {vis} hidebysig {kind} {pinvoke} {ret} '{name}'({inputs}) cil managed {aggrinline}{preservesig}{{// Method ID {method_id:?}"
        )?;
        // Explicit ECMA-335 `.override` (§II.15.4.2.3) for a base-class virtual override (see
        // `MethodDef::with_override`'s doc — distinct from ordinary `implements=` interface
        // satisfaction, which binds implicitly by name+signature with no `.override` at all).
        // Unlike a `call`/`callvirt` operand, `.override`'s target is named WITHOUT a return
        // type or parameter list — just `<class>::<name>` — matching real ilasm output for
        // explicit interface/base-class overrides.
        if let Some(base) = method.overrides() {
            // An abstract member has no body a `.override`'s MethodImpl row could attach to
            // (§II.22.27 requires the overriding method to have a real implementation) — neither
            // `with_override` nor `with_abstract` validates against the other being set (see both
            // docs), so guard the combination here instead of emitting self-contradictory IL.
            assert!(
                !method.is_abstract(),
                "method '{name}' is both abstract (MethodDef::with_abstract) and has an explicit \
                 .override (MethodDef::with_override) -- an abstract member has no body for a \
                 MethodImpl row to attach to."
            );
            let base_ref = &asm_mut[base];
            let base_class = self.partitioned_class(base_ref.class(), base_ref.name(), asm_mut);
            let base_name = &asm_mut[base_ref.name()];
            writeln!(out, ".override {base_class}::'{base_name}'")?;
        }
        debug_assert!(ensure_unqiue.insert(method_id));
        // An abstract member (`MethodDef::is_abstract`, e.g. a synthesized interface method) has
        // RVA=0 and NO body at all (§II.15.4.2.2) — not even `.maxstack`/`.entrypoint`, which are
        // body-only directives ilasm rejects on an abstract method. `implementation()` is an
        // unused `MethodImpl::Missing` placeholder for these (see that field's doc), so it must
        // never be read here.
        if method.is_abstract() {
            writeln!(out, "}}")?;
            return Ok(());
        }
        let stack_size = match method.resolved_implementation(asm_mut) {
            MethodImpl::MethodBody { blocks, .. } => blocks
                .iter()
                .flat_map(|block| block.roots().iter())
                .map(|root| {
                    crate::CILIter::new(asm_mut.get_root(*root).clone(), asm_mut).count()
                        + 10
                })
                .max()
                .unwrap_or(0),
            MethodImpl::Extern { .. } => 0,
            MethodImpl::AliasFor(_) => todo!(),
            MethodImpl::Missing => 3,
        };

        writeln!(out, ".maxstack {stack_size}")?;

        if *name == *"entrypoint" {
            writeln!(out, ".entrypoint")?;
        }
        // Export the implementation
        let mimpl = method.resolved_implementation(asm_mut).clone();
        self.export_method_imp(asm_mut, out, &mimpl, name, method.sig())?;
        writeln!(out, "}}")?;

        Ok(())
    }
    /// Resolve the IL class token for a method reference, honoring an active `MainModule` partition:
    /// a reference to a `MainModule` method is redirected to the per-module class its NAME was emitted
    /// in (see [`partition`]). Def + ref key on the same name, so they always agree.
    fn partitioned_class(
        &self,
        class: Interned<ClassRef>,
        name: Interned<crate::IString>,
        asm: &super::Assembly,
    ) -> String {
        if let Some(part) = self.partition.borrow().as_ref() {
            if asm[asm.class_ref(class).name()] == *super::asm::MAIN_MODULE {
                if let Some(cls) = part.class_of(name) {
                    if cls != super::asm::MAIN_MODULE {
                        return format!("class '{cls}'");
                    }
                }
            }
        }
        class_ref(class, asm)
    }
    fn export_method_imp(
        &self,
        asm: &mut super::Assembly,
        out: &mut impl Write,
        mimpl: &MethodImpl,
        name: &str,
        sig: Interned<FnSig>,
    ) -> std::io::Result<()> {
        match  mimpl{
            MethodImpl::MethodBody { blocks, locals } => {
                let locals_string:String = locals.iter().map(|(name,tpe)|match name {
                    Some(name) => {
                        format!("\n  {} '{}'", non_void_type_il(&asm[*tpe], asm), &asm[*name])
                    }
                    None => format!("\n  {}",non_void_type_il(&asm[*tpe], asm)),
                }).intersperse(",".to_owned()).collect();
                writeln!(out," .locals ({locals_string})")?;
                let mut blocks_iter = blocks.iter().peekable();
                //let mut is_in_multiblock_handler = false;
                while let Some(block) = blocks_iter.next(){
                    if block.handler().is_some() { //&& !is_in_multiblock_handler
                        writeln!(out,".try{{")?;
                    }
                    //DEBUG REMOVE THIS
                    writeln!(out,"// targets:{}",block.targets(asm).count())?;
                    writeln!(out," bb{}:",block.block_id())?;
                    for root in block.roots(){
                        self.export_root(asm,out,*root,false, block.handler().is_some(),sig,locals)?;
                    }
                    if let Some(handler) = block.handler(){
                        if Some(handler) == blocks_iter.peek().and_then(|block|block.handler()){
                            eprintln!("Multiblock handler candiate");
                        }
                        writeln!(out,"}} catch [System.Runtime]System.Object{{")?;
                        // Check for the GetException intrinsic. If it is not used, put a pop here.
                        if !handler.iter().flat_map(super::basic_block::BasicBlock::roots).flat_map(|root|CILIter::new(asm.get_root(*root).clone(),asm)).any(|elem|matches!(elem,CILIterElem::Node(CILNode::GetException))){
                            writeln!(out,"pop")?;
                        }
                        for hblock in handler{
                            writeln!(out," h{}_{}:",block.block_id(),hblock.block_id())?;
                            for root in hblock.roots(){
                                self.export_root(asm,out,*root,true,false,sig,locals)?;
                            }
                        }
                        writeln!(out,"}}")?;
                    }
                }
            }
            MethodImpl::Extern { .. } => (),
            MethodImpl::AliasFor(_) => {
                panic!("resolved_implementation returned `AliasFor`")
            }
            MethodImpl::Missing =>writeln!(out,"ldstr \"missing methiod {name}\"\n newobj void [System.Runtime] System.Exception::.ctor(string)\n throw")?,
        };
        Ok(())
    }
    #[allow(clippy::only_used_in_recursion)] // Futrue proffing. The IL exporter will need this in the future.
    fn export_node(
        &self,
        asm: &mut super::Assembly,
        out: &mut impl Write,
        node: Interned<CILNode>,
        sig: Interned<FnSig>,
        locals: &[LocalDef],
    ) -> std::io::Result<()> {
        let node = asm.get_node(node).clone();
        match node {
            CILNode::Const(cst) => match cst.as_ref() {
                super::Const::ByteBuffer { data, tpe:_ }=>{
                    // Must match the FieldRVA field's declared type (a blob-sized value-type, so ILC
                    // preserves the full blob under AOT — see the const_data emission above).
                    let n = asm.const_data[*data].len().max(1);
                    writeln!(out,"ldsflda valuetype '__rcl_const_blob_{n}' c_{}", encode(data.inner() as u64))
                }
                super::Const::Null(_) => writeln!(out, "ldnull"),
                super::Const::I8(val) => match val {
                    -1 => writeln!(out, "ldc.i4.m1"),
                    0..=8 => writeln!(out, "ldc.i4.{val}"),
                    _ => writeln!(out, "ldc.i4.s {val}"),
                },
                super::Const::I16(val) => match val {
                    -1 => writeln!(out, "ldc.i4.m1"),
                    0..=8 => writeln!(out, "ldc.i4.{val}"),
                    9..=127 => writeln!(out, "ldc.i4.s {val}"),
                    _ => writeln!(out, "ldc.i4 {val}"),
                },
                super::Const::I32(val) => match val {
                    -1 => writeln!(out, "ldc.i4.m1"),
                    0..=8 => writeln!(out, "ldc.i4.{val}"),
                    9..=127 => writeln!(out, "ldc.i4.s {val}"),
                    _ => writeln!(out, "ldc.i4 {val}"),
                },
                super::Const::I64(val) => match val {
                    -1 => writeln!(out, "ldc.i4.m1 conv.i8"),
                    0..=8 => writeln!(out, "ldc.i4.{val} conv.i8"),
                    9..=127 => writeln!(out, "ldc.i4.s {val} conv.i8"),
                    -2_147_483_648i64..0 | 128..=2_147_483_647i64 => {
                        writeln!(out, "ldc.i4 {val} conv.i8")
                    }
                    _ => writeln!(out, "ldc.i8 {val}"),
                },
                super::Const::I128(val) => match val {
                    -1 => writeln!(out, "ldc.i4.m1 call valuetype [System.Runtime]System.Int128 [System.Runtime]System.Int128::op_Implicit(int32)"),
                    0..=8 => writeln!(out, "ldc.i4.{val} call valuetype [System.Runtime]System.Int128 [System.Runtime]System.Int128::op_Implicit(int32)"),
                    9..=127 => writeln!(out, "ldc.i4.s {val} call valuetype [System.Runtime]System.Int128 [System.Runtime]System.Int128::op_Implicit(int32)"),
                    -2_147_483_648i128..0 | 128..=2_147_483_647i128 => {
                        writeln!(out, "ldc.i4 {val} call valuetype [System.Runtime]System.Int128 [System.Runtime]System.Int128::op_Implicit(int32)")
                    }
                    -9_223_372_036_854_775_808_i128..-2_147_483_648i128 | 2_147_483_648i128..=9_223_372_036_854_775_807i128 => {
                        writeln!(out, "ldc.i8 {val} call valuetype [System.Runtime]System.Int128 [System.Runtime]System.Int128::op_Implicit(int64)")
                    }
                    _ => {
                        let low =  u64::try_from((*val as u128) & u128::from(u64::MAX)).expect("trucating cast error");
                        let high = ((*val as u128) >> 64) as u64;
                        writeln!(out, "ldc.i8 {high} ldc.i8 {low} newobj instance void valuetype [System.Runtime]System.Int128::.ctor(uint64,uint64)")
                    },
                },
                super::Const::ISize(val) => match val {
                    -1 => writeln!(out, "ldc.i4.m1 conv.i"),
                    0..=8 => writeln!(out, "ldc.i4.{val} conv.i"),
                    9..=127 => writeln!(out, "ldc.i4.s {val} conv.i"),
                    -2_147_483_648i64..0 | 128..=2_147_483_647i64 => {
                        writeln!(out, "ldc.i4 {val} conv.i")
                    }
                    _ => writeln!(out, "ldc.i8 {val} conv.i"),
                },
                super::Const::U8(val) => match val {
                    0..=8 => writeln!(out, "ldc.i4.{val}"),
                    9..=127 => writeln!(out, "ldc.i4.s {val}"),
                    _ => writeln!(out, "ldc.i4 {val}"),
                },
                super::Const::U16(val) => match val {
                    0..=8 => writeln!(out, "ldc.i4.{val}"),
                    9..=127 => writeln!(out, "ldc.i4.s {val}"),
                    _ => writeln!(out, "ldc.i4 {val}"),
                },
                super::Const::U32(val) => match val {
                    0..=8 => writeln!(out, "ldc.i4.{val}"),
                    9..=127 => writeln!(out, "ldc.i4.s {val}"),
                    _ => writeln!(out, "ldc.i4 {val}"),
                },
                super::Const::U64(val) => match val {
                    0..=8 => writeln!(out, "ldc.i4.{val} conv.u8"),
                    9..=127 => writeln!(out, "ldc.i4.s {val} conv.u8"),
                    128..=4_294_967_295u64 => writeln!(out, "ldc.i4 {val} conv.u8"),
                    _ => writeln!(out, "ldc.i8 {val}"),
                },
                super::Const::USize(val) => match val {
                    0..=8 => writeln!(out, "ldc.i4.{val} conv.u"),
                    9..=127 => writeln!(out, "ldc.i4.s {val} conv.u"),
                    128..=2_147_483_647u64 => writeln!(out, "ldc.i4 {val} conv.u"),
                    _ => writeln!(out, "ldc.i8 {val} conv.u"),
                },
                super::Const::U128(val)=>match val {
                    0..=8 => writeln!(out, "ldc.i4.{val} call valuetype [System.Runtime]System.UInt128 [System.Runtime]System.UInt128::op_Implicit(uint32)"),
                    9..=127 => writeln!(out, "ldc.i4.s {val} call valuetype [System.Runtime]System.UInt128 [System.Runtime]System.UInt128::op_Implicit(uint32)"),
                    128..=4_294_967_295u128 => writeln!(out, "ldc.i4 {val} call valuetype [System.Runtime]System.UInt128 [System.Runtime]System.UInt128::op_Implicit(uint32)"),
                    4_294_967_296u128..=18_446_744_073_709_551_615u128 => writeln!(out, "ldc.i8 {val} call valuetype [System.Runtime]System.UInt128 [System.Runtime]System.UInt128::op_Implicit(uint64)"),
                    _ => {
                        let low =  u64::try_from({ *val } & u128::from(u64::MAX)).expect("trucating cast error");
                        let high = ({ *val } >> 64) as u64;
                        writeln!(out, "ldc.i8 {high} ldc.i8 {low} newobj instance void valuetype [System.Runtime]System.UInt128::.ctor(uint64,uint64)")
                    },
                }
                super::Const::PlatformString(msg) => {
                    let msg = &asm[*msg];
                    writeln!(out, "ldstr {msg:?}")
                }
                super::Const::Bool(val) => {
                    if *val {
                        writeln!(out, "ldc.i4.1")
                    } else {
                        writeln!(out, "ldc.i4.0")
                    }
                }
                super::Const::F32(float) => {
                    let const_literal = float.to_le_bytes();
                    writeln!(
                        out,
                        "ldc.r4 ({:02x} {:02x} {:02x} {:02x})",
                        const_literal[0], const_literal[1], const_literal[2], const_literal[3]
                    )
                }
                super::Const::F64(float) => {
                    let const_literal = float.to_le_bytes();
                    writeln!(
                        out,
                        "ldc.r8 ({:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x})",
                        const_literal[0],
                        const_literal[1],
                        const_literal[2],
                        const_literal[3],
                        const_literal[4],
                        const_literal[5],
                        const_literal[6],
                        const_literal[7]
                    )
                }
            },
            CILNode::BinOp(lhs, rhs, op) => {
                self.export_node(asm, out, lhs, sig, locals)?;
                self.export_node(asm, out, rhs, sig, locals)?;
                match op {
                    BinOp::Add => writeln!(out, "add"),
                    BinOp::Eq => writeln!(out, "ceq"),
                    BinOp::Sub => writeln!(out, "sub"),
                    BinOp::Mul => writeln!(out, "mul"),
                    BinOp::LtUn => writeln!(out, "clt.un"),
                    BinOp::Lt => writeln!(out, "clt"),
                    BinOp::GtUn => writeln!(out, "cgt.un"),
                    BinOp::Gt => writeln!(out, "cgt"),
                    BinOp::Or => writeln!(out, "or"),
                    BinOp::XOr => writeln!(out, "xor"),
                    BinOp::And => writeln!(out, "and"),
                    BinOp::Rem => writeln!(out, "rem"),
                    BinOp::RemUn => writeln!(out, "rem.un"),
                    BinOp::Shl => writeln!(out, "shl"),
                    BinOp::Shr => writeln!(out, "shr"),
                    BinOp::ShrUn => writeln!(out, "shr.un"),
                    BinOp::DivUn => writeln!(out, "div.un"),
                    BinOp::Div => writeln!(out, "div"),
                }
            }
            CILNode::UnOp(arg, un) => {
                self.export_node(asm, out, arg, sig, locals)?;
                match un {
                    UnOp::Not => writeln!(out, "not"),
                    UnOp::Neg => writeln!(out, "neg"),
                }
            }
            CILNode::LdLoc(loc) => match loc {
                0..=3 => writeln!(out, "ldloc.{loc}"),
                4..=255 => writeln!(out, "ldloc.s {loc}"),
                _ => writeln!(out, "ldloc {loc}"),
            },
            CILNode::LdLocA(arg) => match arg {
                0..=255 => writeln!(out, "ldloca.s {arg}"),
                _ => writeln!(out, "ldloca {arg}"),
            },
            CILNode::LdArg(arg) => match arg {
                0..=3 => writeln!(out, "ldarg.{arg}"),
                4..=255 => writeln!(out, "ldarg.s {arg}"),
                _ => writeln!(out, "ldarg {arg}"),
            },
            CILNode::LdArgA(arg) => match arg {
                0..=255 => writeln!(out, "ldarga.s {arg}"),
                _ => writeln!(out, "ldarga {arg}"),
            },
            CILNode::Call(call) => {
                for arg in &call.1 {
                    self.export_node(asm, out, *arg, sig, locals)?;
                }
                let mref = &asm[call.0];
                let call_op = match mref.kind() {
                    crate::cilnode::MethodKind::Static => "call",
                    crate::cilnode::MethodKind::Instance => "call instance",
                    crate::cilnode::MethodKind::Virtual => " callvirt instance",
                    crate::cilnode::MethodKind::Constructor => "newobj instance",
                };
                let sig = &asm[mref.sig()];
                let output = type_il(sig.output(), asm);
                let inputs = match mref.kind() {
                    crate::cilnode::MethodKind::Static => sig.inputs(),
                    crate::cilnode::MethodKind::Instance
                    | crate::cilnode::MethodKind::Virtual
                    | crate::cilnode::MethodKind::Constructor => {
                        assert!(
                            !sig.inputs().is_empty(),
                            "invalid argc when calling {} of {}",
                            &asm[mref.name()],
                            class_ref(mref.class(), asm)
                        );
                        &sig.inputs()[1..]
                    }
                };
                let inputs: String = inputs
                    .iter()
                    .map(|tpe| non_void_type_il(tpe, asm))
                    .intersperse(",".to_owned())
                    .collect();
                let generic = if mref.generics().is_empty() {
                    "".to_string()
                } else {
                    let generic_list: String = mref
                        .generics()
                        .iter()
                        .map(|tpe| type_il(tpe, asm))
                        .intersperse(",".to_owned())
                        .collect();
                    format!("<{generic_list}>")
                };
                let name = &asm[mref.name()];
                let class = self.partitioned_class(mref.class(), mref.name(), asm);
                writeln!(
                    out,
                    "{call_op} {output} {class}::'{name}'{generic}({inputs})"
                )
            }
            CILNode::IntCast {
                input,
                target,
                extend,
            } => {
                self.export_node(asm, out, input, sig, locals)?;
                match (target, extend) {
                    (super::Int::U8 | super::Int::I8, ExtendKind::ZeroExtend) => {
                        writeln!(out, "conv.u1")
                    }
                    (super::Int::U8 | super::Int::I8, ExtendKind::SignExtend) => {
                        writeln!(out, "conv.i1")
                    }
                    (super::Int::U16 | super::Int::I16, ExtendKind::ZeroExtend) => {
                        writeln!(out, "conv.u2")
                    }
                    (super::Int::U16 | super::Int::I16, ExtendKind::SignExtend) => {
                        writeln!(out, "conv.i2")
                    }
                    (super::Int::U32 | super::Int::I32, ExtendKind::ZeroExtend) => {
                        writeln!(out, "conv.u4")
                    }
                    (super::Int::U32 | super::Int::I32, ExtendKind::SignExtend) => {
                        writeln!(out, "conv.i4")
                    }

                    (super::Int::U64 | super::Int::I64, ExtendKind::ZeroExtend) => {
                        writeln!(out, "conv.u8")
                    }
                    (super::Int::U64 | super::Int::I64, ExtendKind::SignExtend) => {
                        writeln!(out, "conv.i8")
                    }
                    (super::Int::USize | super::Int::ISize, ExtendKind::SignExtend) => {
                        writeln!(out, "conv.i")
                    }
                    (super::Int::USize | super::Int::ISize, ExtendKind::ZeroExtend) => {
                        writeln!(out, "conv.u")
                    }
                    (super::Int::U128, ExtendKind::ZeroExtend) => todo!(),
                    (super::Int::U128, ExtendKind::SignExtend) => todo!(),
                    (super::Int::I128, ExtendKind::ZeroExtend) => todo!(),
                    (super::Int::I128, ExtendKind::SignExtend) => todo!(),
                }
            }
            CILNode::FloatCast {
                input,
                target,
                is_signed,
            } => {
                self.export_node(asm, out, input, sig, locals)?;
                match (target, is_signed) {
                    (super::Float::F16, true) => todo!(),
                    (super::Float::F16, false) => todo!(),
                    (super::Float::F32, true) => writeln!(out, "conv.r4"),
                    (super::Float::F32, false) => writeln!(out, "conv.r.un conv.r4"),
                    (super::Float::F64, true) => writeln!(out, "conv.r8"),
                    (super::Float::F64, false) => writeln!(out, "conv.r.un conv.r8"),
                    (super::Float::F128, true) => todo!(),
                    (super::Float::F128, false) => todo!(),
                }
            }
            CILNode::RefToPtr(inner) => {
                self.export_node(asm, out, inner, sig, locals)?;
                writeln!(out, "conv.u//rtp")
            }
            CILNode::PtrCast(val, _) => self.export_node(asm, out, val, sig, locals),
            CILNode::LdFieldAddress { addr, field } => {
                self.export_node(asm, out, addr, sig, locals)?;
                let fld = asm.get_field(field);
                let owner = class_ref(fld.owner(), asm);
                let name = &asm[fld.name()];
                let tpe = type_il(&fld.tpe(), asm);
                writeln!(out, "ldflda {tpe} {owner}::'{name}'")
            }
            CILNode::LdField { addr, field } => {
                self.export_node(asm, out, addr, sig, locals)?;
                let fld = asm.get_field(field);
                let owner = class_ref(fld.owner(), asm);
                let name = &asm[fld.name()];
                let tpe = type_il(&fld.tpe(), asm);
                writeln!(out, "ldfld {tpe} {owner}::'{name}'")
            }
            CILNode::LdInd {
                addr,
                tpe,
                volatile: volitale,
            } => {
                self.export_node(asm, out, addr, sig, locals)?;
                let tpe = asm[tpe];

                match (tpe, volitale) {
                    (Type::Ptr(_), true) => writeln!(out, "volatile. ldind.i"),
                    (Type::Ptr(_), false) => writeln!(out, "ldind.i"),
                    (Type::Ref(_), true) => todo!(),
                    (Type::Ref(_), false) => todo!(),
                    (Type::Int(int), volitale) => match (int, volitale) {
                        (Int::U8, true) => writeln!(out, "volatile. ldind.u1"),
                        (Int::U8, false) => writeln!(out, "ldind.u1"),
                        (Int::U16, true) => writeln!(out, "volatile. ldind.u2"),
                        (Int::U16, false) => writeln!(out, "ldind.u2"),
                        (Int::U32, true) => writeln!(out, "volatile. ldind.u4"),
                        (Int::U32, false) => writeln!(out, "ldind.u4"),
                        (Int::U64, true) => writeln!(out, "volatile. ldind.u8"),
                        (Int::U64, false) => writeln!(out, "ldind.u8"),
                        (Int::U128, true) => writeln!(
                            out,
                            "volatile. ldobj valuetype [System.Runtime]System.UInt128"
                        ),
                        (Int::U128, false) => {
                            writeln!(out, "ldobj valuetype [System.Runtime]System.UInt128")
                        }
                        (Int::USize, true) => writeln!(out, "volatile. ldind.i"),
                        (Int::USize, false) => writeln!(out, "ldind.i"),
                        (Int::I8, true) => writeln!(out, "volatile. ldind.i1"),
                        (Int::I8, false) => writeln!(out, "ldind.i1"),
                        (Int::I16, true) => writeln!(out, "volatile. ldind.i2"),
                        (Int::I16, false) => writeln!(out, "ldind.i2"),
                        (Int::I32, true) => writeln!(out, "volatile. ldind.i4"),
                        (Int::I32, false) => writeln!(out, "ldind.i4"),
                        (Int::I64, true) => writeln!(out, "volatile. ldind.i8"),
                        (Int::I64, false) => writeln!(out, "ldind.i8"),
                        (Int::I128, true) => writeln!(
                            out,
                            "volatile. ldobj valuetype [System.Runtime]System.Int128"
                        ),
                        (Int::I128, false) => {
                            writeln!(
                                out,
                                "ldobj valuetype [System.Runtime]System.Int128"
                            )
                        }
                        (Int::ISize, true) => writeln!(out, "volatile. ldind.i"),
                        (Int::ISize, false) => writeln!(out, "ldind.i"),
                    },
                    (Type::ClassRef(cref), true) => {
                        writeln!(out, "volatile. ldobj {cref}", cref = class_ref(cref, asm))
                    }
                    (Type::ClassRef(cref), false) => {
                        writeln!(out, "ldobj {cref}", cref = class_ref(cref, asm))
                    }
                    (Type::Float(float), volitale) => match (float, volitale) {
                        (super::Float::F16, true) => {
                            writeln!(out, "volatile. ldobj [System.Runtime]System.Half")
                        }
                        (super::Float::F16, false) => {
                            writeln!(out, "ldobj [System.Runtime]System.Half")
                        }
                        (super::Float::F32, true) => writeln!(out, "volatile. ldind.r4"),
                        (super::Float::F32, false) => writeln!(out, "ldind.r4"),
                        (super::Float::F64, true) => writeln!(out, "volatile. ldind.r8"),
                        (super::Float::F64, false) => writeln!(out, "ldind.r8"),
                        (super::Float::F128, true) => {
                            writeln!(out, "volatile. ldobj {}", type_il(&tpe, asm))
                        }
                        (super::Float::F128, false) => {
                            writeln!(out, "ldobj {}", type_il(&tpe, asm))
                        }
                    },
                    (Type::PlatformString | Type::PlatformObject, true) => {
                        writeln!(out, "volatile. ldind.ref")
                    }
                    (Type::PlatformString | Type::PlatformObject, false) => {
                        writeln!(out, "ldind.ref")
                    }
                    (Type::PlatformChar, true) => writeln!(out, "volatile. ldind.i2"),
                    (Type::PlatformChar, false) => writeln!(out, "ldind.i2"),
                    (Type::PlatformGeneric(_, _), true) => todo!(),
                    (Type::PlatformGeneric(_, _), false) => todo!(),
                    (Type::Bool, true) => writeln!(out, "volatile. ldind.i1"),
                    (Type::Bool, false) => writeln!(out, "ldind.i1"),
                    (Type::Void, true | false) => {
                        panic!("Void can't be dereferenced!")
                    }
                    (Type::PlatformArray { .. }, true) => writeln!(out, "volatile. ldind.ref"),
                    (Type::PlatformArray { .. }, false) => writeln!(out, "ldind.ref"),
                    (Type::FnPtr(_), true) => writeln!(out, "volatile. ldind.i"),
                    (Type::FnPtr(_), false) => writeln!(out, "ldind.i"),
                    (Type::SIMDVector(_), true) => {
                        writeln!(out, "volatile. ldobj {}", type_il(&tpe, asm))
                    }
                    (Type::SIMDVector(_), false) => {
                        writeln!(out, "ldobj {}", type_il(&tpe, asm))
                    }
                }
            }
            CILNode::SizeOf(tpe) => {
                let tpe = asm[tpe];
                if tpe == Type::Void{
                    eprintln!("WARNING: attempted to calc size_of(void). This is UB: not all targets support ZSTs. Please use Const::I32(0) instead. Continuing anyway.");
                    writeln!(out, "ldc.i4.0")
                }
                else{
                    writeln!(out, "sizeof {}", type_il(&tpe, asm))
                }
            }
            CILNode::GetException => Ok(()),
            CILNode::IsInst(val, tpe) => {
                self.export_node(asm, out, val, sig, locals)?;
                writeln!(out, "isinst {tpe}", tpe = type_il(&asm[tpe], asm))
            }
            CILNode::CheckedCast(val, tpe) => {
                self.export_node(asm, out, val, sig, locals)?;
                writeln!(out, "castclass {tpe}", tpe = type_il(&asm[tpe], asm))
            }
            CILNode::CallI(calli) => {
                let (fn_ptr, fn_sig, args) = calli.as_ref();
                for arg in args {
                    self.export_node(asm, out, *arg, sig, locals)?;
                }
                let fn_sig = asm[*fn_sig].clone();
                let output = type_il(fn_sig.output(), asm);
                self.export_node(asm, out, *fn_ptr, sig, locals)?;
                let inputs: String = fn_sig
                    .inputs()
                    .iter()
                    .map(|tpe| non_void_type_il(tpe, asm))
                    .intersperse(",".to_owned())
                    .collect();
                writeln!(out, "calli {output} ({inputs})")
            }
            CILNode::LocAlloc { size } => {
                self.export_node(asm, out, size, sig, locals)?;
                writeln!(out, "localloc")
            }
            CILNode::LdStaticField(sfld) => {
                let sfld = asm.get_static_field(sfld);
                let owner = class_ref(sfld.owner(), asm);
                let name = &asm[sfld.name()];
                let tpe = non_void_type_il(&sfld.tpe(), asm);
                writeln!(out, "ldsfld {tpe} {owner}::{name}")
            }
            CILNode::LdStaticFieldAddress(sfld) => {
                let sfld = asm.get_static_field(sfld);
                let owner = class_ref(sfld.owner(), asm);
                let name = &asm[sfld.name()];
                let tpe = non_void_type_il(&sfld.tpe(), asm);
                writeln!(out, "ldsflda {tpe} {owner}::{name}")
            }
            CILNode::LdFtn(ftn) => {
                let mref = &asm[ftn];
                let sig = &asm[mref.sig()];
                let output = type_il(sig.output(), asm);
                let inputs = match mref.kind() {
                    crate::cilnode::MethodKind::Static => sig.inputs(),
                    crate::cilnode::MethodKind::Instance
                    | crate::cilnode::MethodKind::Virtual
                    | crate::cilnode::MethodKind::Constructor => &sig.inputs()[1..],
                };
                let inputs: String = inputs
                    .iter()
                    .map(|tpe| non_void_type_il(tpe, asm))
                    .intersperse(",".to_owned())
                    .collect();
                let name = &asm[mref.name()];
                let class = self.partitioned_class(mref.class(), mref.name(), asm);
                let ldftn_op = match mref.kind() {
                    crate::cilnode::MethodKind::Static => "ldftn",
                    crate::cilnode::MethodKind::Instance => "ldftn instance",
                    crate::cilnode::MethodKind::Virtual => " ldftn instance",
                    crate::cilnode::MethodKind::Constructor => "ldftn instance",
                };
                writeln!(
                    out,
                    "{ldftn_op} {output} {class}::'{name}'({inputs}) //{ftn:?}"
                )
            }
            CILNode::LdTypeToken(tok) => {
                writeln!(out, "ldtoken {tok}", tok = type_il(&asm[tok], asm))
            }
            CILNode::LdLen(array) => {
                self.export_node(asm, out, array, sig, locals)?;
                writeln!(out, "ldlen")
            }
            CILNode::LocAllocAlgined { tpe, align } => {
                writeln!(out, "sizeof {tpe} ldc.i8 {align} conv.i add localloc dup ldc.i8 {align} add ldc.i8 {align} rem sub ldc.i8 {align} add conv.u", tpe = type_il(&asm[tpe], asm))
            }
            CILNode::LdElelemRef { array, index } => {
                self.export_node(asm, out, array, sig, locals)?;
                self.export_node(asm, out, index, sig, locals)?;
                writeln!(out, "ldelem.ref")
            }
            CILNode::UnboxAny { object, tpe } => {
                self.export_node(asm, out, object, sig, locals)?;
                writeln!(out, "unbox.any {object}", object = type_il(&asm[tpe], asm))
            }
            CILNode::Box { value, tpe } => {
                self.export_node(asm, out, value, sig, locals)?;
                writeln!(out, "box {tpe}", tpe = type_il(&asm[tpe], asm))
            }
            CILNode::NewArr { elem, len } => {
                self.export_node(asm, out, len, sig, locals)?;
                writeln!(out, "newarr {elem}", elem = type_il(&asm[elem], asm))
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn export_root(
        &self,
        asm: &mut super::Assembly,
        out: &mut impl Write,
        root: Interned<CILRoot>,
        is_handler: bool,
        has_handler: bool,
        sig: Interned<FnSig>,
        locals: &[LocalDef],
    ) -> std::io::Result<()> {
        let root = asm.get_root(root).clone();
        match root {
            super::CILRoot::StLoc(loc, val) => {
                self.export_node(asm, out, val, sig, locals)?;
                match loc {
                    0..=3 => writeln!(out, "stloc.{loc}"),
                    4..=255 => writeln!(out, "stloc.s {loc}"),
                    _ => writeln!(out, "stloc {loc}"),
                }
            }
            super::CILRoot::StArg(loc, val) => {
                self.export_node(asm, out, val, sig, locals)?;
                match loc {
                    0..=255 => writeln!(out, "starg.s {loc}"),
                    _ => writeln!(out, "starg {loc}"),
                }
            }
            super::CILRoot::Ret(val) => {
                self.export_node(asm, out, val, sig, locals)?;
                writeln!(out, "ret")
            }
            super::CILRoot::Pop(val) => {
                self.export_node(asm, out, val, sig, locals)?;
                writeln!(out, "pop")
            }
            super::CILRoot::Throw(val) => {
                self.export_node(asm, out, val, sig, locals)?;
                writeln!(out, "throw")
            }
            super::CILRoot::VoidRet => {
                writeln!(out, "ret")
            }
            super::CILRoot::Break => {
                writeln!(out, "break")
            }
            super::CILRoot::Nop => {
                writeln!(out, "nop")
            }
            super::CILRoot::Branch(branch) => match &branch.2 {
                Some(BranchCond::Eq(a, b)) => {
                    self.export_node(asm, out, *a, sig, locals)?;
                    self.export_node(asm, out, *b, sig, locals)?;
                    writeln!(
                        out,
                        "beq {}",
                        branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                    )
                }
                Some(BranchCond::Ne(a, b)) => {
                    self.export_node(asm, out, *a, sig, locals)?;
                    self.export_node(asm, out, *b, sig, locals)?;
                    writeln!(
                        out,
                        "bne.un {}",
                        branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                    )
                }
                Some(BranchCond::Lt(a, b, kind)) => {
                    self.export_node(asm, out, *a, sig, locals)?;
                    self.export_node(asm, out, *b, sig, locals)?;
                    match kind {
                        super::cilroot::CmpKind::Ordered | super::cilroot::CmpKind::Signed => {
                            writeln!(
                                out,
                                "blt {}",
                                branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                            )
                        }
                        super::cilroot::CmpKind::Unordered | super::cilroot::CmpKind::Unsigned => {
                            writeln!(
                                out,
                                "blt.un {}",
                                branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                            )
                        }
                    }
                }
                Some(BranchCond::Gt(a, b, kind)) => {
                    self.export_node(asm, out, *a, sig, locals)?;
                    self.export_node(asm, out, *b, sig, locals)?;
                    match kind {
                        super::cilroot::CmpKind::Ordered | super::cilroot::CmpKind::Signed => {
                            writeln!(
                                out,
                                "bgt {}",
                                branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                            )
                        }
                        super::cilroot::CmpKind::Unordered | super::cilroot::CmpKind::Unsigned => {
                            writeln!(
                                out,
                                "bgt.un {}",
                                branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                            )
                        }
                    }
                }
                Some(BranchCond::Le(a, b, kind)) => {
                    self.export_node(asm, out, *a, sig, locals)?;
                    self.export_node(asm, out, *b, sig, locals)?;
                    match kind {
                        super::cilroot::CmpKind::Ordered | super::cilroot::CmpKind::Signed => {
                            writeln!(
                                out,
                                "ble {}",
                                branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                            )
                        }
                        super::cilroot::CmpKind::Unordered | super::cilroot::CmpKind::Unsigned => {
                            writeln!(
                                out,
                                "ble.un {}",
                                branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                            )
                        }
                    }
                }
                Some(BranchCond::Ge(a, b, kind)) => {
                    self.export_node(asm, out, *a, sig, locals)?;
                    self.export_node(asm, out, *b, sig, locals)?;
                    match kind {
                        super::cilroot::CmpKind::Ordered | super::cilroot::CmpKind::Signed => {
                            writeln!(
                                out,
                                "bge {}",
                                branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                            )
                        }
                        super::cilroot::CmpKind::Unordered | super::cilroot::CmpKind::Unsigned => {
                            writeln!(
                                out,
                                "bge.un {}",
                                branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                            )
                        }
                    }
                }
                Some(BranchCond::True(cond)) => {
                    self.export_node(asm, out, *cond, sig, locals)?;
                    writeln!(
                        out,
                        "brtrue {}",
                        branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                    )
                }
                Some(BranchCond::False(cond)) => {
                    self.export_node(asm, out, *cond, sig, locals)?;
                    writeln!(
                        out,
                        "brfalse {}",
                        branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                    )
                }
                None => {
                    writeln!(
                        out,
                        "br {}",
                        branch_cond_to_name(branch.0, branch.1, has_handler, is_handler)
                    )
                }
            },
            super::CILRoot::SourceFileInfo {
                line_start,
                line_len,
                col_start,
                col_len,
                file,
            } => {
                let col_end = u32::from(col_start) + u32::from(col_len);
                let line_end = line_start + u32::from(line_len);
                let file = &asm[file];
                match self.flavour {
                    IlasmFlavour::Clasic => {
                        writeln!(out, ".line {line_start}:{col_start} '{file}'")
                    }
                    IlasmFlavour::Modern => writeln!(
                        out,
                        ".line {line_start},{line_end}:{col_start},{col_end} '{file}'"
                    ),
                }
            }
            super::CILRoot::SetField(flds) => {
                self.export_node(asm, out, flds.1, sig, locals)?;
                self.export_node(asm, out, flds.2, sig, locals)?;
                let fld = asm.get_field(flds.0);
                let owner = class_ref(fld.owner(), asm);
                let name = &asm[fld.name()];
                let tpe = type_il(&fld.tpe(), asm);
                writeln!(out, "stfld {tpe} {owner}::'{name}'")
            }
            super::CILRoot::Call(call) => {
                for arg in &call.1 {
                    self.export_node(asm, out, *arg, sig, locals)?;
                }
                let mref = &asm[call.0];
                let call_op = match mref.kind() {
                    crate::cilnode::MethodKind::Static => "call",
                    crate::cilnode::MethodKind::Instance => "call instance",
                    crate::cilnode::MethodKind::Virtual => " callvirt instance",
                    crate::cilnode::MethodKind::Constructor => {
                        panic!("A constructor can't be a CIL root")
                    }
                };
                let sig = &asm[mref.sig()];
                let output = type_il(sig.output(), asm);
                let inputs = match mref.kind() {
                    crate::cilnode::MethodKind::Static => sig.inputs(),
                    crate::cilnode::MethodKind::Instance
                    | crate::cilnode::MethodKind::Virtual
                    | crate::cilnode::MethodKind::Constructor => &sig.inputs()[1..],
                };
                let inputs: String = inputs
                    .iter()
                    .map(|tpe| non_void_type_il(tpe, asm))
                    .intersperse(",".to_owned())
                    .collect();
                let name = &asm[mref.name()];
                let class = self.partitioned_class(mref.class(), mref.name(), asm);

                writeln!(
                    out,
                    "{call_op} {output} {class}::'{name}'({inputs}) //mref:{:?}",
                    call.0
                )
            }
            super::CILRoot::CpObj { src, dst, tpe } => {
                self.export_node(asm, out, src, sig, locals)?;
                self.export_node(asm, out, dst, sig, locals)?;
                let tpe = type_il(&asm[tpe], asm);
                writeln!(out, "cpobj {tpe}")
            }
            super::CILRoot::InitObj(addr, tpe) => {
                self.export_node(asm, out, addr, sig, locals)?;
                writeln!(out, "initobj {}", type_il(&asm[tpe], asm))
            }
            super::CILRoot::StInd(stind) => {
                self.export_node(asm, out, stind.0, sig, locals)?;
                self.export_node(asm, out, stind.1, sig, locals)?;

                let tpe = stind.2;
                let is_volitale = if stind.3 { "volatile." } else { "" };
                match tpe {
                    Type::Ptr(_) => writeln!(out, "{is_volitale} stind.i"),
                    Type::Ref(_) => todo!(),
                    Type::Int(int) => match int {
                        super::Int::U8 => writeln!(out, "{is_volitale} stind.i1"),
                        super::Int::U16 => writeln!(out, "{is_volitale} stind.i2"),
                        super::Int::U32 => writeln!(out, "{is_volitale} stind.i4"),
                        super::Int::U64 => writeln!(out, "{is_volitale} stind.i8"),
                        super::Int::U128 => {
                            writeln!(out, "{is_volitale} stobj [System.Runtime]System.UInt128")
                        }
                        super::Int::USize => writeln!(out, "{is_volitale} stind.i"),
                        super::Int::I8 => writeln!(out, "{is_volitale} stind.i1"),
                        super::Int::I16 => writeln!(out, "{is_volitale} stind.i2"),
                        super::Int::I32 => writeln!(out, "{is_volitale} stind.i4"),
                        super::Int::I64 => writeln!(out, "{is_volitale} stind.i8"),
                        super::Int::I128 => {
                            writeln!(out, "{is_volitale} stobj [System.Runtime]System.Int128")
                        }
                        super::Int::ISize => writeln!(out, "{is_volitale} stind.i"),
                    },
                    Type::ClassRef(cref_idx) => {
                        let cref = asm.class_ref(cref_idx);
                        if cref.is_valuetype() {
                            writeln!(
                                out,
                                "{is_volitale} stobj {cref}",
                                cref = class_ref(cref_idx, asm)
                            )
                        } else {
                            writeln!(out, "{is_volitale} stind.ref")
                        }
                    }
                    Type::Float(float) => match float {
                        // f16 has no `stind`; store the 2-byte `System.Half` value type via `stobj`,
                        // mirroring the `LdInd(F16) => ldobj System.Half` arm above.
                        super::Float::F16 => {
                            writeln!(out, "{is_volitale} stobj [System.Runtime]System.Half")
                        }
                        super::Float::F32 => writeln!(out, "{is_volitale} stind.r4"),
                        super::Float::F64 => writeln!(out, "{is_volitale} stind.r8"),
                        super::Float::F128 => writeln!(out, "stobj {}", type_il(&tpe, asm)),
                    },
                    Type::PlatformString | Type::PlatformObject => {
                        writeln!(out, "{is_volitale} stind.ref")
                    }
                    Type::PlatformChar => writeln!(out, "{is_volitale} stind.i2"),
                    Type::PlatformGeneric(_, _) => todo!(),
                    Type::Bool => writeln!(out, "{is_volitale} stind.i1"),
                    Type::Void => writeln!(out, "pop pop ldstr \"Attempted to wrtie to a zero-sized type(void).\" newobj void [System.Runtime]System.Exception::.ctor(string) throw"), // TODO: forbid this, since this is NEVER valid.
                    Type::PlatformArray { .. } => writeln!(out, "{is_volitale} stind.ref"),
                    Type::FnPtr(_) => writeln!(out, "{is_volitale} stind.i"),
                    Type::SIMDVector(_)=>writeln!(out, "stobj {}", type_il(&tpe, asm)),
                }
            }
            super::CILRoot::InitBlk(blk) => {
                self.export_node(asm, out, blk.0, sig, locals)?;
                self.export_node(asm, out, blk.1, sig, locals)?;
                self.export_node(asm, out, blk.2, sig, locals)?;
                writeln!(out, "initblk")
            }
            super::CILRoot::CpBlk(cpblk) => {
                self.export_node(asm, out, cpblk.0, sig, locals)?;
                self.export_node(asm, out, cpblk.1, sig, locals)?;
                self.export_node(asm, out, cpblk.2, sig, locals)?;
                writeln!(out, "cpblk")
            }
            super::CILRoot::CallI(calli) => {
                let (fn_ptr, fn_sig, args) = calli.as_ref();
                for arg in args {
                    self.export_node(asm, out, *arg, sig, locals)?;
                }
                let fn_sig = asm[*fn_sig].clone();
                let output = type_il(fn_sig.output(), asm);
                self.export_node(asm, out, *fn_ptr, sig, locals)?;
                let inputs: String = fn_sig
                    .inputs()
                    .iter()
                    .map(|tpe| non_void_type_il(tpe, asm))
                    .intersperse(",".to_owned())
                    .collect();
                writeln!(out, "calli {output} ({inputs})")
            }
            super::CILRoot::TerminateRegion { protected, reason } => {
                // Render a self-contained inner protected region whose catch does an uncatchable
                // `FailFast`. This models a `Drop`-glue call on a MIR cleanup block carrying an
                // `UnwindAction::Terminate` edge (a destructor that may panic while already
                // unwinding, or cross a `nounwind` boundary mid-cleanup). The `protected` op runs;
                // if it throws, the catch aborts the process (FailFast bypasses every managed
                // catch — exactly Rust's `terminate`/double-panic semantics). If it does NOT throw,
                // control `leave`s the region to `tr_done_N` and the surrounding cleanup continues
                // unchanged (the block's `goto`/rethrow continuation is emitted SEPARATELY, after
                // this root — see the frontend `Drop` arm). The `tr_*` label namespace is disjoint
                // from `bb`/`h`/`jp`, so there is no collision with any block/handler label. `N` is a
                // fresh monotonic counter value (NOT the interned root index, which can repeat within
                // a method via shared/CSE'd roots and would yield `Duplicate label` ilasm errors).
                let lbl = self.terminate_region_label.get();
                self.terminate_region_label.set(lbl + 1);
                // Message mirrors `emit_terminate` (src/terminator/mod.rs): 1 = InCleanup, else Abi.
                let msg = if reason == 1 {
                    "Rust panicked while running a destructor during unwinding (panic in a destructor during cleanup); aborted."
                } else {
                    "Rust unwinding crossed a `nounwind` ABI boundary (panic in a function that cannot unwind); aborted."
                };
                writeln!(out, ".try{{")?;
                // `is_handler`/`has_handler` are propagated unchanged: the protected op is lexically
                // inside whatever enclosing region this root already lives in.
                self.export_root(asm, out, protected, is_handler, has_handler, sig, locals)?;
                // A protected region cannot fall through — its normal exit MUST be `leave`.
                writeln!(out, "leave tr_done_{lbl}")?;
                writeln!(out, "}} catch [System.Runtime]System.Object{{")?;
                writeln!(out, "pop")?;
                writeln!(out, "ldstr {msg:?}")?;
                writeln!(
                    out,
                    "call void class [System.Runtime]'System.Environment'::'FailFast'(string)"
                )?;
                // FailFast never returns; the trailing `rethrow` only keeps the catch well-formed.
                writeln!(out, "rethrow")?;
                writeln!(out, "}}")?;
                writeln!(out, "tr_done_{lbl}: nop")
            }
            super::CILRoot::ExitSpecialRegion { target, source } => {
                if is_handler {
                    writeln!(out, "h{source}_{target}: leave bb{target}")
                } else if has_handler {
                    writeln!(out, "jp{source}_{target}: leave bb{target}")
                } else {
                    Ok(())
                }
            }
            super::CILRoot::ReThrow => {
                writeln!(out, "rethrow")
            }
            super::CILRoot::SetStaticField { field, val } => {
                self.export_node(asm, out, val, sig, locals)?;
                let sfld = asm[field];
                let owner = class_ref(sfld.owner(), asm);
                let name = &asm[sfld.name()];
                let tpe = type_il(&sfld.tpe(), asm);
                writeln!(out, "stsfld {tpe} {owner}::{name}")
            }
            super::CILRoot::Unreachable(msg) => {
                writeln!(
                    out,
                    "ldstr {:?} newobj void [System.Runtime]System.Exception::.ctor(string) throw",
                    &asm[msg]
                )
            }
            super::CILRoot::StElem {
                array,
                index,
                value,
                elem,
            } => {
                self.export_node(asm, out, array, sig, locals)?;
                self.export_node(asm, out, index, sig, locals)?;
                self.export_node(asm, out, value, sig, locals)?;
                // Primitive element types use the dedicated `stelem.*` opcodes; everything else
                // uses the generic typed `stelem <type>` form.
                match asm[elem] {
                    Type::Int(super::Int::I8 | super::Int::U8) => writeln!(out, "stelem.i1"),
                    Type::Int(super::Int::I16 | super::Int::U16) => writeln!(out, "stelem.i2"),
                    Type::Int(super::Int::I32 | super::Int::U32) => writeln!(out, "stelem.i4"),
                    Type::Int(super::Int::I64 | super::Int::U64) => writeln!(out, "stelem.i8"),
                    Type::Int(super::Int::ISize | super::Int::USize) => writeln!(out, "stelem.i"),
                    Type::Bool => writeln!(out, "stelem.i1"),
                    Type::Float(super::Float::F32) => writeln!(out, "stelem.r4"),
                    Type::Float(super::Float::F64) => writeln!(out, "stelem.r8"),
                    _ => writeln!(out, "stelem {elem}", elem = type_il(&asm[elem], asm)),
                }
            }
        }
    }
}
#[cfg(not(target_os = "windows"))]
fn assemble_file(exe_out: &Path, il_path: &Path, is_lib: bool) {
    let asm_type = if is_lib { "-dll" } else { "-exe" };
    let run = |debug: bool| {
        let mut cmd = std::process::Command::new(ILASM_PATH.clone());
        cmd.arg(il_path)
            .arg(format!("-output:{exe_out}", exe_out = exe_out.to_string_lossy()))
            .arg("-OPTIMIZE")
            .arg(asm_type);
        // .arg("-FOLD") saves up on space, consider enabling.
        if debug {
            cmd.arg("-debug");
        }
        if *ILASM_FLAVOUR == IlasmFlavour::Clasic {
            // Limit the memory usage of mono
            cmd.env("MONO_GC_PARAMS", "soft-heap-limit=500m");
        }
        let ilasm_start = std::time::Instant::now();
        let out = cmd.output().unwrap();
        println!("==> ilasm in {:?}", ilasm_start.elapsed());
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        (cmd, stdout, stderr)
    };
    let failed = |stdout: &str, stderr: &str| {
        stderr.contains("\nError\n") || stderr.contains("FAILURE") || stdout.contains("FAILURE")
    };
    let (mut cmd, mut stdout, mut stderr) = run(true);
    // ilasm can assemble the whole module and write the PE, then fail *only* when writing the PDB
    // (its debug-info writer chokes on very large assemblies — the rust-lang/rust `coretests`
    // harness is ~5M IL lines and hits `Failed to write PDB file, error code=0x80070057`). The PE
    // is valid and PDBs are optional, so retry without `-debug` to still get a runnable assembly.
    // Gated on "Writing PE file" so a genuine IL error (which fails before the PE) is never masked.
    if failed(&stdout, &stderr)
        && (stdout.contains("Failed to write PDB") || stderr.contains("Failed to write PDB"))
        && stdout.contains("Writing PE file")
    {
        (cmd, stdout, stderr) = run(false);
    }
    assert!(
        !failed(&stdout, &stderr),
        "stdout:{stdout} stderr:{stderr} cmd:{cmd:?}"
    );
}
#[cfg(target_os = "windows")]
fn assemble_file(exe_out: &Path, il_path: &Path, is_lib: bool) {
    let asm_type = if is_lib { "-dll" } else { "-exe" };
    let mut cmd = std::process::Command::new(ILASM_PATH.clone());
    cmd.arg(il_path)
    .arg(format!("-output:{exe_out}", exe_out = exe_out.to_string_lossy()))
    .arg("-OPTIMIZE")
    .arg(asm_type)
    // .arg("-FOLD") saves up on space, consider enabling.
    ;
    if *ILASM_FLAVOUR == IlasmFlavour::Clasic {
        // Limit the memory usage of mono
        cmd.env("MONO_GC_PARAMS", "soft-heap-limit=500m");
    }
    let ilasm_start = std::time::Instant::now();
    let out = cmd.output().unwrap();
    println!("==> ilasm in {:?}", ilasm_start.elapsed());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !(stderr.contains("\nError\n") || stderr.contains("FAILURE") || stdout.contains("FAILURE")),
        "stdout:{} stderr:{} cmd:{cmd:?}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    let asm_type = if is_lib { "-dll" } else { "-exe" };
    let mut cmd = std::process::Command::new(ILASM_PATH.clone());
    cmd.arg(il_path)
    .arg(format!("-output:{exe_out}", exe_out = exe_out.to_string_lossy()))
    .arg("-debug")
    .arg("-OPTIMIZE")
    .arg(asm_type)
    // .arg("-FOLD") saves up on space, consider enabling.
    ;
    if *ILASM_FLAVOUR == IlasmFlavour::Clasic {
        // Limit the memory usage of mono
        cmd.env("MONO_GC_PARAMS", "soft-heap-limit=500m");
    }
    let ilasm_start = std::time::Instant::now();
    let out = cmd.output().unwrap();
    println!("==> ilasm in {:?}", ilasm_start.elapsed());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !(stderr.contains("\nError\n") || stderr.contains("FAILURE") || stdout.contains("FAILURE")),
        "stdout:{} stderr:{} cmd:{cmd:?}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
}
impl Exporter for ILExporter {
    type Error = std::io::Error;

    fn export(
        &mut self,
        asm: &super::Assembly,
        target: &std::path::Path,
    ) -> Result<(), Self::Error> {
        // The IL file should be next to the target
        let il_path = target.with_extension("il");

        if let Err(err) = std::fs::remove_file(&il_path) {
            match err.kind() {
                std::io::ErrorKind::NotFound => (),
                _ => {
                    panic!("Could not remove tmp file because {err:?}")
                }
            }
        };
        let mut il_out = std::io::BufWriter::new(std::fs::File::create(&il_path)?);
        self.export_to_write(asm, &mut il_out)?;
        // Needed to ensure the IL file is valid!
        il_out.flush().unwrap();
        drop(il_out);
        // A library is the final artifact itself — emit the .NET assembly directly to the requested
        // output path (no native launcher wraps it, unlike an executable). An executable still emits
        // to `<stem>.exe`, which the linker's launcher then loads.
        let exe_out = if self.is_lib {
            std::path::absolute(target).unwrap()
        } else {
            std::path::absolute(target.with_extension("exe")).unwrap()
        };
        if let Err(err) = std::fs::remove_file(&exe_out) {
            match err.kind() {
                std::io::ErrorKind::NotFound => (),
                _ => {
                    panic!("Could not remove tmp file because {err:?}")
                }
            }
        };
        assemble_file(&exe_out, &il_path, self.is_lib);

        Ok(())
    }
}
/// The maximum class-name length the CoreCLR `ilasm` accepts ("Full class name too long
/// (N characters, 1023 allowed)"). Mono's `ilasm` (the Linux/Docker assembler) has no such
/// cap, so the deeply-nested monomorphized generic names the backend emits (e.g.
/// `addr2line…LookupResult<…>` can exceed 2KB) only break the CoreCLR assembler — which is the
/// one used on the NATIVE macOS / Windows path. See `dotnet_class_name`.
const ILASM_MAX_CLASS_NAME: usize = 1023;

/// Deterministically shorten an over-long class name so the CoreCLR `ilasm` accepts it.
///
/// .NET class names are pure identifiers — the IL is self-consistent as long as the SAME
/// transform is applied at the type's definition AND at every reference. Both sites resolve
/// the identical interned name string and call this pure function, so the shortened forms
/// always match (no def/ref skew). Names within the limit are returned UNCHANGED (a borrow),
/// so the Linux/Docker output and the `::stable` suite are byte-for-byte unaffected — this only
/// rewrites the handful of >1023-char monomorphized generic names that the stricter CoreCLR
/// assembler would otherwise reject.
///
/// The short form keeps a readable head (so disassembly is still navigable) plus a 64-bit
/// FNV-1a hash of the FULL original name (collision-resistant across distinct long names). The
/// length budget is well under the limit. FNV-1a is used (not `DefaultHasher`) so the mapping is
/// stable and identical at every call site regardless of build.
fn dotnet_class_name(name: &str) -> std::borrow::Cow<'_, str> {
    if name.len() <= ILASM_MAX_CLASS_NAME {
        return std::borrow::Cow::Borrowed(name);
    }
    // FNV-1a 64-bit over the full original name.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // Keep a readable, identifier-safe head. `name` is an already-escaped IL identifier (no `'`
    // or other quoting needed inside the surrounding quotes), so a byte-prefix is safe to splice
    // — but cut on a char boundary to keep it valid UTF-8 for the formatter.
    const HEAD: usize = 900;
    let mut head_end = HEAD.min(name.len());
    while head_end > 0 && !name.is_char_boundary(head_end) {
        head_end -= 1;
    }
    std::borrow::Cow::Owned(format!("{}__h{hash:016x}", &name[..head_end]))
}
/// Map an IMPLEMENTATION-assembly name to the public REFERENCE assembly a C# compiler resolves
/// against: `System.Object`/`ValueType`/`String`/`Exception` physically live in
/// `System.Private.CoreLib` but are type-forwarded from `System.Runtime`. A separately-compiled C#
/// project only references the ref assembly, so any CoreLib name in the C#-VISIBLE METADATA — the
/// `.assembly extern` table and base-type `extends` clauses — fails to resolve with CS0012.
///
/// Applied to METADATA ONLY, never to method-body instruction operands. A
/// `call instance [System.Runtime]System.String::method` is JIT-rejected as "Bad IL format" on a
/// real CoreLib String (see `mycorrhiza/src/system/mod.rs`), so declaring-type refs inside
/// instruction bodies (`class_ref`) keep the impl-assembly name; the runtime resolves the
/// type-forward fine, and a C# compiler never reads method bodies.
fn ref_assembly_name(name: &str) -> &str {
    match name {
        "System.Private.CoreLib" | "mscorlib" => "System.Runtime",
        other => other,
    }
}
/// Like [`ref_assembly_name`], but for the (much rarer) case where the impl-assembly name alone
/// isn't enough: `System.Private.CoreLib` type-forwards `Object`/`String`/`Exception`/… through
/// the umbrella `System.Runtime` reference assembly, but NOT every CoreLib type — some (e.g. the
/// `System.Threading` synchronization primitives `SemaphoreSlim`/`ManualResetEventSlim`/
/// `CountdownEvent`/`Barrier`) are genuine `TypeDef`s in a DIFFERENT, more specific reference
/// assembly instead (confirmed by scanning the actual net8.0 ref-pack DLLs: `System.Threading.dll`
/// defines all four, `System.Runtime.dll` does not forward them). A blanket
/// CoreLib -> `System.Runtime` substitution is simply wrong for these — C# reports `CS7069`
/// ("claims it is defined in 'System.Runtime', but it could not be found"), not CS0012.
///
/// This is a small, explicit, closed table of the types this backend's mycorrhiza bindings
/// actually expose across a C#-visible signature position today — NOT a general BCL
/// type-forwarding resolver (that would need to scan the ref-pack metadata at build time, a much
/// larger undertaking out of scope here). Extend it if/when another such type needs to cross a
/// signature boundary; falls back to [`ref_assembly_name`] for everything else.
fn ref_assembly_name_for_type<'a>(assembly: &'a str, type_name: &str) -> &'a str {
    if matches!(assembly, "System.Private.CoreLib" | "mscorlib") {
        match type_name {
            "System.Threading.SemaphoreSlim"
            | "System.Threading.ManualResetEventSlim"
            | "System.Threading.CountdownEvent"
            | "System.Threading.Barrier" => return "System.Threading",
            // `Task`/`Task<T>` (this table's key is the bare name; the `\`N` generic-arity
            // suffix is appended separately by the caller, so one entry covers both) are
            // genuine TypeDefs in `System.Threading.Tasks.dll`, confirmed by scanning the real
            // net8.0 ref-pack (`System.Runtime.dll` does not define or forward either).
            "System.Threading.Tasks.Task" => return "System.Threading.Tasks",
            _ => {}
        }
    }
    ref_assembly_name(assembly)
}
/// Whether an external assembly name is part of the .NET base class library (so its `.assembly extern`
/// header carries the runtime `.ver` + the shared ECMA public-key token). Everything else is treated as
/// a consumer-supplied assembly, referenced by simple name only (no version/token) so it binds against
/// a plain `dotnet build` output. The `::stable` suite references only `System.*`/`Microsoft.*`/CoreLib
/// assemblies, so its externs are unaffected.
fn is_bcl_assembly(name: &str) -> bool {
    name.starts_with("System")
        || name.starts_with("Microsoft")
        || matches!(name, "mscorlib" | "netstandard" | "WindowsBase")
}
fn simple_class_ref(cref: Interned<ClassRef>, asm: &Assembly) -> String {
    let cref = asm.class_ref(cref);
    let name = dotnet_class_name(&asm[cref.name()]);
    // Every `extends=`/`implements=` reference registered before the generic-interface intrinsic
    // (`rustc_codegen_clr_add_generic_interface_impl`) always had empty `generics()`, so this arm
    // was dead code until that feature existed — adding it here cannot change any previously
    // passing case. A GENERIC reference (e.g. `IEquatable<int>`) needs the arity suffix
    // (`` `1 ``) and the `<…>` instantiation list, exactly like `class_ref`'s own construction
    // below, or ilasm parses it as a reference to a same-named non-generic type that doesn't
    // exist (or worse, an unrelated one that happens to share the un-suffixed name).
    if cref.generics().is_empty() {
        if let Some(assembly) = cref.asm() {
            format!("[{assembly}]'{name}'", assembly = ref_assembly_name(&asm[assembly]))
        } else {
            format!("'{name}'")
        }
    } else {
        let prefix = if cref.is_valuetype() { "valuetype" } else { "class" };
        let generic_postfix = format!("`{}", cref.generics().len());
        let generic_list = format!(
            "<{generics}>",
            generics = cref
                .generics()
                .iter()
                .map(|tpe| type_il(tpe, asm))
                .intersperse(",".to_string())
                .collect::<String>()
        );
        if let Some(assembly) = cref.asm() {
            format!(
                "{prefix} [{assembly}]'{name}{generic_postfix}'{generic_list}",
                assembly = ref_assembly_name_for_type(&asm[assembly], &asm[cref.name()])
            )
        } else {
            format!("{prefix} '{name}{generic_postfix}'{generic_list}")
        }
    }
}
pub(crate) fn class_ref(cref: Interned<ClassRef>, asm: &Assembly) -> String {
    let cref = asm.class_ref(cref);
    let raw_name = &asm[cref.name()];
    let name = dotnet_class_name(raw_name);
    // Normalize the known BCL primitive value types: these CoreLib types are
    // *unconditionally* `valuetype` in .NET, so a `class`-prefixed reference to one
    // makes the runtime reject the type-load with
    // `TypeLoadException: ... due to value type mismatch` the moment the call is JITted.
    // Some codegen/intrinsic paths (e.g. `f64::abs`/`copysign`/`mul_add` ->
    // `System.Double::Abs`/`CopySign`/`FusedMultiplyAdd`) still produce a
    // `is_valuetype=false` ClassRef for these names (a residual of the pre-P2-S1 default);
    // forcing the prefix here closes the whole family at the rendering boundary regardless
    // of which path interned the ref. Safe because these exact names are ALWAYS .NET value
    // types (P2-S2 differential-oracle fix; regression: cargo_tests/float_debug_fmt). Only
    // applies to the System.Runtime-qualified BCL names, never to user/Rust types.
    let is_bcl_valuetype = matches!(
        raw_name,
        "System.Double"
            | "System.Single"
            | "System.Half"
            | "System.Int128"
            | "System.UInt128"
    );
    let prefix = if cref.is_valuetype() || is_bcl_valuetype {
        "valuetype"
    } else {
        "class"
    };
    let generic_list = if cref.generics().is_empty() {
        String::new()
    } else {
        format!(
            "<{generics}>",
            generics = cref
                .generics()
                .iter()
                .map(|tpe| type_il(tpe, asm))
                .intersperse(",".to_string())
                .collect::<String>()
        )
    };
    let generic_postfix = if cref.generics().is_empty() {
        String::new()
    } else {
        format!("`{}", cref.generics().len())
    };
    if let Some(assembly) = cref.asm() {
        // Declaring-type position inside method-body instructions (call/callvirt/ldfld/ldobj/...).
        // Keep the IMPL-assembly name verbatim: a `call instance [System.Runtime]System.String::m`
        // is "Bad IL format" on a real CoreLib String. C# never reads bodies, so the CS0012 fix is
        // confined to metadata (`ref_assembly_name` is applied in `simple_class_ref`/extern table).
        format!(
            "{prefix} [{assembly}]'{name}{generic_postfix}'{generic_list}",
            assembly = &asm[assembly]
        )
    } else {
        format!("{prefix} '{name}{generic_postfix}'{generic_list}")
    }
}
fn non_void_type_il(tpe: &Type, asm: &Assembly) -> String {
    match tpe {
        Type::Void => "valuetype RustVoid".into(),
        _ => type_il(tpe, asm),
    }
}
fn type_il(tpe: &Type, asm: &Assembly) -> String {
    match tpe {
        Type::SIMDVector(simdvec) => {
            let vec_bits = simdvec.bits();
            assert!(
                vec_bits == 64 || vec_bits == 128 || vec_bits == 256 || vec_bits == 512,
                "Unusported SIMD vector size"
            );
            let elem = match simdvec.elem() {
                SIMDElem::Int(int) => type_il(&Type::Int(int), asm),
                SIMDElem::Float(float) => type_il(&Type::Float(float), asm),
            };
            format!("valuetype [System.Runtime.Intrinsics]System.Runtime.Intrinsics.Vector{vec_bits}`1<{elem}>")
        }
        Type::Ptr(inner) => format!("{}*", type_il(&asm[*inner], asm)),
        Type::Ref(inner) => format!("{}&", type_il(&asm[*inner], asm)),
        Type::Int(int) => match int {
            super::Int::U8 => "uint8".into(),
            super::Int::U16 => "uint16".into(),
            super::Int::U32 => "uint32".into(),
            super::Int::U64 => "uint64".into(),
            super::Int::U128 => "valuetype [System.Runtime]System.UInt128".into(),
            super::Int::USize => "native uint".into(),
            super::Int::I8 => "int8".into(),
            super::Int::I16 => "int16".into(),
            super::Int::I32 => "int32".into(),
            super::Int::I64 => "int64".into(),
            super::Int::I128 => "valuetype [System.Runtime]System.Int128".into(),
            super::Int::ISize => "native int".into(),
        },
        Type::ClassRef(cref) => {
            // `System.Object` and `System.String` have dedicated CLI element types
            // (ELEMENT_TYPE_OBJECT / ELEMENT_TYPE_STRING). BCL method signatures are encoded
            // with those, so a plain `class [System.Runtime]System.Object` typeref does NOT
            // match `object` during runtime method resolution -> MissingMethodException
            // (this is why e.g. `GCHandle.Alloc(object)` failed to bind). Emit the canonical
            // element type whenever such a ClassRef appears in *type-signature* position.
            // Declaring-type position goes through `class_ref` (not this fn) and is unaffected.
            let cr = asm.class_ref(*cref);
            if !cr.is_valuetype() && cr.generics().is_empty() {
                match &asm[cr.name()] {
                    "System.Object" => return "object".into(),
                    "System.String" => return "string".into(),
                    _ => {}
                }
            }
            class_ref(*cref, asm)
        }
        Type::Float(float) => match float {
            super::Float::F16 => "valuetype [System.Runtime]System.Half".into(),
            super::Float::F32 => "float32".into(),
            super::Float::F64 => "float64".into(),

            super::Float::F128 => "valuetype f128".into(),
        },
        Type::PlatformChar => "char".into(),
        Type::PlatformGeneric(arg, generic) => match generic {
            super::tpe::GenericKind::MethodGeneric => format!("!{arg}"),
            super::tpe::GenericKind::CallGeneric => format!("!!{arg}"),
            super::tpe::GenericKind::TypeGeneric => format!("!{arg}"),
        },
        Type::Bool => "bool".into(),
        Type::Void => "void".into(),
        Type::PlatformArray { elem, dims } => format!(
            "{elem}[{dims}]",
            elem = type_il(&asm[*elem], asm),
            dims = (1..(dims.get())).map(|_| ',').collect::<String>()
        ),
        Type::FnPtr(sig) => {
            let sig = asm[*sig].clone();
            format!(
                "method {output}*({inputs})",
                output = type_il(sig.output(), asm),
                inputs = sig
                    .inputs()
                    .iter()
                    .map(|tpe| non_void_type_il(tpe, asm))
                    .intersperse(",".to_string())
                    .collect::<String>(),
            )
        }
        Type::PlatformString => "string".into(),
        Type::PlatformObject => "object".into(),
    }
}

/// Like [`non_void_type_il`], but for a method's OWN declared return/parameter types (the `.method`
/// header line) rather than a body-instruction operand, a `calli` signature, a field declaration, or a
/// locals entry. See [`type_il_signature`] for why this split exists and why it must stay narrow.
fn non_void_type_il_signature(tpe: &Type, asm: &Assembly) -> String {
    match tpe {
        Type::Void => "valuetype RustVoid".into(),
        _ => type_il_signature(tpe, asm),
    }
}

/// Like [`type_il`], but renders a `Type::ClassRef`'s assembly qualifier through
/// [`ref_assembly_name`] — the same CoreLib-impl -> `System.Runtime`-ref substitution already applied
/// to the `.assembly extern` table and base-type `extends` clauses (see that fn's doc comment for the
/// full rationale). A method's own `.method {ret} 'name'(...)` header line is C#-visible metadata a
/// separately-compiled consumer resolves a call against — exactly like those two cases, and unlike
/// every other `type_il`/`non_void_type_il` call site in this file (body instructions, `calli`
/// signatures, field declarations, locals), which are either invisible to a C# compiler (bodies are
/// never read) or would be genuinely JIT-rejected if re-qualified this way (a real CoreLib
/// `System.String`/`System.Object` instance method resolved via `[System.Runtime]` inside a body is
/// "Bad IL format" — see `class_ref`'s doc comment). This function must therefore be called ONLY from
/// the two call sites that emit a method's declared signature, never from a body/calli/field/locals
/// position.
///
/// Only `Type::ClassRef` differs from [`type_il`]; every other arm recurses back into the plain
/// `type_il`/`non_void_type_il` (nested types inside a signature, e.g. an array element or generic
/// argument, are exceedingly unlikely to be a cross-assembly-forwarded BCL type in this codebase's
/// supported surface, and doing so keeps this function's diff minimal and easy to audit against
/// `type_il` — a full recursive parallel copy would double the maintenance surface for no currently
/// exercised case).
fn type_il_signature(tpe: &Type, asm: &Assembly) -> String {
    match tpe {
        Type::ClassRef(cref) => {
            let cr = asm.class_ref(*cref);
            if !cr.is_valuetype() && cr.generics().is_empty() {
                match &asm[cr.name()] {
                    "System.Object" => return "object".into(),
                    "System.String" => return "string".into(),
                    _ => {}
                }
            }
            let raw_cref = asm.class_ref(*cref);
            let name = dotnet_class_name(&asm[raw_cref.name()]);
            let prefix = if raw_cref.is_valuetype() { "valuetype" } else { "class" };
            let generic_list = if raw_cref.generics().is_empty() {
                String::new()
            } else {
                format!(
                    "<{generics}>",
                    generics = raw_cref
                        .generics()
                        .iter()
                        .map(|tpe| type_il(tpe, asm))
                        .intersperse(",".to_string())
                        .collect::<String>()
                )
            };
            let generic_postfix = if raw_cref.generics().is_empty() {
                String::new()
            } else {
                format!("`{}", raw_cref.generics().len())
            };
            if let Some(assembly) = raw_cref.asm() {
                let raw_name = &asm[raw_cref.name()];
                format!(
                    "{prefix} [{assembly}]'{name}{generic_postfix}'{generic_list}",
                    assembly = ref_assembly_name_for_type(&asm[assembly], raw_name)
                )
            } else {
                format!("{prefix} '{name}{generic_postfix}'{generic_list}")
            }
        }
        _ => type_il(tpe, asm),
    }
}

/// Cached runtime configuration string, obtained from calling the .NET runtime.
#[must_use]
pub fn get_runtime_config() -> &'static str {
    RUNTIME_CONFIG.as_ref()
}

/// Cached runtime configuration file, obtained from calling the .NET runtime.
static RUNTIME_CONFIG: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let info = std::process::Command::new("dotnet")
        .arg("--info")
        .output()
        .expect("Could not run `dotnet --info`");
    if !info.stderr.is_empty() {
        let stderr = std::str::from_utf8(&info.stderr).expect("Error message not utf8");
        panic!("dotnet --info panicked with {stderr}")
    }
    let info = std::str::from_utf8(&info.stdout).expect("Error message not utf8");
    let version_start = info.find("Host:").unwrap_or_default();
    let version_start = version_start + info[version_start..].find("Version:").unwrap();
    let version_start = version_start + "Version:".len();
    let version_end = info.find("Architecture:").unwrap();
    let version = &info[version_start..version_end].trim();
    // TFM tracks the target .NET version (default net8.0); the framework version stays the live
    // host scrape — this config feeds the `::stable` test harness (compile_test.rs), which always
    // runs the default Net8, so this is byte-identical there. (The cargo-dotnet *bin* path uses the
    // linker's jumpstart runtimeconfig, which is version-parameterised separately.)
    let tfm = crate::ir::dotnet_version().tfm();
    format!(
        "{{
        \"runtimeOptions\": {{
          \"tfm\": \"{tfm}\",
          \"framework\": {{
            \"name\": \"Microsoft.NETCore.App\",
            \"version\": \"{version}\"
          }},
          \"configProperties\": {{
            \"System.Threading.ThreadPool.MinThreads\": 4,
            \"System.Threading.ThreadPool.MaxThreads\": 25
          }}
        }}
      }}"
    )
});
