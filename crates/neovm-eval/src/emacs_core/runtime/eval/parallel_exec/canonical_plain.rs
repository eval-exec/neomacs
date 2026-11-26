//! E4-P2 Slice 2: production [`PlainAdmissionTable`] and [`CanonicalPlain`]
//! implementing [`PlainAccess`] and [`VmPlain`] over C1's [`OwnerSlabLease`]
//! and `neovm-value`'s narrow C3 plain-symbol authority seam.
//!
//! # Concurrency & ordering model (`#15175`, `#15177`, `#15179`, `#15180`)
//!
//! - [`PlainAdmissionTable`] is keeper-published before participants run: the
//!   map structure, [`SymbolHandle`]s, [`TlsIndex`] projections, `constant`
//!   flags, and `(EraId, admitted_shape)` are immutable for the era.
//! - Closed deny-by-default: any `SymId` absent from the table returns
//!   [`PlainAdmission::Refuse(PlainRefusal::NotPlain)`] and refuses canonical
//!   access with [`PlainAccessError::Publication`]; there is no on-demand
//!   fallback classification.
//! - Live per-symbol, per-turn `watched_or_retired: AtomicBool` and live
//!   `current_shape: AtomicU64` are single-writer at serial turns (`Release`
//!   store / `AcqRel` bump) and lock-free readers from parallel owners
//!   (`Acquire` load):
//!   - **Production ordering boundary (`AllStopped` rendezvous):** production
//!     serial turns (`WorldTurn` / `Baton`) execute exclusively under
//!     `AllStopped` — every parallel owner has dropped `OwnerSlabLease` /
//!     `RunAccess` and parked at the rendezvous barrier before the turn runs,
//!     and observes the rendezvous `Release`->`Acquire` handoff on resume.
//!     No parallel owner reads `watched_or_retired` concurrently with a
//!     serial-turn flip in production, so the stop/resume rendezvous guarantees
//!     immediate post-turn `true` visibility (no stale-false decision).
//!   - **Defense-in-depth atomics (`Release`/`Acquire`):** keeping both fields
//!     atomic makes `&PlainAdmissionTable` soundly `Sync` without a `Mutex` on
//!     the owner read path and race-free even under unit-level concurrent
//!     probes (`s2_real_thread_serial_turn_watched_bit_flip_visible_lock_free_under_tsan`),
//!     while the production stop/resume boundary is proven separately by
//!     `s3_turn_boundary_shared_root_intervening_gc_and_fresh_canonical_plain_reacquire`.
//! - [`CanonicalPlain`] composes C1's exclusive [`OwnerSlabLease`] (reusing
//!   `OwnerSlab`'s real binding-record LIFO per C2) with `&PlainAdmissionTable`.
//!   It is `!Send` / `!Sync` / `!Clone` / `!Copy`, dormant (no serial
//!   dispatcher caller), and holds zero TLS or global mutable state.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use neovm_value::forward::TlsIndex;
use neovm_value::intern::SymId;
use neovm_value::tagged::gc::SymbolHandle;
use neovm_value::value::Value;
use rustc_hash::FxHashMap;

use super::effective::{
    EraId, OwnerSlots, OwnerSlotsMut, PlainAccess, PlainAccessError, PlainAdmission, PlainRefusal,
    ShapeGeneration, classify_plain_backing, take_top_binding,
};
use super::vm::VmPlain;
use crate::emacs_core::eval::owner_bindings::{
    BindingKind, Channel, OwnerBindingRecord, OwnerRecordId, OwnerSlabLease, SlotState, UndoTarget,
};

/// One keeper-admitted Plain symbol entry in [`PlainAdmissionTable`].
///
/// Immutable-for-era fields (`handle`, `projected`, `constant`) are frozen at
/// keeper publication. Only `watched_or_retired` mutates during the era,
/// exclusively from serial turns (`Release`) and read lock-free by parallel
/// owners (`Acquire`).
#[derive(Debug)]
pub(crate) struct AdmittedPlainEntry {
    handle: SymbolHandle,
    projected: Option<TlsIndex>,
    constant: bool,
    watched_or_retired: AtomicBool,
}

impl AdmittedPlainEntry {
    /// Construct one keeper-admitted Plain entry.
    pub(crate) fn new(
        handle: SymbolHandle,
        projected: Option<TlsIndex>,
        constant: bool,
        watched_or_retired: bool,
    ) -> Self {
        Self {
            handle,
            projected,
            constant,
            watched_or_retired: AtomicBool::new(watched_or_retired),
        }
    }

    #[inline]
    pub(crate) fn handle(&self) -> SymbolHandle {
        self.handle
    }

    #[inline]
    pub(crate) fn projected(&self) -> Option<TlsIndex> {
        self.projected
    }

    #[inline]
    pub(crate) fn is_constant(&self) -> bool {
        self.constant
    }

    /// Lock-free parallel-owner read (`Acquire`) of the live per-symbol
    /// watched/retired flag written at serial turns (`Release`).
    #[inline]
    pub(crate) fn is_watched_or_retired(&self) -> bool {
        self.watched_or_retired.load(Ordering::Acquire)
    }

    /// Single-writer serial-turn update (`Release`) of the live per-symbol
    /// watched/retired flag.
    #[inline]
    pub(crate) fn set_watched_or_retired(&self, needs_serial: bool) {
        self.watched_or_retired
            .store(needs_serial, Ordering::Release);
    }
}

/// Keeper-published, closed deny-by-default Plain admission table for one
/// parallel era.
///
/// Safe to share (`Send + Sync`) across parallel owner threads as `&Self`:
/// map topology and immutable entry metadata never mutate after publication,
/// while per-symbol `watched_or_retired` and `current_shape` use `Release`
/// stores at serial turns and `Acquire` loads on parallel owners.
#[derive(Debug)]
pub(crate) struct PlainAdmissionTable {
    era: EraId,
    admitted_shape: ShapeGeneration,
    current_shape: AtomicU64,
    entries: FxHashMap<SymId, AdmittedPlainEntry>,
}

static_assertions::assert_impl_all!(PlainAdmissionTable: Send, Sync);

/// Builder for a keeper-published [`PlainAdmissionTable`].
#[derive(Debug, Default)]
pub(crate) struct PlainAdmissionTableBuilder {
    entries: FxHashMap<SymId, AdmittedPlainEntry>,
}

impl PlainAdmissionTableBuilder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Insert one keeper-admitted Plain symbol entry.
    pub(crate) fn insert(
        &mut self,
        handle: SymbolHandle,
        projected: Option<TlsIndex>,
        constant: bool,
        watched_or_retired: bool,
    ) -> &mut Self {
        self.entries.insert(
            handle.symbol(),
            AdmittedPlainEntry::new(handle, projected, constant, watched_or_retired),
        );
        self
    }

    /// Freeze map topology and publish the table for `(era, shape)`.
    pub(crate) fn build(self, era: EraId, shape: ShapeGeneration) -> PlainAdmissionTable {
        PlainAdmissionTable {
            era,
            admitted_shape: shape,
            current_shape: AtomicU64::new(shape.get()),
            entries: self.entries,
        }
    }
}

impl PlainAdmissionTable {
    /// The era this table was published for.
    #[inline]
    pub(crate) const fn era(&self) -> EraId {
        self.era
    }

    /// The shape generation at initial keeper publication.
    #[inline]
    pub(crate) const fn admitted_shape(&self) -> ShapeGeneration {
        self.admitted_shape
    }

    /// Live `(EraId, ShapeGeneration)` observed lock-free (`Acquire`).
    #[inline]
    pub(crate) fn current_generation(&self) -> (EraId, ShapeGeneration) {
        (
            self.era,
            ShapeGeneration::new(self.current_shape.load(Ordering::Acquire)),
        )
    }

    /// Advance the live shape generation from a serial turn (`AcqRel`),
    /// invalidating cached keys and forcing fail-closed `unbind_needs_serial`.
    #[inline]
    pub(crate) fn bump_shape_generation(&self) -> ShapeGeneration {
        let prev = self.current_shape.fetch_add(1, Ordering::AcqRel);
        ShapeGeneration::new(prev.wrapping_add(1))
    }

    /// Update one admitted symbol's live `watched_or_retired` bit from a
    /// serial turn (`Release`). Returns `true` if `sym` is in the table.
    #[inline]
    pub(crate) fn set_symbol_watched_or_retired(&self, sym: SymId, needs_serial: bool) -> bool {
        let Some(entry) = self.entries.get(&sym) else {
            return false;
        };
        entry.set_watched_or_retired(needs_serial);
        true
    }

    /// Closed deny-by-default admission for `sym`.
    ///
    /// Absent symbols and symbols whose live `watched_or_retired` flag is set
    /// refuse with [`PlainRefusal::NotPlain`] (no fallback classification).
    pub(crate) fn admission(&self, sym: SymId) -> PlainAdmission {
        let Some(entry) = self.entries.get(&sym) else {
            return PlainAdmission::Refuse(PlainRefusal::NotPlain);
        };
        if entry.is_watched_or_retired() {
            return PlainAdmission::Refuse(PlainRefusal::NotPlain);
        }
        classify_plain_backing(
            sym,
            Some(entry.handle.backing()),
            entry.constant,
            entry.projected,
        )
    }

    /// Fail-closed unbind re-validation (N-v2-unbind-revalidate + `#15175` +
    /// `#15177` + `#15180`): returns `true` (hand off to serial unbind) if
    /// **either**:
    /// 1. the table's live `(EraId, ShapeGeneration)` no longer matches
    ///    `(expected_era, expected_shape)`, OR
    /// 2. `sym` is absent from the admitted table, OR
    /// 3. `sym`'s live per-symbol `watched_or_retired` atomic bit (`Acquire`)
    ///    is `true`.
    #[inline]
    pub(crate) fn unbind_needs_serial(
        &self,
        sym: SymId,
        expected_era: EraId,
        expected_shape: ShapeGeneration,
    ) -> bool {
        if self.current_generation() != (expected_era, expected_shape) {
            return true;
        }
        let Some(entry) = self.entries.get(&sym) else {
            return true;
        };
        entry.is_watched_or_retired()
    }

    #[inline]
    fn handle_of(&self, sym: SymId) -> Result<SymbolHandle, PlainAccessError> {
        self.entries
            .get(&sym)
            .map(AdmittedPlainEntry::handle)
            .ok_or(PlainAccessError::Publication)
    }
}

/// Production [`PlainAccess`] and [`VmPlain`] over C1's [`OwnerSlabLease`]
/// and a keeper-published [`PlainAdmissionTable`] (E4-P2 Slice 2, C2).
///
/// Confined to the owning OS thread and to the lifetime of its anchoring
/// `RunAccess` stretch (`!Send`, `!Sync`, `!Clone`, `!Copy`).
#[derive(Debug)]
pub(crate) struct CanonicalPlain<'run, 'owner, 'era, 'tbl> {
    lease: OwnerSlabLease<'run, 'owner, 'era>,
    table: &'tbl PlainAdmissionTable,
    era: EraId,
    shape: ShapeGeneration,
}

static_assertions::assert_not_impl_any!(
    CanonicalPlain<'static, 'static, 'static, 'static>: Send, Sync, Clone, Copy
);

impl<'run, 'owner, 'era, 'tbl> CanonicalPlain<'run, 'owner, 'era, 'tbl> {
    /// Compose an active [`OwnerSlabLease`] with the era's keeper-published
    /// [`PlainAdmissionTable`], snapshotting `(EraId, ShapeGeneration)` at
    /// stretch entry.
    pub(crate) fn new(
        lease: OwnerSlabLease<'run, 'owner, 'era>,
        table: &'tbl PlainAdmissionTable,
    ) -> Self {
        let (era, shape) = table.current_generation();
        Self {
            lease,
            table,
            era,
            shape,
        }
    }

    /// Release [`CanonicalPlain`] and return its underlying [`OwnerSlabLease`]
    /// (for G1.2 / `#13957` turn-boundary release before `enter_native`).
    pub(crate) fn into_lease(self) -> OwnerSlabLease<'run, 'owner, 'era> {
        self.lease
    }

    /// Read the admitted `(EraId, ShapeGeneration)` captured when this
    /// stretch began.
    #[inline]
    pub(crate) const fn stretch_generation(&self) -> (EraId, ShapeGeneration) {
        (self.era, self.shape)
    }
}

impl OwnerSlots for CanonicalPlain<'_, '_, '_, '_> {
    #[inline]
    fn slot_state(&self, slot: TlsIndex, channel: Channel) -> SlotState {
        self.lease.slab().slot_state(slot, channel)
    }
}

impl OwnerSlotsMut for CanonicalPlain<'_, '_, '_, '_> {
    #[inline]
    fn set_slot_state(&mut self, slot: TlsIndex, channel: Channel, state: SlotState) {
        self.lease.slab_mut().set_slot_state(slot, channel, state);
    }
}

impl PlainAccess for CanonicalPlain<'_, '_, '_, '_> {
    #[inline]
    fn admission(&self, sym: SymId) -> PlainAdmission {
        self.table.admission(sym)
    }

    fn canonical_read(&mut self, sym: SymId) -> Result<Value, PlainAccessError> {
        let handle = self.table.handle_of(sym)?;
        self.lease
            .run_access_mut()
            .plain_symbol_read(&handle)
            .map_err(PlainAccessError::from)
    }

    fn canonical_write(&mut self, sym: SymId, value: Value) -> Result<(), PlainAccessError> {
        let handle = self.table.handle_of(sym)?;
        self.lease
            .run_access_mut()
            .plain_symbol_write(&handle, value)
            .map_err(PlainAccessError::from)
    }

    fn canonical_bound(&mut self, sym: SymId) -> Result<bool, PlainAccessError> {
        let handle = self.table.handle_of(sym)?;
        self.lease
            .run_access_mut()
            .plain_symbol_bound(&handle)
            .map_err(PlainAccessError::from)
    }

    #[inline]
    fn push_binding(
        &mut self,
        sym: SymId,
        slot: TlsIndex,
        channel: Channel,
        undo: UndoTarget,
    ) -> OwnerRecordId {
        self.lease
            .slab_mut()
            .push_record(sym, slot, BindingKind::Let, channel, undo)
    }

    #[inline]
    fn pop_binding(&mut self, id: OwnerRecordId) -> Option<OwnerBindingRecord> {
        take_top_binding(self.lease.slab_mut(), id)
    }

    #[inline]
    fn unbind_needs_serial(&self, sym: SymId) -> bool {
        self.table.unbind_needs_serial(sym, self.era, self.shape)
    }

    #[inline]
    fn record_live(&self, id: OwnerRecordId) -> bool {
        self.lease.slab().record(id).is_some()
    }

    #[inline]
    fn top_record(&self) -> Option<OwnerRecordId> {
        self.lease.slab().top_record_id()
    }
}

impl VmPlain for CanonicalPlain<'_, '_, '_, '_> {
    #[inline]
    fn generation(&self) -> (EraId, ShapeGeneration) {
        self.table.current_generation()
    }
}
