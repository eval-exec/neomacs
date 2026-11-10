//! Weak-policy shells.
//! The shell owns the Context construction, the keeper scopes and the owned
//! weak-table projection; the bodies and assertions stay in value. One
//! #[test] per former value row, generated from the same list the
//! completeness check compares with the typed case ALL.
use crate::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::Baton;
use neovm_eval::test_support::authority::{
    admit_owned_weak_tables_parallel_for_test, baton_collect_parallel_world_roots,
    with_weak_sweep_keeper_for_test,
};
use neovm_value::tagged::value::TaggedValue;
use neovm_value::test_support::authority::{
    WeakPolicyCase, WeakPolicyHost, run_weak_policy_case_for_test,
};
use neovm_value::value::{HashTableTest, HashTableWeakness, LispHashTable};

fn clean_context() -> Context {
    // The canonical core boot installs the eval hooks and builds the Context;
    // the weak keeper and projection paths read those tables.
    let mut context = crate::emacs_core::eval_hook_installers::new_context();
    context.set_variable("window-dead-windows-table", TaggedValue::NIL);
    context.gc_collect_exact();
    context
}

fn weak_table(context: &mut Context) -> TaggedValue {
    // SAFETY: the fixture Context is serial and this is the sole serial owner;
    // allocation runs no collection or callback.
    let value =
        unsafe { context.tagged_heap_mut() }.alloc_hash_table(LispHashTable::new_with_options(
            HashTableTest::Eq,
            4,
            Some(HashTableWeakness::Key),
            1.5,
            0.8,
        ));
    context.set_variable("p2ht-b8-policy-table", value);
    value
}

fn keeper_case(case: WeakPolicyCase) {
    let mut context = clean_context();
    let table = weak_table(&mut context);
    with_weak_sweep_keeper_for_test(&mut context, |context, keeper| {
        // SAFETY: the keeper scope is the sole serial owner; no collection or
        // callback runs between the borrow and the case body.
        let heap = unsafe { context.tagged_heap_mut() };
        // SAFETY: the route contract is documented on the value fence; this
        // call passes no World walk (the keeper host does not use one).
        unsafe {
            run_weak_policy_case_for_test(
                case,
                WeakPolicyHost::Keeper {
                    heap,
                    keeper,
                    table,
                },
            );
        }
    });
}

fn foreign_keeper_case(case: WeakPolicyCase) {
    let mut first = clean_context();
    let table = weak_table(&mut first);
    with_weak_sweep_keeper_for_test(&mut first, |first, first_keeper| {
        let mut second = clean_context();
        with_weak_sweep_keeper_for_test(&mut second, |second, second_keeper| {
            let first_heap = first.tagged_heap();
            let second_heap = second.tagged_heap();
            // SAFETY: no World walk is used by the foreign host; the contract
            // applies only to the stopped host.
            unsafe {
                run_weak_policy_case_for_test(
                    case,
                    WeakPolicyHost::ForeignKeeper {
                        first_heap,
                        first_keeper,
                        second_heap,
                        second_keeper,
                        table,
                    },
                );
            }
        });
    });
}

fn stopped_case(case: WeakPolicyCase) {
    let mut context = clean_context();
    let table = weak_table(&mut context);
    admit_owned_weak_tables_parallel_for_test(&mut context, &[table], |projection| {
        let era = projection.era();
        // SAFETY: the era and its stopped guard come from the real owned
        // proof projection; the World walk is the real baton walker, so every
        // chain-head slot is a live BufferText-owned `*mut MarkerObj` valid
        // for this collection cycle, and the projection holds exclusive
        // access to the heap and buffer manager.
        unsafe {
            run_weak_policy_case_for_test(
                case,
                WeakPolicyHost::Stopped {
                    era,
                    table,
                    world_roots: baton_collect_parallel_world_roots,
                },
            );
        }
        let (collector, keeper) = projection.finish().unwrap();
        (collector, keeper, ())
    })
    .unwrap();
}

macro_rules! shells {
    ($($name:ident => $case:ident : $driver:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            $driver(WeakPolicyCase::$case);
        })*
        const SHELL_CASES: &[WeakPolicyCase] = &[$(WeakPolicyCase::$case),*];
    };
}

shells! {
    a6_ht_b8_fence_requires_explicit_complete_weak_inventory => ExplicitCompleteInventory : keeper_case,
    a6_ht_b8_fence_refuses_foreign_and_non_table_designators => ForeignAndNonTableDesignators : foreign_keeper_case,
    a6_ht_b8_fence_refuses_pending_active_test_and_storage_authority => PendingActiveTestAndStorageAuthority : keeper_case,
    a6_ht_b8_fence_refuses_permanent_mapped_and_callback_payloads => PermanentMappedAndCallbackPayloads : keeper_case,
    a6_ht_b8_stopped_fence_refuses_before_collection_mutation_and_recovers => StoppedFenceRefusesBeforeMutationAndRecovers : stopped_case,
}

#[test]
fn shell_cases_cover_all_weak_policy_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = WeakPolicyCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per moved value row");
}
