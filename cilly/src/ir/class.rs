use super::{bimap::Interned, Assembly, Const, MethodDefIdx, MethodRef, Type};
use crate::Access;
use crate::{utilis::assert_unique, IString};
use serde::{Deserialize, Serialize};
use std::{num::NonZeroU32, ops::Deref};
#[derive(Debug)]
pub enum LayoutError {
    ManagedRefInOverlapingField {
        owner: String,
        field: String,
        name: String,
    },
}
#[derive(Hash, PartialEq, Eq, Copy, Clone, Debug, Serialize, Deserialize)]
pub struct ClassDefIdx(pub Interned<ClassRef>);
impl ClassDefIdx {
    pub(crate) fn from_raw(class: Interned<ClassRef>) -> ClassDefIdx {
        ClassDefIdx(class)
    }
}
impl Deref for ClassDefIdx {
    type Target = Interned<ClassRef>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl From<Interned<ClassRef>> for Type {
    fn from(val: Interned<ClassRef>) -> Self {
        Type::ClassRef(val)
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ClassRef {
    name: Interned<IString>,
    asm: Option<Interned<IString>>,
    is_valuetype: bool,
    generics: Box<[Type]>,
}

impl ClassRef {
    #[must_use]
    pub fn display(&self, asm: &Assembly) -> String {
        format!(
            "ClassRef{{name:{},asm:{:?},is_valuetype:{},generics{:?}}}",
            &asm[self.name()],
            self.asm().map(|idx| &asm[idx]),
            self.is_valuetype(),
            self.generics()
        )
    }
    #[must_use]
    pub fn new(
        name: Interned<IString>,
        asm: Option<Interned<IString>>,
        is_valuetype: bool,
        generics: Box<[Type]>,
    ) -> Self {
        Self {
            name,
            asm,
            is_valuetype,
            generics,
        }
    }
    pub fn interlocked(asm: &mut super::Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Threading.Interlocked");
        let asm_name = Some(asm.alloc_string("System.Threading"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, vec![].into()))
    }

    /// Returns the assembly containing this typedef
    #[must_use]
    pub fn asm(&self) -> Option<Interned<IString>> {
        self.asm
    }
    /// The name of this class definition
    #[must_use]
    pub fn name(&self) -> Interned<IString> {
        self.name
    }

    #[must_use]
    pub fn is_valuetype(&self) -> bool {
        self.is_valuetype
    }

    #[must_use]
    pub fn generics(&self) -> &[Type] {
        &self.generics
    }
    /// The .NET math class
    pub fn math(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Math");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, vec![].into()))
    }
    /// Retusn a reference to the class `System.Double`
    pub fn double(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Double");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        // `System.Double` is a .NET value type. It MUST be referenced as `valuetype`
        // (not `class`) in IL, or any call whose declaring type is `System.Double`
        // (e.g. `MinNumber`/`MaxNumber`/`Max`/`Min`/`FusedMultiplyAdd`/`Pow`) makes the
        // runtime reject the type-load with `TypeLoadException: ... value type mismatch`.
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, vec![].into()))
    }
    /// Retusn a reference to the class `System.Single`
    pub fn single(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Single");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        // `System.Single` is a .NET value type — see `double` above.
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, vec![].into()))
    }
    /// Returns a reference to the class `System.MathF`
    #[must_use]
    pub fn mathf(asm: &mut Assembly) -> Interned<ClassRef> {
        let name: Interned<IString> = asm.alloc_string("System.MathF");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the `System.UInt128` type.
    pub fn uint_128(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.UInt128");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.Int128` type.
    pub fn int_128(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Int128");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.UIntPtr` type.
    pub fn usize_type(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.UIntPtr");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.UInt16` type.
    pub fn uint16(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.UInt16");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.Int16` type.
    pub fn int16(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Int16");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.UInt32` type.
    pub fn uint32(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.UInt32");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.Int32` type.
    pub fn int32(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Int32");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.UInt64` type.
    pub fn uint64(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.UInt64");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.Int64` type.
    pub fn int64(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Int64");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.IntPtr` type.
    pub fn isize_type(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IntPtr");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.Half` type.
    pub fn half(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Half");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.Byte` type.
    pub fn byte(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Byte");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.SByte` type.
    pub fn sbyte(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.SByte");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the GC handle class.
    pub fn gc_handle(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Runtime.InteropServices.GCHandle");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.String`
    pub fn string(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.String");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the `System.Object`
    pub fn object(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Object");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the `System.Threading.Thread`
    pub fn thread(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Threading.Thread");
        let asm_name = Some(asm.alloc_string("System.Threading.Thread"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the `System.Threading.ThreadStart`
    pub fn thread_start(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Threading.ThreadStart");
        let asm_name = Some(asm.alloc_string("System.Threading.Thread"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the `System.Threading.SemaphoreSlim`
    pub fn semaphore_slim(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Threading.SemaphoreSlim");
        // SemaphoreSlim physically lives in System.Private.CoreLib; method bodies
        // must name the IMPL assembly (the runtime resolves it directly), while
        // `ref_assembly_name` normalizes it to System.Runtime in the C#-visible
        // metadata. Naming `System.Runtime` here makes the body's
        // `[System.Runtime]SemaphoreSlim` unresolvable at run time (TypeLoadException).
        let asm_name = Some(asm.alloc_string("System.Private.CoreLib"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.Threading.ThreadLocal<T>` instantiated at
    /// element type `element` (e.g. `System.Threading.ThreadLocal<nint>`).
    ///
    /// Backs the dotnet PAL's per-thread thread-local storage (Slice 2): each
    /// `thread_local!` TLS key is one `ThreadLocal<IntPtr>` whose `.Value` is
    /// per-thread BY CONSTRUCTION. A reference type (generic arity 1).
    ///
    /// ASM-NAME LESSON (same as `semaphore_slim`): `ThreadLocal<T>` physically
    /// lives in `System.Private.CoreLib`; method-BODY type references must name
    /// the IMPL assembly so the runtime resolves it directly. Naming
    /// `System.Runtime` here makes the body's `[System.Runtime]ThreadLocal\`1`
    /// unresolvable at run time (TypeLoadException); `ref_assembly_name`
    /// normalizes CoreLib -> System.Runtime only in C#-visible metadata.
    pub fn thread_local(asm: &mut Assembly, element: Type) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Threading.ThreadLocal");
        let asm_name = Some(asm.alloc_string("System.Private.CoreLib"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [element].into()))
    }
    /// Returns a reference to the `System.Type`
    pub fn type_type(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Type");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the `System.RuntimeTypeHandle`
    pub fn runtime_type_hadle(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.RuntimeTypeHandle");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the `System.String`
    pub fn exception(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Exception");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the `System.Console`
    pub fn console(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Console");
        let asm_name = Some(asm.alloc_string("System.Console"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Collections.IDictionaryEnumerator`
    #[must_use]
    pub fn dictionary_iterator(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Collections.IDictionaryEnumerator");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Collections.IEnumerator`
    #[must_use]
    pub fn i_enumerator(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Collections.IEnumerator");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Collections.IDictionary`
    #[must_use]
    pub fn i_dictionary(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Collections.IDictionary");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Collections.ICollection`
    #[must_use]
    pub fn i_collection(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Collections.ICollection");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Environment`
    #[must_use]
    pub fn enviroment(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Environment");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Runtime.InteropServices.Marshal`
    #[must_use]
    pub fn marshal(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Runtime.InteropServices.Marshal");
        let asm_name = Some(asm.alloc_string("System.Runtime.InteropServices"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Collections.DictionaryEntry`
    #[must_use]
    pub fn dictionary_entry(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Collections.DictionaryEntry");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the class `System.Runtime.InteropServices.NativeMemory`
    #[must_use]
    pub fn native_mem(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Runtime.InteropServices.NativeMemory");
        let asm_name = Some(asm.alloc_string("System.Runtime.InteropServices"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.Span<T>` instantiated at element type
    /// `element` (a value type, e.g. `System.Span<uint8>`).
    #[must_use]
    pub fn span(asm: &mut Assembly, element: Type) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Span");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [element].into()))
    }
    /// Returns a reference to `System.ReadOnlySpan<T>` instantiated at element
    /// type `element` (a value type, e.g. `System.ReadOnlySpan<uint8>`). Backs
    /// `FileStream.Write(ReadOnlySpan<byte>)` in the dotnet fs PAL arm.
    #[must_use]
    pub fn read_only_span(asm: &mut Assembly, element: Type) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.ReadOnlySpan");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [element].into()))
    }
    /// Returns a reference to the class `System.IO.FileStream`, the open-file
    /// handle backing the dotnet `fs` PAL arm (Read/Write/Seek/Flush/Dispose/
    /// get_Length).
    #[must_use]
    pub fn file_stream(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.FileStream");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the static class `System.IO.File`
    /// (Delete/Move/Exists/GetAttributes) for the dotnet `fs` PAL arm.
    #[must_use]
    pub fn file(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.File");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the static class `System.IO.Directory`
    /// (CreateDirectory/Delete/Exists/GetFileSystemEntries) for the dotnet `fs`
    /// PAL arm.
    #[must_use]
    pub fn directory(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.Directory");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the static class `System.IO.RandomAccess`
    /// (`Read(SafeFileHandle, Span<byte>, long)` / `Write(SafeFileHandle,
    /// ReadOnlySpan<byte>, long)`) — the offset-relative file I/O backing the
    /// dotnet `fs` PAL `read_at`/`write_at` (B2 Piece 3). A reference type.
    #[must_use]
    pub fn random_access(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.RandomAccess");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class
    /// `Microsoft.Win32.SafeHandles.SafeFileHandle` — `RandomAccess.{Read,Write}`
    /// take this rather than a `FileStream`; the fs PAL bridges via the
    /// `FileStream.SafeFileHandle` getter (B2 Piece 3). A reference type.
    #[must_use]
    pub fn safe_file_handle(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("Microsoft.Win32.SafeHandles.SafeFileHandle");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the abstract class `System.IO.FileSystemInfo` — the
    /// return type of `File.CreateSymbolicLink`/`File.ResolveLinkTarget`; the fs
    /// PAL reads its `FullName` to recover a `readlink` target (B2 Piece 4). A
    /// reference type.
    #[must_use]
    pub fn file_system_info(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.FileSystemInfo");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the static class `System.IO.Path`
    /// (`GetTempPath`) for the dotnet `paths` PAL arm (PACKAGE A).
    #[must_use]
    pub fn path_io(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.Path");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.IO.FileInfo`
    /// (`new FileInfo(string).get_Length`) for sizing files in the dotnet `fs`
    /// PAL arm.
    #[must_use]
    pub fn file_info(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.FileInfo");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the int-backed enum `System.IO.FileMode` (a value
    /// type) — needed so `new FileStream(string, FileMode, FileAccess)` resolves
    /// to a real BCL ctor (an `int32` would not match the parameter type).
    #[must_use]
    pub fn file_mode(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.FileMode");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the int-backed enum `System.IO.FileAccess` (a value
    /// type) — paired with [`Self::file_mode`] for the `FileStream` ctor.
    #[must_use]
    pub fn file_access(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.FileAccess");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the int-backed enum `System.IO.SeekOrigin` (a value
    /// type) — for `FileStream.Seek(long, SeekOrigin)`.
    #[must_use]
    pub fn seek_origin(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.SeekOrigin");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the int-backed `[Flags]` enum `System.IO.FileAttributes`
    /// (a value type) — for `File.{Get,Set}Attributes`, backing the dotnet `fs` PAL
    /// `set_perm` (the read-only bit; `ReadOnly = 1`, `Normal = 128`).
    #[must_use]
    pub fn file_attributes(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.FileAttributes");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the class `System.Net.Sockets.Socket`, the open
    /// socket handle backing the dotnet `net` PAL arm (Bind/Listen/Accept/
    /// Connect/Send/Receive/SendTo/ReceiveFrom/Shutdown/Dispose +
    /// LocalEndPoint/RemoteEndPoint). Physically lives in `System.Net.Sockets.dll`,
    /// but — exactly like the `System.IO.*` fs helpers — we name the assembly
    /// `System.Net.Sockets` (its real impl assembly — unlike the `System.IO.*` fs
    /// helpers, CoreCLR does NOT type-forward `System.Net.*` from `System.Runtime`,
    /// so the net helpers must name their physical assemblies). `Socket`,
    /// `SocketType`, `ProtocolType` and `SocketShutdown` live in
    /// `System.Net.Sockets`; `IPAddress`/`IPEndPoint`/`EndPoint`/`AddressFamily`
    /// live in `System.Net.Primitives`. The exe path resolves these simple-name
    /// extern refs leniently at runtime.
    #[must_use]
    pub fn socket(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.Sockets.Socket");
        let asm_name = Some(asm.alloc_string("System.Net.Sockets"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the int-backed enum `System.Net.Sockets.SocketShutdown`
    /// (a value type) — for `Socket.Shutdown(SocketShutdown)` in the dotnet `net`
    /// PAL arm. In `System.Net.Sockets`.
    #[must_use]
    pub fn socket_shutdown(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.Sockets.SocketShutdown");
        let asm_name = Some(asm.alloc_string("System.Net.Sockets"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to `System.Net.Sockets.SocketException` — the exception
    /// the BCL throws on a socket fault. The POSIX shim's errno translation
    /// (`map_socket_exception_to_errno`) reads its `SocketErrorCode` to derive a
    /// POSIX errno. A reference type (not a value type). In `System.Net.Sockets`
    /// (well, `System.Net.Primitives`, but the exe path resolves the simple-name
    /// extern ref leniently at runtime, exactly like the other net classes).
    #[must_use]
    pub fn socket_exception(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.Sockets.SocketException");
        let asm_name = Some(asm.alloc_string("System.Net.Primitives"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.IO.FileNotFoundException` — thrown by the
    /// BCL when a file path does not exist (e.g. `new FileStream` on a missing
    /// file). The fs errno mapper (`rcl_errno_from_exception`) maps it to
    /// `ENOENT`. HOST-AGNOSTIC: the exception type is thrown identically on
    /// Unix-host and Windows-host CoreCLR. A reference type, in `System.Runtime`.
    #[must_use]
    pub fn file_not_found_exception(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.FileNotFoundException");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.IO.DirectoryNotFoundException` — thrown by
    /// the BCL when a directory in a path does not exist. Maps to `ENOENT`.
    /// HOST-AGNOSTIC. A reference type, in `System.Runtime`.
    #[must_use]
    pub fn directory_not_found_exception(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.DirectoryNotFoundException");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.UnauthorizedAccessException` (note: in the
    /// `System` namespace, NOT `System.IO`; it derives from `SystemException`, NOT
    /// `IOException`) — thrown by the BCL on a permission/ACL denial. Maps to
    /// `EACCES`. HOST CAVEAT: the *mapping* is host-agnostic, but the *meaning* of
    /// EACCES (rwx/uid/gid) is only faithful on a Unix host; a Windows-host
    /// CoreCLR throws this for ACL denials too and has no POSIX permission model,
    /// so PermissionDenied fidelity is Unix-host-best-effort. A reference type, in
    /// `System.Runtime`.
    #[must_use]
    pub fn unauthorized_access_exception(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.UnauthorizedAccessException");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.IO.PathTooLongException` — thrown by the BCL
    /// when a path exceeds the platform limit. Maps to `ENAMETOOLONG`.
    /// HOST-AGNOSTIC. A reference type, in `System.Runtime`.
    #[must_use]
    pub fn path_too_long_exception(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.PathTooLongException");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the int-backed enum `System.Net.Sockets.SocketError`
    /// (a value type) — the type returned by `SocketException.SocketErrorCode`. The
    /// errno translation reads it (as its underlying i32) to derive a POSIX errno.
    /// Must be the enum type, not raw i32: the CLR matches the property's signature
    /// EXACTLY (`SocketError get_SocketErrorCode()`), so an i32 return type yields a
    /// runtime `MissingMethodException`. In `System.Net.Primitives`.
    #[must_use]
    pub fn socket_error(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.Sockets.SocketError");
        let asm_name = Some(asm.alloc_string("System.Net.Primitives"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the int-backed enum `System.Net.Sockets.SelectMode`
    /// (a value type) — selects the readiness mode (SelectRead=0 / SelectWrite=1 /
    /// SelectError=2) for `Socket.Poll(int microSeconds, SelectMode)` in the dotnet
    /// mio PAL arm (the readiness multiplexer behind mio's Selector). In
    /// `System.Net.Sockets`.
    #[must_use]
    pub fn select_mode(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.Sockets.SelectMode");
        let asm_name = Some(asm.alloc_string("System.Net.Sockets"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the int-backed enum `System.Net.Sockets.AddressFamily`
    /// (a value type) — selects IPv4/IPv6 for `new Socket(AddressFamily, …)` in the
    /// dotnet `net` PAL arm. In `System.Net.Primitives` (NOT `System.Net.Sockets`).
    #[must_use]
    pub fn address_family(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.Sockets.AddressFamily");
        let asm_name = Some(asm.alloc_string("System.Net.Primitives"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the int-backed enum `System.Net.Sockets.SocketType`
    /// (a value type) — Stream/Dgram for `new Socket(…, SocketType, …)`. In
    /// `System.Net.Sockets`.
    #[must_use]
    pub fn socket_type(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.Sockets.SocketType");
        let asm_name = Some(asm.alloc_string("System.Net.Sockets"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the int-backed enum `System.Net.Sockets.ProtocolType`
    /// (a value type) — Tcp/Udp for `new Socket(…, …, ProtocolType)`. In
    /// `System.Net.Sockets`.
    #[must_use]
    pub fn protocol_type(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.Sockets.ProtocolType");
        let asm_name = Some(asm.alloc_string("System.Net.Sockets"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the class `System.Net.IPAddress` (the IP-address
    /// value carried in an `IPEndPoint`) for the dotnet `net` PAL arm. Built from
    /// network-order octets via `new IPAddress(ReadOnlySpan<byte>)`. In
    /// `System.Net.Primitives`.
    #[must_use]
    pub fn ip_address(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.IPAddress");
        let asm_name = Some(asm.alloc_string("System.Net.Primitives"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Net.IPEndPoint` (an IPAddress +
    /// port) for the dotnet `net` PAL arm. Never crosses the Rust ABI — it is
    /// built/read entirely BCL-side from the decomposed `(family, ip, port)`. In
    /// `System.Net.Primitives`.
    #[must_use]
    pub fn ip_endpoint(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.IPEndPoint");
        let asm_name = Some(asm.alloc_string("System.Net.Primitives"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the abstract base class `System.Net.EndPoint` — the
    /// declared return type of `Socket.LocalEndPoint`/`RemoteEndPoint` and the
    /// `ref` seed type of `Socket.ReceiveFrom`, downcast to `IPEndPoint`
    /// BCL-side in the dotnet `net` PAL arm. In `System.Net.Primitives`.
    #[must_use]
    pub fn endpoint(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.EndPoint");
        let asm_name = Some(asm.alloc_string("System.Net.Primitives"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class
    /// `System.Net.Sockets.UnixDomainSocketEndPoint` — the `EndPoint` subclass
    /// for path-based AF_UNIX sockets (`new UnixDomainSocketEndPoint(string)`),
    /// upcast to `EndPoint` for `Socket.Bind`/`Connect` exactly like `IPEndPoint`
    /// (B2 Piece 1). A reference type. In `System.Net.Sockets` (NOT Primitives).
    #[must_use]
    pub fn unix_domain_socket_endpoint(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Net.Sockets.UnixDomainSocketEndPoint");
        let asm_name = Some(asm.alloc_string("System.Net.Sockets"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class
    /// `System.Security.Cryptography.RandomNumberGenerator`.
    #[must_use]
    pub fn random_number_generator(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Security.Cryptography.RandomNumberGenerator");
        let asm_name = Some(asm.alloc_string("System.Security.Cryptography"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Diagnostics.Stopwatch`, the
    /// monotonic high-resolution timer backing the `Instant` PAL hooks.
    #[must_use]
    pub fn stopwatch(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Diagnostics.Stopwatch");
        let asm_name = Some(asm.alloc_string("System.Runtime.Extensions"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.Diagnostics.ProcessStartInfo` — the spawn
    /// recipe (FileName/Arguments/WorkingDirectory/Redirect*) for the dotnet
    /// `process` PAL arm. A reference type in assembly `System.Diagnostics.Process`.
    #[must_use]
    pub fn process_start_info(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Diagnostics.ProcessStartInfo");
        let asm_name = Some(asm.alloc_string("System.Diagnostics.Process"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.Diagnostics.Process` — a spawned child
    /// (Start/WaitForExit/ExitCode/Id/Kill/HasExited) for the dotnet `process`
    /// PAL arm. A reference type in assembly `System.Diagnostics.Process`.
    #[must_use]
    pub fn process(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.Diagnostics.Process");
        let asm_name = Some(asm.alloc_string("System.Diagnostics.Process"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the abstract class `System.IO.Stream` — the raw byte
    /// stream backing a child's redirected stdout/stderr/stdin (`Read`/`Write`/
    /// `Dispose`) for the dotnet `process` capture path. A reference type.
    #[must_use]
    pub fn stream(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.Stream");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.IO.StreamReader` — `Process.StandardOutput`/
    /// `StandardError`; the PAL reads its `BaseStream` for raw child output.
    #[must_use]
    pub fn stream_reader(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.StreamReader");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to `System.IO.StreamWriter` — `Process.StandardInput`;
    /// the PAL writes its `BaseStream` for raw child input.
    #[must_use]
    pub fn stream_writer(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.IO.StreamWriter");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the value type `System.DateTime`, the wall-clock
    /// struct backing the `SystemTime` PAL hook.
    #[must_use]
    pub fn datetime(asm: &mut Assembly) -> Interned<ClassRef> {
        let name = asm.alloc_string("System.DateTime");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        // value type: instance calls take a managed `this` pointer.
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    /// Returns a reference to the class `System.Numerics.BitOperations`
    #[must_use]
    pub fn bit_operations(asm: &mut Assembly) -> Interned<ClassRef> {
        let name: Interned<IString> = asm.alloc_string("System.Numerics.BitOperations");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.Buffers.Binary.BinaryPrimitives`
    #[must_use]
    pub fn binary_primitives(asm: &mut Assembly) -> Interned<ClassRef> {
        let name: Interned<IString> = asm.alloc_string("System.Buffers.Binary.BinaryPrimitives");
        let asm_name = Some(asm.alloc_string("System.Memory"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    }
    /// Returns a reference to the class `System.MidpointRounding`
    #[must_use]
    pub fn midpoint_rounding(asm: &mut Assembly) -> Interned<ClassRef> {
        let name: Interned<IString> = asm.alloc_string("System.MidpointRounding");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    }
    #[must_use]
    pub fn fixed_array(element: Type, length: u64, asm: &mut Assembly) -> Interned<ClassRef> {
        let name = format!("{element}_{length}", element = element.mangle(asm));
        let name = asm.alloc_string(name);
        let cref = ClassRef::new(name, None, true, [].into());
        asm.alloc_class_ref(cref)
    }
    /// Returns a reference to the constructor of this class  - `.ctor`. The explict inputs of the constructor should not include `this` - that parameter will be automaticaly provided.
    pub fn ctor(&self, explict_inputs: &[Type], asm: &mut Assembly) -> Interned<MethodRef> {
        let this = asm.alloc_class_ref(self.clone());
        let mut inputs = vec![Type::ClassRef(this)];
        inputs.extend(explict_inputs);
        let sig = asm.sig(inputs, Type::Void);
        let fn_name = asm.alloc_string(".ctor");
        asm.alloc_methodref(MethodRef::new(
            this,
            fn_name,
            sig,
            super::cilnode::MethodKind::Constructor,
            [].into(),
        ))
    }
    /// Returns a reference to an instance method of this class, with a given name. The explict inputs of the method should not include `this` - that parameter will be automaticaly provided.
    pub fn instance(
        &self,
        explict_inputs: &[Type],
        output: Type,
        fn_name: Interned<IString>,
        asm: &mut Assembly,
    ) -> Interned<MethodRef> {
        let this = asm.alloc_class_ref(self.clone());
        let mut inputs = if self.is_valuetype() {
            vec![asm.nref(Type::ClassRef(this))]
        } else {
            vec![Type::ClassRef(this)]
        };

        inputs.extend(explict_inputs);
        let sig = asm.sig(inputs, output);
        asm.alloc_methodref(MethodRef::new(
            this,
            fn_name,
            sig,
            super::cilnode::MethodKind::Instance,
            [].into(),
        ))
    }
    /// Returns a reference to an virtual method of this class, with a given name. The explict inputs of the method should not include `this` - that parameter will be automaticaly provided.
    pub fn virtual_mref(
        &self,
        explict_inputs: &[Type],
        output: Type,
        fn_name: Interned<IString>,
        asm: &mut Assembly,
    ) -> Interned<MethodRef> {
        let this = asm.alloc_class_ref(self.clone());
        let mut inputs = vec![Type::ClassRef(this)];
        inputs.extend(explict_inputs);
        let sig = asm.sig(inputs, output);
        asm.alloc_methodref(MethodRef::new(
            this,
            fn_name,
            sig,
            super::cilnode::MethodKind::Virtual,
            [].into(),
        ))
    }
    /// Returns a reference to an static method of this class, with a given name.
    pub fn static_mref(
        &self,
        inputs: &[Type],
        output: Type,
        fn_name: Interned<IString>,
        asm: &mut Assembly,
    ) -> Interned<MethodRef> {
        let this = asm.alloc_class_ref(self.clone());
        let sig = asm.sig(inputs, output);
        asm.alloc_methodref(MethodRef::new(
            this,
            fn_name,
            sig,
            super::cilnode::MethodKind::Static,
            [].into(),
        ))
    }
    /// Returns a reference to an static method of this class, with a given name.
    pub fn static_mref_generic(
        &self,
        inputs: &[Type],
        output: Type,
        fn_name: Interned<IString>,
        asm: &mut Assembly,
        generics: Box<[Type]>,
    ) -> Interned<MethodRef> {
        let this = asm.alloc_class_ref(self.clone());
        let sig = asm.sig(inputs, output);
        asm.alloc_methodref(MethodRef::new(
            this,
            fn_name,
            sig,
            super::cilnode::MethodKind::Static,
            generics,
        ))
    }
    // Returns a `System.Collections.Concurrent.ConcurrentDictionary` of key,value
    pub fn concurent_dictionary(key: Type, value: Type, asm: &mut Assembly) -> Interned<ClassRef> {
        let name: Interned<IString> =
            asm.alloc_string("System.Collections.Concurrent.ConcurrentDictionary");
        let asm_name = Some(asm.alloc_string("System.Collections.Concurrent"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [key, value].into()))
    }
    // Returns a `System.Collections.Generic.Dictionary` of key,value
    pub fn dictionary(key: Type, value: Type, asm: &mut Assembly) -> Interned<ClassRef> {
        let name: Interned<IString> = asm.alloc_string("System.Collections.Generic.Dictionary");
        let asm_name = Some(asm.alloc_string("System.Collections"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [key, value].into()))
    }

    pub fn set_generics(&mut self, generics: Vec<Type>) {
        self.generics = generics.into();
    }
}
#[derive(Hash, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct StaticFieldDef {
    pub tpe: Type,
    pub name: Interned<IString>,
    pub is_tls: bool,
    pub default_value: Option<Const>,
    pub is_const: bool,
}
impl PartialEq for StaticFieldDef {
    fn eq(&self, other: &Self) -> bool {
        self.tpe == other.tpe
            && self.name == other.name
            && self.is_tls == other.is_tls
            && self.is_const == other.is_const
    }
}
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ClassDef {
    name: Interned<IString>,
    is_valuetype: bool,
    generics: u32,
    extends: Option<Interned<ClassRef>>,
    fields: Vec<(Type, Interned<IString>, Option<u32>)>,
    static_fields: Vec<StaticFieldDef>,
    methods: Vec<MethodDefIdx>,
    access: Access,
    explict_size: Option<NonZeroU32>,
    align: Option<NonZeroU32>,
    has_nonveralpping_layout: bool,
}
impl ClassDef {
    /// Checks if this class defition has a with the name and type.
    #[must_use]
    pub fn has_static_field(&self, fld_name: Interned<IString>, fld_tpe: Type) -> bool {
        self.static_fields
            .iter()
            .any(|StaticFieldDef { tpe, name, .. }| *tpe == fld_tpe && *name == fld_name)
    }
    pub(crate) fn iter_types(&self) -> impl Iterator<Item = Type> + '_ {
        self.fields()
            .iter()
            .map(|(tpe, _, _)| tpe)
            .chain(
                self.static_fields()
                    .iter()
                    .map(|StaticFieldDef { tpe, .. }| tpe),
            )
            .copied()
            .chain(self.extends.iter().map(|cref| Type::ClassRef(*cref)))
    }
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        name: Interned<IString>,
        is_valuetype: bool,
        generics: u32,
        extends: Option<Interned<ClassRef>>,
        fields: Vec<(Type, Interned<IString>, Option<u32>)>,
        static_fields: Vec<StaticFieldDef>,
        access: Access,
        explict_size: Option<NonZeroU32>,
        align: Option<NonZeroU32>,
        has_nonveralpping_layout: bool,
    ) -> Self {
        //assert_unique(&methods);
        Self {
            name,
            is_valuetype,
            generics,
            extends,
            fields,
            static_fields,
            methods: vec![],
            access,
            explict_size,
            align,
            has_nonveralpping_layout,
        }
    }

    pub(crate) fn ref_to(&self) -> ClassRef {
        assert_eq!(self.generics, 0);
        ClassRef::new(self.name, None, self.is_valuetype, vec![].into())
    }
    pub fn layout_check(&self, asm: &Assembly) -> Result<(), LayoutError> {
        if !self.has_nonveralpping_layout() {
            for (t, name, _offset) in self.fields() {
                if t.is_gcref(asm) {
                    return Err(LayoutError::ManagedRefInOverlapingField {
                        owner: asm[self.name()].into(),
                        field: t.mangle(asm),
                        name: asm[*name].into(),
                    });
                }
            }
        }
        Ok(())
    }
    pub fn add_def(&mut self, val: MethodDefIdx) {
        self.methods.push(val);
        assert_unique(self.methods(), "add_def failed: method were not unique!");
    }
    pub fn methods_mut(&mut self) -> &mut Vec<MethodDefIdx> {
        &mut self.methods
    }

    pub fn static_fields_mut(&mut self) -> &mut Vec<StaticFieldDef> {
        &mut self.static_fields
    }
    pub fn fields_mut(&mut self) -> &mut Vec<(Type, Interned<IString>, Option<u32>)> {
        &mut self.fields
    }
    #[must_use]
    pub fn access(&self) -> &Access {
        &self.access
    }

    #[must_use]
    pub fn is_valuetype(&self) -> bool {
        self.is_valuetype
    }

    #[must_use]
    pub fn extends(&self) -> Option<Interned<ClassRef>> {
        self.extends
    }

    pub(crate) fn has_explicit_layout(&self) -> bool {
        self.explict_size.is_some() || self.fields.iter().any(|(_, _, offset)| offset.is_some())
    }

    #[must_use]
    pub fn fields(&self) -> &[(Type, Interned<IString>, Option<u32>)] {
        &self.fields
    }

    #[must_use]
    pub fn name(&self) -> Interned<IString> {
        self.name
    }

    #[must_use]
    pub fn static_fields(&self) -> &[StaticFieldDef] {
        &self.static_fields
    }

    #[must_use]
    pub fn methods(&self) -> &[MethodDefIdx] {
        &self.methods
    }

    #[must_use]
    pub fn explict_size(&self) -> Option<NonZeroU32> {
        self.explict_size
    }

    #[must_use]
    pub fn generics(&self) -> u32 {
        self.generics
    }

    pub(super) fn merge_defs(&mut self, translated: ClassDef) {
        // Check name matches
        assert_eq!(self.name(), translated.name());

        // Check valuetype matches
        assert_eq!(self.is_valuetype(), translated.is_valuetype());
        // Check generic count matches
        assert_eq!(self.generics(), translated.generics());
        // Check inheretence matches
        assert_eq!(self.extends(), translated.extends());

        // Merge the static fields, removing duplicates
        self.static_fields_mut()
            .extend(translated.static_fields().iter().cloned());
        make_unique(&mut self.static_fields);
        // Merge the methods, removing duplicates
        self.methods_mut().extend(translated.methods());
        make_unique(self.methods_mut());
        // Check accessibility matches
        assert_eq!(self.access(), translated.access());
    }

    pub fn align(&self) -> Option<NonZeroU32> {
        self.align
    }

    pub fn has_nonveralpping_layout(&self) -> bool {
        self.has_nonveralpping_layout
    }
    /*
    /// Optimizes this class definition, consuming fuel
    pub fn opt(&mut self, fuel: &mut OptFuel, asm: &mut Assembly, cache: &mut SideEffectInfoCache) {
    } */
}
fn into_unique<T: Eq + std::hash::Hash>(input: Vec<T>) -> Vec<T> {
    let set: fxhash::FxHashSet<_> = input.into_iter().collect();
    set.into_iter().collect()
}
fn make_unique<T: Eq + std::hash::Hash>(input: &mut Vec<T>) {
    let mut tmp = Vec::new();
    std::mem::swap(&mut tmp, input);
    let mut tmp = into_unique(tmp);
    std::mem::swap(&mut tmp, input);
}
#[test]
fn test_into_unique() {
    assert!(into_unique::<u32>(vec![]).is_empty());
    assert_eq!(into_unique::<u32>(vec![0]), vec![0]);
    assert_eq!(into_unique::<u32>(vec![0, 0]), vec![0]);
    assert_eq!(into_unique::<u32>(vec![2, 1, 1]).len(), 2);
    let mut v = vec![];
    make_unique::<u32>(&mut v);
    assert!(v.is_empty());
    let mut v = vec![0];
    make_unique::<u32>(&mut v);
    assert_eq!(v, vec![0]);
    let mut v = vec![0, 1];
    make_unique::<u32>(&mut v);
    assert_eq!(v, vec![0, 1]);
    let mut v = vec![2, 1, 1];
    make_unique::<u32>(&mut v);
    assert_eq!(v.len(), 2);
}
#[test]
fn has_explicit_layout() {
    let vt = [true, false];
    for is_valuetype in vt {
        let mut asm = Assembly::default();
        let name = asm.alloc_string("MyClass");
        let def = ClassDef::new(
            name,
            is_valuetype,
            0,
            None,
            vec![],
            vec![],
            Access::Extern,
            None,
            None,
            true,
        );
        assert!(def.explict_size().is_none());
        assert!(!def.has_explicit_layout());
        assert_eq!(is_valuetype, def.is_valuetype());
        assert_eq!(is_valuetype, def.ref_to().is_valuetype());
        let def = ClassDef::new(
            name,
            is_valuetype,
            0,
            None,
            vec![],
            vec![],
            Access::Extern,
            Some(NonZeroU32::new(1000).unwrap()),
            None,
            true,
        );
        assert_eq!(def.fields().len(), 0);
        assert!(def.has_explicit_layout());
        assert_eq!(is_valuetype, def.is_valuetype());
        assert_eq!(is_valuetype, def.ref_to().is_valuetype());
        let def = ClassDef::new(
            name,
            is_valuetype,
            0,
            None,
            vec![(Type::Bool, name, Some(1000))],
            vec![],
            Access::Extern,
            None,
            None,
            true,
        );
        assert!(def.explict_size().is_none());
        assert_eq!(def.fields().len(), 1);
        assert!(def.has_explicit_layout());
        assert_eq!(is_valuetype, def.is_valuetype());
        assert_eq!(is_valuetype, def.ref_to().is_valuetype());
        let mut def = ClassDef::new(
            name,
            is_valuetype,
            0,
            None,
            vec![],
            vec![StaticFieldDef {
                tpe: Type::Bool,
                name,
                is_tls: false,
                default_value: None,
                is_const: false,
            }],
            Access::Extern,
            None,
            None,
            true,
        );
        assert!(def.explict_size().is_none());
        assert_eq!(def.static_fields().len(), 1);
        assert_eq!(def.static_fields_mut().len(), 1);
        assert!(def.has_static_field(name, Type::Bool));
        assert!(!def.has_static_field(name, Type::PlatformChar));
        assert!(!def.has_static_field(asm.alloc_string("CuteString"), Type::Bool));
        assert!(!def.has_explicit_layout());
        assert_eq!(is_valuetype, def.is_valuetype());
        assert_eq!(is_valuetype, def.ref_to().is_valuetype());
        let def = ClassDef::new(
            name,
            is_valuetype,
            0,
            None,
            vec![(Type::Bool, name, None)],
            vec![],
            Access::Extern,
            None,
            None,
            true,
        );
        assert!(def.explict_size().is_none());
        assert_eq!(def.fields().len(), 1);
        assert_eq!(is_valuetype, def.is_valuetype());
        assert_eq!(is_valuetype, def.ref_to().is_valuetype());
        assert!(!def.has_explicit_layout());
        let def = ClassDef::new(
            name,
            is_valuetype,
            0,
            None,
            vec![(Type::Bool, name, Some(1000))],
            vec![],
            Access::Extern,
            Some(NonZeroU32::new(1000).unwrap()),
            None,
            true,
        );
        assert_eq!(def.explict_size(), Some(NonZeroU32::new(1000).unwrap()));
        assert_eq!(def.fields().len(), 1);
        assert!(def.has_explicit_layout());
        assert_eq!(is_valuetype, def.is_valuetype());
        assert_eq!(is_valuetype, def.ref_to().is_valuetype());
    }
}
#[test]
fn generics() {
    let mut asm = Assembly::default();
    let name = asm.alloc_string("MyClass");
    let def = ClassDef::new(
        name,
        false,
        0,
        None,
        vec![],
        vec![],
        Access::Extern,
        None,
        None,
        true,
    );
    assert_eq!(def.generics(), 0);
    assert_eq!(def.ref_to().generics(), &[]);
    let def = ClassDef::new(
        name,
        false,
        5,
        None,
        vec![],
        vec![],
        Access::Extern,
        None,
        None,
        true,
    );
    assert_eq!(def.generics(), 5);
}
#[test]
fn display_class_ref() {
    let mut asm = Assembly::default();
    let name: Interned<IString> = asm.alloc_string("MyClass");
    let def = ClassDef::new(
        name,
        false,
        0,
        None,
        vec![],
        vec![],
        Access::Extern,
        None,
        None,
        true,
    );
    assert_eq!(
        def.ref_to().display(&asm),
        "ClassRef{name:MyClass,asm:None,is_valuetype:false,generics[]}"
    );
}
#[test]
fn type_gc() {
    let mut asm = Assembly::default();
    let name: Interned<IString> = asm.alloc_string("Stay");
    asm.class_def(ClassDef::new(
        name,
        false,
        0,
        None,
        vec![],
        vec![],
        Access::Extern,
        None,
        None,
        true,
    ))
    .unwrap();
    let name: Interned<IString> = asm.alloc_string("Gone");
    asm.class_def(ClassDef::new(
        name,
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
    assert_eq!(asm.class_defs().len(), 2);
    asm.eliminate_dead_types();
    assert_eq!(asm.class_defs().len(), 1);
}
#[test]
fn merge_defs() {
    let mut asm = Assembly::default();
    let name: Interned<IString> = asm.alloc_string("Stay");
    let def = ClassDef::new(
        name,
        false,
        0,
        None,
        vec![],
        vec![],
        Access::Extern,
        None,
        None,
        true,
    );

    def.clone().merge_defs(def);
}
#[test]
#[should_panic]
fn merge_defs_different() {
    let mut asm = Assembly::default();
    let name: Interned<IString> = asm.alloc_string("Stay");
    let mut stay = ClassDef::new(
        name,
        false,
        0,
        None,
        vec![],
        vec![],
        Access::Extern,
        None,
        None,
        true,
    );
    let name: Interned<IString> = asm.alloc_string("Gone");
    let gone = ClassDef::new(
        name,
        false,
        0,
        None,
        vec![],
        vec![],
        Access::Public,
        None,
        None,
        true,
    );

    stay.merge_defs(gone);
}
#[test]
fn extends() {
    let mut asm = Assembly::default();
    let name: Interned<IString> = asm.alloc_string("Stay");
    let exception = ClassRef::exception(&mut asm);
    let def = ClassDef::new(
        name,
        false,
        0,
        None,
        vec![],
        vec![],
        Access::Extern,
        None,
        None,
        true,
    );
    assert_eq!(def.iter_types().count(), 0);
    assert!(def.extends().is_none());
    let def = ClassDef::new(
        name,
        false,
        0,
        Some(exception),
        vec![],
        vec![],
        Access::Extern,
        None,
        None,
        true,
    );
    assert_eq!(def.extends(), Some(exception));
    assert_eq!(def.iter_types().count(), 1);
}
#[test]
fn class_ref() {
    let mut asm = Assembly::default();
    let names = [
        asm.alloc_string("CuteClass"),
        asm.alloc_string("SpookyClass"),
        asm.alloc_string("BraveClass"),
    ];
    let asms = [
        None,
        Some(asm.alloc_string("NiceAssembly")),
        Some(asm.alloc_string("GreatAssembly")),
    ];
    let valuetypes = [false, true];
    let generics = [
        vec![],
        vec![Type::Bool],
        vec![Type::Bool, Type::PlatformObject],
    ];
    for name in names {
        for asm in asms {
            for valuetype in valuetypes {
                for generic in &generics {
                    let cref = ClassRef::new(name, asm, valuetype, generic.clone().into());
                    assert_eq!(cref.name(), name);
                    assert_eq!(cref.asm(), asm);
                    assert_eq!(cref.is_valuetype(), valuetype);
                    assert_eq!(cref.generics(), generic);
                }
            }
        }
    }
}
