//! Managed fallbacks for exact x86 runtime intrinsics emitted by the pinned toolchain.

use crate::{
    BasicBlock, CILRoot, Const, MethodImpl,
    asm::{MissingMethodPatcher, RuntimeService},
};

use super::super::Assembly;

/// Registers the conservative managed fallback for LLVM's `xgetbv` intrinsic.
///
/// The pinned `std_detect` calls `llvm.x86.xgetbv(u32) -> i64` to inspect native XCR0 state before
/// enabling AVX-family features. Managed CIL cannot execute that privileged/native instruction or
/// truthfully infer its per-process result. Returning zero reports that no extended register state
/// is available, which keeps feature detection conservative and deterministic.
pub fn xgetbv_unavailable(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    patcher.insert_runtime_service(
        asm,
        RuntimeService::X86Xgetbv,
        Box::new(|_, asm| {
            let unavailable = asm.alloc_node(Const::I64(0));
            let ret = asm.alloc_root(CILRoot::Ret(unavailable));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        }),
    );
}
