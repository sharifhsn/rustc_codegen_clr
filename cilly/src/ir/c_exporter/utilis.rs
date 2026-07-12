use crate::utilis::encode;

use super::super::{Assembly, Float, Int, MethodRef, Type, asm::MAIN_MODULE, method::LocalDef};

use fxhash::{FxHashSet, FxHasher, hash64};
/// Finds the name of this local
pub(super) fn local_name(locals: &[LocalDef], asm: &Assembly, loc: u32) -> String {
    // If the name of this local repeats, use the L form.
    if locals
        .iter()
        .filter(|(name, _)| *name == locals[loc as usize].0)
        .count()
        > 1
    {
        return format!("L{loc}");
    }
    match locals[loc as usize].0 {
        Some(local_name) => {
            let ident = escape_nonfn_name(&asm[local_name]);
            match ident.as_str() {
                "socket" => {
                    format!("i{}", encode(hash64(&ident)))
                }
                _ => ident,
            }
        }
        None => format!("L{loc}"),
    }
}

/// Escapes a given identifier
pub(super) fn escape_ident(ident: &str) -> String {
    let mut escaped = ident
        .replace(['.', ' '], "_")
        .replace('~', "_tilda_")
        .replace('=', "_eq_")
        .replace("#", "_pound_")
        .replace(":", "_col_")
        .replace("[", "_srpar_")
        .replace("]", "_slpar_")
        .replace("(", "_rpar_")
        .replace(")", "_lpar_")
        .replace("{", "_rbra_")
        .replace("}", "_lbra_");
    if *super::ASCII_IDENTS {
        escaped = escaped.replace("$", "_dsig_");
    }
    if escaped.chars().next().unwrap().is_numeric() {
        escaped = format!("p{escaped}");
    }
    // Check if reserved.
    match escaped.as_str() {
        "int" | "default" | "float" | "double" | "long" | "short" | "register" | "stderr"
        | "environ" | "struct" | "union" | "linux" | "inline" | "asm" | "signed" | "unsigned"
        | "bool" | "char" | "case" | "switch" | "volatile" | "auto" | "void" | "unix" => {
            format!("i{}", encode(hash64(&escaped)))
        }
        _ => escaped,
    }
}
/// Returns a string representing this type, with special handling for `void`.
/// In other cases, this is equivalent to [`c_tpe`].
pub(super) fn nonvoid_c_type(field_tpe: Type, asm: &Assembly) -> String {
    match field_tpe {
        Type::Void => "RustVoid".into(),
        _ => c_tpe(field_tpe, asm),
    }
}
/// Returns a string repesenting this type in C.
pub(super) fn c_tpe(field_tpe: Type, asm: &Assembly) -> String {
    match field_tpe {
        Type::Ptr(type_idx) | Type::Ref(type_idx) => format!("{}*", c_tpe(asm[type_idx], asm)),
        Type::Int(int) => match int {
            Int::U8 => "uint8_t".into(),
            Int::U16 => "uint16_t".into(),
            Int::U32 => "uint32_t".into(),
            Int::U64 => "uint64_t".into(),
            Int::U128 => "__uint128_t".into(),
            Int::USize => "uintptr_t".into(),
            Int::I8 => "int8_t".into(),
            Int::I16 => "int16_t".into(),
            Int::I32 => "int32_t".into(),
            Int::I64 => "int64_t".into(),
            Int::I128 => "__int128".into(),
            Int::ISize => "intptr_t".into(),
        },
        Type::ClassRef(class_ref_idx) => {
            // The CLR spells 128-bit integers as BCL value types, while the C
            // backend represents the same semantic types with the compiler's
            // native integer extensions.  Pointer casts and indirect stores
            // can retain the BCL spelling even when their values have already
            // been normalized to `Type::Int`; keep both spellings identical at
            // the C type boundary instead of inventing an undefined union.
            let class_ref = &asm[class_ref_idx];
            if class_ref.is_valuetype()
                && class_ref.asm().map(|name| asm[name].as_ref()) == Some("System.Runtime")
            {
                match asm[class_ref.name()].as_ref() {
                    "System.Int128" => return "__int128".into(),
                    "System.UInt128" => return "__uint128_t".into(),
                    _ => {}
                }
            }
            if asm.class_ref_to_def(class_ref_idx).is_some_and(|def| {
                asm[def].has_nonveralpping_layout() && asm[def].explict_size().is_some()
            }) {
                format!("struct {}", escape_ident(&asm[asm[class_ref_idx].name()]))
            } else {
                format!("union {}", escape_ident(&asm[asm[class_ref_idx].name()]))
            }
        }
        Type::Float(float) => match float {
            Float::F16 => "_Float16".into(),
            Float::F32 => "float".into(),
            Float::F64 => "double".into(),
            Float::F128 => "_Float128".into(),
        },
        Type::PlatformString => "char*".into(),
        Type::PlatformChar => "char".into(),
        Type::PlatformGeneric(_, generic_kind) => todo!(),
        Type::PlatformObject => "void*".into(),
        Type::Bool => "bool".into(),
        Type::Void => "void".into(),
        Type::PlatformArray { elem, dims } => format!(
            "{elem}{dims}",
            elem = c_tpe(asm[elem], asm),
            dims = "*".repeat(dims.get() as usize)
        ),
        Type::FnPtr(_) => "void*".into(),
        Type::SIMDVector(vec) => {
            format!(
                "__simdvec{elem}_{count}",
                elem = std::convert::Into::<Type>::into(vec.elem()).mangle(asm),
                count = vec.count()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ClassRef;

    #[test]
    fn bcl_int128_class_refs_share_the_native_c_spelling() {
        let mut asm = Assembly::default();
        let i128 = ClassRef::int_128(&mut asm);
        let u128 = ClassRef::uint_128(&mut asm);

        assert_eq!(c_tpe(Type::ClassRef(i128), &asm), "__int128");
        assert_eq!(c_tpe(Type::ClassRef(u128), &asm), "__uint128_t");
    }
}
/// Gets the name of a given method, wiht special handling for intriniscs.
pub(super) fn mref_to_name(mref: &MethodRef, asm: &Assembly) -> String {
    let class = &asm[mref.class()];
    let class_name = escape_nonfn_name(&asm[class.name()]);
    let mname = escape_ident(&asm[mref.name()]);
    if class.asm().is_some()
        || matches!(mref.output(asm), Type::SIMDVector(_))
        || mref
            .stack_inputs(asm)
            .iter()
            .any(|tpe| matches!(tpe, Type::SIMDVector(_)))
        || mname == "transmute"
        || mname == "create_slice"
        || mname == "unaligned_read"
        || mname == "ovf_check_tuple"
        || mname == "_Unwind_Backtrace"
    {
        let mangled = escape_ident(
            &asm[mref.sig()]
                .iter_types()
                .map(|tpe| tpe.mangle(asm))
                .collect::<String>(),
        );

        let stem = class_member_name(&class_name, &mname);
        format!("{stem}{mangled}")
    } else {
        class_member_name(&class_name, &mname)
    }
}
/// Gets the name of a member of a given class.
pub(super) fn class_member_name(class_name: &str, method_name: &str) -> String {
    if class_name == MAIN_MODULE {
        method_name.into()
    } else {
        format!("{class_name}_{method_name}")
    }
}
pub(super) fn escape_nonfn_name(name: &str) -> String {
    let res = escape_ident(name);
    match res.as_ref() {
        "sigaction" => "sigactn".to_owned(),
        "sigaltstack" => "sigaltstck".to_owned(),
        _ => res,
    }
}
