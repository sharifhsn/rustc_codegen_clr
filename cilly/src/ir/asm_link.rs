use super::{
    asm::{CCTOR, TCCTOR, USER_INIT},
    bimap::Interned,
    class::{ClassDefIdx, StaticFieldDef},
    Assembly, BasicBlock, CILNode, CILRoot, ClassDef, ClassRef, Const, FieldDesc, FnSig, MethodDef,
    MethodDefIdx, MethodRef, StaticFieldDesc, Type,
};
impl Assembly {
    pub(crate) fn translate_type(&mut self, source: &Self, tpe: Type) -> Type {
        match tpe {
            Type::Ptr(inner) => {
                let inner = self.translate_type(source, source[inner]);
                self.nptr(inner)
            }
            Type::Ref(inner) => {
                let inner = self.translate_type(source, source[inner]);
                self.nref(inner)
            }
            Type::Int(_)
            | Type::Float(_)
            | Type::PlatformString
            | Type::PlatformChar
            | Type::Bool
            | Type::Void
            | Type::PlatformObject
            | Type::PlatformGeneric(_, _)
            | Type::SIMDVector(_) => tpe,
            Type::ClassRef(class_ref) => {
                Type::ClassRef(self.translate_class_ref(source, class_ref))
            }
            Type::PlatformArray { elem, dims } => {
                let elem = self.translate_type(source, source[elem]);
                let elem = self.alloc_type(elem);
                Type::PlatformArray { elem, dims }
            }
            Type::FnPtr(sig) => {
                let sig = self.translate_sig(source, &source[sig]);
                Type::FnPtr(self.alloc_sig(sig))
            }
        }
    }
    pub(crate) fn translate_class_ref(
        &mut self,
        source: &Assembly,
        class_ref: Interned<ClassRef>,
    ) -> Interned<ClassRef> {
        let cref = source.class_ref(class_ref);

        let name = self.alloc_string(&source[cref.name()]);

        let asm = cref
            .asm()
            .map(|asm_name| self.alloc_string(&source[asm_name]));
        let generics = cref
            .generics()
            .iter()
            .map(|tpe| self.translate_type(source, *tpe))
            .collect();
        self.alloc_class_ref(ClassRef::new(name, asm, cref.is_valuetype(), generics))
    }
    pub(crate) fn translate_sig(&mut self, source: &Assembly, sig: &FnSig) -> FnSig {
        FnSig::new(
            sig.inputs()
                .iter()
                .map(|tpe| self.translate_type(source, *tpe))
                .collect::<Box<_>>(),
            self.translate_type(source, *sig.output()),
        )
    }
    pub(crate) fn translate_field(&mut self, source: &Assembly, field: FieldDesc) -> FieldDesc {
        let name = self.alloc_string(source[field.name()].as_ref());
        let owner = self.translate_class_ref(source, field.owner());
        let tpe = self.translate_type(source, field.tpe());
        FieldDesc::new(owner, name, tpe)
    }
    pub(crate) fn translate_static_field(
        &mut self,
        source: &Assembly,
        field: StaticFieldDesc,
    ) -> StaticFieldDesc {
        let name = self.alloc_string(source[field.name()].as_ref());
        let owner = self.translate_class_ref(source, field.owner());
        let tpe = self.translate_type(source, field.tpe());
        StaticFieldDesc::new(owner, name, tpe)
    }
    pub(crate) fn translate_method_ref(
        &mut self,
        source: &Assembly,
        method_ref: &MethodRef,
    ) -> MethodRef {
        let class = self.translate_class_ref(source, method_ref.class());
        let name = self.alloc_string(source[method_ref.name()].as_ref());
        let sig = self.translate_sig(source, &source[method_ref.sig()]);
        let sig = self.alloc_sig(sig);
        let generics = method_ref
            .generics()
            .iter()
            .map(|tpe| self.translate_type(source, *tpe))
            .collect();
        MethodRef::new(class, name, sig, method_ref.kind(), generics)
    }
    pub(crate) fn translate_const(&mut self, source: &Assembly, cst: &Const) -> Const {
        match cst {
            super::Const::PlatformString(pstr) => {
                super::Const::PlatformString(self.alloc_string(source[*pstr].as_ref()))
            }

            super::Const::Null(cref) => super::Const::Null(self.translate_class_ref(source, *cref)),
            super::Const::ByteBuffer { data, tpe } => {
                let tpe = self.translate_type(source, source[*tpe]);
                super::Const::ByteBuffer {
                    data: self.alloc_const_data(&source.const_data[*data]),
                    tpe: self.alloc_type(tpe),
                }
            }
            _ => cst.clone(),
        }
    }
    // The complexity of this function is unavoidable.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn translate_node(&mut self, source: &Assembly, node: CILNode) -> CILNode {
        match &node {
            CILNode::LdLoc(_) | CILNode::LdLocA(_) | CILNode::LdArg(_) | CILNode::LdArgA(_) => node,
            CILNode::Const(cst) => CILNode::Const(Box::new(self.translate_const(source, cst))),
            CILNode::BinOp(a, b, op) => {
                let a = self.translate_node(source, source.get_node(*a).clone());
                let b = self.translate_node(source, source.get_node(*b).clone());
                CILNode::BinOp(self.alloc_node(a), self.alloc_node(b), *op)
            }
            CILNode::UnOp(a, op) => {
                let a = self.translate_node(source, source.get_node(*a).clone());
                CILNode::UnOp(self.alloc_node(a), op.clone())
            }
            CILNode::Call(call_arg) => {
                let (mref, args, pure) = call_arg.as_ref();
                let method_ref = self.translate_method_ref(source, &source[*mref]);
                let mref = self.alloc_methodref(method_ref);
                let args = args
                    .iter()
                    .map(|arg| {
                        let arg = self.translate_node(source, source.get_node(*arg).clone());
                        self.alloc_node(arg)
                    })
                    .collect();
                CILNode::Call(Box::new((mref, args, *pure)))
            }
            CILNode::IntCast {
                input,
                target,
                extend,
            } => {
                let input = self.translate_node(source, source.get_node(*input).clone());
                let input = self.alloc_node(input);
                CILNode::IntCast {
                    input,
                    target: *target,
                    extend: *extend,
                }
            }
            CILNode::FloatCast {
                input,
                target,
                is_signed,
            } => {
                let input = self.translate_node(source, source.get_node(*input).clone());
                let input = self.alloc_node(input);
                CILNode::FloatCast {
                    input,
                    target: *target,
                    is_signed: *is_signed,
                }
            }
            CILNode::RefToPtr(input) => {
                let input = self.translate_node(source, source.get_node(*input).clone());
                let input = self.alloc_node(input);
                CILNode::RefToPtr(input)
            }
            CILNode::PtrCast(input, cast_res) => {
                let input = self.translate_node(source, source.get_node(*input).clone());
                let input = self.alloc_node(input);
                let cast_res = match cast_res.as_ref() {
                    crate::cilnode::PtrCastRes::Ptr(inner) => {
                        let inner = self.translate_type(source, source[*inner]);
                        crate::cilnode::PtrCastRes::Ptr(self.alloc_type(inner))
                    }
                    crate::cilnode::PtrCastRes::Ref(inner) => {
                        let inner = self.translate_type(source, source[*inner]);
                        crate::cilnode::PtrCastRes::Ref(self.alloc_type(inner))
                    }
                    crate::cilnode::PtrCastRes::FnPtr(sig) => {
                        let sig = self.translate_sig(source, &source[*sig]);
                        crate::cilnode::PtrCastRes::FnPtr(self.alloc_sig(sig))
                    }
                    crate::cilnode::PtrCastRes::USize | crate::cilnode::PtrCastRes::ISize => {
                        *cast_res.clone()
                    }
                };
                CILNode::PtrCast(input, Box::new(cast_res))
            }
            CILNode::LdFieldAddress { addr, field } => {
                let field = self.translate_field(source, *source.get_field(*field));
                let field = self.alloc_field(field);
                let addr = self.translate_node(source, source.get_node(*addr).clone());
                let addr = self.alloc_node(addr);
                CILNode::LdFieldAddress { addr, field }
            }
            CILNode::LdField { addr, field } => {
                let field = self.translate_field(source, *source.get_field(*field));
                let field = self.alloc_field(field);
                let addr = self.translate_node(source, source.get_node(*addr).clone());
                let addr = self.alloc_node(addr);
                CILNode::LdField { addr, field }
            }
            CILNode::LdInd {
                addr,
                tpe,
                volatile: volitale,
            } => {
                let addr = self.translate_node(source, source.get_node(*addr).clone());
                let addr = self.alloc_node(addr);
                let tpe = self.translate_type(source, source[*tpe]);
                let tpe = self.alloc_type(tpe);
                CILNode::LdInd {
                    addr,
                    tpe,
                    volatile: *volitale,
                }
            }
            CILNode::SizeOf(tpe) => {
                let tpe = self.translate_type(source, source[*tpe]);
                let tpe = self.alloc_type(tpe);
                CILNode::SizeOf(tpe)
            }
            CILNode::GetException => CILNode::GetException,
            CILNode::IsInst(object, tpe) => {
                let object = self.translate_node(source, source.get_node(*object).clone());
                let object = self.alloc_node(object);
                let tpe = self.translate_type(source, source[*tpe]);
                let tpe = self.alloc_type(tpe);
                CILNode::IsInst(object, tpe)
            }
            CILNode::CheckedCast(object, tpe) => {
                let object = self.translate_node(source, source.get_node(*object).clone());
                let object = self.alloc_node(object);
                let tpe = self.translate_type(source, source[*tpe]);
                let tpe = self.alloc_type(tpe);
                CILNode::CheckedCast(object, tpe)
            }
            CILNode::CallI(args) => {
                let (fnptr, sig, args) = args.as_ref();
                let fnptr = self.translate_node(source, source.get_node(*fnptr).clone());
                let fnptr = self.alloc_node(fnptr);
                let sig = self.translate_sig(source, &source[*sig]);
                let sig = self.alloc_sig(sig);
                let args = args
                    .iter()
                    .map(|arg| {
                        let arg = self.translate_node(source, source.get_node(*arg).clone());
                        self.alloc_node(arg)
                    })
                    .collect();
                CILNode::CallI(Box::new((fnptr, sig, args)))
            }
            CILNode::LocAlloc { size } => {
                let size = self.translate_node(source, source.get_node(*size).clone());
                let size = self.alloc_node(size);
                CILNode::LocAlloc { size }
            }
            CILNode::LdStaticField(sfld) => {
                let sfld = self.translate_static_field(source, *source.get_static_field(*sfld));
                let sfld = self.alloc_sfld(sfld);
                CILNode::LdStaticField(sfld)
            }
            CILNode::LdStaticFieldAddress(sfld) => {
                let sfld = self.translate_static_field(source, *source.get_static_field(*sfld));
                let sfld = self.alloc_sfld(sfld);
                CILNode::LdStaticFieldAddress(sfld)
            }
            CILNode::LdFtn(mref) => {
                let method_ref = self.translate_method_ref(source, &source[*mref]);
                let mref = self.alloc_methodref(method_ref);
                CILNode::LdFtn(mref)
            }
            CILNode::LdTypeToken(tpe) => {
                let tpe = self.translate_type(source, source[*tpe]);
                let tpe = self.alloc_type(tpe);
                CILNode::LdTypeToken(tpe)
            }
            CILNode::LdLen(len) => {
                let len = self.translate_node(source, source.get_node(*len).clone());
                let len = self.alloc_node(len);
                CILNode::LdLen(len)
            }
            CILNode::LocAllocAlgined { tpe, align } => {
                let tpe = self.translate_type(source, source[*tpe]);
                let tpe = self.alloc_type(tpe);
                CILNode::LocAllocAlgined { tpe, align: *align }
            }
            CILNode::LdElelemRef { array, index } => {
                let array = self.translate_node(source, source.get_node(*array).clone());
                let array = self.alloc_node(array);
                let index = self.translate_node(source, source.get_node(*index).clone());
                let index = self.alloc_node(index);
                CILNode::LdElelemRef { array, index }
            }
            CILNode::UnboxAny { object, tpe } => {
                let object = self.translate_node(source, source.get_node(*object).clone());
                let object = self.alloc_node(object);
                let tpe = self.translate_type(source, source[*tpe]);
                let tpe = self.alloc_type(tpe);
                CILNode::UnboxAny { object, tpe }
            }
            CILNode::Box { value, tpe } => {
                let value = self.translate_node(source, source.get_node(*value).clone());
                let value = self.alloc_node(value);
                let tpe = self.translate_type(source, source[*tpe]);
                let tpe = self.alloc_type(tpe);
                CILNode::Box { value, tpe }
            }
            CILNode::NewArr { elem, len } => {
                let elem = self.translate_type(source, source[*elem]);
                let elem = self.alloc_type(elem);
                let len = self.translate_node(source, source.get_node(*len).clone());
                let len = self.alloc_node(len);
                CILNode::NewArr { elem, len }
            }
        }
    }
    // The complexity of this function is unavoidable.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn translate_root(&mut self, source: &Assembly, root: CILRoot) -> CILRoot {
        match root {
            CILRoot::Unreachable(str) => {
                let str = self.alloc_string(&source[str]);
                CILRoot::Unreachable(str)
            }
            CILRoot::StLoc(loc, node) => {
                let node = self.translate_node(source, source.get_node(node).clone());
                let node = self.alloc_node(node);
                CILRoot::StLoc(loc, node)
            }
            CILRoot::StArg(loc, node) => {
                let node = self.translate_node(source, source.get_node(node).clone());
                let node = self.alloc_node(node);
                CILRoot::StArg(loc, node)
            }
            CILRoot::Ret(node) => {
                let node = self.translate_node(source, source.get_node(node).clone());
                let node = self.alloc_node(node);
                CILRoot::Ret(node)
            }
            CILRoot::Pop(node) => {
                let node = self.translate_node(source, source.get_node(node).clone());
                let node = self.alloc_node(node);
                CILRoot::Pop(node)
            }
            CILRoot::Throw(node) => {
                let node = self.translate_node(source, source.get_node(node).clone());
                let node = self.alloc_node(node);
                CILRoot::Throw(node)
            }
            CILRoot::Branch(branch) => {
                let (target, sub_target, cond) = branch.as_ref();
                let cond = cond.as_ref().map(|cond| match cond {
                    super::cilroot::BranchCond::True(cond) => {
                        let cond = self.translate_node(source, source.get_node(*cond).clone());
                        let cond = self.alloc_node(cond);
                        super::cilroot::BranchCond::True(cond)
                    }
                    super::cilroot::BranchCond::False(cond) => {
                        let cond = self.translate_node(source, source.get_node(*cond).clone());
                        let cond = self.alloc_node(cond);
                        super::cilroot::BranchCond::False(cond)
                    }
                    super::cilroot::BranchCond::Eq(a, b) => {
                        let a = self.translate_node(source, source.get_node(*a).clone());
                        let a = self.alloc_node(a);
                        let b = self.translate_node(source, source.get_node(*b).clone());
                        let b = self.alloc_node(b);
                        super::cilroot::BranchCond::Eq(a, b)
                    }
                    super::cilroot::BranchCond::Ne(a, b) => {
                        let a = self.translate_node(source, source.get_node(*a).clone());
                        let a = self.alloc_node(a);
                        let b = self.translate_node(source, source.get_node(*b).clone());
                        let b = self.alloc_node(b);
                        super::cilroot::BranchCond::Ne(a, b)
                    }
                    super::cilroot::BranchCond::Lt(a, b, cmp_kind) => {
                        let a = self.translate_node(source, source.get_node(*a).clone());
                        let a = self.alloc_node(a);
                        let b = self.translate_node(source, source.get_node(*b).clone());
                        let b = self.alloc_node(b);
                        super::cilroot::BranchCond::Lt(a, b, cmp_kind.clone())
                    }
                    super::cilroot::BranchCond::Gt(a, b, cmp_kind) => {
                        let a = self.translate_node(source, source.get_node(*a).clone());
                        let a = self.alloc_node(a);
                        let b = self.translate_node(source, source.get_node(*b).clone());
                        let b = self.alloc_node(b);
                        super::cilroot::BranchCond::Gt(a, b, cmp_kind.clone())
                    }
                    super::cilroot::BranchCond::Le(a, b, cmp_kind) => {
                        let a = self.translate_node(source, source.get_node(*a).clone());
                        let a = self.alloc_node(a);
                        let b = self.translate_node(source, source.get_node(*b).clone());
                        let b = self.alloc_node(b);
                        super::cilroot::BranchCond::Le(a, b, cmp_kind.clone())
                    }
                    super::cilroot::BranchCond::Ge(a, b, cmp_kind) => {
                        let a = self.translate_node(source, source.get_node(*a).clone());
                        let a = self.alloc_node(a);
                        let b = self.translate_node(source, source.get_node(*b).clone());
                        let b = self.alloc_node(b);
                        super::cilroot::BranchCond::Ge(a, b, cmp_kind.clone())
                    }
                });
                CILRoot::Branch(Box::new((*target, *sub_target, cond)))
            }
            CILRoot::VoidRet | CILRoot::Break | CILRoot::Nop | CILRoot::ReThrow => root,
            CILRoot::SourceFileInfo {
                line_start,
                line_len,
                col_start,
                col_len,
                file,
            } => {
                let file = self.alloc_string(source[file].as_ref());
                CILRoot::SourceFileInfo {
                    line_start,
                    line_len,
                    col_start,
                    col_len,
                    file,
                }
            }
            CILRoot::SetField(info) => {
                let (field, addr, val) = info.as_ref();
                let field = self.translate_field(source, *source.get_field(*field));
                let field = self.alloc_field(field);
                let addr = self.translate_node(source, source.get_node(*addr).clone());
                let addr = self.alloc_node(addr);
                let val = self.translate_node(source, source.get_node(*val).clone());
                let val = self.alloc_node(val);
                CILRoot::SetField(Box::new((field, addr, val)))
            }
            CILRoot::Call(call_arg) => {
                let (mref, args, pure) = call_arg.as_ref();
                let method_ref = self.translate_method_ref(source, &source[*mref]);
                let mref = self.alloc_methodref(method_ref);
                let args = args
                    .iter()
                    .map(|arg| {
                        let arg = self.translate_node(source, source.get_node(*arg).clone());
                        self.alloc_node(arg)
                    })
                    .collect();
                CILRoot::Call(Box::new((mref, args, *pure)))
            }
            CILRoot::StInd(info) => {
                let (addr, val, tpe, volitile) = info.as_ref();
                let addr = self.translate_node(source, source.get_node(*addr).clone());
                let addr = self.alloc_node(addr);
                let val = self.translate_node(source, source.get_node(*val).clone());
                let val = self.alloc_node(val);
                let tpe = self.translate_type(source, *tpe);
                CILRoot::StInd(Box::new((addr, val, tpe, *volitile)))
            }
            CILRoot::CpObj { src, dst, tpe } => {
                let src = self.translate_node(source, source.get_node(src).clone());
                let src = self.alloc_node(src);
                let dst = self.translate_node(source, source.get_node(dst).clone());
                let dst = self.alloc_node(dst);
                let tpe = self.translate_type(source, source[tpe]);
                CILRoot::CpObj {
                    src,
                    dst,
                    tpe: self.alloc_type(tpe),
                }
            }
            CILRoot::InitObj(src, tpe) => {
                let addr = self.translate_node(source, source.get_node(src).clone());
                let addr = self.alloc_node(addr);

                let tpe = self.translate_type(source, source[tpe]);
                CILRoot::InitObj(addr, self.alloc_type(tpe))
            }
            CILRoot::InitBlk(info) => {
                let (dst, val, count) = info.as_ref();
                let dst = self.translate_node(source, source.get_node(*dst).clone());
                let dst = self.alloc_node(dst);
                let val = self.translate_node(source, source.get_node(*val).clone());
                let val = self.alloc_node(val);
                let count = self.translate_node(source, source.get_node(*count).clone());
                let count = self.alloc_node(count);
                CILRoot::InitBlk(Box::new((dst, val, count)))
            }
            CILRoot::CpBlk(info) => {
                let (dst, src, len) = info.as_ref();
                let dst = self.translate_node(source, source.get_node(*dst).clone());
                let dst = self.alloc_node(dst);
                let src = self.translate_node(source, source.get_node(*src).clone());
                let src = self.alloc_node(src);
                let len = self.translate_node(source, source.get_node(*len).clone());
                let len = self.alloc_node(len);
                CILRoot::CpBlk(Box::new((dst, src, len)))
            }
            CILRoot::CallI(args) => {
                let (fnptr, sig, args) = args.as_ref();
                let fnptr = self.translate_node(source, source.get_node(*fnptr).clone());
                let fnptr = self.alloc_node(fnptr);
                let sig = self.translate_sig(source, &source[*sig]);
                let sig = self.alloc_sig(sig);
                let args = args
                    .iter()
                    .map(|arg| {
                        let arg = self.translate_node(source, source.get_node(*arg).clone());
                        self.alloc_node(arg)
                    })
                    .collect();
                CILRoot::CallI(Box::new((fnptr, sig, args)))
            }
            CILRoot::ExitSpecialRegion { target, source } => {
                CILRoot::ExitSpecialRegion { target, source }
            }
            CILRoot::TerminateRegion { protected, reason } => {
                // The protected child root is NOT in any block's root list (only the region and the
                // continuation `goto` are), so it must be translated + re-interned here explicitly.
                let inner = self.translate_root(source, source.get_root(protected).clone());
                let protected = self.alloc_root(inner);
                CILRoot::TerminateRegion { protected, reason }
            }
            CILRoot::SetStaticField { field, val } => {
                let val = self.translate_node(source, source.get_node(val).clone());
                let val = self.alloc_node(val);
                let field = self.translate_static_field(source, *source.get_static_field(field));
                let field = self.alloc_sfld(field);
                CILRoot::SetStaticField { field, val }
            }
            CILRoot::StElem {
                array,
                index,
                value,
                elem,
            } => {
                let array = self.translate_node(source, source.get_node(array).clone());
                let array = self.alloc_node(array);
                let index = self.translate_node(source, source.get_node(index).clone());
                let index = self.alloc_node(index);
                let value = self.translate_node(source, source.get_node(value).clone());
                let value = self.alloc_node(value);
                let elem = self.translate_type(source, source[elem]);
                let elem = self.alloc_type(elem);
                CILRoot::StElem {
                    array,
                    index,
                    value,
                    elem,
                }
            }
        }
    }
    pub(crate) fn translate_block(&mut self, source: &Assembly, block: &BasicBlock) -> BasicBlock {
        let roots = block
            .roots()
            .iter()
            .map(|root| {
                let root = self.translate_root(source, source.get_root(*root).clone());
                self.alloc_root(root)
            })
            .collect();
        let handler = block.handler().map(|blocks| {
            blocks
                .iter()
                .map(|block| self.translate_block(source, block))
                .collect()
        });
        match handler {
            Some(handler) => {
                debug_assert!(block.handler_id().is_none());
                BasicBlock::new(roots, block.block_id(), Some(handler))
            }
            None => BasicBlock::new_raw(roots, block.block_id(), block.handler_id()),
        }
    }
    pub(crate) fn translate_method_def(&mut self, source: &Assembly, def: &MethodDef) -> MethodDef {
        let class = self.translate_class_ref(source, *def.class());

        // OK, becuase our caller translates the parrent of this class too.
        let class = ClassDefIdx::from_raw(class);
        let name = self.alloc_string(source[def.name()].as_ref());
        let sig = self.translate_sig(source, &source[def.sig()]);
        let sig = self.alloc_sig(sig);
        let method_impl = match def.implementation() {
            super::MethodImpl::MethodBody { blocks, locals } => {
                let blocks = blocks
                    .iter()
                    .map(|block| self.translate_block(source, block))
                    .collect();
                let locals = locals
                    .iter()
                    .map(|(name, tpe)| {
                        let tpe = self.translate_type(source, source[*tpe]);
                        (
                            name.map(|name| self.alloc_string(source[name].as_ref())),
                            self.alloc_type(tpe),
                        )
                    })
                    .collect();
                super::MethodImpl::MethodBody { blocks, locals }
            }
            super::MethodImpl::Extern {
                lib,
                preserve_errno,
            } => {
                let lib = self.alloc_string(source[*lib].as_ref());
                super::MethodImpl::Extern {
                    lib,
                    preserve_errno: *preserve_errno,
                }
            }
            super::MethodImpl::AliasFor(mref) => {
                let method_ref = self.translate_method_ref(source, &source[*mref]);
                let mref = self.alloc_methodref(method_ref);
                super::MethodImpl::AliasFor(mref)
            }
            super::MethodImpl::Missing => super::MethodImpl::Missing,
        };
        let arg_names = def
            .arg_names()
            .iter()
            .map(|arg| arg.map(|arg| self.alloc_string(source[arg].as_ref())))
            .collect();
        let mut translated = MethodDef::new(
            *def.access(),
            class,
            name,
            sig,
            def.kind(),
            method_impl,
            arg_names,
        );
        if let Some(base) = def.overrides() {
            let base_ref = self.translate_method_ref(source, &source[base]);
            let base_ref = self.alloc_methodref(base_ref);
            translated = translated.with_override(base_ref);
        }
        if def.is_abstract() {
            translated = translated.with_abstract();
        }
        // `[out]` param flags are plain data (no interned handles to translate) but MUST be
        // re-applied here: this field-wise reconstruction would otherwise silently drop them
        // exactly when a `.bc` crosses the linker — i.e. in every real build (the cd_interface
        // `IsOut` reflection check exists to catch this).
        if !def.out_params().is_empty() {
            translated = translated.with_out_params(def.out_params().to_vec());
        }
        // Generic-method-definition parameter NAMES (`MethodDef::generic_params`): interned
        // strings, re-alloc'd into the target assembly. Same silent-drop hazard as `out_params`
        // just above — this field-wise reconstruction runs in every real build (the `.bc` crosses
        // the linker), so forgetting it here would strip `SIG_GENERIC`/`GenericParam` rows from
        // every linked output while unit tests (no linker) kept passing.
        if !def.generic_params().is_empty() {
            let names = def
                .generic_params()
                .iter()
                .map(|n| self.alloc_string(source[*n].as_ref()))
                .collect();
            translated = translated.with_generic_params(names);
        }
        // `MethodDef::is_special_name` (e.g. a CLR operator-overload method — see that field's
        // doc): plain data, no interned handles to translate, but MUST be re-applied here for the
        // exact same reason as `out_params`/`generic_params` above — this field-wise
        // reconstruction runs in every real build (the `.bc` crosses the linker), so forgetting it
        // here silently drops `SpecialName` from every operator method in a linked output, even
        // though `MethodDef::with_special_name` was correctly applied on the ORIGINAL def before
        // it ever reached this translation. Found via a reflection probe (`IsSpecialName` read
        // back `false` after linking despite the backend stamping `true` before serialization) —
        // the same silent-drop hazard class the two comments above already document, now hit a
        // third time.
        if def.is_special_name() {
            translated = translated.with_special_name();
        }
        translated
    }
    /// Re-interns one `CustomAttrArg` from `source`'s heaps into `self`'s — a `Str` payload needs
    /// re-allocation (its `Interned<IString>` handle is only valid within `source`); every other
    /// variant is `Copy` data with no cross-assembly identity to translate.
    fn translate_custom_attr_arg(
        &mut self,
        source: &Assembly,
        arg: &super::class::CustomAttrArg,
    ) -> super::class::CustomAttrArg {
        match arg {
            super::class::CustomAttrArg::Str(s) => {
                super::class::CustomAttrArg::Str(self.alloc_string(source[*s].as_ref()))
            }
            super::class::CustomAttrArg::Bool(b) => super::class::CustomAttrArg::Bool(*b),
            super::class::CustomAttrArg::I32(i) => super::class::CustomAttrArg::I32(*i),
            super::class::CustomAttrArg::I64(i) => super::class::CustomAttrArg::I64(*i),
        }
    }

    pub(crate) fn translate_class_def(&mut self, source: &Assembly, def: &ClassDef) -> ClassDef {
        let name = self.alloc_string(source[def.name()].as_ref());
        let extends = def
            .extends()
            .map(|cref| self.translate_class_ref(source, cref));
        let fields = def
            .fields()
            .iter()
            .map(|(tpe, name, offset)| {
                let tpe = self.translate_type(source, *tpe);
                let name = self.alloc_string(source[*name].as_ref());
                (tpe, name, *offset)
            })
            .collect();
        let static_fields = def
            .static_fields()
            .iter()
            .map(
                |StaticFieldDef {
                     tpe,
                     name,
                     is_tls,
                     default_value,
                     is_const,
                 }| {
                    let tpe = self.translate_type(source, *tpe);
                    let name = self.alloc_string(source[*name].as_ref());
                    StaticFieldDef {
                        tpe,
                        name,
                        is_tls: *is_tls,
                        default_value: default_value.map(|cst| self.translate_const(source, &cst)),
                        is_const: *is_const,
                    }
                },
            )
            .collect();
        let mut translated = ClassDef::new(
            name,
            def.is_valuetype(),
            def.generics(),
            extends,
            fields,
            static_fields,
            *def.access(),
            def.explict_size(),
            def.align(),
            def.has_nonveralpping_layout(),
        );
        if def.is_valuetype_authoritative() {
            translated = translated.with_valuetype_authoritative();
        }
        if def.is_interface() {
            translated = translated.with_interface();
        }
        // Carry the implemented-interface set across the assembly boundary.
        for iface in def.implements() {
            let iface = self.translate_class_ref(source, *iface);
            translated.add_interface(iface);
        }
        // Carry declared events across the assembly boundary (same reasoning as `implements`).
        for ev in def.events() {
            let name = self.alloc_string(source[ev.name()].as_ref());
            let delegate = self.translate_type(source, ev.delegate());
            let add = self.translate_method_ref(source, &source[ev.add()]);
            let add = self.alloc_methodref(add);
            let remove = self.translate_method_ref(source, &source[ev.remove()]);
            let remove = self.alloc_methodref(remove);
            translated.add_event(super::class::EventDef::new(name, delegate, add, remove));
        }
        // Carry declared properties across the assembly boundary (same silent-drop hazard as
        // `events`/`out_params`: this field-wise reconstruction runs in every real build — a
        // `.bc` always crosses the linker — so forgetting it here would strip every
        // Property/PropertyMap/MethodSemantics row from linked output while unit tests, which
        // have no linker, kept passing).
        for prop in def.properties() {
            let name = self.alloc_string(source[prop.name()].as_ref());
            let tpe = self.translate_type(source, prop.tpe());
            let getter = prop.getter().map(|g| {
                let mref = self.translate_method_ref(source, &source[g]);
                self.alloc_methodref(mref)
            });
            let setter = prop.setter().map(|s| {
                let mref = self.translate_method_ref(source, &source[s]);
                self.alloc_methodref(mref)
            });
            translated.add_property(super::class::PropertyDef::new(name, tpe, getter, setter));
        }
        // Carry attached custom attributes across the assembly boundary (same silent-drop hazard
        // documented on `events`/`properties` above — a real build always crosses the linker, so
        // skipping this would strip every `CustomAttribute` row from linked output while unit
        // tests, which never link, kept passing).
        for attr in def.custom_attributes() {
            let attr_type = self.translate_class_ref(source, attr.attr_type());
            let mut ctor_args = Vec::with_capacity(attr.ctor_args().len());
            for arg in attr.ctor_args() {
                ctor_args.push(self.translate_custom_attr_arg(source, arg));
            }
            let mut named_args = Vec::with_capacity(attr.named_args().len());
            for (name, arg) in attr.named_args() {
                let name = self.alloc_string(source[*name].as_ref());
                let arg = self.translate_custom_attr_arg(source, arg);
                named_args.push((name, arg));
            }
            translated.add_custom_attribute(super::class::CustomAttrDef::new(
                attr_type, ctor_args, named_args,
            ));
        }
        // Carry a generic type DEFINITION's declared parameter names across the assembly
        // boundary (re-interned into THIS assembly's string heap). Must run BEFORE `ref_to()`
        // below: a nonzero-arity def without its names fails `ref_to`'s consistency assert —
        // deliberately loud, so a missed translation can never silently drop `GenericParam` rows.
        if !def.generic_names().is_empty() {
            let names = def
                .generic_names()
                .iter()
                .map(|n| self.alloc_string(source[*n].as_ref()))
                .collect();
            translated = translated.with_type_generic_names(names);
        }
        let class_ref = self.alloc_class_ref(translated.ref_to());
        let (defs_mut, _) = self.class_defs_mut_strings();
        match defs_mut.entry(ClassDefIdx(class_ref)) {
            std::collections::hash_map::Entry::Occupied(mut occupied) => {
                occupied.get_mut().merge_defs(translated.clone());
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(translated.clone());
            }
        }

        def.methods().iter().for_each(|mdef| {
            let mut method_def = self.translate_method_def(source, source.method_def(*mdef));
            let method_ref = self.alloc_methodref(method_def.ref_to());
            // 1st Take the orignal method, if it exists(we need this to be able to mutate methods)
            let original = self.method_defs().get(&MethodDefIdx(method_ref));
            let method_def = match original {
                Some(original) => {
                    assert_eq!(method_def.name(), original.name());
                    // Check if this method has a special name, and needs merging.
                    let name = &self[method_def.name()];
                    if SPECIAL_METHOD_NAMES.iter().any(|val| **val == *name) {
                        // Needs special handling.
                        assert_eq!(method_def.access(), original.access());
                        assert_eq!(method_def.class(), original.class());
                        assert_eq!(method_def.sig(), original.sig());
                        assert_eq!(method_def.kind(), original.kind());
                        method_def
                            .implementation_mut()
                            .merge_cctor_impls(original.implementation(), self);
                        method_def
                    } else {
                        // Not special, proly does not need merging, so we can check if it matches and go on our merry way.
                        assert_eq!(method_def.access(), original.access());
                        assert_eq!(method_def.class(), original.class());
                        assert_eq!(method_def.sig(), original.sig());
                        assert_eq!(method_def.kind(), original.kind());
                        method_def
                    }
                }
                None => method_def,
            };
            self.new_method(method_def);
        });
        translated
    }
}
const SPECIAL_METHOD_NAMES: &[&str] = &[CCTOR, TCCTOR, USER_INIT];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ir::cilnode::MethodKind, Access, Const, IString, Int, MethodImpl, Type};

    fn add_void_method(
        asm: &mut Assembly,
        name: &str,
        blocks: Vec<BasicBlock>,
    ) -> Interned<IString> {
        let owner = asm.main_module();
        let name = asm.alloc_string(name);
        let sig = asm.sig([], Type::Void);
        asm.new_method(MethodDef::new(
            Access::Public,
            owner,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::MethodBody {
                blocks,
                locals: vec![],
            },
            vec![],
        ));
        name
    }

    fn seed_destination_ids(asm: &mut Assembly) {
        let _ = asm.alloc_string("destination-only-string");
        let _ = asm.alloc_type(Type::Int(Int::U16));
        let node = asm.alloc_node(Const::I32(7));
        let _ = asm.alloc_root(CILRoot::Pop(node));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        add_void_method(
            asm,
            "destination_only_method",
            vec![BasicBlock::new(vec![ret], 0, None)],
        );
    }

    #[test]
    fn link_preserves_unresolved_basic_block_handler_id() {
        let mut destination = Assembly::default();
        seed_destination_ids(&mut destination);

        let mut source = Assembly::default();
        let ret = source.alloc_root(CILRoot::VoidRet);
        let source_name = add_void_method(
            &mut source,
            "source_with_unresolved_handler",
            vec![BasicBlock::new_raw(vec![ret], 17, Some(91))],
        );

        let linked = destination.link(source);
        let method = linked
            .method_defs()
            .values()
            .find(|method| &linked[method.name()] == "source_with_unresolved_handler")
            .expect("linked source method");
        assert_ne!(method.name().inner(), source_name.inner());
        let MethodImpl::MethodBody { blocks, .. } = method.implementation() else {
            panic!("source method must keep its body");
        };
        assert_eq!(blocks[0].block_id(), 17);
        assert_eq!(blocks[0].handler_id(), Some(91));
        assert!(blocks[0].handler().is_none());
    }

    #[test]
    fn link_preserves_valuetype_authority_with_relocated_ids() {
        let mut destination = Assembly::default();
        seed_destination_ids(&mut destination);

        let mut source = Assembly::default();
        let authoritative_name = source.alloc_string("AuthoritativeValueType");
        source
            .class_def(
                ClassDef::new(
                    authoritative_name,
                    true,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Public,
                    None,
                    None,
                    true,
                )
                .with_valuetype_authoritative(),
            )
            .unwrap();
        let placeholder_name = source.alloc_string("NonAuthoritativePlaceholder");
        source
            .class_def(ClassDef::new(
                placeholder_name,
                false,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                None,
                None,
                true,
            ))
            .unwrap();

        let linked = destination.link(source);
        let authoritative = linked
            .class_defs()
            .values()
            .find(|def| &linked[def.name()] == "AuthoritativeValueType")
            .expect("linked authoritative type");
        assert_ne!(authoritative.name().inner(), authoritative_name.inner());
        assert!(authoritative.is_valuetype());
        assert!(authoritative.is_valuetype_authoritative());

        let placeholder = linked
            .class_defs()
            .values()
            .find(|def| &linked[def.name()] == "NonAuthoritativePlaceholder")
            .expect("linked placeholder type");
        assert!(!placeholder.is_valuetype_authoritative());
    }
}
