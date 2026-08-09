//! Direct ECMA-335 PE emission — writes the final `.dll`/`.exe` (and, later, the Portable PDB)
//! straight from the interned IR, with no textual `.il` and no external `ilasm`.
//!
//! Design, construct inventory, phasing, and validation strategy: `docs/PE_EMISSION_PLAN.md`.
//! The [`il_exporter`](super::il_exporter) remains the default until this path survives the full
//! `::stable` gate under the `DIRECT_PE=1` A/B differential; the emitted subset of ECMA-335 is
//! exactly the subset `il_exporter` emits today — nothing more.
//!
//! Layout of the writer (each stage is independently unit-tested):
//! * [`crate::ir::pe_exporter::heaps`] — the four metadata heaps (`#Strings`, `#Blob`, `#GUID`, `#US`), interned + deduped.
//! * [`crate::ir::pe_exporter::sig`] — `Type` → `ELEMENT_TYPE_*` signature-blob encoding (fields, methods, locals,
//!   `MethodSpec`, `calli` stand-alone sigs).
//! * [`crate::ir::pe_exporter::tables`] — the metadata tables + coded-index/heap-index width computation and the
//!   populate → size → serialize pipeline. *(Phase 1a: implemented + unit-tested)*
//! * [`crate::ir::pe_exporter::body`] — method bodies: tiny/fat headers, opcode bytes, branch layout, fat EH sections.
//!   *(Phase 1a: implemented + unit-tested)*
//! * [`crate::ir::pe_exporter::pe`] — the PE/COFF container and CLI header, including the native `mscoree.dll`
//!   `_CorExeMain` bootstrap stub (IAT/Import Table/`.reloc`) an `.exe` needs to satisfy the OS's
//!   native PE loader before the CLR ever inspects the CLI header. *(Phase 1a: implemented +
//!   unit-tested)*
//! * [`crate::ir::pe_exporter::export`] — `export_pe`: the top-level driver wiring `tables::MetadataBuilder` +
//!   `body::assemble_method` + the RVA layout pass + `pe::write_pe` into one entry point.
//!   *(Phase 1a MILESTONE PROVEN 2026-07-02: a hand-built static-entrypoint-calling-
//!   `Console.WriteLine` `Assembly`, exported with no `ilasm` anywhere, loads and runs under a
//!   real `dotnet` host — `export::tests::e2e_hand_built_assembly_runs_under_dotnet`. Only the
//!   inventory subset that test exercises is wired; const-data `FieldRVA` blobs, non-`ByteBuffer`
//!   static-field defaults, and `MainModule` method-count partitioning are loud `todo!()`s left
//!   for Phase 1b — see `export`'s module doc.)*
//! * [`crate::ir::pe_exporter::pdb`] — Portable PDB (dotnet/runtime `PortablePdb-Metadata.md`): `#Pdb` stream +
//!   `Document`/`MethodDebugInformation` tables from `CILRoot::SourceFileInfo` sequence points,
//!   plus the PE-side Debug Directory (CodeView/RSDS) hook. *(Phase 2: interface-pinning stub —
//!   see that module's doc for the parity bar against `il_exporter`'s `.line` + `ilasm -debug`.)*

pub mod body;
pub mod export;
pub mod heaps;
pub mod pdb;
pub mod pe;
pub mod sig;
pub mod tables;

use fxhash::FxHashSet;

use super::{
    Assembly, CILNode, CILRoot, ClassRef, Const, Float, FnSig, Int, Type, bimap::Interned,
};

const MAIN_MODULE_METHOD_LIMIT: usize = 60_000;

/// A retained IR construct which the direct-PE writer cannot encode yet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeTargetError {
    construct: &'static str,
    detail: String,
}

impl PeTargetError {
    fn unsupported(construct: &'static str, detail: impl Into<String>) -> Self {
        Self {
            construct,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn construct(&self) -> &'static str {
        self.construct
    }
}

impl std::fmt::Display for PeTargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "direct-PE target does not support {}: {}",
            self.construct, self.detail
        )
    }
}

impl std::error::Error for PeTargetError {}

struct PeCapabilityValidator<'a> {
    asm: &'a Assembly,
    types: FxHashSet<Interned<Type>>,
    sigs: FxHashSet<Interned<FnSig>>,
    classes: FxHashSet<Interned<ClassRef>>,
}

impl<'a> PeCapabilityValidator<'a> {
    fn new(asm: &'a Assembly) -> Self {
        Self {
            asm,
            types: FxHashSet::default(),
            sigs: FxHashSet::default(),
            classes: FxHashSet::default(),
        }
    }

    fn check_type(&mut self, tpe: Type) -> Result<(), PeTargetError> {
        match tpe {
            Type::Float(Float::F128) => Err(PeTargetError::unsupported(
                "f128 type",
                "the PE signature encoder has no f128 TypeDef/signature representation",
            )),
            Type::Ptr(inner) | Type::Ref(inner) | Type::PlatformArray { elem: inner, .. } => {
                self.check_type_id(inner)
            }
            Type::FnPtr(sig) => self.check_sig_id(sig),
            Type::ClassRef(class) => self.check_class_id(class),
            Type::SIMDVector(vector) => self.check_type(vector.elem().into()),
            Type::Int(_)
            | Type::Float(Float::F16 | Float::F32 | Float::F64)
            | Type::PlatformString
            | Type::PlatformChar
            | Type::PlatformGeneric(_, _)
            | Type::PlatformObject
            | Type::Bool
            | Type::Void => Ok(()),
        }
    }

    fn check_type_id(&mut self, tpe: Interned<Type>) -> Result<(), PeTargetError> {
        if !self.types.insert(tpe) {
            return Ok(());
        }
        self.check_type(self.asm[tpe])
    }

    fn check_sig(&mut self, sig: &FnSig) -> Result<(), PeTargetError> {
        for input in sig.inputs() {
            self.check_type(*input)?;
        }
        self.check_type(*sig.output())
    }

    fn check_sig_id(&mut self, sig: Interned<FnSig>) -> Result<(), PeTargetError> {
        if !self.sigs.insert(sig) {
            return Ok(());
        }
        self.check_sig(&self.asm[sig])
    }

    fn check_class_id(&mut self, class: Interned<ClassRef>) -> Result<(), PeTargetError> {
        if !self.classes.insert(class) {
            return Ok(());
        }
        for generic in self.asm[class].generics() {
            self.check_type(*generic)?;
        }
        Ok(())
    }

    fn check_node(&mut self, node: &CILNode) -> Result<(), PeTargetError> {
        match node {
            CILNode::IntCast { target, .. } if matches!(target, Int::I128 | Int::U128) => {
                Err(PeTargetError::unsupported(
                    "128-bit integer cast",
                    format!("unsupported target {target:?} in {node:?}"),
                ))
            }
            CILNode::FloatCast { target, .. } if matches!(target, Float::F16 | Float::F128) => {
                Err(PeTargetError::unsupported(
                    "extended-precision float cast",
                    format!("unsupported target {target:?} in {node:?}"),
                ))
            }
            CILNode::LdInd { tpe, .. } => match self.asm[*tpe] {
                Type::Ref(_) => Err(PeTargetError::unsupported(
                    "ldind of managed-reference type",
                    format!("{node:?}"),
                )),
                Type::PlatformGeneric(_, _) => Err(PeTargetError::unsupported(
                    "ldind of generic-parameter type",
                    format!("{node:?}"),
                )),
                Type::Void => Err(PeTargetError::unsupported(
                    "ldind of void",
                    format!("{node:?}"),
                )),
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }

    fn check_root(&mut self, root: &CILRoot) -> Result<(), PeTargetError> {
        match root {
            CILRoot::StInd(boxed) => {
                self.check_type(boxed.2)?;
                match boxed.2 {
                    Type::Ref(_) => Err(PeTargetError::unsupported(
                        "stind of managed-reference type",
                        format!("{root:?}"),
                    )),
                    Type::PlatformGeneric(_, _) => Err(PeTargetError::unsupported(
                        "stind of generic-parameter type",
                        format!("{root:?}"),
                    )),
                    _ => Ok(()),
                }
            }
            _ => Ok(()),
        }
    }

    fn check_owned_generic_indices(
        &self,
        tpe: Type,
        method_generic_count: u32,
        class_generic_count: u32,
        method_name: &str,
        seen: &mut FxHashSet<Type>,
    ) -> Result<(), PeTargetError> {
        use super::tpe::GenericKind;

        if !seen.insert(tpe) {
            return Ok(());
        }
        match tpe {
            Type::PlatformGeneric(index, GenericKind::CallGeneric)
                if index >= method_generic_count =>
            {
                Err(PeTargetError::unsupported(
                    "method generic parameter index",
                    format!(
                        "method `{method_name}` declares {method_generic_count} generic parameter(s) but its signature references !!{index}"
                    ),
                ))
            }
            Type::PlatformGeneric(index, GenericKind::TypeGeneric | GenericKind::MethodGeneric)
                if index >= class_generic_count =>
            {
                Err(PeTargetError::unsupported(
                    "owner generic parameter index",
                    format!(
                        "method `{method_name}` belongs to a type with {class_generic_count} generic parameter(s) but its signature references !{index}"
                    ),
                ))
            }
            Type::Ptr(inner) | Type::Ref(inner) | Type::PlatformArray { elem: inner, .. } => self
                .check_owned_generic_indices(
                    self.asm[inner],
                    method_generic_count,
                    class_generic_count,
                    method_name,
                    seen,
                ),
            Type::ClassRef(class) => {
                for &generic in self.asm[class].generics() {
                    self.check_owned_generic_indices(
                        generic,
                        method_generic_count,
                        class_generic_count,
                        method_name,
                        seen,
                    )?;
                }
                Ok(())
            }
            Type::FnPtr(sig) => {
                for nested in self.asm[sig].iter_types() {
                    self.check_owned_generic_indices(
                        nested,
                        method_generic_count,
                        class_generic_count,
                        method_name,
                        seen,
                    )?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn check_class(&mut self, class: &super::ClassDef) -> Result<(), PeTargetError> {
        let class_name = &self.asm[class.name()];
        if class_name == super::asm::MAIN_MODULE && class.methods().len() > MAIN_MODULE_METHOD_LIMIT
        {
            return Err(PeTargetError::unsupported(
                "MainModule method count",
                format!(
                    "MainModule has {} methods (limit {MAIN_MODULE_METHOD_LIMIT}); direct PE does not yet implement method partitioning",
                    class.methods().len()
                ),
            ));
        }
        for &method_id in class.methods() {
            let method = self.asm.method_defs().get(&method_id).ok_or_else(|| {
                PeTargetError::unsupported(
                    "dangling class method",
                    format!("type `{class_name}` references absent method {method_id:?}"),
                )
            })?;
            let method_name = &self.asm[method.name()];
            let method_generic_count =
                u32::try_from(method.generic_params().len()).map_err(|_| {
                    PeTargetError::unsupported(
                        "method generic arity",
                        format!("method `{method_name}` has more than u32::MAX generic parameters"),
                    )
                })?;
            let mut seen = FxHashSet::default();
            for tpe in self.asm[method.sig()].iter_types() {
                self.check_owned_generic_indices(
                    tpe,
                    method_generic_count,
                    class.generics(),
                    method_name,
                    &mut seen,
                )?;
            }
        }
        for tpe in class.iter_types() {
            self.check_type(tpe)?;
        }
        for field in class.static_fields() {
            let Some(default) = field.default_value else {
                continue;
            };
            if matches!(
                default,
                Const::PlatformString(_) | Const::Null(_) | Const::ByteBuffer { .. }
            ) {
                return Err(PeTargetError::unsupported(
                    "static-field default value",
                    format!(
                        "field `{}` has unsupported initializer {default:?}",
                        &self.asm[field.name]
                    ),
                ));
            }
        }
        if let Some(enum_def) = class.enum_def() {
            for (name, value) in enum_def.variants() {
                if !matches!(
                    value,
                    Const::I8(_)
                        | Const::U8(_)
                        | Const::I16(_)
                        | Const::U16(_)
                        | Const::I32(_)
                        | Const::U32(_)
                        | Const::I64(_)
                        | Const::U64(_)
                ) {
                    return Err(PeTargetError::unsupported(
                        "CLR enum literal",
                        format!(
                            "variant `{}` has unsupported value {value:?}",
                            &self.asm[*name]
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Checks every retained type/signature/node/root before direct PE emission. The inventory mirrors
/// every currently unsupported `todo!()` arm in `body.rs` and `sig.rs`, turning those cases into a
/// structured error before the byte writer runs.
pub fn validate_for_pe(asm: &Assembly) -> Result<(), PeTargetError> {
    let mut validator = PeCapabilityValidator::new(asm);
    for tpe in asm.iter_type_values() {
        validator.check_type(*tpe)?;
    }
    for sig in asm.iter_signatures() {
        validator.check_sig(sig)?;
    }
    for class in asm.iter_class_refs() {
        for generic in class.generics() {
            validator.check_type(*generic)?;
        }
    }
    for field in asm.iter_field_descs() {
        validator.check_type(field.tpe())?;
    }
    for field in asm.iter_static_field_descs() {
        validator.check_type(field.tpe())?;
    }
    for class in asm.class_defs().values() {
        validator.check_class(class)?;
    }
    for node in asm.iter_nodes() {
        validator.check_node(node)?;
    }
    for root in asm.iter_roots() {
        validator.check_root(root)?;
    }
    Ok(())
}

/// Checks definition-owned target constraints without walking intern arenas. This is safe before
/// compaction because class definitions are themselves retained roots, and lets obviously
/// unsupported shapes (notably an unpartitioned 60k-method MainModule) fail without first copying
/// the entire graph.
pub(crate) fn validate_retained_definitions_for_pe(asm: &Assembly) -> Result<(), PeTargetError> {
    let mut validator = PeCapabilityValidator::new(asm);
    for class in asm.class_defs().values() {
        validator.check_class(class)?;
    }
    Ok(())
}

#[cfg(test)]
mod capability_tests {
    use super::*;
    use crate::{
        Access, Const, MethodDef, MethodImpl, VerificationFailure,
        cilnode::{ExtendKind, MethodKind},
        tpe::GenericKind,
    };

    fn options() -> export::ExportOptions {
        export::ExportOptions {
            runtime: rust_dotnet_sdk_core::runtime::DotnetVersion::Net10,
            is_dll: true,
            assembly_name: "preflight-test".to_string(),
            public_module_full_name: None,
            module_name: "preflight-test.dll".to_string(),
            pdb_file_name: String::new(),
        }
    }

    #[test]
    fn rejects_every_direct_pe_todo_family_before_emission() {
        let mut asm = Assembly::default();
        let value = asm.alloc_node(Const::I32(1));
        asm.alloc_node(CILNode::IntCast {
            input: value,
            target: Int::I128,
            extend: ExtendKind::SignExtend,
        });
        let error = validate_for_pe(&asm).unwrap_err();
        assert_eq!(error.construct(), "128-bit integer cast");

        let mut asm = Assembly::default();
        let value = asm.alloc_node(Const::I32(1));
        asm.alloc_node(CILNode::FloatCast {
            input: value,
            target: Float::F16,
            is_signed: true,
        });
        let error = validate_for_pe(&asm).unwrap_err();
        assert_eq!(error.construct(), "extended-precision float cast");

        let mut asm = Assembly::default();
        asm.sig([], Type::Float(Float::F128));
        let error = validate_for_pe(&asm).unwrap_err();
        assert_eq!(error.construct(), "f128 type");

        let mut asm = Assembly::default();
        let vector = crate::tpe::simd::SIMDVector::new(Float::F128.into(), 1);
        asm.sig([], Type::SIMDVector(vector));
        let error = validate_for_pe(&asm).unwrap_err();
        assert_eq!(error.construct(), "f128 type");

        let mut asm = Assembly::default();
        let i32_tpe = asm.alloc_type(Type::Int(Int::I32));
        let managed_ref = asm.alloc_type(Type::Ref(i32_tpe));
        let addr = asm.alloc_node(CILNode::LdArg(0));
        asm.alloc_node(CILNode::LdInd {
            addr,
            tpe: managed_ref,
            volatile: false,
        });
        let error = validate_for_pe(&asm).unwrap_err();
        assert_eq!(error.construct(), "ldind of managed-reference type");

        let mut asm = Assembly::default();
        let addr = asm.alloc_node(Const::USize(0));
        let value = asm.alloc_node(Const::I32(0));
        asm.alloc_root(CILRoot::StInd(Box::new((
            addr,
            value,
            Type::PlatformGeneric(0, crate::tpe::GenericKind::MethodGeneric),
            false,
        ))));
        let error = validate_for_pe(&asm).unwrap_err();
        assert_eq!(error.construct(), "stind of generic-parameter type");
    }

    #[test]
    fn supported_wide_constants_and_half_signatures_pass_preflight() {
        let mut asm = Assembly::default();
        asm.alloc_node(Const::I128(42));
        asm.sig([Type::Float(Float::F16)], Type::Int(Int::I128));
        assert!(validate_for_pe(&asm).is_ok());
    }

    #[test]
    fn unsupported_static_default_is_a_structured_preflight_error() {
        let mut asm = Assembly::default();
        let text = asm.alloc_string("not an RVA scalar");
        let owner = asm.main_module();
        asm.add_static(
            Type::PlatformString,
            "INVALID_DEFAULT",
            false,
            owner,
            Some(Const::PlatformString(text)),
            false,
        );

        let error = validate_for_pe(&asm).unwrap_err();
        assert_eq!(error.construct(), "static-field default value");
    }

    #[test]
    fn render_preflight_rejects_generic_indices_outside_their_owner_arity() {
        let mut asm = Assembly::default();
        let owner = asm.main_module();
        let signature = asm.sig(
            [Type::PlatformGeneric(0, GenericKind::TypeGeneric)],
            Type::Void,
        );
        let name = asm.alloc_string("bad_owner_generic");
        asm.new_method(
            MethodDef::new(
                Access::Private,
                owner,
                name,
                signature,
                MethodKind::Static,
                MethodImpl::Missing,
                vec![],
            )
            .with_abstract(),
        );

        let ready = asm.verify_for_export().unwrap();
        let error = ready.try_render_pe(&options()).unwrap_err();
        let crate::PeEmissionError::Target(error) = error else {
            panic!("generic arity must fail during PE target preflight")
        };
        assert_eq!(error.construct(), "owner generic parameter index");
    }

    #[test]
    fn render_preflight_rejects_main_module_before_the_partition_limit_panics() {
        let mut asm = Assembly::default();
        let main = asm.main_module();
        asm.add_abstract_methods_bulk_for_test(main, MAIN_MODULE_METHOD_LIMIT + 1);

        let ready = asm.verify_for_export().unwrap();
        let error = ready.try_render_pe(&options()).unwrap_err();
        let crate::PeEmissionError::Target(error) = error else {
            panic!("MainModule overflow must fail during PE target preflight")
        };
        assert_eq!(error.construct(), "MainModule method count");
    }

    #[test]
    fn legacy_render_pe_signature_remains_source_compatible() {
        let _render: fn(
            crate::ExportReadyAssembly,
            &export::ExportOptions,
        ) -> Result<(Vec<u8>, Vec<u8>), VerificationFailure> =
            crate::ExportReadyAssembly::render_pe;
    }
}
