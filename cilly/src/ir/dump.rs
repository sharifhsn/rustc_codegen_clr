//! Deterministic, human-readable IR dump for a single method.
//!
//! Every node is annotated with the type produced by the *same* [`CILNode::typecheck`] inference the
//! verifier uses, so a type that is one indirection too deep (`**X` where `*X` is expected) is read
//! straight off the tree and traced to the exact node that introduces it. This is the tool that
//! cracks "benign but ill-typed" rejections such as the `ThinBox`/`WithHeader::drop` arg mismatch:
//! the printer makes `*X` vs `**X` visually unambiguous and shows whether the offending value is a
//! bare `LdLoc` (⇒ a mis-typed *local*, a back-end local-typing bug), a `PtrCast` (⇒ a cast target
//! one level too deep), or a `Call`/transmute whose *result* type is wrong.
//!
//! Gated by the `DUMP_FN` env var: any method whose mangled **or** demangled name contains the
//! substring is printed (whether or not it passes the checker). The fatal type gate also dumps the
//! offending method unconditionally. Pure function of the IR ⇒ output is deterministic.

use crate::ir::bimap::IntoBiMapIndex;
use crate::ir::method::LocalDef;
use crate::ir::{Assembly, CILNode, FnSig, Interned, MethodDef, Type};
use fxhash::FxHashSet;

/// Render a `Type` in a compact, indirection-explicit form. The whole point is that `*X` and `**X`
/// are visually distinct (the mangled `pX`/`ppX` form is correct but hard to scan).
#[must_use]
pub fn type_readable(tpe: Type, asm: &Assembly) -> String {
    match tpe {
        Type::Ptr(inner) => format!("*{}", type_readable(asm[inner], asm)),
        Type::Ref(inner) => format!("&{}", type_readable(asm[inner], asm)),
        Type::ClassRef(cref) => asm[asm.class_ref(cref).name()].to_string(),
        Type::Int(i) => i.name().to_string(),
        Type::Float(f) => f.name().to_string(),
        Type::Bool => "bool".to_string(),
        Type::Void => "void".to_string(),
        Type::PlatformString => "string".to_string(),
        Type::PlatformChar => "char".to_string(),
        Type::PlatformObject => "object".to_string(),
        Type::PlatformArray { elem, dims } => {
            format!("{}[{}]", type_readable(asm[elem], asm), dims.get())
        }
        Type::FnPtr(sig) => format!("fnptr#{}", sig.as_bimap_index()),
        Type::SIMDVector(v) => v.name(),
        Type::PlatformGeneric(n, _) => format!("generic#{n}"),
    }
}

/// A short, payload-bearing label for a node (callee name for calls, slot for locals/args, otherwise
/// a truncated `Debug`).
fn node_label(node: &CILNode, asm: &Assembly) -> String {
    match node {
        CILNode::Call(info) => {
            let (mref, args, _) = info.as_ref();
            format!("Call {} (argc={})", &asm[asm[*mref].name()], args.len())
        }
        CILNode::CallI(info) => format!("CallI (argc={})", info.as_ref().2.len()),
        CILNode::LdLoc(l) => format!("LdLoc({l})"),
        CILNode::LdLocA(l) => format!("LdLocA({l})"),
        CILNode::LdArg(a) => format!("LdArg({a})"),
        CILNode::LdArgA(a) => format!("LdArgA({a})"),
        CILNode::PtrCast(_, res) => format!("PtrCast<{res:?}>"),
        CILNode::RefToPtr(_) => "RefToPtr".to_string(),
        CILNode::LdField { field, .. } => {
            let fd = asm.get_field(*field);
            format!(
                "LdField {}::{}",
                &asm[asm.class_ref(fd.owner()).name()],
                &asm[fd.name()]
            )
        }
        CILNode::LdFieldAddress { field, .. } => {
            let fd = asm.get_field(*field);
            format!(
                "LdFieldAddress {}::{}",
                &asm[asm.class_ref(fd.owner()).name()],
                &asm[fd.name()]
            )
        }
        other => {
            let s = format!("{other:?}");
            if s.len() > 72 {
                format!("{}…", &s[..72])
            } else {
                s
            }
        }
    }
}

/// Recursively print a node and its operands, each annotated with its inferred type. Interned nodes
/// shared across the tree are printed in full once, then referenced.
fn dump_node(
    out: &mut String,
    nodeidx: Interned<CILNode>,
    depth: usize,
    sig: Interned<FnSig>,
    locals: &[LocalDef],
    asm: &mut Assembly,
    seen: &mut FxHashSet<Interned<CILNode>>,
) {
    use std::fmt::Write;
    let node = asm.get_node(nodeidx).clone();
    let tystr = match node.typecheck(sig, locals, asm) {
        Ok(t) => type_readable(t, asm),
        Err(e) => format!("✗ {e:?}"),
    };
    let indent = "  ".repeat(depth);
    let label = node_label(&node, asm);
    let _ = writeln!(
        out,
        "{indent}#{idx} {label} : {tystr}",
        idx = nodeidx.as_bimap_index()
    );
    if !seen.insert(nodeidx) {
        let _ = writeln!(out, "{indent}  ↑(shown above)");
        return;
    }
    for child in node.child_nodes() {
        dump_node(out, child, depth + 1, sig, locals, asm, seen);
    }
}

/// Full readable dump of one method: signature, locals (with declared types), and every root with
/// its node tree, type-annotated.
#[must_use]
pub fn dump_method(def: &MethodDef, asm: &mut Assembly) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let sig = def.sig();
    let name = asm[def.name()].to_string();
    let dem = format!("{:#}", rustc_demangle::demangle(&name));
    let _ = writeln!(out, "\n╔════════ METHOD {name}");
    if dem != name {
        let _ = writeln!(out, "║ demangled: {dem}");
    }
    // Signature
    let fnsig = asm[sig].clone();
    let inputs = fnsig.inputs().to_vec();
    let output = *fnsig.output();
    let in_strs: Vec<String> = inputs.iter().map(|t| type_readable(*t, asm)).collect();
    let _ = writeln!(
        out,
        "║ sig: ({}) -> {}",
        in_strs.join(", "),
        type_readable(output, asm)
    );
    // Locals
    let locals: Vec<LocalDef> = def.iter_locals(asm).cloned().collect();
    let _ = writeln!(out, "║ locals:");
    for (i, (lname, ltpe)) in locals.iter().enumerate() {
        let nm = lname
            .map(|n| format!("  // {}", &asm[n]))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "║   L{i}: {}{}",
            type_readable(asm[*ltpe], asm),
            nm
        );
    }
    // Body
    let Some(blocks) = def.blocks(asm).map(<[_]>::to_vec) else {
        let _ = writeln!(out, "║ <no body>\n╚════════");
        return out;
    };
    let _ = writeln!(out, "║ body:");
    for block in &blocks {
        let _ = writeln!(out, "║ ── B{:?}", block.block_id());
        for rootidx in block.iter_roots() {
            let root = asm.get_root(rootidx).clone();
            let rootstr = root.display(asm, sig, &locals);
            let rstat = match root.typecheck(sig, &locals, asm) {
                Ok(()) => String::new(),
                Err(e) => format!("   ⟸ ✗ROOT {e:?}"),
            };
            let _ = writeln!(out, "║   {rootstr}{rstat}");
            let mut seen = FxHashSet::default();
            for n in root.nodes().iter() {
                dump_node(&mut out, **n, 2, sig, &locals, asm, &mut seen);
            }
        }
    }
    let _ = writeln!(out, "╚════════");
    out
}
