//! The effective-Plain value API and its one rule (E4-DESIGN §4.1-§4.2).
//!
//! An owner's Plain (untrapped, non-localized, non-forwarded) variable can be
//! bound in the owner's slab. The value a read observes is the owner's bound
//! slot when one exists and the canonical storage otherwise; an assignment
//! replaces the owner slot when one exists and canonical storage otherwise.
//! `default-value` / `default-boundp` / `set-default` use the SAME effective
//! Default channel, so they never bypass the owner's own Plain `let`.
//!
//! The functions here are pure: they take a slot view and a canonical
//! read/write closure and return a typed outcome. They construct no E1
//! capability and are not reachable from a serial dispatcher; stage 1's
//! executor supplies the slot view (the E2 owner slab) and the canonical C3
//! access, and roots the returned values before the lease closes.

use super::super::owner_bindings::{Channel, OwnerSlab, SlotState, SlotValue};
use crate::emacs_core::forward::TlsIndex;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;
use std::marker::PhantomData;

/// The World/era generation a validated key was minted in. Structural
/// changes escape the executor and revalidate cached keys by this value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EraId(u64);

impl EraId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

/// The binding-shape generation: a cold compare after every serial escape,
/// exactly the re-derivation trigger of §4.2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShapeGeneration(u64);

impl ShapeGeneration {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> u64 {
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
}

impl<'era> EffectivePlainKey<'era> {
    #[cfg(test)]
    pub(crate) fn test_key(
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
        }
    }

    pub(crate) const fn symbol(self) -> SymId {
        self.sym
    }

    pub(crate) const fn slot(self) -> TlsIndex {
        self.slot
    }

    pub(crate) const fn channel(self) -> Channel {
        self.channel
    }

    pub(crate) const fn era(self) -> EraId {
        self.era
    }

    pub(crate) const fn shape(self) -> ShapeGeneration {
        self.shape
    }

    /// The cold revalidation compare after a serial escape: a key whose era
    /// or shape moved must be re-derived through classification.
    pub(crate) fn is_valid_for(self, era: EraId, shape: ShapeGeneration) -> bool {
        self.era == era && self.shape == shape
    }
}

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
    pub(crate) fn value(self) -> Option<Value> {
        match self {
            Self::OwnerBound(value) | Self::Canonical(value) => Some(value),
            Self::OwnerVoid => None,
        }
    }

    /// Whether the owner's own slot supplied the observation (including a
    /// void binding); `false` means canonical storage did.
    pub(crate) const fn is_owner_local(self) -> bool {
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

/// Rule §4.2 items 2-3: a Bound slot is replaced; NoBinding delegates the
/// write to canonical storage.
pub(crate) fn write_effective<S: OwnerSlotsMut + ?Sized>(
    slots: &mut S,
    key: &EffectivePlainKey<'_>,
    value: Value,
    canonical: impl FnOnce(Value),
) -> EffectiveWrite {
    match slots.slot_state(key.slot(), key.channel()) {
        SlotState::Bound(_) => {
            slots.set_slot_state(
                key.slot(),
                key.channel(),
                SlotState::Bound(SlotValue::new(value)),
            );
            EffectiveWrite::OwnerSlot
        }
        SlotState::NoBinding => {
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
    match slots.slot_state(key.slot(), Channel::Default) {
        SlotState::Bound(_) => {
            slots.set_slot_state(
                key.slot(),
                Channel::Default,
                SlotState::Bound(SlotValue::new(value)),
            );
            EffectiveWrite::OwnerSlot
        }
        SlotState::NoBinding => {
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
