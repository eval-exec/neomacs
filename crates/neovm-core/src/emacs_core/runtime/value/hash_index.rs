//! The hash index of a Lisp hash table, and the `equal` hash it files keys
//! under.
//!
//! GNU keeps, per entry, the key object and the hash computed when the entry
//! was made (`h->hash[i]`, src/fns.c), and finds a key by
//! `EQ (key, HASH_KEY (h, i)) || (hash == HASH_HASH (h, i) && cmpfn (...))`
//! (`hash_find_with_hash`). The `equal` hash (`sxhash_obj`) looks at no more
//! than `SXHASH_MAX_DEPTH` levels and `SXHASH_MAX_LEN` elements per level, so
//! a lookup costs a bounded hash, and a hit on the very key object costs no
//! comparison at all.
//!
//! This index mirrors that: every entry stores its hash, and an `equal`
//! table's lookup hashes the Lisp value in place with [`equal_value_hash`]
//! (GNU's bounds) and compares candidates as GNU does -- identity first,
//! then the stored hash, then `equal` on the LIVE key object. The previous
//! index hashed and compared a full materialized [`HashKey`] tree, so a key
//! holding a 100,000-element vector cost 17.8M instructions per `gethash`
//! against GNU's 4.3K, a list longer than 200 elements was never found by
//! an equal copy, and a key mutated after insertion answered as the old
//! structure rather than the object GNU compares.
//!
//! The materialized [`HashKey`] is still stored beside each entry: the
//! iteration, dump and user-defined-test consumers read it. It no longer
//! decides `equal`-table membership.
use super::*;

/// GNU `SXHASH_MAX_DEPTH` (src/fns.c): the deepest level `sxhash_obj`
/// examines.
const SXHASH_MAX_DEPTH: u32 = 3;
/// GNU `SXHASH_MAX_LEN` (src/fns.c): elements of a list or vector hashed per
/// level.
const SXHASH_MAX_LEN: usize = 7;

/// Hash-stream tags the walker writes for shapes that have no [`HashKey`]
/// leaf of their own. Leaves write exactly what `HashKey::hash` writes for
/// the leaf key, so a leaf key is filed under the same hash whether it was
/// hashed from the key or from the value.
#[derive(Clone, Copy)]
#[repr(u8)]
enum EqualHashTag {
    List = 12,
    Vector = 13,
    Marker = 18,
    Overlay = 19,
    ByteCode = 22,
    Record = 24,
    CharTable = 25,
    SubCharTable = 26,
    Lambda = 27,
    /// A node past [`SXHASH_MAX_DEPTH`], which GNU folds in as 0.
    Beyond = 28,
}

impl EqualHashTag {
    #[inline(always)]
    fn write(self, hasher: &mut FxHasher) {
        (self as u8).hash(hasher);
    }
}

/// One entry of a table's hash index.
#[derive(Clone, Debug)]
pub(super) struct IndexEntry {
    /// The key's hash, computed once when the entry was made (GNU
    /// `h->hash[i]`): a resize never re-hashes, and a key mutated after
    /// insertion stays where it was filed.
    pub(super) hash: u64,
    /// The key as materialized when the entry was made.
    pub(super) key: HashKey,
    /// The entry's slot in the storage's slot vector.
    pub(super) slot: usize,
}

/// The index of a Lisp hash table: stored hash + materialized key -> slot.
#[derive(Clone, Debug, Default)]
pub(super) struct HashIndex {
    table: hashbrown::HashTable<IndexEntry>,
}

impl HashIndex {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            table: hashbrown::HashTable::with_capacity(capacity),
        }
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.table.len()
    }

    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub(super) fn capacity(&self) -> usize {
        self.table.capacity()
    }

    pub(super) fn clear(&mut self) {
        self.table.clear();
    }

    pub(super) fn reserve(&mut self, additional: usize) {
        self.table.reserve(additional, |entry| entry.hash);
    }

    pub(super) fn iter(&self) -> hashbrown::hash_table::Iter<'_, IndexEntry> {
        self.table.iter()
    }

    /// The entry filed under `hash` that `is_match` accepts.
    #[inline(always)]
    pub(super) fn find(
        &self,
        hash: u64,
        is_match: impl FnMut(&IndexEntry) -> bool,
    ) -> Option<&IndexEntry> {
        self.table.find(hash, is_match)
    }

    /// Unlink and return the entry filed under `hash` that `is_match`
    /// accepts.
    pub(super) fn remove_matching(
        &mut self,
        hash: u64,
        is_match: impl FnMut(&IndexEntry) -> bool,
    ) -> Option<IndexEntry> {
        match self.table.find_entry(hash, is_match) {
            Ok(entry) => Some(entry.remove().0),
            Err(_) => None,
        }
    }

    /// File a NEW entry; the caller has established that no entry matches.
    pub(super) fn insert_new(&mut self, hash: u64, key: HashKey, slot: usize) {
        self.table
            .insert_unique(hash, IndexEntry { hash, key, slot }, |entry| entry.hash);
    }

    /// The entry whose materialized key is `key`, when only the key is at
    /// hand. A key whose hash its materialized form decides is found by
    /// hash; any other (a structural `equal` key) by a scan, which is why
    /// the storage's own callers look structural keys up by VALUE.
    pub(super) fn find_key(&self, key: &HashKey) -> Option<&IndexEntry> {
        match key_index_hash(key) {
            Some(hash) => self.table.find(hash, |entry| entry.key == *key),
            None => self.scan_for_key(key),
        }
    }

    /// [`Self::find_key`], unlinking the entry.
    pub(super) fn remove_key(&mut self, key: &HashKey) -> Option<IndexEntry> {
        let hash = match key_index_hash(key) {
            Some(hash) => hash,
            None => self.scan_for_key(key)?.hash,
        };
        self.remove_matching(hash, |entry| entry.key == *key)
    }

    #[cold]
    #[inline(never)]
    fn scan_for_key(&self, key: &HashKey) -> Option<&IndexEntry> {
        self.table.iter().find(|entry| entry.key == *key)
    }

    /// Keep the entries `keep` accepts, in iteration order.
    pub(super) fn retain(&mut self, keep: impl FnMut(&mut IndexEntry) -> bool) {
        self.table.retain(keep);
    }
}

/// `FxHasher` over a [`HashKey`]'s own `Hash` stream.
#[inline]
pub(super) fn fx_hash_key(key: &HashKey) -> u64 {
    let mut hasher = FxHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}

impl HashKey {
    /// Whether an `equal` table files this key under the hash of the key
    /// OBJECT ([`equal_value_hash`]) rather than of this materialized form.
    ///
    /// These are the keys `to_equal_key` builds from structure; every other
    /// key is a leaf whose `Hash` stream the value walker reproduces.
    #[inline]
    pub(super) fn is_structural(&self) -> bool {
        matches!(
            self,
            HashKey::EqualCons(..)
                | HashKey::EqualVec(_)
                | HashKey::ByteCode(_)
                | HashKey::Marker(_)
                | HashKey::Overlay(_)
                | HashKey::BoolVec(_)
                | HashKey::SymbolWithPos(..)
                | HashKey::Cycle(_)
        )
    }
}

/// The hash an entry for `key` (whose key object is `key_value`) is filed
/// under.
#[inline]
pub(super) fn stored_hash(key: &HashKey, key_value: Value) -> u64 {
    if key.is_structural() {
        equal_value_hash(key_value)
    } else {
        fx_hash_key(key)
    }
}

/// The hash `key` is filed under, when the materialized key alone decides it:
/// every leaf, and the structural keys whose object the key describes
/// completely (a marker's logical fields, a packed bool-vector's bits).
pub(super) fn key_index_hash(key: &HashKey) -> Option<u64> {
    match key {
        HashKey::Marker(parts) => {
            let mut hasher = FxHasher::default();
            EqualHashTag::Marker.write(&mut hasher);
            parts.hash(&mut hasher);
            Some(hasher.finish())
        }
        HashKey::BoolVec(parts) => Some(bool_vector_key_hash(parts.0, parts.1)),
        key if key.is_structural() => None,
        key => Some(fx_hash_key(key)),
    }
}

/// What [`equal_value_hash`] computes for the `--bool-vector--` vector that
/// `bool_vector_equal_hash_key` packs into `HashKey::BoolVec((len, bits))`:
/// its length, then its first [`SXHASH_MAX_LEN`] slots (the tag symbol, the
/// bit count, and the leading bits as fixnums).
fn bool_vector_key_hash(len: usize, bits: u128) -> u64 {
    static TAG: OnceLock<SymId> = OnceLock::new();
    let tag = *TAG.get_or_init(|| intern("--bool-vector--"));
    let mut hasher = FxHasher::default();
    EqualHashTag::Vector.write(&mut hasher);
    (len + 2).hash(&mut hasher);
    HashKey::Symbol(tag).hash(&mut hasher);
    HashKey::Int(len as i64).hash(&mut hasher);
    for index in 0..len.min(SXHASH_MAX_LEN - 2) {
        HashKey::Int(i64::from(bits & (1_u128 << index) != 0)).hash(&mut hasher);
    }
    hasher.finish()
}

/// The hash an `equal` table files `value` under: GNU `sxhash_obj`
/// (src/fns.c), bounded to [`SXHASH_MAX_DEPTH`] levels and
/// [`SXHASH_MAX_LEN`] elements per level, hashing the object in place.
///
/// Two `equal` objects always hash alike -- every arm hashes only what
/// `equal` compares, or less -- and a position-carrying symbol hashes as its
/// bare symbol, so the hash does not depend on `symbols-with-pos-enabled`.
/// For a value whose `equal` key is a leaf (a number, symbol, string, or an
/// object keyed by identity) the hash is exactly `FxHasher` over that leaf
/// [`HashKey`]; see [`stored_hash`].
#[inline]
pub(crate) fn equal_value_hash(value: Value) -> u64 {
    // A symbol, fixnum or string -- what most `equal` tables are keyed by --
    // is one leaf write, without the walker's call.
    if !value.is_cons()
        && !value.is_veclike()
        && let Some(hasher) = probe_leaf(value, HashTableTest::Equal, false, FxHasher::default())
    {
        return hasher.finish();
    }
    equal_hash_obj(value, 0, FxHasher::default()).finish()
}

fn equal_hash_obj(value: Value, depth: u32, mut hasher: FxHasher) -> FxHasher {
    if depth > SXHASH_MAX_DEPTH {
        EqualHashTag::Beyond.write(&mut hasher);
        return hasher;
    }
    let identity = |mut hasher: FxHasher| {
        10u8.hash(&mut hasher);
        value.bits().hash(&mut hasher);
        hasher
    };
    match value.kind() {
        ValueKind::Cons => {
            EqualHashTag::List.write(&mut hasher);
            // GNU `sxhash_list`: the first SXHASH_MAX_LEN elements one level
            // down, then whatever tail is left, also one level down.
            let mut list = value;
            if depth < SXHASH_MAX_DEPTH {
                let mut taken = 0;
                while taken < SXHASH_MAX_LEN && list.is_cons() {
                    hasher = equal_hash_obj(list.cons_car(), depth + 1, hasher);
                    list = list.cons_cdr();
                    taken += 1;
                }
            }
            if !list.is_nil() {
                hasher = equal_hash_obj(list, depth + 1, hasher);
            }
            hasher
        }
        ValueKind::Veclike(kind) => match kind {
            VecLikeType::Vector | VecLikeType::Record | VecLikeType::CharTable => {
                let Some(view) = StructuralPseudovectorView::from_value(value, kind) else {
                    return identity(hasher);
                };
                let tag = match kind {
                    VecLikeType::Vector => EqualHashTag::Vector,
                    VecLikeType::Record => EqualHashTag::Record,
                    _ => EqualHashTag::CharTable,
                };
                // GNU `sxhash_vector`: seeded with the size, then the first
                // SXHASH_MAX_LEN slots one level down.
                tag.write(&mut hasher);
                view.len().hash(&mut hasher);
                for index in 0..view.len().min(SXHASH_MAX_LEN) {
                    hasher = equal_hash_obj(view.slot(index), depth + 1, hasher);
                }
                hasher
            }
            // GNU hashes every sub-char-table alike (42): it is not worth
            // looking into.
            VecLikeType::SubCharTable => {
                EqualHashTag::SubCharTable.write(&mut hasher);
                hasher
            }
            VecLikeType::SymbolWithPos => match value.as_symbol_with_pos_sym() {
                Some(symbol) => equal_hash_obj(symbol, depth, hasher),
                None => identity(hasher),
            },
            VecLikeType::Bignum => {
                value.bignum_hash_key().hash(&mut hasher);
                hasher
            }
            VecLikeType::Marker => {
                // `equal` compares the buffer and, in a buffer, the position.
                match crate::emacs_core::marker::marker_equal_logical_fields(&value) {
                    Some((buffer, bytepos)) => {
                        EqualHashTag::Marker.write(&mut hasher);
                        (buffer.map(|buffer| buffer.0), bytepos).hash(&mut hasher);
                        hasher
                    }
                    None => identity(hasher),
                }
            }
            // GNU folds in the overlay's start and end too. Those live in
            // the buffer's overlay tree, which a table hydrated from a dump
            // may be filed before the buffers are restored; the plist alone
            // keeps the hash a pure function of the object.
            VecLikeType::Overlay => match value.as_overlay_data() {
                Some(overlay) => {
                    EqualHashTag::Overlay.write(&mut hasher);
                    equal_hash_obj(overlay.plist, depth, hasher)
                }
                None => identity(hasher),
            },
            // Closures hash by kind alone. Reading a byte-code object's
            // slots would materialize a lazy dump stub, and an interpreted
            // closure's parameter list is parsed on first use -- hashing
            // must not change the object it hashes, least of all while a
            // dump is being loaded.
            VecLikeType::ByteCode => {
                EqualHashTag::ByteCode.write(&mut hasher);
                hasher
            }
            VecLikeType::Lambda => {
                EqualHashTag::Lambda.write(&mut hasher);
                hasher
            }
            // Everything else is `equal` only when `eq`.
            _ => identity(hasher),
        },
        // nil, t, fixnums, symbols, floats, strings, subrs: the leaf key's
        // own stream.
        _ => match probe_leaf(value, HashTableTest::Equal, false, hasher) {
            Some(hasher) => hasher,
            None => unreachable!("every non-cons, non-veclike value is an equal leaf"),
        },
    }
}
