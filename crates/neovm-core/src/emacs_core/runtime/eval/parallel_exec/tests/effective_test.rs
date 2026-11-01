//! E4-S0 effective-Plain rule unit tests (pure functions over a test slab).

use super::super::owner_bindings::{Channel, SlotState, SlotValue};
use super::effective::*;
use crate::buffer::BufferId;
use crate::emacs_core::forward::TlsIndex;
use crate::emacs_core::intern::NIL_SYM_ID;
use crate::emacs_core::value::Value;
use std::cell::Cell;

struct TestSlots {
    default: SlotState,
    buffer_local: SlotState,
}

impl TestSlots {
    fn new(default: SlotState, buffer_local: SlotState) -> Self {
        Self {
            default,
            buffer_local,
        }
    }

    fn empty() -> Self {
        Self::new(SlotState::NoBinding, SlotState::NoBinding)
    }
}

impl OwnerSlots for TestSlots {
    fn slot_state(&self, _slot: TlsIndex, channel: Channel) -> SlotState {
        match channel {
            Channel::Default => self.default,
            Channel::BufferLocal(_) => self.buffer_local,
        }
    }
}

impl OwnerSlotsMut for TestSlots {
    fn set_slot_state(&mut self, _slot: TlsIndex, channel: Channel, state: SlotState) {
        match channel {
            Channel::Default => self.default = state,
            Channel::BufferLocal(_) => self.buffer_local = state,
        }
    }
}

fn key(channel: Channel) -> EffectivePlainKey<'static> {
    EffectivePlainKey::test_key(
        NIL_SYM_ID,
        TlsIndex::VOID,
        channel,
        EraId::new(3),
        ShapeGeneration::new(7),
    )
}

fn bound(value: i64) -> SlotState {
    SlotState::Bound(SlotValue::new(Value::fixnum(value)))
}

fn assert_slot(state: SlotState, expected: Option<i64>) {
    match (state, expected) {
        (SlotState::NoBinding, None) => {}
        (SlotState::Bound(value), Some(number)) => {
            assert_eq!(value.get().as_fixnum(), Some(number))
        }
        _ => panic!("slot mismatch: {state:?} against {expected:?}"),
    }
}

#[test]
fn no_owner_binding_delegates_the_read_to_canonical_storage() {
    let slots = TestSlots::empty();
    let called = Cell::new(0);
    let read = read_effective(&slots, &key(Channel::Default), || {
        called.set(called.get() + 1);
        Value::fixnum(41)
    });
    assert_eq!(read, EffectiveRead::Canonical(Value::fixnum(41)));
    assert_eq!(called.get(), 1);
    assert!(!read.is_owner_local());
}

#[test]
fn a_bound_slot_supplies_the_value_and_never_reads_canonical() {
    let slots = TestSlots::new(bound(5), SlotState::NoBinding);
    let called = Cell::new(false);
    let read = read_effective(&slots, &key(Channel::Default), || {
        called.set(true);
        Value::fixnum(99)
    });
    assert_eq!(read, EffectiveRead::OwnerBound(Value::fixnum(5)));
    assert!(read.is_owner_local());
    assert!(!called.get());
}

#[test]
fn a_local_void_binding_is_distinct_from_no_binding() {
    let slots = TestSlots::new(
        SlotState::Bound(SlotValue::new(Value::UNBOUND)),
        SlotState::NoBinding,
    );
    let called = Cell::new(false);
    let read = read_effective(&slots, &key(Channel::Default), || {
        called.set(true);
        Value::fixnum(1)
    });
    assert_eq!(read, EffectiveRead::OwnerVoid);
    assert_eq!(read.value(), None);
    assert!(read.is_owner_local());
    assert!(!called.get());
}

#[test]
fn assignment_replaces_a_bound_owner_slot() {
    let mut slots = TestSlots::new(bound(5), SlotState::NoBinding);
    let called = Cell::new(false);
    let write = write_effective(&mut slots, &key(Channel::Default), Value::fixnum(6), |_| {
        called.set(true)
    });
    assert_eq!(write, EffectiveWrite::OwnerSlot);
    assert!(!called.get());
    assert_slot(slots.slot_state(TlsIndex::VOID, Channel::Default), Some(6));
}

#[test]
fn assignment_without_a_binding_writes_canonical_storage() {
    let mut slots = TestSlots::empty();
    let seen = Cell::new(None);
    let write = write_effective(
        &mut slots,
        &key(Channel::Default),
        Value::fixnum(8),
        |value| {
            seen.set(value.as_fixnum());
        },
    );
    assert_eq!(write, EffectiveWrite::Canonical);
    assert_eq!(seen.get(), Some(8));
    assert_slot(slots.slot_state(TlsIndex::VOID, Channel::Default), None);
}

#[test]
fn default_operations_use_the_default_channel_not_the_access_channel() {
    let buffer_key = key(Channel::BufferLocal(BufferId(7)));
    let slots = TestSlots::new(bound(30), SlotState::NoBinding);

    // The ordinary read meets the buffer-local channel and falls through to
    // canonical storage...
    let ordinary = read_effective(&slots, &buffer_key, || Value::fixnum(9));
    assert_eq!(ordinary, EffectiveRead::Canonical(Value::fixnum(9)));
    // ...while default-value uses the owner's Default-channel binding, so it
    // does not bypass the owner's Plain let.
    let default = read_default_effective(&slots, &buffer_key, || Value::fixnum(9));
    assert_eq!(default, EffectiveRead::OwnerBound(Value::fixnum(30)));

    let mut slots = TestSlots::new(bound(30), SlotState::NoBinding);
    let canonical = Cell::new(false);
    let write = write_default_effective(&mut slots, &buffer_key, Value::fixnum(31), |_| {
        canonical.set(true)
    });
    assert_eq!(write, EffectiveWrite::OwnerSlot);
    assert!(!canonical.get());
    assert_slot(slots.slot_state(TlsIndex::VOID, Channel::Default), Some(31));
    assert_slot(
        slots.slot_state(TlsIndex::VOID, Channel::BufferLocal(BufferId(7))),
        None,
    );
}

#[test]
fn boundp_through_the_rule_treats_owner_void_as_unbound() {
    let slots = TestSlots::new(
        SlotState::Bound(SlotValue::new(Value::UNBOUND)),
        SlotState::NoBinding,
    );
    let canonical = Cell::new(false);
    assert!(!bound_effective(&slots, &key(Channel::Default), || {
        canonical.set(true);
        true
    }));
    assert!(!canonical.get());

    let slots = TestSlots::new(bound(1), SlotState::NoBinding);
    assert!(bound_effective(&slots, &key(Channel::Default), || false));

    // No owner binding: canonical storage answers, and default-boundp asks
    // the Default channel even for a buffer-local access key.
    let buffer_key = key(Channel::BufferLocal(BufferId(7)));
    let slots = TestSlots::empty();
    assert!(default_bound_effective(&slots, &buffer_key, || true));
    assert!(!bound_effective(&slots, &buffer_key, || false));
}

#[test]
fn key_revalidation_compares_era_and_shape() {
    let plain = key(Channel::Default);
    assert!(plain.is_valid_for(EraId::new(3), ShapeGeneration::new(7)));
    assert!(!plain.is_valid_for(EraId::new(4), ShapeGeneration::new(7)));
    assert!(!plain.is_valid_for(EraId::new(3), ShapeGeneration::new(8)));
    assert_eq!(plain.symbol(), NIL_SYM_ID);
    assert_eq!(plain.slot(), TlsIndex::VOID);
    assert_eq!(plain.channel(), Channel::Default);
}

#[test]
fn an_outermost_unlet_exposes_the_latest_canonical_value() {
    // Popping the owner's last binding restores NoBinding; the next read must
    // consult canonical storage again instead of a value captured at bind time.
    let mut slots = TestSlots::new(bound(5), SlotState::NoBinding);
    slots.set_slot_state(TlsIndex::VOID, Channel::Default, SlotState::NoBinding);
    let canonical = Cell::new(1);
    let read = read_effective(&slots, &key(Channel::Default), || {
        canonical.set(canonical.get() + 1);
        Value::fixnum(2)
    });
    assert_eq!(read, EffectiveRead::Canonical(Value::fixnum(2)));
    assert_eq!(canonical.get(), 2, "the latest canonical value is read");
}
