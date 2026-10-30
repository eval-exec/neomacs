//! Shared real-World test fixture: exact rooted operands and total teardown. No feature-dependent test hiding.
use super::super::symbol_functions::retry::{self as contention, Deadline, Retry, retry_owned};
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::{Mutator, PlainSymbolBacking, RendezvousError};
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::cell::RefCell;
use std::ops::ControlFlow;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Homes<'era> {
    pub(super) name: OwnerRootSlot<'era>,
    pub(super) equal_name: OwnerRootSlot<'era>,
    pub(super) dst: OwnerRootSlot<'era>,
    pub(super) value: OwnerRootSlot<'era>,
    pub(super) target: Option<crate::emacs_core::intern::SymId>,
    pub(super) old_id: crate::emacs_core::intern::SymId,
    pub(super) old_handle: crate::tagged::gc::SymbolHandle,
    pub(super) equal_holder: crate::tagged::gc::SymbolHandle,
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
            retry_owned(
                Deadline::contention(),
                owner,
                |owner| owner.retire_rooted(&[]),
                |failed| match contention::rendezvous(&failed.reason) {
                    Retry::Busy => ControlFlow::Continue(failed.into_owner()),
                    Retry::Refuse => ControlFlow::Break(failed),
                },
            )
            .map_err(|failed| format!("C5 cleanup retirement: {failed:?}"))
        })();
        *self.cleanup.borrow_mut() = Some(result);
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Query {
    Fresh,
    FreshBoundary,
    TableGrowth,
    FreshReusedAtom,
    Existing,
    RegistryOnly,
    SameChunk,
    Boundary,
    Nonmember,
    Nil,
    T,
    Keyword,
}
pub(super) fn fixture(
    kind: PlainSymbolBacking,
    query: Query,
    body: impl for<'owner, 'era> FnOnce(&mut RunAccess<'owner, 'era>, &mut Homes<'era>),
) -> std::thread::Result<()> {
    fixture_owner(kind, query, |_, owner, homes| {
        let mut run = owner.run_access().unwrap();
        body(&mut run, homes);
    })
}

pub(super) fn fixture_owner(
    kind: PlainSymbolBacking,
    query: Query,
    body: impl for<'heap, 'era> FnOnce(
        &crate::tagged::gc::ProofEra<'heap>,
        &mut Mutator<'era>,
        &mut Homes<'era>,
    ),
) -> std::thread::Result<()> {
    with_query_era(kind, query, |era, ids, target| {
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
        let era_ref = &era;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let mut guard = FixtureGuard {
                owner: Some(owner),
                inputs: Some(inputs),
                homes: None,
                cleanup: receipt,
            };
            {
                let mut run = guard.owner.as_mut().unwrap().run_access().unwrap();
                let inputs = guard.inputs.as_ref().unwrap();
                guard.homes = Some(Homes {
                    name: run.root_slot(&inputs[0]).unwrap(),
                    equal_name: run.root_slot(&inputs[1]).unwrap(),
                    dst: run.root_slot(&inputs[2]).unwrap(),
                    value: run.root_slot(&inputs[2]).unwrap(),
                    target,
                    old_id: ids[1],
                    old_handle: handles[0],
                    equal_holder: handles[1],
                });
                let homes = guard.homes.as_mut().unwrap();
                run.sym_value_into(&handles[0], &mut homes.name).unwrap();
                run.sym_value_into(&handles[1], &mut homes.equal_name)
                    .unwrap();
                assert_ne!(
                    homes.name.word_for_test(&run),
                    homes.equal_name.word_for_test(&run)
                );
            }
            body(
                era_ref,
                guard.owner.as_mut().unwrap(),
                guard.homes.as_mut().unwrap(),
            );
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

pub(super) fn with_query_era<R>(
    kind: PlainSymbolBacking,
    query: Query,
    body: impl for<'heap> FnOnce(
        crate::tagged::gc::ProofEra<'heap>,
        Vec<crate::emacs_core::intern::SymId>,
        Option<crate::emacs_core::intern::SymId>,
    ) -> R,
) -> R {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    // Existing input holders stay below a deliberately distant target ID.
    // Seeding them after the target would pre-grow the World and invalidate
    // the actual chunk-growth witness.
    for spelling in ["p3sym-c2-global", "p3sym-c3-global-1", "p3sym-c3-global-2"] {
        crate::emacs_core::intern::intern(spelling);
    }
    let spelling = match query {
        Query::Fresh | Query::FreshBoundary | Query::TableGrowth | Query::FreshReusedAtom => {
            let spelling = format!("p3sym-c5-new-{}", NEXT.fetch_add(1, Ordering::Relaxed));
            if matches!(query, Query::FreshBoundary | Query::TableGrowth) {
                let mut id = crate::emacs_core::intern::intern_uninterned("p3sym-c5-gap");
                let added_chunks = if matches!(query, Query::TableGrowth) {
                    1
                } else {
                    2
                };
                let target = (u64::from(id.0) / 4096 + added_chunks) * 4096 + 17;
                while u64::from(id.0) + 1 < target {
                    id = crate::emacs_core::intern::intern_uninterned("p3sym-c5-gap");
                }
            } else if matches!(query, Query::FreshReusedAtom) {
                crate::emacs_core::intern::intern_uninterned(&spelling);
            }
            spelling
        }
        Query::Existing | Query::Nonmember => "p3sym-c2-global".to_owned(),
        Query::RegistryOnly | Query::SameChunk => {
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
    if matches!(
        query,
        Query::Fresh | Query::FreshBoundary | Query::TableGrowth | Query::FreshReusedAtom
    ) {
        assert!(crate::emacs_core::intern::lookup_interned(&spelling).is_none());
    }
    let target = crate::emacs_core::intern::lookup_interned(&spelling);
    Context::with_test_symbol_name_inputs_init_era(
        kind,
        &spelling,
        |obarray, ids| {
            if matches!(query, Query::SameChunk) {
                let id = target.expect("serially prepared canonical identity");
                let slots = (id.0 as usize).checked_add(1).unwrap();
                obarray.prepare_symbol_table_slots_for_test(
                    crate::tagged::gc::symbol_table::SymbolSlotCount::from(slots),
                );
            }
            if matches!(query, Query::Existing | Query::Nonmember) {
                obarray
                    .prepare_initial_nonmember_for_test(ids[0], matches!(query, Query::Nonmember));
            }
        },
        |era, ids| body(era, ids, target),
    )
}

pub(super) fn intern_retry<'era>(
    run: &mut RunAccess<'_, 'era>,
    name: &OwnerRootSlot<'era>,
    dst: &mut OwnerRootSlot<'era>,
) -> Result<(), crate::tagged::gc::InitialInternError> {
    contention::retry(
        Deadline::contention(),
        || run.initial_intern_into(name, dst),
        contention::initial_intern,
    )
}

pub(super) fn soft_retry<'era>(
    run: &mut RunAccess<'_, 'era>,
    name: &OwnerRootSlot<'era>,
    dst: &mut OwnerRootSlot<'era>,
) -> Result<(), crate::tagged::gc::InitialInternError> {
    // Refused acquisition is legal. Every Busy releases its loan before the
    // shared elapsed-time retry yields; a refusal stays a typed error.
    contention::retry(
        Deadline::contention(),
        || run.initial_intern_soft_into(name, dst),
        contention::initial_intern,
    )
}
