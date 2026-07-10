use fxhash::hash64;
use hashbrown::HashTable;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::{hash::Hash, marker::PhantomData, num::NonZeroU32, ops::Index};

/// Hash-conses `Value`s: `alloc` structurally dedups by value equality, so two equal
/// `Value`s always yield the same `Interned<Value>` key. This is load-bearing for the
/// optimizer (e.g. dedup passes rely on identical nodes already sharing one id).
///
/// Values have exactly one owner: `values`. The raw hash table stores only stable ids; hashing and
/// equality (including collision resolution) dereference those ids into canonical `values`.
pub struct BiMap<Value: Eq + Hash> {
    values: Vec<Value>,
    index: HashTable<Interned<Value>>,
}

impl<Value: Eq + Hash> Default for BiMap<Value> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            index: HashTable::new(),
        }
    }
}

impl<Value: Eq + Hash + Clone> Clone for BiMap<Value> {
    fn clone(&self) -> Self {
        Self::from_values(self.values.clone())
            .expect("cloning a valid BiMap must preserve its uniqueness invariant")
    }
}

impl<Value: Eq + Hash + Serialize> Serialize for BiMap<Value> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.values.serialize(serializer)
    }
}

impl<'de, Value> Deserialize<'de> for BiMap<Value>
where
    Value: Deserialize<'de> + Eq + Hash,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let values = Vec::<Value>::deserialize(deserializer)?;
        Self::from_values(values).map_err(D::Error::custom)
    }
}

impl<Value: Eq + Hash> Index<Interned<Value>> for BiMap<Value> {
    type Output = Value;

    fn index(&self, index: Interned<Value>) -> &Self::Output {
        self.get(index)
    }
}

impl<Value: Eq + Hash> BiMap<Value> {
    /// Builds an interner from values already arranged in stable-id order.
    ///
    /// # Errors
    ///
    /// Rejects duplicate values instead of silently assigning two ids to one value.
    pub fn from_values(values: Vec<Value>) -> Result<Self, BiMapValidationError> {
        let index = Self::build_index(&values)?;
        Ok(Self { values, index })
    }

    /// Interns `val`, returning its stable id and whether it was newly inserted.
    pub fn intern_full(&mut self, val: Value) -> (Interned<Value>, bool) {
        let hash = hash64(&val);
        if let Some(id) = self.find_id(hash, &val) {
            return (id, false);
        }

        let id = Self::id_for_zero_based(self.values.len());
        self.values.push(val);
        let values = &self.values;
        self.index.insert_unique(hash, id, |stored| {
            hash64(&values[stored.inner() as usize - 1])
        });
        (id, true)
    }

    /// Interns `val`, returning its stable id. The value is moved into storage without cloning.
    pub fn intern(&mut self, val: Value) -> Interned<Value> {
        self.intern_full(val).0
    }

    /// Backwards-compatible name for [`Self::intern`].
    pub fn alloc(&mut self, val: Value) -> Interned<Value> {
        self.intern(val)
    }

    /// Gets an allocated value with id `key`
    // Interned<Value> is tiny(32 or 64 bit), so passing it by value makes sense
    #[allow(clippy::needless_pass_by_value)]
    pub fn get(&self, key: Interned<Value>) -> &Value {
        self.values
            .get(key.as_bimap_index().get() as usize - 1)
            .unwrap()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns the values in stable, interned-id order.
    #[must_use]
    pub fn values(&self) -> &[Value] {
        &self.values
    }

    /// Looks up the interned id for an already-allocated value.
    #[must_use]
    pub fn get_id(&self, value: &Value) -> Option<Interned<Value>> {
        self.find_id(hash64(value), value)
    }

    /// Returns whether `value` has already been interned.
    #[must_use]
    pub fn contains_value(&self, value: &Value) -> bool {
        self.get_id(value).is_some()
    }

    pub fn contais_val(&self, def: Value) -> bool {
        self.contains_value(&def)
    }

    /// Iterates over every allocated id in ascending, stable id order.
    pub fn ids(&self) -> impl ExactSizeIterator<Item = Interned<Value>> + DoubleEndedIterator {
        (0..self.values.len()).map(Self::id_for_zero_based)
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

    pub fn map_values(&mut self, map: impl Fn(&mut Value))
    where
        Value: Clone,
    {
        let mut transformed = self.values.clone();
        transformed.iter_mut().for_each(map);
        let index = Self::build_index(&transformed).unwrap_or_else(|error| {
            panic!("BiMap::map_values violated the interner uniqueness invariant: {error}")
        });
        self.values = transformed;
        self.index = index;
    }

    /// Validates that the id-only hash index exactly describes the canonical value storage.
    pub fn validate(&self) -> Result<(), BiMapValidationError> {
        Self::build_index(&self.values)?;
        if self.index.len() != self.values.len() {
            return Err(BiMapValidationError::IndexOutOfSync);
        }
        for (id, value) in self.iter() {
            if self.find_id(hash64(value), value) != Some(id) {
                return Err(BiMapValidationError::IndexOutOfSync);
            }
        }
        Ok(())
    }

    /// Rebuilds the id-only hash index from canonical value storage.
    ///
    /// # Errors
    ///
    /// Rejects duplicate values. The old index remains intact on failure.
    pub fn rebuild_index(&mut self) -> Result<(), BiMapValidationError> {
        let rebuilt = Self::build_index(&self.values)?;
        self.index = rebuilt;
        Ok(())
    }

    /// Truncates canonical storage and rebuilds its index. Existing ids below `len` stay stable.
    pub fn truncate(&mut self, len: usize) {
        self.values.truncate(len);
        self.rebuild_index()
            .expect("truncating a valid BiMap cannot introduce duplicate values");
    }

    /// Removes all values and ids.
    pub fn clear(&mut self) {
        self.values.clear();
        self.index.clear();
    }

    fn find_id(&self, hash: u64, value: &Value) -> Option<Interned<Value>> {
        self.index
            .find(hash, |id: &Interned<Value>| self.get(*id) == value)
            .copied()
    }

    fn build_index(values: &[Value]) -> Result<HashTable<Interned<Value>>, BiMapValidationError> {
        let mut index: HashTable<Interned<Value>> = HashTable::new();
        for (zero_based, value) in values.iter().enumerate() {
            let id = Self::id_for_zero_based(zero_based);
            let hash = hash64(value);
            if let Some(first) = index.find(hash, |existing: &Interned<Value>| {
                values[existing.inner() as usize - 1] == *value
            }) {
                return Err(BiMapValidationError::DuplicateValue {
                    first: first.inner(),
                    duplicate: id.inner(),
                });
            }
            index.insert_unique(hash, id, |stored| {
                hash64(&values[stored.inner() as usize - 1])
            });
        }
        Ok(index)
    }

    fn id_for_zero_based(zero_based: usize) -> Interned<Value> {
        let one_based = u32::try_from(zero_based)
            .expect("Interned<Value> ID out of range")
            .checked_add(1)
            .expect("Interned<Value> ID overflow");
        Interned::from_index(NonZeroU32::new(one_based).unwrap())
    }
}

/// Structural validation failure for a [`BiMap`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BiMapValidationError {
    /// Canonical storage contains the same value under two ids.
    DuplicateValue {
        /// First stable id containing the value.
        first: u32,
        /// Later duplicate id.
        duplicate: u32,
    },
    /// The hash index does not exactly match canonical storage.
    IndexOutOfSync,
}

impl std::fmt::Display for BiMapValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateValue { first, duplicate } => write!(
                f,
                "duplicate serialized/interned value at ids {first} and {duplicate}"
            ),
            Self::IndexOutOfSync => f.write_str("id-only hash index is out of sync with values"),
        }
    }
}

impl std::error::Error for BiMapValidationError {}
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
        ids.iter().copied().zip([11, 22, 33]).collect::<Vec<_>>()
    );
    assert_eq!(map.get_id(&22), Some(ids[1]));
    assert_eq!(map.get_id(&44), None);
    assert!(map.contains_value(&33));
    assert!(!map.contains_value(&44));
}

#[test]
fn bimap_accessors_preserve_dedup_and_postcard_roundtrip() {
    use fxhash::FxHashMap;

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

#[test]
fn bimap_interns_non_clone_values_and_handles_hash_collisions() {
    #[derive(Debug, Eq, PartialEq)]
    struct NonClone(u32);

    impl Hash for NonClone {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            // Deliberately force every value into one collision bucket.
            0_u8.hash(state);
        }
    }

    let mut map = BiMap::<NonClone>::default();
    let (first, inserted) = map.intern_full(NonClone(10));
    assert!(inserted);
    let (second, inserted) = map.intern_full(NonClone(20));
    assert!(inserted);
    let (first_again, inserted) = map.intern_full(NonClone(10));
    assert!(!inserted);
    assert_eq!(first_again, first);
    assert_ne!(first, second);
    assert_eq!(map.get_id(&NonClone(20)), Some(second));
    assert_eq!(map.get(first).0, 10);
    assert_eq!(map.get(second).0, 20);
    map.validate().unwrap();
}

#[test]
fn bimap_deserialize_rejects_duplicate_values() {
    let duplicate_values = postcard::to_allocvec(&vec![7_u32, 9, 7]).unwrap();
    let error = postcard::from_bytes::<BiMap<u32>>(&duplicate_values)
        .err()
        .unwrap();
    assert_eq!(error, postcard::Error::SerdeDeCustom);
}

#[test]
fn bimap_last_id_and_truncate_remain_stable() {
    let mut map = BiMap::<u32>::default();
    let ids: Vec<_> = [10, 20, 30, 40]
        .into_iter()
        .map(|value| map.intern(value))
        .collect();

    assert_eq!(map.ids().next_back(), Some(ids[3]));
    assert_eq!(map.iter().next_back(), Some((ids[3], &40)));
    map.truncate(3);
    assert_eq!(map.ids().collect::<Vec<_>>(), ids[..3]);
    assert_eq!(map.values(), &[10, 20, 30]);
    assert_eq!(map.get_id(&40), None);
    map.validate().unwrap();
}

#[test]
fn bimap_serializes_only_canonical_values_and_rebuilds_index() {
    let mut map = BiMap::<[u8; 32]>::default();
    for byte in 0..8 {
        map.intern([byte; 32]);
    }

    // The index contains one small id per canonical value, never another owned Value.
    let _: &HashTable<Interned<[u8; 32]>> = &map.index;
    assert_eq!(map.index.len(), map.len());
    let map_bytes = postcard::to_allocvec(&map).unwrap();
    let value_bytes = postcard::to_allocvec(&map.values).unwrap();
    assert_eq!(map_bytes, value_bytes);

    let decoded: BiMap<[u8; 32]> = postcard::from_bytes(&map_bytes).unwrap();
    assert_eq!(decoded.values(), map.values());
    assert_eq!(
        decoded.ids().collect::<Vec<_>>(),
        map.ids().collect::<Vec<_>>()
    );
    decoded.validate().unwrap();
}

#[test]
fn bimap_map_values_fails_loudly_on_transformed_duplicates() {
    let mut map = BiMap::<u32>::default();
    map.intern(1);
    map.intern(2);

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        map.map_values(|value| *value = 0);
    }));
    let panic = panic.expect_err("duplicate-producing map_values must panic");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("unknown panic");
    assert!(message.contains("uniqueness invariant"));
    assert!(message.contains("ids 1 and 2"));
    assert_eq!(map.values(), &[1, 2]);
    map.validate().unwrap();
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
