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
//   * RustHashMap<K,V> where K,V : unmanaged — a size-erased Rust HashMap keyed by the raw key bytes
//                                         (needs `export_rust_hashmap!()` in the Rust cdylib).
//   * RustString                        — a mutable, Rust-owned UTF-8 buffer that marshals to/from a
//                                         managed string (needs `export_rust_string!()`).
//
// All are thin, move-only handles to a Rust-owned allocation; call Dispose() (or use `using`) to
// free it (and, for RustBoxVec, to release the element GCHandles).
//
// The Rust cores are `MainModule.rcl_vec_*` / `rcl_map_*` / `rcl_str_*` — flat `#[no_mangle]` symbols
// on the consuming crate's module. `global::MainModule` names them regardless of this file's
// namespace. A wrapper here compiles only if the matching `export_rust_*!()` macro was invoked in the
// Rust cdylib; otherwise Roslyn reports the unresolved MainModule member (a clear signal).
//
// Each wrapper is guarded by a preprocessor symbol that RustDotnet.targets defines from the opt-in
// props, so a project that exports only some of the cores never compiles a reference to a core it
// lacks:  <UseRustDotnetContainers> -> RUSTDOTNET_VEC (RustVec/RustBoxVec),
//         <UseRustDotnetHashMap>    -> RUSTDOTNET_HASHMAP (RustHashMap),
//         <UseRustDotnetString>     -> RUSTDOTNET_STRING (RustString).

using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace RustDotnet
{
#if RUSTDOTNET_VEC
    /// <summary>
    /// A growable list of unmanaged <typeparamref name="T"/>, backed by a single size-erased Rust
    /// vector. Near-zero-cost: each element is memcpy'd to/from the Rust buffer by its raw bytes.
    /// </summary>
    public unsafe struct RustVec<T> : IDisposable, IEnumerable<T> where T : unmanaged
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

        /// <summary>
        /// Enumerate the elements by index (each <c>Get</c> re-reads the Rust-owned buffer, so this
        /// is safe against growth via <c>Push</c> made *before* the enumerator was created, but — like
        /// <c>List&lt;T&gt;</c> — mutating the count mid-iteration is not a supported pattern).
        /// A struct enumerator, so a plain `foreach` over a `RustVec&lt;T&gt;` allocates nothing.
        /// </summary>
        public Enumerator GetEnumerator() => new Enumerator(this);

        IEnumerator<T> IEnumerable<T>.GetEnumerator() => GetEnumerator();

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        /// <summary>Allocation-free struct enumerator for <see cref="RustVec{T}"/>.</summary>
        public struct Enumerator : IEnumerator<T>
        {
            private RustVec<T> _vec;
            private int _index;
            private T _current;

            internal Enumerator(RustVec<T> vec)
            {
                _vec = vec;
                _index = -1;
                _current = default;
            }

            public T Current => _current;

            object IEnumerator.Current => _current;

            public bool MoveNext()
            {
                int next = _index + 1;
                if (next >= _vec.Count)
                    return false;
                _current = _vec.Get(next);
                _index = next;
                return true;
            }

            public void Reset()
            {
                _index = -1;
                _current = default;
            }

            public void Dispose() { }
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
    public unsafe struct RustBoxVec<T> : IDisposable, IEnumerable<T>
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

        /// <summary>
        /// Enumerate the elements by index (each step resolves the element's <see cref="GCHandle"/>
        /// back to its managed object). A struct enumerator, so a plain `foreach` allocates nothing
        /// beyond what <see cref="Get"/> itself does.
        /// </summary>
        public Enumerator GetEnumerator() => new Enumerator(this);

        IEnumerator<T> IEnumerable<T>.GetEnumerator() => GetEnumerator();

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        /// <summary>Allocation-free struct enumerator for <see cref="RustBoxVec{T}"/>.</summary>
        public struct Enumerator : IEnumerator<T>
        {
            private RustBoxVec<T> _vec;
            private int _index;
            private T _current;

            internal Enumerator(RustBoxVec<T> vec)
            {
                _vec = vec;
                _index = -1;
                _current = default;
            }

            public T Current => _current;

            object IEnumerator.Current => _current;

            public bool MoveNext()
            {
                int next = _index + 1;
                if (next >= _vec.Count)
                    return false;
                _current = _vec.Get(next);
                _index = next;
                return true;
            }

            public void Reset()
            {
                _index = -1;
                _current = default;
            }

            public void Dispose() { }
        }
    }
#endif // RUSTDOTNET_VEC

#if RUSTDOTNET_HASHMAP
    /// <summary>
    /// A hash map from unmanaged <typeparamref name="K"/> to unmanaged <typeparamref name="V"/>,
    /// backed by a size-erased Rust <c>HashMap</c> keyed by the raw key bytes. Near-zero-cost: keys
    /// and values are memcpy'd to/from the Rust side by their raw bytes, so a single Rust
    /// monomorphization backs every <c>K</c>/<c>V</c> instantiation. Requires
    /// <c>mycorrhiza::export_rust_hashmap!()</c> in the consuming Rust cdylib.
    /// </summary>
    public unsafe struct RustHashMap<K, V> : IDisposable
        where K : unmanaged
        where V : unmanaged
    {
        private nuint _handle;

        /// <summary>Create an empty <c>RustHashMap&lt;K, V&gt;</c>.</summary>
        public static RustHashMap<K, V> New() =>
            new RustHashMap<K, V>
            {
                _handle = global::MainModule.rcl_map_new((nuint)sizeof(K), (nuint)sizeof(V))
            };

        /// <summary>Number of entries.</summary>
        public readonly int Count => (int)global::MainModule.rcl_map_len(_handle);

        /// <summary>Insert or overwrite; returns <c>true</c> if a previous value was replaced.</summary>
        public readonly bool Insert(K key, V value) =>
            global::MainModule.rcl_map_insert(_handle, (byte*)&key, (byte*)&value);

        /// <summary>Get the value for <paramref name="key"/> into <paramref name="value"/>; returns
        /// <c>false</c> (leaving <paramref name="value"/> default) if the key is absent.</summary>
        public readonly bool TryGetValue(K key, out V value)
        {
            V v = default;
            bool found = global::MainModule.rcl_map_get(_handle, (byte*)&key, (byte*)&v);
            value = v;
            return found;
        }

        /// <summary>The value for <paramref name="key"/>; throws if absent.</summary>
        public readonly V this[K key]
        {
            get
            {
                V v = default;
                if (!global::MainModule.rcl_map_get(_handle, (byte*)&key, (byte*)&v))
                    throw new System.Collections.Generic.KeyNotFoundException();
                return v;
            }
            set => global::MainModule.rcl_map_insert(_handle, (byte*)&key, (byte*)&value);
        }

        /// <summary><c>true</c> if <paramref name="key"/> is present.</summary>
        public readonly bool ContainsKey(K key) =>
            global::MainModule.rcl_map_contains(_handle, (byte*)&key);

        /// <summary>Remove <paramref name="key"/>; returns <c>true</c> if it was present.</summary>
        public readonly bool Remove(K key) =>
            global::MainModule.rcl_map_remove(_handle, (byte*)&key);

        /// <summary>Free the Rust-owned allocation. The handle is invalid afterwards.</summary>
        public void Dispose()
        {
            if (_handle != 0)
            {
                global::MainModule.rcl_map_free(_handle);
                _handle = 0;
            }
        }
    }
#endif // RUSTDOTNET_HASHMAP

#if RUSTDOTNET_STRING
    /// <summary>
    /// A mutable, growable string owned by Rust (a UTF-8 byte buffer). Text crosses the seam as UTF-8:
    /// <see cref="Append(string)"/> encodes with <see cref="Encoding.UTF8"/>, and
    /// <see cref="ToString"/> decodes the whole buffer back. <see cref="Length"/> is a UTF-8 <b>byte</b>
    /// count (not a UTF-16 char count). Requires <c>mycorrhiza::export_rust_string!()</c> in the
    /// consuming Rust cdylib.
    /// </summary>
    public unsafe struct RustString : IDisposable
    {
        private nuint _handle;

        /// <summary>Create an empty <c>RustString</c>.</summary>
        public static RustString New() =>
            new RustString { _handle = global::MainModule.rcl_str_new() };

        /// <summary>Create a <c>RustString</c> initialised from <paramref name="s"/>.</summary>
        public static RustString From(string s)
        {
            var rs = New();
            rs.Append(s);
            return rs;
        }

        /// <summary>Length in UTF-8 <b>bytes</b>.</summary>
        public readonly int Length => (int)global::MainModule.rcl_str_len(_handle);

        /// <summary>Append <paramref name="s"/> (encoded as UTF-8). A null/empty string is a no-op.</summary>
        public readonly void Append(string s)
        {
            if (string.IsNullOrEmpty(s))
                return;
            byte[] bytes = Encoding.UTF8.GetBytes(s);
            fixed (byte* p = bytes)
                global::MainModule.rcl_str_push_bytes(_handle, p, (nuint)bytes.Length);
        }

        /// <summary>Truncate to empty (keeps the backing allocation).</summary>
        public readonly void Clear() => global::MainModule.rcl_str_clear(_handle);

        /// <summary>Decode the whole Rust-owned buffer back into a managed UTF-8 string.</summary>
        public override readonly string ToString()
        {
            int n = Length;
            if (n == 0)
                return string.Empty;
            byte[] bytes = new byte[n];
            fixed (byte* p = bytes)
                global::MainModule.rcl_str_copy_to(_handle, p);
            return Encoding.UTF8.GetString(bytes);
        }

        /// <summary>Free the Rust-owned allocation. The handle is invalid afterwards.</summary>
        public void Dispose()
        {
            if (_handle != 0)
            {
                global::MainModule.rcl_str_free(_handle);
                _handle = 0;
            }
        }
    }
#endif // RUSTDOTNET_STRING
}
