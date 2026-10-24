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
}
fn fixture(
    kind: PlainSymbolBacking,
    query: Query,
    body: impl for<'owner, 'era> FnOnce(&mut RunAccess<'owner, 'era>, &mut Homes<'era>),
) -> std::thread::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let spelling = match query {
        Query::Fresh => format!("p3sym-c5-new-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
        Query::Existing => "p3sym-c2-global".to_owned(),
    };
    if matches!(query, Query::Fresh) {
        assert!(crate::emacs_core::intern::lookup_interned(&spelling).is_none());
    }
    Context::with_test_symbol_name_inputs_era(kind, &spelling, |era, ids| {
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
    })
}
fn missing(kind: PlainSymbolBacking, query: Query, op: InternOpKind) {
    fixture(kind, query, |run, homes| {
        let before = homes.dst.word_for_test(run);
        let result = match op {
            InternOpKind::Intern => run.initial_intern_into(&homes.name, &mut homes.dst),
            InternOpKind::Soft => run.initial_intern_soft_into(&homes.name, &mut homes.dst),
        };
        if let Err(error) = result {
            assert_eq!(
                error,
                InternError::MissingOperation(op),
                "unexpected fixture/op error"
            );
            assert_eq!(
                homes.dst.word_for_test(run),
                before,
                "missing op changed output"
            );
        }
        // Named missing-op assertion runs before RAII teardown; the outer
        // fixture reports successful teardown even when this expected red fires.
        assert!(
            result.is_ok(),
            "C5 missing executor op {}: {result:?}",
            op.method()
        );
    })
    .expect("C5 missing-op red after independently successful fixture teardown");
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
#[ignore = "C5 preparation: missing initial_intern_into"]
fn p3sym_c5_red_plain_existing_intern() {
    missing(
        PlainSymbolBacking::Unprojected,
        Query::Existing,
        InternOpKind::Intern,
    );
}
#[test]
#[ignore = "C5 preparation: missing initial_intern_soft_into"]
fn p3sym_c5_red_plain_soft_miss() {
    missing(
        PlainSymbolBacking::Unprojected,
        Query::Fresh,
        InternOpKind::Soft,
    );
}
#[test]
#[ignore = "C5 preparation: missing initial_intern_soft_into"]
fn p3sym_c5_red_plain_soft_hit() {
    missing(
        PlainSymbolBacking::Unprojected,
        Query::Existing,
        InternOpKind::Soft,
    );
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
#[ignore = "C5 preparation: missing initial_intern_into"]
fn p3sym_c5_red_projected_existing_intern() {
    missing(
        PlainSymbolBacking::VoidProjection,
        Query::Existing,
        InternOpKind::Intern,
    );
}
#[test]
#[ignore = "C5 preparation: missing initial_intern_soft_into"]
fn p3sym_c5_red_projected_soft_miss() {
    missing(
        PlainSymbolBacking::VoidProjection,
        Query::Fresh,
        InternOpKind::Soft,
    );
}
#[test]
#[ignore = "C5 preparation: missing initial_intern_soft_into"]
fn p3sym_c5_red_projected_soft_hit() {
    missing(
        PlainSymbolBacking::VoidProjection,
        Query::Existing,
        InternOpKind::Soft,
    );
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
    assert!(source.contains("&OwnerRootSlot<'era>"));
    assert!(source.contains("&mut OwnerRootSlot<'era>"));
    assert_eq!(
        include_str!("../symbol_intern_oracle.expected")
            .lines()
            .count(),
        24
    );
}
