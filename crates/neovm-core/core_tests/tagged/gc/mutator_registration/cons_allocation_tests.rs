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

fn numeric_roots_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, RootsFixture)) {
    Context::with_test_collection_era(
        |context| {
            let float = Value::make_float(17.25);
            // SAFETY: serial setup owns this initialized float. Root it before
            // allocating the other numeric fixture object.
            let float = unsafe { SharedRoot::new(context.tagged_heap(), float) };
            let bignum = Value::make_int(i64::MAX);
            // SAFETY: serial setup owns this initialized bignum on this heap.
            // Both roots are installed before the era is admitted.
            unsafe {
                (
                    [float, SharedRoot::new(context.tagged_heap(), bignum)],
                    SharedRoot::new(context.tagged_heap(), Value::NIL),
                )
            }
        },
        |era, input| body(era, input),
    );
}

fn owner_reserve_gen1_roots_era_impl(
    body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, RootsFixture),
) {
    Context::with_test_collection_era(
        |context| {
            // SAFETY: setup owns the serial Context before era admission,
            // with no marker, worker, or foreign heap loan.
            unsafe {
                neovm_value::test_support::authority::set_generational_enabled_for_test(
                    context.tagged_heap_mut(),
                    true,
                );
            }
            roots(context)
        },
        |era, input| body(era, input),
    );
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
    let mut roots_era: fn(&mut dyn for<'e> FnMut(ProofEra<'e, Baton>, RootsFixture)) =
        if case == ConsAllocationCase::NumericStorageTransfersIntoEraAndRoundTripsAtStops {
            numeric_roots_era_impl
        } else if case == ConsAllocationCase::OwnerConsReserveAnOldStoreKeepsTheOwnerConsAndTheUnrootedConsIsReclaimed {
            owner_reserve_gen1_roots_era_impl
        } else {
            roots_era_impl
        };
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
    numeric_storage_transfers_into_era_and_round_trips_at_stops => NumericStorageTransfersIntoEraAndRoundTripsAtStops,
    two_owner_numeric_allocations_survive_interleaved_catalog_work => TwoOwnerNumericAllocationsSurviveInterleavedCatalogWork,
    partial_numeric_page_reclaims_tail_without_stop => PartialNumericPageReclaimsTailWithoutStop,
    numeric_mint_receipt_survives_failed_outer_finish => NumericMintReceiptSurvivesFailedOuterFinish,
    numeric_partial_pool_survives_refill_and_swept_prefix_holes => NumericPartialPoolSurvivesRefillAndSweptPrefixHoles,
    numeric_refusals_preserve_destinations_counters_and_requests => NumericRefusalsPreserveDestinationsCountersAndRequests,
    dropping_open_numeric_run_poison_retains_backing => DroppingOpenNumericRunPoisonRetainsBacking,
    forgotten_open_numeric_run_refuses_publication_native_and_finish => ForgottenOpenNumericRunRefusesPublicationNativeAndFinish,
    poisoned_numeric_close_retains_runs_and_counters_on_repeat => PoisonedNumericCloseRetainsRunsAndCountersOnRepeat,
    numeric_stopped_serial_allocations_rebase_both_class_frontiers => NumericStoppedSerialAllocationsRebaseBothClassFrontiers,
    numeric_stopped_serial_allocations_fill_interior_holes_before_owner_tails => NumericStoppedSerialAllocationsFillInteriorHolesBeforeOwnerTails,
    fully_dead_numeric_retained_pages_refreeze_with_full_capacity => FullyDeadNumericRetainedPagesRefreezeWithFullCapacity,
    numeric_exhaustion_requests_are_once_per_owner_class_and_generation => NumericExhaustionRequestsAreOncePerOwnerClassAndGeneration,
    numeric_request_epoch_does_not_repeat_after_an_unobserved_pool => NumericRequestEpochDoesNotRepeatAfterAnUnobservedPool,
    owner_cons_reserve_refuses_use_after_heap_drop => OwnerConsReserveRefusesUseAfterHeapDrop,
    owner_cons_reserve_grants_only_cons_storage => OwnerConsReserveGrantsOnlyConsStorage,
    owner_cons_reserve_refuses_sweep_until_explicit_return => OwnerConsReserveRefusesSweepUntilExplicitReturn,
    owner_cons_reserve_refuses_foreign_book_without_mutation => OwnerConsReserveRefusesForeignBookWithoutMutation,
    owner_cons_reserve_refuses_every_foreign_heap_operation => OwnerConsReserveRefusesEveryForeignHeapOperation,
    owner_cons_reserve_drop_and_forget_keep_collection_excluded => OwnerConsReserveDropAndForgetKeepCollectionExcluded,
    owner_cons_reserve_main_gc_returns_the_book_then_collects => OwnerConsReserveMainGcReturnsTheBookThenCollects,
    owner_cons_reserve_ordinary_gc_refuses_a_live_lease => OwnerConsReserveOrdinaryGcRefusesALiveLease,
    owner_cons_reserve_two_idle_stops_hold_no_reserve => OwnerConsReserveTwoIdleStopsHoldNoReserve,
    owner_cons_reserve_pacing_charges_carved_cells_and_subtracts_the_open_tail => OwnerConsReservePacingChargesCarvedCellsAndSubtractsTheOpenTail,
    owner_cons_reserve_the_cap_is_a_reserve_miss => OwnerConsReserveTheCapIsAReserveMiss,
    owner_cons_reserve_two_owners_carve_disjoint_runs_without_moving_the_heap_window => OwnerConsReserveTwoOwnersCarveDisjointRunsWithoutMovingTheHeapWindow,
    owner_cons_reserve_a_moved_block_tail_is_not_rewound => OwnerConsReserveAMovedBlockTailIsNotRewound,
    owner_cons_reserve_an_old_store_keeps_the_owner_cons_and_the_unrooted_cons_is_reclaimed => OwnerConsReserveAnOldStoreKeepsTheOwnerConsAndTheUnrootedConsIsReclaimed,
}

#[test]
fn shell_cases_cover_all_cons_allocation_case_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = ConsAllocationCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per former value row");
}
