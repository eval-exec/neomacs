//! Cons-allocation shells.
//! The shell performs the serial setup, admits each era, and passes the baton
//! World walk; the bodies and their assertions stay in value.
use crate::emacs_core::eval::Context;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use neovm_eval::emacs_core::eval::Baton;
use neovm_eval::test_support::authority::baton_collect_parallel_world_roots;
use neovm_value::tagged::gc::QuiescentCollector;
use neovm_value::test_support::authority::{
    ConsAllocationCase, ProofEra, run_cons_allocation_test_for_test,
};

type RootsFixture = ([SharedRoot; 2], SharedRoot);
type CreditsFixture = (RootsFixture, usize, [u64; 7], u64);
type CreditsResult = (usize, [u64; 7], u64);

fn roots(context: &mut Context) -> RootsFixture {
    // SAFETY: immediate values are live on this serial heap; no heap object
    // or raw Value is transported to an owner.
    unsafe {
        (
            [
                SharedRoot::new(context.tagged_heap(), Value::fixnum(1701)),
                SharedRoot::new(context.tagged_heap(), Value::fixnum(1702)),
            ],
            SharedRoot::new(context.tagged_heap(), Value::NIL),
        )
    }
}

fn roots_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, RootsFixture)) {
    Context::with_test_collection_era(roots, |era, input| body(era, input));
}

fn empty_outer_era_impl(_flag: i64, body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)) {
    Context::with_test_collection_era(|_| (), |era, ()| body(era));
}

fn roots_nested_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, RootsFixture)) {
    Context::with_test_collection_era(roots, |era, input| body(era, input));
}

fn roots_era_then_words_impl(
    body: &mut dyn for<'e> FnMut(
        ProofEra<'e, Baton>,
        RootsFixture,
    ) -> (QuiescentCollector<'e>, Baton, [usize; 2]),
) {
    Context::with_test_collection_era_then(
        roots,
        |era, input| body(era, input),
        |context, words| {
            context.gc_collect_exact();
            for word in words {
                assert!(
                    !context
                        .tagged_heap()
                        .owns_heap_value_for_test(unsafe { Value::from_bits(word) })
                );
            }
        },
    );
}

fn partial_era_then_impl(
    body: &mut dyn for<'e> FnMut(
        ProofEra<'e, Baton>,
        CreditsFixture,
    ) -> (QuiescentCollector<'e>, Baton, CreditsResult),
) {
    Context::with_test_collection_era_then(
        |context| {
            let inputs = roots(context);
            (
                inputs,
                context.tagged_heap().cons_live_count_exact(),
                context.tagged_heap().memory_use_counts_snapshot(),
                context.tagged_heap().total_allocated_bytes(),
            )
        },
        |era, input| body(era, input),
        |context, (live, memory, bytes)| {
            // Inspect BEFORE another GC: serial reunion itself must preserve
            // exactly the post-sweep balance and cumulative owner histories.
            assert_eq!(context.tagged_heap().allocated_count(), live);
            assert_eq!(context.tagged_heap().memory_use_counts_snapshot(), memory);
            assert_eq!(context.tagged_heap().total_allocated_bytes(), bytes);
        },
    );
}

fn run(case: ConsAllocationCase) {
    let mut roots_era = roots_era_impl;
    let mut empty_outer_era = empty_outer_era_impl;
    let mut roots_nested_era = roots_nested_era_impl;
    let mut roots_era_then_words = roots_era_then_words_impl;
    let mut partial_era_then = partial_era_then_impl;
    run_cons_allocation_test_for_test(
        case,
        &mut roots_era,
        &mut empty_outer_era,
        &mut roots_nested_era,
        &mut roots_era_then_words,
        &mut partial_era_then,
        baton_collect_parallel_world_roots,
    );
}

macro_rules! shells {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            run(ConsAllocationCase::$case);
        })*
        const SHELL_CASES: &[ConsAllocationCase] = &[$(ConsAllocationCase::$case),*];
    };
}

shells! {
    two_owner_cons_allocator_stress => TwoOwnerConsAllocatorStress,
    cons_result_survives_from_published_head_then_reclaims_after_serial_restore => ConsResultSurvivesFromPublishedHeadThenReclaimsAfterSerialRestore,
    cons_without_catalog_refuses_before_charging_or_producing_a_root => ConsWithoutCatalogRefusesBeforeChargingOrProducingARoot,
    foreign_cons_operand_refuses_before_any_grant_and_correct_inputs_still_work => ForeignConsOperandRefusesBeforeAnyGrantAndCorrectInputsStillWork,
    dropping_open_cons_region_poison_retains_storage_and_refuses_publication => DroppingOpenConsRegionPoisonRetainsStorageAndRefusesPublication,
    collection_requires_returned_catalog_and_resume_requires_current_stop => CollectionRequiresReturnedCatalogAndResumeRequiresCurrentStop,
    earlier_tail_close_never_rewinds_through_another_owners_live_region => EarlierTailCloseNeverRewindsThroughAnotherOwnersLiveRegion,
    foreign_stop_cannot_return_or_resume_a_cons_catalog => ForeignStopCannotReturnOrResumeAConsCatalog,
    return_cons_storage_preserves_busy_stop_validation_for_retry => ReturnConsStoragePreservesBusyStopValidationForRetry,
    forgotten_open_cons_run_refuses_publication_native_and_finish_without_waiting => ForgottenOpenConsRunRefusesPublicationNativeAndFinishWithoutWaiting,
    partial_cons_grant_refunds_owner_and_global_credits_then_merges_history_once => PartialConsGrantRefundsOwnerAndGlobalCreditsThenMergesHistoryOnce,
}

#[test]
fn shell_cases_cover_all_cons_allocation_case_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = ConsAllocationCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per former value row");
}
