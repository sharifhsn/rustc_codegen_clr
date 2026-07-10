use fxhash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::Entry, fmt::Debug, hash::Hash, marker::PhantomData, num::NonZeroU32,
    ops::Index,
};

/// Hash-conses `Value`s: `alloc` structurally dedups by value equality, so two equal
/// `Value`s always yield the same `Interned<Value>` key. This is load-bearing for the
/// optimizer (e.g. dedup passes rely on identical nodes already sharing one id).
#[derive(Serialize, Deserialize, Clone)]
pub struct BiMap<Value: Eq + Hash>(pub Vec<Value>, pub FxHashMap<Value, Interned<Value>>);
impl<Value: Eq + Hash + Clone> Default for BiMap<Value> {
    fn default() -> Self {
        Self(Vec::default(), FxHashMap::default())
    }
}
impl<Value: Eq + Hash + Clone + Debug> Index<Interned<Value>> for BiMap<Value> {
    type Output = Value;

    fn index(&self, index: Interned<Value>) -> &Self::Output {
        self.get(index)
    }
}

impl<Value: Eq + Hash + Clone + Debug> BiMap<Value> {
    /// Allocates a new Value and returns a Interned<Value>.
    pub fn alloc(&mut self, val: Value) -> Interned<Value> {
        match self.1.entry(val.clone()) {
            Entry::Occupied(key) => key.get().clone(),
            Entry::Vacant(empty) => {
                let key = Interned::from_index(
                    NonZeroU32::new(u32::try_from(self.0.len()).expect("Interned<Value> ID out of range") + 1)
                        .expect(
                            "Interned<Value> ID 0 when a non-zero value expected, this could be an overflow",
                        ),
                );

                empty.insert(key.clone());
                self.0.push(val);
                key
            }
        }
    }
    /// Gets an allocated value with id `key`
    // Interned<Value> is tiny(32 or 64 bit), so passing it by value makes sense
    #[allow(clippy::needless_pass_by_value)]
    pub fn get(&self, key: Interned<Value>) -> &Value {
        self.0.get(key.as_bimap_index().get() as usize - 1).unwrap()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the values in stable, interned-id order.
    #[must_use]
    pub fn values(&self) -> &[Value] {
        &self.0
    }

    /// Looks up the interned id for an already-allocated value.
    #[must_use]
    pub fn get_id(&self, value: &Value) -> Option<Interned<Value>> {
        self.1.get(value).copied()
    }

    /// Returns whether `value` has already been interned.
    #[must_use]
    pub fn contains_value(&self, value: &Value) -> bool {
        self.1.contains_key(value)
    }

    pub fn contais_val(&self, def: Value) -> bool {
        self.contains_value(&def)
    }

    /// Iterates over every allocated id in ascending, stable id order.
    pub fn ids(&self) -> impl ExactSizeIterator<Item = Interned<Value>> + DoubleEndedIterator {
        (0..self.0.len()).map(|zero_based| {
            let one_based = u32::try_from(zero_based)
                .expect("Interned<Value> ID out of range")
                .checked_add(1)
                .expect("Interned<Value> ID overflow");
            Interned::from_index(NonZeroU32::new(one_based).unwrap())
        })
    }

    /// Iterates over every `(id, value)` pair in ascending, stable id order.
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = (Interned<Value>, &Value)> + DoubleEndedIterator {
        self.ids().zip(self.values())
    }

    /// Backwards-compatible name for [`Self::ids`].
    pub fn iter_keys(&self) -> impl Iterator<Item = Interned<Value>> {
        self.ids()
    }

    pub fn map_values(&mut self, map: impl Fn(&mut Value)) {
        self.0.iter_mut().for_each(&map);
        self.1 = self
            .1
            .iter()
            .map(|(value, key)| {
                let mut value = value.clone();
                map(&mut value);
                (value, key.clone())
            })
            .collect();
    }
}
pub type BiMapIndex = NonZeroU32;
pub trait IntoBiMapIndex {
    fn from_index(val: BiMapIndex) -> Self;
    fn as_bimap_index(&self) -> BiMapIndex;
}
#[test]
fn bimap_alloc() {
    use crate::IString;
    let mut map = BiMap::<IString>::default();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
    let hi = map.alloc("Hi".into());
    assert!(!map.is_empty());
    assert_eq!(**map.get(hi), *"Hi");
    assert_eq!(map.len(), 1);
    let bob = map.alloc("Bob".into());
    assert_ne!(hi, bob);
    assert_eq!(**map.get(bob), *"Bob");
    assert_eq!(map.len(), 2);
    assert!(!map.is_empty());
}

#[test]
fn bimap_ids_values_and_iter_cover_every_entry() {
    let mut map = BiMap::<u32>::default();
    assert_eq!(map.ids().count(), 0);
    assert_eq!(map.iter().count(), 0);

    let ids: Vec<_> = [11, 22, 33]
        .into_iter()
        .map(|value| map.alloc(value))
        .collect();

    assert_eq!(map.ids().collect::<Vec<_>>(), ids);
    assert_eq!(map.iter_keys().collect::<Vec<_>>(), ids);
    assert_eq!(map.values(), &[11, 22, 33]);
    assert_eq!(
        map.iter()
            .map(|(id, value)| (id, *value))
            .collect::<Vec<_>>(),
        ids.iter()
            .copied()
            .zip([11, 22, 33])
            .collect::<Vec<_>>()
    );
    assert_eq!(map.get_id(&22), Some(ids[1]));
    assert_eq!(map.get_id(&44), None);
    assert!(map.contains_value(&33));
    assert!(!map.contains_value(&44));
}

#[test]
fn bimap_accessors_preserve_dedup_and_postcard_roundtrip() {
    let mut map = BiMap::<u32>::default();
    let mut first_ids = FxHashMap::default();
    for value in (0..128).map(|idx| (idx * 37) % 23) {
        let id = map.alloc(value);
        assert_eq!(*first_ids.entry(value).or_insert(id), id);
    }

    assert_eq!(map.len(), 23);
    for (id, value) in map.iter() {
        assert_eq!(map.get(id), value);
        assert_eq!(map.get_id(value), Some(id));
    }

    let bytes = postcard::to_allocvec(&map).unwrap();
    let decoded: BiMap<u32> = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.values(), map.values());
    assert_eq!(
        decoded.ids().collect::<Vec<_>>(),
        map.ids().collect::<Vec<_>>()
    );
    for (id, value) in decoded.iter() {
        assert_eq!(decoded.get_id(value), Some(id));
    }
}
/// A 1-based index into a `BiMap<T>`, tagged with `T` only via `PhantomData` — there is
/// no run-time type check, so an `Interned<A>` and `Interned<B>` sharing a bit pattern are
/// only distinguishable by which `BiMap` you index with them. `Copy` regardless of whether
/// `T` is, since it carries no `T` value, just the index.
#[derive(Hash, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Interned<T: ?Sized> {
    pd: PhantomData<T>,
    idx: BiMapIndex,
}
impl<T: ?Sized> Copy for Interned<T> {}

impl<T> Interned<T> {
    pub fn inner(&self) -> u32 {
        self.idx.get()
    }
}
impl<T: ?Sized> Clone for Interned<T> {
    fn clone(&self) -> Self {
        Self {
            pd: self.pd.clone(),
            idx: self.idx.clone(),
        }
    }
}
impl<T> IntoBiMapIndex for Interned<T> {
    fn from_index(idx: BiMapIndex) -> Self {
        Self {
            pd: PhantomData,
            idx,
        }
    }

    fn as_bimap_index(&self) -> BiMapIndex {
        self.idx
    }
}
