//! Immutable native UTF-8 positive index. The live registry remains authority;
//! this builder cannot mint ReadySnapshot or retain a caller's query string.
use super::super::{NameAtomRef, NativeCacheQuery, SymId, SymbolNameCharacterCount};
use super::pages::CacheProbe;
use super::snapshot::{
    CanonicalClass, NativeIndexShape, NativeQueryClass, NativeSlotCount, SymbolRow,
};
use std::sync::Arc;

const FANOUT: usize = 16;
const BUCKET_ENTRIES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeAtomRefusal {
    InvalidUtf8,
    CharacterCountMismatch,
}

/// Frozen identity atom whose bytes AND declared character count are in the
/// ordinary &str query domain. Multibyte flags do not decide name identity.
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub(crate) struct NativeUtf8Atom(NameAtomRef);
impl NativeUtf8Atom {
    pub(super) fn try_from_atom(atom: NameAtomRef) -> Result<Self, NativeAtomRefusal> {
        let text = atom.0.as_utf8_str().ok_or(NativeAtomRefusal::InvalidUtf8)?;
        if SymbolNameCharacterCount::from(atom.0) != SymbolNameCharacterCount::from(text) {
            return Err(NativeAtomRefusal::CharacterCountMismatch);
        }
        Ok(Self(atom))
    }

    pub(crate) fn as_str(self) -> &'static str {
        // SAFETY: the only constructor validated UTF-8, and NameAtomRef owns
        // the existing frozen, property-free, process-lifetime storage proof.
        unsafe { std::str::from_utf8_unchecked(self.0.0.as_bytes()) }
    }

    fn matches(self, query: &NativeCacheQuery<'_>) -> bool {
        self.as_str() == query.name
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeIndexError {
    Atom(NativeAtomRefusal),
    Uninterned,
    ExactOnly,
    ConflictingCanonical,
    Capacity,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HashFragment(u32);
#[derive(Clone, Copy)]
struct NativeHash(u64);
impl NativeHash {
    fn of(query: &NativeCacheQuery<'_>) -> Self {
        // Reuse the live cache's paired query/hash rule; never hash a byte
        // slice instead of &str, or invent a separate native hash function.
        Self(query.hash)
    }
    fn fragment(self) -> HashFragment {
        HashFragment(self.0 as u32)
    }
}

#[derive(Clone, Copy)]
struct BranchIndex(u8);
#[derive(Clone, Copy)]
struct HashDepth(u8);
impl HashDepth {
    const ROOT: Self = Self(0);
    fn descend(self, hash: NativeHash) -> Option<(BranchIndex, Self)> {
        if self.0 == 16 {
            return None;
        }
        Some((
            BranchIndex(((hash.0 >> (u32::from(self.0) * 4)) & 15) as u8),
            Self(self.0 + 1),
        ))
    }
}
impl BranchIndex {
    fn offset(self) -> usize {
        usize::from(self.0)
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct NativeEntry {
    atom: NativeUtf8Atom,
    symbol: SymId,
    hash: HashFragment,
}
impl NativeEntry {
    fn from_row(symbol: SymId, row: &SymbolRow) -> Result<Self, NativeIndexError> {
        match row.canonical() {
            CanonicalClass::Uninterned => return Err(NativeIndexError::Uninterned),
            CanonicalClass::Canonical => {}
        }
        match row.native_class() {
            NativeQueryClass::ExactOnly => return Err(NativeIndexError::ExactOnly),
            NativeQueryClass::NativeUtf8 => {}
        }
        let atom = NativeUtf8Atom::try_from_atom(row.atom()).map_err(NativeIndexError::Atom)?;
        let hash = NativeHash::of(&NativeCacheQuery::from(atom.as_str())).fragment();
        Ok(Self { atom, symbol, hash })
    }
    fn matches(self, query: &NativeCacheQuery<'_>) -> bool {
        self.hash == NativeHash::of(query).fragment() && self.atom.matches(query)
    }
}

#[derive(Clone)]
enum Node {
    Bucket(Arc<[NativeEntry]>),
    Directory(Box<[Option<Arc<Node>>; FANOUT]>),
}

/// A root plus typed count; readers borrow the immutable nodes and never touch
/// their reference counts. Persistent updates copy one bounded directory path
/// and at most eight old bucket rows. Hash exhaustion is a typed capacity
/// refusal, never an unbounded collision bucket or recursion.
#[derive(Clone)]
pub(crate) struct NativeIndex {
    root: Option<Arc<Node>>,
    entries: NativeSlotCount,
}
impl NativeIndex {
    pub(crate) fn probe(&self, name: &str) -> CacheProbe<SymId> {
        let query = NativeCacheQuery::from(name);
        let hash = NativeHash::of(&query);
        let mut depth = HashDepth::ROOT;
        let mut next = self.root.as_deref();
        while let Some(node) = next {
            match node {
                Node::Bucket(entries) => {
                    return entries
                        .iter()
                        .find(|entry| entry.matches(&query))
                        .map_or(CacheProbe::Miss, |entry| CacheProbe::Hit(entry.symbol));
                }
                Node::Directory(children) => {
                    let Some((branch, child_depth)) = depth.descend(hash) else {
                        return CacheProbe::Miss;
                    };
                    next = children[branch.offset()].as_deref();
                    depth = child_depth;
                }
            }
        }
        CacheProbe::Miss
    }

    pub(crate) fn builder(&self) -> NativeIndexBuilder {
        NativeIndexBuilder {
            index: self.clone(),
        }
    }

    /// Remove only the matching canonical identity. A delayed invalidation
    /// of an old SymId must not remove a successor with the same spelling.
    pub(super) fn without_symbol(
        &self,
        atom: NativeUtf8Atom,
        id: SymId,
    ) -> Result<Self, NativeIndexError> {
        match self.probe(atom.as_str()) {
            CacheProbe::Hit(found) if found == id => {}
            CacheProbe::Hit(_) | CacheProbe::Miss => return Ok(self.clone()),
        }
        let count = self
            .entries
            .len()
            .checked_sub(1)
            .ok_or(NativeIndexError::Capacity)?;
        let entries =
            NativeSlotCount::try_from_len(count).map_err(|_| NativeIndexError::Capacity)?;
        let query = NativeCacheQuery::from(atom.as_str());
        Ok(Self {
            root: removing(self.root.as_ref(), &query, HashDepth::ROOT),
            entries,
        })
    }
}
impl NativeIndexShape for NativeIndex {
    fn entry_count(&self) -> NativeSlotCount {
        self.entries
    }
}

/// Cold, exclusively owned construction. Row classification and atom-domain
/// validation run before structural mutation. Complete authoritative span
/// coverage and publication still belong to P9.2's typed producer adapter.
pub(crate) struct NativeIndexBuilder {
    index: NativeIndex,
}
impl NativeIndexBuilder {
    pub(crate) fn new() -> Self {
        Self {
            index: NativeIndex {
                root: None,
                entries: NativeSlotCount::EMPTY,
            },
        }
    }
    pub(super) fn insert(&mut self, id: SymId, row: &SymbolRow) -> Result<(), NativeIndexError> {
        let entry = NativeEntry::from_row(id, row)?;
        let query = NativeCacheQuery::from(entry.atom.as_str());
        match self.index.probe(query.name) {
            CacheProbe::Hit(found) if found == id => return Ok(()),
            CacheProbe::Hit(_) => return Err(NativeIndexError::ConflictingCanonical),
            CacheProbe::Miss => {}
        }
        let count = self
            .index
            .entries
            .len()
            .checked_add(1)
            .ok_or(NativeIndexError::Capacity)?;
        let entries =
            NativeSlotCount::try_from_len(count).map_err(|_| NativeIndexError::Capacity)?;
        let root = inserting(
            self.index.root.as_ref(),
            entry,
            NativeHash::of(&query),
            HashDepth::ROOT,
        )?;
        self.index = NativeIndex {
            root: Some(root),
            entries,
        };
        Ok(())
    }
    pub(crate) fn build(self) -> NativeIndex {
        self.index
    }
}

fn inserting(
    node: Option<&Arc<Node>>,
    entry: NativeEntry,
    hash: NativeHash,
    depth: HashDepth,
) -> Result<Arc<Node>, NativeIndexError> {
    let Some(node) = node else {
        return Ok(Arc::new(Node::Bucket(Arc::from(vec![entry]))));
    };
    match &**node {
        Node::Bucket(entries) if entries.len() < BUCKET_ENTRIES => {
            let mut changed = entries.to_vec();
            changed.push(entry);
            Ok(Arc::new(Node::Bucket(Arc::from(changed))))
        }
        Node::Bucket(entries) => {
            let mut children = Box::new(std::array::from_fn(|_| None));
            // A full bucket splits by the next hash nibble. Rehashing at most
            // eight validated atoms uses the same NativeCacheQuery definition.
            for old in entries.iter().copied().chain(std::iter::once(entry)) {
                let old_hash = NativeHash::of(&NativeCacheQuery::from(old.atom.as_str()));
                let (branch, next) = depth.descend(old_hash).ok_or(NativeIndexError::Capacity)?;
                children[branch.offset()] = Some(inserting(
                    children[branch.offset()].as_ref(),
                    old,
                    old_hash,
                    next,
                )?);
            }
            Ok(Arc::new(Node::Directory(children)))
        }
        Node::Directory(children) => {
            let (branch, next) = depth.descend(hash).ok_or(NativeIndexError::Capacity)?;
            let mut changed = children.clone();
            changed[branch.offset()] = Some(inserting(
                children[branch.offset()].as_ref(),
                entry,
                hash,
                next,
            )?);
            Ok(Arc::new(Node::Directory(changed)))
        }
    }
}

fn removing(
    node: Option<&Arc<Node>>,
    query: &NativeCacheQuery<'_>,
    depth: HashDepth,
) -> Option<Arc<Node>> {
    let node = node?;
    match &**node {
        Node::Bucket(entries) => {
            let changed: Vec<_> = entries
                .iter()
                .copied()
                .filter(|entry| !entry.matches(query))
                .collect();
            if changed.is_empty() {
                None
            } else {
                Some(Arc::new(Node::Bucket(Arc::from(changed))))
            }
        }
        Node::Directory(children) => {
            let (branch, next) = depth.descend(NativeHash::of(query))?;
            let mut changed = children.clone();
            changed[branch.offset()] = removing(children[branch.offset()].as_ref(), query, next);
            if changed.iter().all(Option::is_none) {
                None
            } else {
                Some(Arc::new(Node::Directory(changed)))
            }
        }
    }
}

static_assertions::assert_impl_all!(NativeIndex: Send, Sync, Clone);
static_assertions::assert_impl_all!(NativeUtf8Atom: Send, Sync, Copy);
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const _: () = {
    assert!(std::mem::size_of::<NativeUtf8Atom>() == 8);
    assert!(std::mem::align_of::<NativeUtf8Atom>() == 8);
    assert!(std::mem::size_of::<NativeEntry>() == 16);
    assert!(std::mem::align_of::<NativeEntry>() == 8);
    assert!(std::mem::offset_of!(NativeEntry, symbol) == 8);
    assert!(std::mem::offset_of!(NativeEntry, hash) == 12);
};

#[cfg(test)]
#[path = "tests/native.rs"]
mod tests;
