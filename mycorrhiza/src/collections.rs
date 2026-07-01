//! Ready-to-use, idiomatic wrappers over the common .NET generic collections
//! (`System.Collections.Generic`), backed by real managed objects on the CLR heap.
//!
//! Use them like `std` — no knowledge of the CLR generic-interop machinery (`get_Item`, the `!0`
//! definition-shape signatures, `callvirt`) is required. That machinery lives in [`crate::generic_bridge`]
//! and is applied once, here, per collection:
//!
//! ```ignore
//! use mycorrhiza::collections::List;
//!
//! let mut xs = List::<i32>::new();
//! xs.push(10);
//! xs.push(20);
//! assert_eq!(xs.len(), 2);
//! assert_eq!(xs.get(0), Some(10));
//! for x in xs.iter() { /* … */ }
//! ```
//!
//! **Element types.** `T` (and `K`/`V`) must be a type that crosses the managed boundary: a .NET
//! primitive (`i32`/`i64`/`f64`/`bool`/…), a `#[repr(C)]` value-type struct of such, or a managed
//! handle (`RustcCLRInteropManagedClass`/`…Generic`). A Rust `String`/`Vec`/`&str` is **not** a .NET
//! type — marshal it first. There is no compile-time bound (the backend validates at codegen), so an
//! unmarshalable `T` is a build error, not a silent failure.
//!
//! **Reference semantics.** Each wrapper is a thin handle to a managed object; it is move-only, so you
//! don't accidentally alias. `.handle()` exposes the raw [`crate::intrinsics::RustcCLRInteropManagedGeneric`]
//! for advanced interop. There is no `Drop` — the .NET GC owns the object.

/// `System.Collections.Generic.List<T>` — a growable, index-addressable managed list.
pub use self::list::{List, ListIter};
/// `System.Collections.Generic.Dictionary<K, V>` — a managed hash map.
pub use self::dictionary::Dictionary;
/// `System.Collections.Generic.HashSet<T>` — a managed hash set.
pub use self::hash_set::HashSet;
/// `System.Collections.Generic.Stack<T>` — a managed LIFO stack.
pub use self::stack::Stack;
/// `System.Collections.Generic.Queue<T>` — a managed FIFO queue.
pub use self::queue::Queue;

// The core BCL collections all live in the implementation assembly `System.Private.CoreLib` — a
// reference assembly forwards the type and throws `TypeLoadException` at JIT, so method-body refs must
// name the impl assembly (the same rule the generic bridge documents).
const CORELIB: &str = "System.Private.CoreLib";

mod list {
    use super::CORELIB;
    use crate::{dotnet_generic, dotnet_generic_impl, gen};

    dotnet_generic!(Handle<T> = [CORELIB] "System.Collections.Generic.List" < (T,) >);
    dotnet_generic_impl! {
        Handle<T> = [CORELIB] "System.Collections.Generic.List" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_add       = "Add"(r, item: T as gen!(0));
        fn raw_get       = "get_Item"(r, idx: i32 as i32) -> T as gen!(0);
        fn raw_set       = "set_Item"(r, idx: i32 as i32, item: T as gen!(0));
        fn raw_count     = "get_Count"(r) -> i32 as i32;
        fn raw_contains  = "Contains"(r, item: T as gen!(0)) -> bool as bool;
        fn raw_index_of  = "IndexOf"(r, item: T as gen!(0)) -> i32 as i32;
        fn raw_remove_at = "RemoveAt"(r, idx: i32 as i32);
        fn raw_insert    = "Insert"(r, idx: i32 as i32, item: T as gen!(0));
        fn raw_clear     = "Clear"(r);
        fn raw_sort      = "Sort"(r);
        fn raw_reverse   = "Reverse"(r);
    }

    /// A managed `System.Collections.Generic.List<T>`. See the [module docs](super).
    pub struct List<T> {
        h: Handle<T>,
    }

    impl<T> List<T> {
        /// `new List<T>()`.
        pub fn new() -> Self {
            Self { h: raw_ctor::<T>() }
        }
        /// Number of elements (`Count`).
        pub fn len(&self) -> i32 {
            raw_count::<T>(self.h)
        }
        /// `true` if empty.
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
        /// Append `item` (`Add`).
        pub fn push(&mut self, item: T) {
            raw_add::<T>(self.h, item)
        }
        /// The element at `idx`, or `None` if out of range (bounds-checked, like `Vec::get`).
        pub fn get(&self, idx: i32) -> Option<T> {
            if idx >= 0 && idx < self.len() {
                Some(raw_get::<T>(self.h, idx))
            } else {
                None
            }
        }
        /// Overwrite the element at `idx`; returns `false` (no write) if out of range.
        pub fn set(&mut self, idx: i32, item: T) -> bool {
            if idx >= 0 && idx < self.len() {
                raw_set::<T>(self.h, idx, item);
                true
            } else {
                false
            }
        }
        /// Insert `item` at `idx`, shifting later elements right (bounds-checked; `false` if `idx > len`).
        pub fn insert(&mut self, idx: i32, item: T) -> bool {
            if idx >= 0 && idx <= self.len() {
                raw_insert::<T>(self.h, idx, item);
                true
            } else {
                false
            }
        }
        /// Remove the element at `idx`, shifting later elements left; `false` if out of range.
        pub fn remove_at(&mut self, idx: i32) -> bool {
            if idx >= 0 && idx < self.len() {
                raw_remove_at::<T>(self.h, idx);
                true
            } else {
                false
            }
        }
        /// Whether `item` is present (`Contains`, `.NET` equality).
        pub fn contains(&self, item: T) -> bool {
            raw_contains::<T>(self.h, item)
        }
        /// Index of the first occurrence of `item`, or `-1` (`IndexOf`).
        pub fn index_of(&self, item: T) -> i32 {
            raw_index_of::<T>(self.h, item)
        }
        /// Remove all elements (`Clear`).
        pub fn clear(&mut self) {
            raw_clear::<T>(self.h)
        }
        /// The first element, or `None` if empty (like `slice::first`, by value).
        pub fn first(&self) -> Option<T> {
            self.get(0)
        }
        /// The last element, or `None` if empty (like `slice::last`, by value).
        pub fn last(&self) -> Option<T> {
            self.get(self.len() - 1)
        }
        /// Remove and return the last element, or `None` if empty (like `Vec::pop`).
        pub fn pop(&mut self) -> Option<T> {
            let last = self.len() - 1;
            if last >= 0 {
                let v = raw_get::<T>(self.h, last);
                raw_remove_at::<T>(self.h, last);
                Some(v)
            } else {
                None
            }
        }
        /// Sort in place using the default .NET comparer (`List<T>.Sort()`; ascending for the
        /// numeric primitives). The element type must be `.NET`-comparable or this throws at runtime.
        pub fn sort(&mut self) {
            raw_sort::<T>(self.h)
        }
        /// Reverse the elements in place (`List<T>.Reverse()`).
        pub fn reverse(&mut self) {
            raw_reverse::<T>(self.h)
        }
        /// Copy the elements out into a Rust [`Vec`] (by value, in order).
        pub fn to_vec(&self) -> std::vec::Vec<T> {
            let n = self.len();
            let mut v = std::vec::Vec::with_capacity(n.max(0) as usize);
            let mut i = 0;
            while i < n {
                v.push(raw_get::<T>(self.h, i));
                i += 1;
            }
            v
        }
        /// Build a `List<T>` from a slice, copying each element (`T: Copy`).
        pub fn from_slice(items: &[T]) -> Self
        where
            T: Copy,
        {
            let mut l = Self::new();
            for &item in items {
                l.push(item);
            }
            l
        }
        /// Iterate the elements by value (index-based; the list must not be mutated during iteration).
        pub fn iter(&self) -> ListIter<T> {
            ListIter {
                h: self.h,
                idx: 0,
                len: self.len(),
            }
        }
        /// The raw managed handle, for advanced interop.
        pub fn handle(&self) -> Handle<T> {
            self.h
        }
    }

    impl<T> Default for List<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    // Deep clone: a fresh managed `List<T>` with the elements copied over (`T: Copy`, so the copy is
    // element-wise and the two lists are independent — mutating one does not affect the other).
    impl<T: Copy> Clone for List<T> {
        fn clone(&self) -> Self {
            let mut out = Self::new();
            let n = self.len();
            let mut i = 0;
            while i < n {
                out.push(raw_get::<T>(self.h, i));
                i += 1;
            }
            out
        }
    }

    // Element-wise equality, computed in Rust (NOT reference identity — `List<T>` inherits `object`'s
    // reference `Equals`, which would compare handles, so we compare lengths + elements ourselves to
    // match what a Rust user means by `==`).
    impl<T: Copy + PartialEq> PartialEq for List<T> {
        fn eq(&self, other: &Self) -> bool {
            let n = self.len();
            if n != other.len() {
                return false;
            }
            let mut i = 0;
            while i < n {
                if raw_get::<T>(self.h, i) != raw_get::<T>(other.h, i) {
                    return false;
                }
                i += 1;
            }
            true
        }
    }
    impl<T: Copy + Eq> Eq for List<T> {}

    // Hash the elements in order, consistent with the element-wise `PartialEq` above.
    impl<T: Copy + core::hash::Hash> core::hash::Hash for List<T> {
        fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
            let n = self.len();
            state.write_i32(n);
            let mut i = 0;
            while i < n {
                raw_get::<T>(self.h, i).hash(state);
                i += 1;
            }
        }
    }

    impl<T: Copy> From<std::vec::Vec<T>> for List<T> {
        fn from(v: std::vec::Vec<T>) -> Self {
            let mut l = Self::new();
            for item in v {
                l.push(item);
            }
            l
        }
    }

    impl<T> FromIterator<T> for List<T> {
        fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
            let mut l = Self::new();
            for item in iter {
                l.push(item);
            }
            l
        }
    }

    impl<T> Extend<T> for List<T> {
        fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
            for item in iter {
                self.push(item);
            }
        }
    }

    /// By-value iterator over a [`List`] (see [`List::iter`]).
    pub struct ListIter<T> {
        h: Handle<T>,
        idx: i32,
        len: i32,
    }

    impl<T> Iterator for ListIter<T> {
        type Item = T;
        fn next(&mut self) -> Option<T> {
            if self.idx < self.len {
                let v = raw_get::<T>(self.h, self.idx);
                self.idx += 1;
                Some(v)
            } else {
                None
            }
        }
        fn size_hint(&self) -> (usize, Option<usize>) {
            let rem = (self.len - self.idx).max(0) as usize;
            (rem, Some(rem))
        }
    }
}

mod dictionary {
    use super::CORELIB;
    use crate::{dotnet_generic, dotnet_generic_impl, gen};

    dotnet_generic!(Handle<K, V> = [CORELIB] "System.Collections.Generic.Dictionary" < (K, V) >);
    dotnet_generic_impl! {
        Handle<K, V> = [CORELIB] "System.Collections.Generic.Dictionary" < (K, V) > ;
        ctor fn raw_ctor();
        fn raw_set       = "set_Item"(r, key: K as gen!(0), value: V as gen!(1));
        fn raw_get       = "get_Item"(r, key: K as gen!(0)) -> V as gen!(1);
        fn raw_contains  = "ContainsKey"(r, key: K as gen!(0)) -> bool as bool;
        fn raw_remove    = "Remove"(r, key: K as gen!(0)) -> bool as bool;
        fn raw_count     = "get_Count"(r) -> i32 as i32;
        fn raw_clear     = "Clear"(r);
    }

    /// A managed `System.Collections.Generic.Dictionary<K, V>`. See the [module docs](super).
    pub struct Dictionary<K, V> {
        h: Handle<K, V>,
    }

    impl<K, V> Dictionary<K, V> {
        /// `new Dictionary<K, V>()`.
        pub fn new() -> Self {
            Self { h: raw_ctor::<K, V>() }
        }
        /// Number of entries (`Count`).
        pub fn len(&self) -> i32 {
            raw_count::<K, V>(self.h)
        }
        /// `true` if empty.
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
        /// Insert or overwrite `key => value` (the indexer `set_Item` — never throws on a duplicate).
        pub fn insert(&mut self, key: K, value: V) {
            raw_set::<K, V>(self.h, key, value)
        }
        /// Whether `key` is present (`ContainsKey`).
        pub fn contains_key(&self, key: K) -> bool {
            raw_contains::<K, V>(self.h, key)
        }
        /// Remove `key`; returns whether it was present (`Remove`).
        pub fn remove(&mut self, key: K) -> bool {
            raw_remove::<K, V>(self.h, key)
        }
        /// Remove all entries (`Clear`).
        pub fn clear(&mut self) {
            raw_clear::<K, V>(self.h)
        }
        /// The raw managed handle, for advanced interop.
        pub fn handle(&self) -> Handle<K, V> {
            self.h
        }
    }

    impl<K: Copy, V> Dictionary<K, V> {
        /// The value for `key`, or `None` if absent (checks `ContainsKey` first, so it never throws;
        /// `K: Copy` because the key is used for both the presence check and the lookup).
        pub fn get(&self, key: K) -> Option<V> {
            if raw_contains::<K, V>(self.h, key) {
                Some(raw_get::<K, V>(self.h, key))
            } else {
                None
            }
        }
        /// The value for `key`, or `default` if the key is absent (never throws, never inserts).
        pub fn get_or_default(&self, key: K, default: V) -> V {
            if raw_contains::<K, V>(self.h, key) {
                raw_get::<K, V>(self.h, key)
            } else {
                default
            }
        }
    }

    impl<K, V> Default for Dictionary<K, V> {
        fn default() -> Self {
            Self::new()
        }
    }
}

mod hash_set {
    use super::CORELIB;
    use crate::{dotnet_generic, dotnet_generic_impl, gen};

    dotnet_generic!(Handle<T> = [CORELIB] "System.Collections.Generic.HashSet" < (T,) >);
    dotnet_generic_impl! {
        Handle<T> = [CORELIB] "System.Collections.Generic.HashSet" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_add      = "Add"(r, item: T as gen!(0)) -> bool as bool;
        fn raw_contains = "Contains"(r, item: T as gen!(0)) -> bool as bool;
        fn raw_remove   = "Remove"(r, item: T as gen!(0)) -> bool as bool;
        fn raw_count    = "get_Count"(r) -> i32 as i32;
        fn raw_clear    = "Clear"(r);
    }

    /// A managed `System.Collections.Generic.HashSet<T>`. See the [module docs](super).
    pub struct HashSet<T> {
        h: Handle<T>,
    }

    impl<T> HashSet<T> {
        /// `new HashSet<T>()`.
        pub fn new() -> Self {
            Self { h: raw_ctor::<T>() }
        }
        /// Number of elements (`Count`).
        pub fn len(&self) -> i32 {
            raw_count::<T>(self.h)
        }
        /// `true` if empty.
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
        /// Insert `item`; returns `true` if it was newly added, `false` if already present (`Add`).
        pub fn insert(&mut self, item: T) -> bool {
            raw_add::<T>(self.h, item)
        }
        /// Whether `item` is present (`Contains`).
        pub fn contains(&self, item: T) -> bool {
            raw_contains::<T>(self.h, item)
        }
        /// Remove `item`; returns whether it was present (`Remove`).
        pub fn remove(&mut self, item: T) -> bool {
            raw_remove::<T>(self.h, item)
        }
        /// Remove all elements (`Clear`).
        pub fn clear(&mut self) {
            raw_clear::<T>(self.h)
        }
        /// The raw managed handle, for advanced interop.
        pub fn handle(&self) -> Handle<T> {
            self.h
        }
    }

    impl<T> Default for HashSet<T> {
        fn default() -> Self {
            Self::new()
        }
    }
}

mod stack {
    use crate::{dotnet_generic, dotnet_generic_impl, gen};

    // `Stack<T>`/`Queue<T>` are implemented in `System.Collections.dll` (moved out of
    // `System.Private.CoreLib` in .NET Core), so their method-body refs must name that impl assembly —
    // unlike `List`/`Dictionary`/`HashSet`, which the runtime keeps in CoreLib.
    const ASM: &str = "System.Collections";

    dotnet_generic!(Handle<T> = [ASM] "System.Collections.Generic.Stack" < (T,) >);
    dotnet_generic_impl! {
        Handle<T> = [ASM] "System.Collections.Generic.Stack" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_push  = "Push"(r, item: T as gen!(0));
        fn raw_pop   = "Pop"(r) -> T as gen!(0);
        fn raw_peek  = "Peek"(r) -> T as gen!(0);
        fn raw_count = "get_Count"(r) -> i32 as i32;
        fn raw_clear = "Clear"(r);
    }

    /// A managed `System.Collections.Generic.Stack<T>` (LIFO). See the [module docs](super).
    pub struct Stack<T> {
        h: Handle<T>,
    }

    impl<T> Stack<T> {
        /// `new Stack<T>()`.
        pub fn new() -> Self {
            Self { h: raw_ctor::<T>() }
        }
        /// Number of elements (`Count`).
        pub fn len(&self) -> i32 {
            raw_count::<T>(self.h)
        }
        /// `true` if empty.
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
        /// Push `item` onto the top (`Push`).
        pub fn push(&mut self, item: T) {
            raw_push::<T>(self.h, item)
        }
        /// Pop the top element, or `None` if empty (bounds-checked, so it never throws).
        pub fn pop(&mut self) -> Option<T> {
            if self.len() > 0 {
                Some(raw_pop::<T>(self.h))
            } else {
                None
            }
        }
        /// The top element without removing it, or `None` if empty (`Peek`).
        pub fn peek(&self) -> Option<T> {
            if self.len() > 0 {
                Some(raw_peek::<T>(self.h))
            } else {
                None
            }
        }
        /// Remove all elements (`Clear`).
        pub fn clear(&mut self) {
            raw_clear::<T>(self.h)
        }
        /// The raw managed handle, for advanced interop.
        pub fn handle(&self) -> Handle<T> {
            self.h
        }
    }

    impl<T> Default for Stack<T> {
        fn default() -> Self {
            Self::new()
        }
    }
}

mod queue {
    use crate::{dotnet_generic, dotnet_generic_impl, gen};

    // See `stack`: `Queue<T>`'s impl assembly is `System.Collections.dll`, not CoreLib.
    const ASM: &str = "System.Collections";

    dotnet_generic!(Handle<T> = [ASM] "System.Collections.Generic.Queue" < (T,) >);
    dotnet_generic_impl! {
        Handle<T> = [ASM] "System.Collections.Generic.Queue" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_enqueue = "Enqueue"(r, item: T as gen!(0));
        fn raw_dequeue = "Dequeue"(r) -> T as gen!(0);
        fn raw_peek    = "Peek"(r) -> T as gen!(0);
        fn raw_count   = "get_Count"(r) -> i32 as i32;
        fn raw_clear   = "Clear"(r);
    }

    /// A managed `System.Collections.Generic.Queue<T>` (FIFO). See the [module docs](super).
    pub struct Queue<T> {
        h: Handle<T>,
    }

    impl<T> Queue<T> {
        /// `new Queue<T>()`.
        pub fn new() -> Self {
            Self { h: raw_ctor::<T>() }
        }
        /// Number of elements (`Count`).
        pub fn len(&self) -> i32 {
            raw_count::<T>(self.h)
        }
        /// `true` if empty.
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
        /// Add `item` to the back (`Enqueue`).
        pub fn enqueue(&mut self, item: T) {
            raw_enqueue::<T>(self.h, item)
        }
        /// Remove and return the front element, or `None` if empty (bounds-checked; never throws).
        pub fn dequeue(&mut self) -> Option<T> {
            if self.len() > 0 {
                Some(raw_dequeue::<T>(self.h))
            } else {
                None
            }
        }
        /// The front element without removing it, or `None` if empty (`Peek`).
        pub fn peek(&self) -> Option<T> {
            if self.len() > 0 {
                Some(raw_peek::<T>(self.h))
            } else {
                None
            }
        }
        /// Remove all elements (`Clear`).
        pub fn clear(&mut self) {
            raw_clear::<T>(self.h)
        }
        /// The raw managed handle, for advanced interop.
        pub fn handle(&self) -> Handle<T> {
            self.h
        }
    }

    impl<T> Default for Queue<T> {
        fn default() -> Self {
            Self::new()
        }
    }
}
