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

/// The composed owner access shared by the E4 interpreter and the parallel VM:
/// the owner slot view plus the canonical Plain storage plus the slab's
/// binding-record LIFO. The production implementation is E4-P2 (owner slab
/// lease + C3); the trait itself names no test type and the only E4-P1
/// implementation is the harness double.
pub(crate) trait PlainAccess: OwnerSlotsMut {
    /// Classify one symbol for this owner (admission only; no slot access).
    fn admission(&self, sym: SymId) -> PlainAdmission;
    /// Read canonical Plain storage (C3 in production).
    fn canonical_read(&self, sym: SymId) -> Value;
    /// Write canonical Plain storage (C3 in production).
    fn canonical_write(&mut self, sym: SymId, value: Value);
    /// Whether canonical Plain storage has a binding.
    fn canonical_bound(&self, sym: SymId) -> bool;
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
}

/// The slab half of [`PlainAccess::pop_binding`] over a slab the implementor
/// holds: the top record when it is `id`. `OwnerSlab::take_record` alone does
/// not do: of a record below the top it takes that record and leaves a hole.
pub(crate) fn take_top_binding(
    slab: &mut OwnerSlab,
    id: OwnerRecordId,
) -> Option<OwnerBindingRecord> {
    let top = slab.live_records().last().map(|(top, _)| top)?;
    (top == id).then(|| slab.take_record(id)).flatten()
}

/// An executor record did not name the slab's top record: nothing was taken.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecordMismatch;

/// One composed read: delegates to the pure rule with the canonical closure.
pub(crate) fn effective_plain_read<A: PlainAccess + ?Sized>(
    access: &A,
    key: &EffectivePlainKey<'_>,
) -> EffectiveRead {
    read_effective(access, key, || access.canonical_read(key.symbol()))
}

/// One composed write: the shared decision, then the slot or canonical write.
pub(crate) fn effective_plain_write<A: PlainAccess + ?Sized>(
    access: &mut A,
    key: &EffectivePlainKey<'_>,
    value: Value,
) -> EffectiveWrite {
    match write_effective_decision(access, key) {
        EffectiveWrite::OwnerSlot => {
            access.set_slot_state(
                key.slot(),
                key.channel(),
                SlotState::Bound(SlotValue::new(value)),
            );
            EffectiveWrite::OwnerSlot
        }
        EffectiveWrite::Canonical => {
            access.canonical_write(key.symbol(), value);
            EffectiveWrite::Canonical
        }
    }
}

/// `boundp` through the composed rule, delegating to the pure rule.
pub(crate) fn effective_plain_bound<A: PlainAccess + ?Sized>(
    access: &A,
    key: &EffectivePlainKey<'_>,
) -> bool {
    bound_effective(access, key, || access.canonical_bound(key.symbol()))
}

/// `default-value` through the same effective Default channel.
pub(crate) fn effective_plain_default_read<A: PlainAccess + ?Sized>(
    access: &A,
    key: &EffectivePlainKey<'_>,
) -> EffectiveRead {
    read_default_effective(access, key, || access.canonical_read(key.symbol()))
}

/// `set-default` through the shared Default decision.
pub(crate) fn effective_plain_default_write<A: PlainAccess + ?Sized>(
    access: &mut A,
    key: &EffectivePlainKey<'_>,
    value: Value,
) -> EffectiveWrite {
    match write_default_decision(access, key) {
        EffectiveWrite::OwnerSlot => {
            access.set_slot_state(
                key.slot(),
                Channel::Default,
                SlotState::Bound(SlotValue::new(value)),
            );
            EffectiveWrite::OwnerSlot
        }
        EffectiveWrite::Canonical => {
            access.canonical_write(key.symbol(), value);
            EffectiveWrite::Canonical
        }
    }
}

/// `default-boundp` through the same effective Default channel.
pub(crate) fn effective_plain_default_bound<A: PlainAccess + ?Sized>(
    access: &A,
    key: &EffectivePlainKey<'_>,
) -> bool {
    default_bound_effective(access, key, || access.canonical_bound(key.symbol()))
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
/// A record that is not the slab's top is [`RecordMismatch`]: the caller's
/// entry and the slab disagree, nothing was taken, and the slot is as it was.
pub(crate) fn effective_plain_unbind<A: PlainAccess + ?Sized>(
    access: &mut A,
    record: SlotRecord,
) -> Result<(), RecordMismatch> {
    let taken = access.pop_binding(record.record).ok_or(RecordMismatch)?;
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
            access.canonical_write(taken.sym, saved.get());
        }
    }
    Ok(())
}
