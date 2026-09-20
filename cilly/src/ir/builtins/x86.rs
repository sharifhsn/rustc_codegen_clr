//! Managed fallbacks for exact x86 runtime intrinsics emitted by the pinned toolchain.

use crate::{
    BasicBlock, CILRoot, Const, MethodImpl, Type,
    asm::{MissingMethodPatcher, RuntimeService},
    cilnode::MethodKind,
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

/// Registers the managed no-op for LLVM's `vzeroupper` instruction.
///
/// `vzeroupper` only clears the x86 AVX upper-register transition state. CIL has no exposed
/// register file, so there is no Rust-visible state to preserve and the correct managed lowering
/// is an explicit void no-op. Keep the ABI check here so a future LLVM intrinsic with the same
/// spelling but a different signature cannot silently receive the wrong body.
pub fn vzeroupper_noop(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("llvm.x86.avx.vzeroupper");
    patcher.insert(
        name,
        Box::new(|mref, asm| {
            let signature = &asm[asm[mref].sig()];
            assert!(
                signature.inputs().is_empty()
                    && *signature.output() == Type::Void
                    && asm[mref].kind() == MethodKind::Static,
                "llvm.x86.avx.vzeroupper must have a static () -> void signature"
            );
            let ret = asm.alloc_root(CILRoot::VoidRet);
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        }),
    );
}
