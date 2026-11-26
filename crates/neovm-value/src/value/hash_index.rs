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
//! `puthash` of a cons, vector, record or string into an `equal` table,
//! with `symbols-with-pos-enabled` nil, stores no materialized key either:
//! the entry holds [`HashKey::LiveKey`] and its slot holds the key object,
//! as GNU `hash_put` stores `key` beside its hash. Materializing that key
//! copied it whole -- 4.8MB of allocation for one 100,000-element vector
//! key, where GNU stores one word. Every other key is still materialized
//! beside its entry for the iteration, dump and user-defined-test consumers
//! that read it; a reader that wants the materialized form of a live-key
//! entry builds it from the slot's key object. It no longer decides
//! `equal`-table membership.
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
    BoolVector = 29,
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
    /// The key as materialized when the entry was made, or
    /// [`HashKey::LiveKey`] when the entry's slot holds the key object it
    /// was filed by.
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

    pub(super) fn try_with_capacity(capacity: usize) -> Result<Self, crate::error::Flow> {
        let mut index = Self::default();
        index
            .table
            .try_reserve(capacity, |entry| entry.hash)
            .map_err(|_| crate::alloc::memory_exhausted())?;
        Ok(index)
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

    // Expose the index-only loan to the caller's capacity comparison, while
    // retaining hashbrown's usable-capacity changes when tombstones clear.
    #[inline]
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
        self.insert_new_with_growth(hash, key, slot, &mut |_| {});
    }

    /// Only an absent-key insertion can grow the index. Keep its capacity
    /// observation out of overwrites, and let hashbrown decide whether a
    /// tombstone can be reused instead of forcing an eager reserve.
    /// The shared generic body lets the unmetered reporter erase accounting,
    /// and keeps insertion and its capacity observations visible together.
    #[inline]
    pub(super) fn insert_new_with_growth(
        &mut self,
        hash: u64,
        key: HashKey,
        slot: usize,
        report: &mut impl FnMut(usize),
    ) {
        let before = self.table.capacity();
        self.table
            .insert_unique(hash, IndexEntry { hash, key, slot }, |entry| entry.hash);
        let after = self.table.capacity();
        // Byte scaling is needed only when usable capacity increases. This
        // includes tombstone reuse: capacity is items plus growth_left, so
        // testing only whether the table was full would lose existing charges.
        if after > before {
            let before_bytes = before.saturating_mul(size_of::<IndexEntry>());
            let after_bytes = after.saturating_mul(size_of::<IndexEntry>());
            let grown = after_bytes.saturating_sub(before_bytes);
            if grown != 0 {
                report(grown);
            }
        }
    }

    /// The entry whose materialized key is `key`, when only the key is at
    /// hand; `slots` holds the key objects of live-key entries. A key whose
    /// hash its materialized form decides is found by hash; any other (a
    /// structural `equal` key) by a scan, which is why the storage's own
    /// callers look structural keys up by VALUE. [`HashKey::LiveKey`] names
    /// no entry.
    pub(super) fn find_key(
        &self,
        key: &HashKey,
        slots: &[Option<HashTableEntry>],
    ) -> Option<&IndexEntry> {
        if matches!(key, HashKey::LiveKey) {
            return None;
        }
        match key_index_hash(key) {
            Some(hash) => self
                .table
                .find(hash, |entry| entry_has_key(entry, key, slots)),
            None => self.scan_for_key(key, slots),
        }
    }

    /// [`Self::find_key`], unlinking the entry.
    pub(super) fn remove_key(
        &mut self,
        key: &HashKey,
        slots: &[Option<HashTableEntry>],
    ) -> Option<IndexEntry> {
        let found = self.find_key(key, slots)?;
        let (hash, slot) = (found.hash, found.slot);
        self.remove_matching(hash, |entry| entry.slot == slot)
    }

    #[cold]
    #[inline(never)]
    fn scan_for_key(&self, key: &HashKey, slots: &[Option<HashTableEntry>]) -> Option<&IndexEntry> {
        self.table
            .iter()
            .find(|entry| entry_has_key(entry, key, slots))
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

/// Whether `entry` is the entry whose materialized key is `key`: for a
/// live-key entry, whether its key object, read from `slots`, materializes
/// to `key` under `equal` (only an `equal` table files live keys).
pub(super) fn entry_has_key(
    entry: &IndexEntry,
    key: &HashKey,
    slots: &[Option<HashTableEntry>],
) -> bool {
    match &entry.key {
        HashKey::LiveKey => slots
            .get(entry.slot)
            .and_then(Option::as_ref)
            .is_some_and(|stored| {
                live_key_can_be(stored.key, key)
                    && stored
                        .key
                        .to_equal_key_bounded(hash_key_node_count(key), false)
                        == *key
            }),
        stored => stored == key,
    }
}

/// The number of structural nodes a live key needs to materialize for `key`.
/// A scan candidate is bounded by the query's already-materialized shape.
fn hash_key_node_count(key: &HashKey) -> usize {
    let children = match key {
        HashKey::EqualCons(car, cdr) => {
            hash_key_node_count(car).saturating_add(hash_key_node_count(cdr))
        }
        HashKey::EqualVec(items) => items.iter().fold(0usize, |count, item| {
            count.saturating_add(hash_key_node_count(item))
        }),
        HashKey::ByteCode(parts) => parts.iter().fold(0usize, |count, part| {
            count.saturating_add(match part {
                ByteCodeKeyPart::Value(value) => hash_key_node_count(value),
                ByteCodeKeyPart::Values(values) => values.iter().fold(0usize, |nested, value| {
                    nested.saturating_add(hash_key_node_count(value))
                }),
                _ => 0,
            })
        }),
        HashKey::Overlay(parts) => hash_key_node_count(&parts.3),
        HashKey::SymbolWithPos(symbol, position) => {
            hash_key_node_count(symbol).saturating_add(hash_key_node_count(position))
        }
        _ => 0,
    };
    children.saturating_add(1)
}

/// Whether the key object `value` can materialize to `key` at all: the
/// cheap test that spares a scan materializing every live key it passes.
fn live_key_can_be(value: Value, key: &HashKey) -> bool {
    match key {
        HashKey::EqualCons(..) => value.is_cons(),
        HashKey::EqualVec(_) => value.is_veclike(),
        HashKey::StringContent(_) => value.is_string(),
        // A leaf, an identity key or a nested marker (`Cycle`): no cons,
        // vector, record or string materializes to one as a whole.
        _ => false,
    }
}

/// Whether an `equal` table files `value` by the key object itself
/// ([`HashKey::LiveKey`]) rather than by its materialized key: a cons,
/// vector, record or string, whose materialized key is a copy as large as
/// the object.
#[inline]
pub(super) fn keyed_by_live_object(value: Value) -> bool {
    matches!(
        value.kind(),
        ValueKind::Cons
            | ValueKind::String
            | ValueKind::Veclike(VecLikeType::Vector | VecLikeType::Record)
    )
}

impl HashKey {
    /// Whether an `equal` table files this key under the hash of the key
    /// OBJECT ([`equal_value_hash`]) rather than of this materialized form.
    ///
    /// These are the keys `to_equal_key` builds from structure, and the
    /// live-key marker; every other key is a leaf whose `Hash` stream the
    /// value walker reproduces.
    #[inline]
    pub(super) fn is_structural(&self) -> bool {
        matches!(
            self,
            HashKey::LiveKey
                | HashKey::EqualCons(..)
                | HashKey::EqualVec(_)
                | HashKey::ByteCode(_)
                | HashKey::Marker(_)
                | HashKey::Overlay(_)
                | HashKey::BoolVector(_)
                | HashKey::SymbolWithPos(..)
                | HashKey::Cycle(_)
        )
    }

    /// Whether this key stopped at [`super::TRUNCATED_KEY_CYCLE`] anywhere.
    ///
    /// A budgeted structural key (see
    /// [`crate::value::TaggedValue::to_equal_key_bounded`]) uses that
    /// sentinel where its node budget ran out.  Such a key still identifies
    /// the entry for membership, but it no longer describes the key object
    /// completely, so exact-key consumers must decline it.
    pub(crate) fn contains_truncation(&self) -> bool {
        match self {
            HashKey::Cycle(index) => *index == super::TRUNCATED_KEY_CYCLE,
            HashKey::EqualCons(car, cdr) => {
                if car.contains_truncation() {
                    return true;
                }
                let mut node = cdr.as_ref();
                while let HashKey::EqualCons(next_car, next_cdr) = node {
                    if next_car.contains_truncation() {
                        return true;
                    }
                    node = next_cdr.as_ref();
                }
                node.contains_truncation()
            }
            HashKey::EqualVec(keys) => keys.iter().any(HashKey::contains_truncation),
            HashKey::SymbolWithPos(sym, pos) => {
                sym.contains_truncation() || pos.contains_truncation()
            }
            HashKey::Overlay(parts) => parts.3.contains_truncation(),
            _ => false,
        }
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
        HashKey::BoolVector(parts) => Some(packed_bool_vector_hash(parts.0, &parts.1)),
        key if key.is_structural() => None,
        key => Some(fx_hash_key(key)),
    }
}

/// The stream [`equal_value_hash`] writes for a packed bool-vector: GNU
/// `sxhash_bool_vector` (src/fns.c), the size and then the first
/// [`SXHASH_MAX_LEN`] words. The trailing bits are zero, so `equal`
/// bool-vectors hash alike.
fn write_packed_bool_vector(hasher: &mut FxHasher, nbits: usize, words: &[u64]) {
    EqualHashTag::BoolVector.write(hasher);
    nbits.hash(hasher);
    for word in words.iter().take(SXHASH_MAX_LEN) {
        word.hash(hasher);
    }
}

/// [`equal_value_hash`] of a packed bool-vector, from its key.
fn packed_bool_vector_hash(nbits: usize, words: &[u64]) -> u64 {
    let mut hasher = FxHasher::default();
    write_packed_bool_vector(&mut hasher, nbits, words);
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
            VecLikeType::Vector
            | VecLikeType::Record
            | VecLikeType::TsNode
            | VecLikeType::CharTable => {
                let Some(view) = StructuralPseudovectorView::from_value(&value, kind) else {
                    return identity(hasher);
                };
                let tag = match kind {
                    VecLikeType::Vector => EqualHashTag::Vector,
                    // Preserve the old slot hash for native node wrappers;
                    // equality still distinguishes their intrinsic type.
                    VecLikeType::Record | VecLikeType::TsNode => EqualHashTag::Record,
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
            VecLikeType::BoolVector => match value.as_bool_vector_obj() {
                Some(bv) => {
                    write_packed_bool_vector(&mut hasher, bv.nbits, bv.words());
                    hasher
                }
                None => identity(hasher),
            },
            VecLikeType::Marker => {
                // `equal` compares the buffer and, in a buffer, the position.
                match super::marker_equal_logical_fields(&value) {
                    Some((buffer, bytepos)) => {
                        EqualHashTag::Marker.write(&mut hasher);
                        (buffer, bytepos).hash(&mut hasher);
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
