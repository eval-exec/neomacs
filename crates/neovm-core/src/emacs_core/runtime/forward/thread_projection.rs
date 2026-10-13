//! The internal thread-projection descriptor (Phase 0 step 2).
//!
//! When a second Lisp thread is admitted, every mutable variable cell is to
//! become `SYMBOL_FORWARDED` to one of these descriptors, and dynamic
//! bindings then live in per-owner slots selected by the descriptor's
//! [`TlsIndex`]. The value the symbol held before admission stays in the
//! descriptor's [`CanonicalStorage`], which records the cell's LOGICAL
//! redirect.
//!
//! # Encoding: a per-buffer header no buffer has
//!
//! A descriptor begins with a real [`LispBufferObjFwd`] header (`repr(C)`,
//! first field) whose `offset` is [`THREAD_PROJECTION_OFFSET`] -- a slot
//! index no buffer has, pinned below against `BUFFER_SLOT_COUNT` -- with
//! `local_flags_idx = -1` and `default = UNBOUND`. The family byte stays
//! `BufferObj`, so `LispFwdType`, `ForwardSlot` and every dispatch over them
//! are exactly what they were: no switch sees a new value, and none is
//! lowered differently.
//!
//! Every existing reader refuses such a cell with a check it already makes
//! for per-buffer slots, before touching any storage:
//! - `LispFwd::load` answers `None` (the per-buffer arm), so the VM's
//!   forwarded varref and the interpreter fall back to their general walks;
//! - `LispBufferObjFwd::value_in` fails its `off < slots.len()` test and
//!   answers `default`, which is UNBOUND, so the cached read tier refuses on
//!   its `is_unbound` test;
//! - the header's `offset` is private to the forward module; a slot is
//!   reachable only through `LispBufferObjFwd::buffer_slot`
//!   (`BufferSlot::from_u16`) or `slot_index().below(len)`, the checks
//!   every slot access already made, and both reject the marker;
//! - JIT classification (`FwdKind::of`) gives per-buffer slots no inline
//!   shape, so compiled sites refuse it on their redirect guard;
//! - the projected cell also carries the host-projection bit, which every
//!   checked writer refuses.
//!
//! # Round-2 obligations (owner-aware paths, admission)
//!
//! Each needs an out-of-line hook on a branch a real per-buffer descriptor
//! never takes (`from_u16` failing, `off >= len`, an UNBOUND result):
//! - `Obarray::find_symbol_value`, `find_symbol_value_in_buffer` and
//!   `value_without_buffer_id` answer `Some(UNBOUND)` for a projected cell:
//!   their callers' void handling is where owner-aware reads start;
//! - `make-local-variable` / `kill-local-variable` (`custom`) must check
//!   for a projection before localizing, or the cell leaves `Forwarded`;
//! - the remaining type-only `BufferObj` tests give per-buffer semantics:
//!   `data/mod.rs` (`local-variable-if-set-p`, `setq-default` refusal,
//!   `default-value`), `eval/mod.rs` assignment checks,
//!   `variable-binding-locus` (`symbols.rs`), the VM varset slow path
//!   (which signals "Invalid buffer slot offset") and the specbind and
//!   `set_internal` BufferObj arms;
//! - pdump writes a per-buffer cell as `Forwarded`, losing the canonical
//!   value, and `Obarray::clone` shares the descriptor;
//! - the canonical plain word is rooted by a lease fixed at projection;
//!   canonical writes must replace it (SATB-noted).
//!
//! # Reachability
//!
//! Admission does not exist yet. Every constructor here, the World's table
//! and the cell transitions into this state are compiled for tests only, so
//! the production binary is unchanged.

use super::{AtomicValue, LispBufferObjFwd, LispFwd};
use crate::buffer::buffer::BUFFER_SLOT_COUNT;
use crate::emacs_core::symbol::BlvPtr;
use crate::emacs_core::value::Value;
use std::num::NonZeroU32;

/// The per-buffer slot index a projection's header carries: no buffer has
/// it, so every slot access derived from the header fails its existing
/// bounds or validity check.
pub const THREAD_PROJECTION_OFFSET: u16 = u16::MAX;

const _: () = assert!(BUFFER_SLOT_COUNT < THREAD_PROJECTION_OFFSET as usize);

/// A projected variable's dense slot number in every owner's binding vector.
///
/// One per canonical base variable (aliases resolve first), issued by the
/// World's [`ThreadProjectionTable`]. Nonzero, so `Option<TlsIndex>` is a
/// `u32`. It names a slot; it grants no access to another heap or owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TlsIndex(NonZeroU32);

static_assertions::assert_eq_size!(TlsIndex, Option<TlsIndex>, u32);

impl TlsIndex {
    /// The slot's position in a zero-based owner vector.
    #[inline]
    pub fn slot(self) -> usize {
        // `NonZeroU32` fits `usize` on every supported target.
        (self.0.get() - 1) as usize
    }
}

/// The canonical value word of a projected `Plainval` variable: the global
/// value every owner without a binding reads. Stores note the overwritten
/// word first (`AtomicValue::store`), as the forwarder slots do.
#[repr(transparent)]
pub struct CanonicalPlainCell(AtomicValue);

static_assertions::assert_not_impl_any!(CanonicalPlainCell: Copy, Clone);

impl CanonicalPlainCell {
    /// The canonical global value ([`Value::UNBOUND`] when void).
    #[inline]
    pub fn get(&self) -> Value {
        self.0.load()
    }
}

impl std::fmt::Debug for CanonicalPlainCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CanonicalPlainCell").field(&self.0).finish()
    }
}

/// The buffer-local record a projected `SYMBOL_LOCALIZED` variable keeps.
/// The obarray's BLV pool still owns, traces and frees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalBlv(BlvPtr);

static_assertions::assert_not_impl_any!(CanonicalBlv: Send, Sync);

/// A GNU built-in forwarder a projected variable keeps as its canonical
/// storage: never a projection's own header, so a projection never wraps a
/// projection.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinForwarder(&'static LispFwd);

static_assertions::assert_not_impl_any!(BuiltinForwarder: Send, Sync);

impl BuiltinForwarder {
    /// The GNU descriptor.
    #[inline]
    pub fn descriptor(self) -> &'static LispFwd {
        self.0
    }
}

/// Where a projected variable's canonical (shared, unbound-owner) state
/// lives: the cell's logical redirect before projection. `Varalias` has no
/// arm: an alias stays an alias and resolves to its projected base.
#[derive(Debug)]
pub enum CanonicalStorage {
    /// GNU `SYMBOL_PLAINVAL`: the global value word.
    Plain(CanonicalPlainCell),
    /// GNU `SYMBOL_LOCALIZED`: the variable's buffer-local record.
    Localized(CanonicalBlv),
    /// GNU `SYMBOL_FORWARDED`: the built-in descriptor, still rooted where
    /// it was.
    Forwarded(BuiltinForwarder),
}

static_assertions::assert_not_impl_any!(CanonicalStorage: Copy, Clone, Send, Sync);

/// The thread-projection descriptor: a `SYMBOL_FORWARDED` cell's target once
/// its variable is owner-local. See the module documentation for why its
/// first field is a per-buffer header.
///
/// Fields are private and the only constructor is the World table's, so a
/// header carrying [`THREAD_PROJECTION_OFFSET`] heads one of these.
#[repr(C)]
pub struct ThreadProjectionDescriptor {
    header: LispBufferObjFwd,
    index: TlsIndex,
    canonical: CanonicalStorage,
}

static_assertions::assert_not_impl_any!(ThreadProjectionDescriptor: Copy, Clone, Send, Sync);
const _: () = {
    use std::mem::{needs_drop, offset_of};
    // Leaked like every forwarder body: drop glue would be stranded.
    assert!(!needs_drop::<ThreadProjectionDescriptor>());
    // The header is the body's start, so the cell's `&LispFwd` and a
    // per-buffer reader's `&LispBufferObjFwd` both address it.
    assert!(offset_of!(ThreadProjectionDescriptor, header) == 0);
};

impl ThreadProjectionDescriptor {
    /// The variable's slot in every owner's binding vector.
    #[inline]
    pub fn index(&self) -> TlsIndex {
        self.index
    }

    /// The variable's canonical storage (its logical redirect).
    #[inline]
    pub fn canonical(&self) -> &CanonicalStorage {
        &self.canonical
    }

    /// The per-buffer header a symbol cell stores.
    #[inline]
    pub fn header(&self) -> &LispFwd {
        use super::FwdDescriptor;
        self.header.header()
    }
}

impl std::fmt::Debug for ThreadProjectionDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadProjectionDescriptor")
            .field("index", &self.index)
            .field("canonical", &self.canonical)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
impl CanonicalStorage {
    /// The canonical storage of a variable whose cell was `Plainval` VALUE.
    pub(crate) fn plain(value: Value) -> Self {
        Self::Plain(CanonicalPlainCell(AtomicValue::new(value)))
    }

    /// The canonical storage of a variable whose cell was `Localized` BLV.
    pub(crate) fn localized(blv: BlvPtr) -> Self {
        Self::Localized(CanonicalBlv(blv))
    }

    /// The canonical storage of a forwarded variable, or `None` for a
    /// projection's own header.
    pub(crate) fn forwarded(fwd: &'static LispFwd) -> Option<Self> {
        let is_projection = fwd
            .as_buffer_obj_fwd()
            .is_some_and(|buf_fwd| buf_fwd.offset == THREAD_PROJECTION_OFFSET);
        (!is_projection).then_some(Self::Forwarded(BuiltinForwarder(fwd)))
    }

    /// The heap value this record keeps alive itself: only the plain word.
    pub(crate) fn owned_value(&self) -> Option<Value> {
        match self {
            Self::Plain(cell) => Some(cell.get()),
            Self::Localized(_) | Self::Forwarded(_) => None,
        }
    }
}

/// The World's registry of thread projections: slot allocation, and the one
/// authority that maps a cell's header back to its descriptor.
///
/// Descriptors are leaked like every forwarder body, since symbol cells hold
/// `&'static LispFwd`; the table owns the variable-to-slot assignment.
///
/// Test-only, like the World field that owns it, until stopped admission
/// exists.
#[cfg(test)]
#[derive(Debug, Default)]
pub struct ThreadProjectionTable {
    issued: Vec<&'static ThreadProjectionDescriptor>,
    /// Roots for canonical plain words, one per projection that has one.
    roots: Vec<crate::emacs_core::owned_roots::OwnedRoots>,
    // The table lives in a confined World.
    _thread_confined: std::marker::PhantomData<*const ()>,
}

#[cfg(test)]
static_assertions::assert_not_impl_any!(ThreadProjectionTable: Send, Sync, Clone);

#[cfg(test)]
impl ThreadProjectionTable {
    /// How many projections the table has issued.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.issued.len()
    }

    /// Whether no projection has been issued.
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.issued.is_empty()
    }

    /// The projection FWD heads, if this table issued it.
    ///
    /// Safe by construction: the marker is checked first (one compare a
    /// real per-buffer header always fails), then the address is matched
    /// against the descriptors this table issued, so a header copied out of
    /// a projection is never mistaken for one.
    pub(crate) fn projection(&self, fwd: &LispFwd) -> Option<&'static ThreadProjectionDescriptor> {
        if fwd.as_buffer_obj_fwd()?.offset != THREAD_PROJECTION_OFFSET {
            return None;
        }
        self.issued
            .iter()
            .copied()
            .find(|issued| std::ptr::eq(issued.header(), fwd))
    }
}

#[cfg(test)]
impl ThreadProjectionTable {
    /// Issue the next slot with CANONICAL as its backing. Test-only until
    /// stopped admission exists: no production path builds a projection.
    pub(crate) fn project_for_test(
        &mut self,
        canonical: CanonicalStorage,
    ) -> &'static ThreadProjectionDescriptor {
        let next = u32::try_from(self.issued.len() + 1)
            .ok()
            .and_then(NonZeroU32::new)
            .expect("thread-projection index space exhausted");
        let descriptor: &'static ThreadProjectionDescriptor =
            Box::leak(Box::new(ThreadProjectionDescriptor {
                header: LispBufferObjFwd {
                    ty: super::LispFwdType::BufferObj,
                    offset: THREAD_PROJECTION_OFFSET,
                    local_flags_idx: -1,
                    predicate: crate::buffer::buffer::BufferSlotPredicate::Unrestricted,
                    default: Value::UNBOUND,
                },
                index: TlsIndex(next),
                canonical,
            }));
        self.issued.push(descriptor);
        descriptor
    }

    /// Keep ROOTS alive as long as the table: the canonical plain words.
    pub(crate) fn retain_roots(&mut self, roots: crate::emacs_core::owned_roots::OwnedRoots) {
        self.roots.push(roots);
    }

    /// The descriptor issued for INDEX, if any.
    pub(crate) fn get(&self, index: TlsIndex) -> Option<&'static ThreadProjectionDescriptor> {
        self.issued.get(index.slot()).copied()
    }
}
