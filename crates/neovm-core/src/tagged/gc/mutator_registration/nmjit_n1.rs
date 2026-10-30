//! N1 missing-entry witnesses, not a substitute parallel JIT implementation.
//!
//! The only executed operations are existing symbol authority/root operations.
//! Completed manifest arms exercise their real boundary before entering the
//! lease fixture. Every remaining red reaches its named assertion, releases
//! its real lease and root homes, finishes the era, and only then resumes the
//! original assertion. The retired-publication arm exercises the actual era
//! descriptor boundary. No machine code or grace receipt is minted here.

use super::symbol_functions::retry::{self as contention, Deadline, Retry, retry_owned};
use super::{Mutator, OwnerRootSlot, RegistrationError, RunAccess, SymbolHandle};
use crate::emacs_core::eval::Context;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue;
use std::cell::RefCell;
use std::ops::ControlFlow;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MissingFeature {
    Gen1Admission,
    RetiredPublication,
}

/// The staged boundary manifest. An arm becomes green only by exercising its
/// actual boundary; later-stage refusal witnesses remain plain failing tests.
/// Exhaustiveness makes each new feature require a deliberate disposition.
fn missing_entry(feature: MissingFeature) -> Result<(), MissingFeature> {
    match feature {
        MissingFeature::Gen1Admission => {
            Context::nmjit_n2_production_boundary_for_test().map_err(|reason| {
                eprintln!("NMJIT_N2_PRODUCTION_BOUNDARY_FAILURE {reason}");
                feature
            })
        }
        MissingFeature::RetiredPublication => {
            super::symbol_intern::nmjit_n3_retired_publication_manifest().map_err(|reason| {
                eprintln!("NMJIT_N3_RETIRED_PUBLICATION_FAILURE {reason}");
                feature
            })
        }
    }
}

/// Domain labels keep controls distinct from absent implementation features.
#[derive(Clone, Copy, Debug)]
enum FixtureCase {
    Missing(MissingFeature),
    Noop,
    EarlyReturn,
}

type CleanupReceipt = Rc<RefCell<Option<Result<(), String>>>>;

#[must_use = "root homes and owner must be retired before era.finish"]
struct FixtureGuard<'era> {
    owner: Option<Mutator<'era>>,
    root: Option<SharedRoot>,
    home: Option<OwnerRootSlot<'era>>,
    cleanup: CleanupReceipt,
}

impl Drop for FixtureGuard<'_> {
    fn drop(&mut self) {
        // No assertions while the feature assertion is unwinding. The outer
        // fixture checks this result before attempting era.finish().
        let cleanup = (|| {
            drop(self.home.take());
            let mut owner = self.owner.take().ok_or("N1 fixture owner absent")?;
            {
                let mut run = owner
                    .run_access()
                    .map_err(|reason| format!("N1 cleanup run: {reason:?}"))?;
                run.clear_root_slots()
                    .map_err(|reason| format!("N1 cleanup roots: {reason:?}"))?;
            }
            drop(self.root.take());
            retry_owned(
                Deadline::contention(),
                owner,
                |owner| owner.retire_rooted(&[]),
                |failure| match contention::rendezvous(&failure.reason) {
                    Retry::Busy => ControlFlow::Continue(failure.into_owner()),
                    Retry::Refuse => ControlFlow::Break(failure),
                },
            )
            .map_err(|failure| format!("N1 cleanup retirement: {failure:?}"))
        })();
        *self.cleanup.borrow_mut() = Some(cleanup);
    }
}

static_assertions::assert_not_impl_any!(FixtureGuard<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(MissingFeature: Send, Sync, Copy, Eq);

fn fixture(
    case: FixtureCase,
    body: impl for<'owner, 'era> FnOnce(
        &mut RunAccess<'owner, 'era>,
        &SymbolHandle,
        &OwnerRootSlot<'era>,
    ),
) -> std::thread::Result<()> {
    Context::with_test_symbol_era(|era, id| {
        let handle = era
            .admit_plain_symbol(id)
            .expect("N1 existing Plain symbol");
        let mut stopped = era.stop().expect("N1 root setup stop");
        let root = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: NIL is an immediate word and heap is the actual
                // matching stopped heap; this root cannot refer off-heap.
                SharedRoot::new(heap, TaggedValue::NIL)
            })
            .expect("N1 matching stopped root setup");
        stopped.trace(|_, _| Ok(())).expect("N1 root setup trace");
        stopped.complete().expect("N1 root setup resume");
        let owner = era
            .register(1, 4)
            .expect("N1 single fixture registration")
            .activate(era.heap_identity(), 1)
            .expect("N1 single fixture activation");
        let cleanup = Rc::new(RefCell::new(None));
        let guard_receipt = Rc::clone(&cleanup);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let mut guard = FixtureGuard {
                owner: Some(owner),
                root: Some(root),
                home: None,
                cleanup: guard_receipt,
            };
            // Locals drop in reverse order: the body/reader and RunAccess
            // unwind before FixtureGuard clears homes, releases SharedRoot,
            // and retires the owner. Era finish happens outside this catch.
            let mut run = guard.owner.as_mut().unwrap().run_access().unwrap();
            let mut home = run.root_slot(guard.root.as_ref().unwrap()).unwrap();
            run.sym_value_into(&handle, &mut home).unwrap();
            assert_eq!(home.word_for_test(&run), TaggedValue::fixnum(73).bits());
            guard.home = Some(home);
            body(&mut run, &handle, guard.home.as_ref().unwrap());
        }));
        assert_eq!(
            cleanup.borrow().as_ref(),
            Some(&Ok(())),
            "NMJIT_N1_TEARDOWN_FAILURE {case:?}"
        );
        let (collector, keeper) = era.finish().expect("N1 era finish after clean teardown");
        drop(collector);
        drop(keeper);
        match case {
            FixtureCase::Missing(feature) => eprintln!("NMJIT_N1_TEARDOWN_OK {feature:?}"),
            FixtureCase::Noop => eprintln!("NMJIT_N1_TEARDOWN_OK Noop"),
            FixtureCase::EarlyReturn => eprintln!("NMJIT_N1_TEARDOWN_OK EarlyReturn"),
        }
        outcome
    })
}

fn missing_case(feature: MissingFeature) {
    // Completed entries run before the real lease fixture. Constructing a
    // second Context while that old era/lease lives would test a nested World
    // transition instead of this production admission boundary.
    let entry = missing_entry(feature);
    let result = fixture(FixtureCase::Missing(feature), |run, handle, _home| {
        // This is a real reader lease, intentionally live at the assertion.
        // Unwinding must release it before FixtureGuard retires the owner.
        let _reader = run.symbol_ref(handle).expect("N1 real symbol reader lease");
        assert_eq!(entry, Ok(()), "NMJIT_N1_MISSING_FEATURE {feature:?}");
    });
    if let Err(assertion) = result {
        std::panic::resume_unwind(assertion);
    }
}

macro_rules! missing_test {
    ($name:ident, $feature:ident) => {
        #[test]
        fn $name() {
            missing_case(MissingFeature::$feature);
        }
    };
}

missing_test!(nmjit_n1_missing_gen1_admission, Gen1Admission);





missing_test!(nmjit_n1_missing_retired_publication, RetiredPublication);


#[test]
fn nmjit_n1_noop_fixture_finishes_cleanly() {
    fixture(FixtureCase::Noop, |run, handle, _home| {
        let _reader = run.symbol_ref(handle).expect("N1 no-op reader lease");
    })
    .expect("N1 no-op fixture completion");
}

#[derive(Debug, PartialEq, Eq)]
enum ControlledReturn {
    EntryNotRequested,
}

#[test]
fn nmjit_n1_early_return_fixture_finishes_cleanly() {
    fixture(FixtureCase::EarlyReturn, |run, handle, _home| {
        let mut early_return = || -> Result<(), ControlledReturn> {
            let _reader = run
                .symbol_ref(handle)
                .expect("N1 early-return reader lease");
            Err(ControlledReturn::EntryNotRequested)
        };
        assert_eq!(early_return(), Err(ControlledReturn::EntryNotRequested));
    })
    .expect("N1 early-return fixture completion");
}

// The real fixture refusal uses the shared exhaustive classifier rather than
// a count-capped Busy loop. This type check keeps that boundary explicit.
const _: fn(&RegistrationError) -> Retry = contention::registration;
