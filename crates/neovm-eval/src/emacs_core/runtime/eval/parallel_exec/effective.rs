//! The effective-Plain value API and its one rule (E4-DESIGN §4.1-§4.2).
//!
//! An owner's Plain (untrapped, non-localized, non-forwarded) variable can be
//! bound in the owner's slab. The value a read observes is the owner's bound
//! slot when one exists and the canonical storage otherwise; an assignment
//! replaces the owner slot when one exists and canonical storage otherwise.
//! `default-value` / `default-boundp` / `set-default` use the SAME effective
//! Default channel, so they never bypass the owner's own Plain `let`.
//!
//! The functions here are composed, not capability-bearing: they take a
//! slot view, a canonical read/write closure and, for a bind or an unbind,
//! the slab's record API, and return a typed outcome. They construct no E1
//! capability and are not reachable from a serial dispatcher; stage 1's
//! executor supplies the slot view (the E2 owner slab) and the canonical C3
//! access, and roots the returned values before the lease closes.

use super::super::owner_bindings::{
    Channel, OwnerBindingRecord, OwnerRecordId, OwnerSlab, SlotState, SlotValue, UndoTarget,
};
use super::frames::SlotRecord;
use neovm_value::forward::TlsIndex;
use neovm_value::intern::SymId;
use neovm_value::symbol::parallel::PlainSymbolBacking;
use neovm_value::value::Value;
use std::marker::PhantomData;
use std::rc::Rc;

/// The World/era generation a validated key was minted in. Structural
/// changes escape the executor and revalidate cached keys by this value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EraId(u64);

impl EraId {
    #[doc(hidden)]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[doc(hidden)]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The binding-shape generation: a cold compare after every serial escape,
/// exactly the re-derivation trigger of §4.2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShapeGeneration(u64);

impl ShapeGeneration {
    #[doc(hidden)]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[doc(hidden)]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A Plain variable admitted by validated classification (E1 inventory and
/// C3 admission) for one owner. The private fields and constructor keep an
/// unchecked integer or name from naming a slot; stage 1 mints it only from
/// the classification result and revalidates it after every escape.
///
/// The `'era` brand is the borrow that ends before a structural change; the
/// `era` and `shape` values are the cold revalidation compare. S0 carries
/// the values (plain data) and leaves the generative mint to stage 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EffectivePlainKey<'era> {
    sym: SymId,
    slot: TlsIndex,
    channel: Channel,
    era: EraId,
    shape: ShapeGeneration,
    _era: PhantomData<&'era ()>,
    /// The key is owner-confined: sending it to another owner would name a
    /// slot it does not own.
    _confined: PhantomData<Rc<()>>,
}

impl<'era> EffectivePlainKey<'era> {
    #[cfg(test)]
    #[doc(hidden)]
    pub fn test_key(
        sym: SymId,
        slot: TlsIndex,
        channel: Channel,
        era: EraId,
        shape: ShapeGeneration,
    ) -> Self {
        Self {
            sym,
            slot,
            channel,
            era,
            shape,
            _era: PhantomData,
            _confined: PhantomData,
        }
    }

    #[doc(hidden)]
    pub const fn symbol(self) -> SymId {
        self.sym
    }

    #[doc(hidden)]
    pub const fn slot(self) -> TlsIndex {
        self.slot
    }

    #[doc(hidden)]
    pub const fn channel(self) -> Channel {
        self.channel
    }

    #[doc(hidden)]
    pub const fn era(self) -> EraId {
        self.era
    }

    #[doc(hidden)]
    pub const fn shape(self) -> ShapeGeneration {
        self.shape
    }

    /// The cold revalidation compare after a serial escape: a key whose era
    /// or shape moved must be re-derived through classification.
    #[doc(hidden)]
    pub fn is_valid_for(self, era: EraId, shape: ShapeGeneration) -> bool {
        self.era == era && self.shape == shape
    }
}

static_assertions::assert_not_impl_any!(EffectivePlainKey<'static>: Send, Sync);

/// A read view over one owner's binding slab.
pub(crate) trait OwnerSlots {
    fn slot_state(&self, slot: TlsIndex, channel: Channel) -> SlotState;
}

/// A write view over the same slab.
pub(crate) trait OwnerSlotsMut: OwnerSlots {
    fn set_slot_state(&mut self, slot: TlsIndex, channel: Channel, state: SlotState);
}

impl OwnerSlots for OwnerSlab {
    fn slot_state(&self, slot: TlsIndex, channel: Channel) -> SlotState {
        OwnerSlab::slot(self, slot, channel)
    }
}

impl OwnerSlotsMut for OwnerSlab {
    fn set_slot_state(&mut self, slot: TlsIndex, channel: Channel, state: SlotState) {
        OwnerSlab::set_slot(self, slot, channel, state);
    }
}

/// What a read observes. `OwnerVoid` is a local void binding (GNU's
/// `makunbound` inside a `let`): it is distinct from "no owner binding",
/// which falls through to canonical storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectiveRead {
    OwnerBound(Value),
    OwnerVoid,
    Canonical(Value),
}

impl EffectiveRead {
    /// The value, or `None` for a local void binding.
    #[doc(hidden)]
    pub fn value(self) -> Option<Value> {
        match self {
            Self::OwnerBound(value) | Self::Canonical(value) => Some(value),
            Self::OwnerVoid => None,
        }
    }

    /// Whether the owner's own slot supplied the observation (including a
    /// void binding); `false` means canonical storage did.
    #[doc(hidden)]
    pub const fn is_owner_local(self) -> bool {
        !matches!(self, Self::Canonical(_))
    }
}

/// Rule §4.2 items 2-3: a Bound slot supplies the value (a void binding
/// included); NoBinding delegates to the canonical read.
pub(crate) fn read_effective<S: OwnerSlots + ?Sized>(
    slots: &S,
    key: &EffectivePlainKey<'_>,
    canonical: impl FnOnce() -> Value,
) -> EffectiveRead {
    match slots.slot_state(key.slot(), key.channel()) {
        SlotState::NoBinding => EffectiveRead::Canonical(canonical()),
        SlotState::Bound(slot) => match slot.bound() {
            Some(value) => EffectiveRead::OwnerBound(value),
            None => EffectiveRead::OwnerVoid,
        },
    }
}

/// Rule §4.2 item 4: `default-value` reads the effective Default channel.
pub(crate) fn read_default_effective<S: OwnerSlots + ?Sized>(
    slots: &S,
    key: &EffectivePlainKey<'_>,
    canonical: impl FnOnce() -> Value,
) -> EffectiveRead {
    match slots.slot_state(key.slot(), Channel::Default) {
        SlotState::NoBinding => EffectiveRead::Canonical(canonical()),
        SlotState::Bound(slot) => match slot.bound() {
            Some(value) => EffectiveRead::OwnerBound(value),
            None => EffectiveRead::OwnerVoid,
        },
    }
}

/// What an assignment does. The caller performs the canonical write in the
/// second case; the owner slot was already replaced in the first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectiveWrite {
    OwnerSlot,
    Canonical,
}

/// The single write transition shared by the closure family and the composed
/// `PlainAccess` entry points: a Bound slot is replaced in place; NoBinding
/// means the caller performs the canonical write.
#[inline]
fn write_effective_decision<S: OwnerSlots + ?Sized>(
    slots: &S,
    key: &EffectivePlainKey<'_>,
) -> EffectiveWrite {
    match slots.slot_state(key.slot(), key.channel()) {
        SlotState::Bound(_) => EffectiveWrite::OwnerSlot,
        SlotState::NoBinding => EffectiveWrite::Canonical,
    }
}

/// `set-default`/`setq-default` use the Default channel for the decision.
#[inline]
fn write_default_decision<S: OwnerSlots + ?Sized>(
    slots: &S,
    key: &EffectivePlainKey<'_>,
) -> EffectiveWrite {
    match slots.slot_state(key.slot(), Channel::Default) {
        SlotState::Bound(_) => EffectiveWrite::OwnerSlot,
        SlotState::NoBinding => EffectiveWrite::Canonical,
    }
}

/// Rule §4.2 items 2-3: a Bound slot is replaced; NoBinding delegates the
/// write to canonical storage.
pub(crate) fn write_effective<S: OwnerSlotsMut + ?Sized>(
    slots: &mut S,
    key: &EffectivePlainKey<'_>,
    value: Value,
    canonical: impl FnOnce(Value),
) -> EffectiveWrite {
    match write_effective_decision(slots, key) {
        EffectiveWrite::OwnerSlot => {
            slots.set_slot_state(
                key.slot(),
                key.channel(),
                SlotState::Bound(SlotValue::new(value)),
            );
            EffectiveWrite::OwnerSlot
        }
        EffectiveWrite::Canonical => {
            canonical(value);
            EffectiveWrite::Canonical
        }
    }
}

/// Rule §4.2 item 4: `set-default` writes the effective Default channel,
/// even when the key's normal access channel is a buffer-local one.
pub(crate) fn write_default_effective<S: OwnerSlotsMut + ?Sized>(
    slots: &mut S,
    key: &EffectivePlainKey<'_>,
    value: Value,
    canonical: impl FnOnce(Value),
) -> EffectiveWrite {
    match write_default_decision(slots, key) {
        EffectiveWrite::OwnerSlot => {
            slots.set_slot_state(
                key.slot(),
                Channel::Default,
                SlotState::Bound(SlotValue::new(value)),
            );
            EffectiveWrite::OwnerSlot
        }
        EffectiveWrite::Canonical => {
            canonical(value);
            EffectiveWrite::Canonical
        }
    }
}

/// `boundp` through the same rule: an owner void binding is unbound; no
/// owner binding asks canonical storage.
pub(crate) fn bound_effective<S: OwnerSlots + ?Sized>(
    slots: &S,
    key: &EffectivePlainKey<'_>,
    canonical_bound: impl FnOnce() -> bool,
) -> bool {
    match slots.slot_state(key.slot(), key.channel()) {
        SlotState::NoBinding => canonical_bound(),
        SlotState::Bound(slot) => slot.bound().is_some(),
    }
}

/// `default-boundp` reads the effective Default channel.
pub(crate) fn default_bound_effective<S: OwnerSlots + ?Sized>(
    slots: &S,
    key: &EffectivePlainKey<'_>,
    canonical_bound: impl FnOnce() -> bool,
) -> bool {
    match slots.slot_state(key.slot(), Channel::Default) {
        SlotState::NoBinding => canonical_bound(),
        SlotState::Bound(slot) => slot.bound().is_some(),
    }
}

/// Why a symbol cannot be admitted to the owner-local Plain fast path.
///
/// `NotPlain` is the value layer's `SymbolBacking::classify` returning `None`:
/// a trapped write, a non-Plain value cell, a localized or forwarded variable,
/// a host-projected variable, or a stale backing. E4-P2 may refine the reason
/// once it owns the descriptor; the refusal stays typed either way.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlainRefusal {
    /// A symbol whose constant/NoWrite flag is set (nil, t, keywords): `setq`
    /// signals `setting-constant`, so the operation stays on the serial path.
    /// GNU `defconst` does not itself enforce constancy (eval.c:1018-1024); the
    /// flag, not the defining form, is the predicate (Neo's `is_constant_id`).
    Constant,
    /// Not an untrapped Plain or projected-Plain variable.
    NotPlain,
    /// A Plain variable without its owner-slot descriptor (E4-P2 materializes
    /// unprojected descriptors at the structural boundary).
    MissingProjection,
    /// The backing was stale or absent.
    StaleBacking,
}

/// One validated admission result. `Mint` is the only source of a production
/// [`EffectivePlainKey`]; `Refuse` keeps the access on the serial path. A Plain
/// admission has no channel field: it always mints [`Channel::Default`], so a
/// buffer-local Plain key is unrepresentable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlainAdmission {
    Mint { sym: SymId, slot: TlsIndex },
    Refuse(PlainRefusal),
}

impl PlainAdmission {
    #[doc(hidden)]
    pub(crate) const fn mint(sym: SymId, slot: TlsIndex) -> Self {
        Self::Mint { sym, slot }
    }

    #[doc(hidden)]
    pub(crate) const fn refuse(reason: PlainRefusal) -> Self {
        Self::Refuse(reason)
    }
}

/// Exhaustive admission over the value layer's [`PlainSymbolBacking`].
///
/// `backing` is `SymbolBacking::classify`'s result, so a new backing kind is a
/// compile error here rather than a wildcard. `constant` is the symbol's
/// constant/NoWrite flag (nil, t, keywords; NOT a bare `defconst` name, see
/// eval.c:1018-1024); `projected` is the owner slot descriptor published for a
/// projected Plain variable. E4-P2 supplies both from the owner slab lease and
/// C3; the harness supplies them directly.
///
/// A Plain variable is never buffer-local or forwarded, so the admission mints
/// only [`Channel::Default`]; the channel cannot be supplied by a caller.
pub(crate) fn classify_plain_backing(
    sym: SymId,
    backing: Option<PlainSymbolBacking>,
    constant: bool,
    projected: Option<TlsIndex>,
) -> PlainAdmission {
    if constant {
        return PlainAdmission::refuse(PlainRefusal::Constant);
    }
    match backing {
        None => PlainAdmission::refuse(PlainRefusal::NotPlain),
        // An untrapped Plain symbol with no projection still needs the owner
        // slot descriptor E4-P2 materializes at the structural boundary.
        Some(PlainSymbolBacking::Unprojected)
        | Some(PlainSymbolBacking::Projected)
        | Some(PlainSymbolBacking::VoidProjection) => match projected {
            Some(slot) => PlainAdmission::mint(sym, slot),
            None => PlainAdmission::refuse(PlainRefusal::MissingProjection),
        },
    }
}

impl<'era> EffectivePlainKey<'era> {
    /// Mint a key from a validated admission. Private fields stay private;
    /// this is the only non-test constructor.
    #[doc(hidden)]
    pub(crate) fn from_admission(
        admission: &PlainAdmission,
        era: EraId,
        shape: ShapeGeneration,
    ) -> Result<Self, PlainRefusal> {
        match *admission {
            PlainAdmission::Mint { sym, slot } => Ok(Self {
                sym,
                slot,
                channel: Channel::Default,
                era,
                shape,
                _era: PhantomData,
                _confined: PhantomData,
            }),
            PlainAdmission::Refuse(reason) => Err(reason),
        }
    }
}

/// Recoverable or terminal authority failure from canonical Plain storage
/// (E4-P2 Option A).
///
/// Canonical reads, writes, and bound checks never panic and never fabricate a
/// fallback value on symbol-authority refusal: every caller receives this typed
/// error and turns it into an explicit fault or escape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlainAccessError {
    /// Symbol authority was contended (`RegistrationError::Busy`).
    Busy,
    /// The parallel era or symbol authority is poisoned.
    Poisoned,
    /// The owner registration, thread, or symbol handle is stale.
    StaleRegistration,
    /// Symbol publication or admission invariant failed (including an
    /// unadmitted `SymId`).
    Publication,
    /// Backing storage reservation exceeded capacity.
    Capacity,
}

impl From<neovm_value::tagged::gc::RegistrationError> for PlainAccessError {
    fn from(error: neovm_value::tagged::gc::RegistrationError) -> Self {
        use neovm_value::tagged::gc::RegistrationError;
        match error {
            RegistrationError::Busy
            | RegistrationError::OwnerReserveHeld
            | RegistrationError::OutstandingLoan
            | RegistrationError::OpenRegion
            | RegistrationError::ResidualSatb
            | RegistrationError::PendingRoots
            | RegistrationError::PendingYoung => Self::Busy,
            RegistrationError::Poisoned => Self::Poisoned,
            RegistrationError::WrongHeap
            | RegistrationError::WrongOwner
            | RegistrationError::StaleRegistration
            | RegistrationError::DuplicateOwner
            | RegistrationError::WrongThread => Self::StaleRegistration,
            RegistrationError::Publication => Self::Publication,
            RegistrationError::Exhausted | RegistrationError::Capacity => Self::Capacity,
        }
    }
}

/// The composed owner access shared by the E4 interpreter and the parallel VM:
/// the owner slot view plus the canonical Plain storage plus the slab's
/// binding-record LIFO. The production implementation is E4-P2 (owner slab
/// lease + C3); the trait itself names no test type.
pub(crate) trait PlainAccess: OwnerSlotsMut {
    /// Classify one symbol for this owner (admission only; no slot access).
    fn admission(&self, sym: SymId) -> PlainAdmission;
    /// Read canonical Plain storage (C3 in production).
    fn canonical_read(&mut self, sym: SymId) -> Result<Value, PlainAccessError>;
    /// Write canonical Plain storage (C3 in production).
    fn canonical_write(&mut self, sym: SymId, value: Value) -> Result<(), PlainAccessError>;
    /// Whether canonical Plain storage has a binding.
    fn canonical_bound(&mut self, sym: SymId) -> Result<bool, PlainAccessError>;
    /// Push one binding record on the owner slab's record LIFO and name it.
    /// The record is the binding's storage: the collector traces its values
    /// through the slab, so a caller keeps only the id.
    fn push_binding(
        &mut self,
        sym: SymId,
        slot: TlsIndex,
        channel: Channel,
        undo: UndoTarget,
    ) -> OwnerRecordId;
    /// Take the top record when it is the one `id` names, and nothing
    /// otherwise: a record below the top, or one already taken, leaves the
    /// LIFO as it was. The serial unwinder is LIFO (`unbind_to`, eval.c:3946),
    /// so an unbind of anything but the top is a defect on one side of the
    /// record pairing, which the caller reports as such.
    fn pop_binding(&mut self, id: OwnerRecordId) -> Option<OwnerBindingRecord>;
    /// Whether a binding of `sym` must be undone by the serial side
    /// (N-v2-unbind-revalidate). GNU's `do_one_unbind` for a `SPECPDL_LET`
    /// re-reads the symbol when the binding leaves (eval.c:3848-3870): a
    /// watcher added while the binding was live gets the `unlet` notification
    /// through `set_internal`, a variable made local or forwarded goes
    /// through `set_default_internal`. The owner-local restore writes the
    /// slot and nothing else, so the serial `unbind_owner_binding`
    /// (owner_bindings.rs:1276-1360) keeps its slow path for exactly these
    /// cases: `has_watchers`, a retired projection, a dead channel. This is
    /// that predicate, read cold at
    /// unbind time. A `true` hands the record to the serial unbind; nothing
    /// is restored on the owner.
    fn unbind_needs_serial(&self, sym: SymId) -> bool;
    /// Whether the slab still holds the record `id` names. Read cold after a
    /// `PlainUnbind` escape: the serial side undoes the binding by taking
    /// that record, and the walk's own check is that it is gone, as its check
    /// for a serial run is that the named tokens are gone. A record still
    /// live after the hand-off is the record-contract fault.
    fn record_live(&self, id: OwnerRecordId) -> bool;
    /// The id of the slab's innermost live record, O(1). Read before a
    /// `PlainUnbind` hand-off: the record the walk hands to the serial side
    /// is the top Plain entry above the frame's base, and the executor's
    /// entries mirror the slab's records in one LIFO order (C2), so it is
    /// the slab's top. Anything else is a record the owner's frame never
    /// pushed: the record-contract fault, nothing handed off.
    fn top_record(&self) -> Option<OwnerRecordId>;
}

/// The slab half of [`PlainAccess::pop_binding`] over a slab the implementor
/// holds: the top record when it is `id`. `OwnerSlab::take_record` alone does
/// not do: of a record below the top it takes that record and leaves a hole.
///
/// [`OwnerSlab::take_top_record`] is the slab's own form of the same rule, with
/// an O(1) read of the top; this is the free function the composed unbind and
/// the harness doubles call.
pub(crate) fn take_top_binding(
    slab: &mut OwnerSlab,
    id: OwnerRecordId,
) -> Option<OwnerBindingRecord> {
    slab.take_top_record(id)
}

/// An executor record did not name the slab's top record: nothing was taken.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecordMismatch;

/// Why an owner-local Plain unbind failed (E4-P2 Option A).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlainUnbindError {
    /// The executor's entry did not name the slab's top record; nothing was taken.
    RecordMismatch,
    /// Restoring a `Captured` binding into canonical storage failed.
    Access(PlainAccessError),
}

impl From<RecordMismatch> for PlainUnbindError {
    fn from(_: RecordMismatch) -> Self {
        Self::RecordMismatch
    }
}

impl From<PlainAccessError> for PlainUnbindError {
    fn from(error: PlainAccessError) -> Self {
        Self::Access(error)
    }
}

/// One composed read: checks the owner slot first and falls through to
/// canonical Plain storage only on [`SlotState::NoBinding`].
pub(crate) fn effective_plain_read<A: PlainAccess + ?Sized>(
    access: &mut A,
    key: &EffectivePlainKey<'_>,
) -> Result<EffectiveRead, PlainAccessError> {
    match access.slot_state(key.slot(), key.channel()) {
        SlotState::Bound(value) => Ok(match value.bound() {
            Some(bound) => EffectiveRead::OwnerBound(bound),
            None => EffectiveRead::OwnerVoid,
        }),
        SlotState::NoBinding => access
            .canonical_read(key.symbol())
            .map(EffectiveRead::Canonical),
    }
}

/// One composed write: the shared decision, then the slot or canonical write.
pub(crate) fn effective_plain_write<A: PlainAccess + ?Sized>(
    access: &mut A,
    key: &EffectivePlainKey<'_>,
    value: Value,
) -> Result<EffectiveWrite, PlainAccessError> {
    match write_effective_decision(access, key) {
        EffectiveWrite::OwnerSlot => {
            access.set_slot_state(
                key.slot(),
                key.channel(),
                SlotState::Bound(SlotValue::new(value)),
            );
            Ok(EffectiveWrite::OwnerSlot)
        }
        EffectiveWrite::Canonical => {
            access.canonical_write(key.symbol(), value)?;
            Ok(EffectiveWrite::Canonical)
        }
    }
}

/// `boundp` through the composed rule.
pub(crate) fn effective_plain_bound<A: PlainAccess + ?Sized>(
    access: &mut A,
    key: &EffectivePlainKey<'_>,
) -> Result<bool, PlainAccessError> {
    match access.slot_state(key.slot(), key.channel()) {
        SlotState::Bound(value) => Ok(value.bound().is_some()),
        SlotState::NoBinding => access.canonical_bound(key.symbol()),
    }
}

/// `default-value` through the same effective Default channel.
pub(crate) fn effective_plain_default_read<A: PlainAccess + ?Sized>(
    access: &mut A,
    key: &EffectivePlainKey<'_>,
) -> Result<EffectiveRead, PlainAccessError> {
    match access.slot_state(key.slot(), Channel::Default) {
        SlotState::Bound(value) => Ok(match value.bound() {
            Some(bound) => EffectiveRead::OwnerBound(bound),
            None => EffectiveRead::OwnerVoid,
        }),
        SlotState::NoBinding => access
            .canonical_read(key.symbol())
            .map(EffectiveRead::Canonical),
    }
}

/// `set-default` through the shared Default decision.
pub(crate) fn effective_plain_default_write<A: PlainAccess + ?Sized>(
    access: &mut A,
    key: &EffectivePlainKey<'_>,
    value: Value,
) -> Result<EffectiveWrite, PlainAccessError> {
    match write_default_decision(access, key) {
        EffectiveWrite::OwnerSlot => {
            access.set_slot_state(
                key.slot(),
                Channel::Default,
                SlotState::Bound(SlotValue::new(value)),
            );
            Ok(EffectiveWrite::OwnerSlot)
        }
        EffectiveWrite::Canonical => {
            access.canonical_write(key.symbol(), value)?;
            Ok(EffectiveWrite::Canonical)
        }
    }
}

/// `default-boundp` through the same effective Default channel.
pub(crate) fn effective_plain_default_bound<A: PlainAccess + ?Sized>(
    access: &mut A,
    key: &EffectivePlainKey<'_>,
) -> Result<bool, PlainAccessError> {
    match access.slot_state(key.slot(), Channel::Default) {
        SlotState::Bound(value) => Ok(value.bound().is_some()),
        SlotState::NoBinding => access.canonical_bound(key.symbol()),
    }
}

/// One composed bind (GNU `Bvarbind`, bytecode.c:743-746, is `specbind
/// (vectorp[arg], POP)`; the serial arm pushes the specpdl entry and its
/// frame mark, bytecode/vm.rs:5722-5734). In the order of the unwatched
/// Plain bind of `specbind_owner` (owner_bindings.rs:1048-1068): the owner
/// slot is replaced with VALUE, the undo target is what the slot held — a
/// bound outer value is [`UndoTarget::Previous`], no owner binding is
/// [`UndoTarget::Inherited`] with nothing suspended — and the record goes on
/// the slab's LIFO. The returned [`SlotRecord`] is the executor's index of
/// that record (one record, two views): the slab stores and roots the saved
/// value, the executor keeps the id for its frame-unwind LIFO. The bind is
/// composed rather than a closure rule for `effective_plain_write`'s reason:
/// no second borrow of the access.
pub(crate) fn effective_plain_bind<A: PlainAccess + ?Sized>(
    access: &mut A,
    key: &EffectivePlainKey<'_>,
    value: Value,
) -> SlotRecord {
    let undo = match access.slot_state(key.slot(), key.channel()) {
        SlotState::Bound(outer) => UndoTarget::Previous(outer),
        SlotState::NoBinding => UndoTarget::Inherited { suspended: None },
    };
    access.set_slot_state(
        key.slot(),
        key.channel(),
        SlotState::Bound(SlotValue::new(value)),
    );
    let record = access.push_binding(key.symbol(), key.slot(), key.channel(), undo);
    SlotRecord::new(key.symbol(), record)
}

/// One composed unbind: the slab record the executor's entry names is taken
/// from the top of the LIFO, then the storage transitions of the serial
/// unbind for one `let` record run in the serial order (slot first, then
/// canonical storage; `discard_owner_binding`, owner_bindings.rs:1504-1516,
/// whose four arms these are, read off the record the slab stored). `Retired`
/// leaves the slot as it is; `Previous` puts the outer value back, a void
/// binding included; `Inherited` clears the slot so the canonical value shows
/// through again; `Captured` clears the slot and stores the value the binding
/// captured from canonical storage. GNU `unbind_to` → `do_one_unbind`
/// (eval.c:3946-3957) is the reference for "undo the binding where it
/// stands"; the host mirror's publication is not an admitted Plain record's
/// concern.
///
/// A record that is not the slab's top is [`PlainUnbindError::RecordMismatch`]:
/// the caller's entry and the slab disagree, nothing was taken, and the slot
/// is as it was.
///
/// This is the restore without the unbind-time re-validation; the stepper's
/// walk goes through [`effective_plain_unbind_checked`], which is this
/// function behind [`PlainAccess::unbind_needs_serial`]. The stepper's
/// `Return` unwinds and the idle undo (V4) take the checked form; V8's
/// unwind loop will too.
pub(crate) fn effective_plain_unbind<A: PlainAccess + ?Sized>(
    access: &mut A,
    record: SlotRecord,
) -> Result<(), PlainUnbindError> {
    let taken = access
        .pop_binding(record.record)
        .ok_or(PlainUnbindError::RecordMismatch)?;
    restore_taken_binding(access, taken)?;
    Ok(())
}

/// The store half of an unbind, for a record already taken off the slab:
/// the owner-local form of `discard_owner_binding`'s four arms
/// (owner_bindings.rs:1504-1516).
///
/// GNU runs an unbind in three steps, and a watched symbol makes the order
/// visible: `do_one_unbind` takes the entry off the specpdl first
/// (eval.c:3946-3955), `set_internal` then notifies the watchers with
/// `Qunlet` (data.c:1697-1708), and only then stores the outer value
/// (`SET_SYMBOL_VAL`, data.c:1720, after the `start:` label at :1716). A
/// watcher that signals skips the store:
/// the inner value stays in place (owner_bindings.rs:1265-1272 says the
/// same). The owner-local restore runs take and store back to back because
/// nothing runs between them; a hand-off with a watcher runs them around
/// the watcher's Lisp, and that is G1's order.
pub(crate) fn restore_taken_binding<A: PlainAccess + ?Sized>(
    access: &mut A,
    taken: OwnerBindingRecord,
) -> Result<(), PlainAccessError> {
    match taken.undo {
        UndoTarget::Retired => {}
        UndoTarget::Previous(outer) => {
            access.set_slot_state(taken.index, taken.channel, SlotState::Bound(outer));
        }
        UndoTarget::Inherited { .. } => {
            access.set_slot_state(taken.index, taken.channel, SlotState::NoBinding);
        }
        UndoTarget::Captured(saved) => {
            access.set_slot_state(taken.index, taken.channel, SlotState::NoBinding);
            access.canonical_write(taken.sym, saved.get())?;
        }
    }
    Ok(())
}

/// What the stepper's walk does with one owner binding entry (N-v2-
/// unbind-revalidate). The re-validation runs BEFORE anything is taken, so a
/// refused record leaves the slab, the slot and the executor's LIFO exactly
/// as they were for the serial unbind to undo.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlainUnbind {
    /// Undone on the owner: the slot and the canonical storage are restored.
    Done,
    /// The serial side undoes it: [`PlainAccess::unbind_needs_serial`] held
    /// for the symbol, and nothing was taken or restored.
    NeedsSerial { symbol: SymId },
}

/// One composed unbind with the unbind-time re-validation: GNU's
/// `do_one_unbind` for a `SPECPDL_LET` re-reads the symbol as the binding
/// leaves (eval.c:3848-3870), so a watcher added while the binding was live
/// is notified, and a variable made local or forwarded takes the default
/// path. The predicate is [`PlainAccess::unbind_needs_serial`]; a refusal
/// hands the record to the serial unbind (owner_bindings.rs:1276-1360)
/// and restores nothing here.
pub(crate) fn effective_plain_unbind_checked<A: PlainAccess + ?Sized>(
    access: &mut A,
    record: SlotRecord,
) -> Result<PlainUnbind, PlainUnbindError> {
    if access.unbind_needs_serial(record.symbol) {
        return Ok(PlainUnbind::NeedsSerial {
            symbol: record.symbol,
        });
    }
    effective_plain_unbind(access, record).map(|()| PlainUnbind::Done)
}
