//! BCL bindings for the .NET ("dotnet") PAL.
//!
//! The std-side PAL (under `dotnet_pal/`) declares a small set of `extern "C"`
//! symbols and routes its allocator / stdio through them. This module
//! implements those symbols as [`MissingMethodPatcher`] builtins that emit CIL
//! calling the .NET Base Class Library. The names below **must match exactly**
//! the symbols the PAL declares — see `dotnet_pal/sys/alloc/dotnet.rs` and
//! `dotnet_pal/sys/stdio/dotnet.rs`.
//!
//! FIXED extern contract:
//! * `rcl_dotnet_alloc(size, align) -> *mut u8`
//!   => `System.Runtime.InteropServices.NativeMemory.AlignedAlloc((nuint)size, (nuint)align)`
//! * `rcl_dotnet_free(ptr, align)`
//!   => `System.Runtime.InteropServices.NativeMemory.AlignedFree((void*)ptr)`
//! * `rcl_dotnet_write(fd, ptr, len) -> isize`
//!   => writes `len` UTF-8 bytes from `ptr` to `System.Console`'s stdout (fd 1)
//!      or stderr (fd 2); returns bytes written (-1 on error — never taken here,
//!      a managed exception unwinds instead).
//!
//! `realloc` is handled std-side via `realloc_fallback` (alloc+copy+free) and
//! `alloc_zeroed` via `rcl_dotnet_alloc` + zeroing, so only these three symbols
//! need a binding.

use crate::cilnode::{ExtendKind, MethodKind};
use crate::ir::asm::MissingMethodPatcher;
use crate::ir::cilroot::BranchCond;
use crate::ir::{BasicBlock, CILNode, CILRoot, ClassRef, Int, MethodImpl, MethodRef, Type};
use crate::Assembly;

/// Registers all `rcl_dotnet_*` BCL bindings in `patcher`.
pub fn insert_dotnet_pal(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    insert_dotnet_alloc(asm, patcher);
    insert_dotnet_free(asm, patcher);
    insert_dotnet_write(asm, patcher);
}

/// `rcl_dotnet_alloc(size: usize, align: usize) -> *mut u8`
///   => `NativeMemory.AlignedAlloc((nuint)size, (nuint)align)`.
///
/// Models the existing `__rust_alloc` builtin: forward straight to
/// `AlignedAlloc`. Recent rustc wraps allocator-shim scalars in transparent
/// value types, but this symbol comes from our own PAL's `extern "C"` decl with
/// plain `usize` arguments, so the args are loaded directly.
fn insert_dotnet_alloc(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_alloc");
    let generator = move |_, asm: &mut Assembly| {
        let size = asm.alloc_node(CILNode::LdArg(0));
        let align = asm.alloc_node(CILNode::LdArg(1));
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
        // Result type is *mut u8; AlignedAlloc returns void*, which is
        // pointer-compatible, so a plain return suffices.
        let ret = asm.alloc_root(CILRoot::Ret(alloc));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_free(ptr: *mut u8, align: usize)`
///   => `NativeMemory.AlignedFree((void*)ptr)`.
///
/// Models the non-libc `__rust_dealloc` builtin. The `align` argument is unused
/// (`AlignedFree` takes only the pointer); it is part of the contract so the
/// std side can stay symmetric with `AlignedAlloc`.
fn insert_dotnet_free(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_free");
    let generator = move |_, asm: &mut Assembly| {
        let ptr = asm.alloc_node(CILNode::LdArg(0));
        let void_ptr = asm.nptr(Type::Void);
        // Reinterpret *mut u8 as void* for the AlignedFree signature.
        let ptr = asm.cast_ptr(ptr, void_ptr);
        let sig = asm.sig([void_ptr], Type::Void);
        let aligned_free = asm.alloc_string("AlignedFree");
        let native_mem = ClassRef::native_mem(asm);
        let call_method = asm.alloc_methodref(MethodRef::new(
            native_mem,
            aligned_free,
            sig,
            MethodKind::Static,
            [].into(),
        ));
        let free = asm.alloc_root(CILRoot::call(call_method, [ptr]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![free, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_write(fd: i32, ptr: *const u8, len: usize) -> isize`
///   => write `len` UTF-8 bytes from `ptr` to `System.Console`'s stdout (fd 1)
///      or stderr (fd 2); returns bytes written.
///
/// The `(ptr, len)` pair is turned into a managed `string` via
/// `System.Text.Encoding.UTF8.GetString(byte*, int)` (the overload that takes a
/// raw pointer, so no managed `byte[]` needs to be materialised), then written
/// with `System.Console.Out.Write(string)` / `Console.Error.Write(string)`
/// (a virtual `System.IO.TextWriter.Write`). Returns the input `len` (the bytes
/// consumed); on a managed I/O fault a .NET exception unwinds rather than the
/// `-1` path being taken.
fn insert_dotnet_write(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_write");
    let generator = move |_, asm: &mut Assembly| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));

        // ---- BCL class refs ----
        let console = ClassRef::console(asm);
        let encoding = {
            let name = asm.alloc_string("System.Text.Encoding");
            let asm_name = Some(asm.alloc_string("System.Runtime"));
            asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
        };
        let text_writer = {
            let name = asm.alloc_string("System.IO.TextWriter");
            let asm_name = Some(asm.alloc_string("System.Runtime"));
            asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
        };
        let encoding_ty = Type::ClassRef(encoding);
        let text_writer_ty = Type::ClassRef(text_writer);

        // ---- decode the (ptr, len) buffer into a managed string ----
        // Encoding.UTF8 -> Encoding   (static property getter)
        let get_utf8 = MethodRef::new(
            encoding,
            asm.alloc_string("get_UTF8"),
            asm.sig([], encoding_ty),
            MethodKind::Static,
            [].into(),
        );
        let get_utf8 = asm.alloc_methodref(get_utf8);
        let utf8 = asm.alloc_node(CILNode::call(get_utf8, []));

        // Encoding.GetString(byte* bytes, int byteCount) -> string  (instance)
        let len = asm.alloc_node(CILNode::LdArg(2));
        // usize -> int32 (`conv.u4`); truncates the 64-bit length to the int the
        // GetString(byte*, int) overload expects.
        let len_i32 = asm.int_cast(len, Int::I32, ExtendKind::ZeroExtend);
        let ptr = asm.alloc_node(CILNode::LdArg(1));
        let ptr = asm.cast_ptr(ptr, u8_ptr);
        let get_string = MethodRef::new(
            encoding,
            asm.alloc_string("GetString"),
            asm.sig(
                [encoding_ty, u8_ptr, Type::Int(Int::I32)],
                Type::PlatformString,
            ),
            MethodKind::Instance,
            [].into(),
        );
        let get_string = asm.alloc_methodref(get_string);
        let managed = asm.alloc_node(CILNode::call(get_string, [utf8, ptr, len_i32]));
        let store_str = asm.alloc_root(CILRoot::StLoc(0, managed));

        // ---- select stdout/stderr by fd, then Write(string) ----
        // get_Out / get_Error -> TextWriter (static), then virtual Write(string).
        let make_writer = |asm: &mut Assembly, getter: &str| {
            let getter = asm.alloc_string(getter);
            let mref = MethodRef::new(
                console,
                getter,
                asm.sig([], text_writer_ty),
                MethodKind::Static,
                [].into(),
            );
            asm.alloc_methodref(mref)
        };
        let get_out = make_writer(asm, "get_Out");
        let get_error = make_writer(asm, "get_Error");
        let write = {
            let mref = MethodRef::new(
                text_writer,
                asm.alloc_string("Write"),
                asm.sig([text_writer_ty, Type::PlatformString], Type::Void),
                MethodKind::Virtual,
                [].into(),
            );
            asm.alloc_methodref(mref)
        };

        // Block 0: if (fd == 1) goto stdout(1) else goto stderr(2)
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let one = asm.alloc_node(crate::Const::I32(1));
        let branch_stdout = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(fd, one)),
        ))));
        let goto_stderr = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // Block 1 (stdout): Console.Out.Write(str); goto ret(3)
        let writer_out = asm.alloc_node(CILNode::call(get_out, []));
        let str_out = asm.alloc_node(CILNode::LdLoc(0));
        let write_out = asm.alloc_root(CILRoot::call(write, [writer_out, str_out]));
        let out_to_ret = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // Block 2 (stderr): Console.Error.Write(str); goto ret(3)
        let writer_err = asm.alloc_node(CILNode::call(get_error, []));
        let str_err = asm.alloc_node(CILNode::LdLoc(0));
        let write_err = asm.alloc_root(CILRoot::call(write, [writer_err, str_err]));
        let err_to_ret = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // Block 3 (ret): return (isize)len
        let len_ret = asm.alloc_node(CILNode::LdArg(2));
        let len_ret = asm.int_cast(len_ret, Int::ISize, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(len_ret));

        let string_ty = asm.alloc_type(Type::PlatformString);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_str, branch_stdout, goto_stderr], 0, None),
                BasicBlock::new(vec![write_out, out_to_ret], 1, None),
                BasicBlock::new(vec![write_err, err_to_ret], 2, None),
                BasicBlock::new(vec![ret], 3, None),
            ],
            locals: vec![(Some(asm.alloc_string("managed_str")), string_ty)],
        }
    };
    patcher.insert(name, Box::new(generator));
}
