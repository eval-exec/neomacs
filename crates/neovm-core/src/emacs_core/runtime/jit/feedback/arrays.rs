//! Plain-array physical-kind observations, owned by a bytecode source.
//!
//! Threading: a process registry keys sites by a SOURCE RuntimeState
//! process-unique compiled id. Closures share its source, and several mutators
//! may observe a site concurrently. The registry lock publishes a fully
//! initialized immutable-address Arc table; relaxed fetch_or atomically joins immutable physical kind
//! bits without losing a concurrent observation. Counters are diagnostics only
//! (modulo 2^32), never a stability predicate or a native-entry denominator.
//! No slot stores a Lisp word, object address, backing pointer, length or root.
//! Compiler snapshots own copied masks; observations remain speculation hints,
//! and native code must execute an actual shape guard before dereferencing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, RwLock, Weak};

use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::{Runtime, RuntimeState};
use crate::emacs_core::value::{Value, ValueKind, VecLikeType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ObservedArrayKind {
    PlainVector = 1,
    PlainRecord = 2,
    Other = 4,
}

impl ObservedArrayKind {
    /// Only the tag and immutable physical veclike header are read. The caller
    /// supplies a live valid Lisp operand on its owning mutator; this cannot
    /// allocate, call Lisp, signal, poll, or inspect mutable backing/length.
    #[inline]
    pub(crate) fn of(value: Value) -> Self {
        match value.kind() {
            ValueKind::Veclike(VecLikeType::Vector) => Self::PlainVector,
            ValueKind::Veclike(VecLikeType::Record) => Self::PlainRecord,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlainArrayKind {
    Vector,
    Record,
    VectorOrRecord,
}

/// No observed kind is bottom; any non-plain observation prevents admission.
/// Both plain kinds are acceptable to the union guard, without assuming size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ArrayKindMask(u8);

impl ArrayKindMask {
    #[inline]
    pub(crate) fn plain(self) -> Option<PlainArrayKind> {
        match self.0 {
            1 => Some(PlainArrayKind::Vector),
            2 => Some(PlainArrayKind::Record),
            3 => Some(PlainArrayKind::VectorOrRecord),
            _ => None,
        }
    }
}

/// Immutable-address scalar site. Source/leaf Arc holds keep this address live;
/// it is not persisted in dumps or AOT objects and never stores Lisp state.
#[derive(Debug, Default)]
pub(crate) struct ArraySiteFeedback {
    mask: AtomicU8,
    samples: AtomicU32,
}

impl ArraySiteFeedback {
    #[inline]
    pub(crate) fn observe(&self, kind: ObservedArrayKind) -> bool {
        let seen = kind as u8;
        let old = self.mask.fetch_or(seen, Ordering::Relaxed);
        self.samples.fetch_add(1, Ordering::Relaxed);
        old & seen == 0
    }

    #[inline]
    pub(crate) fn mask(&self) -> ArrayKindMask {
        ArrayKindMask(self.mask.load(Ordering::Relaxed))
    }

    #[cfg(test)]
    pub(crate) fn samples(&self) -> u32 {
        self.samples.load(Ordering::Relaxed)
    }
}

/// The source's original instruction-pc mapping and fixed compact site array.
/// Initialization happens on the compile front, before any native activation.
/// The shared source already guarantees identical executable ops for closures.
#[derive(Debug)]
pub(crate) struct ArraySites {
    at_pc: Box<[Option<u32>]>,
    sites: Box<[ArraySiteFeedback]>,
}

impl ArraySites {
    fn new(ops: &[Op]) -> Self {
        let mut sites = Vec::new();
        let at_pc = ops
            .iter()
            .map(|op| {
                if *op == Op::Aref {
                    let index = sites.len() as u32;
                    sites.push(ArraySiteFeedback::default());
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        Self {
            at_pc,
            sites: sites.into_boxed_slice(),
        }
    }

    #[inline]
    pub(crate) fn site_at(&self, pc: usize) -> Option<&ArraySiteFeedback> {
        self.sites.get(*self.at_pc.get(pc)?.as_ref()? as usize)
    }

    /// Scalar-only copy. No sample count is used to make the tier stable.
    pub(crate) fn snapshot(&self, ops_len: usize) -> Vec<ArrayKindMask> {
        (0..ops_len)
            .map(|pc| {
                self.site_at(pc)
                    .map_or_else(ArrayKindMask::default, ArraySiteFeedback::mask)
            })
            .collect()
    }
}

/// Selected scalar tables live outside RuntimeState so its OFF layout remains
/// unchanged. The registry owns each table while its weak source is live;
/// registration sweeps dead sources. Generated leaves already hold a strong
/// source Arc through FeedbackHolds, preventing a baked site's removal while
/// code can execute. Snapshots own table Arcs or copied masks. Neither key nor
/// table stores Lisp words, root pointers, or mutable array storage.
///
/// Threading: mutators share this process registry under a RwLock, publishing
/// the fully initialized Arc before readers clone it. Existing source ids use
/// Acquire/AcqRel publication and are never reused. Site mask joins retain
/// Relaxed fetch_or: only set union is required, with no dependent publication.
/// Cleanup uses Weak::strong_count only to remove sources with no owner; any
/// compiling/active leaf's existing source Arc prevents removal. No lookup is
/// emitted in generated code, and no worker borrows mutator Lisp state.
struct ArrayRegistryEntry {
    source: Weak<RuntimeState>,
    sites: Arc<ArraySites>,
}

type ArrayRegistry = HashMap<u64, ArrayRegistryEntry>;
static ARRAY_REGISTRY: OnceLock<RwLock<ArrayRegistry>> = OnceLock::new();

impl Runtime {
    /// Create only from the selected compile frontend, before emitting a site
    /// pointer. A clone of this Runtime shares the same source and table.
    pub(crate) fn array_sites_for(&self, ops: &[Op]) -> Arc<ArraySites> {
        let id = self.compiled_id_or_assign();
        let source = self.share_state();
        let registry = ARRAY_REGISTRY.get_or_init(|| RwLock::new(HashMap::new()));
        let mut entries = registry
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        entries.retain(|_, entry| entry.source.strong_count() != 0);
        Arc::clone(
            &entries
                .entry(id)
                .or_insert_with(|| ArrayRegistryEntry {
                    source: Arc::downgrade(&source),
                    sites: Arc::new(ArraySites::new(ops)),
                })
                .sites,
        )
    }
}

impl RuntimeState {
    /// A copied ownership hold, without initializing a registry or assigning
    /// an id for a source the selected frontend has never observed.
    pub(crate) fn array_sites(&self) -> Option<Arc<ArraySites>> {
        let id = self.compiled_id()?;
        let registry = ARRAY_REGISTRY.get()?;
        let entries = registry
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        entries.get(&id).map(|entry| Arc::clone(&entry.sites))
    }

    pub(crate) fn array_kind_snapshot(&self, ops_len: usize) -> Vec<ArrayKindMask> {
        self.array_sites()
            .map_or_else(Vec::new, |sites| sites.snapshot(ops_len))
    }
}

#[cfg(test)]
#[path = "tests/array_registry_test.rs"]
mod registry_tests;
