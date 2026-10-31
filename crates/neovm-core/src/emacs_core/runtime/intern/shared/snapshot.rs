//! Typed immutable snapshot shape; its native-index payload is supplied by
//! the separate builder step. This is ownership, not yet a published pointer.
use super::super::{NameAtomRef, NameId, SymId, SymbolNameOrigin};
use super::facets::{HeapVisibleNames, ReaderTemplates};
use super::pages::{CacheProbe, CanonicalSlot, PagedRows, SnapshotError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CanonicalClass {
    Canonical,
    Uninterned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum AtomKeyword {
    Colon,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum NativeQueryClass {
    NativeUtf8,
    ExactOnly,
}

/// Keyword is an immutable-atom hint, never proof for Lisp keywordp on a
/// mutable visible name. Origin uses the canonical registry's one enum.
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct SymbolRow {
    atom: NameAtomRef,
    name: NameId,
    canonical: CanonicalClass,
    keyword: AtomKeyword,
    origin: SymbolNameOrigin,
    native: NativeQueryClass,
}
impl SymbolRow {
    pub(super) fn new(
        atom: NameAtomRef,
        name: NameId,
        canonical: CanonicalClass,
        origin: SymbolNameOrigin,
        native: NativeQueryClass,
    ) -> Self {
        let keyword = match atom.0.as_bytes().first() {
            Some(b':') => AtomKeyword::Colon,
            Some(_) | None => AtomKeyword::Other,
        };
        Self {
            atom,
            name,
            canonical,
            keyword,
            origin,
            native,
        }
    }
    pub(crate) fn atom(&self) -> NameAtomRef {
        self.atom
    }
    pub(crate) fn name(&self) -> NameId {
        self.name
    }
    pub(crate) fn canonical(&self) -> CanonicalClass {
        self.canonical
    }
    pub(crate) fn atom_keyword(&self) -> AtomKeyword {
        self.keyword
    }
    pub(crate) fn origin(&self) -> SymbolNameOrigin {
        self.origin
    }
    pub(crate) fn native_class(&self) -> NativeQueryClass {
        self.native
    }
}

macro_rules! clock {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(transparent)]
        pub(crate) struct $name(u64);
        impl $name {
            pub(super) const fn from_published(value: u64) -> Self {
                Self(value)
            }
            pub(crate) fn checked_successor(self) -> Option<Self> {
                self.0.checked_add(1).map(Self)
            }
        }
    };
}
clock!(RegistryEpoch);
clock!(MaterializationEpoch);
clock!(DescriptorGeneration);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct NativeSlotCount(u32);
impl NativeSlotCount {
    pub(crate) fn try_from_len(len: usize) -> Result<Self, SnapshotError> {
        Ok(Self(
            len.try_into().map_err(|_| SnapshotError::SpanOverflow)?,
        ))
    }
    pub(crate) fn len(self) -> usize {
        self.0 as usize
    }
}
/// Counts come from an immutable native-index owner, not a caller's guess.
pub(crate) trait NativeIndexShape {
    fn entry_count(&self) -> NativeSlotCount;
}

#[repr(transparent)]
struct HeapCount(u32);

#[repr(C)]
pub(crate) struct InternSnapshot<N> {
    symbol_span: super::pages::TableSpan<SymId>,
    name_span: super::pages::TableSpan<NameId>,
    native_slots: NativeSlotCount,
    heap_count: HeapCount,
    symbols: Box<PagedRows<SymbolRow, SymId>>,
    names: Box<PagedRows<CanonicalSlot, NameId>>,
    native: Box<N>,
    templates: Box<ReaderTemplates>,
    visible: Box<HeapVisibleNames>,
    registry_epoch: RegistryEpoch,
    materialization_epoch: MaterializationEpoch,
    generation: DescriptorGeneration,
}
impl<N: NativeIndexShape> InternSnapshot<N> {
    /// Structural completeness only. The producer's authority/semantic
    /// coverage proof and retained-ledger publication are later stages.
    pub(super) fn try_new(
        symbols: PagedRows<SymbolRow, SymId>,
        names: PagedRows<CanonicalSlot, NameId>,
        native: N,
        templates: ReaderTemplates,
        visible: HeapVisibleNames,
        registry_epoch: RegistryEpoch,
        materialization_epoch: MaterializationEpoch,
        generation: DescriptorGeneration,
    ) -> Result<Self, SnapshotError> {
        let symbol_span = symbols.span();
        let name_span = names.span();
        for id in 0..symbol_span.len() {
            if let CacheProbe::Hit(row) = symbols.probe(SymId(id as u32)) {
                if !name_span.contains(row.name()) {
                    return Err(SnapshotError::NameOutsideSpan);
                }
            }
        }
        for id in 0..name_span.len() {
            if let CacheProbe::Hit(slot) = names.probe(NameId(id as u32)) {
                if let CacheProbe::Hit(symbol) = slot.probe() {
                    if !symbol_span.contains(symbol) {
                        return Err(SnapshotError::OutsideSpan);
                    }
                }
            }
        }
        if !templates.fits(symbol_span) || !visible.fits(symbol_span) {
            return Err(SnapshotError::OutsideSpan);
        }
        let native_slots = native.entry_count();
        let heap_count = HeapCount(
            visible
                .heap_count()
                .try_into()
                .map_err(|_| SnapshotError::SpanOverflow)?,
        );
        Ok(Self {
            symbol_span,
            name_span,
            native_slots,
            heap_count,
            symbols: Box::new(symbols),
            names: Box::new(names),
            native: Box::new(native),
            templates: Box::new(templates),
            visible: Box::new(visible),
            registry_epoch,
            materialization_epoch,
            generation,
        })
    }
    pub(crate) fn symbol(&self, id: SymId) -> CacheProbe<&SymbolRow> {
        self.symbols.probe(id)
    }
    pub(crate) fn canonical(&self, id: NameId) -> CacheProbe<SymId> {
        match self.names.probe(id) {
            CacheProbe::Hit(slot) => slot.probe(),
            CacheProbe::Miss => CacheProbe::Miss,
        }
    }
    pub(crate) fn native(&self) -> &N {
        &self.native
    }
    pub(crate) fn templates(&self) -> &ReaderTemplates {
        &self.templates
    }
    pub(crate) fn visible(&self) -> &HeapVisibleNames {
        &self.visible
    }
}

/// No public constructor yet: P9.2 must prove authoritative coverage before
/// a snapshot can be classified ready for actual process publication.
pub(crate) struct ReadySnapshot<N>(InternSnapshot<N>);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeferredPublication {
    NeedGrace,
    Capacity,
}
pub(crate) enum PreparedSnapshot<N> {
    ReadySnapshot(ReadySnapshot<N>),
    DeferredPublication(DeferredPublication),
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const _: () = {
    assert!(std::mem::size_of::<SymbolRow>() == 16);
    assert!(std::mem::align_of::<SymbolRow>() == 8);
    assert!(std::mem::offset_of!(SymbolRow, name) == 8);
    assert!(std::mem::offset_of!(SymbolRow, canonical) == 12);
    assert!(std::mem::offset_of!(SymbolRow, keyword) == 13);
    assert!(std::mem::offset_of!(SymbolRow, origin) == 14);
    assert!(std::mem::offset_of!(SymbolRow, native) == 15);
    assert!(std::mem::size_of::<InternSnapshot<()>>() == 80);
    assert!(std::mem::align_of::<InternSnapshot<()>>() == 8);
};
static_assertions::assert_impl_all!(SymbolRow: Send, Sync, Copy);

#[cfg(test)]
#[path = "tests/snapshot.rs"]
mod tests;
