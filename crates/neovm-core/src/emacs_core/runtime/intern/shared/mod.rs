//! Dormant shared intern-cache vocabulary. No production cache reads or writes
//! use this module yet. P9.1b supplies retention ownership before publication;
//! P9.2 supplies authoritative producer coverage, and P9.3 switches readers.
//!
//! Heap-name words here are nonowning metadata, never GC roots or permission
//! to dereference a Lisp object. Frozen atoms retain their existing owner proof.

mod facets;
mod pages;
mod snapshot;

pub(crate) use facets::{HeapVisibleNames, ReaderTemplates};
pub(crate) use pages::{CacheProbe, CanonicalSlot, PagedRows, SnapshotError, TableSpan};
pub(crate) use snapshot::{
    AtomKeyword, CanonicalClass, DeferredPublication, DescriptorGeneration, InternSnapshot,
    MaterializationEpoch, NativeIndexShape, NativeQueryClass, NativeSlotCount, PreparedSnapshot,
    ReadySnapshot, RegistryEpoch, SnapshotEpochs, SymbolRow,
};
