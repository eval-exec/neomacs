//! Keeper-table scope test re-homed from value (MEMQ-TAIL-SHAPES (b)+(i)+(ii),
//! peer 22:30Z approvals). The era fixture is eval-level, so the whole test
//! lives here.
//!
//! The value internals the pre-move test reached directly (the collector
//! loan, `_satb_layout_reserve`, `collector_tables`, `KeeperTableMut`, the raw
//! table pointer) are replaced by the signed `test_support::authority` seams:
//! the closure-scoped stopped-tables install, the scope's null check / nested
//! install probe, the &self authority-word guard, and the scope's own inspect.
//! Every semantic assertion of the original test is kept.

use super::era_fixtures::EraFixtureExt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{HashTableTest, LispHashTable};
use crate::tagged::value::TaggedValue;
use neovm_value::tagged::gc::mutator_registration::RendezvousError;
use neovm_value::test_support::authority::{AuthorityProbeState, with_stopped_tables_for_test};

#[test]
fn keeper_table_scope_guard_test_clears_on_unwind_and_refuses_nested_install() {
    Context::with_test_collection_era(
        |context| {
            // SAFETY: serial test setup before the World is frozen; the
            // non-swappable view is used in place for this one allocation.
            let table = unsafe { context.tagged_heap_mut() }
                .alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
            context.set_variable("p2ht-keeper-scope-root", table);
            table.bits()
        },
        |era, table_bits| {
            let mut stopped = era.stop().unwrap();
            {
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // SAFETY: `stopped` is a real stop of this era; the body
                    // installs exactly one stopped-tables scope and resumes
                    // nothing.
                    unsafe {
                        with_stopped_tables_for_test(&era, &stopped, |scope| {
                            assert!(
                                !scope.collector_tables_is_null(),
                                "the stopped-tables scope must be installed"
                            );
                            assert!(matches!(
                                scope.try_nested_install_for_test(),
                                Err(RendezvousError::Busy)
                            ));
                            // SAFETY: the rooted table survives the frozen
                            // World; the bits came from the rooted setup.
                            let table = unsafe { TaggedValue::from_bits(table_bits) };
                            scope.enter(table).unwrap();
                            // Simulate a leaked reader then a leaked writer
                            // while all real owners are stopped. The installed
                            // adapter must mint a keeper lease and refuse
                            // before invoking its callback.
                            for state in [
                                AuthorityProbeState::LeakedReader { readers: 1 },
                                AuthorityProbeState::LeakedWriter,
                            ] {
                                // SAFETY: the guard restores the quiescent
                                // word on drop; no other observer runs.
                                let guard = unsafe { scope.hold_authority_word_for_test(state) };
                                let rejected =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        // SAFETY: rooted table and active real stop scope.
                                        unsafe { scope.inspect(|_| ()) };
                                    }));
                                drop(guard);
                                assert!(
                                    rejected.is_err(),
                                    "active collector read bypassed keeper mint"
                                );
                            }
                            // SAFETY: same rooted table and still-active stop scope.
                            unsafe { scope.inspect(|ht| assert!(ht.data.is_empty())) };
                            panic!("exercise scoped bridge cleanup");
                        })
                    };
                }));
                assert!(panic.is_err());
            }
            // The cleaned bridge allows a subsequent real collector job.
            era.collect_stopped(
                &mut stopped,
                neovm_eval::test_support::authority::baton_collect_parallel_world_roots,
            )
            .unwrap();
            stopped.complete().unwrap();
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}
