//! Real small World, exact rooted string operands and independent teardown.
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::{Mutator, PlainSymbolBacking, RendezvousError};
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

struct Homes<'era> {
    name: OwnerRootSlot<'era>,
    equal_name: OwnerRootSlot<'era>,
    dst: OwnerRootSlot<'era>,
    value: OwnerRootSlot<'era>,
    target: Option<crate::emacs_core::intern::SymId>,
}
struct FixtureGuard<'era> {
    owner: Option<Mutator<'era>>,
    inputs: Option<[SharedRoot; 3]>,
    homes: Option<Homes<'era>>,
    cleanup: Rc<RefCell<Option<Result<(), String>>>>,
}
impl Drop for FixtureGuard<'_> {
    fn drop(&mut self) {
        let result = (|| {
            drop(self.homes.take());
            let mut owner = self.owner.take().ok_or("C5 owner absent")?;
            owner
                .run_access()
                .map_err(|e| format!("C5 cleanup run: {e:?}"))?
                .clear_root_slots()
                .map_err(|e| format!("C5 cleanup clear: {e:?}"))?;
            drop(self.inputs.take());
            for _ in 0..10_000 {
                match owner.retire_rooted(&[]) {
                    Ok(()) => return Ok(()),
                    Err(failed) if failed.reason == RendezvousError::Busy => {
                        owner = failed.into_owner();
                        std::thread::yield_now();
                    }
                    Err(failed) => return Err(format!("C5 cleanup retirement: {failed:?}")),
                }
            }
            Err("C5 cleanup retirement remained Busy".into())
        })();
        *self.cleanup.borrow_mut() = Some(result);
    }
}
#[derive(Clone, Copy, Debug)]
enum Query {
    Fresh,
    Existing,
    RegistryOnly,
    Boundary,
    Nonmember,
    Nil,
    T,
    Keyword,
}
fn fixture(
    kind: PlainSymbolBacking,
    query: Query,
    body: impl for<'owner, 'era> FnOnce(&mut RunAccess<'owner, 'era>, &mut Homes<'era>),
) -> std::thread::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    // Existing input holders stay below a deliberately distant target ID.
    // Seeding them after the target would pre-grow the World and invalidate
    // the actual chunk-growth witness.
    for spelling in ["p3sym-c2-global", "p3sym-c3-global-1", "p3sym-c3-global-2"] {
        crate::emacs_core::intern::intern(spelling);
    }
    let spelling = match query {
        Query::Fresh => format!("p3sym-c5-new-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
        Query::Existing | Query::Nonmember => "p3sym-c2-global".to_owned(),
        Query::RegistryOnly => {
            let spelling = format!(
                "p3sym-c5-registry-only-{}",
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            crate::emacs_core::intern::intern(&spelling);
            spelling
        }
        Query::Boundary => {
            let spelling = format!("p3sym-c5-boundary-{}", NEXT.fetch_add(1, Ordering::Relaxed));
            let mut id = crate::emacs_core::intern::intern_uninterned("p3sym-c5-gap");
            let target = (u64::from(id.0) / 4096 + 2) * 4096 + 17;
            while u64::from(id.0) + 1 < target {
                id = crate::emacs_core::intern::intern_uninterned("p3sym-c5-gap");
            }
            crate::emacs_core::intern::intern(&spelling);
            spelling
        }
        Query::Nil => "nil".to_owned(),
        Query::T => "t".to_owned(),
        Query::Keyword => ":p3sym-c5-excluded".to_owned(),
    };
    if matches!(query, Query::Fresh) {
        assert!(crate::emacs_core::intern::lookup_interned(&spelling).is_none());
    }
    let target = crate::emacs_core::intern::lookup_interned(&spelling);
    Context::with_test_symbol_name_inputs_init_era(
        kind,
        &spelling,
        |obarray, ids| {
            if matches!(query, Query::Existing | Query::Nonmember) {
                obarray
                    .prepare_initial_nonmember_for_test(ids[0], matches!(query, Query::Nonmember));
            }
        },
        |era, ids| {
            let handles = [
                era.admit_plain_symbol(ids[1]).unwrap(),
                era.admit_plain_symbol(ids[2]).unwrap(),
            ];
            let mut stopped = era.stop().unwrap();
            let inputs = era
                .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                    // SAFETY: immediate seed words on the actual matching heap.
                    // Real string operands stay in serially initialized cells.
                    [
                        SharedRoot::new(heap, Value::NIL),
                        SharedRoot::new(heap, Value::NIL),
                        SharedRoot::new(heap, Value::fixnum(23)),
                    ]
                })
                .unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let owner = era
                .register(1, 8)
                .unwrap()
                .activate(era.heap_identity(), 1)
                .unwrap();
            let cleanup = Rc::new(RefCell::new(None));
            let receipt = Rc::clone(&cleanup);
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let mut guard = FixtureGuard {
                    owner: Some(owner),
                    inputs: Some(inputs),
                    homes: None,
                    cleanup: receipt,
                };
                let mut run = guard.owner.as_mut().unwrap().run_access().unwrap();
                let inputs = guard.inputs.as_ref().unwrap();
                guard.homes = Some(Homes {
                    name: run.root_slot(&inputs[0]).unwrap(),
                    equal_name: run.root_slot(&inputs[1]).unwrap(),
                    dst: run.root_slot(&inputs[2]).unwrap(),
                    value: run.root_slot(&inputs[2]).unwrap(),
                    target,
                });
                let homes = guard.homes.as_mut().unwrap();
                run.sym_value_into(&handles[0], &mut homes.name).unwrap();
                run.sym_value_into(&handles[1], &mut homes.equal_name)
                    .unwrap();
                assert_ne!(
                    homes.name.word_for_test(&run),
                    homes.equal_name.word_for_test(&run)
                );
                body(&mut run, homes);
            }));
            assert_eq!(
                cleanup.borrow().as_ref(),
                Some(&Ok(())),
                "C5 fixture teardown failed independently of operation outcome"
            );
            let (collector, keeper) = era.finish().expect("C5 era finish after clean teardown");
            drop(collector);
            drop(keeper);
            eprintln!("P3SYM_C5_TEARDOWN_OK {kind:?} {query:?}");
            outcome
        },
    )
}
fn missing(kind: PlainSymbolBacking, query: Query, op: InternOpKind) {
    use crate::tagged::gc::InitialInternError;
    fixture(kind, query, |run, homes| {
        let before = homes.dst.word_for_test(run);
        let result = intern_retry(run, &homes.name, &mut homes.dst);
        assert_eq!(
            result,
            Err(InitialInternError::NewRegistryName),
            "unexpected fixture/op error"
        );
        assert_eq!(
            homes.dst.word_for_test(run),
            before,
            "unimplemented new-ID path changed output"
        );
        assert!(
            result.is_ok(),
            "C5 missing executor op {} (new registry ID): {result:?}",
            op.method()
        );
    })
    .expect("C5 missing-op red after independently successful fixture teardown");
}
fn intern_retry<'era>(
    run: &mut RunAccess<'_, 'era>,
    name: &OwnerRootSlot<'era>,
    dst: &mut OwnerRootSlot<'era>,
) -> Result<(), crate::tagged::gc::InitialInternError> {
    use crate::tagged::gc::{InitialInternError, RegistrationError};
    for _ in 0..10_000 {
        match run.initial_intern_into(name, dst) {
            Err(InitialInternError::Authority(RegistrationError::Busy)) => std::thread::yield_now(),
            result => return result,
        }
    }
    Err(InitialInternError::Authority(RegistrationError::Busy))
}
fn existing_intern(kind: PlainSymbolBacking, query: Query) {
    fixture(kind, query, |run, homes| {
        let id = homes
            .target
            .expect("existing canonical ID prepared serially");
        let before = run.initial_facts_for_test(id).unwrap();
        let charge = run.owner.local.as_ref().unwrap().hot.bytes_since_gc;
        intern_retry(run, &homes.name, &mut homes.dst).unwrap();
        assert_eq!(homes.dst.word_for_test(run), Value::from_sym_id(id).bits());
        let after = run.initial_facts_for_test(id).unwrap();
        assert!(after.present && after.member);
        assert_eq!(after.members, before.members + usize::from(!before.member));
        assert_eq!(after.epoch, before.epoch + u64::from(!before.member));
        assert_eq!(after.chunks, after.sides);
        assert!(after.spine_current);
        assert_eq!(
            run.owner.local.as_ref().unwrap().hot.bytes_since_gc - charge,
            after.bytes - before.bytes
        );
        if before.present {
            assert_eq!(
                after.cells, before.cells,
                "intern must preserve function/plist and nonmembership flags"
            );
        }
        if matches!(query, Query::Boundary) {
            assert!(
                after.slots > before.slots && after.chunks >= before.chunks + 2,
                "actual chunk/side growth required"
            );
            assert!(
                !run.initial_facts_for_test(crate::emacs_core::intern::SymId(id.0 - 1))
                    .unwrap()
                    .present,
                "registry-ID gap remains absent in World"
            );
        }
        intern_retry(run, &homes.equal_name, &mut homes.dst).unwrap();
        let repeated = run.initial_facts_for_test(id).unwrap();
        assert_eq!(
            (repeated.members, repeated.epoch, repeated.bytes),
            (after.members, after.epoch, after.bytes)
        );
        assert_eq!(homes.dst.word_for_test(run), Value::from_sym_id(id).bits());
        let handle = run.initial_symbol_handle(&homes.dst).unwrap();
        run.sym_function_into(&handle, &mut homes.dst).unwrap();
        assert_eq!(
            homes.dst.word_for_test(run),
            if before.present {
                Value::fixnum(31).bits()
            } else {
                Value::NIL.bits()
            }
        );
        if !before.present {
            run.sym_boundp_into(&handle, &mut homes.dst).unwrap();
            assert_eq!(homes.dst.word_for_test(run), Value::NIL.bits());
            assert_eq!(
                after.backing,
                Some(if kind == PlainSymbolBacking::VoidProjection {
                    PlainSymbolBacking::VoidProjection
                } else {
                    PlainSymbolBacking::Unprojected
                })
            );
            run.sym_set_into(&handle, &homes.value, &mut homes.dst)
                .unwrap();
            run.sym_value_into(&handle, &mut homes.dst).unwrap();
            assert_eq!(homes.dst.word_for_test(run), Value::fixnum(23).bits());
            let set = run.initial_facts_for_test(id).unwrap();
            assert_eq!(
                set.projections,
                after.projections + usize::from(kind == PlainSymbolBacking::VoidProjection)
            );
            run.sym_set_into(&handle, &homes.value, &mut homes.dst)
                .unwrap();
            assert_eq!(
                run.initial_facts_for_test(id).unwrap().projections,
                set.projections,
                "one descriptor per symbol, no repeated issue"
            );
        } else if kind != PlainSymbolBacking::VoidProjection {
            run.sym_value_into(&handle, &mut homes.dst).unwrap();
            assert_eq!(homes.dst.word_for_test(run), Value::fixnum(73).bits());
        }
    })
    .expect("C5 existing intern completed with clean teardown");
}
fn soft_retry<'era>(
    run: &mut RunAccess<'_, 'era>,
    name: &OwnerRootSlot<'era>,
    dst: &mut OwnerRootSlot<'era>,
) -> Result<(), crate::tagged::gc::InitialInternError> {
    use crate::tagged::gc::{InitialInternError, RegistrationError};
    // Refused acquisition is legal. Only the completed result is unwrapped
    // by the single-owner behavioral fixtures; every Busy releases its loan.
    for _ in 0..10_000 {
        match run.initial_intern_soft_into(name, dst) {
            Err(InitialInternError::Authority(RegistrationError::Busy)) => std::thread::yield_now(),
            result => return result,
        }
    }
    Err(InitialInternError::Authority(RegistrationError::Busy))
}
fn soft(kind: PlainSymbolBacking, query: Query) {
    let expected = match query {
        Query::Existing => Value::from_sym_id(crate::emacs_core::intern::intern("p3sym-c2-global")),
        Query::T => Value::T,
        _ => Value::NIL,
    };
    fixture(kind, query, |run, homes| {
        let name = homes.name.word_for_test(run);
        let equal_name = homes.equal_name.word_for_test(run);
        soft_retry(run, &homes.name, &mut homes.dst).expect("C5 real soft lookup");
        assert_eq!(homes.dst.word_for_test(run), expected.bits());
        assert_eq!(homes.name.word_for_test(run), name);
        soft_retry(run, &homes.equal_name, &mut homes.dst).expect("C5 equal-spelling soft lookup");
        assert_eq!(homes.dst.word_for_test(run), expected.bits());
        assert_eq!(homes.equal_name.word_for_test(run), equal_name);
        let mut alias = homes.name.clone();
        soft_retry(run, &homes.name, &mut alias).expect("C5 aliasing source and output");
        assert_eq!(homes.name.word_for_test(run), expected.bits());
    })
    .expect("C5 soft completed with clean teardown");
}
#[test]
fn p3sym_c5_noop_fixture_releases_every_root_and_finishes() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        for query in [Query::Fresh, Query::Existing] {
            fixture(kind, query, |_, _| {}).expect("C5 no-op fixture completion");
        }
    }
}
#[test]
#[ignore = "C5 preparation: missing initial_intern_into"]
fn p3sym_c5_red_plain_new_intern() {
    missing(
        PlainSymbolBacking::Unprojected,
        Query::Fresh,
        InternOpKind::Intern,
    );
}
#[test]
fn p3sym_c5_plain_existing_intern() {
    existing_intern(PlainSymbolBacking::Unprojected, Query::Existing);
}
#[test]
fn p3sym_c5_plain_soft_miss() {
    soft(PlainSymbolBacking::Unprojected, Query::Fresh);
}
#[test]
fn p3sym_c5_plain_soft_hit() {
    soft(PlainSymbolBacking::Unprojected, Query::Existing);
}
#[test]
#[ignore = "C5 preparation: missing initial_intern_into"]
fn p3sym_c5_red_projected_new_intern() {
    missing(
        PlainSymbolBacking::VoidProjection,
        Query::Fresh,
        InternOpKind::Intern,
    );
}
#[test]
fn p3sym_c5_projected_existing_intern() {
    existing_intern(PlainSymbolBacking::VoidProjection, Query::Existing);
}
#[test]
fn p3sym_c5_projected_soft_miss() {
    soft(PlainSymbolBacking::VoidProjection, Query::Fresh);
}
#[test]
fn p3sym_c5_projected_soft_hit() {
    soft(PlainSymbolBacking::VoidProjection, Query::Existing);
}
#[test]
fn p3sym_c5_soft_registry_hit_world_miss_creates_no_membership() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        soft(kind, Query::RegistryOnly);
    }
}
#[test]
fn p3sym_c5_soft_fundamental_symbols_keep_their_lisp_values() {
    soft(PlainSymbolBacking::Unprojected, Query::Nil);
    soft(PlainSymbolBacking::Unprojected, Query::T);
}
#[test]
fn p3sym_c5_soft_invalid_name_leaves_output_unchanged() {
    use crate::tagged::gc::InitialInternError;
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::Fresh,
        |run, homes| {
            let input = homes.dst.clone();
            let before = homes.dst.word_for_test(run);
            assert_eq!(
                run.initial_intern_soft_into(&input, &mut homes.dst),
                Err(InitialInternError::ExpectedString)
            );
            assert_eq!(homes.dst.word_for_test(run), before);
            soft_retry(run, &homes.name, &mut homes.dst).unwrap();
            assert_eq!(homes.dst.word_for_test(run), Value::NIL.bits());
        },
    )
    .unwrap();
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::Keyword,
        |run, homes| {
            let before = homes.dst.word_for_test(run);
            assert_eq!(
                run.initial_intern_soft_into(&homes.name, &mut homes.dst),
                Err(InitialInternError::UnsupportedName)
            );
            assert_eq!(homes.dst.word_for_test(run), before);
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c5_soft_busy_is_a_refusal_and_releases_the_reader() {
    use crate::tagged::gc::{InitialInternError, RegistrationError};
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::Existing,
        |run, homes| {
            let before = homes.dst.word_for_test(run);
            crate::emacs_core::intern::parallel::with_registry_writer_for_test(|| {
                assert_eq!(
                    run.initial_intern_soft_into(&homes.name, &mut homes.dst),
                    Err(InitialInternError::Authority(RegistrationError::Busy))
                );
                assert_eq!(
                    homes.dst.word_for_test(run),
                    before,
                    "Busy is not a NIL miss"
                );
            });
            soft_retry(run, &homes.name, &mut homes.dst).unwrap();
            assert!(!Value::from_bits(homes.dst.word_for_test(run)).is_nil());
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c5_soft_stale_homes_refuse_before_decoding() {
    use crate::tagged::gc::{InitialInternError, RegistrationError};
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::Fresh,
        |run, homes| {
            run.clear_root_slots().unwrap();
            assert_eq!(
                run.initial_intern_soft_into(&homes.name, &mut homes.dst),
                Err(InitialInternError::Authority(
                    RegistrationError::StaleRegistration
                ))
            );
        },
    )
    .unwrap();
}
#[test]
fn p3sym_c5_surface_is_rooted_and_has_no_legacy_intern_fallback() {
    let source = include_str!("../symbol_intern.rs");
    for forbidden in [
        "-> Value",
        "-> TaggedValue",
        "-> usize",
        "unsafe {",
        ".unwrap(",
        ".expect(",
        "intern_lisp_value(",
        "current_tagged_heap",
    ] {
        assert!(
            !source.contains(forbidden),
            "C5 preparation surface exposed {forbidden}"
        );
    }

    let sealed = include_str!("../symbol_authority/intern.rs");
    assert!(sealed.contains("&OwnerRootSlot<'era>"));
    assert!(sealed.contains("&mut OwnerRootSlot<'era>"));
    for forbidden in [
        ".unwrap(",
        ".expect(",
        "panic!(",
        "current_tagged_heap",
        "intern_lisp_value(",
        "NEOVM_THREADS",
    ] {
        assert!(
            !sealed.contains(forbidden),
            "C5 sealed lookup exposed {forbidden}"
        );
    }
    assert_eq!(
        include_str!("../symbol_intern_oracle.expected")
            .lines()
            .count(),
        24
    );
}

#[test]
fn p3sym_c5_existing_registry_id_creates_world_membership_once() {
    existing_intern(PlainSymbolBacking::Unprojected, Query::RegistryOnly);
    existing_intern(PlainSymbolBacking::VoidProjection, Query::RegistryOnly);
}
#[test]
fn p3sym_c5_existing_id_grows_chunks_and_sides_leaving_gaps_absent() {
    existing_intern(PlainSymbolBacking::Unprojected, Query::Boundary);
    existing_intern(PlainSymbolBacking::VoidProjection, Query::Boundary);
}
#[test]
fn p3sym_c5_existing_nonmember_preserves_payload_and_projection() {
    existing_intern(PlainSymbolBacking::Unprojected, Query::Nonmember);
    existing_intern(PlainSymbolBacking::Projected, Query::Nonmember);
}
#[test]
fn p3sym_c5_intern_busy_leaves_output_and_membership_unchanged() {
    use crate::tagged::gc::{InitialInternError, RegistrationError};
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::RegistryOnly,
        |run, homes| {
            let id = homes.target.unwrap();
            let before = run.initial_facts_for_test(id).unwrap();
            let output = homes.dst.word_for_test(run);
            crate::emacs_core::intern::parallel::with_registry_writer_for_test(|| {
                assert_eq!(
                    run.initial_intern_into(&homes.name, &mut homes.dst),
                    Err(InitialInternError::Authority(RegistrationError::Busy))
                );
            });
            let after = run.initial_facts_for_test(id).unwrap();
            assert_eq!(
                (after.members, after.epoch, after.bytes, after.present),
                (before.members, before.epoch, before.bytes, before.present)
            );
            assert_eq!(homes.dst.word_for_test(run), output);
            intern_retry(run, &homes.name, &mut homes.dst).unwrap();
        },
    )
    .unwrap();
}

#[test]
fn p3sym_c5_existing_intern_can_replace_its_name_home() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        fixture(kind, Query::RegistryOnly, |run, homes| {
            let id = homes.target.unwrap();
            let before = run.initial_facts_for_test(id).unwrap();
            assert!(!before.member);
            let mut alias = homes.name.clone();
            intern_retry(run, &homes.name, &mut alias).unwrap();
            assert_eq!(homes.name.word_for_test(run), Value::from_sym_id(id).bits());
            assert_eq!(
                run.initial_facts_for_test(id).unwrap().members,
                before.members + 1
            );
            run.initial_symbol_handle(&homes.name).unwrap();
        })
        .unwrap();
    }
}

#[test]
#[ignore = "exact process registry epoch/count fixture; run alone"]
fn p3sym_c5_existing_id_preserves_registry_epoch_and_count() {
    use crate::emacs_core::intern::parallel::identity_census_for_test;
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        fixture(kind, Query::Boundary, |run, homes| {
            let before = identity_census_for_test();
            intern_retry(run, &homes.name, &mut homes.dst).unwrap();
            assert_eq!(identity_census_for_test(), before, "World growth neither issues a process ID nor invalidates a positive canonical identity");
            intern_retry(run, &homes.equal_name, &mut homes.dst).unwrap();
            soft_retry(run, &homes.equal_name, &mut homes.dst).unwrap();
            assert_eq!(identity_census_for_test(), before, "existing-name lookups never bump the registry epoch");
        }).unwrap();
    }
}
