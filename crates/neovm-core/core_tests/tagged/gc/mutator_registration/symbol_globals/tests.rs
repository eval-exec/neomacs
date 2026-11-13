//! Real retained-World primitive twins; RAII cleanup runs for every outcome.
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::tagged::gc::PlainSymbolBacking;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;

pub(super) fn oracle() -> Vec<&'static str> {
    let lines: Vec<_> = include_str!("../../../../../../neovm-value/src/tagged/gc/mutator_registration/symbol_globals_oracle.expected")
        .lines()
        .collect();
    assert_eq!(lines.len(), 16, "GNU 31.1 C3 fixture row count");
    lines
}

/// Own every fixture obligation, independently of the operation's result.
/// An inner RunAccess/lease must drop before this guard can regain the owner.
struct FixtureGuard<'era> {
    owner: Option<super::super::Mutator<'era>>,
    roots: Option<(SharedRoot, SharedRoot)>,
    homes: Option<(OwnerRootSlot<'era>, OwnerRootSlot<'era>)>,
    cleanup: std::rc::Rc<std::cell::RefCell<Option<Result<(), String>>>>,
}

impl Drop for FixtureGuard<'_> {
    fn drop(&mut self) {
        // Drop cannot assert while the missing-op assertion is unwinding.
        // Record any teardown error for the outer harness to reject instead.
        let result = (|| {
            drop(self.homes.take());
            let mut owner = self.owner.take().ok_or("fixture owner absent")?;
            {
                let mut run = owner
                    .run_access()
                    .map_err(|e| format!("cleanup run: {e:?}"))?;
                run.clear_root_slots()
                    .map_err(|e| format!("clear census: {e:?}"))?;
            }
            drop(self.roots.take());
            owner
                .retire_rooted(&[])
                .map_err(|e| format!("retire: {e:?}"))?;
            Ok(())
        })();
        *self.cleanup.borrow_mut() = Some(result);
    }
}

pub(super) fn fixture_case(
    kind: PlainSymbolBacking,
    body: impl for<'owner, 'era> FnOnce(
        &mut RunAccess<'owner, 'era>,
        &SymbolHandle,
        &OwnerRootSlot<'era>,
        &mut OwnerRootSlot<'era>,
        SymId,
    ),
) -> std::thread::Result<()> {
    crate::test_utils::ensure_eval_hooks_installed();
    Context::with_test_symbol_backing_era(kind, |era, id| {
        let handle = era.admit_plain_symbol(id).unwrap();
        // Explicit matching stopped heap for serial immediate-root setup;
        // no collection, current-heap TLS access or raw global read.
        let mut stopped = era.stop().unwrap();
        let roots = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: live scalar immediates on the matching stopped heap.
                (
                    SharedRoot::new(heap, Value::fixnum(97)),
                    SharedRoot::new(heap, Value::NIL),
                )
            })
            .unwrap();
        stopped.trace(|_, _| Ok(())).unwrap();
        stopped.complete().unwrap();
        let owner = era
            .register(1, 4)
            .unwrap()
            .activate(era.heap_identity(), 1)
            .unwrap();
        let cleanup = std::rc::Rc::new(std::cell::RefCell::new(None));
        let cleanup_receipt = std::rc::Rc::clone(&cleanup);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let mut fixture = FixtureGuard {
                owner: Some(owner),
                roots: Some(roots),
                homes: None,
                cleanup: cleanup_receipt,
            };
            let mut run = fixture.owner.as_mut().unwrap().run_access().unwrap();
            {
                let reader = run.symbol_ref(&handle).unwrap();
                assert_eq!(reader.backing(), kind);
            }
            let (input, output) = fixture.roots.as_ref().unwrap();
            fixture.homes = Some((
                run.root_slot(input).unwrap(),
                run.root_slot(output).unwrap(),
            ));
            let (src, dst) = fixture.homes.as_mut().unwrap();
            // Assertions in body run BEFORE teardown. On panic, this run and
            // any nested lease unwind before FixtureGuard clears its census.
            body(&mut run, &handle, src, dst, id);
        }));
        assert_eq!(
            cleanup.borrow().as_ref(),
            Some(&Ok(())),
            "C3 fixture teardown failed independently of operation outcome"
        );
        let (collector, keeper) = era
            .finish()
            .expect("C3 fixture era finish after root cleanup");
        drop(collector);
        drop(keeper);
        eprintln!("P3SYM_C3_TEARDOWN_OK {kind:?}");
        outcome
    })
}

fn primitive_case(op: GlobalOp, kind: PlainSymbolBacking) {
    fixture_case(kind, |run, handle, src, dst, id| {
        match op {
            GlobalOp::Value { .. } => run.sym_value_into(handle, dst),
            GlobalOp::Set { .. } => run.sym_set_into(handle, src, dst),
            GlobalOp::Boundp { .. } => run.sym_boundp_into(handle, dst),
            GlobalOp::Makunbound { .. } => run.sym_makunbound_into(handle, dst),
        }
        .expect("sealed global primitive");
        // SAFETY: the word comes from this heap's own census.
        let value = unsafe { Value::from_bits(dst.word_for_test(run)) };
        let (printed, expected) = match op {
            GlobalOp::Value { .. } => (crate::emacs_core::print_value(&value), oracle()[0]),
            GlobalOp::Set { .. } => (crate::emacs_core::print_value(&value), oracle()[1]),
            GlobalOp::Boundp { .. } => (crate::emacs_core::print_value(&value), oracle()[2]),
            GlobalOp::Makunbound { .. } => {
                // The real symbol identity is rooted, not a borrowed cell.
                let same = value.bits() == Value::from_sym_id(id).bits();
                (
                    crate::emacs_core::print_value(&Value::bool_val(same)),
                    oracle()[3],
                )
            }
        };
        assert_eq!(printed, expected);
        if matches!(op, GlobalOp::Set { .. }) {
            run.sym_value_into(handle, dst).unwrap();
            assert_eq!(
                // SAFETY: the word comes from this heap's own census.
                crate::emacs_core::print_value(&unsafe {
                    Value::from_bits(dst.word_for_test(run))
                }),
                oracle()[1]
            );
        }
        if matches!(op, GlobalOp::Makunbound { .. }) {
            run.sym_boundp_into(handle, dst).unwrap();
            assert_eq!(dst.word_for_test(run), Value::NIL.bits());
        }
    })
    .expect("primitive twin and RAII teardown");
}

#[test]
fn p3sym_c3_noop_fixture_releases_roots_and_finishes() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture_case(kind, |_run, _handle, _src, _dst, _id| {})
            .expect("no-op body must finish without a fixture panic");
    }
}

macro_rules! primitive {
    ($(#[$attr:meta])* $name:ident, $kind:ident, $op:expr) => {
        #[test]
        $(#[$attr])*
        fn $name() {
            primitive_case($op, PlainSymbolBacking::$kind);
        }
    };
}

primitive!(
    p3sym_c3_plain_value,
    Unprojected,
    GlobalOp::Value { symbol: 0, dst: 1 }
);
primitive!(
    p3sym_c3_plain_set,
    Unprojected,
    GlobalOp::Set {
        symbol: 0,
        src: 0,
        dst: 1
    }
);
primitive!(
    p3sym_c3_plain_boundp,
    Unprojected,
    GlobalOp::Boundp { symbol: 0, dst: 1 }
);
primitive!(
    p3sym_c3_plain_makunbound,
    Unprojected,
    GlobalOp::Makunbound { symbol: 0, dst: 1 }
);
primitive!(
    p3sym_c3_projected_value,
    Projected,
    GlobalOp::Value { symbol: 0, dst: 1 }
);
primitive!(
    p3sym_c3_projected_set,
    Projected,
    GlobalOp::Set {
        symbol: 0,
        src: 0,
        dst: 1
    }
);
primitive!(
    p3sym_c3_projected_boundp,
    Projected,
    GlobalOp::Boundp { symbol: 0, dst: 1 }
);
primitive!(
    p3sym_c3_projected_makunbound,
    Projected,
    GlobalOp::Makunbound { symbol: 0, dst: 1 }
);

#[test]
fn p3sym_c3_surface_is_slot_to_slot() {
    let api = include_str!(
        "../../../../../../neovm-value/src/tagged/gc/mutator_registration/symbol_globals.rs"
    );
    for forbidden in [
        "-> Value",
        "-> TaggedValue",
        "-> usize",
        "pub trait",
        "unsafe {",
    ] {
        assert!(
            !api.contains(forbidden),
            "C3 preparation exports {forbidden}"
        );
    }
    let parent =
        include_str!("../../../../../../neovm-value/src/tagged/gc/mutator_registration.rs");
    // The module is a cfg-split pair: value's own unit tests take the private
    // arm, and dependency builds expose it only under `test-support`. Neither
    // arm may exist in a production build.
    assert!(
        parent
            .contains("#[cfg(test)]\n#[cfg(not(feature = \"test-support\"))]\nmod symbol_globals;")
    );
    assert!(parent.contains("#[cfg(feature = \"test-support\")]\npub mod symbol_globals;"));
    let production = include_str!(
        "../../../../../../neovm-value/src/tagged/gc/mutator_registration/symbol_authority/globals.rs"
    );
    for forbidden in [
        "-> Value",
        "-> TaggedValue",
        "-> &Obarray",
        "pub trait",
        "unwrap(",
        "expect(",
        "panic!(",
        "from_current_heap",
        "note_root_overwrite",
    ] {
        assert!(
            !production.contains(forbidden),
            "C3 production surface contains {forbidden}"
        );
    }
    let fixture = oracle();
    assert_ne!(fixture[7], fixture[8], "GNU records both writer orders");
    assert_ne!(fixture[10], fixture[11], "GNU permits a lost update");
}
