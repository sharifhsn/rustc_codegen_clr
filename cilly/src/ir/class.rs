use super::{
    Assembly, Const, MethodDefIdx, MethodRef, Type,
    asm_link::{RelocateCtx, RelocateValue},
    bimap::Interned,
};
use crate::Access;
use crate::{IString, utilis::assert_unique};
use serde::{Deserialize, Serialize};
use std::{num::NonZeroU32, ops::Deref};
#[derive(Debug)]
pub enum LayoutError {
    /// A GC-tracked field sits inside overlapping (`[FieldOffset]`-style union) storage at a
    /// byte offset another overlapping variant uses for a differently-shaped field — the CLR GC
    /// can't consistently interpret that byte range (an object reference in one variant, raw data
    /// or a different reference shape in another), so CoreCLR's class loader rejects it. Hit this
    /// by e.g. trying to hold a raw managed handle (or a struct nesting one, like
    /// `mycorrhiza::task::TaskFuture<T>`) inside an enum payload or an async fn's captured state
    /// at an offset another variant reuses incompatibly; fix by using a GCHandle-backed newtype
    /// (`mycorrhiza::class::Class<..>`, which has no gcref field at all) instead, or ensure the
    /// same gcref-shaped field is reused consistently across every overlapping variant.
    ManagedRefInOverlapingField {
        owner: String,
        field: String,
        name: String,
    },
}
#[derive(Hash, PartialEq, Eq, Copy, Clone, Debug, Serialize, Deserialize)]
pub struct ClassDefIdx(pub Interned<ClassRef>);
impl RelocateValue for ClassDefIdx {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self(class) = self;
        Self(ctx.class_ref(destination, class))
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

impl RelocateValue for ClassRef {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self {
            name,
            asm,
            is_valuetype,
            generics,
        } = self;
        Self {
            name: ctx.string(destination, name),
            asm: asm.map(|name| ctx.string(destination, name)),
            is_valuetype,
            generics: generics
                .iter()
                .map(|tpe| destination.translate_type(ctx, *tpe))
                .collect(),
        }
    }
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
    #[must_use]
    pub fn fixed_array_with_layout(
        element: Type,
        length: u64,
        requested_size: u64,
        requested_align: u64,
        asm: &mut Assembly,
    ) -> Interned<ClassRef> {
        // Element + length alone is not a physical-layout identity. Rust can give two fixed arrays
        // with the same logical lanes different storage alignment (for example an ordinary
        // `[u32; 32]` and the backing array inside `#[repr(align(128))]`). Non-native SIMD and
        // managed-sidecar normalization can also change CLR storage size independently of the Rust
        // semantic size. If those definitions share a TypeDef name, linking codegen shards either
        // silently chooses one layout or (correctly) rejects the conflict. Make the physical layout
        // request part of the synthetic type's identity instead. These values must be caller-known
        // and deterministic; opportunistic post-normalization details do not belong in an intern key.
        let name = format!(
            "{element}_{length}_s{requested_size}_a{requested_align}",
            element = element.mangle(asm)
        );
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
    // NOTE: kept hand-written (not folded into the `bcl_class!` table) because its
    // signature takes `asm` LAST — unlike the span/thread_local family which take
    // `asm` first — and hundreds of call-sites depend on that argument order.
    pub fn concurent_dictionary(key: Type, value: Type, asm: &mut Assembly) -> Interned<ClassRef> {
        let name: Interned<IString> =
            asm.alloc_string("System.Collections.Concurrent.ConcurrentDictionary");
        let asm_name = Some(asm.alloc_string("System.Collections.Concurrent"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [key, value].into()))
    }
    // Returns a `System.Collections.Generic.Dictionary` of key,value
    // NOTE: kept hand-written for the same `asm`-last reason as `concurent_dictionary`.
    pub fn dictionary(key: Type, value: Type, asm: &mut Assembly) -> Interned<ClassRef> {
        let name: Interned<IString> = asm.alloc_string("System.Collections.Generic.Dictionary");
        let asm_name = Some(asm.alloc_string("System.Collections"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [key, value].into()))
    }

    pub fn set_generics(&mut self, generics: Vec<Type>) {
        self.generics = generics.into();
    }
}
// The bulk of the `ClassRef` BCL-type constructors are near-identical one-liners
// (name string + assembly string + valuetype flag, sometimes a generic arg), so
// they are generated from a table by `bcl_class!`. The generated functions have
// exactly the same names/signatures as before (`ClassRef::double(asm)`, …), so
// every call-site across the repo is unchanged. `value`/`class` is the valuetype
// flag; the assembly string defaults to `"System.Runtime"` (used by most rows)
// and is given explicitly only when it differs. Load-bearing doc-comments are
// preserved verbatim above their rows. The handful of helpers whose bodies are
// not pure table rows (`fixed_array`, which formats its name; `ctor`/`instance`/
// `static_mref`/… instance helpers; the accessors) stay hand-written above.
crate::bcl_class! {
    impl ClassRef {
        interlocked => "System.Threading.Interlocked", "System.Threading", class;
        /// The .NET math class
        math => "System.Math", class;
        /// Retusn a reference to the class `System.Double`
        // `System.Double` is a .NET value type. It MUST be referenced as `valuetype`
        // (not `class`) in IL, or any call whose declaring type is `System.Double`
        // (e.g. `MinNumber`/`MaxNumber`/`Max`/`Min`/`FusedMultiplyAdd`/`Pow`) makes the
        // runtime reject the type-load with `TypeLoadException: ... value type mismatch`.
        double => "System.Double", value;
        /// Retusn a reference to the class `System.Single`
        // `System.Single` is a .NET value type — see `double` above.
        single => "System.Single", value;
        /// Returns a reference to the class `System.MathF`
        #[must_use]
        mathf => "System.MathF", class;
        /// Returns a reference to the `System.UInt128` type.
        uint_128 => "System.UInt128", value;
        /// Returns a reference to the `System.Int128` type.
        int_128 => "System.Int128", value;
        /// Returns a reference to the `System.UIntPtr` type.
        usize_type => "System.UIntPtr", value;
        /// Returns a reference to the `System.UInt16` type.
        uint16 => "System.UInt16", value;
        /// Returns a reference to the `System.Int16` type.
        int16 => "System.Int16", value;
        /// Returns a reference to the `System.UInt32` type.
        uint32 => "System.UInt32", value;
        /// Returns a reference to the `System.Int32` type.
        int32 => "System.Int32", value;
        /// Returns a reference to the `System.UInt64` type.
        uint64 => "System.UInt64", value;
        /// Returns a reference to the `System.Int64` type.
        int64 => "System.Int64", value;
        /// Returns a reference to the `System.IntPtr` type.
        isize_type => "System.IntPtr", value;
        /// Returns a reference to the `System.Half` type.
        half => "System.Half", value;
        /// Returns a reference to the `System.Byte` type.
        byte => "System.Byte", value;
        /// Returns a reference to the `System.SByte` type.
        sbyte => "System.SByte", value;
        /// Returns a reference to the GC handle class.
        gc_handle => "System.Runtime.InteropServices.GCHandle", value;
        /// Returns a reference to the `System.String`
        string => "System.String", class;
        /// Returns a reference to the `System.Object`
        object => "System.Object", class;
        /// Returns a reference to the `System.Threading.Thread`
        thread => "System.Threading.Thread", "System.Threading.Thread", class;
        /// Returns a reference to the `System.Threading.ThreadStart`
        thread_start => "System.Threading.ThreadStart", "System.Threading.Thread", class;
        /// Returns a reference to the `System.Threading.SemaphoreSlim`
        // SemaphoreSlim physically lives in System.Private.CoreLib; method bodies
        // must name the IMPL assembly (the runtime resolves it directly), while
        // `ref_assembly_name` normalizes it to System.Runtime in the C#-visible
        // metadata. Naming `System.Runtime` here makes the body's
        // `[System.Runtime]SemaphoreSlim` unresolvable at run time (TypeLoadException).
        semaphore_slim => "System.Threading.SemaphoreSlim", "System.Private.CoreLib", class;
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
        thread_local => "System.Threading.ThreadLocal", "System.Private.CoreLib", class, generics(element);
        /// Returns a reference to the `System.Type`
        type_type => "System.Type", class;
        /// Returns a reference to the `System.RuntimeTypeHandle`
        runtime_type_hadle => "System.RuntimeTypeHandle", value;
        /// Returns a reference to the `System.String`
        exception => "System.Exception", class;
        /// Returns a reference to the `System.Console`
        console => "System.Console", "System.Console", class;
        /// Returns a reference to the class `System.Collections.IDictionaryEnumerator`
        #[must_use]
        dictionary_iterator => "System.Collections.IDictionaryEnumerator", class;
        /// Returns a reference to the class `System.Collections.IEnumerator`
        #[must_use]
        i_enumerator => "System.Collections.IEnumerator", class;
        /// Returns a reference to the class `System.Collections.IDictionary`
        #[must_use]
        i_dictionary => "System.Collections.IDictionary", class;
        /// Returns a reference to the class `System.Collections.ICollection`
        #[must_use]
        i_collection => "System.Collections.ICollection", class;
        /// Returns a reference to the class `System.Environment`
        #[must_use]
        enviroment => "System.Environment", class;
        /// Returns a reference to the class `System.Runtime.InteropServices.Marshal`
        #[must_use]
        marshal => "System.Runtime.InteropServices.Marshal", "System.Runtime.InteropServices", class;
        /// Returns a reference to the class `System.Collections.DictionaryEntry`
        #[must_use]
        dictionary_entry => "System.Collections.DictionaryEntry", value;
        /// Returns a reference to the class `System.Runtime.InteropServices.NativeMemory`
        #[must_use]
        native_mem => "System.Runtime.InteropServices.NativeMemory", "System.Runtime.InteropServices", class;
        /// Returns a reference to `System.Span<T>` instantiated at element type
        /// `element` (a value type, e.g. `System.Span<uint8>`).
        #[must_use]
        span => "System.Span", value, generics(element);
        /// Returns a reference to `System.ReadOnlySpan<T>` instantiated at element
        /// type `element` (a value type, e.g. `System.ReadOnlySpan<uint8>`). Backs
        /// `FileStream.Write(ReadOnlySpan<byte>)` in the dotnet fs PAL arm.
        #[must_use]
        read_only_span => "System.ReadOnlySpan", value, generics(element);
        /// Returns a reference to the class `System.IO.FileStream`, the open-file
        /// handle backing the dotnet `fs` PAL arm (Read/Write/Seek/Flush/Dispose/
        /// get_Length).
        #[must_use]
        file_stream => "System.IO.FileStream", class;
        /// Returns a reference to the static class `System.IO.File`
        /// (Delete/Move/Exists/GetAttributes) for the dotnet `fs` PAL arm.
        #[must_use]
        file => "System.IO.File", class;
        /// Returns a reference to the static class `System.IO.Directory`
        /// (CreateDirectory/Delete/Exists/GetFileSystemEntries) for the dotnet `fs`
        /// PAL arm.
        #[must_use]
        directory => "System.IO.Directory", class;
        /// Returns a reference to the static class `System.IO.RandomAccess`
        /// (`Read(SafeFileHandle, Span<byte>, long)` / `Write(SafeFileHandle,
        /// ReadOnlySpan<byte>, long)`) — the offset-relative file I/O backing the
        /// dotnet `fs` PAL `read_at`/`write_at` (B2 Piece 3). A reference type.
        #[must_use]
        random_access => "System.IO.RandomAccess", class;
        /// Returns a reference to the class
        /// `Microsoft.Win32.SafeHandles.SafeFileHandle` — `RandomAccess.{Read,Write}`
        /// take this rather than a `FileStream`; the fs PAL bridges via the
        /// `FileStream.SafeFileHandle` getter (B2 Piece 3). A reference type.
        #[must_use]
        safe_file_handle => "Microsoft.Win32.SafeHandles.SafeFileHandle", class;
        /// Returns a reference to the abstract class `System.IO.FileSystemInfo` — the
        /// return type of `File.CreateSymbolicLink`/`File.ResolveLinkTarget`; the fs
        /// PAL reads its `FullName` to recover a `readlink` target (B2 Piece 4). A
        /// reference type.
        #[must_use]
        file_system_info => "System.IO.FileSystemInfo", class;
        /// Returns a reference to the static class `System.IO.Path`
        /// (`GetTempPath`) for the dotnet `paths` PAL arm (PACKAGE A).
        #[must_use]
        path_io => "System.IO.Path", class;
        /// Returns a reference to the class `System.IO.FileInfo`
        /// (`new FileInfo(string).get_Length`) for sizing files in the dotnet `fs`
        /// PAL arm.
        #[must_use]
        file_info => "System.IO.FileInfo", class;
        /// Returns a reference to the int-backed enum `System.IO.FileMode` (a value
        /// type) — needed so `new FileStream(string, FileMode, FileAccess)` resolves
        /// to a real BCL ctor (an `int32` would not match the parameter type).
        #[must_use]
        file_mode => "System.IO.FileMode", value;
        /// Returns a reference to the int-backed enum `System.IO.FileAccess` (a value
        /// type) — paired with [`Self::file_mode`] for the `FileStream` ctor.
        #[must_use]
        file_access => "System.IO.FileAccess", value;
        /// Returns a reference to the int-backed enum `System.IO.SeekOrigin` (a value
        /// type) — for `FileStream.Seek(long, SeekOrigin)`.
        #[must_use]
        seek_origin => "System.IO.SeekOrigin", value;
        /// Returns a reference to the int-backed `[Flags]` enum `System.IO.FileAttributes`
        /// (a value type) — for `File.{Get,Set}Attributes`, backing the dotnet `fs` PAL
        /// `set_perm` (the read-only bit; `ReadOnly = 1`, `Normal = 128`).
        #[must_use]
        file_attributes => "System.IO.FileAttributes", value;
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
        socket => "System.Net.Sockets.Socket", "System.Net.Sockets", class;
        /// Returns a reference to the int-backed enum `System.Net.Sockets.SocketShutdown`
        /// (a value type) — for `Socket.Shutdown(SocketShutdown)` in the dotnet `net`
        /// PAL arm. In `System.Net.Sockets`.
        #[must_use]
        socket_shutdown => "System.Net.Sockets.SocketShutdown", "System.Net.Sockets", value;
        /// Returns a reference to `System.Net.Sockets.SocketException` — the exception
        /// the BCL throws on a socket fault. The POSIX shim's errno translation
        /// (`map_socket_exception_to_errno`) reads its `SocketErrorCode` to derive a
        /// POSIX errno. A reference type (not a value type). In `System.Net.Sockets`
        /// (well, `System.Net.Primitives`, but the exe path resolves the simple-name
        /// extern ref leniently at runtime, exactly like the other net classes).
        #[must_use]
        socket_exception => "System.Net.Sockets.SocketException", "System.Net.Primitives", class;
        /// Returns a reference to `System.IO.FileNotFoundException` — thrown by the
        /// BCL when a file path does not exist (e.g. `new FileStream` on a missing
        /// file). The fs errno mapper (`rcl_errno_from_exception`) maps it to
        /// `ENOENT`. HOST-AGNOSTIC: the exception type is thrown identically on
        /// Unix-host and Windows-host CoreCLR. A reference type, in `System.Runtime`.
        #[must_use]
        file_not_found_exception => "System.IO.FileNotFoundException", class;
        /// Returns a reference to `System.IO.DirectoryNotFoundException` — thrown by
        /// the BCL when a directory in a path does not exist. Maps to `ENOENT`.
        /// HOST-AGNOSTIC. A reference type, in `System.Runtime`.
        #[must_use]
        directory_not_found_exception => "System.IO.DirectoryNotFoundException", class;
        /// Returns a reference to `System.UnauthorizedAccessException` (note: in the
        /// `System` namespace, NOT `System.IO`; it derives from `SystemException`, NOT
        /// `IOException`) — thrown by the BCL on a permission/ACL denial. Maps to
        /// `EACCES`. HOST CAVEAT: the *mapping* is host-agnostic, but the *meaning* of
        /// EACCES (rwx/uid/gid) is only faithful on a Unix host; a Windows-host
        /// CoreCLR throws this for ACL denials too and has no POSIX permission model,
        /// so PermissionDenied fidelity is Unix-host-best-effort. A reference type, in
        /// `System.Runtime`.
        #[must_use]
        unauthorized_access_exception => "System.UnauthorizedAccessException", class;
        /// Returns a reference to `System.IO.PathTooLongException` — thrown by the BCL
        /// when a path exceeds the platform limit. Maps to `ENAMETOOLONG`.
        /// HOST-AGNOSTIC. A reference type, in `System.Runtime`.
        #[must_use]
        path_too_long_exception => "System.IO.PathTooLongException", class;
        /// Returns a reference to the int-backed enum `System.Net.Sockets.SocketError`
        /// (a value type) — the type returned by `SocketException.SocketErrorCode`. The
        /// errno translation reads it (as its underlying i32) to derive a POSIX errno.
        /// Must be the enum type, not raw i32: the CLR matches the property's signature
        /// EXACTLY (`SocketError get_SocketErrorCode()`), so an i32 return type yields a
        /// runtime `MissingMethodException`. In `System.Net.Primitives`.
        #[must_use]
        socket_error => "System.Net.Sockets.SocketError", "System.Net.Primitives", value;
        /// Returns a reference to the int-backed enum `System.Net.Sockets.SelectMode`
        /// (a value type) — selects the readiness mode (SelectRead=0 / SelectWrite=1 /
        /// SelectError=2) for `Socket.Poll(int microSeconds, SelectMode)` in the dotnet
        /// mio PAL arm (the readiness multiplexer behind mio's Selector). In
        /// `System.Net.Sockets`.
        #[must_use]
        select_mode => "System.Net.Sockets.SelectMode", "System.Net.Sockets", value;
        /// Returns a reference to the int-backed enum `System.Net.Sockets.AddressFamily`
        /// (a value type) — selects IPv4/IPv6 for `new Socket(AddressFamily, …)` in the
        /// dotnet `net` PAL arm. In `System.Net.Primitives` (NOT `System.Net.Sockets`).
        #[must_use]
        address_family => "System.Net.Sockets.AddressFamily", "System.Net.Primitives", value;
        /// Returns a reference to the int-backed enum `System.Net.Sockets.SocketType`
        /// (a value type) — Stream/Dgram for `new Socket(…, SocketType, …)`. In
        /// `System.Net.Sockets`.
        #[must_use]
        socket_type => "System.Net.Sockets.SocketType", "System.Net.Sockets", value;
        /// Returns a reference to the int-backed enum `System.Net.Sockets.ProtocolType`
        /// (a value type) — Tcp/Udp for `new Socket(…, …, ProtocolType)`. In
        /// `System.Net.Sockets`.
        #[must_use]
        protocol_type => "System.Net.Sockets.ProtocolType", "System.Net.Sockets", value;
        /// Returns a reference to the class `System.Net.IPAddress` (the IP-address
        /// value carried in an `IPEndPoint`) for the dotnet `net` PAL arm. Built from
        /// network-order octets via `new IPAddress(ReadOnlySpan<byte>)`. In
        /// `System.Net.Primitives`.
        #[must_use]
        ip_address => "System.Net.IPAddress", "System.Net.Primitives", class;
        /// Returns a reference to the class `System.Net.IPEndPoint` (an IPAddress +
        /// port) for the dotnet `net` PAL arm. Never crosses the Rust ABI — it is
        /// built/read entirely BCL-side from the decomposed `(family, ip, port)`. In
        /// `System.Net.Primitives`.
        #[must_use]
        ip_endpoint => "System.Net.IPEndPoint", "System.Net.Primitives", class;
        /// Returns a reference to the abstract base class `System.Net.EndPoint` — the
        /// declared return type of `Socket.LocalEndPoint`/`RemoteEndPoint` and the
        /// `ref` seed type of `Socket.ReceiveFrom`, downcast to `IPEndPoint`
        /// BCL-side in the dotnet `net` PAL arm. In `System.Net.Primitives`.
        #[must_use]
        endpoint => "System.Net.EndPoint", "System.Net.Primitives", class;
        /// Returns a reference to the class
        /// `System.Net.Sockets.UnixDomainSocketEndPoint` — the `EndPoint` subclass
        /// for path-based AF_UNIX sockets (`new UnixDomainSocketEndPoint(string)`),
        /// upcast to `EndPoint` for `Socket.Bind`/`Connect` exactly like `IPEndPoint`
        /// (B2 Piece 1). A reference type. In `System.Net.Sockets` (NOT Primitives).
        #[must_use]
        unix_domain_socket_endpoint => "System.Net.Sockets.UnixDomainSocketEndPoint", "System.Net.Sockets", class;
        /// Returns a reference to the class
        /// `System.Security.Cryptography.RandomNumberGenerator`.
        #[must_use]
        random_number_generator => "System.Security.Cryptography.RandomNumberGenerator", "System.Security.Cryptography", class;
        /// Returns a reference to the class `System.Diagnostics.Stopwatch`, the
        /// monotonic high-resolution timer backing the `Instant` PAL hooks.
        #[must_use]
        stopwatch => "System.Diagnostics.Stopwatch", "System.Runtime.Extensions", class;
        /// Returns a reference to `System.Diagnostics.ProcessStartInfo` — the spawn
        /// recipe (FileName/Arguments/WorkingDirectory/Redirect*) for the dotnet
        /// `process` PAL arm. A reference type in assembly `System.Diagnostics.Process`.
        #[must_use]
        process_start_info => "System.Diagnostics.ProcessStartInfo", "System.Diagnostics.Process", class;
        /// Returns a reference to `System.Diagnostics.Process` — a spawned child
        /// (Start/WaitForExit/ExitCode/Id/Kill/HasExited) for the dotnet `process`
        /// PAL arm. A reference type in assembly `System.Diagnostics.Process`.
        #[must_use]
        process => "System.Diagnostics.Process", "System.Diagnostics.Process", class;
        /// Returns a reference to the abstract class `System.IO.Stream` — the raw byte
        /// stream backing a child's redirected stdout/stderr/stdin (`Read`/`Write`/
        /// `Dispose`) for the dotnet `process` capture path. A reference type.
        #[must_use]
        stream => "System.IO.Stream", class;
        /// Returns a reference to `System.IO.StreamReader` — `Process.StandardOutput`/
        /// `StandardError`; the PAL reads its `BaseStream` for raw child output.
        #[must_use]
        stream_reader => "System.IO.StreamReader", class;
        /// Returns a reference to `System.IO.StreamWriter` — `Process.StandardInput`;
        /// the PAL writes its `BaseStream` for raw child input.
        #[must_use]
        stream_writer => "System.IO.StreamWriter", class;
        /// Returns a reference to the value type `System.DateTime`, the wall-clock
        /// struct backing the `SystemTime` PAL hook.
        #[must_use]
        // value type: instance calls take a managed `this` pointer.
        datetime => "System.DateTime", value;
        /// Returns a reference to the class `System.Numerics.BitOperations`
        #[must_use]
        bit_operations => "System.Numerics.BitOperations", class;
        /// Returns a reference to the class `System.Buffers.Binary.BinaryPrimitives`
        #[must_use]
        binary_primitives => "System.Buffers.Binary.BinaryPrimitives", "System.Memory", class;
        /// Returns a reference to the class `System.MidpointRounding`
        #[must_use]
        midpoint_rounding => "System.MidpointRounding", value;
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
            && self.default_value == other.default_value
            && self.is_const == other.is_const
    }
}
impl RelocateValue for StaticFieldDef {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self {
            tpe,
            name,
            is_tls,
            default_value,
            is_const,
        } = self;
        Self {
            tpe: destination.translate_type(ctx, tpe),
            name: ctx.string(destination, name),
            is_tls,
            default_value: default_value.map(|value| destination.translate_const(ctx, &value)),
            is_const,
        }
    }
}
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct FixedArrayLayout {
    element: Type,
    length: u64,
    requested_size: u64,
    semantic_size: u64,
    requested_align: u64,
}

impl FixedArrayLayout {
    #[must_use]
    pub fn new(
        element: Type,
        length: u64,
        requested_size: u64,
        semantic_size: u64,
        requested_align: u64,
    ) -> Self {
        Self {
            element,
            length,
            requested_size,
            semantic_size,
            requested_align,
        }
    }

    #[must_use]
    pub fn element(&self) -> Type {
        self.element
    }

    #[must_use]
    pub fn requested_align(&self) -> u64 {
        self.requested_align
    }

    #[must_use]
    pub fn semantic_element_stride(&self) -> Option<u64> {
        (self.length != 0 && self.semantic_size % self.length == 0)
            .then_some(self.semantic_size / self.length)
    }
}

impl RelocateValue for FixedArrayLayout {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self::Output {
        Self {
            element: destination.translate_type(ctx, self.element),
            length: self.length,
            requested_size: self.requested_size,
            semantic_size: self.semantic_size,
            requested_align: self.requested_align,
        }
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ClassDef {
    name: Interned<IString>,
    is_valuetype: bool,
    /// Whether `is_valuetype` above has been set by an AUTHORITATIVE comptime entrypoint
    /// (`#[dotnet_class]`'s own `value_type = ...` attribute, or `#[dotnet_interface]`, always
    /// fresh) as opposed to still holding a placeholder from a re-opening `#[dotnet_methods]`
    /// entrypoint (which always registers a FRESH `ClassDef` with `is_valuetype = false` if it
    /// happens to run before the authoritative entrypoint — see `finish_type`'s doc). `false` by
    /// construction (`ClassDef::new` never sets it); [`Self::with_valuetype_authoritative`] flips
    /// it once, and [`Self::set_is_valuetype`] uses it to distinguish "correct a placeholder" from
    /// "assert a real conflict" — see that method's doc, `set_extends`'s sibling for a field a
    /// bare bool can't spell "no opinion" for on its own. NOTE: adding this field changed the
    /// postcard-serialized `.bc` format (the same documented fingerprint trap as `generic_names`/
    /// `properties` above — rebuild dylib+linker together and clean consumers).
    is_valuetype_authoritative: bool,
    generics: u32,
    extends: Option<Interned<ClassRef>>,
    /// `.NET` interfaces this class implements (`implements` clause). Empty for the vast majority of
    /// classes; populated only for Rust-defined managed classes that deliberately implement a managed
    /// interface (see `comptime::finish_type` and `#[dotnet_class(implements(...))]`). The implementing
    /// methods are the ordinary `MethodKind::Virtual` aliases — CLR resolves them by name+signature
    /// (implicit interface implementation), which is why no explicit `.override` is needed.
    implements: Vec<Interned<ClassRef>>,
    fields: Vec<(Type, Interned<IString>, Option<u32>)>,
    static_fields: Vec<StaticFieldDef>,
    methods: Vec<MethodDefIdx>,
    access: Access,
    explict_size: Option<NonZeroU32>,
    align: Option<NonZeroU32>,
    /// `false` for any class using overlapping/union storage (enum variant payloads,
    /// compiler-generated async-fn state machines). `layout_check` rejects a gcref-shaped field
    /// here whenever it would collide (at the same starting offset) with a differently-typed
    /// field from another overlapping variant, since the CLR GC can't consistently interpret that
    /// byte range — see `LayoutError::ManagedRefInOverlapingField` and `layout_check`'s own doc
    /// for exactly what is/isn't allowed (a gcref-shaped field reused identically across
    /// variants is fine and is relied on by real code).
    has_nonveralpping_layout: bool,
    /// Marks this `ClassDef` as a genuine ECMA-335 `interface` `TypeDef` (§II.10.1.3: `Interface`+
    /// `Abstract` flags, no `extends` clause, every member `Abstract`+`Virtual`+`NewSlot`) rather
    /// than an ordinary class — for synthesizing a real C#-consumable interface from a Rust trait.
    /// Every method attached to an interface `ClassDef` must have `MethodDef::is_abstract()` set
    /// (see that method's doc for why abstract-ness is a separate flag rather than folded into
    /// this one — a `ClassDef` doesn't know its own methods' bodies). `false` for every class that
    /// existed before this field: additive, no existing caller sets it. Scoped intentionally
    /// narrow, matching `MethodDef::with_abstract`'s scope — see `docs/MYCORRHIZA_ERGONOMICS_
    /// BACKLOG.md`'s Tier C finding #2 for the full scope discussion (no ctor synthesis, no
    /// default interface methods, no static interface members).
    is_interface: bool,
    /// `.NET` events (ECMA-335 §II.22.13, the `add_*`/`remove_*` shape the C# `event` keyword
    /// compiles to) declared on this class. Empty for the vast majority of classes. See
    /// `EventDef`'s doc for what a single entry needs.
    events: Vec<EventDef>,
    /// The declared names of this type's generic parameters (`T`, `U`, …), in declaration order —
    /// one `GenericParam` row each (§II.22.20) in the PE writer. NON-EMPTY only for a genuine
    /// generic type DEFINITION (today: `#[dotnet_interface] trait IFoo<T>` — a generic
    /// *interface*, which has no layout, so the historical "no explicit layout on .NET generics"
    /// ban does not apply). Must satisfy `generic_names.len() == generics as usize` whenever
    /// non-empty (enforced by [`ClassDef::with_type_generic_names`], the only setter). Empty for
    /// every class that existed before this field: additive, `ClassDef::new` never sets it.
    /// NOTE: adding this field changed the postcard-serialized `.bc` format (the documented
    /// build-std fingerprint trap — rebuild dylib+linker together and clean consumers).
    generic_names: Vec<Interned<IString>>,
    /// `.NET` properties (ECMA-335 §II.22.34 Property + §II.22.35 PropertyMap + §II.22.28
    /// MethodSemantics `Getter`/`Setter` — the shape C#'s `int Volume { get; set; }` compiles
    /// to) declared on this class. Empty for the vast majority of classes; today populated only
    /// for `#[dotnet_property]` members inside a `#[dotnet_interface]` trait. See `PropertyDef`'s
    /// doc for what a single entry needs. NOTE: adding this field changed the
    /// postcard-serialized `.bc` format (the same fingerprint trap as `generic_names` above).
    properties: Vec<PropertyDef>,
    /// General ECMA-335 `CustomAttribute` rows (§II.21/§II.23.3) attached to this TYPE (not its
    /// members — method/param-level attributes are a separate, not-yet-implemented surface).
    /// Populated by `#[dotnet_class(attr(...))]` via `comptime::finish_type`. Empty for every
    /// class that existed before this field: additive, `ClassDef::new` never sets it. See
    /// `CustomAttrDef`'s own doc for the exact argument shapes supported and why a raw-bytes
    /// escape hatch is deliberately not offered. NOTE: adding this field changed the
    /// postcard-serialized `.bc` format (the same fingerprint trap as `generic_names`/
    /// `properties` above — rebuild dylib+linker together and clean consumers).
    custom_attributes: Vec<CustomAttrDef>,
    /// Provenance for a synthetic inline fixed array. This is serialized so layout validation is
    /// performed only after all codegen shards and dependency artifacts have been merged.
    fixed_array_layout: Option<FixedArrayLayout>,
}

/// One constructor-argument or named-argument value inside a `CustomAttribute` blob (§II.23.3).
/// Deliberately restricted to the handful of shapes that are *mechanically* well-formed by
/// construction — no raw-bytes escape hatch: ECMA-335's fixed-arg encoding is one of {primitive,
/// `string`, `Type`, boxed primitive, single-dim array of one of those}; this enum covers the
/// primitive/string subset (the overwhelming majority of real-world attribute usages — MVC route
/// attributes, `JsonPropertyName`, Swashbuckle metadata, …). `Type`/boxed/array/enum arguments are
/// NOT supported yet (a real gap, not a soundness concern: the builder simply can't express them,
/// so there's nothing unsound to reject) — see `CustomAttrDef`'s doc for the full scope note.
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub enum CustomAttrArg {
    /// `ELEMENT_TYPE_STRING` (0x0E): a UTF-8 `SerString` (§II.23.3), never boxed.
    Str(Interned<IString>),
    /// `ELEMENT_TYPE_BOOLEAN` (0x02): one byte, 0/1.
    Bool(bool),
    /// `ELEMENT_TYPE_I4` (0x08): four bytes, little-endian.
    I32(i32),
    /// `ELEMENT_TYPE_I8` (0x0A): eight bytes, little-endian.
    I64(i64),
}

impl RelocateValue for CustomAttrArg {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        match self {
            Self::Str(value) => Self::Str(ctx.string(destination, value)),
            Self::Bool(value) => Self::Bool(value),
            Self::I32(value) => Self::I32(value),
            Self::I64(value) => Self::I64(value),
        }
    }
}

/// One ECMA-335 `CustomAttribute` row (§II.22.10) attached to a type: the attribute TYPE (its
/// `.ctor` is resolved from this at export time via the same `ClassRef`→`TypeRef`/`TypeDef`
/// machinery `extends`/`implements` already use — see `pe_exporter::tables::class_ref_token`),
/// positional constructor arguments (in declaration order, matching the ctor overload this
/// implies), and named PROPERTY arguments (§II.23.3 `NamedArg` — field-targeted named args are
/// not supported, matching how virtually every real .NET attribute exposes its named-arg surface
/// as settable properties, not public fields).
///
/// SAFETY NOTE (this is why the API is safe, not `unsafe`): because every `CustomAttrArg` is one
/// of a small set of mechanically well-formed shapes, [`CustomAttrDef`] can only ever produce a
/// syntactically valid `CustomAttribute` blob — there is no way to construct a malformed one
/// through this type. A malformed attribute TYPE reference (e.g. a typo'd class name) still fails
/// safely: .NET reflection parses `CustomAttribute` blobs LAZILY, so a bad reference surfaces as a
/// catchable `TypeLoadException`/`CustomAttributeFormatException` when a consumer actually asks
/// for the attribute — never a loader-level crash. The one real risk this type does NOT protect
/// against on its own is attaching a *runtime-semantic* attribute (one CoreCLR's own loader/JIT
/// interprets to change layout or calling convention, e.g. `InlineArrayAttribute`,
/// `UnmanagedCallersOnlyAttribute`, `StructLayoutAttribute`) — callers MUST run
/// `pe_exporter::custom_attr_denylist::check` (or the `dotnet_macros` compile-time twin) before
/// constructing one of these from user input; this type itself has no opinion on WHICH attribute
/// type is being attached.
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct CustomAttrDef {
    attr_type: Interned<ClassRef>,
    ctor_args: Vec<CustomAttrArg>,
    named_args: Vec<(Interned<IString>, CustomAttrArg)>,
}
impl RelocateValue for CustomAttrDef {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self {
            attr_type,
            ctor_args,
            named_args,
        } = self;
        Self {
            attr_type: ctx.class_ref(destination, attr_type),
            ctor_args: ctor_args
                .into_iter()
                .map(|arg| arg.relocate(ctx, destination))
                .collect(),
            named_args: named_args
                .into_iter()
                .map(|(name, arg)| {
                    (
                        ctx.string(destination, name),
                        arg.relocate(ctx, destination),
                    )
                })
                .collect(),
        }
    }
}
impl CustomAttrDef {
    #[must_use]
    pub fn new(
        attr_type: Interned<ClassRef>,
        ctor_args: Vec<CustomAttrArg>,
        named_args: Vec<(Interned<IString>, CustomAttrArg)>,
    ) -> Self {
        Self {
            attr_type,
            ctor_args,
            named_args,
        }
    }
    #[must_use]
    pub fn attr_type(&self) -> Interned<ClassRef> {
        self.attr_type
    }
    #[must_use]
    pub fn ctor_args(&self) -> &[CustomAttrArg] {
        &self.ctor_args
    }
    #[must_use]
    pub fn named_args(&self) -> &[(Interned<IString>, CustomAttrArg)] {
        &self.named_args
    }
}
/// One ECMA-335 event (§II.22.13 Event + §II.22.28 MethodSemantics `AddOn`/`RemoveOn`): a name, the
/// delegate type subscribers must match, and the two *ordinary* instance methods (already emitted
/// as regular `MethodDef`s by their owning class — this struct only LINKS their names into the
/// Event-shaped IL block, it introduces no new invocation semantics) that back `+=`/`-=`. The real
/// `add_`/`remove_` method bodies are expected to call `Delegate.Combine`/`Delegate.Remove` on a
/// backing delegate-typed field — exactly the pattern `mycorrhiza::delegate` already proves works
/// as a plain method call.
///
/// Scoped intentionally narrow (see `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md`'s Tier C finding #5):
/// proven for a single delegate-typed field with non-thread-safe `add`/`remove` bodies (no
/// `Interlocked.CompareExchange`-based synchronization — a real C# `event` needs that for
/// concurrent-subscription correctness, which this spike does not attempt) via a hand-written
/// `.il` file assembled with `ilasm` directly (bypassing this struct and `il_exporter` entirely)
/// plus this struct's own `il_exporter` wiring, both hand-verified against a real C# consumer
/// (`+=`/`-=`/multi-subscriber fan-out/`GetEvent` reflection all correct). `pe_exporter` has no
/// EventMap/Event/MethodSemantics table support at all yet — same `DIRECT_PE=1` gap class as
/// virtual overrides and interface export.
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct EventDef {
    name: Interned<IString>,
    delegate: Type,
    add: Interned<MethodRef>,
    remove: Interned<MethodRef>,
}
impl RelocateValue for EventDef {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self {
            name,
            delegate,
            add,
            remove,
        } = self;
        Self {
            name: ctx.string(destination, name),
            delegate: destination.translate_type(ctx, delegate),
            add: ctx.method_ref(destination, add),
            remove: ctx.method_ref(destination, remove),
        }
    }
}
impl EventDef {
    #[must_use]
    pub fn new(
        name: Interned<IString>,
        delegate: Type,
        add: Interned<MethodRef>,
        remove: Interned<MethodRef>,
    ) -> Self {
        Self {
            name,
            delegate,
            add,
            remove,
        }
    }
    #[must_use]
    pub fn name(&self) -> Interned<IString> {
        self.name
    }
    #[must_use]
    pub fn delegate(&self) -> Type {
        self.delegate
    }
    #[must_use]
    pub fn add(&self) -> Interned<MethodRef> {
        self.add
    }
    #[must_use]
    pub fn remove(&self) -> Interned<MethodRef> {
        self.remove
    }
}
/// One ECMA-335 property (§II.22.34 `Property` + §II.22.28 `MethodSemantics` `Getter`(0x2)/
/// `Setter`(0x1)): a name, the property's value type, and up to two *ordinary* accessor methods
/// (already emitted as regular `MethodDef`s by their owning class — this struct only LINKS them
/// into the Property-shaped metadata, it introduces no new invocation semantics). At least one
/// accessor must be present (`new` asserts it): a get-only property has `setter == None`;
/// write-only properties are not representable (rejected upstream at the `#[dotnet_property]`
/// macro level — C# has no idiomatic write-only-property surface worth emitting).
///
/// Scoped intentionally narrow, matching `EventDef`'s scope discipline: non-indexer (`ParamCount
/// == 0` in the §II.23.2.5 `PropertySig`), instance-membered, proven for `#[dotnet_property]`
/// accessors on a `#[dotnet_interface]` trait (abstract `get_X`/`set_X` `MethodDef`s, RVA=0).
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct PropertyDef {
    name: Interned<IString>,
    tpe: Type,
    getter: Option<Interned<MethodRef>>,
    setter: Option<Interned<MethodRef>>,
}
impl RelocateValue for PropertyDef {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self {
            name,
            tpe,
            getter,
            setter,
        } = self;
        Self {
            name: ctx.string(destination, name),
            tpe: destination.translate_type(ctx, tpe),
            getter: getter.map(|method| ctx.method_ref(destination, method)),
            setter: setter.map(|method| ctx.method_ref(destination, method)),
        }
    }
}
impl PropertyDef {
    /// # Panics
    /// If BOTH accessors are `None` — a property with no accessors is malformed metadata
    /// (nothing for `MethodSemantics` to associate), fail at construction rather than emit it.
    #[must_use]
    pub fn new(
        name: Interned<IString>,
        tpe: Type,
        getter: Option<Interned<MethodRef>>,
        setter: Option<Interned<MethodRef>>,
    ) -> Self {
        assert!(
            getter.is_some() || setter.is_some(),
            "PropertyDef::new: a property needs at least one accessor"
        );
        Self {
            name,
            tpe,
            getter,
            setter,
        }
    }
    #[must_use]
    pub fn name(&self) -> Interned<IString> {
        self.name
    }
    #[must_use]
    pub fn tpe(&self) -> Type {
        self.tpe
    }
    #[must_use]
    pub fn getter(&self) -> Option<Interned<MethodRef>> {
        self.getter
    }
    #[must_use]
    pub fn setter(&self) -> Option<Interned<MethodRef>> {
        self.setter
    }
}

/// Relocated class metadata plus the source method-definition ids that must be merged separately.
///
/// Methods are intentionally not installed in `definition`: special initializers can already
/// exist in the destination and retain their established merge semantics in `asm_link`.
pub(crate) struct RelocatedClassDef {
    pub(crate) definition: ClassDef,
    pub(crate) source_methods: Vec<MethodDefIdx>,
}

impl RelocateValue for ClassDef {
    type Output = RelocatedClassDef;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self::Output {
        let Self {
            name,
            is_valuetype,
            is_valuetype_authoritative,
            generics,
            extends,
            implements,
            fields,
            static_fields,
            methods,
            access,
            explict_size,
            align,
            has_nonveralpping_layout,
            is_interface,
            events,
            generic_names,
            properties,
            custom_attributes,
            fixed_array_layout,
        } = self;
        let definition = Self {
            name: ctx.string(destination, name),
            is_valuetype,
            is_valuetype_authoritative,
            generics,
            extends: extends.map(|class| ctx.class_ref(destination, class)),
            implements: implements
                .into_iter()
                .map(|class| ctx.class_ref(destination, class))
                .collect(),
            fields: fields
                .into_iter()
                .map(|(tpe, name, offset)| {
                    (
                        destination.translate_type(ctx, tpe),
                        ctx.string(destination, name),
                        offset,
                    )
                })
                .collect(),
            static_fields: static_fields
                .into_iter()
                .map(|field| field.relocate(ctx, destination))
                .collect(),
            methods: Vec::new(),
            access,
            explict_size,
            align,
            has_nonveralpping_layout,
            is_interface,
            events: events
                .into_iter()
                .map(|event| event.relocate(ctx, destination))
                .collect(),
            generic_names: generic_names
                .into_iter()
                .map(|name| ctx.string(destination, name))
                .collect(),
            properties: properties
                .into_iter()
                .map(|property| property.relocate(ctx, destination))
                .collect(),
            custom_attributes: custom_attributes
                .into_iter()
                .map(|attribute| attribute.relocate(ctx, destination))
                .collect(),
            fixed_array_layout: fixed_array_layout.map(|layout| layout.relocate(ctx, destination)),
        };
        RelocatedClassDef {
            definition,
            source_methods: methods,
        }
    }
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
            // Pull each implemented interface's assembly into the `.assembly extern` table (avoids
            // CS0012 when the interface lives in a third assembly, e.g. a consumer's own library).
            .chain(self.implements.iter().map(|cref| Type::ClassRef(*cref)))
            // Same CS0012-avoidance reasoning for each event's delegate type.
            .chain(self.events.iter().map(EventDef::delegate))
            // And for each property's value type (the `Type` inside the §II.23.2.5 PropertySig).
            .chain(self.properties.iter().map(PropertyDef::tpe))
            // And for each custom attribute's TYPE (pulls a third-party attribute assembly, e.g.
            // `Microsoft.AspNetCore.Mvc`, into `.assembly extern` — same CS0012-avoidance
            // reasoning as `implements`/events/properties above).
            .chain(
                self.custom_attributes
                    .iter()
                    .map(|a| Type::ClassRef(a.attr_type())),
            )
            .chain(
                self.fixed_array_layout
                    .iter()
                    .map(FixedArrayLayout::element),
            )
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
            is_valuetype_authoritative: false,
            generics,
            extends,
            implements: vec![],
            fields,
            static_fields,
            methods: vec![],
            access,
            explict_size,
            align,
            has_nonveralpping_layout,
            is_interface: false,
            events: vec![],
            generic_names: vec![],
            properties: vec![],
            custom_attributes: vec![],
            fixed_array_layout: None,
        }
    }

    /// Marks this definition as the synthetic storage class for one Rust fixed-array request.
    #[must_use]
    pub fn with_fixed_array_layout(mut self, layout: FixedArrayLayout) -> Self {
        assert!(self.is_valuetype, "a fixed array must be a value type");
        assert!(
            self.fixed_array_layout.is_none(),
            "fixed-array layout already set"
        );
        self.fixed_array_layout = Some(layout);
        self
    }

    #[must_use]
    pub fn fixed_array_layout(&self) -> Option<&FixedArrayLayout> {
        self.fixed_array_layout.as_ref()
    }

    /// Marks the `is_valuetype` this `ClassDef` was constructed with as AUTHORITATIVE (see the
    /// `is_valuetype_authoritative` field's doc) — call right after [`Self::new`] when the
    /// constructing comptime entrypoint had a real opinion (`#[dotnet_class]`/
    /// `#[dotnet_interface]`), never for a `#[dotnet_methods]`-style re-opening placeholder.
    #[must_use]
    pub fn with_valuetype_authoritative(mut self) -> Self {
        self.is_valuetype_authoritative = true;
        self
    }

    /// Marks this `ClassDef` as a genuine ECMA-335 `interface` `TypeDef`. See the `is_interface`
    /// field's doc for the exact shape this implies and its scope.
    #[must_use]
    pub fn with_interface(mut self) -> Self {
        self.is_interface = true;
        self
    }

    /// Attaches the declared generic-parameter names of a generic type DEFINITION (see the
    /// `generic_names` field's doc). `names.len()` must equal the `generics` arity this def was
    /// constructed with — a mismatched def is a construction bug, failed loudly here rather than
    /// as malformed `GenericParam` metadata.
    ///
    /// # Panics
    /// If `names.len() != self.generics()`.
    #[must_use]
    pub fn with_type_generic_names(mut self, names: Vec<Interned<IString>>) -> Self {
        assert_eq!(
            names.len(),
            self.generics as usize,
            "ClassDef::with_type_generic_names: {} name(s) for a generic arity of {}",
            names.len(),
            self.generics
        );
        self.generic_names = names;
        self
    }

    /// The declared generic-parameter names of a generic type definition (empty for every
    /// non-generic class — see the `generic_names` field's doc).
    #[must_use]
    pub fn generic_names(&self) -> &[Interned<IString>] {
        &self.generic_names
    }

    #[must_use]
    pub fn is_interface(&self) -> bool {
        self.is_interface
    }

    /// The `.NET` interfaces this class implements (see the `implements` field). Usually empty.
    #[must_use]
    pub fn implements(&self) -> &[Interned<ClassRef>] {
        &self.implements
    }

    /// Declare that this class implements `iface` (append, deduplicating). The class must expose a
    /// public virtual method matching each interface member's name+signature — CLR then binds them
    /// implicitly, so no `.override` is emitted.
    pub fn add_interface(&mut self, iface: Interned<ClassRef>) {
        if !self.implements.contains(&iface) {
            self.implements.push(iface);
        }
    }

    /// The `.NET` events declared on this class (see `EventDef`'s doc). Usually empty.
    #[must_use]
    pub fn events(&self) -> &[EventDef] {
        &self.events
    }

    /// Declare an event on this class. The `add`/`remove` methods it names must already exist as
    /// ordinary `MethodDef`s on this class — this only links their names into the Event-shaped IL
    /// block (see `EventDef`'s doc).
    ///
    /// Deduplicates by name (first-registration wins, silently skipping a later same-named call):
    /// a class can be re-opened by more than one comptime entrypoint (multiple `#[dotnet_methods]`
    /// impl blocks touching the same class), and each entrypoint's `finish_type` call builds its
    /// OWN local `pending_events` map from only ITS OWN methods — it has no visibility into an
    /// event a DIFFERENT entrypoint already registered. Without this dedup, two entrypoints that
    /// happen to declare the same event name (e.g. an authoring mistake, or the same event
    /// re-declared across a split impl) would silently produce TWO `Event` metadata rows with an
    /// identical name under one class — ambiguous ECMA-335 metadata, not a clean error. Mirrors
    /// the dedup `ClassDef::merge_defs` already applies when combining events across SEPARATE
    /// assemblies at link time; this closes the same gap for the same-assembly,
    /// multiple-comptime-entrypoint case.
    pub fn add_event(&mut self, ev: EventDef) {
        if !self
            .events
            .iter()
            .any(|existing| existing.name() == ev.name())
        {
            self.events.push(ev);
        }
    }

    /// The `.NET` properties declared on this class (see `PropertyDef`'s doc). Usually empty.
    #[must_use]
    pub fn properties(&self) -> &[PropertyDef] {
        &self.properties
    }

    /// Declare a property on this class. The accessor methods it names must already exist as
    /// ordinary `MethodDef`s on this class — this only links them into the Property-shaped
    /// metadata (see `PropertyDef`'s doc).
    ///
    /// Deduplicates by name, for the same reason as [`Self::add_event`] (see its doc): a
    /// multiple-comptime-entrypoint class must not accumulate two same-named `Property` rows just
    /// because two entrypoints happened to both declare one.
    pub fn add_property(&mut self, prop: PropertyDef) {
        if !self
            .properties
            .iter()
            .any(|existing| existing.name() == prop.name())
        {
            self.properties.push(prop);
        }
    }

    /// The general `CustomAttribute` rows attached to this TYPE (see `CustomAttrDef`'s doc).
    /// Usually empty.
    #[must_use]
    pub fn custom_attributes(&self) -> &[CustomAttrDef] {
        &self.custom_attributes
    }

    /// Attach a custom attribute to this class. Deduplicates by full equality (a class re-opened
    /// by several comptime entrypoints must not accumulate the identical attribute twice) — two
    /// DIFFERENT attributes of the same TYPE (e.g. two `[Route("...")]` with different literal
    /// args) are both legitimate and both kept.
    pub fn add_custom_attribute(&mut self, attr: CustomAttrDef) {
        if !self.custom_attributes.contains(&attr) {
            self.custom_attributes.push(attr);
        }
    }

    pub(crate) fn ref_to(&self) -> ClassRef {
        // A generic type DEFINITION's canonical identity is its OPEN shape (bare name, no
        // argument list) — exactly what every registration/lookup site (`Assembly::class_def`,
        // `asm_link`) keys on — so the open `ClassRef` below is correct for the generic case
        // too. The invariant enforced here is def-construction consistency, NOT a typechecker
        // rule: a nonzero arity is only valid with matching declared parameter names
        // (`with_type_generic_names`), otherwise the PE writer would emit an arity with no
        // `GenericParam` rows.
        assert!(
            self.generics == 0 || self.generics as usize == self.generic_names.len(),
            "ClassDef::ref_to: generic arity {} but {} declared generic parameter name(s)",
            self.generics,
            self.generic_names.len()
        );
        ClassRef::new(self.name, None, self.is_valuetype, vec![].into())
    }
    /// Rejects GC-ref-*shaped* fields sitting in overlapping (`[FieldOffset]`-style union /
    /// coroutine variant) storage whenever CoreCLR's class loader would reject them too.
    ///
    /// A field "contains a gcref" if [`Type::contains_gcref`] says so — this recurses into
    /// value-type struct fields (unlike the shallow [`Type::is_gcref`]), since a plain-looking
    /// struct field (e.g. `mycorrhiza::task::TaskFuture<T>`) can nest a real managed reference.
    ///
    /// Empirically (`cargo_tests/cd_persisted_async`, proven on real CoreCLR), the CLR does NOT
    /// blanket-reject every gcref in overlapping storage — it only rejects a byte offset whose
    /// *interpretation disagrees* across the overlapping occupants: an object reference in one
    /// coroutine variant and something else (a raw primitive, or a differently-shaped reference)
    /// in another. Roslyn's own async state-machine lowering relies on the same allowance (a
    /// single object-typed slot reused, unambiguously, across suspend points). So this check
    /// groups fields by their starting offset and only rejects a group that mixes a
    /// gcref-containing field with ANY other field whose exact type differs — a solo occupant, or
    /// several occupants that all agree on the exact same type, is left alone.
    ///
    /// This differs from (and supersedes) a blanket "any gcref anywhere in overlapping storage is
    /// illegal" rule: that rule is unsound-by-omission (it happened to pass `TaskFuture<T>` only
    /// because of `is_gcref`'s shallowness, not because the pattern is actually always illegal on
    /// CoreCLR) — seeing the SAME gcref-shaped field reused consistently across variants is a
    /// legitimate, CoreCLR-accepted pattern this project already depends on.
    pub fn layout_check(&self, asm: &Assembly) -> Result<(), LayoutError> {
        if !self.has_nonveralpping_layout() {
            let mut by_offset: std::collections::HashMap<u32, Vec<(&Type, Interned<IString>)>> =
                std::collections::HashMap::new();
            for (t, name, offset) in self.fields() {
                let Some(off) = offset else {
                    // No offset recorded at all (shouldn't happen for a coroutine/enum's
                    // overlapping-storage fields, which always carry one) — fall back to the
                    // original unconditional rejection rather than silently skip the check.
                    if t.contains_gcref(asm) {
                        return Err(LayoutError::ManagedRefInOverlapingField {
                            owner: asm[self.name()].into(),
                            field: t.mangle(asm),
                            name: asm[*name].into(),
                        });
                    }
                    continue;
                };
                by_offset.entry(*off).or_default().push((t, *name));
            }
            for group in by_offset.values() {
                if !group.iter().any(|(t, _)| t.contains_gcref(asm)) {
                    continue;
                }
                let (first_ty, _) = group[0];
                for (t, name) in group {
                    if *t != first_ty {
                        return Err(LayoutError::ManagedRefInOverlapingField {
                            owner: asm[self.name()].into(),
                            field: t.mangle(asm),
                            name: asm[*name].into(),
                        });
                    }
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

    /// Overwrites the base-class reference this `TypeDef` will `extends` (`None` = default to
    /// `System.Object`/`System.ValueType` at export time, see `pe_exporter::export_pe`'s /
    /// `il_exporter`'s identical fallback).
    ///
    /// Needed because a class can be described by MULTIPLE comptime entrypoints (the
    /// `#[dotnet_class]` struct declaration, which knows the real `extends = "..."`, and each
    /// `#[dotnet_methods]` impl block re-opening it, which does NOT — it has no access to the
    /// original struct's attributes and used to hardcode a `System.Object` "superclass" on its
    /// own `rustc_codegen_clr_new_typedef` call). Comptime entrypoint order is NOT guaranteed
    /// (rustc's mono-item collection order), so whichever entrypoint happened to register the
    /// `ClassDef` FIRST used to permanently decide `extends` — if that was a re-opening
    /// `#[dotnet_methods]` block, the class silently got `extends = System.Object` regardless of
    /// what `#[dotnet_class(extends = "...")]` said, an inconsistency invisible in IL text (the
    /// `.override`/base-ctor-chain call sites still correctly named the real base) but fatal at
    /// CLR type-load time: an explicit `.override` `MethodImpl` naming a base method that isn't
    /// anywhere in the ACTUAL (wrongly-Object-rooted) hierarchy makes CoreCLR's
    /// `MethodTableBuilder::FindDeclMethodOnClassInHierarchy` walk off the end and dereference a
    /// null `MethodTable*` — a hard segfault, not a graceful `TypeLoadException`. Confirmed via
    /// `cargo_tests/cd_bgservice/rustlib_bgtest` and a minimal isolate-probe (plain non-abstract
    /// override of a simple user-compiled base with one private field) — BOTH crashed identically
    /// until `finish_type`'s re-opening path started calling this setter.
    pub fn set_extends(&mut self, extends: Option<Interned<ClassRef>>) {
        self.extends = extends;
    }

    /// Overwrites whether this `TypeDef` is a `.NET` value type (struct) or reference type
    /// (class) from an AUTHORITATIVE comptime entrypoint's opinion.
    ///
    /// This is `set_extends`'s sibling, closing the SAME multi-comptime-entrypoint hazard for a
    /// second field: a `#[dotnet_methods]` impl block re-opening a class has no access to the
    /// original struct's `#[dotnet_class(value_type = ...)]` attribute, so it has no real opinion
    /// on `IS_VALUETYPE` — but unlike `extends` (which has a natural "no opinion" value, `None`),
    /// a bare `bool` can't spell that, so a re-opening entrypoint that happens to register the
    /// class FIRST still writes a placeholder `is_valuetype = false` into a genuinely fresh
    /// `ClassDef` (see `finish_type`'s doc). This setter is therefore only ever called by an
    /// AUTHORITATIVE entrypoint, and behaves like `set_extends`'s `None -> Some` correction:
    /// * if no authoritative opinion has been recorded yet (`is_valuetype_authoritative == false`
    ///   — i.e. the current value is just such a placeholder, or this is the type's first real
    ///   opinion), this ADOPTS `new_value` and marks the def authoritative.
    /// * if an authoritative opinion is already recorded, a disagreement is a genuine authoring
    ///   conflict (e.g. two `#[dotnet_class]` declarations of the same name disagreeing on
    ///   `value_type`), not ordering noise — fail loudly rather than silently keep whichever one
    ///   happened to register first, which used to produce a phantom second `ClassDef` under the
    ///   same name at the wrong identity (see `finish_type`'s doc for the exact
    ///   CLR-metadata-corruption shape this closes).
    ///
    /// # Panics
    /// If an authoritative opinion is already recorded and `new_value` disagrees with it.
    pub fn set_is_valuetype(&mut self, new_value: bool) {
        if self.is_valuetype_authoritative {
            assert_eq!(
                self.is_valuetype, new_value,
                "comptime: class {:?} re-declares a different `value_type` across its comptime \
                 entrypoints (`#[dotnet_class]`/`#[dotnet_interface]`) — this is a real conflict, \
                 not codegen-unit ordering noise",
                self.name
            );
        } else {
            self.is_valuetype = new_value;
            self.is_valuetype_authoritative = true;
        }
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
        self.is_valuetype_authoritative |= translated.is_valuetype_authoritative;
        // Check generic count matches
        assert_eq!(self.generics(), translated.generics());
        // Check declared generic-parameter names match (interned in the SAME assembly by the
        // time merge runs — `asm_link::translate_class_def` re-interns them before merging, so
        // a missed translation site fails THIS assert loudly instead of silently dropping rows).
        assert_eq!(self.generic_names(), translated.generic_names());
        // A partial/re-opening comptime entrypoint carries `extends = None` to mean "no opinion";
        // the authoritative `#[dotnet_class]` shard carries the real base. Mono-item/shard order is
        // not stable, so merge this optional metadata symmetrically. Two concrete, different bases
        // are a genuine type-identity conflict and must still fail loudly.
        match (self.extends, translated.extends) {
            (None, Some(incoming)) => self.extends = Some(incoming),
            (Some(existing), Some(incoming)) => assert_eq!(
                existing, incoming,
                "class base differs across codegen shards: class={:?}, existing_base={existing:?}, \
                 incoming_base={incoming:?}",
                self.name
            ),
            (None, None) | (Some(_), None) => {}
        }
        // Check interface-ness matches (a class re-opened by several entrypoints must agree on
        // whether it's a genuine ECMA-335 interface `TypeDef` — see `with_interface`'s doc).
        assert_eq!(self.is_interface(), translated.is_interface());

        // Union the implemented interfaces (a class re-opened by several entrypoints may accumulate
        // its `implements` set across them, exactly like fields/methods).
        for iface in translated.implements() {
            self.add_interface(*iface);
        }

        // Union the declared events, deduplicating by name (same reasoning as `implements` above).
        for ev in translated.events() {
            if !self
                .events
                .iter()
                .any(|existing| existing.name() == ev.name())
            {
                self.events.push(ev.clone());
            }
        }

        // Union the declared properties, deduplicating by name (same reasoning as events above).
        for prop in translated.properties() {
            if !self
                .properties
                .iter()
                .any(|existing| existing.name() == prop.name())
            {
                self.properties.push(prop.clone());
            }
        }

        // Union the custom attributes, deduplicating by full equality (same reasoning as
        // `add_custom_attribute`'s own dedup — see its doc).
        for attr in translated.custom_attributes() {
            self.add_custom_attribute(attr.clone());
        }

        // A codegen shard may first encounter a type only as a method owner and a later shard may
        // carry its actual instance-field definition. Dropping the latter produces a structurally
        // valid TypeDef with methods that reference fields it does not declare (and ultimately a
        // MissingFieldException). Merge instance fields by their semantic identity, while treating
        // a same-named field with a different type or offset as a hard cross-shard inconsistency.
        for field @ (field_tpe, field_name, field_offset) in translated.fields() {
            if let Some(existing) = self
                .fields
                .iter()
                .find(|(_, existing_name, _)| existing_name == field_name)
            {
                assert_eq!(
                    existing, field,
                    "class field differs across codegen shards: name={field_name:?}, \
                     incoming_type={field_tpe:?}, incoming_offset={field_offset:?}"
                );
            } else {
                self.fields.push(*field);
            }
        }

        // Re-opened/partial definitions may omit physical-layout metadata. Preserve the concrete
        // value from whichever shard has it, but never silently reconcile contradictory layouts.
        fn merge_optional_layout<T: Copy + Eq + std::fmt::Debug>(
            current: &mut Option<T>,
            incoming: Option<T>,
            label: &str,
        ) {
            match (*current, incoming) {
                (None, Some(value)) => *current = Some(value),
                (Some(left), Some(right)) => {
                    assert_eq!(left, right, "class {label} differs across codegen shards")
                }
                _ => {}
            }
        }
        merge_optional_layout(&mut self.explict_size, translated.explict_size, "size");
        merge_optional_layout(&mut self.align, translated.align, "alignment");
        match (&self.fixed_array_layout, &translated.fixed_array_layout) {
            (None, Some(incoming)) => self.fixed_array_layout = Some(incoming.clone()),
            (Some(existing), Some(incoming)) => assert_eq!(
                existing, incoming,
                "fixed-array provenance differs across codegen shards"
            ),
            (None, None) | (Some(_), None) => {}
        }
        // `false` is the conservative truth: if any shard describes overlapping storage, the
        // merged class must continue to receive the stricter GC/layout validation.
        self.has_nonveralpping_layout &= translated.has_nonveralpping_layout;

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
fn static_field_default_value_participates_in_equality_and_hash() {
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut asm = Assembly::default();
    let name = asm.alloc_string("VALUE");
    let first = StaticFieldDef {
        tpe: Type::Int(super::Int::I32),
        name,
        is_tls: false,
        default_value: Some(Const::I32(1)),
        is_const: true,
    };
    let same = first.clone();
    let different_default = StaticFieldDef {
        default_value: Some(Const::I32(2)),
        ..first.clone()
    };

    assert_eq!(first, same);
    assert_ne!(first, different_default);

    let hash = |value: &StaticFieldDef| {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(hash(&first), hash(&same));
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
fn merge_defs_preserves_instance_fields_from_partial_shards() {
    let mut asm = Assembly::default();
    let name = asm.alloc_string("ShardOwned");
    let field_name = asm.alloc_string("payload");
    let mut owner_only = ClassDef::new(
        name,
        true,
        0,
        None,
        vec![],
        vec![],
        Access::Public,
        None,
        None,
        true,
    );
    let with_layout = ClassDef::new(
        name,
        true,
        0,
        None,
        vec![(Type::Int(crate::Int::I32), field_name, Some(4))],
        vec![],
        Access::Public,
        NonZeroU32::new(8),
        NonZeroU32::new(4),
        false,
    );

    owner_only.merge_defs(with_layout);

    assert_eq!(
        owner_only.fields(),
        &[(Type::Int(crate::Int::I32), field_name, Some(4))]
    );
    assert_eq!(owner_only.explict_size(), NonZeroU32::new(8));
    assert_eq!(owner_only.align(), NonZeroU32::new(4));
    assert!(!owner_only.has_nonveralpping_layout());
}

#[test]
fn merge_defs_adopts_concrete_base_from_incoming_partial_shard() {
    let mut asm = Assembly::default();
    let name = asm.alloc_string("ShardBaseAdoption");
    let base_name = asm.alloc_string("ManagedBase");
    let base = asm.alloc_class_ref(ClassRef::new(base_name, None, false, [].into()));
    let mut no_opinion = ClassDef::new(
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
    let authoritative = ClassDef::new(
        name,
        false,
        0,
        Some(base),
        vec![],
        vec![],
        Access::Public,
        None,
        None,
        true,
    );

    no_opinion.merge_defs(authoritative);

    assert_eq!(no_opinion.extends(), Some(base));
}

#[test]
fn merge_defs_preserves_concrete_base_when_incoming_shard_has_no_opinion() {
    let mut asm = Assembly::default();
    let name = asm.alloc_string("ShardBasePreservation");
    let base_name = asm.alloc_string("ManagedBase");
    let base = asm.alloc_class_ref(ClassRef::new(base_name, None, false, [].into()));
    let mut authoritative = ClassDef::new(
        name,
        false,
        0,
        Some(base),
        vec![],
        vec![],
        Access::Public,
        None,
        None,
        true,
    );
    let no_opinion = ClassDef::new(
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

    authoritative.merge_defs(no_opinion);

    assert_eq!(authoritative.extends(), Some(base));
}

#[test]
#[should_panic(expected = "class base differs across codegen shards")]
fn merge_defs_rejects_conflicting_concrete_bases() {
    let mut asm = Assembly::default();
    let name = asm.alloc_string("ShardBaseConflict");
    let left_name = asm.alloc_string("LeftBase");
    let right_name = asm.alloc_string("RightBase");
    let left_base = asm.alloc_class_ref(ClassRef::new(left_name, None, false, [].into()));
    let right_base = asm.alloc_class_ref(ClassRef::new(right_name, None, false, [].into()));
    let mut left = ClassDef::new(
        name,
        false,
        0,
        Some(left_base),
        vec![],
        vec![],
        Access::Public,
        None,
        None,
        true,
    );
    let right = ClassDef::new(
        name,
        false,
        0,
        Some(right_base),
        vec![],
        vec![],
        Access::Public,
        None,
        None,
        true,
    );

    left.merge_defs(right);
}

#[test]
fn fixed_array_identity_includes_physical_storage_layout() {
    let mut asm = Assembly::default();
    let element = Type::Int(crate::Int::U32);
    let ordinary = ClassRef::fixed_array_with_layout(element, 32, 128, 4, &mut asm);
    let ordinary_again = ClassRef::fixed_array_with_layout(element, 32, 128, 4, &mut asm);
    let over_aligned = ClassRef::fixed_array_with_layout(element, 32, 128, 128, &mut asm);

    assert_eq!(ordinary, ordinary_again);
    assert_ne!(ordinary, over_aligned);
    assert_ne!(asm[ordinary].name(), asm[over_aligned].name());
}

#[test]
#[should_panic(expected = "class field differs across codegen shards")]
fn merge_defs_rejects_conflicting_instance_fields() {
    let mut asm = Assembly::default();
    let name = asm.alloc_string("ShardConflict");
    let field_name = asm.alloc_string("payload");
    let mut left = ClassDef::new(
        name,
        true,
        0,
        None,
        vec![(Type::Int(crate::Int::I32), field_name, Some(0))],
        vec![],
        Access::Public,
        NonZeroU32::new(4),
        NonZeroU32::new(4),
        true,
    );
    let right = ClassDef::new(
        name,
        true,
        0,
        None,
        vec![(Type::Int(crate::Int::I64), field_name, Some(0))],
        vec![],
        Access::Public,
        NonZeroU32::new(4),
        NonZeroU32::new(4),
        true,
    );

    left.merge_defs(right);
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
fn implements_roundtrip() {
    let mut asm = Assembly::default();
    let name: Interned<IString> = asm.alloc_string("Impl");
    let iface_name = asm.alloc_string("Some.IFace");
    let iface_asm = asm.alloc_string("SomeLib");
    let iface = asm.alloc_class_ref(ClassRef::new(iface_name, Some(iface_asm), false, [].into()));
    let mut def = ClassDef::new(
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
    def.add_interface(iface);
    def.add_interface(iface); // dedup
    assert_eq!(def.implements(), &[iface]);
    // The interface is pulled into iter_types (so its assembly lands in `.assembly extern`).
    assert_eq!(def.iter_types().count(), 1);
    // Exact postcard round-trip: the serialized `.bc` shape must survive dylib->linker unchanged.
    let bytes = postcard::to_allocvec(&def).unwrap();
    let back: ClassDef = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(def, back);
    assert_eq!(back.implements(), &[iface]);
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
