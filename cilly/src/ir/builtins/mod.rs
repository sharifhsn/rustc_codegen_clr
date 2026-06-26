use std::num::{NonZeroU32, NonZeroU8};

use crate::{utilis::mstring_to_utf8ptr, StaticFieldDesc};

use super::{
    asm::MissingMethodPatcher,
    bimap::Interned,
    cilnode::{MethodKind, PtrCastRes},
    cilroot::BranchCond,
    Access, Assembly, BasicBlock, CILNode, CILRoot, ClassDef, ClassRef, Const, FieldDesc, Int,
    MethodDef, MethodImpl, MethodRef, Type,
};

pub mod atomics;
pub mod casts;
pub mod dotnet;
pub mod math;
pub mod posix;
pub use posix::insert_posix_shim;
pub mod select;
pub mod thread;
pub use thread::*;
pub mod int128;
pub use int128::*;
pub mod f16;
pub use f16::*;
pub mod simd;
pub mod unwind;

pub fn insert_swap_at_generic(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("swap_at_generic");
    let generator = move |_, asm: &mut Assembly| {
        let buf1 = asm.alloc_node(CILNode::LdArg(0));
        let buf2 = asm.alloc_node(CILNode::LdArg(1));
        let size = asm.alloc_node(CILNode::LdArg(2));
        let tmp_alloc = asm.alloc_node(CILNode::LocAlloc { size });
        let tmp = asm.alloc_node(CILNode::LdLoc(0));
        // Alloc the tmp buffer
        let alloc_buff = asm.alloc_root(CILRoot::StLoc(0, tmp_alloc));
        // Swap buffers
        let buf1_to_tmp = asm.alloc_root(CILRoot::CpBlk(Box::new((tmp, buf1, size))));
        let buf2_to_buff1 = asm.alloc_root(CILRoot::CpBlk(Box::new((buf1, buf2, size))));
        let tmp_to_buf2 = asm.alloc_root(CILRoot::CpBlk(Box::new((buf2, tmp, size))));
        // Ret
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let uint8_ptr = asm.nptr(Type::Int(Int::U8));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![alloc_buff, buf1_to_tmp, buf2_to_buff1, tmp_to_buf2, ret],
                0,
                None,
            )],
            locals: vec![(Some(asm.alloc_string("tmp")), asm.alloc_type(uint8_ptr))],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn insert_bounds_check(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("bounds_check");
    let generator = move |_, asm: &mut Assembly| {
        let idx = asm.alloc_node(CILNode::LdArg(0));
        let _size = asm.alloc_node(CILNode::LdArg(1));
        // Ret
        let ret = asm.alloc_root(CILRoot::Ret(idx));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn unaligned_read(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("unaligned_read");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let tpe = asm[asm[mref].sig()].output();
        let tpe = asm.alloc_type(*tpe);
        // Copy to a local
        let ptr = asm.alloc_node(CILNode::LdArg(0));
        let local = asm.alloc_node(CILNode::LdLocA(0));
        let size = asm.size_of(tpe);
        let copy = asm.alloc_root(CILRoot::CpBlk(Box::new((local, ptr, size))));
        // Ret
        let local = asm.alloc_node(CILNode::LdLoc(0));
        let ret = asm.alloc_root(CILRoot::Ret(local));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![copy, ret], 0, None)],
            locals: vec![(None, tpe)],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// Loads argument `arg` reinterpreted as `tpe` (which must be the same size as
/// the argument), by reading the argument's address rather than the argument
/// directly.
///
/// Recent rustc passes allocator-shim arguments wrapped in transparent value
/// types instead of raw scalars: alignments as `core::ptr::Alignment` (a
/// `#[repr(usize)]` niche-enum) and pointers as `NonNull<u8>`. Loading such an
/// argument as a plain `usize`/pointer produces invalid CIL — the JIT rejects
/// the whole method with an `InvalidProgramException`. Reinterpreting via the
/// argument's address recovers the underlying scalar and also works for the
/// plain (unwrapped) ABI, since both representations are pointer-sized with the
/// scalar as their value.
fn reinterpret_arg(asm: &mut Assembly, arg: u32, tpe: Interned<Type>) -> Interned<CILNode> {
    let addr = asm.alloc_node(CILNode::LdArgA(arg));
    let addr = asm.alloc_node(CILNode::RefToPtr(addr));
    let addr = asm.alloc_node(CILNode::PtrCast(
        addr,
        Box::new(super::cilnode::PtrCastRes::Ptr(tpe)),
    ));
    asm.alloc_node(CILNode::LdInd {
        addr,
        tpe,
        volatile: false,
    })
}
/// Loads an allocator alignment argument (arg index `arg`) as a `usize`. See
/// [`reinterpret_arg`].
fn load_align_usize(asm: &mut Assembly, arg: u32) -> Interned<CILNode> {
    let usize_ty = asm.alloc_type(Type::Int(Int::USize));
    reinterpret_arg(asm, arg, usize_ty)
}
/// Loads an allocator pointer argument (arg index `arg`) as a `void*`. See
/// [`reinterpret_arg`]; recent rustc passes the pointer wrapped in `NonNull<u8>`.
fn load_ptr_arg(asm: &mut Assembly, arg: u32) -> Interned<CILNode> {
    let void_ptr = asm.nptr(Type::Void);
    let void_ptr = asm.alloc_type(void_ptr);
    reinterpret_arg(asm, arg, void_ptr)
}
fn insert_rust_alloc(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("__rust_alloc");
    let generator = move |_, asm: &mut Assembly| {
        let size = asm.alloc_node(CILNode::LdArg(0));
        let align = load_align_usize(asm, 1);
        let void_ptr = asm.nptr(Type::Void);
        let sig = asm.sig([Type::Int(Int::USize), Type::Int(Int::USize)], void_ptr);
        let aligned_alloc = asm.alloc_string("AlignedAlloc");
        let native_mem = ClassRef::native_mem(asm);
        let call_method = asm.alloc_methodref(MethodRef::new(
            native_mem,
            aligned_alloc,
            sig,
            MethodKind::Static,
            [].into(),
        ));
        let alloc = asm.alloc_node(CILNode::call(call_method, [size, align]));
        let ret = asm.alloc_root(CILRoot::Ret(alloc));
        let cap = asm.alloc_node(Const::USize(ALLOC_CAP));
        let check = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(super::cilroot::BranchCond::Gt(
                size,
                cap,
                super::cilroot::CmpKind::Unsigned,
            )),
        ))));
        let zero = asm.alloc_node(Const::USize(0));
        let ret_zero = CILRoot::Ret(zero);

        let throw = asm.alloc_root(ret_zero);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![check, ret], 0, None),
                BasicBlock::new(vec![throw], 1, None),
            ],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
fn insert_rust_alloc_zeroed(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("__rust_alloc_zeroed");
    let generator = move |_, asm: &mut Assembly| {
        let size = asm.alloc_node(CILNode::LdArg(0));
        let align = load_align_usize(asm, 1);
        let void_ptr = asm.nptr(Type::Void);
        let void_idx = asm.alloc_type(Type::Void);
        let sig = asm.sig([Type::Int(Int::USize), Type::Int(Int::USize)], void_ptr);
        let aligned_alloc = asm.alloc_string("AlignedAlloc");
        let native_mem = ClassRef::native_mem(asm);
        let call_method = asm.alloc_methodref(MethodRef::new(
            native_mem,
            aligned_alloc,
            sig,
            MethodKind::Static,
            [].into(),
        ));
        let alloc = asm.alloc_node(CILNode::call(call_method, [size, align]));
        let alloc = asm.alloc_node(CILNode::PtrCast(
            alloc,
            Box::new(super::cilnode::PtrCastRes::Ptr(void_idx)),
        ));
        let alloc = asm.alloc_root(CILRoot::StLoc(0, alloc));
        let cap = asm.alloc_node(Const::USize(ALLOC_CAP));
        let check = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(super::cilroot::BranchCond::Gt(
                size,
                cap,
                super::cilroot::CmpKind::Unsigned,
            )),
        ))));
        let throw = asm.throw_msg(&format!("Alloc limit of {ALLOC_CAP} exceeded.",));
        let zero = asm.alloc_node(Const::U8(0));
        let alloc_val = asm.alloc_node(CILNode::LdLoc(0));
        let zero = asm.alloc_root(CILRoot::InitBlk(Box::new((alloc_val, zero, size))));
        let ret = asm.alloc_root(CILRoot::Ret(alloc_val));
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![check, alloc, zero, ret], 0, None),
                BasicBlock::new(vec![throw], 1, None),
            ],
            locals: vec![(None, asm.alloc_type(void_ptr))],
        }
    };
    patcher.insert(name, Box::new(generator));
}

pub fn uninit_val(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("uninit_val");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let ret = asm.alloc_node(CILNode::LdLoc(0));
        let res = *asm[asm[mref].sig()].output();
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![asm.alloc_root(CILRoot::Ret(ret))],
                0,
                None,
            )],
            locals: vec![(None, asm.alloc_type(res))],
        }
    };

    patcher.insert(name, Box::new(generator));
}
pub fn ovf_check_tuple(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("ovf_check_tuple");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let res = *asm[asm[mref].sig()].output();
        let addr = asm.alloc_node(CILNode::LdLocA(0));
        let arg0 = asm.alloc_node(CILNode::LdArg(0));
        let arg1 = asm.alloc_node(CILNode::LdArg(1));
        let ret = asm.alloc_node(CILNode::LdLoc(0));
        let item1 = asm.alloc_string("Item1");
        let item2 = asm.alloc_string("Item2");
        let tpe = asm[asm[mref].sig()].inputs()[0];
        let item1 = asm.alloc_field(FieldDesc::new(res.as_class_ref().unwrap(), item1, tpe));
        let item2 = asm.alloc_field(FieldDesc::new(
            res.as_class_ref().unwrap(),
            item2,
            Type::Bool,
        ));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![
                    asm.alloc_root(CILRoot::SetField(Box::new((item1, addr, arg0)))),
                    asm.alloc_root(CILRoot::SetField(Box::new((item2, addr, arg1)))),
                    asm.alloc_root(CILRoot::Ret(ret)),
                ],
                0,
                None,
            )],
            locals: vec![(None, asm.alloc_type(res))],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn create_slice(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("create_slice");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let res = *asm[asm[mref].sig()].output();
        let addr = asm.alloc_node(CILNode::LdLocA(0));
        let arg0 = asm.alloc_node(CILNode::LdArg(0));
        let arg1 = asm.alloc_node(CILNode::LdArg(1));
        let ret = asm.alloc_node(CILNode::LdLoc(0));
        let data_ptr = Interned::data_ptr(asm, res.as_class_ref().unwrap());
        let metadata = Interned::metadata(asm, res.as_class_ref().unwrap());
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![
                    asm.alloc_root(CILRoot::SetField(Box::new((data_ptr, addr, arg0)))),
                    asm.alloc_root(CILRoot::SetField(Box::new((metadata, addr, arg1)))),
                    asm.alloc_root(CILRoot::Ret(ret)),
                ],
                0,
                None,
            )],
            locals: vec![(None, asm.alloc_type(res))],
        }
    };
    patcher.insert(name, Box::new(generator));
}
fn insert_rust_realloc(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, use_libc: bool) {
    let name = asm.alloc_string("__rust_realloc");
    if use_libc {
        let generator = move |_, asm: &mut Assembly| {
            let ptr = load_ptr_arg(asm, 0);
            let align = load_align_usize(asm, 2);
            let new_size = asm.alloc_node(CILNode::LdArg(3));
            let new_size = asm.alloc_node(CILNode::IntCast {
                input: new_size,
                target: Int::USize,
                extend: super::cilnode::ExtendKind::ZeroExtend,
            });
            let old_size = asm.alloc_node(CILNode::LdArg(1));
            let old_size = asm.alloc_node(CILNode::IntCast {
                input: old_size,
                target: Int::USize,
                extend: super::cilnode::ExtendKind::ZeroExtend,
            });
            let void_ptr = asm.nptr(Type::Void);
            let mm_malloc_sig = asm.sig([Type::Int(Int::USize), Type::Int(Int::USize)], void_ptr);
            // 1. call _mm_malloc
            let aligned_realloc = asm.alloc_string("_mm_malloc");
            let main_module = asm.main_module();
            let _mm_malloc = asm.alloc_methodref(MethodRef::new(
                *main_module,
                aligned_realloc,
                mm_malloc_sig,
                MethodKind::Static,
                [].into(),
            ));
            let _mm_malloc = asm.alloc_node(CILNode::call(_mm_malloc, [new_size, align]));
            let call_mm_malloc = asm.alloc_root(CILRoot::StLoc(0, _mm_malloc));
            // 2. memcpy the buffer.
            let buff = asm.alloc_node(CILNode::LdLoc(0));
            let copy = asm.alloc_root(CILRoot::CpBlk(Box::new((buff, ptr, old_size))));
            // 3. free the old buffer
            let aligned_free = asm.alloc_string("_mm_free");
            let mm_free_sig = asm.sig([void_ptr], Type::Void);
            let aligned_free = asm.alloc_methodref(MethodRef::new(
                *main_module,
                aligned_free,
                mm_free_sig,
                MethodKind::Static,
                [].into(),
            ));
            let call_aligned_free = asm.alloc_root(CILRoot::call(aligned_free, [ptr]));
            let ret = asm.alloc_root(CILRoot::Ret(buff));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(
                    vec![call_mm_malloc, copy, call_aligned_free, ret],
                    0,
                    None,
                )],
                locals: vec![(None, asm.alloc_type(void_ptr))],
            }
        };
        patcher.insert(name, Box::new(generator));
    } else {
        let generator = move |_, asm: &mut Assembly| {
            // realloc = AlignedAlloc + copy + AlignedFree, NOT `NativeMemory.AlignedRealloc`.
            // `AlignedRealloc` THROWS `OutOfMemoryException` when the requested size is unsatisfiable
            // (e.g. a `Vec<u8>` grown to `isize::MAX` bytes), which aborts the process; `AlignedAlloc`
            // instead returns NULL there — exactly like `__rust_alloc` above — so `try_reserve`/
            // `GlobalAlloc::grow` can report `AllocError` rather than aborting. The new block is also
            // only filled and the old block freed when the allocation SUCCEEDS: the
            // `GlobalAlloc::realloc` contract requires the old block to stay valid on failure (the
            // previous unconditional copy+free would memcpy into NULL and free the still-owned old
            // block). `min(old, new)` bytes are copied so a shrink never overruns the smaller block.
            // Surfaced by alloctests `{vec,string,vec_deque}::test_try_reserve`.
            let void_ptr = asm.nptr(Type::Void);
            let ptr = load_ptr_arg(asm, 0);
            let old_size = asm.alloc_node(CILNode::LdArg(1));
            let old_size = asm.alloc_node(CILNode::IntCast {
                input: old_size,
                target: Int::USize,
                extend: super::cilnode::ExtendKind::ZeroExtend,
            });
            let align = load_align_usize(asm, 2);
            let new_size = asm.alloc_node(CILNode::LdArg(3));
            let new_size = asm.alloc_node(CILNode::IntCast {
                input: new_size,
                target: Int::USize,
                extend: super::cilnode::ExtendKind::ZeroExtend,
            });
            // loc0 = NativeMemory.AlignedAlloc(new_size, align)   (NULL on unsatisfiable size)
            let native_mem = ClassRef::native_mem(asm);
            let alloc_name = asm.alloc_string("AlignedAlloc");
            let alloc_sig = asm.sig([Type::Int(Int::USize), Type::Int(Int::USize)], void_ptr);
            let aligned_alloc = asm.alloc_methodref(MethodRef::new(
                native_mem,
                alloc_name,
                alloc_sig,
                MethodKind::Static,
                [].into(),
            ));
            // cap guard, mirroring `__rust_alloc`: `AlignedAlloc` OOM-THROWS for sizes beyond
            // `ALLOC_CAP` (it does not return NULL), so short-circuit to NULL *without calling it*
            // when `new_size` exceeds the cap. (block 0 -> block 2 on `new_size > ALLOC_CAP`.)
            let cap = asm.alloc_node(Const::USize(ALLOC_CAP));
            let cap_check = asm.alloc_root(CILRoot::Branch(Box::new((
                2,
                0,
                Some(super::cilroot::BranchCond::Gt(
                    new_size,
                    cap,
                    super::cilroot::CmpKind::Unsigned,
                )),
            ))));
            let buff = asm.alloc_node(CILNode::call(aligned_alloc, [new_size, align]));
            let st_buff = asm.alloc_root(CILRoot::StLoc(0, buff));
            // if the new block is NULL, return NULL WITHOUT touching the old block (block 0 -> 2).
            let buff_ld = asm.alloc_node(CILNode::LdLoc(0));
            let buff_usize = asm.cast_ptr_to(buff_ld, Type::Int(Int::USize));
            let alloc_fail = asm.alloc_root(CILRoot::Branch(Box::new((
                2,
                0,
                Some(BranchCond::False(buff_usize)),
            ))));
            // copy_len = min(old_size, new_size)
            let lt = asm.alloc_node(CILNode::BinOp(old_size, new_size, super::cilnode::BinOp::LtUn));
            let copy_len = asm.select(Type::Int(Int::USize), old_size, new_size, lt);
            let buff_dst = asm.alloc_node(CILNode::LdLoc(0));
            let copy = asm.alloc_root(CILRoot::CpBlk(Box::new((buff_dst, ptr, copy_len))));
            // free the old block (only reached on success)
            let free_name = asm.alloc_string("AlignedFree");
            let free_sig = asm.sig([void_ptr], Type::Void);
            let aligned_free = asm.alloc_methodref(MethodRef::new(
                native_mem,
                free_name,
                free_sig,
                MethodKind::Static,
                [].into(),
            ));
            let free_old = asm.alloc_root(CILRoot::call(aligned_free, [ptr]));
            // explicit goto block 1: block 0 ends in a (non-terminating) `call`, so without this it
            // would fall through into block 2 (`ret NULL`) and a SUCCESSFUL realloc would return NULL.
            let goto_ok = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));
            // block 1: success -> return the new block.
            let ret_buff = asm.alloc_node(CILNode::LdLoc(0));
            let ret_ok = asm.alloc_root(CILRoot::Ret(ret_buff));
            // block 2: failure (cap exceeded or AlignedAlloc returned NULL) -> return NULL; the old
            // block is left allocated, as `GlobalAlloc::realloc` requires.
            let null = asm.alloc_node(Const::USize(0));
            let ret_null = asm.alloc_root(CILRoot::Ret(null));
            MethodImpl::MethodBody {
                blocks: vec![
                    BasicBlock::new(
                        vec![cap_check, st_buff, alloc_fail, copy, free_old, goto_ok],
                        0,
                        None,
                    ),
                    BasicBlock::new(vec![ret_ok], 1, None),
                    BasicBlock::new(vec![ret_null], 2, None),
                ],
                locals: vec![(None, asm.alloc_type(void_ptr))],
            }
        };
        patcher.insert(name, Box::new(generator));
    }
}
fn insert_rust_dealloc(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, use_libc: bool) {
    let name = asm.alloc_string("__rust_dealloc");
    if use_libc {
        let generator = move |_, asm: &mut Assembly| {
            let ldarg_0 = load_ptr_arg(asm, 0);
            let void_ptr = asm.nptr(Type::Void);
            let sig = asm.sig([void_ptr], Type::Void);
            let mm_free = asm.alloc_string("_mm_free");
            let main_module = asm.main_module();
            let call_method = asm.alloc_methodref(MethodRef::new(
                *main_module,
                mm_free,
                sig,
                MethodKind::Static,
                [].into(),
            ));
            let alloc = asm.alloc_node(CILNode::call(call_method, [ldarg_0]));
            let ret = asm.alloc_root(CILRoot::Ret(alloc));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    } else {
        let generator = move |_, asm: &mut Assembly| {
            let ldarg_0 = load_ptr_arg(asm, 0);
            let void_ptr = asm.nptr(Type::Void);
            let sig = asm.sig([void_ptr], Type::Void);
            let aligned_realloc = asm.alloc_string("AlignedFree");
            let native_mem = ClassRef::native_mem(asm);
            let call_method = asm.alloc_methodref(MethodRef::new(
                native_mem,
                aligned_realloc,
                sig,
                MethodKind::Static,
                [].into(),
            ));
            let alloc = asm.alloc_node(CILNode::call(call_method, [ldarg_0]));
            let ret = asm.alloc_root(CILRoot::Ret(alloc));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    }
}
pub fn insert_exeception_stub(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let rust_exception = asm.alloc_string("RustException");
    let data_pointer = asm.alloc_string("data_pointer");
    let extends = Some(ClassRef::exception(asm));
    asm.class_def(ClassDef::new(
        rust_exception,
        false,
        0,
        extends,
        vec![(Type::Int(Int::USize), data_pointer, Some(0))],
        vec![],
        Access::Public,
        Some(NonZeroU32::new(8).unwrap()),
        None,
        true,
    ))
    .unwrap();
    insert_catch_unwind_stub(asm, patcher);
    insert_interop_try_catch(asm, patcher);
}
pub fn insert_exception(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let rust_exception = asm.alloc_string("RustException");
    let data_pointer = asm.alloc_string("data_pointer");
    let this = asm.alloc_string("this");
    let extends = Some(ClassRef::exception(asm));
    let rust_exception = asm
        .class_def(ClassDef::new(
            rust_exception,
            false,
            0,
            extends,
            vec![(Type::Int(Int::USize), data_pointer, None)],
            vec![],
            Access::Public,
            None,
            None,
            true,
        ))
        .unwrap();
    let ctor = asm.alloc_string(".ctor");
    let sig = asm.sig(
        [Type::ClassRef(*rust_exception), Type::Int(Int::USize)],
        Type::Void,
    );
    let ldarg_0 = asm.alloc_node(CILNode::LdArg(0));
    let ldarg_1 = asm.alloc_node(CILNode::LdArg(1));
    let field = asm.alloc_field(FieldDesc::new(
        *rust_exception,
        data_pointer,
        Type::Int(Int::USize),
    ));
    let set_field = asm.alloc_root(CILRoot::SetField(Box::new((field, ldarg_0, ldarg_1))));
    let void_ret = asm.alloc_root(CILRoot::VoidRet);

    asm.new_method(MethodDef::new(
        Access::Public,
        rust_exception,
        ctor,
        sig,
        MethodKind::Constructor,
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![set_field, void_ret], 0, None)],
            locals: vec![],
        },
        vec![Some(this), Some(data_pointer)],
    ));
    insert_catch_unwind(asm, patcher);
    insert_interop_try_catch(asm, patcher);
}
pub fn insert_heap(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, use_libc: bool) {
    insert_rust_alloc(asm, patcher);
    insert_rust_alloc_zeroed(asm, patcher);
    insert_rust_realloc(asm, patcher, use_libc);
    insert_rust_dealloc(asm, patcher, use_libc);
    insert_pause(asm, patcher);
}

fn insert_pause(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("llvm.x86.sse2.pause");
    let generator = move |_, asm: &mut Assembly| {
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn rust_assert(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    fn assert(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, name: &str) {
        let name = asm.alloc_string(name);
        let generator = move |_, asm: &mut Assembly| {
            let ret = asm.alloc_root(CILRoot::VoidRet);
            let assert = asm.alloc_node(CILNode::LdArg(0));
            let assert = asm.alloc_root(CILRoot::Branch(Box::new((
                1,
                0,
                Some(BranchCond::False(assert)),
            ))));
            let mref = Interned::abort(asm);
            let assert_failed = asm.alloc_root(CILRoot::call(mref, vec![]));
            MethodImpl::MethodBody {
                blocks: vec![
                    BasicBlock::new(vec![assert, ret], 0, None),
                    BasicBlock::new(vec![assert_failed, ret], 1, None),
                ],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    }
    const ASSERTS: &[&str] = &[
        "assert_bounds_check",
        "assert_ptr_align",
        "assert_notnull",
        "assert_add",
        "assert_mul",
        "assert_shl",
        "assert_zero_rem",
        "assert_sub",
        "assert_div",
        "assert_zero_div",
        "assert_shr",
        "assert_neg_overflow",
        "assert_mod",
    ];
    for kind in ASSERTS {
        assert(asm, patcher, kind);
    }
}

fn insert_catch_unwind_stub(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("catch_unwind");
    let generator = move |_, asm: &mut Assembly| {
        let uint8_ptr = asm.nptr(Type::Int(Int::U8));
        let try_sig = asm.sig([uint8_ptr], Type::Void);

        let ldarg_0 = asm.alloc_node(CILNode::LdArg(0));
        let ldarg_1 = asm.alloc_node(CILNode::LdArg(1));

        // Call indirect try
        let calli_try = asm.alloc_root(CILRoot::CallI(Box::new((
            ldarg_0,
            try_sig,
            [ldarg_1].into(),
        ))));

        let const_0 = asm.alloc_node(Const::I32(0));
        let ret_0 = asm.alloc_root(CILRoot::Ret(const_0));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![calli_try, ret_0], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
fn insert_catch_unwind(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("catch_unwind");
    let generator = move |_, asm: &mut Assembly| {
        let uint8_ptr = asm.nptr(Type::Int(Int::U8));
        let try_sig = asm.sig([uint8_ptr], Type::Void);
        let catch_sig = asm.sig([uint8_ptr, uint8_ptr], Type::Void);
        let ldarg_0 = asm.alloc_node(CILNode::LdArg(0));
        let ldarg_1 = asm.alloc_node(CILNode::LdArg(1));
        let ldarg_2 = asm.alloc_node(CILNode::LdArg(2));
        let ldloc_1 = asm.alloc_node(CILNode::LdLoc(1));
        // Call indirect try
        let calli_try = asm.alloc_root(CILRoot::CallI(Box::new((
            ldarg_0,
            try_sig,
            [ldarg_1].into(),
        ))));
        let exit_try_success = asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 2,
            source: 0,
        });
        let exit_try_failure = asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 3,
            source: 0,
        });
        let get_exception = asm.alloc_node(CILNode::GetException);
        let set_exception = asm.alloc_root(CILRoot::StLoc(1, get_exception));
        let exception = Type::ClassRef(ClassRef::exception(asm));
        let exception = asm.alloc_type(exception);
        let rust_exception = asm.alloc_string("RustException");
        let rust_exception =
            asm.alloc_class_ref(ClassRef::new(rust_exception, None, false, [].into()));
        let rust_exception_tpe = Type::ClassRef(rust_exception);
        let rust_exception_tpe = asm.alloc_type(rust_exception_tpe);
        // Check if exception is the right type, otherwise jump away
        let check_exception_tpe = asm.alloc_node(CILNode::IsInst(ldloc_1, rust_exception_tpe));
        let rethrow_if_wrong_exception = asm.alloc_root(CILRoot::Branch(Box::new((
            0,
            4,
            Some(BranchCond::False(check_exception_tpe)),
        ))));
        // Cast the excpetion
        let cast_exception = asm.alloc_node(CILNode::CheckedCast(ldloc_1, rust_exception_tpe));
        let data_pointer = asm.alloc_string("data_pointer");
        let ptr_field = asm.alloc_field(FieldDesc::new(
            rust_exception,
            data_pointer,
            Type::Int(Int::USize),
        ));
        let exception_ptr = asm.alloc_node(CILNode::LdField {
            addr: cast_exception,
            field: ptr_field,
        });
        let calli_catch = asm.alloc_root(CILRoot::CallI(Box::new((
            ldarg_2,
            catch_sig,
            [ldarg_1, exception_ptr].into(),
        ))));
        let const_0 = asm.alloc_node(Const::I32(0));
        let const_1 = asm.alloc_node(Const::I32(1));
        let ret_0 = asm.alloc_root(CILRoot::Ret(const_0));
        let ret_1 = asm.alloc_root(CILRoot::Ret(const_1));
        let rethrow = asm.alloc_root(CILRoot::ReThrow);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(
                    vec![calli_try, exit_try_success],
                    0,
                    Some(vec![
                        BasicBlock::new(
                            vec![
                                set_exception,
                                rethrow_if_wrong_exception,
                                calli_catch,
                                exit_try_failure,
                            ],
                            1,
                            None,
                        ),
                        BasicBlock::new(vec![rethrow], 4, None),
                    ]),
                ),
                BasicBlock::new(vec![ret_0], 2, None),
                BasicBlock::new(vec![ret_1], 3, None),
            ],
            locals: vec![
                (
                    Some(asm.alloc_string("data_ptr")),
                    asm.alloc_type(Type::Int(Int::USize)),
                ),
                (Some(asm.alloc_string("exception")), exception),
            ],
        }
    };
    patcher.insert(name, Box::new(generator));
}
/// `interop_try_catch(try_fn, data, catch_fn) -> i32` — the interop counterpart of
/// `catch_unwind`. It wraps an indirect call to `try_fn(data)` in a CIL `try/catch`
/// that catches **everything** (`catch [System.Runtime]System.Object`), so a foreign
/// (.NET BCL) exception — which `catch_unwind` deliberately rethrows because it is not a
/// `RustException` (see `insert_catch_unwind`) — is caught here. On a caught exception
/// it invokes `catch_fn(data)` and returns 1; on normal completion it returns 0.
/// The exception object itself is not handed to `catch_fn` (a managed reference can't be
/// safely carried in a `*mut u8`); the handler uses no `GetException`, so the IL exporter
/// emits a `pop` to discard the on-stack exception. Inspecting the exception is a
/// follow-up (it can be done with further managed calls once a managed-ref ABI exists).
fn insert_interop_try_catch(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("interop_try_catch");
    let generator = move |_, asm: &mut Assembly| {
        let uint8_ptr = asm.nptr(Type::Int(Int::U8));
        let try_sig = asm.sig([uint8_ptr], Type::Void);
        let catch_sig = asm.sig([uint8_ptr], Type::Void);
        let ldarg_0 = asm.alloc_node(CILNode::LdArg(0)); // try_fn
        let ldarg_1 = asm.alloc_node(CILNode::LdArg(1)); // data
        let ldarg_2 = asm.alloc_node(CILNode::LdArg(2)); // catch_fn
        // try region: call try_fn(data); on success leave to the "ok" block (2).
        let calli_try = asm.alloc_root(CILRoot::CallI(Box::new((
            ldarg_0,
            try_sig,
            [ldarg_1].into(),
        ))));
        let exit_try_success = asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 2,
            source: 0,
        });
        // catch-all handler: call catch_fn(data); leave to the "caught" block (3).
        // No `GetException` is referenced, so the exporter inserts a `pop` to balance the
        // stack (the caught exception object is discarded).
        let calli_catch = asm.alloc_root(CILRoot::CallI(Box::new((
            ldarg_2,
            catch_sig,
            [ldarg_1].into(),
        ))));
        let exit_try_failure = asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 3,
            source: 0,
        });
        let const_0 = asm.alloc_node(Const::I32(0));
        let const_1 = asm.alloc_node(Const::I32(1));
        let ret_0 = asm.alloc_root(CILRoot::Ret(const_0));
        let ret_1 = asm.alloc_root(CILRoot::Ret(const_1));
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(
                    vec![calli_try, exit_try_success],
                    0,
                    Some(vec![BasicBlock::new(
                        vec![calli_catch, exit_try_failure],
                        1,
                        None,
                    )]),
                ),
                BasicBlock::new(vec![ret_0], 2, None),
                BasicBlock::new(vec![ret_1], 3, None),
            ],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}
const ALLOC_CAP: u64 = u32::MAX as u64;
pub(crate) const UNMANAGED_THREAD_START: &str = "UnmanagedThreadStart";

pub fn transmute(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("transmute");
    let generator = move |mref: Interned<MethodRef>, asm: &mut Assembly| {
        let target = *asm[asm[mref].sig()].output();
        let source = asm[asm[mref].sig()].inputs()[0];
        let source = asm.alloc_type(source);
        let target_idx = asm.alloc_type(target);
        let addr = asm.alloc_node(CILNode::LdArgA(0));
        if asm.alignof_type(source) >= asm.alignof_type(target_idx) {
            let ptr = asm.alloc_node(CILNode::RefToPtr(addr));
            let ptr = asm.alloc_node(CILNode::PtrCast(ptr, Box::new(PtrCastRes::Ptr(target_idx))));
            let valuetype = asm.alloc_node(CILNode::LdInd {
                addr: ptr,
                tpe: target_idx,
                volatile: false,
            });
            let ret = asm.alloc_root(CILRoot::Ret(valuetype));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        } else {
            let dst = asm.alloc_node(CILNode::LdLocA(0));
            let size = asm.alloc_node(CILNode::SizeOf(source));
            let load = asm.alloc_root(CILRoot::CpBlk(Box::new((dst, addr, size))));
            let ret = asm.alloc_node(CILNode::LdLoc(0));
            let ret = asm.alloc_root(CILRoot::Ret(ret));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![load, ret], 0, None)],
                locals: vec![(None, target_idx)],
            }
        }
    };
    patcher.insert(name, Box::new(generator));
}
pub fn argc_argv_init(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("argc_argv_init");
    let generator = move |_, asm: &mut Assembly| {
        let main_module = asm.main_module();
        use crate::cilnode::{ExtendKind, IsPure};
        use crate::{BinOp, FnSig};

        let uint8_ptr = asm.nptr(Type::Int(Int::I8));
        // Local layout (mirrors the previous `add_local` order):
        //   0: argument_count(argc), 1: argument_array(argv), 2: managed_args, 3: arg_idx
        let argc: u32 = 0;
        let argv: u32 = 1;
        let managed_args: u32 = 2;
        let arg_idx: u32 = 3;
        let string = asm.alloc_type(Type::PlatformString);
        let uint8_ptr_ptr = asm.nptr(uint8_ptr);
        let locals = vec![
            (
                Some(asm.alloc_string("argument_count")),
                asm.alloc_type(Type::Int(Int::I32)),
            ),
            (
                Some(asm.alloc_string("argument_array")),
                asm.alloc_type(uint8_ptr_ptr),
            ),
            (
                Some(asm.alloc_string("managed_args")),
                asm.alloc_type(Type::PlatformArray {
                    elem: string,
                    dims: NonZeroU8::new(1).unwrap(),
                }),
            ),
            (
                Some(asm.alloc_string("arg_idx")),
                asm.alloc_type(Type::Int(Int::I32)),
            ),
        ];
        // Block ids (mirror the `new_bb` order): 0 start, 1 loop, 2 loop_end, 3 final.
        let start_bb: u32 = 0;
        let loop_bb: u32 = 1;
        let loop_end_bb: u32 = 2;
        let final_bb: u32 = 3;

        let status = StaticFieldDesc::new(
            *asm.main_module(),
            asm.alloc_string("argv_argc_init_status"),
            Type::Bool,
        );
        let status = asm.alloc_sfld(status);

        // ---- start block ----
        let mut start_roots = Vec::new();
        // BTrue(status) -> final
        let status_load = asm.load_static(status);
        start_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
            final_bb,
            0,
            Some(BranchCond::True(status_load)),
        )))));
        // managed_args = GetCommandLineArgs()
        let mref = MethodRef::new(
            ClassRef::enviroment(asm),
            asm.alloc_string("GetCommandLineArgs"),
            asm.sig(
                [],
                Type::PlatformArray {
                    elem: string,
                    dims: NonZeroU8::new(1).unwrap(),
                },
            ),
            MethodKind::Static,
            vec![].into(),
        );
        let mref = asm.alloc_methodref(mref);
        let margs = asm.call(mref, &[] as &[Interned<CILNode>], IsPure::NOT);
        start_roots.push(asm.alloc_root(CILRoot::StLoc(managed_args, margs)));
        // argc = conv_i32(LdLen(managed_args))
        let ld_margs = asm.alloc_node(CILNode::LdLoc(managed_args));
        let len = asm.alloc_node(CILNode::LdLen(ld_margs));
        let len_i32 = asm.int_cast(len, Int::I32, ExtendKind::SignExtend);
        start_roots.push(asm.alloc_root(CILRoot::StLoc(argc, len_i32)));
        // argv = AlignedAlloc(zext_usize(argc) * zext_usize(size_of(usize)), zext_usize(8)) cast *uint8_ptr
        let aligned_alloc = MethodRef::aligned_alloc(asm);
        let aligned_alloc = asm.alloc_methodref(aligned_alloc);
        let ld_argc = asm.alloc_node(CILNode::LdLoc(argc));
        let argc_usize = asm.int_cast(ld_argc, Int::USize, ExtendKind::ZeroExtend);
        let szof = asm.size_of(Int::USize);
        let szof_usize = asm.int_cast(szof, Int::USize, ExtendKind::ZeroExtend);
        let alloc_size = asm.biop(argc_usize, szof_usize, BinOp::Mul);
        let eight = asm.alloc_node(8_i32);
        let eight_usize = asm.int_cast(eight, Int::USize, ExtendKind::ZeroExtend);
        let alloc_call = asm.call(aligned_alloc, &[alloc_size, eight_usize], IsPure::NOT);
        // `argv` is `int8**` (a C `char**`). `cast_ptr(x, T)` yields `Ptr(T)`, so the
        // pointee for an `int8**` is `int8*` (`uint8_ptr`, which is `nptr(i8)` here),
        // NOT `uint8_ptr_ptr` (`int8**`) — passing the latter over-pointers to
        // `int8***` and fails the `StLoc(argv, …)` assignability check.
        let alloc_call = asm.cast_ptr(alloc_call, uint8_ptr);
        start_roots.push(asm.alloc_root(CILRoot::StLoc(argv, alloc_call)));
        // arg_idx = 0
        let zero_i32 = asm.alloc_node(0_i32);
        start_roots.push(asm.alloc_root(CILRoot::StLoc(arg_idx, zero_i32)));
        start_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((loop_bb, 0, None)))));

        // ---- loop block ----
        let mut loop_roots = Vec::new();
        // arg_nth = LdElemRef(managed_args, arg_idx)
        let ld_margs = asm.alloc_node(CILNode::LdLoc(managed_args));
        let ld_arg_idx = asm.alloc_node(CILNode::LdLoc(arg_idx));
        let arg_nth = asm.ld_elem_ref(ld_margs, ld_arg_idx);
        // `mstring_to_utf8ptr` yields a `uint8*` (UTF8 bytes). The argv slots are
        // `int8*` (this builds a C `char**`), so reinterpret the byte pointer as
        // `int8*` before storing it (signedness-only pointer cast).
        let uarg = mstring_to_utf8ptr(arg_nth, asm);
        let uarg = asm.cast_ptr(uarg, Type::Int(Int::I8));
        // STIndPtr(argv + zext_usize(size_of(usize) * arg_idx), uarg, *i8)
        let ld_argv = asm.alloc_node(CILNode::LdLoc(argv));
        let szof = asm.size_of(Int::USize);
        let ld_arg_idx = asm.alloc_node(CILNode::LdLoc(arg_idx));
        let mul = asm.biop(szof, ld_arg_idx, BinOp::Mul);
        let mul = asm.int_cast(mul, Int::USize, ExtendKind::ZeroExtend);
        let addr = asm.biop(ld_argv, mul, BinOp::Add);
        let i8_ptr_ty = asm.nptr(Type::Int(Int::I8));
        loop_roots.push(asm.alloc_root(CILRoot::StInd(Box::new((
            addr,
            uarg,
            i8_ptr_ty,
            false,
        )))));
        // arg_idx += 1
        let ld_arg_idx = asm.alloc_node(CILNode::LdLoc(arg_idx));
        let one = asm.alloc_node(1_i32);
        let inc = asm.biop(ld_arg_idx, one, BinOp::Add);
        loop_roots.push(asm.alloc_root(CILRoot::StLoc(arg_idx, inc)));
        // BTrue( (arg_idx < conv_i32(LdLen(managed_args))) == false ) -> loop_end
        let ld_arg_idx = asm.alloc_node(CILNode::LdLoc(arg_idx));
        let ld_margs = asm.alloc_node(CILNode::LdLoc(managed_args));
        let len = asm.alloc_node(CILNode::LdLen(ld_margs));
        let len_i32 = asm.int_cast(len, Int::I32, ExtendKind::SignExtend);
        let lt = asm.biop(ld_arg_idx, len_i32, BinOp::Lt);
        let false_node = asm.alloc_node(false);
        let cond = asm.biop(lt, false_node, BinOp::Eq);
        loop_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((
            loop_end_bb,
            0,
            Some(BranchCond::True(cond)),
        )))));
        loop_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((loop_bb, 0, None)))));

        // ---- loop_end block ----
        let mut loop_end_roots = Vec::new();
        let argv_static = StaticFieldDesc::new(
            *asm.main_module(),
            asm.alloc_string("argv"),
            uint8_ptr_ptr,
        );
        let argv_static = asm.alloc_sfld(argv_static);
        let ld_argv = asm.alloc_node(CILNode::LdLoc(argv));
        loop_end_roots.push(asm.alloc_root(CILRoot::SetStaticField {
            field: argv_static,
            val: ld_argv,
        }));
        let argc_static = StaticFieldDesc::new(
            *asm.main_module(),
            asm.alloc_string("argc"),
            Type::Int(Int::I32),
        );
        let argc_static = asm.alloc_sfld(argc_static);
        let ld_argc = asm.alloc_node(CILNode::LdLoc(argc));
        loop_end_roots.push(asm.alloc_root(CILRoot::SetStaticField {
            field: argc_static,
            val: ld_argc,
        }));
        loop_end_roots.push(asm.alloc_root(CILRoot::Branch(Box::new((final_bb, 0, None)))));
        // status = true (intentionally kept after the GoTo).
        let true_node = asm.alloc_node(true);
        loop_end_roots.push(asm.alloc_root(CILRoot::SetStaticField {
            field: status,
            val: true_node,
        }));

        // ---- final block ----
        let final_roots = vec![asm.alloc_root(CILRoot::VoidRet)];

        // Blocks in id order: start(0), loop(1), loop_end(2), final(3).
        let blocks = vec![
            BasicBlock::new(start_roots, start_bb, None),
            BasicBlock::new(loop_roots, loop_bb, None),
            BasicBlock::new(loop_end_roots, loop_end_bb, None),
            BasicBlock::new(final_roots, final_bb, None),
        ];
        let sig = asm.alloc_sig(FnSig::new([], Type::Void));
        let def = crate::ir::MethodDef::from_blocks(
            crate::Access::Extern,
            main_module,
            "argc_argv_init",
            sig,
            MethodKind::Static,
            blocks,
            locals,
            vec![],
            asm,
        );
        asm.new_method(def.clone());
        asm.add_static(
            Type::Bool,
            "argv_argc_init_status",
            false,
            main_module,
            None,
            false,
        );
        asm.add_static(uint8_ptr_ptr, "argv", false, main_module, None, false);
        asm.add_static(Type::Int(Int::I32), "argc", false, main_module, None, false);
        def.implementation().clone()
    };
    patcher.insert(name, Box::new(generator));
}
