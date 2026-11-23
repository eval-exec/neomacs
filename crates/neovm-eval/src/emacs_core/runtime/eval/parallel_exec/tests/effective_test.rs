//! E4-S0 effective-Plain rule unit tests (pure functions over a test slab).

use super::super::owner_bindings::{
    BindingKind, Channel, OwnerBindingRecord, OwnerRecordId, OwnerSlab, SlotState, SlotValue,
    UndoTarget,
};
use super::effective::*;
use super::frames::SlotRecord;
use neovm_edit::buffer::BufferId;
use neovm_value::forward::TlsIndex;
use neovm_value::intern::NIL_SYM_ID;
use neovm_value::symbol::parallel::PlainSymbolBacking;
use neovm_value::tagged::SymbolMode;
use neovm_value::value::{Value, symbol_id};
use std::cell::{Cell, RefCell};

struct TestSlots {
    default: SlotState,
    buffer_local: SlotState,
    /// The binding-record LIFO, on a real slab: the composed bind and unbind
    /// pair the executor's entry with the record the slab stores (V2, C2).
    records: OwnerSlab,
    canonical: Value,
    canonical_reads: Cell<usize>,
    canonical_writes: Cell<usize>,
    admission: PlainAdmission,
    canonical_bound: bool,
    /// Every write in order: the slot writes and the canonical writes.
    trace: RefCell<Vec<&'static str>>,
}

impl TestSlots {
    #[doc(hidden)]
    pub fn new(default: SlotState, buffer_local: SlotState) -> Self {
        Self {
            default,
            buffer_local,
            records: OwnerSlab::new(0),
            canonical: Value::NIL,
            canonical_reads: Cell::new(0),
            canonical_writes: Cell::new(0),
            admission: PlainAdmission::refuse(PlainRefusal::NotPlain),
            canonical_bound: true,
            trace: RefCell::new(Vec::new()),
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

    fn trace(&self) -> Vec<&'static str> {
        self.trace.borrow().clone()
    }

    /// The slab record an executor entry names; it must name a live one.
    fn stored(&self, record: SlotRecord) -> OwnerBindingRecord {
        *self
            .records
            .record(record.record)
            .expect("the entry names a live slab record")
    }

    /// The slab's top record.
    fn top_record(&self) -> Option<OwnerRecordId> {
        self.records.live_records().last().map(|(id, _)| id)
    }

    /// The slab's live records, outermost first.
    fn live_records(&self) -> usize {
        self.records.live_records().count()
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
        self.trace.borrow_mut().push("slot");
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
        self.trace.borrow_mut().push("canonical");
        self.canonical = value;
    }

    fn canonical_bound(&self, _sym: neovm_value::intern::SymId) -> bool {
        self.canonical_bound
    }

    fn push_binding(
        &mut self,
        sym: neovm_value::intern::SymId,
        slot: TlsIndex,
        channel: Channel,
        undo: UndoTarget,
    ) -> OwnerRecordId {
        self.records
            .push_record(sym, slot, BindingKind::Let, channel, undo)
    }

    fn pop_binding(&mut self, id: OwnerRecordId) -> Option<OwnerBindingRecord> {
        take_top_binding(&mut self.records, id)
    }
}

fn plain_sym(name: &'static str) -> neovm_value::intern::SymId {
    symbol_id(&Value::symbol(name), SymbolMode::Bare).expect("interned test symbol")
}

fn mint(sym: neovm_value::intern::SymId) -> PlainAdmission {
    PlainAdmission::mint(sym, TlsIndex::VOID)
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
            PlainAdmission::mint(NIL_SYM_ID, TlsIndex::VOID)
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
        &PlainAdmission::mint(sym, TlsIndex::VOID),
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

    // No owner binding and no canonical binding: both composed bound
    // questions are false (GNU `default-boundp` on a void Plain variable).
    let mut slots = TestSlots::access(SlotState::NoBinding, Value::NIL, mint(sym));
    slots.canonical_bound = false;
    assert!(!effective_plain_bound(&slots, &key));
    assert!(!effective_plain_default_bound(&slots, &key));
}

#[test]
fn composed_bind_saves_the_outer_value_or_nothing_and_never_touches_canonical() {
    let sym = plain_sym("bind_x");
    let key = EffectivePlainKey::from_admission(&mint(sym), EraId::new(3), ShapeGeneration::new(7))
        .expect("mint");
    let slot = |slots: &TestSlots| slots.slot_state(TlsIndex::VOID, Channel::Default);

    // `(let ((x 7)) ...)` over this owner's binding: the slab record carries
    // the outer value (`specbind`'s `old_value = SYMBOL_VAL (sym)`,
    // eval.c:3671). The returned entry names that record, and nothing else.
    let mut slots = TestSlots::access(bound(4), Value::fixnum(1), mint(sym));
    let record = effective_plain_bind(&mut slots, &key, Value::fixnum(7));
    let stored = slots.stored(record);
    assert_eq!(record.symbol, sym);
    assert_eq!(stored.sym, sym);
    assert_eq!(stored.index, key.slot());
    assert_eq!(stored.channel, Channel::Default);
    assert_eq!(stored.kind, BindingKind::Let);
    assert!(matches!(
        stored.undo,
        UndoTarget::Previous(outer) if outer.get() == Value::fixnum(4)
    ));
    assert_eq!(slots.top_record(), Some(record.record));
    assert_eq!(slots.live_records(), 1);
    assert_slot(slot(&slots), Some(7));
    assert_eq!(slots.canonical_reads(), 0);
    assert_eq!(slots.canonical_writes(), 0);
    assert_eq!(
        slots.trace(),
        Vec::<&str>::from(["slot"]),
        "the one slot write, and no canonical write"
    );

    // A let over that let: the second record saves the first let's value
    // and sits above the first.
    let inner = effective_plain_bind(&mut slots, &key, Value::fixnum(8));
    assert_ne!(inner.record, record.record);
    assert!(matches!(
        slots.stored(inner).undo,
        UndoTarget::Previous(outer) if outer.get() == Value::fixnum(7)
    ));
    assert_eq!(slots.top_record(), Some(inner.record));
    assert_eq!(slots.live_records(), 2);
    assert_slot(slot(&slots), Some(8));

    // No binding of this owner: the record saves nothing; popping it clears
    // the slot and the canonical value shows through again.
    let mut slots = TestSlots::access(SlotState::NoBinding, Value::fixnum(1), mint(sym));
    let record = effective_plain_bind(&mut slots, &key, Value::fixnum(7));
    assert!(matches!(
        slots.stored(record).undo,
        UndoTarget::Inherited { suspended: None }
    ));
    assert_slot(slot(&slots), Some(7));
    assert_eq!(slots.canonical_reads(), 0);
    assert_eq!(slots.canonical_writes(), 0);
}

/// One record of each undo kind, as the slab's LIFO holds it, and the
/// executor's entry naming it.
fn pushed_record(
    slots: &mut TestSlots,
    sym: neovm_value::intern::SymId,
    undo: UndoTarget,
) -> SlotRecord {
    let (slot, channel) = (TlsIndex::VOID, Channel::Default);
    let record = slots.push_binding(sym, slot, channel, undo);
    SlotRecord::new(sym, record)
}

#[test]
fn composed_unbind_runs_each_of_the_four_undo_transitions() {
    let sym = plain_sym("unbind_x");
    let slot = |slots: &TestSlots| slots.slot_state(TlsIndex::VOID, Channel::Default);

    // Retired (GNU's `SPECPDL_NOP`): nothing changes, nothing is written,
    // and the record is taken.
    let mut slots = TestSlots::access(bound(7), Value::fixnum(1), mint(sym));
    let record = pushed_record(&mut slots, sym, UndoTarget::Retired);
    effective_plain_unbind(&mut slots, record).expect("the top record");
    assert_slot(slot(&slots), Some(7));
    assert_eq!(slots.canonical_writes(), 0);
    assert_eq!(slots.live_records(), 0);

    // Previous: the outer binding returns, a void outer binding included.
    let mut slots = TestSlots::access(bound(7), Value::fixnum(1), mint(sym));
    let record = pushed_record(
        &mut slots,
        sym,
        UndoTarget::Previous(SlotValue::new(Value::fixnum(4))),
    );
    effective_plain_unbind(&mut slots, record).expect("the top record");
    assert_slot(slot(&slots), Some(4));
    assert_eq!(slots.trace(), Vec::<&str>::from(["slot"]));
    let mut slots = TestSlots::access(bound(7), Value::fixnum(1), mint(sym));
    let record = pushed_record(
        &mut slots,
        sym,
        UndoTarget::Previous(SlotValue::new(Value::UNBOUND)),
    );
    effective_plain_unbind(&mut slots, record).expect("the top record");
    assert!(matches!(
        slot(&slots),
        SlotState::Bound(outer) if outer.get().is_unbound()
    ));
    assert_eq!(slots.canonical_writes(), 0);

    // Inherited: the slot clears, whatever it suspended; the canonical
    // value shows through again and nothing is written (`an_outermost_unlet_
    // exposes_the_latest_canonical_value`).
    for suspended in [None, Some(SlotValue::new(Value::fixnum(9)))] {
        let mut slots = TestSlots::access(bound(7), Value::fixnum(1), mint(sym));
        let record = pushed_record(&mut slots, sym, UndoTarget::Inherited { suspended });
        effective_plain_unbind(&mut slots, record).expect("the top record");
        assert_slot(slot(&slots), None);
        assert_eq!(slots.canonical_writes(), 0);
        assert_eq!(slots.live_records(), 0);
    }

    // Captured: the slot clears and the captured value returns to canonical
    // storage, in the serial order (slot first: `discard_owner_binding`,
    // owner_bindings.rs:1504-1516).
    let mut slots = TestSlots::access(bound(7), Value::fixnum(1), mint(sym));
    let record = pushed_record(
        &mut slots,
        sym,
        UndoTarget::Captured(SlotValue::new(Value::fixnum(5))),
    );
    effective_plain_unbind(&mut slots, record).expect("the top record");
    assert_slot(slot(&slots), None);
    assert_eq!(slots.canonical, Value::fixnum(5));
    assert_eq!(slots.canonical_writes(), 1);
    assert_eq!(slots.trace(), Vec::<&str>::from(["slot", "canonical"]));
    assert_eq!(slots.live_records(), 0);
}

#[test]
fn composed_unbind_of_a_record_that_is_not_the_top_takes_nothing_and_changes_nothing() {
    // C2: the entry and the slab disagree when the entry names a record
    // below the top, or one already taken. Nothing is taken, nothing is
    // written, and the result is the typed mismatch the caller faults on.
    let sym = plain_sym("unbind_mismatch");
    let slot = |slots: &TestSlots| slots.slot_state(TlsIndex::VOID, Channel::Default);
    let mut slots = TestSlots::access(bound(7), Value::fixnum(1), mint(sym));
    let outer = pushed_record(&mut slots, sym, UndoTarget::Retired);
    let inner = pushed_record(
        &mut slots,
        sym,
        UndoTarget::Previous(SlotValue::new(Value::fixnum(4))),
    );
    assert_eq!(slots.live_records(), 2);

    // The outer record, under the inner one.
    assert_eq!(
        effective_plain_unbind(&mut slots, outer),
        Err(RecordMismatch)
    );
    assert_slot(slot(&slots), Some(7));
    assert_eq!(slots.trace(), Vec::<&str>::new());
    assert_eq!(slots.canonical_writes(), 0);
    assert_eq!(slots.live_records(), 2, "the record below is not holed");
    assert_eq!(slots.top_record(), Some(inner.record));

    // In LIFO order both come off, each undone as it leaves.
    effective_plain_unbind(&mut slots, inner).expect("the top record");
    assert_slot(slot(&slots), Some(4));
    effective_plain_unbind(&mut slots, outer).expect("the top record");
    assert_slot(slot(&slots), Some(4));
    assert_eq!(slots.live_records(), 0);

    // A taken record, named again.
    assert_eq!(
        effective_plain_unbind(&mut slots, inner),
        Err(RecordMismatch)
    );
    assert_slot(slot(&slots), Some(4));
    assert_eq!(slots.trace(), Vec::<&str>::from(["slot"]));
    assert_eq!(slots.live_records(), 0);
}
