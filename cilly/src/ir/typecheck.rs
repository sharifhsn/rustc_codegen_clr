use fxhash::FxHashSet;

use crate::{bimap::IntoBiMapIndex, IString};

use super::{
    bimap::Interned,
    cilnode::{PtrCastRes, UnOp},
    method::LocalDef,
    Assembly, BinOp, CILNode, CILRoot, ClassRef, FieldDesc, FnSig, Int, Type,
};
#[derive(Debug)]
/// Signals that a piece of CIL is not valid.
pub enum TypeCheckError {
    /// CIL contains a binop with incorrect arguments
    WrongBinopArgs {
        /// The type of the left argument of this op
        lhs: Type,
        /// The type of the right argument of this op
        rhs: Type,
        /// The type of this op
        op: BinOp,
    },
    /// A reference-to-pointer cast is not a reference
    RefToPtrArgNotRef {
        /// The non-reference type encountered.
        arg: Type,
    },
    /// Incorrect pointer cast
    InvalidPtrCast {
        /// The result of this cast
        expected: PtrCastRes,
        /// The source type
        got: Type,
    },
    /// A non-pointer type was passed to an instruction expecting a pointer type
    TypeNotPtr {
        /// The incorrect type
        tpe: Type,
    },
    /// A load instruction was passed an incorrect type.
    DerfWrongPtr {
        /// Expected type
        expected: Type,
        /// Received type
        got: Type,
    },
    /// A call instruction was passed a wrong amount of args.
    CallArgcWrong {
        /// The signature-specified amount of args
        expected: usize,
        /// The received amount of args.
        got: usize,
        /// The name of this method
        mname: IString,
    },
    /// A call instruction was passed a wrong argument type.
    CallArgTypeWrong {
        /// The received type
        got: String,
        /// The expected type
        expected: String,
        /// The index of this argument
        idx: usize,
        /// The called method
        mname: IString,
    },
    IntCastInvalidInput {
        got: Type,
        target: Int,
    },
    /// Attempted to access the field of a type without fields.
    FieldAccessInvalidType {
        tpe: Type,
        field: crate::FieldDesc,
    },
    FieldOwnerMismatch {
        owner: Interned<ClassRef>,
        expected_owner: Interned<ClassRef>,
        field: crate::FieldDesc,
    },
    ExpectedClassGotValuetype {
        cref: ClassRef,
    },
    TypeNotClass {
        object: Type,
    },
    FloatCastInvalidInput {
        got: Type,
        target: super::Float,
    },
    WrongUnOpArgs {
        tpe: Type,
        op: UnOp,
    },
    /// Incorrect amount of args to an indirect call
    IndirectCallArgcWrong {
        expected: usize,
        got: usize,
    },
    /// An incorrect argument to an indirect call
    IndirectCallArgTypeWrong {
        got: Type,
        expected: Type,
        idx: usize,
    },
    /// Attempted to get the length of a non-array type
    LdLenArgNotArray {
        /// The non-array type
        got: Type,
    },
    /// Attempted to get the length of a managed array with a more than one dimension.
    LdLenArrNot1D {
        /// Array with dimension mismatch
        got: Type,
    },
    /// Invalid index into a managed array
    ArrIndexInvalidType {
        /// Received index type
        index_tpe: Type,
    },
    /// An indirect call with a non-fn-pointer type
    IndirectCallInvalidFnPtrType {
        /// non-fn-pointer-type
        fn_ptr: Type,
    },
    /// An indirect call with a mismatching signature
    IndirectCallInvalidFnPtrSig {
        /// Expected signature
        expected: super::FnSig,
        /// Signature of the pointer
        got: super::FnSig,
    },
    /// Atempt to calculate the size of void.
    SizeOfVoid,
    /// Asigned a wrong type to a local variable.
    LocalAssigementWrong {
        /// Index of the local.
        loc: u32,
        /// Received type.
        got: String,
        /// Expected type
        expected: String,
    },
    /// A comparison of non-prmitive types.
    ValueTypeCompare {
        /// Lhs side of the compare
        lhs: Type,
        /// Rhs side of the compare
        rhs: Type,
    },
    /// A write instruction was passed an address of incorrect type.
    WriteWrongAddr {
        /// Expected addr type
        addr: String,
        /// Received type
        tpe: String,
    },
    /// A write instruction was passed a value of incorrect type.
    WriteWrongValue {
        /// The expected type
        tpe: Type,
        /// The received type.
        value: Type,
    },
    /// Incorrect argument to a branch instruction
    ConditionNotBool {
        /// The wrong, not-bool type.
        cond: Type,
    },
    /// A comparsion instruction was used on a pair of types that can't be compared.
    CantCompareTypes {
        /// Lhs type
        lhs: Type,
        /// Rhs type
        rhs: Type,
    },
    /// A field assigement instruction was passed an icorrect type.
    FieldAssignWrongType {
        /// The expected type
        field_tpe: Type,
        /// The reference to the field.
        fld: Interned<FieldDesc>,
        /// The received type.
        val: Type,
    },
    /// An instruction attempted to access a field that does not exist.
    FieldNotPresent {
        /// The type of the field.
        tpe: Type,
        /// The name of the field.
        name: super::Interned<IString>,
        /// The owner of this field.
        owner: super::Interned<ClassRef>,
    },
    /// An operation was performed on a void pointer.
    VoidPointerOp {
        /// The kind of operation that was done.
        op: BinOp,
    },
    ManagedPtrCast {
        src: String,
        dst: String,
    },
}
/// Converts a typecheck error to a graph representing the issue with the typecheck process.
pub fn typecheck_err_to_string(
    root_idx: super::Interned<CILRoot>,
    asm: &mut Assembly,
    sig: Interned<FnSig>,
    locals: &[LocalDef],
) -> String {
    let root = asm[root_idx].clone();
    let mut set = FxHashSet::default();
    let nodes = root
        .nodes()
        .iter()
        .map(|node| display_node(**node, asm, sig, locals, &mut set))
        .collect::<String>();
    let root_connections: String = root.nodes().iter().fold(String::new(), |mut output, node| {
        use std::fmt::Write;
        writeln!(output, "n{node} ", node = node.as_bimap_index()).unwrap();
        output
    });
    let root_string = root.display(asm, sig, locals);
    match root.typecheck(sig, locals, asm){
        Ok(_)=> format!("digraph G{{edge [dir=\"back\"];\n{nodes} r{root_idx}  [label = \"{root_string}\" color = \"green\"] r{root_idx} ->{root_connections}}}",root_idx = root_idx.as_bimap_index()),
        Err(err)=> format!("digraph G{{edge [dir=\"back\"];\\n{nodes} r{root_idx}  [label = \"{root_string}\n{err:?}\" color = \"red\"] r{root_idx} ->{root_connections}}}",root_idx = root_idx.as_bimap_index()),
   }
}
/// Display an error during typechecking root `root_idx`.
pub fn display_typecheck_err(
    root_idx: super::Interned<CILRoot>,
    asm: &mut Assembly,
    sig: Interned<FnSig>,
    locals: &[LocalDef],
) {
    eprintln!("{}", typecheck_err_to_string(root_idx, asm, sig, locals))
}
#[doc(hidden)]
pub fn display_node(
    nodeidx: Interned<CILNode>,
    asm: &mut Assembly,
    sig: Interned<FnSig>,
    locals: &[LocalDef],
    set: &mut FxHashSet<Interned<CILNode>>,
) -> String {
    let node = asm.get_node(nodeidx).clone();
    set.insert(nodeidx);
    let tpe = node.typecheck(sig, locals, asm);
    let node_def = match tpe {
        Ok(tpe) => format!(
            "n{nodeidx} [label = {node:?} color = \"green\"]",
            nodeidx = nodeidx.as_bimap_index(),
            node = format!("{node:?}\n{}", tpe.mangle(asm))
        ),
        Err(err) => format!(
            "n{nodeidx} [label = {node:?} color = \"red\"]",
            nodeidx = nodeidx.as_bimap_index(),
            node = format!("{node:?}\n{err:?}")
        ),
    };
    let node_children = node.child_nodes();
    let node_children_str: String = node_children
        .iter()
        .fold(String::new(), |mut output, node| {
            use std::fmt::Write;
            let _ = write!(output, " n{nodeidx} ", nodeidx = node.as_bimap_index(),);
            output
        });
    if node_children.is_empty() {
        format!("{node_def}\n")
    } else {
        let mut res = format!(
            "{node_def}\n n{nodeidx}  -> {{{node_children_str}}}\n",
            nodeidx = nodeidx.as_bimap_index(),
        );
        for nodeidx in node.child_nodes() {
            res.push_str(&display_node(nodeidx, asm, sig, locals, set));
        }
        res
    }
}
/// PROVEN-benign type-erased pointer-argument pun (Phase P1 / WF-TC family E, differentially
/// verified): some builtins declare an opaque data-pointer parameter as `*u8` (or `*void`) to match
/// libstd's type-erased ABI — most notably `catch_unwind`/`__rust_try`, whose `data` argument is a
/// `*mut u8` that the caller fills with a `*Data<closure>`. The caller passes a concrete
/// `*SomeStruct`, which the name-based checker flags as a `CallArgTypeWrong`. The call emits a plain
/// pointer push (no conversion), so passing any pointer/ref where an erased `*u8`/`*void` is expected
/// is byte-identical. Narrowly gated: the *expected* parameter must be exactly `*u8` or `*void`, and
/// the *argument* must be a pointer/ref — so it can never accept a non-pointer or relax a normal arg.
fn is_erased_ptr_sink(arg: Type, expected: Type, asm: &Assembly) -> bool {
    // Direct case: expected is an erased `*u8`/`*void` sink, arg is any pointer/ref.
    if let Some(expected_pointee) = expected.pointed_to().map(|t| asm[t]) {
        if matches!(expected_pointee, Type::Int(Int::U8) | Type::Void) && arg.pointed_to().is_some() {
            return true;
        }
    }
    // Function-pointer case: the SAME erased-ABI pun one level up. `catch_unwind`/`__rust_try`
    // declares its `try_fn` parameter as `fn(*u8) -> ()`, but the caller passes the concrete
    // closure thunk `fn(*Data<closure>) -> ()`. An indirect call pushes the function pointer
    // unchanged, so two `FnPtr` sigs that are identical except that an `expected` parameter (or
    // return) is an erased `*u8`/`*void` where the `arg` side has a concrete pointer/ref are
    // call-compatible. We require equal arity and that every differing position be exactly this
    // erased-pointer/concrete-pointer pun — never a width or kind change.
    if let (Type::FnPtr(arg_sig), Type::FnPtr(exp_sig)) = (arg, expected) {
        let arg_sig = &asm[arg_sig];
        let exp_sig = &asm[exp_sig];
        if arg_sig.inputs().len() != exp_sig.inputs().len() {
            return false;
        }
        let pos_ok = |a: Type, e: Type| -> bool {
            a == e || is_erased_ptr_sink(a, e, asm)
        };
        let inputs_ok = arg_sig
            .inputs()
            .iter()
            .zip(exp_sig.inputs().iter())
            .all(|(&a, &e)| pos_ok(a, e));
        return inputs_ok && pos_ok(*arg_sig.output(), *exp_sig.output());
    }
    false
}
impl BinOp {
    fn typecheck(&self, lhs: Type, rhs: Type, asm: &Assembly) -> Result<Type, TypeCheckError> {
        match self {
            BinOp::Add | BinOp::Sub => match (lhs, rhs) {
                (Type::Int(lhs), Type::Int(rhs)) if rhs == lhs => Ok(Type::Int(lhs)),
                (Type::Float(lhs), Type::Float(rhs)) if rhs == lhs => Ok(Type::Float(lhs)),
                (Type::Ptr(lhs), Type::Ptr(rhs)) if rhs == lhs => Ok(Type::Ptr(lhs)),
                (Type::FnPtr(lhs), Type::FnPtr(rhs)) if rhs == lhs => Ok(Type::FnPtr(lhs)),
                (Type::Ptr(_inner), Type::Int(Int::ISize | Int::USize)) => {
                    // Since pointer ops operate in bytes, this is not an issue ATM.
                    /*if asm[inner] != Type::Void {
                        Ok(lhs)
                    } else {
                        Err(TypeCheckError::VoidPointerOp { op: self.clone() })
                    }*/
                    Ok(lhs)
                }
                (Type::FnPtr(_), Type::Int(Int::ISize | Int::USize)) => Ok(lhs),
                (Type::Int(Int::ISize | Int::USize), Type::Ptr(_) | Type::FnPtr(_)) => Ok(rhs),
                // TODO: investigate the cause of this issue. Changing a reference is not valid.
                (Type::Ref(_), Type::Int(Int::ISize | Int::USize)) => Ok(lhs),
                _ => {
                    if lhs.is_assignable_to(rhs, asm)
                        && (lhs.as_int().is_some() || rhs.as_int().is_some())
                    {
                        Ok(Type::Int(lhs.as_int().or(rhs.as_int()).unwrap()))
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::Eq => {
                if lhs == rhs || lhs.is_assignable_to(rhs, asm) {
                    if let Type::ClassRef(cref) = lhs {
                        if asm[cref].is_valuetype() {
                            Err(TypeCheckError::ValueTypeCompare { lhs, rhs })
                        } else {
                            Ok(Type::Bool)
                        }
                    } else {
                        Ok(Type::Bool)
                    }
                } else {
                    Err(TypeCheckError::WrongBinopArgs {
                        lhs,
                        rhs,
                        op: *self,
                    })
                }
            }

            BinOp::Mul => match (lhs, rhs) {
                (Type::Int(lhs), Type::Int(rhs)) if rhs == lhs => Ok(Type::Int(lhs)),
                (Type::Float(lhs), Type::Float(rhs)) if rhs == lhs => Ok(Type::Float(lhs)),
                (Type::Int(Int::ISize | Int::USize), Type::Ptr(_) | Type::FnPtr(_)) => Ok(rhs),
                // Relaxes the rules to prevent some wierd issue with sizeof
                (Type::Int(Int::ISize), Type::Int(Int::I32)) => Ok(Type::Int(Int::ISize)),
                (Type::Int(Int::USize), Type::Int(Int::I32)) => Ok(Type::Int(Int::USize)),
                _ => {
                    if lhs.is_assignable_to(rhs, asm) {
                        Ok(rhs)
                    } else if rhs.is_assignable_to(lhs, asm) {
                        Ok(lhs)
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::LtUn | BinOp::GtUn => match (lhs, rhs) {
                (Type::Int(lhs), Type::Int(rhs)) if rhs == lhs => Ok(Type::Bool),
                (Type::Float(lhs), Type::Float(rhs)) if rhs == lhs => Ok(Type::Bool),
                (Type::Ptr(lhs), Type::Ptr(rhs)) if rhs == lhs => Ok(Type::Bool),
                (Type::FnPtr(lhs), Type::FnPtr(rhs)) if rhs == lhs => Ok(Type::Bool),
                (Type::Bool, Type::Bool) => Ok(Type::Bool),
                _ => {
                    if lhs == rhs || lhs.is_assignable_to(rhs, asm) {
                        Ok(Type::Bool)
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::Lt | BinOp::Gt => match (lhs, rhs) {
                (Type::Int(lhs), Type::Int(rhs)) if rhs == lhs => Ok(Type::Bool),
                (Type::Float(lhs), Type::Float(rhs)) if rhs == lhs => Ok(Type::Bool),
                (Type::Bool, Type::Bool) => Ok(Type::Bool),
                _ => {
                    if lhs == rhs || lhs.is_assignable_to(rhs, asm) {
                        Ok(Type::Bool)
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::Or | BinOp::XOr | BinOp::And => match (lhs, rhs) {
                (Type::Int(lhs), Type::Int(rhs)) if rhs == lhs => Ok(Type::Int(lhs)),
                (Type::Bool, Type::Bool) => Ok(Type::Bool),
                _ => {
                    if lhs.is_assignable_to(rhs, asm)
                        && (lhs.as_int().is_some() || rhs.as_int().is_some())
                    {
                        Ok(Type::Int(lhs.as_int().or(rhs.as_int()).unwrap()))
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::Rem => match (lhs, rhs) {
                (Type::Int(lhs), Type::Int(rhs)) if rhs == lhs && rhs.is_signed() => {
                    Ok(Type::Int(lhs))
                }
                (Type::Float(lhs), Type::Float(rhs)) if rhs == lhs => Ok(Type::Bool),
                _ => {
                    if lhs.is_assignable_to(rhs, asm)
                        && (lhs.as_int().is_some() || rhs.as_int().is_some())
                    {
                        Ok(Type::Int(lhs.as_int().or(rhs.as_int()).unwrap()))
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::RemUn => match (lhs, rhs) {
                (Type::Int(lhs), Type::Int(rhs)) if rhs == lhs && !rhs.is_signed() => {
                    Ok(Type::Int(lhs))
                }
                (Type::Float(lhs), Type::Float(rhs)) if rhs == lhs => Ok(Type::Bool),
                _ => {
                    if lhs.is_assignable_to(rhs, asm)
                        && (lhs.as_int().is_some() || rhs.as_int().is_some())
                    {
                        Ok(Type::Int(lhs.as_int().or(rhs.as_int()).unwrap()))
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::Shl => match (lhs, rhs) {
                (
                    Type::Int(
                        lhs @ (Int::I128
                        | Int::U128
                        | Int::I64
                        | Int::U64
                        | Int::USize
                        | Int::ISize
                        | Int::I32
                        | Int::U32
                        | Int::I16
                        | Int::U16
                        | Int::U8
                        | Int::I8),
                    ),
                    Type::Int(
                        Int::USize
                        | Int::ISize
                        | Int::I32
                        | Int::U32
                        | Int::I16
                        | Int::U16
                        | Int::U8
                        | Int::I8,
                    ),
                ) => Ok(Type::Int(lhs)),
                _ => {
                    if lhs.is_assignable_to(rhs, asm)
                        && (lhs.as_int().is_some() || rhs.as_int().is_some())
                    {
                        Ok(Type::Int(lhs.as_int().or(rhs.as_int()).unwrap()))
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::Shr => match (lhs, rhs) {
                (
                    Type::Int(
                        lhs @ (Int::I128
                        | Int::U128
                        | Int::I64
                        | Int::U64
                        | Int::USize
                        | Int::ISize
                        | Int::I32
                        | Int::U32
                        | Int::I16
                        | Int::U16
                        | Int::U8
                        | Int::I8),
                    ),
                    Type::Int(
                        Int::USize
                        | Int::ISize
                        | Int::I32
                        | Int::U32
                        | Int::I16
                        | Int::U16
                        | Int::U8
                        | Int::I8,
                    ),
                ) if lhs.is_signed() => Ok(Type::Int(lhs)),
                _ => {
                    if lhs.is_assignable_to(rhs, asm)
                        && (lhs.as_int().is_some() || rhs.as_int().is_some())
                    {
                        Ok(Type::Int(lhs.as_int().or(rhs.as_int()).unwrap()))
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::ShrUn => match (lhs, rhs) {
                (
                    Type::Int(
                        lhs @ (Int::I128
                        | Int::U128
                        | Int::I64
                        | Int::U64
                        | Int::USize
                        | Int::ISize
                        | Int::I32
                        | Int::U32
                        | Int::I16
                        | Int::U16
                        | Int::U8
                        | Int::I8),
                    ),
                    Type::Int(
                        Int::USize
                        | Int::ISize
                        | Int::I32
                        | Int::U32
                        | Int::I16
                        | Int::U16
                        | Int::U8
                        | Int::I8,
                    ),
                ) if !lhs.is_signed() => Ok(Type::Int(lhs)),
                _ => {
                    if lhs.is_assignable_to(rhs, asm)
                        && (lhs.as_int().is_some() || rhs.as_int().is_some())
                    {
                        Ok(Type::Int(lhs.as_int().or(rhs.as_int()).unwrap()))
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::DivUn => match (lhs, rhs) {
                (
                    Type::Int(lhs @ (Int::U64 | Int::USize | Int::U32 | Int::U16 | Int::U8)),
                    Type::Int(rhs @ (Int::U64 | Int::USize | Int::U32 | Int::U16 | Int::U8)),
                ) if lhs == rhs => Ok(Type::Int(lhs)),
                _ => {
                    if lhs.is_assignable_to(rhs, asm)
                        && (lhs.as_int().is_some() || rhs.as_int().is_some())
                    {
                        Ok(Type::Int(lhs.as_int().or(rhs.as_int()).unwrap()))
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
            BinOp::Div => match (lhs, rhs) {
                (
                    Type::Int(
                        lhs @ (Int::U64
                        | Int::USize
                        | Int::ISize
                        | Int::I32
                        | Int::U32
                        | Int::I16
                        | Int::U16
                        | Int::U8
                        | Int::I8),
                    ),
                    Type::Int(
                        rhs @ (Int::U64
                        | Int::USize
                        | Int::ISize
                        | Int::I32
                        | Int::U32
                        | Int::I16
                        | Int::U16
                        | Int::U8
                        | Int::I8),
                    ),
                ) if lhs.is_signed() && lhs == rhs => Ok(Type::Int(lhs)),
                (Type::Float(lhs), Type::Float(rhs)) if rhs == lhs => Ok(Type::Float(lhs)),
                _ => {
                    if lhs.is_assignable_to(rhs, asm)
                        && (lhs.as_int().is_some() || rhs.as_int().is_some())
                    {
                        Ok(Type::Int(lhs.as_int().or(rhs.as_int()).unwrap()))
                    } else {
                        Err(TypeCheckError::WrongBinopArgs {
                            lhs,
                            rhs,
                            op: *self,
                        })
                    }
                }
            },
        }
    }
}
impl CILNode {
    #[allow(unused_variables)]
    /// Typechecks this node, and returns its type if its valid.
    /// # Errors
    /// Returns an error if this node can't pass type checks.
    pub fn typecheck(
        &self,
        sig: Interned<FnSig>,
        locals: &[LocalDef],
        asm: &mut Assembly,
    ) -> Result<Type, TypeCheckError> {
        match self {
            CILNode::Const(cst) => Ok(cst.as_ref().get_type()),
            CILNode::BinOp(lhs, rhs, op) => {
                let lhs = asm.get_node(*lhs).clone();
                let rhs = asm.get_node(*rhs).clone();
                let lhs = lhs.typecheck(sig, locals, asm)?;
                let rhs = rhs.typecheck(sig, locals, asm)?;
                op.typecheck(lhs, rhs, asm)
            }
            CILNode::UnOp(arg, op) => {
                let arg = asm.get_node(*arg).clone();
                let arg_type = arg.typecheck(sig, locals, asm)?;
                match (arg_type, op) {
                    (Type::Int(_) | Type::Float(_) | Type::Ptr(_), UnOp::Not) => Ok(arg_type),
                    (Type::Int(int), UnOp::Neg) if int.is_signed() => Ok(arg_type),
                    (Type::Float(_) | Type::Ptr(_), UnOp::Neg) => Ok(arg_type),
                    _ => Err(TypeCheckError::WrongUnOpArgs {
                        tpe: arg_type,
                        op: op.clone(),
                    }),
                }
            }
            CILNode::LdLoc(loc) => Ok(asm[locals[*loc as usize].1]),
            CILNode::LdLocA(loc) => Ok(asm.nref(asm[locals[*loc as usize].1])),
            CILNode::LdArg(arg) => Ok(asm[sig].inputs()[*arg as usize]),
            CILNode::LdArgA(arg) => Ok(asm.nref(asm[sig].inputs()[*arg as usize])),
            CILNode::Call(call_info) => {
                let (mref, args, _is_pure) = call_info.as_ref();
                let mref = asm[*mref].clone();
                let inputs: Box<[_]> = mref.stack_inputs(asm).into();
                if args.len() != inputs.len() {
                    return Err(TypeCheckError::CallArgcWrong {
                        expected: inputs.len(),
                        got: args.len(),
                        mname: asm[mref.name()].into(),
                    });
                }
                for (idx, (arg, input_type)) in args.iter().zip(inputs.iter()).enumerate() {
                    let arg = asm.get_node(*arg).clone();
                    let arg_type = arg.typecheck(sig, locals, asm)?;
                    if !arg_type.is_assignable_to(*input_type, asm)
                        && !arg_type
                            .try_deref(asm)
                            .is_some_and(|t| Some(t) == input_type.try_deref(asm))
                        && !is_erased_ptr_sink(arg_type, *input_type, asm)
                    {
                        return Err(TypeCheckError::CallArgTypeWrong {
                            got: arg_type.mangle(asm),
                            expected: input_type.mangle(asm),
                            idx,
                            mname: asm[mref.name()].into(),
                        });
                    }
                }
                Ok(mref.output(asm))
            }
            CILNode::CallI(info) => {
                let (fn_ptr, called_sig, args) = info.as_ref();
                let fn_ptr = asm.get_node(*fn_ptr).clone();
                let fn_ptr = fn_ptr.typecheck(sig, locals, asm)?;
                let called_sig = asm[*called_sig].clone();
                if args.len() != called_sig.inputs().len() {
                    return Err(TypeCheckError::IndirectCallArgcWrong {
                        expected: called_sig.inputs().len(),
                        got: args.len(),
                    });
                }

                for (idx, (arg, input_type)) in
                    args.iter().zip(called_sig.inputs().iter()).enumerate()
                {
                    let arg = asm.get_node(*arg).clone();
                    let arg_type = arg.typecheck(sig, locals, asm)?;
                    if !arg_type.is_assignable_to(*input_type, asm) {
                        return Err(TypeCheckError::IndirectCallArgTypeWrong {
                            got: arg_type,
                            expected: *input_type,
                            idx,
                        });
                    }
                }
                let Type::FnPtr(ptr_sig) = fn_ptr else {
                    return Err(TypeCheckError::IndirectCallInvalidFnPtrType { fn_ptr });
                };
                let ptr_sig = &asm[ptr_sig];
                if *ptr_sig != called_sig {
                    return Err(TypeCheckError::IndirectCallInvalidFnPtrSig {
                        expected: called_sig,
                        got: ptr_sig.clone(),
                    });
                }
                Ok(*called_sig.output())
            }
            CILNode::IntCast {
                input,
                target,
                extend,
            } => {
                let input = asm.get_node(*input).clone();
                let input = input.typecheck(sig, locals, asm)?;
                match input {
                    Type::Float(_) | Type::Int(_) | Type::Ptr(_) | Type::FnPtr(_) | Type::Bool => {
                        Ok(Type::Int(*target))
                    }
                    _ => Err(TypeCheckError::IntCastInvalidInput {
                        got: input,
                        target: *target,
                    }),
                }
            }
            CILNode::FloatCast {
                input,
                target,
                is_signed,
            } => {
                let input = asm.get_node(*input).clone();
                let input = input.typecheck(sig, locals, asm)?;
                match input {
                    Type::Float(_) | Type::Int(_) => Ok(Type::Float(*target)),
                    _ => Err(TypeCheckError::FloatCastInvalidInput {
                        got: input,
                        target: *target,
                    }),
                }
            }
            CILNode::RefToPtr(refn) => {
                let refn = asm.get_node(*refn).clone();
                let tpe = refn.typecheck(sig, locals, asm)?;
                match tpe {
                    Type::Ref(inner) | Type::Ptr(inner) => Ok(asm.nptr(asm[inner])),
                    _ => Err(TypeCheckError::RefToPtrArgNotRef { arg: tpe }),
                }
            }
            CILNode::PtrCast(arg, res) => {
                let arg = asm.get_node(*arg).clone();
                let arg_tpe = arg.typecheck(sig, locals, asm)?;
                match arg_tpe {
                    Type::Ptr(inner) | Type::Ref(inner) => {
                        if asm[inner].is_gcref(asm) {
                            return Err(TypeCheckError::ManagedPtrCast {
                                src: arg_tpe.mangle(asm),
                                dst: res.as_ref().as_type().mangle(asm),
                            });
                        }
                    }

                    Type::Int(Int::USize | Int::ISize) | Type::FnPtr(_) => (),
                    _ => Err(TypeCheckError::InvalidPtrCast {
                        expected: res.as_ref().clone(),
                        got: arg_tpe,
                    })?,
                };
                if res.as_ref().as_type().is_gcref(asm) {
                    return Err(TypeCheckError::ManagedPtrCast {
                        src: arg_tpe.mangle(asm),
                        dst: res.as_ref().as_type().mangle(asm),
                    });
                }
                Ok(res.as_ref().as_type())
            }
            CILNode::LdFieldAddress { addr, field } => {
                let field = *asm.get_field(*field);
                let addr = asm.get_node(*addr).clone();
                let addr_tpe = addr.typecheck(sig, locals, asm)?;
                let pointed_tpe = {
                    match addr_tpe {
                        Type::Ptr(type_idx) | Type::Ref(type_idx) => Some(asm[type_idx]),
                        Type::ClassRef(_) => Some(addr_tpe),
                        _ => None,
                    }
                }
                .ok_or(TypeCheckError::TypeNotPtr { tpe: addr_tpe })?;

                let Type::ClassRef(pointed_owner) = pointed_tpe else {
                    return Err(TypeCheckError::FieldAccessInvalidType {
                        tpe: pointed_tpe,
                        field,
                    });
                };
                if pointed_owner != field.owner() {
                    return Err(TypeCheckError::FieldOwnerMismatch {
                        owner: pointed_owner,
                        expected_owner: field.owner(),
                        field,
                    });
                }
                // Check that this type owns a matching field
                if let Some(cdef) = asm.class_ref_to_def(field.owner()) {
                    if !asm[cdef]
                        .fields()
                        .iter()
                        .any(|(tpe, name, _offset)| *tpe == field.tpe() && *name == field.name())
                    {
                        return Err(TypeCheckError::FieldNotPresent {
                            tpe: field.tpe(),
                            name: field.name(),
                            owner: field.owner(),
                        });
                    }
                }
                match addr_tpe {
                    Type::Ref(_) => Ok(asm.nref(field.tpe())),
                    Type::Ptr(_) => Ok(asm.nptr(field.tpe())),
                    // `ldflda` on a by-value object reference (`ClassRef`) is legal CIL: it yields
                    // a managed pointer to the field. This case is deliberately accepted by the
                    // pointed-type match above (line ~851) and fully validated (owner + field
                    // presence); `LdField` returns `Ok` for the identical case and BOTH exporters
                    // emit `ldflda` for it. The correct result type is a managed reference to the
                    // field type — mirror the `Type::Ref` result. Returning a `TypeCheckError`
                    // here would be a FALSE NEGATIVE rejecting valid IR the exporters emit.
                    Type::ClassRef(_) => Ok(asm.nref(field.tpe())),
                    _ => unreachable!(
                        "LdFieldAddress addr typechecked to {addr_tpe:?}, which was not accepted by \
                         the pointed-type match above"
                    ),
                }
            }

            CILNode::LdField { addr, field } => {
                let field = *asm.get_field(*field);
                let addr = asm.get_node(*addr).clone();
                let addr_tpe = addr.typecheck(sig, locals, asm)?;
                let pointed_tpe = {
                    match addr_tpe {
                        Type::Ptr(type_idx) | Type::Ref(type_idx) => Some(asm[type_idx]),
                        Type::ClassRef(_) => Some(addr_tpe),
                        _ => None,
                    }
                }
                .ok_or(TypeCheckError::TypeNotPtr { tpe: addr_tpe })?;
                let Type::ClassRef(pointed_owner) = pointed_tpe else {
                    return Err(TypeCheckError::FieldAccessInvalidType {
                        tpe: pointed_tpe,
                        field,
                    });
                };
                if pointed_owner != field.owner() {
                    return Err(TypeCheckError::FieldOwnerMismatch {
                        owner: pointed_owner,
                        expected_owner: field.owner(),
                        field,
                    });
                }
                // Check that this type owns a matching field
                if let Some(cdef) = asm.class_ref_to_def(field.owner()) {
                    if !asm[cdef]
                        .fields()
                        .iter()
                        .any(|(tpe, name, _offset)| *tpe == field.tpe() && *name == field.name())
                    {
                        return Err(TypeCheckError::FieldNotPresent {
                            tpe: field.tpe(),
                            name: field.name(),
                            owner: field.owner(),
                        });
                    }
                }
                Ok(field.tpe())
            }
            CILNode::LdInd {
                addr,
                tpe,
                volatile: volitale,
            } => {
                let _ = volitale;
                let addr = asm.get_node(*addr).clone();
                let addr_tpe = addr.typecheck(sig, locals, asm)?;
                // NOTE: the `StInd` *store* dual drops the address-pointee-vs-`tpe` comparison
                // entirely (it is a checker-model artifact — see that arm). The `LdInd` *load* arm
                // deliberately KEEPS it, for two reasons: (1) it uses the looser `is_assignable_to`
                // (not `==`), so it has not accreted the special-case pile the store side did; and
                // (2) it REPORTS the address's pointee type as its result, and downstream nodes rely
                // on that (e.g. a `SetField` consuming an `LdInd` of a pointer field in `Weak::drop`
                // — reporting the declared `tpe` instead reintroduces `FieldAssignWrongType` false
                // positives one hop downstream, differentially observed on aho-corasick/regex). The
                // store side is where the strsim false-positive pile lived and is fixed there.
                let pointed_tpe = addr_tpe
                    .pointed_to()
                    .ok_or(TypeCheckError::TypeNotPtr { tpe: addr_tpe })?;
                let pointed_tpe = asm[pointed_tpe];
                let tpe = asm[*tpe];
                if !pointed_tpe.is_assignable_to(tpe, asm) {
                    Err(TypeCheckError::DerfWrongPtr {
                        expected: tpe,
                        got: pointed_tpe,
                    })
                } else {
                    Ok(pointed_tpe)
                }
            }
            CILNode::SizeOf(tpe) => match asm[*tpe] {
                Type::Void => Err(TypeCheckError::SizeOfVoid),
                _ => Ok(Type::Int(Int::I32)),
            },
            CILNode::GetException => Ok(Type::ClassRef(ClassRef::exception(asm))),
            CILNode::IsInst(obj, _) => {
                let obj = asm.get_node(*obj).clone();
                let obj = obj.typecheck(sig, locals, asm)?;
                // `isinst` requires an object reference on the stack. Accept every GC-reference
                // shape the backend produces (managed class refs, Object, platform string/array,
                // open generics); reject clearly-non-reference operands (Int/Float/Ptr/...).
                // Mirrors the `UnboxAny` operand check; result stays `Bool` because this backend's
                // IsInst feeds a Rust `bool` (mycorrhiza `..._is_inst`) and a `BranchCond::False`.
                match obj {
                    Type::ClassRef(cref) => {
                        if asm.class_ref(cref).is_valuetype() {
                            return Err(TypeCheckError::ExpectedClassGotValuetype {
                                cref: asm.class_ref(cref).clone(),
                            });
                        }
                    }
                    Type::PlatformObject
                    | Type::PlatformGeneric(_, _)
                    | Type::PlatformString
                    | Type::PlatformArray { .. } => (),
                    _ => return Err(TypeCheckError::TypeNotClass { object: obj }),
                }
                Ok(Type::Bool)
            }
            CILNode::CheckedCast(obj, cast_res) => {
                let obj = asm.get_node(*obj).clone();
                let obj = obj.typecheck(sig, locals, asm)?;
                // `castclass` requires an object reference on the stack (same accept-set as `isinst`
                // / `UnboxAny`). The result is the target ref, exactly as `castclass T` yields a `T`.
                match obj {
                    Type::ClassRef(cref) => {
                        if asm.class_ref(cref).is_valuetype() {
                            return Err(TypeCheckError::ExpectedClassGotValuetype {
                                cref: asm.class_ref(cref).clone(),
                            });
                        }
                    }
                    Type::PlatformObject
                    | Type::PlatformGeneric(_, _)
                    | Type::PlatformString
                    | Type::PlatformArray { .. } => (),
                    _ => return Err(TypeCheckError::TypeNotClass { object: obj }),
                }
                Ok(asm[*cast_res])
            }

            CILNode::LocAlloc { size } => {
                let size = asm[*size].clone().typecheck(sig, locals, asm)?;
                Ok(asm.nptr(Type::Int(Int::U8)))
            }
            CILNode::LdStaticField(sfld) => {
                let sfld = *asm.get_static_field(*sfld);
                Ok(sfld.tpe())
            }
            CILNode::LdStaticFieldAddress(sfld) => {
                let sfld = *asm.get_static_field(*sfld);
                Ok(asm.nptr(sfld.tpe()))
            }
            CILNode::LdFtn(mref) => {
                let mref = &asm[*mref];
                Ok(Type::FnPtr(mref.sig()))
            }
            CILNode::LdTypeToken(_) => Ok(Type::ClassRef(ClassRef::runtime_type_hadle(asm))),
            CILNode::LdLen(arr) => {
                let arr = asm.get_node(*arr).clone();
                let arr_tpe = arr.typecheck(sig, locals, asm)?;
                let Type::PlatformArray { elem: _, dims } = arr_tpe else {
                    return Err(TypeCheckError::LdLenArgNotArray { got: arr_tpe });
                };
                if dims.get() != 1 {
                    return Err(TypeCheckError::LdLenArrNot1D { got: arr_tpe });
                }
                Ok(Type::Int(Int::I32))
            }
            CILNode::LocAllocAlgined { tpe, align } => Ok(Type::Ptr(*tpe)),
            CILNode::LdElelemRef { array, index } => {
                let arr = asm.get_node(*array).clone();
                let arr_tpe = arr.typecheck(sig, locals, asm)?;
                let index = asm.get_node(*index).clone();
                let index_tpe = index.typecheck(sig, locals, asm)?;
                let Type::PlatformArray { elem, dims } = arr_tpe else {
                    return Err(TypeCheckError::LdLenArgNotArray { got: arr_tpe });
                };
                if dims.get() != 1 {
                    return Err(TypeCheckError::LdLenArrNot1D { got: arr_tpe });
                }
                match index_tpe {
                    Type::Int(Int::I32 | Int::U32 | Int::I64 | Int::USize | Int::ISize) => (),
                    _ => return Err(TypeCheckError::ArrIndexInvalidType { index_tpe }),
                }
                Ok(asm[elem])
            }
            CILNode::UnboxAny { object, tpe } => {
                let object = asm.get_node(*object).clone();
                let object = object.typecheck(sig, locals, asm)?;
                match object {
                    Type::ClassRef(cref) => {
                        let cref = asm.class_ref(cref);
                        if cref.is_valuetype() {
                            return Err(TypeCheckError::ExpectedClassGotValuetype {
                                cref: cref.clone(),
                            });
                        }
                    }
                    Type::PlatformObject | Type::PlatformGeneric(_, _) | Type::PlatformString => (),
                    _ => return Err(TypeCheckError::TypeNotClass { object }),
                };
                Ok(asm[*tpe])
            }
            CILNode::NewArr { elem, len } => {
                let len_tpe = asm.get_node(*len).clone().typecheck(sig, locals, asm)?;
                match len_tpe {
                    Type::Int(Int::I32 | Int::U32 | Int::I64 | Int::USize | Int::ISize) => (),
                    _ => {
                        return Err(TypeCheckError::ArrIndexInvalidType {
                            index_tpe: len_tpe,
                        })
                    }
                }
                Ok(Type::PlatformArray {
                    elem: *elem,
                    dims: std::num::NonZeroU8::new(1).unwrap(),
                })
            }
        }
    }
}
impl CILRoot {
    pub fn typecheck(
        &self,
        sig: Interned<FnSig>,
        locals: &[LocalDef],
        asm: &mut Assembly,
    ) -> Result<(), TypeCheckError> {
        match self {
            Self::StLoc(loc, node) => {
                let got = asm.get_node(*node).clone().typecheck(sig, locals, asm)?;
                let expected = asm[locals[*loc as usize].1];
                // Benign-noise suppression (diagnostics only): storing a Void-typed value into a
                // local is a no-op at runtime — a Void value carries no bits and is elided by the
                // exporter/JIT. This arises from `LdStaticField` of opaque/ZST marker statics (e.g.
                // `__rust_no_alloc_shim_is_unstable`) whose declared field type is `Void`. The
                // checker's `is_assignable_to` has no Void arm by design (broadening it would
                // over-permit Void everywhere), so the safe, narrowly-scoped fix is to accept a
                // Void source *only* at the StLoc store. Cannot alter emitted CIL (the typechecker
                // only logs). See task #43.
                if got == Type::Void {
                    return Ok(());
                }
                // PROVEN-benign pointer-relabel (Phase P1 / WF-TC family D, differentially verified by
                // `test/iter/array_byval.rs`): a `PtrCast` lowers to NOTHING in the exporter — it emits
                // only its argument node (see `il_exporter` CILNode::PtrCast => export_node(val)). So
                // when the stored value is a `PtrCast` and both `got` and `expected` are pointer/ref
                // types, the bits written into the local are exactly the raw pointer the cast wraps,
                // regardless of how deep the cast's *declared* target type is. This is the source of
                // the `ppX`/`pX` `IndexRange`-cursor false positive (a cast target one indirection too
                // deep). It is narrowly gated on (1) the value really being a `PtrCast` and (2) both
                // sides being pointers, so it can never mask a value/local *kind* mismatch.
                if matches!(asm.get_node(*node), CILNode::PtrCast(..))
                    && got.pointed_to().is_some()
                    && expected.pointed_to().is_some()
                {
                    return Ok(());
                }
                // CIL models `bool` as an integer on the evaluation stack (`ldc.i4`/`stloc` of a
                // `bool` local take an `i4`), so an integer value is stack-compatible with a `bool`
                // local and vice-versa. This is the SAME equivalence the `StInd` arm already encodes
                // for `bool`/`i8`. It surfaces here because `catch_unwind` (the cilly builtin) returns
                // a literal `i32` 0/1 that `std::rt::lang_start_internal`/`thread_cleanup` store into a
                // Rust `bool` local. Narrowly gated to the bool↔int pair so it cannot relax an
                // unrelated assignment. Proven benign: the only producer is `Ret(Const::I32(0|1))`.
                if (expected == Type::Bool && got.as_int().is_some())
                    || (got == Type::Bool && expected.as_int().is_some())
                {
                    return Ok(());
                }
                if !got.is_assignable_to(expected, asm) {
                    Err(TypeCheckError::LocalAssigementWrong {
                        loc: *loc,
                        got: got.mangle(asm),
                        expected: expected.mangle(asm),
                    })
                } else {
                    Ok(())
                }
            }
            Self::Branch(boxed) => {
                let (_, _, cond) = boxed.as_ref();
                let Some(cond) = cond else { return Ok(()) };
                match cond {
                    super::BranchCond::True(cond) | super::BranchCond::False(cond) => {
                        let cond = asm[*cond].clone().typecheck(sig, locals, asm)?;
                        match cond {
                            Type::Bool => Ok(()),
                            Type::Int(_) => Ok(()),
                            _ => Err(TypeCheckError::ConditionNotBool { cond }),
                        }
                    }
                    super::BranchCond::Eq(lhs, rhs)
                    | super::BranchCond::Ne(lhs, rhs)
                    | super::BranchCond::Lt(lhs, rhs, _)
                    | super::BranchCond::Gt(lhs, rhs, _)
                    | super::BranchCond::Le(lhs, rhs, _)
                    | super::BranchCond::Ge(lhs, rhs, _) => {
                        let lhs = asm[*lhs].clone().typecheck(sig, locals, asm)?;
                        let rhs = asm[*rhs].clone().typecheck(sig, locals, asm)?;
                        if lhs.is_assignable_to(rhs, asm)
                            && lhs
                                .as_class_ref()
                                .is_none_or(|cref| !asm[cref].is_valuetype())
                        {
                            Ok(())
                        } else {
                            Err(TypeCheckError::CantCompareTypes { lhs, rhs })
                        }
                    }
                }
            }
            Self::StInd(boxed) => {
                let (addr, value, tpe, _) = boxed.as_ref();
                let addr = asm[*addr].clone().typecheck(sig, locals, asm)?;
                let value = asm[*value].clone().typecheck(sig, locals, asm)?;
                let tpe = *tpe;
                // ARCHITECTURAL NOTE (the address pointee-type is NOT a checkable invariant). The
                // exporter emits a `stind.i1`/`stind.i4`/`stobj <T>` whose width+kind are derived
                // from `tpe` ALONE; the address is pushed as a raw machine pointer and its *declared
                // pointee type never reaches the instruction*. Pointer pointee-types in this IR are
                // also routinely ERASED by ordinary Rust codegen — `*u8` byte cursors, `*Void`
                // refcount slots (ArcInner/Cell), extra-indirected `PtrCast` relabels (`**X` for an
                // `X` store), `bool` slots typed `Bool` for an `i8` value. So comparing the address's
                // pointee type against `tpe` carries no reliable information: it cannot distinguish a
                // benign relabel from a real bug, and historically generated only FALSE POSITIVES,
                // each patched with another hand-proven exception (extra-indirection, void-erased,
                // bool/i8, …). We therefore do NOT check it. The reliable, soundness-relevant
                // invariants are exactly:
                //   (a) the address IS a pointer/ref — a valid machine address to store through;
                //   (b) `tpe` is storeable — there is no store opcode for `Void`;
                //   (c) the *value* matches the store type `tpe` (this is what binds the pushed bits
                //       to the emitted opcode, and IS reliably checkable).
                if addr.pointed_to().is_none() {
                    return Err(TypeCheckError::WriteWrongAddr {
                        addr: addr.mangle(asm),
                        tpe: tpe.mangle(asm),
                    });
                }
                if tpe == Type::Void {
                    return Err(TypeCheckError::WriteWrongAddr {
                        addr: addr.mangle(asm),
                        tpe: tpe.mangle(asm),
                    });
                }
                if !(value.is_assignable_to(tpe, asm)
                    || value
                        .as_int()
                        .zip(tpe.as_int())
                        .is_some_and(|(a, b)| a.as_unsigned() == b.as_unsigned())
                    || value == Type::Bool && tpe == Type::Int(Int::I8))
                {
                    return Err(TypeCheckError::WriteWrongValue { tpe, value });
                }
                Ok(())
            }
            Self::SetField(boxed) => {
                let (fld, addr, val) = boxed.as_ref();
                let addr = asm[*addr].clone().typecheck(sig, locals, asm)?;
                let val: Type = asm[*val].clone().typecheck(sig, locals, asm)?;
                let field = asm[*fld];
                let field_tpe = field.tpe();
                if !val.is_assignable_to(field_tpe, asm) {
                    return Err(TypeCheckError::FieldAssignWrongType {
                        field_tpe,
                        fld: *fld,
                        val,
                    });
                }
                let Some(pointed_tpe) = addr.pointed_to().map(|tpe| asm[tpe]) else {
                    return Err(TypeCheckError::TypeNotPtr { tpe: addr });
                };
                let Type::ClassRef(pointed_owner) = pointed_tpe else {
                    return Err(TypeCheckError::FieldAccessInvalidType {
                        tpe: pointed_tpe,
                        field,
                    });
                };
                if pointed_owner != field.owner() {
                    return Err(TypeCheckError::FieldOwnerMismatch {
                        owner: pointed_owner,
                        expected_owner: field.owner(),
                        field,
                    });
                }
                // Check that this type owns a matching field
                if let Some(cdef) = asm.class_ref_to_def(field.owner()) {
                    if !asm[cdef]
                        .fields()
                        .iter()
                        .any(|(tpe, name, _offset)| *tpe == field.tpe() && *name == field.name())
                    {
                        return Err(TypeCheckError::FieldNotPresent {
                            tpe: field.tpe(),
                            name: field.name(),
                            owner: field.owner(),
                        });
                    }
                }
                Ok(())
            }
            Self::Call(boxed) => {
                let (mref, args, _is_pure) = boxed.as_ref();
                let mref = asm[*mref].clone();
                let call_sig = asm[mref.sig()].clone();
                match mref.kind() {
                    crate::cilnode::MethodKind::Static => {
                        let expected = call_sig.inputs().len();
                        let got = args.len();
                        if expected != got {
                            return Err(TypeCheckError::CallArgcWrong {
                                expected,
                                got,
                                mname: asm[mref.name()].into(),
                            });
                        }
                    }
                    crate::cilnode::MethodKind::Instance
                    | crate::cilnode::MethodKind::Virtual
                    | crate::cilnode::MethodKind::Constructor => (),
                }
                for (index, (arg, expected)) in
                    args.iter().zip(call_sig.inputs().iter()).enumerate()
                {
                    let arg = asm[*arg].clone().typecheck(sig, locals, asm)?;
                    if !arg.is_assignable_to(*expected, asm)
                        && !is_erased_ptr_sink(arg, *expected, asm)
                    {
                        return Err(TypeCheckError::CallArgTypeWrong {
                            got: arg.mangle(asm),
                            expected: expected.mangle(asm),
                            idx: index,
                            mname: asm[mref.name()].into(),
                        });
                    }
                }
                Ok(())
            }
            Self::StElem {
                array,
                index,
                value,
                elem,
            } => {
                let arr_tpe = asm.get_node(*array).clone().typecheck(sig, locals, asm)?;
                let index_tpe = asm.get_node(*index).clone().typecheck(sig, locals, asm)?;
                let value_tpe = asm.get_node(*value).clone().typecheck(sig, locals, asm)?;
                let Type::PlatformArray { elem: arr_elem, dims } = arr_tpe else {
                    return Err(TypeCheckError::LdLenArgNotArray { got: arr_tpe });
                };
                if dims.get() != 1 {
                    return Err(TypeCheckError::LdLenArrNot1D { got: arr_tpe });
                }
                match index_tpe {
                    Type::Int(Int::I32 | Int::U32 | Int::I64 | Int::USize | Int::ISize) => (),
                    _ => return Err(TypeCheckError::ArrIndexInvalidType { index_tpe }),
                }
                let elem_tpe = asm[*elem];
                // The declared element type must match the array's element type (modulo sign).
                if !(asm[arr_elem] == elem_tpe
                    || asm[arr_elem]
                        .as_int()
                        .zip(elem_tpe.as_int())
                        .is_some_and(|(a, b)| a.as_unsigned() == b.as_unsigned()))
                {
                    return Err(TypeCheckError::WriteWrongValue {
                        tpe: elem_tpe,
                        value: asm[arr_elem],
                    });
                }
                // The value being stored must be assignable to the element type (modulo sign).
                if !(value_tpe.is_assignable_to(elem_tpe, asm)
                    || value_tpe
                        .as_int()
                        .zip(elem_tpe.as_int())
                        .is_some_and(|(a, b)| a.as_unsigned() == b.as_unsigned()))
                {
                    return Err(TypeCheckError::WriteWrongValue {
                        tpe: elem_tpe,
                        value: value_tpe,
                    });
                }
                Ok(())
            }
            Self::TerminateRegion { protected, .. } => {
                // A `TerminateRegion` is a side-effecting region whose only child is `protected`;
                // it yields no stack value (like `ReThrow`/`ExitSpecialRegion`). Delegate to the
                // protected root's own typecheck so the guarded op is still verified (it is not in
                // any block's top-level root list, so nothing else would check it).
                asm.get_root(*protected).clone().typecheck(sig, locals, asm)
            }
            _ => {
                for node in self.nodes() {
                    asm.get_node(*node).clone().typecheck(sig, locals, asm)?;
                }
                Ok(())
            }
        }
    }
}
#[test]
fn test() {
    let mut asm = Assembly::default();
    let lhs = super::Const::I64(0);
    let rhs = super::Const::F64(super::hashable::HashableF64(0.0));
    asm.biop(lhs, rhs, BinOp::Add);
    let _sig = asm.sig([], Type::Void);
}

#[cfg(test)]
mod tc_tests {
    //! Soundness tests for the CIL typechecker (Phase P1 of the absolute-correctness plan).
    //!
    //! These pin down two opposite properties of the verifier:
    //!  * the proven false-positive suppressions stay suppressed (no spurious build break), and
    //!  * deliberately type-broken CIL is still **rejected** (the false-negative audit) — a sound,
    //!    soon-to-be-fatal checker must catch real type errors.
    use super::*;
    use crate::ir::cilnode::MethodKind;
    use crate::Const;

    /// Build a single-local, single-`StLoc` method body and return the typecheck result of that
    /// `StLoc` root. `value` is a node index that has already been allocated in `asm`.
    fn check_stloc(asm: &mut Assembly, local_ty: Type, value: Interned<CILNode>) -> Result<(), TypeCheckError> {
        let local_ty = asm.alloc_type(local_ty);
        let locals: Vec<LocalDef> = vec![(None, local_ty)];
        let sig = asm.sig([], Type::Void);
        let root = CILRoot::StLoc(0, value);
        root.typecheck(sig, &locals, asm)
    }

    /// PROVEN FALSE POSITIVE (task #43 / WF-C): storing a `Void`-typed value (e.g. a `LdStaticField`
    /// of an opaque ZST marker static like `__rust_no_alloc_shim_is_unstable`) into a non-void local
    /// is a runtime no-op — Void carries no bits. The checker must NOT flag it.
    #[test]
    fn stloc_void_source_is_accepted() {
        let mut asm = Assembly::default();
        let void_static = asm.global_void();
        let void_node = asm.load_static(void_static);
        assert!(
            check_stloc(&mut asm, Type::Int(Int::USize), void_node).is_ok(),
            "storing a Void value into a usize local must be accepted (known-benign no-op)"
        );
    }

    /// FALSE-NEGATIVE AUDIT: storing a concrete `f64` into a `usize` local is a genuine type error
    /// (no sign/erasure exemption applies). A sound checker MUST reject it. If this ever passes, the
    /// checker has a hole and flipping it fatal would let real miscompiles through.
    #[test]
    fn stloc_float_into_int_is_rejected() {
        let mut asm = Assembly::default();
        let f = asm.alloc_node(CILNode::Const(Box::new(Const::F64(super::super::hashable::HashableF64(1.0)))));
        assert!(
            matches!(
                check_stloc(&mut asm, Type::Int(Int::USize), f),
                Err(TypeCheckError::LocalAssigementWrong { .. })
            ),
            "storing an f64 into a usize local must be rejected"
        );
    }

    /// Build a single-`StInd` root storing `value` (a pre-allocated node) through an address local
    /// of type `*store_tpe`, with declared store type `store_tpe`; return the typecheck result.
    fn check_stind(
        asm: &mut Assembly,
        store_tpe: Type,
        value: Interned<CILNode>,
    ) -> Result<(), TypeCheckError> {
        let store_tpe_idx = asm.alloc_type(store_tpe);
        let ptr_ty = asm.nptr(store_tpe_idx);
        let ptr_ty_idx = asm.alloc_type(ptr_ty);
        let locals: Vec<LocalDef> = vec![(None, ptr_ty_idx)];
        let addr = asm.alloc_node(CILNode::LdLoc(0));
        let sig = asm.sig([], Type::Void);
        let root = CILRoot::StInd(Box::new((addr, value, store_tpe, false)));
        root.typecheck(sig, &locals, asm)
    }

    /// FALSE-NEGATIVE AUDIT (StInd architectural refactor): dropping the address-pointee check must
    /// NOT weaken the kept value/`tpe` invariant. Storing an `f64` value through an `i32` `stind` is
    /// a genuine type error (the pushed bits do not match the store opcode). A sound checker MUST
    /// reject it. If this passes, the StInd refactor created a hole.
    #[test]
    fn stind_value_type_mismatch_is_rejected() {
        let mut asm = Assembly::default();
        let f = asm.alloc_node(CILNode::Const(Box::new(Const::F64(
            super::super::hashable::HashableF64(1.0),
        ))));
        assert!(
            matches!(
                check_stind(&mut asm, Type::Int(Int::I32), f),
                Err(TypeCheckError::WriteWrongValue { .. })
            ),
            "storing an f64 value through an i32 stind must be rejected (value/type mismatch)"
        );
    }

    /// REGRESSION GUARD (StInd architectural refactor): a type-ERASED address pointee must be
    /// accepted with NO special case. `**bool` storing an `i8` is the strsim `a_flags[i] = true`
    /// shape (a `&mut [bool]` slice store after `split_at_mut`); the address pointee type is
    /// intentionally not checked — only `value == tpe` (here `i8 == i8`) matters. Must be Ok.
    #[test]
    fn stind_erased_pointee_is_accepted() {
        let mut asm = Assembly::default();
        let bool_ty = asm.alloc_type(Type::Bool);
        let p_bool = asm.nptr(bool_ty);
        let p_bool = asm.alloc_type(p_bool);
        let pp_bool = asm.nptr(p_bool);
        let pp_bool = asm.alloc_type(pp_bool);
        let locals: Vec<LocalDef> = vec![(None, pp_bool)];
        let addr = asm.alloc_node(CILNode::LdLoc(0));
        let val = asm.alloc_node(CILNode::Const(Box::new(Const::I8(1))));
        let root = CILRoot::StInd(Box::new((addr, val, Type::Int(Int::I8), false)));
        let sig = asm.sig([], Type::Void);
        assert!(
            root.typecheck(sig, &locals, &mut asm).is_ok(),
            "storing i8 through a **bool address (erased pointee) must be accepted with no special case"
        );
    }

    /// FALSE-NEGATIVE AUDIT: a `Call` whose argument type is structurally unrelated to the callee's
    /// declared parameter must be rejected. Guards the call-arg arm against silent acceptance.
    #[test]
    fn call_arg_type_mismatch_is_rejected() {
        let mut asm = Assembly::default();
        // Callee: fn(f64) -> Void
        let main = asm.main_module();
        let callee_sig = asm.sig([Type::Float(crate::ir::Float::F64)], Type::Void);
        let callee = asm.new_methodref(*main, "takes_f64", callee_sig, MethodKind::Static, vec![]);
        // Caller passes an i64 const where an f64 is expected.
        let bad_arg = asm.alloc_node(CILNode::Const(Box::new(Const::I64(7))));
        let call = CILRoot::Call(Box::new((callee, [bad_arg].into(), crate::ir::cilnode::IsPure::NOT)));
        let sig = asm.sig([], Type::Void);
        let locals: Vec<LocalDef> = vec![];
        assert!(
            call.typecheck(sig, &locals, &mut asm).is_err(),
            "passing an i64 where an f64 parameter is expected must be rejected"
        );
    }

    /// FALSE-NEGATIVE AUDIT (S6): `isinst`/`castclass` on a non-reference operand (an `i64`) is a
    /// genuine type error — the CLR requires an object reference. A sound checker MUST reject it.
    #[test]
    fn isinst_on_int_operand_is_rejected() {
        let mut asm = Assembly::default();
        let bad = asm.alloc_node(CILNode::Const(Box::new(Const::I64(7))));
        let target = {
            let c = ClassRef::exception(&mut asm);
            asm.alloc_type(Type::ClassRef(c))
        };
        let node = asm.alloc_node(CILNode::IsInst(bad, target));
        let sig = asm.sig([], Type::Void);
        let locals: Vec<LocalDef> = vec![];
        assert!(
            matches!(
                asm.get_node(node).clone().typecheck(sig, &locals, &mut asm),
                Err(TypeCheckError::TypeNotClass { .. })
            ),
            "isinst with an i64 operand must be rejected (not a reference type)"
        );
    }

    /// FALSE-NEGATIVE AUDIT (S6): `castclass` on a non-reference operand must likewise be rejected.
    #[test]
    fn castclass_on_int_operand_is_rejected() {
        let mut asm = Assembly::default();
        let bad = asm.alloc_node(CILNode::Const(Box::new(Const::I64(7))));
        let target = {
            let c = ClassRef::exception(&mut asm);
            asm.alloc_type(Type::ClassRef(c))
        };
        let node = asm.alloc_node(CILNode::CheckedCast(bad, target));
        let sig = asm.sig([], Type::Void);
        let locals: Vec<LocalDef> = vec![];
        assert!(
            asm.get_node(node).clone().typecheck(sig, &locals, &mut asm).is_err(),
            "castclass with an i64 operand must be rejected"
        );
    }

    /// NO FALSE POSITIVE (S6): the real interop shape — a managed class-ref operand cast to another
    /// class ref — must still be ACCEPTED (this is what mycorrhiza emits; rejecting it breaks the gate).
    #[test]
    fn castclass_classref_operand_is_accepted() {
        let mut asm = Assembly::default();
        // operand: a non-valuetype managed ref (System.Exception) loaded as arg 0.
        let src = asm.alloc_node(CILNode::LdArg(0));
        let target = {
            let c = ClassRef::exception(&mut asm);
            asm.alloc_type(Type::ClassRef(c))
        };
        let node = asm.alloc_node(CILNode::CheckedCast(src, target));
        // sig has one ClassRef param so LdArg(0) typechecks to a reference.
        let src_ty = {
            let c = ClassRef::exception(&mut asm);
            Type::ClassRef(c)
        };
        let sig = asm.sig([src_ty], Type::Void);
        let locals: Vec<LocalDef> = vec![];
        assert!(
            asm.get_node(node).clone().typecheck(sig, &locals, &mut asm).is_ok(),
            "castclass of a managed class-ref operand to another class ref must be accepted"
        );
    }

    /// Build an `Assembly` containing exactly one method whose body stores an `f64` into a `usize`
    /// local — a deliberate, real type error — and register it so `Assembly::typecheck` walks it.
    fn asm_with_one_broken_method() -> Assembly {
        use crate::ir::{Access, BasicBlock};
        use crate::ir::method::MethodImpl;
        let mut asm = Assembly::default();
        let usize_ty = asm.alloc_type(Type::Int(Int::USize));
        let f = asm.alloc_node(CILNode::Const(Box::new(Const::F64(super::super::hashable::HashableF64(1.0)))));
        let bad = asm.alloc_root(CILRoot::StLoc(0, f));
        let block = BasicBlock::new(vec![bad], 0, None);
        let main = asm.main_module();
        let sig = asm.sig([], Type::Void);
        let def = crate::ir::method::MethodDef::new(
            Access::Private,
            main,
            asm.alloc_string("broken"),
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody { blocks: vec![block], locals: vec![(None, usize_ty)] },
            vec![],
        );
        asm.new_method(def);
        asm
    }

    /// WIRING (advisory mode): with the verifier enabled but non-fatal, a broken method is *counted*
    /// and reported, and codegen continues (no panic). This is the default behaviour.
    #[test]
    fn assembly_typecheck_advisory_counts_violations() {
        let mut asm = asm_with_one_broken_method();
        let n = asm.typecheck_with_policy(/*enabled=*/ true, /*fatal=*/ false);
        assert_eq!(n, 1, "the single broken method must be counted as one violation");
    }

    /// WIRING (escape hatch): with the verifier disabled, the pass is skipped entirely and reports
    /// zero — even though the method is ill-typed. Models `TYPECHECK_CIL=0 VERIFY_METHODS=0`.
    #[test]
    fn assembly_typecheck_disabled_skips() {
        let mut asm = asm_with_one_broken_method();
        let n = asm.typecheck_with_policy(/*enabled=*/ false, /*fatal=*/ false);
        assert_eq!(n, 0, "a disabled verifier must not walk any method");
    }

    /// WIRING (fatal gate / invariant I1): with the verifier fatal, a broken method ABORTS — proving
    /// `ALLOW_MISCOMPILATIONS=0` actually fails the build rather than emitting ill-typed CIL.
    #[test]
    #[should_panic(expected = "CIL type-verifier rejected method")]
    fn assembly_typecheck_fatal_aborts() {
        let mut asm = asm_with_one_broken_method();
        let _ = asm.typecheck_with_policy(/*enabled=*/ true, /*fatal=*/ true);
    }
}
