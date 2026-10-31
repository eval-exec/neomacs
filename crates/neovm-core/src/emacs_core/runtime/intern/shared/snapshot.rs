//! Typed immutable snapshot shape; its native-index payload is supplied by
//! the separate builder step. This is ownership, not yet a published pointer.
use super::super::{NameAtomRef, NameId, SymId, SymbolNameOrigin};
use super::facets::{HeapVisibleNames, ReaderTemplates};
use super::pages::{CacheProbe, CanonicalSlot, PagedRows, SnapshotError};
use std::sync::Arc;

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

/// Three independent clocks, supplied together at structural construction.
#[derive(Clone, Copy)]
pub(crate) struct SnapshotEpochs {
    pub(super) registry: RegistryEpoch,
    pub(super) materialization: MaterializationEpoch,
    pub(super) generation: DescriptorGeneration,
}

#[repr(transparent)]
struct HeapCount(u32);

#[repr(C)]
pub(crate) struct InternSnapshot<N> {
    symbol_span: super::pages::TableSpan<SymId>,
    name_span: super::pages::TableSpan<NameId>,
    native_slots: NativeSlotCount,
    heap_count: HeapCount,
    symbols: PagedRows<SymbolRow, SymId>,
    names: PagedRows<CanonicalSlot, NameId>,
    native: Arc<N>,
    templates: Arc<ReaderTemplates>,
    visible: Arc<HeapVisibleNames>,
    registry_epoch: RegistryEpoch,
    materialization_epoch: MaterializationEpoch,
    generation: DescriptorGeneration,
}
impl<N: NativeIndexShape> InternSnapshot<N> {
    /// From-scratch bootstrap/restore construction, with a full structural
    /// scan. Incremental updates use extending/invalidating instead. The
    /// producer's semantic coverage proof and retained ledger are later stages.
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
                Self::validate_symbol(row, name_span)?;
            }
        }
        for id in 0..name_span.len() {
            if let CacheProbe::Hit(slot) = names.probe(NameId(id as u32)) {
                Self::validate_canonical(*slot, symbol_span)?;
            }
        }
        if !templates.fits(symbol_span) || !visible.fits(symbol_span) {
            return Err(SnapshotError::OutsideSpan);
        }
        Self::assemble(
            symbols,
            names,
            Arc::new(native),
            Arc::new(templates),
            Arc::new(visible),
            SnapshotEpochs {
                registry: registry_epoch,
                materialization: materialization_epoch,
                generation,
            },
        )
    }

    fn assemble(
        symbols: PagedRows<SymbolRow, SymId>,
        names: PagedRows<CanonicalSlot, NameId>,
        native: Arc<N>,
        templates: Arc<ReaderTemplates>,
        visible: Arc<HeapVisibleNames>,
        epochs: SnapshotEpochs,
    ) -> Result<Self, SnapshotError> {
        let symbol_span = symbols.span();
        let name_span = names.span();
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
            symbols,
            names,
            native,
            templates,
            visible,
            registry_epoch: epochs.registry,
            materialization_epoch: epochs.materialization,
            generation: epochs.generation,
        })
    }

    fn validate_symbol(
        row: &SymbolRow,
        names: super::pages::TableSpan<NameId>,
    ) -> Result<(), SnapshotError> {
        if !names.contains(row.name()) {
            return Err(SnapshotError::NameOutsideSpan);
        }
        Ok(())
    }

    fn validate_canonical(
        slot: CanonicalSlot,
        symbols: super::pages::TableSpan<SymId>,
    ) -> Result<(), SnapshotError> {
        match slot.probe() {
            CacheProbe::Hit(id) if !symbols.contains(id) => Err(SnapshotError::OutsideSpan),
            CacheProbe::Hit(_) | CacheProbe::Miss => Ok(()),
        }
    }

    fn check_successor(&self, epochs: SnapshotEpochs) -> Result<(), SnapshotError> {
        if epochs.registry.0 < self.registry_epoch.0
            || epochs.materialization.0 < self.materialization_epoch.0
            || epochs.generation.0 <= self.generation.0
        {
            return Err(SnapshotError::NonMonotonicEpoch);
        }
        Ok(())
    }

    /// From-scratch try_new is reserved for bootstrap/restore. This path checks
    /// only the added rows: existing spans only grow, so the validated prefix
    /// and unchanged sparse facets stay valid. Native entries for new names
    /// can safely miss; authoritative native/facet producer coverage is still
    /// required before ReadySnapshot can be constructed in P9.2.
    pub(super) fn extending(
        &self,
        symbols: impl IntoIterator<Item = SymbolRow>,
        names: impl IntoIterator<Item = CanonicalSlot>,
        epochs: SnapshotEpochs,
    ) -> Result<Self, SnapshotError> {
        self.check_successor(epochs)?;
        let symbols: Vec<_> = symbols.into_iter().collect();
        let names: Vec<_> = names.into_iter().collect();
        let symbol_span = self.symbol_span.extending(symbols.len())?;
        let name_span = self.name_span.extending(names.len())?;
        for row in &symbols {
            Self::validate_symbol(row, name_span)?;
        }
        for slot in &names {
            Self::validate_canonical(*slot, symbol_span)?;
        }
        Self::assemble(
            self.symbols.extending(symbols)?,
            self.names.extending(names)?,
            Arc::clone(&self.native),
            Arc::clone(&self.templates),
            Arc::clone(&self.visible),
            epochs,
        )
    }

    /// Invalidation preserves all row identity facts and only weakens the old
    /// canonical facts. The caller supplies the corresponding prepared native
    /// index; this structural builder cannot yet mint a ReadySnapshot proof.
    pub(super) fn invalidating(
        &self,
        id: SymId,
        native_without_symbol: N,
        epochs: SnapshotEpochs,
    ) -> Result<Self, SnapshotError> {
        self.check_successor(epochs)?;
        if epochs.registry.0 == self.registry_epoch.0 {
            return Err(SnapshotError::NonMonotonicEpoch);
        }
        let mut row = match self.symbols.probe(id) {
            CacheProbe::Hit(row) => *row,
            CacheProbe::Miss => return Err(SnapshotError::OutsideSpan),
        };
        row.canonical = CanonicalClass::Uninterned;
        let names = match self.canonical(row.name()) {
            CacheProbe::Hit(canonical) if canonical == id => self
                .names
                .replacing(row.name(), CanonicalSlot::from_symbol(SymId(0)))?,
            CacheProbe::Hit(_) | CacheProbe::Miss => self.names.clone(),
        };
        Self::assemble(
            self.symbols.replacing(id, row)?,
            names,
            Arc::new(native_without_symbol),
            Arc::clone(&self.templates),
            Arc::clone(&self.visible),
            epochs,
        )
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
    assert!(std::mem::size_of::<InternSnapshot<()>>() == 112);
    assert!(std::mem::align_of::<InternSnapshot<()>>() == 8);
    assert!(std::mem::offset_of!(InternSnapshot<()>, symbols) == 16);
    assert!(std::mem::offset_of!(InternSnapshot<()>, names) == 40);
    assert!(std::mem::size_of::<PagedRows<SymbolRow, SymId>>() == 24);
    assert!(std::mem::align_of::<PagedRows<SymbolRow, SymId>>() == 8);
};
static_assertions::assert_impl_all!(SymbolRow: Send, Sync, Copy);

#[cfg(test)]
#[path = "tests/snapshot.rs"]
mod tests;
