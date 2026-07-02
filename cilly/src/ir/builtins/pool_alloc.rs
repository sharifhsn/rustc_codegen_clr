//! Optional pooled unmanaged allocator for the Rust/.NET allocation shims.
//!
//! The public allocator hooks stay default-direct. When `POOL_ALLOC=1`, they call
//! these internal helpers, which keep per-thread free lists for small power-of-two
//! size classes and pass larger requests through to `NativeMemory`.

use crate::Access;

use super::ALLOC_CAP;
use crate::ir::asm::MissingMethodPatcher;
use crate::ir::cilnode::{ExtendKind, MethodKind};
use crate::ir::cilroot::{BranchCond, CmpKind};
use crate::ir::{
    BasicBlock, BinOp, CILNode, CILRoot, ClassRef, Const, Int, MethodDef, MethodImpl, MethodRef,
    StaticFieldDesc, Type,
};
use crate::Assembly;

const SIZE_CLASSES: [u64; 11] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192];
const SLAB_SIZE: u64 = 1024 * 1024;
const SLAB_ALIGN: u64 = 8192;

const POOL_ALLOC_NAME: &str = "__rcl_pool_alloc";
const POOL_FREE_NAME: &str = "__rcl_pool_free";
const POOL_REALLOC_NAME: &str = "__rcl_pool_realloc";

pub(super) fn insert_pool_helpers(asm: &mut Assembly) {
    ensure_pool_alloc_method(asm);
    ensure_pool_free_method(asm);
    ensure_pool_realloc_method(asm);
}

pub(super) fn pool_alloc_mref(asm: &mut Assembly) -> crate::Interned<MethodRef> {
    let main = *asm.main_module();
    let name = asm.alloc_string(POOL_ALLOC_NAME);
    let void_ptr = asm.nptr(Type::Void);
    let sig = asm.sig([Type::Int(Int::USize), Type::Int(Int::USize)], void_ptr);
    asm.alloc_methodref(MethodRef::new(
        main,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ))
}

pub(super) fn pool_free_mref(asm: &mut Assembly) -> crate::Interned<MethodRef> {
    let main = *asm.main_module();
    let name = asm.alloc_string(POOL_FREE_NAME);
    let void_ptr = asm.nptr(Type::Void);
    let sig = asm.sig(
        [void_ptr, Type::Int(Int::USize), Type::Int(Int::USize)],
        Type::Void,
    );
    asm.alloc_methodref(MethodRef::new(
        main,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ))
}

pub(super) fn pool_realloc_mref(asm: &mut Assembly) -> crate::Interned<MethodRef> {
    let main = *asm.main_module();
    let name = asm.alloc_string(POOL_REALLOC_NAME);
    let void_ptr = asm.nptr(Type::Void);
    let sig = asm.sig(
        [
            void_ptr,
            Type::Int(Int::USize),
            Type::Int(Int::USize),
            Type::Int(Int::USize),
        ],
        void_ptr,
    );
    asm.alloc_methodref(MethodRef::new(
        main,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ))
}

pub(super) fn insert_rust_alloc(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("__rust_alloc");
    let generator = move |_, asm: &mut Assembly| {
        let size = asm.alloc_node(CILNode::LdArg(0));
        let align = super::load_align_usize(asm, 1);
        let alloc_mref = pool_alloc_mref(asm);
        let alloc = asm.alloc_node(CILNode::call(alloc_mref, [size, align]));
        let ret = asm.alloc_root(CILRoot::Ret(alloc));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

pub(super) fn insert_rust_alloc_zeroed(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("__rust_alloc_zeroed");
    let generator = move |_, asm: &mut Assembly| {
        let size = asm.alloc_node(CILNode::LdArg(0));
        let align = super::load_align_usize(asm, 1);
        let alloc_mref = pool_alloc_mref(asm);
        let alloc = asm.alloc_node(CILNode::call(alloc_mref, [size, align]));
        let st_alloc = asm.alloc_root(CILRoot::StLoc(0, alloc));
        let alloc_ld = asm.alloc_node(CILNode::LdLoc(0));
        let alloc_word = asm.cast_ptr_to(alloc_ld, Type::Int(Int::USize));
        let null_check = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::False(alloc_word)),
        ))));
        let zero = asm.alloc_node(Const::U8(0));
        let alloc_ld = asm.alloc_node(CILNode::LdLoc(0));
        let zero = asm.alloc_root(CILRoot::InitBlk(Box::new((alloc_ld, zero, size))));
        let alloc_ld = asm.alloc_node(CILNode::LdLoc(0));
        let ret = asm.alloc_root(CILRoot::Ret(alloc_ld));
        let null = asm.alloc_node(Const::USize(0));
        let ret_null = asm.alloc_root(CILRoot::Ret(null));
        let void_ptr = asm.nptr(Type::Void);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![st_alloc, null_check, zero, ret], 0, None),
                BasicBlock::new(vec![ret_null], 1, None),
            ],
            locals: vec![(None, asm.alloc_type(void_ptr))],
        }
    };
    patcher.insert(name, Box::new(generator));
}

pub(super) fn insert_rust_realloc(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("__rust_realloc");
    let generator = move |_, asm: &mut Assembly| {
        let ptr = super::load_ptr_arg(asm, 0);
        let old_size = asm.alloc_node(CILNode::LdArg(1));
        let old_size = asm.alloc_node(CILNode::IntCast {
            input: old_size,
            target: Int::USize,
            extend: ExtendKind::ZeroExtend,
        });
        let align = super::load_align_usize(asm, 2);
        let new_size = asm.alloc_node(CILNode::LdArg(3));
        let new_size = asm.alloc_node(CILNode::IntCast {
            input: new_size,
            target: Int::USize,
            extend: ExtendKind::ZeroExtend,
        });
        let realloc_mref = pool_realloc_mref(asm);
        let realloc = asm.alloc_node(CILNode::call(
            realloc_mref,
            [ptr, old_size, align, new_size],
        ));
        let ret = asm.alloc_root(CILRoot::Ret(realloc));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

pub(super) fn insert_rust_dealloc(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("__rust_dealloc");
    let generator = move |_, asm: &mut Assembly| {
        let ptr = super::load_ptr_arg(asm, 0);
        let size = asm.alloc_node(CILNode::LdArg(1));
        let size = asm.alloc_node(CILNode::IntCast {
            input: size,
            target: Int::USize,
            extend: ExtendKind::ZeroExtend,
        });
        let align = super::load_align_usize(asm, 2);
        let free_mref = pool_free_mref(asm);
        let free = asm.alloc_root(CILRoot::call(free_mref, [ptr, size, align]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![free, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

fn ensure_pool_alloc_method(asm: &mut Assembly) {
    let main = asm.main_module();
    let void_ptr = asm.nptr(Type::Void);
    let usize_ty = Type::Int(Int::USize);
    let sig = asm.sig([usize_ty, usize_ty], void_ptr);
    let name = asm.alloc_string(POOL_ALLOC_NAME);
    let size_name = asm.alloc_string("size");
    let align_name = asm.alloc_string("align");
    let implementation = MethodImpl::MethodBody {
        blocks: pool_alloc_blocks(asm),
        locals: vec![
            (Some(asm.alloc_string("head")), asm.alloc_type(void_ptr)),
            (Some(asm.alloc_string("slab")), asm.alloc_type(void_ptr)),
            (Some(asm.alloc_string("idx")), asm.alloc_type(usize_ty)),
            (Some(asm.alloc_string("current")), asm.alloc_type(void_ptr)),
        ],
    };
    asm.new_method(MethodDef::new(
        Access::Private,
        main,
        name,
        sig,
        MethodKind::Static,
        implementation,
        vec![Some(size_name), Some(align_name)],
    ));
}

fn ensure_pool_free_method(asm: &mut Assembly) {
    let main = asm.main_module();
    let void_ptr = asm.nptr(Type::Void);
    let usize_ty = Type::Int(Int::USize);
    let sig = asm.sig([void_ptr, usize_ty, usize_ty], Type::Void);
    let name = asm.alloc_string(POOL_FREE_NAME);
    let ptr_name = asm.alloc_string("ptr");
    let size_name = asm.alloc_string("size");
    let align_name = asm.alloc_string("align");
    let implementation = MethodImpl::MethodBody {
        blocks: pool_free_blocks(asm),
        locals: vec![],
    };
    asm.new_method(MethodDef::new(
        Access::Private,
        main,
        name,
        sig,
        MethodKind::Static,
        implementation,
        vec![Some(ptr_name), Some(size_name), Some(align_name)],
    ));
}

fn ensure_pool_realloc_method(asm: &mut Assembly) {
    let main = asm.main_module();
    let void_ptr = asm.nptr(Type::Void);
    let usize_ty = Type::Int(Int::USize);
    let sig = asm.sig([void_ptr, usize_ty, usize_ty, usize_ty], void_ptr);
    let name = asm.alloc_string(POOL_REALLOC_NAME);
    let ptr_name = asm.alloc_string("ptr");
    let old_size_name = asm.alloc_string("old_size");
    let align_name = asm.alloc_string("align");
    let new_size_name = asm.alloc_string("new_size");
    let implementation = MethodImpl::MethodBody {
        blocks: pool_realloc_blocks(asm),
        locals: vec![
            (Some(asm.alloc_string("new_ptr")), asm.alloc_type(void_ptr)),
            (
                Some(asm.alloc_string("old_class")),
                asm.alloc_type(usize_ty),
            ),
            (
                Some(asm.alloc_string("new_class")),
                asm.alloc_type(usize_ty),
            ),
        ],
    };
    asm.new_method(MethodDef::new(
        Access::Private,
        main,
        name,
        sig,
        MethodKind::Static,
        implementation,
        vec![
            Some(ptr_name),
            Some(old_size_name),
            Some(align_name),
            Some(new_size_name),
        ],
    ));
}

fn pool_alloc_blocks(asm: &mut Assembly) -> Vec<BasicBlock> {
    let pass_block = SIZE_CLASSES.len() as u32;
    let class_start = pass_block + 1;
    let null_block = class_start + (SIZE_CLASSES.len() as u32 * 3);

    let mut blocks = Vec::new();
    for (idx, class_size) in SIZE_CLASSES.iter().copied().enumerate() {
        let block_id = idx as u32;
        let next = if idx + 1 == SIZE_CLASSES.len() {
            pass_block
        } else {
            block_id + 1
        };
        let size = asm.alloc_node(CILNode::LdArg(0));
        let align = asm.alloc_node(CILNode::LdArg(1));
        let class = asm.alloc_node(Const::USize(class_size));
        let mut roots = Vec::new();
        if idx == 0 {
            let cap = asm.alloc_node(Const::USize(ALLOC_CAP));
            roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
                null_block,
                0,
                Some(BranchCond::Gt(size, cap, CmpKind::Unsigned)),
            )))));
            let size = asm.alloc_node(CILNode::LdArg(0));
            roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
                next,
                0,
                Some(BranchCond::Gt(size, class, CmpKind::Unsigned)),
            )))));
        } else {
            roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
                next,
                0,
                Some(BranchCond::Gt(size, class, CmpKind::Unsigned)),
            )))));
        }
        let class = asm.alloc_node(Const::USize(class_size));
        roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
            next,
            0,
            Some(BranchCond::Gt(align, class, CmpKind::Unsigned)),
        )))));
        roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
            class_start + idx as u32 * 3,
            0,
            None,
        )))));
        blocks.push(BasicBlock::new(roots, block_id, None));
    }

    let size = asm.alloc_node(CILNode::LdArg(0));
    let align = asm.alloc_node(CILNode::LdArg(1));
    let alloc_mref = native_aligned_alloc_mref(asm);
    let alloc = asm.alloc_node(CILNode::call(alloc_mref, [size, align]));
    let ret = asm.alloc_root(CILRoot::Ret(alloc));
    blocks.push(BasicBlock::new(vec![ret], pass_block, None));

    for (idx, class_size) in SIZE_CLASSES.iter().copied().enumerate() {
        let entry = class_start + idx as u32 * 3;
        let refill = entry + 1;
        let loop_block = entry + 2;
        let free_head = pool_free_head(asm, class_size);
        let head = asm.alloc_node(CILNode::LdStaticField(free_head));
        let st_head = asm.alloc_root(CILRoot::StLoc(0, head));
        let head = asm.alloc_node(CILNode::LdLoc(0));
        let head_word = asm.cast_ptr_to(head, Type::Int(Int::USize));
        let empty = asm.alloc_root(CILRoot::Branch(Box::new((
            refill,
            0,
            Some(BranchCond::False(head_word)),
        ))));
        let head = asm.alloc_node(CILNode::LdLoc(0));
        let next = load_next_ptr(asm, head);
        let st_free_head = asm.alloc_root(CILRoot::SetStaticField {
            field: free_head,
            val: next,
        });
        let head = asm.alloc_node(CILNode::LdLoc(0));
        let ret_head = asm.alloc_root(CILRoot::Ret(head));
        blocks.push(BasicBlock::new(
            vec![st_head, empty, st_free_head, ret_head],
            entry,
            None,
        ));

        let slab_size = asm.alloc_node(Const::USize(SLAB_SIZE));
        let slab_align = asm.alloc_node(Const::USize(SLAB_ALIGN));
        let alloc_mref = native_aligned_alloc_mref(asm);
        let slab = asm.alloc_node(CILNode::call(alloc_mref, [slab_size, slab_align]));
        let st_slab = asm.alloc_root(CILRoot::StLoc(1, slab));
        let slab = asm.alloc_node(CILNode::LdLoc(1));
        let slab_word = asm.cast_ptr_to(slab, Type::Int(Int::USize));
        let alloc_failed = asm.alloc_root(CILRoot::Branch(Box::new((
            null_block,
            0,
            Some(BranchCond::False(slab_word)),
        ))));
        let zero = asm.alloc_node(Const::USize(0));
        let st_idx = asm.alloc_root(CILRoot::StLoc(2, zero));
        let goto_loop = asm.alloc_root(CILRoot::Branch(Box::new((loop_block, 0, None))));
        blocks.push(BasicBlock::new(
            vec![st_slab, alloc_failed, st_idx, goto_loop],
            refill,
            None,
        ));

        let slab = asm.alloc_node(CILNode::LdLoc(1));
        let idx_ld = asm.alloc_node(CILNode::LdLoc(2));
        let class = asm.alloc_node(Const::USize(class_size));
        let offset = asm.alloc_node(CILNode::BinOp(idx_ld, class, BinOp::Mul));
        let current = asm.alloc_node(CILNode::BinOp(slab, offset, BinOp::Add));
        let st_current = asm.alloc_root(CILRoot::StLoc(3, current));
        let head = asm.alloc_node(CILNode::LdStaticField(free_head));
        let current = asm.alloc_node(CILNode::LdLoc(3));
        let void_ptr = void_ptr(asm);
        let store_next = asm.alloc_root(CILRoot::StInd(Box::new((current, head, void_ptr, false))));
        let current = asm.alloc_node(CILNode::LdLoc(3));
        let st_free_head = asm.alloc_root(CILRoot::SetStaticField {
            field: free_head,
            val: current,
        });
        let idx_ld = asm.alloc_node(CILNode::LdLoc(2));
        let one = asm.alloc_node(Const::USize(1));
        let idx_inc = asm.alloc_node(CILNode::BinOp(idx_ld, one, BinOp::Add));
        let st_idx_inc = asm.alloc_root(CILRoot::StLoc(2, idx_inc));
        let idx_ld = asm.alloc_node(CILNode::LdLoc(2));
        let count = asm.alloc_node(Const::USize(SLAB_SIZE / class_size));
        let loop_more = asm.alloc_root(CILRoot::Branch(Box::new((
            loop_block,
            0,
            Some(BranchCond::Lt(idx_ld, count, CmpKind::Unsigned)),
        ))));
        let goto_entry = asm.alloc_root(CILRoot::Branch(Box::new((entry, 0, None))));
        blocks.push(BasicBlock::new(
            vec![
                st_current,
                store_next,
                st_free_head,
                st_idx_inc,
                loop_more,
                goto_entry,
            ],
            loop_block,
            None,
        ));
    }

    let null = asm.alloc_node(Const::USize(0));
    let ret_null = asm.alloc_root(CILRoot::Ret(null));
    blocks.push(BasicBlock::new(vec![ret_null], null_block, None));
    blocks
}

fn pool_free_blocks(asm: &mut Assembly) -> Vec<BasicBlock> {
    let pass_block = SIZE_CLASSES.len() as u32;
    let class_start = pass_block + 1;

    let mut blocks = Vec::new();
    for (idx, class_size) in SIZE_CLASSES.iter().copied().enumerate() {
        let block_id = idx as u32;
        let next = if idx + 1 == SIZE_CLASSES.len() {
            pass_block
        } else {
            block_id + 1
        };
        let size = asm.alloc_node(CILNode::LdArg(1));
        let align = asm.alloc_node(CILNode::LdArg(2));
        let class = asm.alloc_node(Const::USize(class_size));
        let mut roots = Vec::new();
        roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
            next,
            0,
            Some(BranchCond::Gt(size, class, CmpKind::Unsigned)),
        )))));
        let class = asm.alloc_node(Const::USize(class_size));
        roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
            next,
            0,
            Some(BranchCond::Gt(align, class, CmpKind::Unsigned)),
        )))));
        roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
            class_start + idx as u32,
            0,
            None,
        )))));
        blocks.push(BasicBlock::new(roots, block_id, None));
    }

    let ptr = asm.alloc_node(CILNode::LdArg(0));
    let free_mref = native_aligned_free_mref(asm);
    let free = asm.alloc_root(CILRoot::call(free_mref, [ptr]));
    let ret = asm.alloc_root(CILRoot::VoidRet);
    blocks.push(BasicBlock::new(vec![free, ret], pass_block, None));

    for (idx, class_size) in SIZE_CLASSES.iter().copied().enumerate() {
        let free_head = pool_free_head(asm, class_size);
        let ptr = asm.alloc_node(CILNode::LdArg(0));
        let head = asm.alloc_node(CILNode::LdStaticField(free_head));
        let void_ptr = void_ptr(asm);
        let store_next = asm.alloc_root(CILRoot::StInd(Box::new((ptr, head, void_ptr, false))));
        let ptr = asm.alloc_node(CILNode::LdArg(0));
        let st_free_head = asm.alloc_root(CILRoot::SetStaticField {
            field: free_head,
            val: ptr,
        });
        let ret = asm.alloc_root(CILRoot::VoidRet);
        blocks.push(BasicBlock::new(
            vec![store_next, st_free_head, ret],
            class_start + idx as u32,
            None,
        ));
    }

    blocks
}

fn pool_realloc_blocks(asm: &mut Assembly) -> Vec<BasicBlock> {
    let old_start = 0;
    let old_large = SIZE_CLASSES.len() as u32;
    let new_start = old_large + 1;
    let new_large = new_start + SIZE_CLASSES.len() as u32;
    let compare_block = new_large + 1;
    let alloc_block = compare_block + 1;
    let ok_block = alloc_block + 1;
    let null_block = ok_block + 1;

    let mut blocks = Vec::new();
    append_classify_blocks(asm, &mut blocks, old_start, 1, 2, 1, new_start, old_large);
    let zero = asm.alloc_node(Const::USize(0));
    let st_old_large = asm.alloc_root(CILRoot::StLoc(1, zero));
    let goto_new = asm.alloc_root(CILRoot::Branch(Box::new((new_start, 0, None))));
    blocks.push(BasicBlock::new(
        vec![st_old_large, goto_new],
        old_large,
        None,
    ));

    append_classify_blocks(
        asm,
        &mut blocks,
        new_start,
        3,
        2,
        2,
        compare_block,
        new_large,
    );
    let zero = asm.alloc_node(Const::USize(0));
    let st_new_large = asm.alloc_root(CILRoot::StLoc(2, zero));
    let goto_compare = asm.alloc_root(CILRoot::Branch(Box::new((compare_block, 0, None))));
    blocks.push(BasicBlock::new(
        vec![st_new_large, goto_compare],
        new_large,
        None,
    ));

    let old_class = asm.alloc_node(CILNode::LdLoc(1));
    let zero = asm.alloc_node(Const::USize(0));
    let old_large = asm.alloc_root(CILRoot::Branch(Box::new((
        alloc_block,
        0,
        Some(BranchCond::Eq(old_class, zero)),
    ))));
    let old_class = asm.alloc_node(CILNode::LdLoc(1));
    let new_class = asm.alloc_node(CILNode::LdLoc(2));
    let class_changed = asm.alloc_root(CILRoot::Branch(Box::new((
        alloc_block,
        0,
        Some(BranchCond::Ne(old_class, new_class)),
    ))));
    let ptr = asm.alloc_node(CILNode::LdArg(0));
    let ret_same = asm.alloc_root(CILRoot::Ret(ptr));
    blocks.push(BasicBlock::new(
        vec![old_large, class_changed, ret_same],
        compare_block,
        None,
    ));

    let new_size = asm.alloc_node(CILNode::LdArg(3));
    let align = asm.alloc_node(CILNode::LdArg(2));
    let alloc_mref = pool_alloc_mref(asm);
    let new_ptr = asm.alloc_node(CILNode::call(alloc_mref, [new_size, align]));
    let st_new_ptr = asm.alloc_root(CILRoot::StLoc(0, new_ptr));
    let new_ptr = asm.alloc_node(CILNode::LdLoc(0));
    let new_ptr_word = asm.cast_ptr_to(new_ptr, Type::Int(Int::USize));
    let alloc_failed = asm.alloc_root(CILRoot::Branch(Box::new((
        null_block,
        0,
        Some(BranchCond::False(new_ptr_word)),
    ))));
    let old_size = asm.alloc_node(CILNode::LdArg(1));
    let new_size = asm.alloc_node(CILNode::LdArg(3));
    let lt = asm.alloc_node(CILNode::BinOp(old_size, new_size, BinOp::LtUn));
    let copy_len = asm.select(Type::Int(Int::USize), old_size, new_size, lt);
    let new_ptr = asm.alloc_node(CILNode::LdLoc(0));
    let old_ptr = asm.alloc_node(CILNode::LdArg(0));
    let copy = asm.alloc_root(CILRoot::CpBlk(Box::new((new_ptr, old_ptr, copy_len))));
    let old_ptr = asm.alloc_node(CILNode::LdArg(0));
    let old_size = asm.alloc_node(CILNode::LdArg(1));
    let align = asm.alloc_node(CILNode::LdArg(2));
    let free_mref = pool_free_mref(asm);
    let free_old = asm.alloc_root(CILRoot::call(free_mref, [old_ptr, old_size, align]));
    let goto_ok = asm.alloc_root(CILRoot::Branch(Box::new((ok_block, 0, None))));
    blocks.push(BasicBlock::new(
        vec![st_new_ptr, alloc_failed, copy, free_old, goto_ok],
        alloc_block,
        None,
    ));

    let new_ptr = asm.alloc_node(CILNode::LdLoc(0));
    let ret_ok = asm.alloc_root(CILRoot::Ret(new_ptr));
    blocks.push(BasicBlock::new(vec![ret_ok], ok_block, None));

    let null = asm.alloc_node(Const::USize(0));
    let ret_null = asm.alloc_root(CILRoot::Ret(null));
    blocks.push(BasicBlock::new(vec![ret_null], null_block, None));

    blocks
}

fn append_classify_blocks(
    asm: &mut Assembly,
    blocks: &mut Vec<BasicBlock>,
    start: u32,
    size_arg: u32,
    align_arg: u32,
    class_local: u32,
    done_block: u32,
    large_block: u32,
) {
    for (idx, class_size) in SIZE_CLASSES.iter().copied().enumerate() {
        let block_id = start + idx as u32;
        let next = if idx + 1 == SIZE_CLASSES.len() {
            large_block
        } else {
            block_id + 1
        };
        let size = asm.alloc_node(CILNode::LdArg(size_arg));
        let align = asm.alloc_node(CILNode::LdArg(align_arg));
        let class = asm.alloc_node(Const::USize(class_size));
        let mut roots = Vec::new();
        roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
            next,
            0,
            Some(BranchCond::Gt(size, class, CmpKind::Unsigned)),
        )))));
        let class = asm.alloc_node(Const::USize(class_size));
        roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
            next,
            0,
            Some(BranchCond::Gt(align, class, CmpKind::Unsigned)),
        )))));
        let class = asm.alloc_node(Const::USize(class_size));
        roots.push(asm.alloc_root(CILRoot::StLoc(class_local, class)));
        roots.push(asm.alloc_root(CILRoot::Branch(Box::new((done_block, 0, None)))));
        blocks.push(BasicBlock::new(roots, block_id, None));
    }
}

fn pool_free_head(asm: &mut Assembly, class_size: u64) -> crate::Interned<StaticFieldDesc> {
    let main = asm.main_module();
    let name = format!("rcl_pool_free_{class_size}");
    let void_ptr = void_ptr(asm);
    asm.add_static(void_ptr, name, true, main, None, false)
}

fn load_next_ptr(asm: &mut Assembly, ptr: crate::Interned<CILNode>) -> crate::Interned<CILNode> {
    let void_ptr = void_ptr(asm);
    let void_ptr_idx = asm.alloc_type(void_ptr);
    let ptr_to_ptr = asm.cast_ptr(ptr, void_ptr);
    asm.alloc_node(CILNode::LdInd {
        addr: ptr_to_ptr,
        tpe: void_ptr_idx,
        volatile: false,
    })
}

fn native_aligned_alloc_mref(asm: &mut Assembly) -> crate::Interned<MethodRef> {
    let void_ptr = asm.nptr(Type::Void);
    let sig = asm.sig([Type::Int(Int::USize), Type::Int(Int::USize)], void_ptr);
    let name = asm.alloc_string("AlignedAlloc");
    let native_mem = ClassRef::native_mem(asm);
    asm.alloc_methodref(MethodRef::new(
        native_mem,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ))
}

fn native_aligned_free_mref(asm: &mut Assembly) -> crate::Interned<MethodRef> {
    let void_ptr = asm.nptr(Type::Void);
    let sig = asm.sig([void_ptr], Type::Void);
    let name = asm.alloc_string("AlignedFree");
    let native_mem = ClassRef::native_mem(asm);
    asm.alloc_methodref(MethodRef::new(
        native_mem,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ))
}

fn void_ptr(asm: &mut Assembly) -> Type {
    asm.nptr(Type::Void)
}
