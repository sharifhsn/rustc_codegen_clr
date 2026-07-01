// RustDotnet.Containers.cs — the C# half of the reusable C#→Rust generic-container bridge.
//
// Auto-included in a C# project that imports RustDotnet.targets and sets
// <UseRustDotnetContainers>true</UseRustDotnetContainers>. It provides two generic list wrappers over
// a single size-erased Rust core (emitted into your Rust cdylib's `MainModule` by
// `mycorrhiza::export_rust_containers!()`):
//
//   * RustVec<T> where T : unmanaged   — near-zero-cost, stores raw `T` bytes via memcpy.
//   * RustBoxVec<T>                     — works for ANY T (managed classes, arrays, structs holding
//                                         references) by storing a GCHandle per element; reference
//                                         identity is preserved.
//
// Both are thin, move-only handles to a Rust-owned allocation; call Dispose() (or use `using`) to
// free it (and, for RustBoxVec, to release the element GCHandles).
//
// The Rust core is `MainModule.rcl_vec_{new,push,get,set,len,free}` — flat `#[no_mangle]` symbols on
// the consuming crate's module. `global::MainModule` names them regardless of this file's namespace.

using System;
using System.Runtime.InteropServices;

namespace RustDotnet
{
    /// <summary>
    /// A growable list of unmanaged <typeparamref name="T"/>, backed by a single size-erased Rust
    /// vector. Near-zero-cost: each element is memcpy'd to/from the Rust buffer by its raw bytes.
    /// </summary>
    public unsafe struct RustVec<T> : IDisposable where T : unmanaged
    {
        private nuint _handle;

        /// <summary>Create an empty <c>RustVec&lt;T&gt;</c>.</summary>
        public static RustVec<T> New() =>
            new RustVec<T> { _handle = global::MainModule.rcl_vec_new((nuint)sizeof(T)) };

        /// <summary>Number of elements.</summary>
        public int Count => (int)global::MainModule.rcl_vec_len(_handle);

        /// <summary>Append <paramref name="value"/>.</summary>
        public void Push(T value) => global::MainModule.rcl_vec_push(_handle, (byte*)&value);

        /// <summary>The element at <paramref name="idx"/>; throws if out of range.</summary>
        public T Get(int idx)
        {
            T v = default;
            if (!global::MainModule.rcl_vec_get(_handle, (nuint)idx, (byte*)&v))
                throw new IndexOutOfRangeException();
            return v;
        }

        /// <summary>Overwrite the element at <paramref name="idx"/>; throws if out of range.</summary>
        public void Set(int idx, T value)
        {
            if (!global::MainModule.rcl_vec_set(_handle, (nuint)idx, (byte*)&value))
                throw new IndexOutOfRangeException();
        }

        /// <summary>Free the Rust-owned allocation. The handle is invalid afterwards.</summary>
        public void Dispose()
        {
            if (_handle != 0)
            {
                global::MainModule.rcl_vec_free(_handle);
                _handle = 0;
            }
        }
    }

    /// <summary>
    /// A growable list of ANY <typeparamref name="T"/> (managed reference types, arrays, structs
    /// holding references), backed by the same size-erased Rust vector — but each element is a
    /// <see cref="GCHandle"/> rooting the managed object, and only the pointer-sized handle is stored
    /// on the Rust side, so Rust never sees the object. Reference identity is preserved:
    /// <see cref="Get"/> returns the very object that was <see cref="Push"/>ed. Costs a box + GC root
    /// per element (vs <see cref="RustVec{T}"/>'s raw byte copy).
    /// </summary>
    public unsafe struct RustBoxVec<T> : IDisposable
    {
        private nuint _handle;

        /// <summary>Create an empty <c>RustBoxVec&lt;T&gt;</c>.</summary>
        public static RustBoxVec<T> New() =>
            new RustBoxVec<T> { _handle = global::MainModule.rcl_vec_new((nuint)IntPtr.Size) };

        /// <summary>Number of elements.</summary>
        public int Count => (int)global::MainModule.rcl_vec_len(_handle);

        /// <summary>Append <paramref name="value"/> (roots it with a strong GCHandle).</summary>
        public void Push(T value)
        {
            GCHandle gh = GCHandle.Alloc(value);
            IntPtr p = GCHandle.ToIntPtr(gh);
            global::MainModule.rcl_vec_push(_handle, (byte*)&p);
        }

        /// <summary>The element at <paramref name="idx"/>; throws if out of range.</summary>
        public T Get(int idx)
        {
            IntPtr p = default;
            if (!global::MainModule.rcl_vec_get(_handle, (nuint)idx, (byte*)&p))
                throw new IndexOutOfRangeException();
            return (T)GCHandle.FromIntPtr(p).Target;
        }

        /// <summary>Overwrite the element at <paramref name="idx"/> (frees the replaced root); throws if out of range.</summary>
        public void Set(int idx, T value)
        {
            IntPtr old = default;
            if (!global::MainModule.rcl_vec_get(_handle, (nuint)idx, (byte*)&old))
                throw new IndexOutOfRangeException();
            GCHandle.FromIntPtr(old).Free();
            GCHandle gh = GCHandle.Alloc(value);
            IntPtr p = GCHandle.ToIntPtr(gh);
            global::MainModule.rcl_vec_set(_handle, (nuint)idx, (byte*)&p);
        }

        /// <summary>Free every element's GCHandle, then the Rust-owned allocation.</summary>
        public void Dispose()
        {
            if (_handle != 0)
            {
                int n = Count;
                for (int i = 0; i < n; i++)
                {
                    IntPtr p = default;
                    global::MainModule.rcl_vec_get(_handle, (nuint)i, (byte*)&p);
                    GCHandle.FromIntPtr(p).Free();
                }
                global::MainModule.rcl_vec_free(_handle);
                _handle = 0;
            }
        }
    }
}
