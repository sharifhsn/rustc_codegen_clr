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

/// `System.Collections.Concurrent.ConcurrentBag<T>` — a thread-safe managed unordered bag.
pub use self::concurrent_bag::ConcurrentBag;
/// `System.Collections.Concurrent.ConcurrentDictionary<K, V>` — a thread-safe managed hash map.
pub use self::concurrent_dictionary::ConcurrentDictionary;
/// `System.Collections.Concurrent.ConcurrentQueue<T>` — a thread-safe managed FIFO queue.
pub use self::concurrent_queue::ConcurrentQueue;
/// `System.Collections.Generic.Dictionary<K, V>` — a managed hash map.
pub use self::dictionary::Dictionary;
/// `System.Collections.Generic.HashSet<T>` — a managed hash set.
pub use self::hash_set::HashSet;
/// `System.Collections.Generic.LinkedList<T>` — a managed doubly-linked list.
pub use self::linked_list::LinkedList;
/// `System.Collections.Generic.List<T>` — a growable, index-addressable managed list.
pub use self::list::{List, ListIter};
/// `System.Collections.Generic.PriorityQueue<TElement, TPriority>` — a managed min-priority queue.
pub use self::priority_queue::PriorityQueue;
/// `System.Collections.Generic.Queue<T>` — a managed FIFO queue.
pub use self::queue::Queue;
/// `System.Collections.Generic.SortedDictionary<K, V>` — a managed key-ordered map (red-black tree).
pub use self::sorted_dictionary::SortedDictionary;
/// `System.Collections.Generic.SortedSet<T>` — a managed ordered set (red-black tree).
pub use self::sorted_set::SortedSet;
/// `System.Collections.Generic.Stack<T>` — a managed LIFO stack.
pub use self::stack::Stack;

// The core BCL collections all live in the implementation assembly `System.Private.CoreLib` — a
// reference assembly forwards the type and throws `TypeLoadException` at JIT, so method-body refs must
// name the impl assembly (the same rule the generic bridge documents).
const CORELIB: &str = "System.Private.CoreLib";

mod list {
    use super::CORELIB;
    use crate::intrinsics::{
        RustcCLRInteropManagedGeneric, RustcCLRInteropTypeGeneric, rustc_clr_interop_generic_call2,
    };
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    const LIST: &str = "System.Collections.Generic.List";
    const ACTION: &str = "System.Action";
    const COMPARISON: &str = "System.Comparison";
    /// The concrete `Action<T>` / `Comparison<T>` delegate handles a `List<T>` method accepts.
    type ActionH<T> = RustcCLRInteropManagedGeneric<CORELIB, ACTION, (T,)>;
    type ComparisonH<T> = RustcCLRInteropManagedGeneric<CORELIB, COMPARISON, (T,)>;

    // `List<T>.ForEach(Action<!0>)` / `Sort(Comparison<!0>)` — the delegate parameter's def-shape type
    // is parameterised by the *class* generic (`Action<!0>`), so it binds against the concrete
    // `Action<T>` argument via the nested-generic rule. Hand-written (the `dotnet_generic_impl!` line
    // grammar doesn't express a nested-generic delegate arg); the `r#gen!(0)` inside the delegate's own
    // generics is the `!0` def-shape spelling.
    fn raw_for_each<T>(h: Handle<T>, action: ActionH<T>) {
        rustc_clr_interop_generic_call2::<
            CORELIB,
            LIST,
            false,
            "ForEach",
            2,
            (T,),
            (
                (),
                RustcCLRInteropManagedGeneric<CORELIB, ACTION, (RustcCLRInteropTypeGeneric<0>,)>,
            ),
            (),
            Handle<T>,
            ActionH<T>,
        >(h, action)
    }
    fn raw_sort_by<T>(h: Handle<T>, cmp: ComparisonH<T>) {
        rustc_clr_interop_generic_call2::<
            CORELIB,
            LIST,
            false,
            "Sort",
            2,
            (T,),
            (
                (),
                RustcCLRInteropManagedGeneric<
                    CORELIB,
                    COMPARISON,
                    (RustcCLRInteropTypeGeneric<0>,),
                >,
            ),
            (),
            Handle<T>,
            ComparisonH<T>,
        >(h, cmp)
    }

    dotnet_generic!(Handle<T> = [CORELIB] "System.Collections.Generic.List" < (T,) >);
    // SAFETY-BEARING: `Handle<..>` really is a .NET class implementing `IEnumerable<..>` (the BCL type this alias names), so `as_enum_handle` on it is sound.
    unsafe impl<T> crate::enumerate::ImplementsIEnumerable<T> for Handle<T> {}
    dotnet_generic_impl! {
        Handle<T> = [CORELIB] "System.Collections.Generic.List" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_add       = "Add"(r, item: T as r#gen!(0));
        fn raw_get       = "get_Item"(r, idx: i32 as i32) -> T as r#gen!(0);
        fn raw_set       = "set_Item"(r, idx: i32 as i32, item: T as r#gen!(0));
        fn raw_count     = "get_Count"(r) -> i32 as i32;
        fn raw_contains  = "Contains"(r, item: T as r#gen!(0)) -> bool as bool;
        fn raw_index_of  = "IndexOf"(r, item: T as r#gen!(0)) -> i32 as i32;
        fn raw_remove_at = "RemoveAt"(r, idx: i32 as i32);
        fn raw_insert    = "Insert"(r, idx: i32 as i32, item: T as r#gen!(0));
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
        /// Apply a callback to each element in order (`List<T>.ForEach(Action<T>)`) — the .NET side
        /// drives the Rust `extern "C" fn` through a managed delegate. Pass a top-level fn or a
        /// capture-less closure. (Capturing closures need a boxed-env delegate — see [`crate::delegate`].)
        pub fn for_each(&self, f: extern "C" fn(T)) {
            raw_for_each::<T>(self.h, crate::delegate::Action1::<T>::from_fn(f).handle())
        }
        /// Sort in place by a comparator (`List<T>.Sort(Comparison<T>)`); `cmp(a, b)` returns negative
        /// / zero / positive like `Ord::cmp`. The .NET sort drives the Rust `extern "C" fn`.
        pub fn sort_by(&mut self, cmp: extern "C" fn(T, T) -> i32) {
            raw_sort_by::<T>(
                self.h,
                crate::delegate::Comparison::<T>::from_fn(cmp).handle(),
            )
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

    // By-reference iteration via the enumerator bridge: `for x in &list` drives the .NET
    // `IEnumerator<T>` (`GetEnumerator`/`MoveNext`/`Current`) rather than an index loop.
    impl<T> crate::enumerate::Enumerable<T> for List<T> {
        fn enumerable_handle(&self) -> crate::enumerate::IEnumerable<T> {
            crate::enumerate::as_enum_handle(self.h)
        }
    }
    impl<'a, T> IntoIterator for &'a List<T> {
        type Item = T;
        type IntoIter = crate::enumerate::Enumerator<T>;
        fn into_iter(self) -> Self::IntoIter {
            use crate::enumerate::Enumerable;
            self.iter_enumerator()
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
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    dotnet_generic!(Handle<K, V> = [CORELIB] "System.Collections.Generic.Dictionary" < (K, V) >);
    // SAFETY-BEARING: `Handle<..>` really is a .NET class implementing `IEnumerable<..>` (the BCL type this alias names), so `as_enum_handle` on it is sound.
    unsafe impl<K, V> crate::enumerate::ImplementsIEnumerable<crate::enumerate::KeyValuePair<K, V>>
        for Handle<K, V>
    {
    }
    dotnet_generic_impl! {
        Handle<K, V> = [CORELIB] "System.Collections.Generic.Dictionary" < (K, V) > ;
        ctor fn raw_ctor();
        fn raw_set       = "set_Item"(r, key: K as r#gen!(0), value: V as r#gen!(1));
        fn raw_get       = "get_Item"(r, key: K as r#gen!(0)) -> V as r#gen!(1);
        fn raw_contains  = "ContainsKey"(r, key: K as r#gen!(0)) -> bool as bool;
        fn raw_remove    = "Remove"(r, key: K as r#gen!(0)) -> bool as bool;
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
            Self {
                h: raw_ctor::<K, V>(),
            }
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

    impl<K, V> Dictionary<K, V> {
        /// Iterate the `(key, value)` entries (`for (k, v) in &dict` / `dict.iter()`), driving the
        /// .NET enumerator over `KeyValuePair<K, V>`. The dictionary must not be mutated during
        /// iteration (the .NET enumerator throws `InvalidOperationException`, exactly as in C#).
        pub fn iter(&self) -> crate::enumerate::EntryIter<K, V> {
            use crate::enumerate::EnumerableEntries;
            self.iter_entries()
        }
        /// Iterate the keys (`for k in dict.keys()`). Built on entry iteration — the .NET
        /// `get_Keys()`/`KeyCollection` route additionally needs nested-generic type-name rendering,
        /// which this sidesteps.
        pub fn keys(&self) -> impl Iterator<Item = K> {
            self.iter().map(|(k, _)| k)
        }
        /// Iterate the values (`for v in dict.values()`).
        pub fn values(&self) -> impl Iterator<Item = V> {
            self.iter().map(|(_, v)| v)
        }
    }

    // Entry enumeration: a `Dictionary<K, V>` enumerates as `KeyValuePair<K, V>` — a generic *value
    // type* — and each pair is split into `(K, V)` with the value-type instance getters. Both halves
    // (value-type-generic instance methods for `KeyValuePair`) are now supported by the backend.
    impl<K, V> crate::enumerate::Enumerable<crate::enumerate::KeyValuePair<K, V>> for Dictionary<K, V> {
        fn enumerable_handle(
            &self,
        ) -> crate::enumerate::IEnumerable<crate::enumerate::KeyValuePair<K, V>> {
            crate::enumerate::as_enum_handle::<_, crate::enumerate::KeyValuePair<K, V>>(self.h)
        }
    }
    impl<'a, K, V> IntoIterator for &'a Dictionary<K, V> {
        type Item = (K, V);
        type IntoIter = crate::enumerate::EntryIter<K, V>;
        fn into_iter(self) -> Self::IntoIter {
            self.iter()
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
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    dotnet_generic!(Handle<T> = [CORELIB] "System.Collections.Generic.HashSet" < (T,) >);
    // SAFETY-BEARING: `Handle<..>` really is a .NET class implementing `IEnumerable<..>` (the BCL type this alias names), so `as_enum_handle` on it is sound.
    unsafe impl<T> crate::enumerate::ImplementsIEnumerable<T> for Handle<T> {}
    dotnet_generic_impl! {
        Handle<T> = [CORELIB] "System.Collections.Generic.HashSet" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_add      = "Add"(r, item: T as r#gen!(0)) -> bool as bool;
        fn raw_contains = "Contains"(r, item: T as r#gen!(0)) -> bool as bool;
        fn raw_remove   = "Remove"(r, item: T as r#gen!(0)) -> bool as bool;
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

    // Iteration via the enumerator bridge (order is the .NET set's internal order, as in C#).
    impl<T> crate::enumerate::Enumerable<T> for HashSet<T> {
        fn enumerable_handle(&self) -> crate::enumerate::IEnumerable<T> {
            crate::enumerate::as_enum_handle(self.h)
        }
    }
    impl<'a, T> IntoIterator for &'a HashSet<T> {
        type Item = T;
        type IntoIter = crate::enumerate::Enumerator<T>;
        fn into_iter(self) -> Self::IntoIter {
            use crate::enumerate::Enumerable;
            self.iter_enumerator()
        }
    }

    impl<T> FromIterator<T> for HashSet<T> {
        fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
            let mut s = Self::new();
            for item in iter {
                s.insert(item);
            }
            s
        }
    }

    impl<T> Extend<T> for HashSet<T> {
        fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
            for item in iter {
                self.insert(item);
            }
        }
    }
}

mod stack {
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    // `Stack<T>`/`Queue<T>` are implemented in `System.Collections.dll` (moved out of
    // `System.Private.CoreLib` in .NET Core), so their method-body refs must name that impl assembly —
    // unlike `List`/`Dictionary`/`HashSet`, which the runtime keeps in CoreLib.
    const ASM: &str = "System.Collections";

    dotnet_generic!(Handle<T> = [ASM] "System.Collections.Generic.Stack" < (T,) >);
    // SAFETY-BEARING: `Handle<..>` really is a .NET class implementing `IEnumerable<..>` (the BCL type this alias names), so `as_enum_handle` on it is sound.
    unsafe impl<T> crate::enumerate::ImplementsIEnumerable<T> for Handle<T> {}
    dotnet_generic_impl! {
        Handle<T> = [ASM] "System.Collections.Generic.Stack" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_push  = "Push"(r, item: T as r#gen!(0));
        fn raw_pop   = "Pop"(r) -> T as r#gen!(0);
        fn raw_peek  = "Peek"(r) -> T as r#gen!(0);
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

    // Iteration via the enumerator bridge. `Stack<T>` enumerates LIFO (top first), matching C#.
    impl<T> crate::enumerate::Enumerable<T> for Stack<T> {
        fn enumerable_handle(&self) -> crate::enumerate::IEnumerable<T> {
            crate::enumerate::as_enum_handle(self.h)
        }
    }
    impl<'a, T> IntoIterator for &'a Stack<T> {
        type Item = T;
        type IntoIter = crate::enumerate::Enumerator<T>;
        fn into_iter(self) -> Self::IntoIter {
            use crate::enumerate::Enumerable;
            self.iter_enumerator()
        }
    }
}

mod queue {
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    // See `stack`: `Queue<T>`'s impl assembly is `System.Collections.dll`, not CoreLib.
    const ASM: &str = "System.Collections";

    dotnet_generic!(Handle<T> = [ASM] "System.Collections.Generic.Queue" < (T,) >);
    // SAFETY-BEARING: `Handle<..>` really is a .NET class implementing `IEnumerable<..>` (the BCL type this alias names), so `as_enum_handle` on it is sound.
    unsafe impl<T> crate::enumerate::ImplementsIEnumerable<T> for Handle<T> {}
    dotnet_generic_impl! {
        Handle<T> = [ASM] "System.Collections.Generic.Queue" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_enqueue = "Enqueue"(r, item: T as r#gen!(0));
        fn raw_dequeue = "Dequeue"(r) -> T as r#gen!(0);
        fn raw_peek    = "Peek"(r) -> T as r#gen!(0);
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

    // Iteration via the enumerator bridge. `Queue<T>` enumerates FIFO (front first), matching C#.
    impl<T> crate::enumerate::Enumerable<T> for Queue<T> {
        fn enumerable_handle(&self) -> crate::enumerate::IEnumerable<T> {
            crate::enumerate::as_enum_handle(self.h)
        }
    }
    impl<'a, T> IntoIterator for &'a Queue<T> {
        type Item = T;
        type IntoIter = crate::enumerate::Enumerator<T>;
        fn into_iter(self) -> Self::IntoIter {
            use crate::enumerate::Enumerable;
            self.iter_enumerator()
        }
    }
}

mod sorted_dictionary {
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    // `SortedDictionary`/`SortedSet`/`LinkedList`/`PriorityQueue` are implemented in
    // `System.Collections.dll` (not CoreLib, unlike `List`/`Dictionary`/`HashSet`), so their
    // method-body refs must name that impl assembly or the CLR throws `TypeLoadException` at JIT.
    const ASM: &str = "System.Collections";

    dotnet_generic!(Handle<K, V> = [ASM] "System.Collections.Generic.SortedDictionary" < (K, V) >);
    dotnet_generic_impl! {
        Handle<K, V> = [ASM] "System.Collections.Generic.SortedDictionary" < (K, V) > ;
        ctor fn raw_ctor();
        fn raw_set      = "set_Item"(r, key: K as r#gen!(0), value: V as r#gen!(1));
        fn raw_get      = "get_Item"(r, key: K as r#gen!(0)) -> V as r#gen!(1);
        fn raw_contains = "ContainsKey"(r, key: K as r#gen!(0)) -> bool as bool;
        fn raw_remove   = "Remove"(r, key: K as r#gen!(0)) -> bool as bool;
        fn raw_count    = "get_Count"(r) -> i32 as i32;
        fn raw_clear    = "Clear"(r);
    }

    /// A managed `System.Collections.Generic.SortedDictionary<K, V>` — a map kept sorted by key (a
    /// red-black tree; keys iterate in ascending order). Same surface as [`super::Dictionary`], but
    /// `K` must be `.NET`-comparable (implements `IComparable`) or operations throw at runtime.
    /// See the [module docs](super).
    pub struct SortedDictionary<K, V> {
        h: Handle<K, V>,
    }

    impl<K, V> SortedDictionary<K, V> {
        /// `new SortedDictionary<K, V>()`.
        pub fn new() -> Self {
            Self {
                h: raw_ctor::<K, V>(),
            }
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

    impl<K: Copy, V> SortedDictionary<K, V> {
        /// The value for `key`, or `None` if absent (checks `ContainsKey` first, so it never throws).
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

    impl<K, V> Default for SortedDictionary<K, V> {
        fn default() -> Self {
            Self::new()
        }
    }
}

mod sorted_set {
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    // See `sorted_dictionary`: impl assembly is `System.Collections`, not CoreLib.
    const ASM: &str = "System.Collections";

    dotnet_generic!(Handle<T> = [ASM] "System.Collections.Generic.SortedSet" < (T,) >);
    // SAFETY-BEARING: `Handle<..>` really is a .NET class implementing `IEnumerable<..>` (the BCL type this alias names), so `as_enum_handle` on it is sound.
    unsafe impl<T> crate::enumerate::ImplementsIEnumerable<T> for Handle<T> {}
    dotnet_generic_impl! {
        Handle<T> = [ASM] "System.Collections.Generic.SortedSet" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_add      = "Add"(r, item: T as r#gen!(0)) -> bool as bool;
        fn raw_contains = "Contains"(r, item: T as r#gen!(0)) -> bool as bool;
        fn raw_remove   = "Remove"(r, item: T as r#gen!(0)) -> bool as bool;
        fn raw_count    = "get_Count"(r) -> i32 as i32;
        fn raw_clear    = "Clear"(r);
    }

    /// A managed `System.Collections.Generic.SortedSet<T>` — a set kept in ascending order (a
    /// red-black tree; iteration yields elements sorted). Same surface as [`super::HashSet`], but `T`
    /// must be `.NET`-comparable (`IComparable`) or operations throw at runtime. See the
    /// [module docs](super).
    pub struct SortedSet<T> {
        h: Handle<T>,
    }

    impl<T> SortedSet<T> {
        /// `new SortedSet<T>()`.
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

    impl<T> Default for SortedSet<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    // Iteration via the enumerator bridge — yields elements in ascending sorted order (as in C#).
    impl<T> crate::enumerate::Enumerable<T> for SortedSet<T> {
        fn enumerable_handle(&self) -> crate::enumerate::IEnumerable<T> {
            crate::enumerate::as_enum_handle(self.h)
        }
    }
    impl<'a, T> IntoIterator for &'a SortedSet<T> {
        type Item = T;
        type IntoIter = crate::enumerate::Enumerator<T>;
        fn into_iter(self) -> Self::IntoIter {
            use crate::enumerate::Enumerable;
            self.iter_enumerator()
        }
    }

    impl<T> FromIterator<T> for SortedSet<T> {
        fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
            let mut s = Self::new();
            for item in iter {
                s.insert(item);
            }
            s
        }
    }

    impl<T> Extend<T> for SortedSet<T> {
        fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
            for item in iter {
                self.insert(item);
            }
        }
    }
}

mod linked_list {
    use super::CORELIB;
    use crate::intrinsics::{
        RustcCLRInteropManagedGeneric, RustcCLRInteropTypeGeneric, rustc_clr_interop_generic_call2,
    };
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    // `LinkedList<T>` is implemented in `System.Collections.dll` (see `sorted_dictionary`). The
    // `ICollection<T>` interface it is upcast to, however, is a core interface in CoreLib.
    const ASM: &str = "System.Collections";
    const LL: &str = "System.Collections.Generic.LinkedList";
    const LLNODE: &str = "System.Collections.Generic.LinkedListNode";
    /// The `LinkedListNode<T>` a node-returning op hands back (discarded by `push_front`).
    type NodeH<T> = RustcCLRInteropManagedGeneric<ASM, LLNODE, (T,)>;

    // `LinkedList<T>.AddFirst(T)` returns a `LinkedListNode<T>` — a *nested generic* reference type.
    // The def-shape return `LinkedListNode`1<!0>` now binds against the concrete `NodeH<T>` local
    // (nested-generic binding), so `push_front` can call it (and discard the node).
    fn raw_add_first<T>(h: Handle<T>, item: T) -> NodeH<T> {
        rustc_clr_interop_generic_call2::<
            ASM,
            LL,
            false,
            "AddFirst",
            2u8,
            (T,),
            (
                RustcCLRInteropManagedGeneric<ASM, LLNODE, (RustcCLRInteropTypeGeneric<0>,)>,
                RustcCLRInteropTypeGeneric<0>,
            ),
            NodeH<T>,
            Handle<T>,
            T,
        >(h, item)
    }

    dotnet_generic!(Handle<T> = [ASM] "System.Collections.Generic.LinkedList" < (T,) >);
    // SAFETY-BEARING: `Handle<..>` really is a .NET class implementing `IEnumerable<..>` (the BCL type this alias names), so `as_enum_handle` on it is sound.
    unsafe impl<T> crate::enumerate::ImplementsIEnumerable<T> for Handle<T> {}
    // `ICollection<T>` — the interface view used to append (`Add` == `AddLast` for a linked list).
    // `LinkedList<T>.AddLast(T)` returns a `LinkedListNode<T>` (a nested generic reference type whose
    // definition-shape return the CIL typechecker cannot accept against a concrete local — see the
    // `dictionary` note), so we append through `ICollection<T>.Add(T)`, which is `void` and reachable
    // with a bare `!0` argument. `LinkedList<T>` implements `ICollection<T>`, so the upcast always
    // succeeds; the runtime routes `ICollection<T>.Add` to `AddLast`.
    dotnet_generic!(ICollection<T> = [CORELIB] "System.Collections.Generic.ICollection" < (T,) >);
    dotnet_generic_impl! {
        Handle<T> = [ASM] "System.Collections.Generic.LinkedList" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_contains = "Contains"(r, item: T as r#gen!(0)) -> bool as bool;
        fn raw_remove   = "Remove"(r, item: T as r#gen!(0)) -> bool as bool;
        fn raw_count    = "get_Count"(r) -> i32 as i32;
        fn raw_clear    = "Clear"(r);
    }
    dotnet_generic_impl! {
        ICollection<T> = [CORELIB] "System.Collections.Generic.ICollection" < (T,) > ;
        fn raw_icoll_add = "Add"(r, item: T as r#gen!(0));
    }

    /// A managed `System.Collections.Generic.LinkedList<T>` — a doubly-linked list. `push_front`
    /// (`AddFirst`) and `push_back` are both exposed; `AddFirst`'s returned `LinkedListNode<T>` (a
    /// nested generic) is now bindable and simply discarded. See the [module docs](super).
    pub struct LinkedList<T> {
        h: Handle<T>,
    }

    impl<T> LinkedList<T> {
        /// `new LinkedList<T>()`.
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
        /// Append `item` at the end (`AddLast`, reached through `ICollection<T>.Add`).
        pub fn push_back(&mut self, item: T) {
            let ic = crate::intrinsics::rustc_clr_interop_managed_checked_cast::<
                ICollection<T>,
                Handle<T>,
            >(self.h);
            raw_icoll_add::<T>(ic, item)
        }
        /// Prepend `item` at the front (`AddFirst`). The `LinkedListNode<T>` it returns is discarded.
        pub fn push_front(&mut self, item: T) {
            let _node = raw_add_first::<T>(self.h, item);
        }
        /// Whether `item` is present (`Contains`, `.NET` equality).
        pub fn contains(&self, item: T) -> bool {
            raw_contains::<T>(self.h, item)
        }
        /// Remove the first node whose value equals `item`; returns whether one was found (`Remove`).
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

    impl<T> Default for LinkedList<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    // Iteration via the enumerator bridge — front-to-back order (as in C#).
    impl<T> crate::enumerate::Enumerable<T> for LinkedList<T> {
        fn enumerable_handle(&self) -> crate::enumerate::IEnumerable<T> {
            crate::enumerate::as_enum_handle(self.h)
        }
    }
    impl<'a, T> IntoIterator for &'a LinkedList<T> {
        type Item = T;
        type IntoIter = crate::enumerate::Enumerator<T>;
        fn into_iter(self) -> Self::IntoIter {
            use crate::enumerate::Enumerable;
            self.iter_enumerator()
        }
    }

    impl<T> FromIterator<T> for LinkedList<T> {
        fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
            let mut l = Self::new();
            for item in iter {
                l.push_back(item);
            }
            l
        }
    }

    impl<T> Extend<T> for LinkedList<T> {
        fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
            for item in iter {
                self.push_back(item);
            }
        }
    }
}

mod priority_queue {
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    // See `sorted_dictionary`: impl assembly is `System.Collections`, not CoreLib.
    const ASM: &str = "System.Collections";

    dotnet_generic!(Handle<E, P> = [ASM] "System.Collections.Generic.PriorityQueue" < (E, P) >);
    dotnet_generic_impl! {
        Handle<E, P> = [ASM] "System.Collections.Generic.PriorityQueue" < (E, P) > ;
        ctor fn raw_ctor();
        fn raw_enqueue = "Enqueue"(r, element: E as r#gen!(0), priority: P as r#gen!(1));
        fn raw_dequeue = "Dequeue"(r) -> E as r#gen!(0);
        fn raw_peek    = "Peek"(r) -> E as r#gen!(0);
        fn raw_count   = "get_Count"(r) -> i32 as i32;
        fn raw_clear   = "Clear"(r);
    }

    /// A managed `System.Collections.Generic.PriorityQueue<TElement, TPriority>` — a **min**-priority
    /// queue (the lowest priority dequeues first; `P` must be `.NET`-comparable, `IComparable`).
    /// Elements with equal priority are **not** ordered relative to each other (as in C#). See the
    /// [module docs](super).
    pub struct PriorityQueue<E, P> {
        h: Handle<E, P>,
    }

    impl<E, P> PriorityQueue<E, P> {
        /// `new PriorityQueue<TElement, TPriority>()`.
        pub fn new() -> Self {
            Self {
                h: raw_ctor::<E, P>(),
            }
        }
        /// Number of elements (`Count`).
        pub fn len(&self) -> i32 {
            raw_count::<E, P>(self.h)
        }
        /// `true` if empty.
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
        /// Add `element` with the given `priority` (`Enqueue`).
        pub fn enqueue(&mut self, element: E, priority: P) {
            raw_enqueue::<E, P>(self.h, element, priority)
        }
        /// Remove and return the lowest-priority element, or `None` if empty (bounds-checked; never
        /// throws).
        pub fn dequeue(&mut self) -> Option<E> {
            if self.len() > 0 {
                Some(raw_dequeue::<E, P>(self.h))
            } else {
                None
            }
        }
        /// The lowest-priority element without removing it, or `None` if empty (`Peek`).
        pub fn peek(&self) -> Option<E> {
            if self.len() > 0 {
                Some(raw_peek::<E, P>(self.h))
            } else {
                None
            }
        }
        /// Remove all elements (`Clear`).
        pub fn clear(&mut self) {
            raw_clear::<E, P>(self.h)
        }
        /// The raw managed handle, for advanced interop.
        pub fn handle(&self) -> Handle<E, P> {
            self.h
        }
    }

    impl<E, P> Default for PriorityQueue<E, P> {
        fn default() -> Self {
            Self::new()
        }
    }
}

mod concurrent_dictionary {
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    // The concurrent collections live in `System.Collections.Concurrent.dll` — a distinct impl
    // assembly from CoreLib and from `System.Collections` (a wrong name → `TypeLoadException` at JIT).
    const ASM: &str = "System.Collections.Concurrent";

    dotnet_generic!(Handle<K, V> = [ASM] "System.Collections.Concurrent.ConcurrentDictionary" < (K, V) >);
    dotnet_generic_impl! {
        Handle<K, V> = [ASM] "System.Collections.Concurrent.ConcurrentDictionary" < (K, V) > ;
        ctor fn raw_ctor();
        fn raw_set      = "set_Item"(r, key: K as r#gen!(0), value: V as r#gen!(1));
        fn raw_get      = "get_Item"(r, key: K as r#gen!(0)) -> V as r#gen!(1);
        fn raw_try_add  = "TryAdd"(r, key: K as r#gen!(0), value: V as r#gen!(1)) -> bool as bool;
        fn raw_contains = "ContainsKey"(r, key: K as r#gen!(0)) -> bool as bool;
        fn raw_count    = "get_Count"(r) -> i32 as i32;
        fn raw_empty    = "get_IsEmpty"(r) -> bool as bool;
        fn raw_clear    = "Clear"(r);
    }

    /// A managed `System.Collections.Concurrent.ConcurrentDictionary<K, V>` — a thread-safe hash map.
    ///
    /// Removal (`TryRemove(K, out V)`) is **not** exposed: it returns the removed value through a .NET
    /// `out` parameter, which the current generic bridge cannot marshal (no by-ref `!N` argument).
    /// Everything else is by-value and fully supported. See the [module docs](super).
    pub struct ConcurrentDictionary<K, V> {
        h: Handle<K, V>,
    }

    impl<K, V> ConcurrentDictionary<K, V> {
        /// `new ConcurrentDictionary<K, V>()`.
        pub fn new() -> Self {
            Self {
                h: raw_ctor::<K, V>(),
            }
        }
        /// Number of entries (`Count`).
        pub fn len(&self) -> i32 {
            raw_count::<K, V>(self.h)
        }
        /// `true` if empty (`IsEmpty` — the lock-free property, not `Count == 0`).
        pub fn is_empty(&self) -> bool {
            raw_empty::<K, V>(self.h)
        }
        /// Insert or overwrite `key => value` (the indexer `set_Item`).
        pub fn insert(&mut self, key: K, value: V) {
            raw_set::<K, V>(self.h, key, value)
        }
        /// Attempt to add `key => value` only if the key is absent; returns `true` if it was added,
        /// `false` if the key already existed (`TryAdd` — atomic, never overwrites).
        pub fn try_add(&mut self, key: K, value: V) -> bool {
            raw_try_add::<K, V>(self.h, key, value)
        }
        /// Whether `key` is present (`ContainsKey`).
        pub fn contains_key(&self, key: K) -> bool {
            raw_contains::<K, V>(self.h, key)
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

    impl<K: Copy, V> ConcurrentDictionary<K, V> {
        /// The value for `key`, or `None` if absent (checks `ContainsKey` first, so it never throws).
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

    impl<K, V> Default for ConcurrentDictionary<K, V> {
        fn default() -> Self {
            Self::new()
        }
    }
}

mod concurrent_queue {
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    // See `concurrent_dictionary`: impl assembly is `System.Collections.Concurrent`.
    const ASM: &str = "System.Collections.Concurrent";

    dotnet_generic!(Handle<T> = [ASM] "System.Collections.Concurrent.ConcurrentQueue" < (T,) >);
    // SAFETY-BEARING: `Handle<..>` really is a .NET class implementing `IEnumerable<..>` (the BCL type this alias names), so `as_enum_handle` on it is sound.
    unsafe impl<T> crate::enumerate::ImplementsIEnumerable<T> for Handle<T> {}
    dotnet_generic_impl! {
        Handle<T> = [ASM] "System.Collections.Concurrent.ConcurrentQueue" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_enqueue = "Enqueue"(r, item: T as r#gen!(0));
        fn raw_count   = "get_Count"(r) -> i32 as i32;
        fn raw_empty   = "get_IsEmpty"(r) -> bool as bool;
    }

    /// A managed `System.Collections.Concurrent.ConcurrentQueue<T>` — a thread-safe FIFO queue.
    ///
    /// Removal (`TryDequeue`/`TryPeek`) is **not** exposed: both hand the element back through a .NET
    /// `out` parameter, which the current generic bridge cannot marshal (no by-ref `!N` argument). The
    /// supported pattern is *produce then drain by iteration* — enqueue on producers, then read the
    /// snapshot with `for x in &q` (each iteration takes a moment-in-time snapshot, as in C#). See the
    /// [module docs](super).
    pub struct ConcurrentQueue<T> {
        h: Handle<T>,
    }

    impl<T> ConcurrentQueue<T> {
        /// `new ConcurrentQueue<T>()`.
        pub fn new() -> Self {
            Self { h: raw_ctor::<T>() }
        }
        /// Number of elements at this instant (`Count` — a snapshot for a concurrent collection).
        pub fn len(&self) -> i32 {
            raw_count::<T>(self.h)
        }
        /// `true` if empty (`IsEmpty` — the lock-free property).
        pub fn is_empty(&self) -> bool {
            raw_empty::<T>(self.h)
        }
        /// Add `item` to the back (`Enqueue`).
        pub fn enqueue(&mut self, item: T) {
            raw_enqueue::<T>(self.h, item)
        }
        /// The raw managed handle, for advanced interop.
        pub fn handle(&self) -> Handle<T> {
            self.h
        }
    }

    impl<T> Default for ConcurrentQueue<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    // Iteration via the enumerator bridge — a moment-in-time snapshot, front-to-back (as in C#).
    impl<T> crate::enumerate::Enumerable<T> for ConcurrentQueue<T> {
        fn enumerable_handle(&self) -> crate::enumerate::IEnumerable<T> {
            crate::enumerate::as_enum_handle(self.h)
        }
    }
    impl<'a, T> IntoIterator for &'a ConcurrentQueue<T> {
        type Item = T;
        type IntoIter = crate::enumerate::Enumerator<T>;
        fn into_iter(self) -> Self::IntoIter {
            use crate::enumerate::Enumerable;
            self.iter_enumerator()
        }
    }

    impl<T> FromIterator<T> for ConcurrentQueue<T> {
        fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
            let mut q = Self::new();
            for item in iter {
                q.enqueue(item);
            }
            q
        }
    }

    impl<T> Extend<T> for ConcurrentQueue<T> {
        fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
            for item in iter {
                self.enqueue(item);
            }
        }
    }
}

mod concurrent_bag {
    use crate::{dotnet_generic, dotnet_generic_impl, r#gen};

    // See `concurrent_dictionary`: impl assembly is `System.Collections.Concurrent`.
    const ASM: &str = "System.Collections.Concurrent";

    dotnet_generic!(Handle<T> = [ASM] "System.Collections.Concurrent.ConcurrentBag" < (T,) >);
    // SAFETY-BEARING: `Handle<..>` really is a .NET class implementing `IEnumerable<..>` (the BCL type this alias names), so `as_enum_handle` on it is sound.
    unsafe impl<T> crate::enumerate::ImplementsIEnumerable<T> for Handle<T> {}
    dotnet_generic_impl! {
        Handle<T> = [ASM] "System.Collections.Concurrent.ConcurrentBag" < (T,) > ;
        ctor fn raw_ctor();
        fn raw_add   = "Add"(r, item: T as r#gen!(0));
        fn raw_count = "get_Count"(r) -> i32 as i32;
        fn raw_empty = "get_IsEmpty"(r) -> bool as bool;
    }

    /// A managed `System.Collections.Concurrent.ConcurrentBag<T>` — a thread-safe unordered bag,
    /// optimized for the case where the same thread both adds and removes.
    ///
    /// Removal (`TryTake`/`TryPeek`) is **not** exposed: both hand the element back through a .NET
    /// `out` parameter, which the current generic bridge cannot marshal (no by-ref `!N` argument). The
    /// supported pattern is *add then drain by iteration* — `add` on producers, then read the snapshot
    /// with `for x in &bag` (unordered, as in C#). See the [module docs](super).
    pub struct ConcurrentBag<T> {
        h: Handle<T>,
    }

    impl<T> ConcurrentBag<T> {
        /// `new ConcurrentBag<T>()`.
        pub fn new() -> Self {
            Self { h: raw_ctor::<T>() }
        }
        /// Number of elements at this instant (`Count` — a snapshot for a concurrent collection).
        pub fn len(&self) -> i32 {
            raw_count::<T>(self.h)
        }
        /// `true` if empty (`IsEmpty`).
        pub fn is_empty(&self) -> bool {
            raw_empty::<T>(self.h)
        }
        /// Add `item` to the bag (`Add`).
        pub fn add(&mut self, item: T) {
            raw_add::<T>(self.h, item)
        }
        /// The raw managed handle, for advanced interop.
        pub fn handle(&self) -> Handle<T> {
            self.h
        }
    }

    impl<T> Default for ConcurrentBag<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    // Iteration via the enumerator bridge — a moment-in-time snapshot, unordered (as in C#).
    impl<T> crate::enumerate::Enumerable<T> for ConcurrentBag<T> {
        fn enumerable_handle(&self) -> crate::enumerate::IEnumerable<T> {
            crate::enumerate::as_enum_handle(self.h)
        }
    }
    impl<'a, T> IntoIterator for &'a ConcurrentBag<T> {
        type Item = T;
        type IntoIter = crate::enumerate::Enumerator<T>;
        fn into_iter(self) -> Self::IntoIter {
            use crate::enumerate::Enumerable;
            self.iter_enumerator()
        }
    }

    impl<T> FromIterator<T> for ConcurrentBag<T> {
        fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
            let mut b = Self::new();
            for item in iter {
                b.add(item);
            }
            b
        }
    }

    impl<T> Extend<T> for ConcurrentBag<T> {
        fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
            for item in iter {
                self.add(item);
            }
        }
    }
}
