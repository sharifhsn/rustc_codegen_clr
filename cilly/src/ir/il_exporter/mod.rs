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

pub struct ILExporter {
    flavour: IlasmFlavour,
    is_lib: bool,
    /// The .NET assembly name to emit in the `.assembly` directive. `None` keeps the legacy `_`
    /// placeholder (used for executables, where the assembly is loaded by file path via the native
    /// launcher and the name is irrelevant). A library passes its crate name here so C# can reference
    /// the produced `.dll` by a real assembly identity.
    asm_name: Option<String>,
}
impl ILExporter {
    #[must_use]
    pub fn new(flavour: IlasmFlavour, is_lib: bool, asm_name: Option<String>) -> Self {
        Self {
            flavour,
            is_lib,
            asm_name,
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
                // CoreLib/mscorlib are now normalized to System.Runtime, so they fall through to `_`.
                let (ver, token) = (dv_ver, "B0 3F 5F 7F 11 D5 0A 3A");
                writeln!(
                    out,
                    ".assembly extern '{ext}' {{ .ver {ver} .publickeytoken = ({token}) }}"
                )?;
            }
        }
        for (const_data, idx) in asm.const_data.1.iter() {
            let encoded = encode(idx.inner() as u64);
            let data: String = const_data.iter().map(|u| format!("{u:x} ")).collect();
            writeln!(out, " .data cil I_{encoded} = bytearray ({data})\n.field assembly static uint8 c_{encoded} at I_{encoded}")?;
        }
        let mut c = 0;
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
            let name = dotnet_class_name(&asm[class_def.name()]);
            writeln!(
                out,
                ".class {vis} ansi {sealed} {explicit} '{name}' extends {extends}{{"
            )?;
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
                                b.to_le_bytes()
                                    .iter()
                                    .map(|v| format!(" {v}"))
                                    .collect::<String>()
                            )?;
                            format!(" at C_{c}")
                        }
                        _ => todo!("unhandled const {default_value:?}"),
                    }
                } else {
                    "".into()
                };
                writeln!(
                    out,
                    ".field static {is_const} {tpe} '{name}'{default_value}"
                )?;
                if *is_tls {
                    writeln!(out,".custom instance void [System.Runtime]System.ThreadStaticAttribute::.ctor() = (01 00 00 00)")?;
                };
            }
            // Debug check
            let mut ensure_unqiue: std::collections::HashSet<MethodDefIdx> =
                std::collections::HashSet::new();
            // Export all methods

            for method_id in class_def.methods() {
                let method = asm.method_def(*method_id);
                let vis = match method.access() {
                    crate::Access::Extern | crate::Access::Public => "public",
                    crate::Access::Private => "private",
                };
                let kind = match method.kind() {
                    crate::cilnode::MethodKind::Static => "static",
                    crate::cilnode::MethodKind::Instance => "instance",
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
                let ret = type_il(sig.output(), asm);
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
                            format!("{} '{}'", non_void_type_il(tpe, asm_mut), &asm_mut[*name])
                        }
                        None => non_void_type_il(tpe, asm_mut),
                    })
                    .intersperse(",".to_string())
                    .collect();
                let preservesig = if method.implementation().is_extern() {
                    "preservesig"
                } else {
                    ""
                };
                writeln!(
                    out,
                    ".method {vis} hidebysig {kind} {pinvoke} {ret} '{name}'({inputs}) cil managed {preservesig}{{// Method ID {method_id:?}"
                )?;
                debug_assert!(ensure_unqiue.insert(*method_id));
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
            }
            writeln!(out, "}}")?;
        }

        Ok(())
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
                    writeln!(out,"ldsflda uint8 c_{}", encode(data.inner() as u64))
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
                let class = class_ref(mref.class(), asm);
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
                let class = class_ref(mref.class(), asm);
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
                let class = class_ref(mref.class(), asm);

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
                        super::Float::F16 => todo!(),
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
        }
    }
}
#[cfg(not(target_os = "windows"))]
fn assemble_file(exe_out: &Path, il_path: &Path, is_lib: bool) {
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
    let out = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !(stderr.contains("\nError\n") || stderr.contains("FAILURE") || stdout.contains("FAILURE")),
        "stdout:{} stderr:{} cmd:{cmd:?}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
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
    let out = cmd.output().unwrap();
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
    let out = cmd.output().unwrap();
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
fn simple_class_ref(cref: Interned<ClassRef>, asm: &Assembly) -> String {
    let cref = asm.class_ref(cref);
    let name = dotnet_class_name(&asm[cref.name()]);
    if let Some(assembly) = cref.asm() {
        format!("[{assembly}]'{name}'", assembly = ref_assembly_name(&asm[assembly]))
    } else {
        format!("'{name}'")
    }
}
pub(crate) fn class_ref(cref: Interned<ClassRef>, asm: &Assembly) -> String {
    let cref = asm.class_ref(cref);
    let name = dotnet_class_name(&asm[cref.name()]);
    let prefix = if cref.is_valuetype() {
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
