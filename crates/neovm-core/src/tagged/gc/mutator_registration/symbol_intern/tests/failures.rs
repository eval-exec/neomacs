//! Real reservation/partial-publication paths, with test-only per-World faults.
use super::*;
use crate::emacs_core::symbol::parallel::InitialInternFault;
use crate::tagged::gc::{GlobalError, InitialInternError, RegistrationError};

fn reservation_refusal(kind: PlainSymbolBacking, query: Query) {
    fixture(kind, query, |run, homes| {
        let id = homes.target.unwrap();
        let before = run.initial_facts_for_test(id).unwrap();
        assert!(!before.present && !before.member);
        let table_before = run
            .initial_table_statistics_for_test()
            .unwrap()
            .retained_bytes;
        let charge = run.owner.local.as_ref().unwrap().hot.bytes_since_gc;
        let output = homes.dst.word_for_test(run);
        let old_cell = homes.old_id;
        // Keep the actual keeper-admitted cell identity across the move;
        // no registry lookup or newly fabricated authority substitutes.
        let old = run.initial_facts_for_test(old_cell).unwrap();
        run.initial_fault_for_test(InitialInternFault::ReservationAfterSpineMove)
            .unwrap();
        assert_eq!(
            intern_retry(run, &homes.name, &mut homes.dst),
            Err(InitialInternError::Authority(RegistrationError::Capacity))
        );
        let after = run.initial_facts_for_test(id).unwrap();
        assert_ne!(
            after.spine_address, before.spine_address,
            "fault must actually relocate the spine"
        );
        assert!(
            after.spine_current,
            "reservation refusal left a retired JIT spine"
        );
        assert_eq!(
            (
                after.members,
                after.epoch,
                after.slots,
                after.chunks,
                after.sides,
                after.present,
                after.member
            ),
            (
                before.members,
                before.epoch,
                before.slots,
                before.chunks,
                before.sides,
                before.present,
                before.member
            )
        );
        assert_eq!(homes.dst.word_for_test(run), output);
        assert!(
            after.bytes > before.bytes,
            "retained reservation capacity is real"
        );
        assert_eq!(
            run.owner.local.as_ref().unwrap().hot.bytes_since_gc - charge,
            after.bytes - before.bytes
                + run
                    .initial_table_statistics_for_test()
                    .unwrap()
                    .retained_bytes
                    .get()
                - table_before.get()
        );
        let old_after = run.initial_facts_for_test(old_cell).unwrap();
        assert_eq!(
            (old_after.cells, old_after.backing, old_after.member),
            (old.cells, old.backing, old.member)
        );
        run.sym_value_into(&homes.old_handle, &mut homes.dst)
            .unwrap();
        assert_eq!(homes.dst.word_for_test(run), homes.name.word_for_test(run));
        assert_eq!(
            run.initial_poison_receipt_for_test(),
            (
                false,
                true,
                InitialInternFault::ReservationAfterSpineMove as u8
            )
        );
        // The fault is one-shot. The same name now succeeds, then behaves
        // like an ordinary hit rather than publishing membership twice.
        intern_retry(run, &homes.name, &mut homes.dst).unwrap();
        let committed = run.initial_facts_for_test(id).unwrap();
        assert_eq!(
            (committed.members, committed.epoch),
            (before.members + 1, before.epoch + 1)
        );
        intern_retry(run, &homes.equal_name, &mut homes.dst).unwrap();
        assert_eq!(
            run.initial_facts_for_test(id).unwrap().epoch,
            committed.epoch
        );
    })
    .expect("clean refusal permits complete root teardown and serial handoff");
}

#[test]
fn p3sym_c5_reservation_refusal_after_spine_move_preserves_old_slots() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        reservation_refusal(kind, Query::Boundary);
    }
}

/// Small provenance/handoff fixture: only the initial chunk, two name
/// operands and an immediate destination. Real spine-capacity growth moves
/// its buffer; no setup collection or large registry-ID gap is needed.
#[test]
fn p3sym_c5_small_spine_move_hands_back_existing_cells() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        reservation_refusal(kind, Query::RegistryOnly);
    }
}

#[test]
fn p3sym_c5_partial_slot_publication_poisons_before_writer_release() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        // Genuine canonical registry identity, absent from this small World.
        let spelling = "p3sym-c5-partial-existing";
        for name in [
            "p3sym-c2-global",
            "p3sym-c3-global-1",
            "p3sym-c3-global-2",
            spelling,
        ] {
            crate::emacs_core::intern::intern(name);
        }
        Context::with_test_symbol_name_inputs_era(kind, spelling, |era, ids| {
            let old = era.admit_plain_symbol(ids[1]).unwrap();
            let mut stopped = era.stop().unwrap();
            let inputs = era
                .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                    // SAFETY: immediate seeds on this exact stopped heap.
                    [
                        SharedRoot::new(heap, Value::NIL),
                        SharedRoot::new(heap, Value::fixnum(23)),
                    ]
                })
                .unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let mut owner = era
                .register(1, 8)
                .unwrap()
                .activate(era.heap_identity(), 1)
                .unwrap();
            {
                let mut run = owner.run_access().unwrap();
                let mut name = run.root_slot(&inputs[0]).unwrap();
                let mut dst = run.root_slot(&inputs[1]).unwrap();
                run.sym_value_into(&old, &mut name).unwrap();
                run.initial_fault_for_test(InitialInternFault::AfterSlotPublication)
                    .unwrap();
                assert_eq!(
                    intern_retry(&mut run, &name, &mut dst),
                    Err(InitialInternError::Authority(RegistrationError::Poisoned))
                );
                assert_eq!(
                    dst.word_for_test(&run),
                    Value::fixnum(23).bits(),
                    "partial publication cannot return a symbol"
                );
                assert_eq!(
                    run.initial_poison_receipt_for_test(),
                    (
                        true,
                        true,
                        128 | InitialInternFault::AfterSlotPublication as u8
                    ),
                    "real poison recorded under WRITER, followed by word-zero release"
                );
                assert_eq!(
                    run.initial_intern_soft_into(&name, &mut dst),
                    Err(InitialInternError::Authority(RegistrationError::Poisoned))
                );
                assert_eq!(
                    run.sym_value_into(&old, &mut dst),
                    Err(GlobalError::Authority(RegistrationError::Poisoned))
                );
                drop(name);
                drop(dst);
                // Normal clearing correctly refuses poison too. A test-only
                // terminal cleanup invalidates private homes without granting
                // World access or making this partial era resumable.
                assert_eq!(run.clear_root_slots(), Err(RegistrationError::Poisoned));
                run.clear_terminal_roots_for_test().unwrap();
            }
            drop(inputs);
            assert!(matches!(
                owner.run_access(),
                Err(RegistrationError::Poisoned)
            ));
            drop(owner);
            assert!(
                matches!(era.stop(), Err(RendezvousError::Poisoned)),
                "poison forbids keeper trace admission"
            );
            let failed = match era.finish() {
                Err(failed) => failed,
                Ok(_) => panic!("partial membership returned serial authority"),
            };
            assert_eq!(failed.reason, RegistrationError::Poisoned);
            // Terminal negative: dropping the failed admission retains backing
            // and never resumes this intentionally inconsistent World.
            drop(failed);
            eprintln!("P3SYM_C5_POISON_RETAINED {kind:?}");
        });
    }
}
