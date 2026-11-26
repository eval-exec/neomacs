//! E4-P2 Slice 2 tests: production `PlainAdmissionTable` + `CanonicalPlain`
//! over C1's `OwnerSlabLease` and `neovm-value`'s narrow C3 plain-symbol seam.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

use neovm_value::forward::TlsIndex;
use neovm_value::intern::{SymId, intern};
use neovm_value::tagged::gc::SymbolHandle;
use neovm_value::value::Value;

use super::canonical_plain::{CanonicalPlain, PlainAdmissionTableBuilder};
use super::effective::{
    EffectivePlainKey, EffectiveRead, EffectiveWrite, EraId, OwnerSlots, OwnerSlotsMut,
    PlainAccess, PlainAccessError, PlainAdmission, PlainRefusal, PlainUnbind, PlainUnbindError,
    ShapeGeneration, effective_plain_bind, effective_plain_bound, effective_plain_default_bound,
    effective_plain_default_read, effective_plain_default_write, effective_plain_read,
    effective_plain_unbind, effective_plain_unbind_checked, effective_plain_write,
};
use super::engine::{
    CoreOutcome, EngineFault, EscapeRequest, OwnerPoll, SealedOps, SerialEscape, Stage1,
    StepOutcome,
};
use super::frames::{FrameArena, SlotRecord};
use super::vm::VmPlain;
use crate::emacs_core::eval::owner_bindings::{Channel, SlotState, SlotValue, UndoTarget};
use crate::emacs_core::eval::world::poll::PollOutcome;
use crate::emacs_core::eval::world::{WorldProofEra, WorldProofEraExt};
use crate::emacs_core::eval::{Context, ThreadingMode};
use neovm_value::tagged::gc::{RegistrationError, RendezvousError};
use std::num::NonZeroU32;

fn finish_epoch() -> NonZeroU64 {
    NonZeroU64::new(1).expect("nonzero epoch")
}

fn fresh_context() -> Context {
    let mut ctx = Context::new_minimal_vm_harness(crate::test_utils::unit_runtime());
    ctx.setup_thread_locals();
    ctx
}

fn project_plain_var(ctx: &mut Context, name: &str, init: Option<Value>) -> (SymId, TlsIndex) {
    let sym = intern(name);
    ctx.set_variable(name, init.unwrap_or(Value::NIL));
    let tls = ctx
        .project_thread_variable_for_test(sym)
        .expect("project plain test variable")
        .index();
    if init.is_none() {
        ctx.owner_canonical_store(sym, Channel::Default, Value::UNBOUND);
    }
    (sym, tls)
}

fn open_proof_era<'h>(ctx: &'h mut Context) -> WorldProofEra<'h> {
    let keeper = ctx.issue_green_baton().expect("issue baton");
    let frozen = ctx
        .capture_parallel_legacy_roots(&keeper)
        .expect("capture frozen legacy roots");
    let heap: *mut neovm_value::tagged::gc::TaggedHeap = ctx.tagged_heap.heap_root().as_ptr();
    // SAFETY: sole serial Context in test fixture; capture completed before
    // this heap loan, and `keeper` excludes concurrent mark.
    let collector = unsafe {
        (*heap).drain_to_quiescent(|heap| {
            ctx.seed_registered_mutator_roots_world_stopped(heap);
        })
    }
    .expect("drain to quiescent");
    WorldProofEra::admit_rooted(collector, keeper, frozen).expect("admit proof era")
}

#[test]
fn s2_closed_deny_by_default_admission_and_unadmitted_refusal() {
    let mut ctx = fresh_context();
    let heap = ctx.tagged_heap.heap_identity();
    let (sym_ok, tls_ok) = project_plain_var(&mut ctx, "e4p2-s2-admit-ok", Some(Value::fixnum(10)));
    let (sym_const, tls_const) =
        project_plain_var(&mut ctx, "e4p2-s2-admit-const", Some(Value::fixnum(20)));
    let (sym_watched, tls_watched) =
        project_plain_var(&mut ctx, "e4p2-s2-admit-watched", Some(Value::fixnum(30)));
    let sym_unproj = intern("e4p2-s2-admit-unproj");
    ctx.set_variable("e4p2-s2-admit-unproj", Value::fixnum(40));
    let sym_absent = intern("e4p2-s2-admit-absent");
    let mut handle = ctx
        .take_parallel_owner_slab_handle()
        .expect("extract owner slab handle");

    let era = open_proof_era(&mut ctx);
    let h_ok = era.admit_plain_symbol(sym_ok).expect("admit sym_ok");
    let h_const = era.admit_plain_symbol(sym_const).expect("admit sym_const");
    let h_watched = era
        .admit_plain_symbol(sym_watched)
        .expect("admit sym_watched");
    let h_unproj = era
        .admit_plain_symbol(sym_unproj)
        .expect("admit sym_unproj");

    let mut builder = PlainAdmissionTableBuilder::new();
    builder
        .insert(h_ok, Some(tls_ok), false, false)
        .insert(h_const, Some(tls_const), true, false)
        .insert(h_watched, Some(tls_watched), false, true)
        .insert(h_unproj, None, false, false);
    let table = builder.build(EraId::new(11), ShapeGeneration::new(2));

    let permit = era.register(1, 8).expect("register owner");
    let mut mutator = permit.activate(heap, 1).expect("activate owner");
    {
        let mut run = mutator.run_access().expect("run stretch");
        let lease = handle.lease(&mut run).expect("mint slab lease");
        let mut plain = CanonicalPlain::new(lease, &table);

        assert_eq!(
            plain.generation(),
            (EraId::new(11), ShapeGeneration::new(2))
        );
        assert_eq!(
            plain.admission(sym_ok),
            PlainAdmission::Mint {
                sym: sym_ok,
                slot: tls_ok,
            }
        );
        assert_eq!(
            plain.admission(sym_const),
            PlainAdmission::Refuse(PlainRefusal::Constant)
        );
        assert_eq!(
            plain.admission(sym_watched),
            PlainAdmission::Refuse(PlainRefusal::NotPlain)
        );
        assert_eq!(
            plain.admission(sym_unproj),
            PlainAdmission::Refuse(PlainRefusal::MissingProjection)
        );
        // Closed deny-by-default: absent SymId refuses without fallback classification.
        assert_eq!(
            plain.admission(sym_absent),
            PlainAdmission::Refuse(PlainRefusal::NotPlain)
        );
        assert!(
            plain.unbind_needs_serial(sym_absent),
            "absent symbol must fail closed on unbind_needs_serial"
        );
        assert_eq!(
            plain.canonical_read(sym_absent),
            Err(PlainAccessError::Publication)
        );
        assert_eq!(
            plain.canonical_write(sym_absent, Value::fixnum(1)),
            Err(PlainAccessError::Publication)
        );
        assert_eq!(
            plain.canonical_bound(sym_absent),
            Err(PlainAccessError::Publication)
        );
        let _lease = plain.into_lease();
    }

    mutator.finish(finish_epoch()).expect("finish mutator");
    let (collector, keeper) = era.finish().expect("finish era");
    drop(collector);
    drop(keeper);
}

#[test]
fn s2_canonical_read_write_bound_and_owner_slot_precedence_over_real_c3() {
    let mut ctx = fresh_context();
    let heap = ctx.tagged_heap.heap_identity();
    let (sym_x, tls_x) = project_plain_var(&mut ctx, "e4p2-s2-x", Some(Value::fixnum(100)));
    let (sym_void, tls_void) = project_plain_var(&mut ctx, "e4p2-s2-void", None);
    let mut handle = ctx
        .take_parallel_owner_slab_handle()
        .expect("extract owner slab handle");

    let era = open_proof_era(&mut ctx);
    let h_x = era.admit_plain_symbol(sym_x).expect("admit sym_x");
    let h_void = era.admit_plain_symbol(sym_void).expect("admit sym_void");

    let mut builder = PlainAdmissionTableBuilder::new();
    builder
        .insert(h_x, Some(tls_x), false, false)
        .insert(h_void, Some(tls_void), false, false);
    let table = builder.build(EraId::new(1), ShapeGeneration::new(1));

    let permit = era.register(1, 8).expect("register owner");
    let mut mutator = permit.activate(heap, 1).expect("activate owner");
    {
        let mut run = mutator.run_access().expect("run stretch");
        let lease = handle.lease(&mut run).expect("mint slab lease");
        let mut plain = CanonicalPlain::new(lease, &table);

        let key_x = EffectivePlainKey::from_admission(
            &plain.admission(sym_x),
            EraId::new(1),
            ShapeGeneration::new(1),
        )
        .expect("mint key_x");
        let key_void = EffectivePlainKey::from_admission(
            &plain.admission(sym_void),
            EraId::new(1),
            ShapeGeneration::new(1),
        )
        .expect("mint key_void");

        // 1. Canonical read & boundp fallthrough (NoBinding).
        assert_eq!(
            effective_plain_read(&mut plain, &key_x),
            Ok(EffectiveRead::Canonical(Value::fixnum(100)))
        );
        assert_eq!(effective_plain_bound(&mut plain, &key_x), Ok(true));
        assert_eq!(effective_plain_default_bound(&mut plain, &key_x), Ok(true));
        assert_eq!(
            effective_plain_read(&mut plain, &key_void),
            Ok(EffectiveRead::Canonical(Value::UNBOUND))
        );
        assert_eq!(effective_plain_bound(&mut plain, &key_void), Ok(false));

        // 2. Canonical write with no owner binding updates shared C3 cell.
        assert_eq!(
            effective_plain_write(&mut plain, &key_x, Value::fixnum(200)),
            Ok(EffectiveWrite::Canonical)
        );
        assert_eq!(
            effective_plain_default_read(&mut plain, &key_x),
            Ok(EffectiveRead::Canonical(Value::fixnum(200)))
        );

        // 3. C2 let-bind pushes onto real OwnerSlab LIFO; shadows canonical.
        let outer = effective_plain_bind(&mut plain, &key_x, Value::fixnum(300));
        assert_eq!(plain.top_record(), Some(outer.record));
        assert!(plain.record_live(outer.record));
        assert_eq!(
            effective_plain_read(&mut plain, &key_x),
            Ok(EffectiveRead::OwnerBound(Value::fixnum(300)))
        );
        // Write while bound stays in owner slot; canonical stays 200.
        assert_eq!(
            effective_plain_default_write(&mut plain, &key_x, Value::fixnum(301)),
            Ok(EffectiveWrite::OwnerSlot)
        );
        assert_eq!(plain.canonical_read(sym_x), Ok(Value::fixnum(200)));

        // Nested bind + local OwnerVoid check.
        let inner = effective_plain_bind(&mut plain, &key_x, Value::UNBOUND);
        assert_eq!(plain.top_record(), Some(inner.record));
        assert_eq!(
            effective_plain_read(&mut plain, &key_x),
            Ok(EffectiveRead::OwnerVoid)
        );
        assert_eq!(effective_plain_bound(&mut plain, &key_x), Ok(false));

        // 4. Non-top unbind refuses with RecordMismatch without touching slab or slot.
        assert_eq!(
            effective_plain_unbind_checked(&mut plain, outer),
            Err(PlainUnbindError::RecordMismatch)
        );
        assert_eq!(plain.top_record(), Some(inner.record));
        assert!(plain.record_live(outer.record));
        assert!(plain.record_live(inner.record));

        // 5. LIFO unbind restores Previous (301), then Inherited -> canonical (200).
        assert_eq!(
            effective_plain_unbind_checked(&mut plain, inner),
            Ok(PlainUnbind::Done)
        );
        assert!(!plain.record_live(inner.record));
        assert_eq!(
            effective_plain_read(&mut plain, &key_x),
            Ok(EffectiveRead::OwnerBound(Value::fixnum(301)))
        );
        assert_eq!(
            effective_plain_unbind_checked(&mut plain, outer),
            Ok(PlainUnbind::Done)
        );
        assert_eq!(plain.top_record(), None);
        assert_eq!(
            effective_plain_read(&mut plain, &key_x),
            Ok(EffectiveRead::Canonical(Value::fixnum(200)))
        );

        // 6. Captured undo target writes back to real C3 canonical storage.
        plain.set_slot_state(
            tls_x,
            Channel::Default,
            SlotState::Bound(SlotValue::new(Value::fixnum(999))),
        );
        let captured_id = plain.push_binding(
            sym_x,
            tls_x,
            Channel::Default,
            UndoTarget::Captured(SlotValue::new(Value::fixnum(777))),
        );
        let captured = SlotRecord::new(sym_x, captured_id);
        assert_eq!(effective_plain_unbind(&mut plain, captured), Ok(()));
        assert!(matches!(
            plain.slot_state(tls_x, Channel::Default),
            SlotState::NoBinding
        ));
        assert_eq!(
            effective_plain_read(&mut plain, &key_x),
            Ok(EffectiveRead::Canonical(Value::fixnum(777)))
        );
    }

    mutator.finish(finish_epoch()).expect("finish mutator");
    let (collector, keeper) = era.finish().expect("finish era");
    drop(collector);
    drop(keeper);

    // Canonical write through C3 persisted to the Context's real obarray!
    assert_eq!(
        ctx.owner_canonical_value(sym_x, Channel::Default),
        Value::fixnum(777)
    );
}

#[test]
fn s2_unbind_needs_serial_checks_both_live_watched_bit_and_shape_generation() {
    let mut ctx = fresh_context();
    let heap = ctx.tagged_heap.heap_identity();
    let (sym_a, tls_a) = project_plain_var(&mut ctx, "e4p2-s2-watch-a", Some(Value::fixnum(1)));
    let (sym_b, tls_b) = project_plain_var(&mut ctx, "e4p2-s2-watch-b", Some(Value::fixnum(2)));
    let mut handle = ctx
        .take_parallel_owner_slab_handle()
        .expect("extract owner slab handle");

    let era = open_proof_era(&mut ctx);
    let h_a = era.admit_plain_symbol(sym_a).expect("admit a");
    let h_b = era.admit_plain_symbol(sym_b).expect("admit b");

    let mut builder = PlainAdmissionTableBuilder::new();
    builder
        .insert(h_a, Some(tls_a), false, false)
        .insert(h_b, Some(tls_b), false, false);
    let table = builder.build(EraId::new(5), ShapeGeneration::new(10));

    let permit = era.register(1, 8).expect("register owner");
    let mut mutator = permit.activate(heap, 1).expect("activate owner");
    {
        let mut run = mutator.run_access().expect("run stretch");
        let lease = handle.lease(&mut run).expect("mint slab lease");
        let mut plain = CanonicalPlain::new(lease, &table);

        let key_a = EffectivePlainKey::from_admission(
            &plain.admission(sym_a),
            EraId::new(5),
            ShapeGeneration::new(10),
        )
        .expect("mint key_a");
        let key_b = EffectivePlainKey::from_admission(
            &plain.admission(sym_b),
            EraId::new(5),
            ShapeGeneration::new(10),
        )
        .expect("mint key_b");

        let rec_b = effective_plain_bind(&mut plain, &key_b, Value::fixnum(20));
        let rec_a = effective_plain_bind(&mut plain, &key_a, Value::fixnum(10));
        assert!(!plain.unbind_needs_serial(sym_a));
        assert!(!plain.unbind_needs_serial(sym_b));

        // Serial turn flips sym_a's live watched bit -> sym_a needs serial, sym_b does not.
        assert!(table.set_symbol_watched_or_retired(sym_a, true));
        assert!(plain.unbind_needs_serial(sym_a));
        assert!(!plain.unbind_needs_serial(sym_b));
        assert_eq!(
            plain.admission(sym_a),
            PlainAdmission::Refuse(PlainRefusal::NotPlain)
        );
        // Checked unbind leaves rec_a untouched on the slab and returns NeedsSerial.
        assert_eq!(
            effective_plain_unbind_checked(&mut plain, rec_a),
            Ok(PlainUnbind::NeedsSerial { symbol: sym_a })
        );
        assert!(plain.record_live(rec_a.record));
        assert_eq!(plain.top_record(), Some(rec_a.record));

        // Clear sym_a's watched bit -> owner-local unbind succeeds again.
        assert!(table.set_symbol_watched_or_retired(sym_a, false));
        assert_eq!(
            effective_plain_unbind_checked(&mut plain, rec_a),
            Ok(PlainUnbind::Done)
        );

        // Bumping shape generation makes even unwatched sym_b fail closed to NeedsSerial.
        let bumped = table.bump_shape_generation();
        assert_eq!(bumped, ShapeGeneration::new(11));
        assert_eq!(
            plain.generation(),
            (EraId::new(5), ShapeGeneration::new(11))
        );
        assert_eq!(
            plain.stretch_generation(),
            (EraId::new(5), ShapeGeneration::new(10))
        );
        assert!(
            plain.unbind_needs_serial(sym_b),
            "stale shape generation must fail closed on unbind_needs_serial"
        );
        assert_eq!(
            effective_plain_unbind_checked(&mut plain, rec_b),
            Ok(PlainUnbind::NeedsSerial { symbol: sym_b })
        );
        assert!(plain.record_live(rec_b.record));
        // Clean up the binding before exit.
        let _ = plain.pop_binding(rec_b.record);
        plain.set_slot_state(tls_b, Channel::Default, SlotState::NoBinding);
    }

    mutator.finish(finish_epoch()).expect("finish mutator");
    let (collector, keeper) = era.finish().expect("finish era");
    drop(collector);
    drop(keeper);
}

#[test]
fn s2_authority_errors_surface_typed_plain_access_error_never_panic() {
    // Mint a real SymbolHandle on World A, then feed it into a table used with World B:
    // every canonical operation must return Err(PlainAccessError::StaleRegistration).
    let mut ctx_a = fresh_context();
    let (sym, tls_a) = project_plain_var(&mut ctx_a, "e4p2-s2-foreign-sym", Some(Value::fixnum(5)));
    let era_a = open_proof_era(&mut ctx_a);
    let foreign_handle: SymbolHandle = era_a.admit_plain_symbol(sym).expect("admit on era A");
    let (collector_a, keeper_a) = era_a.finish().expect("finish era A");
    drop(collector_a);
    drop(keeper_a);
    let _parked_a = ctx_a.park_for_test();

    let mut ctx_b = fresh_context();
    let heap_b = ctx_b.tagged_heap.heap_identity();
    let mut handle_b = ctx_b
        .take_parallel_owner_slab_handle()
        .expect("extract handle on world B");
    let era_b = open_proof_era(&mut ctx_b);

    let mut builder = PlainAdmissionTableBuilder::new();
    builder.insert(foreign_handle, Some(tls_a), false, false);
    let foreign_table = builder.build(EraId::new(2), ShapeGeneration::new(1));

    let permit = era_b.register(1, 8).expect("register on era B");
    let mut mutator = permit.activate(heap_b, 1).expect("activate on era B");
    {
        let mut run = mutator.run_access().expect("run stretch on era B");
        let lease = handle_b.lease(&mut run).expect("mint lease on era B");
        let mut plain = CanonicalPlain::new(lease, &foreign_table);
        let key = EffectivePlainKey::from_admission(
            &plain.admission(sym),
            EraId::new(2),
            ShapeGeneration::new(1),
        )
        .expect("mint key");

        assert_eq!(
            effective_plain_read(&mut plain, &key),
            Err(PlainAccessError::StaleRegistration)
        );
        assert_eq!(
            effective_plain_write(&mut plain, &key, Value::fixnum(9)),
            Err(PlainAccessError::StaleRegistration)
        );
        assert_eq!(
            effective_plain_bound(&mut plain, &key),
            Err(PlainAccessError::StaleRegistration)
        );

        // Captured unbind against stale canonical storage returns PlainUnbindError::Access.
        let rec_id = plain.push_binding(
            sym,
            tls_a,
            Channel::Default,
            UndoTarget::Captured(SlotValue::new(Value::fixnum(12))),
        );
        assert_eq!(
            effective_plain_unbind(&mut plain, SlotRecord::new(sym, rec_id)),
            Err(PlainUnbindError::Access(
                PlainAccessError::StaleRegistration
            ))
        );
    }

    mutator.finish(finish_epoch()).expect("finish mutator B");
    let (collector_b, keeper_b) = era_b.finish().expect("finish era B");
    drop(collector_b);
    drop(keeper_b);
    drop(handle_b);
    drop(ctx_b);

    // Also verify RegistrationError -> PlainAccessError exhaustive variants.
    assert_eq!(
        PlainAccessError::from(RegistrationError::Busy),
        PlainAccessError::Busy
    );
    assert_eq!(
        PlainAccessError::from(RegistrationError::Poisoned),
        PlainAccessError::Poisoned
    );
    assert_eq!(
        PlainAccessError::from(RegistrationError::StaleRegistration),
        PlainAccessError::StaleRegistration
    );
    assert_eq!(
        PlainAccessError::from(RegistrationError::Publication),
        PlainAccessError::Publication
    );
    assert_eq!(
        PlainAccessError::from(RegistrationError::Capacity),
        PlainAccessError::Capacity
    );

    // Poisoned era surfaces Err(PlainAccessError::Poisoned) on read/write/bound.
    let mut ctx_p = fresh_context();
    let heap_p = ctx_p.tagged_heap.heap_identity();
    let (sym_p, tls_p) =
        project_plain_var(&mut ctx_p, "e4p2-s2-poison-sym", Some(Value::fixnum(3)));
    let mut handle_p = ctx_p
        .take_parallel_owner_slab_handle()
        .expect("extract handle on world P");
    let era_p = open_proof_era(&mut ctx_p);
    let h_p = era_p.admit_plain_symbol(sym_p).expect("admit on era P");
    let mut b_p = PlainAdmissionTableBuilder::new();
    b_p.insert(h_p, Some(tls_p), false, false);
    let table_p = b_p.build(EraId::new(3), ShapeGeneration::new(1));

    let permit_1 = era_p.register(1, 8).expect("register owner 1");
    let permit_2 = era_p.register(2, 8).expect("register owner 2");
    let mut mutator_1 = permit_1.activate(heap_p, 1).expect("activate owner 1");
    let mutator_2 = permit_2.activate(heap_p, 2).expect("activate owner 2");
    {
        let mut run = mutator_1.run_access().expect("enter run stretch");
        let lease = handle_p.lease(&mut run).expect("mint lease");
        let mut plain = CanonicalPlain::new(lease, &table_p);
        // Dropping an unfinished peer mutator poisons the era registry.
        drop(mutator_2);
        assert!(era_p.registry_poisoned_for_test());
        assert_eq!(plain.canonical_read(sym_p), Err(PlainAccessError::Poisoned));
        assert_eq!(
            plain.canonical_write(sym_p, Value::fixnum(9)),
            Err(PlainAccessError::Poisoned)
        );
        assert_eq!(
            plain.canonical_bound(sym_p),
            Err(PlainAccessError::Poisoned)
        );
    }
    drop(mutator_1);
    let _ = era_p.finish();
}

struct NoopEscape;
impl SerialEscape for NoopEscape {
    fn escape(&mut self, _request: EscapeRequest) {}
}

struct NoopSealed;
impl SealedOps for NoopSealed {
    fn read_cons_field(&mut self, _ctx: &mut Context, _name: &str, _args: &[Value]) -> CoreOutcome {
        CoreOutcome::MissingCore
    }
    fn read_header_word(
        &mut self,
        _ctx: &mut Context,
        _name: &str,
        _args: &[Value],
    ) -> CoreOutcome {
        CoreOutcome::MissingCore
    }
}

struct NoopPoll;
impl OwnerPoll for NoopPoll {
    fn interp_call_entry(&mut self, _arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        Ok(PollOutcome::Continue)
    }
    fn interp_back_edge(&mut self, _arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        Ok(PollOutcome::Continue)
    }
    fn serial_request(&mut self, _arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        Ok(PollOutcome::Continue)
    }
}

fn assert_vm_plain<P: VmPlain>(plain: &P) -> (EraId, ShapeGeneration) {
    plain.generation()
}

#[test]
fn s2_stage1_engine_propagates_canonical_plain_read_void_and_authority_fault() {
    // Prepare a foreign-era handle to exercise Stage1's EngineFault::PlainAccess path.
    let mut ctx_foreign = fresh_context();
    let (sym_stale, tls_stale) = project_plain_var(
        &mut ctx_foreign,
        "e4p2-s2-stage1-stale",
        Some(Value::fixnum(7)),
    );
    let era_foreign = open_proof_era(&mut ctx_foreign);
    let h_stale = era_foreign
        .admit_plain_symbol(sym_stale)
        .expect("admit stale");
    let (cf, kf) = era_foreign.finish().expect("finish foreign era");
    drop(cf);
    drop(kf);
    drop(ctx_foreign);

    let mut ctx =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    ctx.setup_thread_locals();
    let heap = ctx.tagged_heap.heap_identity();
    let (sym_ok, tls_ok) =
        project_plain_var(&mut ctx, "e4p2-s2-stage1-ok", Some(Value::fixnum(555)));
    let (sym_void, tls_void) =
        project_plain_var(&mut ctx, "e4p2-s2-stage1-void", Some(Value::fixnum(888)));
    let form_ok = Value::list(vec![Value::from_sym_id(sym_ok)]);
    let form_void = Value::list(vec![Value::from_sym_id(sym_void)]);
    let form_stale = Value::list(vec![Value::from_sym_id(sym_stale)]);
    // Keep the bare-symbol form lists rooted on `ctx` across GC and the era.
    ctx.lexenv = Value::list(vec![form_ok, form_void, form_stale]);
    ctx.gc_collect_exact();
    ctx.tagged_heap.close_alloc_regions();
    let form_ok_bits = form_ok.bits();
    let form_void_bits = form_void.bits();
    let form_stale_bits = form_stale.bits();
    let bootstrap = ctx.capture_green_bootstrap_for_test();

    let keeper = ctx.issue_green_baton().expect("issue baton");
    let frozen = ctx
        .capture_parallel_legacy_roots(&keeper)
        .expect("capture frozen legacy roots");
    keeper.install_stowed_green_runtime_for_test(bootstrap);
    let world_handle = keeper.test_world_handle_for_test();
    let heap_ptr: *mut neovm_value::tagged::gc::TaggedHeap = ctx.tagged_heap.heap_root().as_ptr();
    // SAFETY: sole serial Context in test fixture; capture completed before
    // this heap loan, and `keeper` excludes concurrent mark.
    let collector = unsafe {
        (*heap_ptr).drain_to_quiescent(|heap| {
            ctx.seed_registered_mutator_roots_world_stopped(heap);
        })
    }
    .expect("drain to quiescent");
    let era = WorldProofEra::admit_rooted(collector, keeper, frozen).expect("admit proof era");

    let h_ok = era.admit_plain_symbol(sym_ok).expect("admit ok");
    let h_void = era.admit_plain_symbol(sym_void).expect("admit void");

    let mut builder = PlainAdmissionTableBuilder::new();
    builder
        .insert(h_ok, Some(tls_ok), false, false)
        .insert(h_void, Some(tls_void), false, false)
        .insert(h_stale, Some(tls_stale), false, false);
    let table = builder.build(EraId::new(4), ShapeGeneration::new(2));

    let permit = era.register(1, 8).expect("register owner");
    std::thread::scope(|scope| {
        let table_ref = &table;
        scope
            .spawn(move || {
                // Seated worker Context shares `ctx`'s HeapIdentity on its own OS thread.
                let (mut owner_ctx, retention) = world_handle.spawn_seated_context_for_test(1);
                let mut handle = owner_ctx
                    .take_parallel_owner_slab_handle()
                    .expect("extract worker owner slab handle");
                let mut mutator = permit.activate(heap, 1).expect("activate owner");
                {
                    let mut run = mutator.run_access().expect("run stretch");
                    let lease = handle.lease(&mut run).expect("mint slab lease");
                    let mut plain = CanonicalPlain::new(lease, table_ref);
                    assert_eq!(
                        assert_vm_plain(&plain),
                        (EraId::new(4), ShapeGeneration::new(2))
                    );
                    // Plant an owner-local void binding on sym_void.
                    plain.set_slot_state(
                        tls_void,
                        Channel::Default,
                        SlotState::Bound(SlotValue::new(Value::UNBOUND)),
                    );

                    let mut stage1 = Stage1::new(
                        plain,
                        NoopEscape,
                        NoopSealed,
                        NoopPoll,
                        EraId::new(4),
                        ShapeGeneration::new(2),
                    );
                    let interval = NonZeroU32::new(8).expect("nonzero");

                    // SAFETY: all three cons lists were allocated on `ctx`'s
                    // heap and rooted in `ctx.lexenv` for the entire era; no GC
                    // runs during this active `RunAccess` stretch.
                    let form_ok = unsafe { Value::from_bits(form_ok_bits) };
                    let form_void = unsafe { Value::from_bits(form_void_bits) };
                    let form_stale = unsafe { Value::from_bits(form_stale_bits) };

                    // 1. Bare Plain symbol reads canonical value through C3.
                    match stage1.start(&mut owner_ctx, form_ok, interval) {
                        StepOutcome::Done { pending, .. } => {
                            assert_eq!(
                                stage1.take(pending).expect("take root"),
                                Value::fixnum(555)
                            );
                        }
                        other => panic!(
                            "expected Done(555), got {:?}",
                            std::mem::discriminant(&other)
                        ),
                    }

                    // 2. Local OwnerVoid binding yields StepOutcome::VoidVariable without reading C3.
                    match stage1.start(&mut owner_ctx, form_void, interval) {
                        StepOutcome::VoidVariable { symbol } => {
                            assert_eq!(symbol, Value::from_sym_id(sym_void));
                        }
                        other => panic!(
                            "expected VoidVariable, got {:?}",
                            std::mem::discriminant(&other)
                        ),
                    }

                    // 3. Stale SymbolHandle yields StepOutcome::Fault(EngineFault::PlainAccess(StaleRegistration)).
                    match stage1.start(&mut owner_ctx, form_stale, interval) {
                        StepOutcome::Fault(EngineFault::PlainAccess(
                            PlainAccessError::StaleRegistration,
                        )) => {}
                        other => panic!(
                            "expected Fault(PlainAccess(StaleRegistration)), got {:?}",
                            std::mem::discriminant(&other)
                        ),
                    }
                }
                mutator.finish(finish_epoch()).expect("finish mutator");
                drop(handle);
                drop(retention);
                owner_ctx.retire_green_owner();
            })
            .join()
            .expect("stage1 worker thread panicked");
    });

    let (collector, keeper) = era.finish().expect("finish era");
    keeper.uninstall_green_runtime_for_test();
    drop(collector);
    drop(keeper);
}

/// Real-thread concurrency + TSan row for `#15180`:
///
/// A serial-turn writer thread flips `AdmittedPlainEntry::watched_or_retired`
/// (`Ordering::Release`) while a parallel-owner reader thread continuously
/// calls `PlainAdmissionTable::unbind_needs_serial` and `admission`
/// (`Ordering::Acquire`), with synchronized phase checkpoints asserting exact
/// old/new visibility and zero torn/stale observations.
#[test]
fn s2_real_thread_serial_turn_watched_bit_flip_visible_lock_free_under_tsan() {
    const TURNS: usize = 64;

    let mut ctx = fresh_context();
    let (sym, tls) = project_plain_var(&mut ctx, "e4p2-s2-tsan-watch", Some(Value::fixnum(1)));
    let (sym_other, tls_other) =
        project_plain_var(&mut ctx, "e4p2-s2-tsan-other", Some(Value::fixnum(2)));
    let era = open_proof_era(&mut ctx);
    let h_sym = era.admit_plain_symbol(sym).expect("admit sym");
    let h_other = era.admit_plain_symbol(sym_other).expect("admit sym_other");
    let (collector, keeper) = era.finish().expect("finish era");
    drop(collector);
    drop(keeper);

    let mut builder = PlainAdmissionTableBuilder::new();
    builder
        .insert(h_sym, Some(tls), false, false)
        .insert(h_other, Some(tls_other), false, false);
    let table = Arc::new(builder.build(EraId::new(9), ShapeGeneration::new(3)));

    let start_barrier = Arc::new(Barrier::new(2));
    let done = Arc::new(AtomicBool::new(false));
    let observations = Arc::new(AtomicUsize::new(0));

    // Phase 1: continuous unsynchronized concurrent flips vs lock-free reads
    // (TSan verifies Release/Acquire is race-free with zero Mutex on read path).
    std::thread::scope(|scope| {
        let table_r = Arc::clone(&table);
        let start_r = Arc::clone(&start_barrier);
        let done_r = Arc::clone(&done);
        let obs_r = Arc::clone(&observations);

        let reader = scope.spawn(move || {
            start_r.wait();
            let mut saw_true = false;
            let mut saw_false = false;
            while !done_r.load(Ordering::Acquire) {
                let needs =
                    table_r.unbind_needs_serial(sym, EraId::new(9), ShapeGeneration::new(3));
                if needs {
                    saw_true = true;
                } else {
                    saw_false = true;
                }
                // `sym_other` is never flipped; must NEVER observe a cross-symbol mix.
                assert!(
                    !table_r
                        .unbind_needs_serial(sym_other, EraId::new(9), ShapeGeneration::new(3),),
                    "unrelated symbol must never observe another symbol's watched flip"
                );
                // `admission` must be either Mint(sym) or Refuse(NotPlain), never anything else.
                match table_r.admission(sym) {
                    PlainAdmission::Mint { sym: s, .. } => assert_eq!(s, sym),
                    PlainAdmission::Refuse(PlainRefusal::NotPlain) => {}
                    other => panic!("unexpected admission state during watched flip: {other:?}"),
                }
                obs_r.fetch_add(1, Ordering::Relaxed);
            }
            (saw_true, saw_false)
        });

        start_barrier.wait();
        for turn in 0..TURNS {
            let next = turn % 2 == 1;
            assert!(table.set_symbol_watched_or_retired(sym, next));
            std::thread::yield_now();
        }
        // Final deterministic turn state before Phase 2 barrier check.
        assert!(table.set_symbol_watched_or_retired(sym, true));
        done.store(true, Ordering::Release);

        let (_saw_true, _saw_false) = reader.join().expect("join parallel reader");
    });

    assert!(observations.load(Ordering::Relaxed) > 0);

    // Phase 2: synchronized serial-turn handoff proves non-stale Acquire observation.
    let turn_barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|scope| {
        let table_r = Arc::clone(&table);
        let barrier_r = Arc::clone(&turn_barrier);
        let reader = scope.spawn(move || {
            for turn in 0..16 {
                barrier_r.wait();
                let expected = turn % 2 == 0;
                assert_eq!(
                    table_r.unbind_needs_serial(sym, EraId::new(9), ShapeGeneration::new(3)),
                    expected,
                    "turn {turn}: Acquire read must see exact Release-published watched bit"
                );
                if expected {
                    assert_eq!(
                        table_r.admission(sym),
                        PlainAdmission::Refuse(PlainRefusal::NotPlain)
                    );
                } else {
                    assert_eq!(
                        table_r.admission(sym),
                        PlainAdmission::Mint { sym, slot: tls }
                    );
                }
                barrier_r.wait();
            }
        });

        for turn in 0..16 {
            let expected = turn % 2 == 0;
            assert!(table.set_symbol_watched_or_retired(sym, expected));
            turn_barrier.wait();
            turn_barrier.wait();
        }
        reader.join().expect("join phase 2 reader");
    });
}
