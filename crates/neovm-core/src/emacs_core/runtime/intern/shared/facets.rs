//! Separate process-lifetime reader templates and heap-specific name handles.
use super::super::{HeapNameWord, NameAtomRef, SymId};
use super::pages::{CacheProbe, SnapshotError, TableSpan};
use crate::tagged::gc::HeapIdentity;
use rustc_hash::FxHashMap;

pub(crate) struct ReaderTemplates {
    by_symbol: FxHashMap<SymId, NameAtomRef>,
}
impl ReaderTemplates {
    pub(crate) fn from_entries(
        span: TableSpan<SymId>,
        entries: impl IntoIterator<Item = (SymId, NameAtomRef)>,
    ) -> Result<Self, SnapshotError> {
        let mut by_symbol = FxHashMap::default();
        for (id, atom) in entries {
            if !span.contains(id) {
                return Err(SnapshotError::OutsideSpan);
            }
            if by_symbol.insert(id, atom).is_some() {
                return Err(SnapshotError::DuplicateTemplate);
            }
        }
        Ok(Self { by_symbol })
    }
    pub(crate) fn probe(&self, id: SymId) -> CacheProbe<NameAtomRef> {
        match self.by_symbol.get(&id) {
            Some(atom) => CacheProbe::Hit(*atom),
            None => CacheProbe::Miss,
        }
    }
    pub(super) fn fits(&self, span: TableSpan<SymId>) -> bool {
        self.by_symbol.keys().all(|id| span.contains(*id))
    }
}

/// No Value conversion is offered. P9.2 must supply a matching live-heap/name
/// authority before adding one; process publication is never that authority.
pub(crate) struct HeapVisibleName {
    heap: HeapIdentity,
    word: HeapNameWord,
}
impl HeapVisibleName {
    pub(crate) fn heap(&self) -> HeapIdentity {
        self.heap
    }
}
pub(crate) struct HeapVisibleNames {
    by_heap: FxHashMap<HeapIdentity, FxHashMap<SymId, HeapVisibleName>>,
}
impl HeapVisibleNames {
    pub(crate) fn from_entries(
        span: TableSpan<SymId>,
        entries: impl IntoIterator<Item = (HeapIdentity, SymId, HeapNameWord)>,
    ) -> Result<Self, SnapshotError> {
        let mut by_heap: FxHashMap<_, FxHashMap<_, _>> = FxHashMap::default();
        for (heap, symbol, word) in entries {
            if !span.contains(symbol) {
                return Err(SnapshotError::OutsideSpan);
            }
            let names = by_heap.entry(heap).or_default();
            if names
                .insert(symbol, HeapVisibleName { heap, word })
                .is_some()
            {
                return Err(SnapshotError::DuplicateHeapName);
            }
        }
        Ok(Self { by_heap })
    }
    pub(crate) fn probe(&self, heap: HeapIdentity, id: SymId) -> CacheProbe<&HeapVisibleName> {
        match self.by_heap.get(&heap).and_then(|names| names.get(&id)) {
            Some(name) => CacheProbe::Hit(name),
            None => CacheProbe::Miss,
        }
    }
    pub(super) fn heap_count(&self) -> usize {
        self.by_heap.len()
    }
    pub(super) fn fits(&self, span: TableSpan<SymId>) -> bool {
        self.by_heap
            .values()
            .all(|names| names.keys().all(|id| span.contains(*id)))
    }
}

static_assertions::assert_impl_all!(HeapVisibleNames: Send, Sync);
static_assertions::assert_impl_all!(ReaderTemplates: Send, Sync);

#[cfg(test)]
#[path = "tests/facets.rs"]
mod tests;
