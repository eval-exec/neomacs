//! A prepared fresh registry transaction must remain unpublished when World
//! preparation refuses, or when partial World publication closes the era.
use super::*;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::intern::parallel::{
    FreshRegistryFacts, fresh_registry_facts_for_test, identity_census_for_test,
    name_publication_for_test,
};
use crate::emacs_core::symbol::parallel::InitialInternFault;
use crate::tagged::gc::{GlobalError, InitialInternError, RegistrationError};

fn unpublished(before: FreshRegistryFacts, after: FreshRegistryFacts) {
    assert!(!before.id_present && !before.input_rooted && before.canonical_id.is_none());
    assert!(
        after.bytes >= before.bytes,
        "capacity reservations remain retained"
    );
    assert_eq!(
        after,
        FreshRegistryFacts {
            bytes: after.bytes,
            ..before
        },
        "no ID, atom, mapping, exact root or epoch may publish before World success"
    );
}

fn refusal(kind: PlainSymbolBacking, query: Query) {
    fixture(kind, query, |run, homes| {
        assert!(homes.target.is_none());
        let id = SymId(identity_census_for_test().1.try_into().unwrap());
        let heap = run.owner.registry.heap;
        let name = homes.name.word_for_test(run);
        let equal = homes.equal_name.word_for_test(run);
        let spelling = String::from_utf8(Value::from_bits(name).as_lisp_string().unwrap().as_bytes().to_vec()).unwrap();
        let registry_before = fresh_registry_facts_for_test(heap, id, name, &spelling);
        let before = run.initial_facts_for_test(id).unwrap();
        let old = run.initial_facts_for_test(homes.old_id).unwrap();
        let charge = run.owner.local.as_ref().unwrap().hot.bytes_since_gc;
        let output = homes.dst.word_for_test(run);
        run.initial_fault_for_test(InitialInternFault::ReservationAfterSpineMove).unwrap();
        assert_eq!(intern_retry(run, &homes.name, &mut homes.dst),
            Err(InitialInternError::Authority(RegistrationError::Capacity)));
        let after = run.initial_facts_for_test(id).unwrap();
        let registry_after = fresh_registry_facts_for_test(heap, id, name, &spelling);
        unpublished(registry_before, registry_after);
        assert_eq!(name_publication_for_test(heap, id, name), (None, false, false, false));
        assert_eq!(name_publication_for_test(heap, id, equal), (None, false, false, false));
        assert_eq!(homes.dst.word_for_test(run), output);
        assert_ne!(before.spine_address, after.spine_address);
        assert!(after.spine_current && after.bytes > before.bytes);
        assert_eq!((after.members, after.epoch, after.slots, after.chunks, after.sides,
            after.present, after.member, after.projections),
            (before.members, before.epoch, before.slots, before.chunks, before.sides,
            before.present, before.member, before.projections));
        assert!(!after.present && !after.member);
        assert_eq!(run.owner.local.as_ref().unwrap().hot.bytes_since_gc - charge,
            after.bytes - before.bytes + registry_after.bytes - registry_before.bytes,
            "charge retained World AND registry capacity, excluding abandoned staging");
        let old_after = run.initial_facts_for_test(homes.old_id).unwrap();
        assert_eq!((old_after.cells, old_after.backing, old_after.member),
            (old.cells, old.backing, old.member));
        run.sym_value_into(&homes.old_handle, &mut homes.dst).unwrap();
        assert_eq!(homes.dst.word_for_test(run), name);
        assert_eq!(run.initial_poison_receipt_for_test(),
            (false, true, InitialInternFault::ReservationAfterSpineMove as u8));
        // Refusal is clean: the next attempt reuses the same proposed ID and
        // publishes exactly once, rather than leaving a gap or stale atom.
        intern_retry(run, &homes.name, &mut homes.dst).unwrap();
        assert_eq!(Value::from_bits(homes.dst.word_for_test(run)).as_bare_symbol_id(), Some(id));
        let committed = fresh_registry_facts_for_test(heap, id, name, &spelling);
        assert_eq!((committed.epoch, committed.symbols),
            (registry_before.epoch + 1, registry_before.symbols + 1));
        assert!(committed.id_present && committed.input_rooted && committed.atom_present);
        assert_eq!(committed.canonical_id, Some(id));
        assert_eq!(committed.atoms - registry_before.atoms,
            usize::from(!matches!(query, Query::FreshReusedAtom)));
        let world = run.initial_facts_for_test(id).unwrap();
        assert_eq!((world.members, world.epoch), (before.members + 1, before.epoch + 1));
        if matches!(query, Query::FreshBoundary) {
            assert!(world.chunks >= before.chunks + 2);
            assert!(!run.initial_facts_for_test(SymId(id.0 - 1)).unwrap().present);
        }
        intern_retry(run, &homes.equal_name, &mut homes.dst).unwrap();
        assert_eq!(Value::from_bits(homes.dst.word_for_test(run)).as_bare_symbol_id(), Some(id));
        assert_eq!(fresh_registry_facts_for_test(heap, id, name, &spelling), committed);
        assert_eq!(name_publication_for_test(heap, id, equal), (Some(name), true, false, true));
        eprintln!("P3SYM_C5_FRESH_REFUSAL {kind:?} {query:?} unpublished=1 retry_winners=1 world_bytes={} registry_bytes={}",
            after.bytes - before.bytes, registry_after.bytes - registry_before.bytes);
    }).expect("fresh reservation refusal permits ordinary root teardown and handoff");
}

#[test]
fn p3sym_c5_fresh_reservation_refusal_preserves_registry_across_chunk_gaps() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        refusal(kind, Query::FreshBoundary);
    }
}

#[test]
fn p3sym_c5_small_fresh_reservation_refusal_hands_back_old_cells() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        for query in [Query::Fresh, Query::FreshReusedAtom] {
            refusal(kind, query);
        }
    }
}

#[test]
fn p3sym_c5_fresh_partial_world_publication_closes_era_without_registry_commit() {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        let spelling = format!(
            "p3sym-c5-fresh-partial-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        assert!(crate::emacs_core::intern::lookup_interned(&spelling).is_none());
        Context::with_test_symbol_name_inputs_era(kind, &spelling, |era, ids| {
            let old = era.admit_plain_symbol(ids[1]).unwrap();
            let mut stopped = era.stop().unwrap();
            let inputs = era
                .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                    // SAFETY: immediate seeds on this actual stopped heap only.
                    [
                        SharedRoot::new(heap, Value::NIL),
                        SharedRoot::new(heap, Value::fixnum(23)),
                    ]
                })
                .unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let mut owner = era
                .register(1, 4)
                .unwrap()
                .activate(era.heap_identity(), 1)
                .unwrap();
            let (outcome, clear_refused) = {
                let mut run = owner.run_access().unwrap();
                let mut name = run.root_slot(&inputs[0]).unwrap();
                let mut dst = run.root_slot(&inputs[1]).unwrap();
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run.sym_value_into(&old, &mut name).unwrap();
                    let word = name.word_for_test(&run);
                    let heap = run.owner.registry.heap;
                    let id = SymId(identity_census_for_test().1.try_into().unwrap());
                    let before = fresh_registry_facts_for_test(heap, id, word, &spelling);
                    run.initial_fault_for_test(InitialInternFault::AfterSlotPublication)
                        .unwrap();
                    assert_eq!(
                        intern_retry(&mut run, &name, &mut dst),
                        Err(InitialInternError::Authority(RegistrationError::Poisoned))
                    );
                    unpublished(
                        before,
                        fresh_registry_facts_for_test(heap, id, word, &spelling),
                    );
                    assert_eq!(
                        name_publication_for_test(heap, id, word),
                        (None, false, false, false)
                    );
                    assert_eq!(dst.word_for_test(&run), Value::fixnum(23).bits());
                    assert_eq!(
                        run.initial_poison_receipt_for_test(),
                        (
                            true,
                            true,
                            128 | InitialInternFault::AfterSlotPublication as u8
                        ),
                        "partial World publication records poison under WRITER, then releases it"
                    );
                    assert_eq!(
                        run.initial_intern_soft_into(&name, &mut dst),
                        Err(InitialInternError::Authority(RegistrationError::Poisoned))
                    );
                    assert_eq!(
                        run.initial_intern_into(&name, &mut dst),
                        Err(InitialInternError::Authority(RegistrationError::Poisoned))
                    );
                    assert!(matches!(
                        run.initial_symbol_handle(&dst),
                        Err(InitialInternError::Authority(RegistrationError::Poisoned))
                    ));
                    assert!(
                        matches!(
                            run.initial_facts_for_test(id),
                            Err(RegistrationError::Poisoned)
                        ),
                        "even assertion World views must refuse the uncommitted NameId"
                    );
                    assert_eq!(
                        run.sym_value_into(&old, &mut dst),
                        Err(GlobalError::Authority(RegistrationError::Poisoned))
                    );
                }));
                // Cleanup runs even if a semantic assertion failed. Private homes
                // may be invalidated; this grants no access to the partial World.
                drop(name);
                drop(dst);
                let cleared = run.clear_root_slots();
                let clear_refused = cleared == Err(RegistrationError::Poisoned);
                match cleared {
                    Err(RegistrationError::Poisoned) => {
                        run.clear_terminal_roots_for_test().unwrap()
                    }
                    other => other.expect("fresh failure cleanup before terminal poison"),
                }
                (outcome, clear_refused)
            };
            drop(inputs);
            let run_refused = matches!(owner.run_access(), Err(RegistrationError::Poisoned));
            let stop_refused = matches!(era.stop(), Err(RendezvousError::Poisoned));
            drop(owner);
            let finish_reason = match era.finish() {
                Err(failed) => {
                    let reason = failed.reason;
                    drop(failed);
                    Some(reason)
                }
                Ok((collector, keeper)) => {
                    drop(collector);
                    drop(keeper);
                    None
                }
            };
            // The intentionally inconsistent World remains retained, never traced
            // or resumed; only copied registry facts were inspected after failure.
            eprintln!("P3SYM_C5_FRESH_POISON_RETAINED {kind:?}");
            if let Err(panic) = outcome {
                std::panic::resume_unwind(panic);
            }
            assert!(clear_refused && run_refused && stop_refused);
            assert_eq!(finish_reason, Some(RegistrationError::Poisoned));
        });
    }
}
