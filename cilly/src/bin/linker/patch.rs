use cilly::{
    IString,
    {asm::MissingMethodPatcher, Assembly, CILNode, CILRoot, MethodRef, Type},
};

pub fn call_alias(
    overrides: &mut MissingMethodPatcher,
    asm: &mut Assembly,
    name: impl Into<IString>,
    call: MethodRef,
) {
    overrides.insert(
        asm.alloc_string(name),
        Box::new(move |original, asm| {
            let method_ref = asm.alloc_methodref(call.clone());
            let inputs: Box<[_]> = asm[call.sig()].inputs().into();
            let original_inputs: Box<[_]> =
                asm[asm[original].sig()].inputs().into();
            assert_eq!(
                inputs.len(),
                original_inputs.len(),
                "call alias must preserve arity"
            );
            let args: Box<_> = inputs
                .iter()
                .zip(original_inputs.iter())
                .enumerate()
                .map(|(argument, (target, source))| {
                    asm.adapt_call_argument(argument as u32, *source, *target)
                })
                .collect();
            if *asm[call.sig()].output() == Type::Void {
                let call = asm.alloc_root(CILRoot::call(method_ref, args));
                let ret = asm.alloc_root(CILRoot::VoidRet);
                cilly::MethodImpl::MethodBody {
                    blocks: vec![cilly::BasicBlock::new(vec![call, ret], 0, None)],
                    locals: vec![],
                }
            } else {
                let ret_value = asm.alloc_node(CILNode::call(method_ref, args));
                let target = *asm[asm[original].sig()].output();
                let source = *asm[call.sig()].output();
                let ret_value = asm.adapt_call_result(ret_value, source, target);
                let ret = asm.alloc_root(CILRoot::Ret(ret_value));
                cilly::MethodImpl::MethodBody {
                    blocks: vec![cilly::BasicBlock::new(vec![ret], 0, None)],
                    locals: vec![],
                }
            }
        }),
    );
}
