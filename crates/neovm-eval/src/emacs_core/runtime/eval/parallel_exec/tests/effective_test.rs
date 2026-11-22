//! E4-S0 effective-Plain rule unit tests (pure functions over a test slab).

use super::super::owner_bindings::{Channel, SlotState, SlotValue};
use super::effective::*;
use neovm_edit::buffer::BufferId;
use neovm_value::forward::TlsIndex;
use neovm_value::intern::NIL_SYM_ID;
use neovm_value::symbol::parallel::PlainSymbolBacking;
use neovm_value::tagged::SymbolMode;
use neovm_value::value::{Value, symbol_id};
use std::cell::Cell;

struct TestSlots {
    default: SlotState,
    buffer_local: SlotState,
    canonical: Value,
    canonical_reads: Cell<usize>,
    canonical_writes: Cell<usize>,
    admission: PlainAdmission,
}

impl TestSlots {
    #[doc(hidden)]
    pub fn new(default: SlotState, buffer_local: SlotState) -> Self {
        Self {
            default,
            buffer_local,
            canonical: Value::NIL,
            canonical_reads: Cell::new(0),
            canonical_writes: Cell::new(0),
            admission: PlainAdmission::refuse(PlainRefusal::NotPlain),
        }
    }

    #[doc(hidden)]
    pub fn empty() -> Self {
        Self::new(SlotState::NoBinding, SlotState::NoBinding)
    }

    fn access(default: SlotState, canonical: Value, admission: PlainAdmission) -> Self {
        let mut slots = Self::new(default, SlotState::NoBinding);
        slots.canonical = canonical;
        slots.admission = admission;
        slots
    }

    fn canonical_reads(&self) -> usize {
        self.canonical_reads.get()
    }

    fn canonical_writes(&self) -> usize {
        self.canonical_writes.get()
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

impl PlainAccess for TestSlots {
    fn admission(&self, sym: neovm_value::intern::SymId) -> PlainAdmission {
        match self.admission {
            PlainAdmission::Mint { sym: admitted, .. } if admitted == sym => self.admission,
            refusal @ PlainAdmission::Refuse(_) => refusal,
            _ => PlainAdmission::refuse(PlainRefusal::StaleBacking),
        }
    }

    fn canonical_read(&self, _sym: neovm_value::intern::SymId) -> Value {
        self.canonical_reads.set(self.canonical_reads.get() + 1);
        self.canonical
    }

    fn canonical_write(&mut self, _sym: neovm_value::intern::SymId, value: Value) {
        self.canonical_writes.set(self.canonical_writes.get() + 1);
        self.canonical = value;
    }

    fn canonical_bound(&self, _sym: neovm_value::intern::SymId) -> bool {
        true
    }
}

fn plain_sym(name: &'static str) -> neovm_value::intern::SymId {
    symbol_id(&Value::symbol(name), SymbolMode::Bare).expect("interned test symbol")
}

fn mint(sym: neovm_value::intern::SymId) -> PlainAdmission {
    PlainAdmission::mint(sym, TlsIndex::VOID, Channel::Default)
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
    // This is a buffer-local (non-Plain) access key: C1's admission mints only
    // Channel::Default for a Plain backing, so this pins the pure rule's channel
    // split for the serial buffer-local path, not a Plain admission.
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

#[test]
fn admission_refuses_a_constant_even_when_plain() {
    let admission = classify_plain_backing(
        NIL_SYM_ID,
        Some(PlainSymbolBacking::Unprojected),
        true,
        Some(TlsIndex::VOID),
    );
    assert_eq!(admission, PlainAdmission::Refuse(PlainRefusal::Constant));
}

#[test]
fn admission_refuses_a_non_plain_backing() {
    let admission = classify_plain_backing(NIL_SYM_ID, None, false, None);
    assert_eq!(admission, PlainAdmission::Refuse(PlainRefusal::NotPlain));
}

#[test]
fn admission_is_exhaustive_over_plain_backing_kinds() {
    for backing in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
        PlainSymbolBacking::VoidProjection,
    ] {
        assert_eq!(
            classify_plain_backing(NIL_SYM_ID, Some(backing), false, Some(TlsIndex::VOID),),
            PlainAdmission::mint(NIL_SYM_ID, TlsIndex::VOID, Channel::Default)
        );
        assert_eq!(
            classify_plain_backing(NIL_SYM_ID, Some(backing), false, None),
            PlainAdmission::Refuse(PlainRefusal::MissingProjection)
        );
    }
}

#[test]
fn from_admission_mints_the_key_and_carries_the_refusal() {
    let sym = plain_sym("admitted");
    let key = EffectivePlainKey::from_admission(
        &PlainAdmission::mint(sym, TlsIndex::VOID, Channel::Default),
        EraId::new(3),
        ShapeGeneration::new(7),
    )
    .expect("mint");
    assert_eq!(key.symbol(), sym);
    assert_eq!(key.slot(), TlsIndex::VOID);
    assert_eq!(key.channel(), Channel::Default);
    assert!(key.is_valid_for(EraId::new(3), ShapeGeneration::new(7)));

    assert_eq!(
        EffectivePlainKey::from_admission(
            &PlainAdmission::refuse(PlainRefusal::Constant),
            EraId::new(3),
            ShapeGeneration::new(7),
        ),
        Err(PlainRefusal::Constant)
    );
}

#[test]
fn composed_read_owner_bound_never_canonical() {
    let sym = plain_sym("bound");
    let slots = TestSlots::access(bound(5), Value::fixnum(99), mint(sym));
    let key = EffectivePlainKey::from_admission(&mint(sym), EraId::new(3), ShapeGeneration::new(7))
        .expect("mint");
    assert_eq!(
        effective_plain_read(&slots, &key),
        EffectiveRead::OwnerBound(Value::fixnum(5))
    );
    assert_eq!(slots.canonical_reads(), 0, "canonical storage is not read");
}

#[test]
fn composed_read_no_binding_delegates_canonical_once() {
    let sym = plain_sym("unbound");
    let slots = TestSlots::access(SlotState::NoBinding, Value::fixnum(41), mint(sym));
    let key = EffectivePlainKey::from_admission(&mint(sym), EraId::new(3), ShapeGeneration::new(7))
        .expect("mint");
    assert_eq!(
        effective_plain_read(&slots, &key),
        EffectiveRead::Canonical(Value::fixnum(41))
    );
    assert_eq!(slots.canonical_reads(), 1);
}

#[test]
fn composed_read_void_is_unbound_not_canonical() {
    let sym = plain_sym("void");
    let slots = TestSlots::access(
        SlotState::Bound(SlotValue::new(Value::UNBOUND)),
        Value::fixnum(1),
        mint(sym),
    );
    let key = EffectivePlainKey::from_admission(&mint(sym), EraId::new(3), ShapeGeneration::new(7))
        .expect("mint");
    assert_eq!(effective_plain_read(&slots, &key), EffectiveRead::OwnerVoid);
    assert_eq!(
        slots.canonical_reads(),
        0,
        "a local void binding does not read the canonical value"
    );
}

#[test]
fn composed_write_bound_replaces_slot_not_canonical() {
    let sym = plain_sym("write_bound");
    let mut slots = TestSlots::access(bound(5), Value::fixnum(99), mint(sym));
    let key = EffectivePlainKey::from_admission(&mint(sym), EraId::new(3), ShapeGeneration::new(7))
        .expect("mint");
    assert_eq!(
        effective_plain_write(&mut slots, &key, Value::fixnum(6)),
        EffectiveWrite::OwnerSlot
    );
    assert_eq!(slots.canonical_writes(), 0);
    assert_slot(slots.slot_state(TlsIndex::VOID, Channel::Default), Some(6));
    assert_eq!(slots.canonical, Value::fixnum(99));
}

#[test]
fn composed_write_unbound_writes_canonical() {
    let sym = plain_sym("write_unbound");
    let mut slots = TestSlots::access(SlotState::NoBinding, Value::fixnum(1), mint(sym));
    let key = EffectivePlainKey::from_admission(&mint(sym), EraId::new(3), ShapeGeneration::new(7))
        .expect("mint");
    assert_eq!(
        effective_plain_write(&mut slots, &key, Value::fixnum(8)),
        EffectiveWrite::Canonical
    );
    assert_eq!(slots.canonical_writes(), 1);
    assert_eq!(slots.canonical, Value::fixnum(8));
    assert_slot(slots.slot_state(TlsIndex::VOID, Channel::Default), None);
}

#[test]
fn composed_default_ops_use_the_default_channel() {
    let sym = plain_sym("default");
    let mut slots = TestSlots::access(bound(30), Value::fixnum(9), mint(sym));
    let key = EffectivePlainKey::from_admission(&mint(sym), EraId::new(3), ShapeGeneration::new(7))
        .expect("mint");

    // A Plain admission mints only Channel::Default (C1): the ordinary read
    // and default-value are the same rule, so both see the owner's let (GNU
    // `(1 t t 1)`), never the canonical 9.
    assert_eq!(key.channel(), Channel::Default);
    assert_eq!(
        effective_plain_read(&slots, &key),
        EffectiveRead::OwnerBound(Value::fixnum(30))
    );
    assert_eq!(
        effective_plain_default_read(&slots, &key),
        EffectiveRead::OwnerBound(Value::fixnum(30))
    );
    assert_eq!(slots.canonical_reads(), 0);
    assert_eq!(
        effective_plain_default_write(&mut slots, &key, Value::fixnum(31)),
        EffectiveWrite::OwnerSlot
    );
    assert_eq!(slots.canonical_writes(), 0);
    assert_slot(slots.slot_state(TlsIndex::VOID, Channel::Default), Some(31));
}

#[test]
fn composed_bound_treats_void_as_unbound() {
    let sym = plain_sym("bound_p");
    let slots = TestSlots::access(
        SlotState::Bound(SlotValue::new(Value::UNBOUND)),
        Value::fixnum(1),
        mint(sym),
    );
    let key = EffectivePlainKey::from_admission(&mint(sym), EraId::new(3), ShapeGeneration::new(7))
        .expect("mint");
    assert!(!effective_plain_bound(&slots, &key));
    assert!(!effective_plain_default_bound(&slots, &key));

    let slots = TestSlots::access(bound(1), Value::fixnum(1), mint(sym));
    assert!(effective_plain_bound(&slots, &key));
    assert!(effective_plain_default_bound(&slots, &key));
}
