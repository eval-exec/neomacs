//! Real admitted cons storage: Rust-only owners, rooted results, and A5 stops.
#![cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
use super::rendezvous::entry_state;
use super::*;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::gc::arena_pages::ArenaStorageSnapshot;
use crate::tagged::gc::card_table::ChunkSlot;
use crate::tagged::gc::chunk_map::{ChunkClass, ChunkEntry};
use crate::tagged::gc::cons_allocator::ConsBlockBase;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

mod owner_reserve_cases;

// Serial histories have no competing actor that could make admission Busy.
// Any refusal must fail immediately rather than disappear inside a retry.
fn activate<'era>(permit: MutatorPermit<'era>, heap: HeapIdentity, id: u64) -> Mutator<'era> {
    permit.activate(heap, id).unwrap()
}
fn with_run<R>(owner: &mut Mutator<'_>, body: impl FnOnce(&mut RunAccess<'_, '_>) -> R) -> R {
    run_body(owner.run_access().unwrap(), body)
}
fn run_body<R>(mut run: RunAccess<'_, '_>, body: impl FnOnce(&mut RunAccess<'_, '_>) -> R) -> R {
    let result = body(&mut run);
    // Owner-local tail rewind has no contended admission to retry.
    run.close_cons_region().unwrap();
    result
}
fn complete(stopped: AllStopped<'_>) {
    stopped.complete().unwrap();
}
fn retire(owner: Mutator<'_>) {
    owner.retire_rooted(&[]).unwrap();
}

// Keep the original concurrent schedules separate from strict serial helpers.
// Their admission retry contracts are audited independently of these histories.
mod contended {
    use super::*;

    pub(super) fn retry<T>(mut f: impl FnMut() -> Result<T, RendezvousError>) -> T {
        loop {
            match f() {
                Ok(value) => return value,
                Err(RendezvousError::Busy) => thread::yield_now(),
                Err(error) => panic!("cons protocol: {error:?}"),
            }
        }
    }
    pub(super) fn retry_registration<T>(mut f: impl FnMut() -> Result<T, RegistrationError>) -> T {
        loop {
            match f() {
                Ok(value) => return value,
                Err(RegistrationError::Busy) => thread::yield_now(),
                Err(error) => panic!("cons registration: {error:?}"),
            }
        }
    }
    pub(super) fn activate<'era>(
        mut permit: MutatorPermit<'era>,
        heap: HeapIdentity,
        id: u64,
    ) -> Mutator<'era> {
        loop {
            match permit.activate(heap, id) {
                Ok(owner) => return owner,
                Err(failure) if failure.reason == RegistrationError::Busy => {
                    permit = failure.into_permit();
                    thread::yield_now();
                }
                Err(failure) => panic!("activate cons owner: {:?}", failure.reason),
            }
        }
    }
    pub(super) fn with_run<R>(
        owner: &mut Mutator<'_>,
        body: impl FnOnce(&mut RunAccess<'_, '_>) -> R,
    ) -> R {
        super::run_body(owner.run_access_after_resume_drain().unwrap(), body)
    }
    pub(super) fn apply(mut image: ResumeGcImage<'_, '_>) {
        loop {
            match image.apply() {
                Ok(()) => return,
                Err(failure) if failure.reason == RendezvousError::Busy => {
                    image = failure.into_image();
                    thread::yield_now();
                }
                Err(failure) => panic!("restore cons owner: {:?}", failure.reason),
            }
        }
    }
    pub(super) fn complete(mut stopped: AllStopped<'_>) {
        loop {
            match stopped.complete() {
                Ok(()) => return,
                Err(failure) if failure.reason == RendezvousError::Busy => {
                    stopped = failure.into_stopped();
                    thread::yield_now();
                }
                Err(failure) => panic!("complete cons stop: {:?}", failure.reason),
            }
        }
    }
    pub(super) fn retire(mut owner: Mutator<'_>) {
        loop {
            match owner.retire_rooted(&[]) {
                Ok(()) => return,
                Err(failure) if failure.reason == RendezvousError::Busy => {
                    owner = failure.into_owner();
                    thread::yield_now();
                }
                Err(failure) => panic!("retire cons owner: {:?}", failure.reason),
            }
        }
    }
}
fn receive<T>(rx: &mpsc::Receiver<T>) -> T {
    rx.recv_timeout(Duration::from_secs(30))
        .expect("cons actor reaches bounded boundary")
}
fn check_catalog(heap: &TaggedHeap) {
    heap.cons_allocator.verify();
    for (index, block) in heap.cons_blocks.iter().enumerate() {
        let base = block.base_addr();
        assert_eq!(heap.cons_block_index_by_base.get(&base), Some(&index));
        assert!(heap.cons_allocator.is_ready(ConsBlockBase::of(block)));
        if let Some(map) = heap.chunk_map.as_deref() {
            assert_eq!(map.get(base), ChunkEntry::new(ChunkClass::Cons, index));
        }
        if heap.tagged_heap_generational_enabled() {
            assert!(
                heap.card_slots()
                    .unwrap()
                    .blocks(ChunkSlot::of(base))
                    .contains(&base)
            );
            assert!(block.trailer().ages_match_old_bits());
        }
    }
}
fn check_pair(heap: &TaggedHeap, root: &SharedRoot, car: i64) {
    let local = root.materialize(heap).unwrap();
    let value = local.value();
    assert!(heap.owns_heap_value_for_test(value));
    // SAFETY: the matching stopped heap retains this rooted initialized cell;
    // owners hold no run/grant and cannot write it during inspection.
    unsafe {
        let cell = &*value.as_cons_ptr().unwrap();
        assert_eq!(cell.car.as_fixnum(), Some(car));
        assert!(cell.cdr().is_nil());
    }
}
struct PoisonOnUnwind<'a>(&'a Registry);
impl Drop for PoisonOnUnwind<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.poison();
        }
    }
}

pub(crate) type RootsFixture = ([SharedRoot; 2], SharedRoot);
pub(crate) type CreditsFixture = (
    RootsFixture,
    usize,
    [u64; crate::tagged::gc::MEMORY_USE_COUNT_LEN],
    u64,
);
pub(crate) type CreditsResult = (usize, [u64; crate::tagged::gc::MEMORY_USE_COUNT_LEN], u64);

/// One row per former value lib-test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConsAllocationCase {
    TwoOwnerConsAllocatorStress,
    ConsWithoutCatalogRefusesBeforeChargingOrProducingARoot,
    ForeignConsOperandRefusesBeforeAnyGrantAndCorrectInputsStillWork,
    DroppingOpenConsRegionPoisonRetainsStorageAndRefusesPublication,
    CollectionRequiresReturnedCatalogAndResumeRequiresCurrentStop,
    EarlierTailCloseNeverRewindsThroughAnotherOwnersLiveRegion,
    ReturnConsStorageWaitsOutABriefGateHolder,
    CollectWaitsOutAnOwnerMomentInItsReaderScan,
    ForgottenOpenConsRunRefusesPublicationNativeAndFinishWithoutWaiting,
    ForeignStopCannotReturnOrResumeAConsCatalog,
    ConsResultSurvivesFromPublishedHeadThenReclaimsAfterSerialRestore,
    PartialConsGrantRefundsOwnerAndGlobalCreditsThenMergesHistoryOnce,
    NumericStorageTransfersIntoEraAndRoundTripsAtStops,
    TwoOwnerNumericAllocationsSurviveInterleavedCatalogWork,
    PartialNumericPageReclaimsTailWithoutStop,
    NumericMintReceiptSurvivesFailedOuterFinish,
    NumericPartialPoolSurvivesRefillAndSweptPrefixHoles,
    NumericRefusalsPreserveDestinationsCountersAndRequests,
    DroppingOpenNumericRunPoisonRetainsBacking,
    ForgottenOpenNumericRunRefusesPublicationNativeAndFinish,
    PoisonedNumericCloseRetainsRunsAndCountersOnRepeat,
    NumericStoppedSerialAllocationsRebaseBothClassFrontiers,
    NumericStoppedSerialAllocationsFillInteriorHolesBeforeOwnerTails,
    FullyDeadNumericRetainedPagesRefreezeWithFullCapacity,
    NumericExhaustionRequestsAreOncePerOwnerClassAndGeneration,
    NumericRequestEpochDoesNotRepeatAfterAnUnobservedPool,
    OwnerConsReserveRefusesUseAfterHeapDrop,
    OwnerConsReserveGrantsOnlyConsStorage,
    OwnerConsReserveSessionReturnsClosedBookThenCollects,
    OwnerConsReserveRefusesForeignBookWithoutMutation,
    OwnerConsReserveRefusesEveryForeignHeapOperation,
    OwnerConsReserveDropAndForgetKeepCollectionExcluded,
    OwnerConsReserveTwoIdleStopsHoldNoReserve,
    OwnerConsReservePacingChargesCarvedCellsAndSubtractsTheOpenTail,
    OwnerConsReserveTheCapIsAReserveMiss,
    OwnerConsReserveTwoOwnersCarveDisjointRunsWithoutMovingTheHeapWindow,
    OwnerConsReserveAMovedBlockTailIsNotRewound,
    OwnerConsReserveAnOldStoreKeepsTheOwnerConsAndTheUnrootedConsIsReclaimed,
    OwnerConsReserveMainGcReturnsTheBookThenCollects,
    OwnerConsReserveOrdinaryGcRefusesALiveLease,
    OwnerConsReservePoisonedAllocatorIsPublication,
    OwnerConsReserveLeftoverCountIsPublication,
    OwnerConsReserveOrdinaryRequestRoutesThroughStop,
    OwnerConsReserveResumeOfALiveLeaseIsNotBusy,
    OwnerConsReserveSessionReturnKeepsHandedCellAndReclaimsTail,
    OwnerConsReserveRetireRootedReclaimsOneRegistration,
    OwnerConsReserveRetireQueueHygieneSkipsUnleasedAndDrainsAtStop,
    OwnerConsReserveRetireRequiresClosedBookAndReclaimsCarvedTail,
    OwnerConsReserveFinishRefusesLiveLeaseUntilReclaimed,
    OwnerConsReserveReclaimBusyRequeuesForLaterApply,
    OwnerConsReserveReclaimPoisonedAllocatorRequeues,
    OwnerConsReserveReclaimRefusesWrongThread,
}

impl ConsAllocationCase {
    pub const ALL: &'static [Self] = &[
        Self::TwoOwnerConsAllocatorStress,
        Self::ConsWithoutCatalogRefusesBeforeChargingOrProducingARoot,
        Self::ForeignConsOperandRefusesBeforeAnyGrantAndCorrectInputsStillWork,
        Self::DroppingOpenConsRegionPoisonRetainsStorageAndRefusesPublication,
        Self::CollectionRequiresReturnedCatalogAndResumeRequiresCurrentStop,
        Self::EarlierTailCloseNeverRewindsThroughAnotherOwnersLiveRegion,
        Self::ReturnConsStorageWaitsOutABriefGateHolder,
        Self::CollectWaitsOutAnOwnerMomentInItsReaderScan,
        Self::ForgottenOpenConsRunRefusesPublicationNativeAndFinishWithoutWaiting,
        Self::ForeignStopCannotReturnOrResumeAConsCatalog,
        Self::ConsResultSurvivesFromPublishedHeadThenReclaimsAfterSerialRestore,
        Self::PartialConsGrantRefundsOwnerAndGlobalCreditsThenMergesHistoryOnce,
        Self::NumericStorageTransfersIntoEraAndRoundTripsAtStops,
        Self::TwoOwnerNumericAllocationsSurviveInterleavedCatalogWork,
        Self::PartialNumericPageReclaimsTailWithoutStop,
        Self::NumericMintReceiptSurvivesFailedOuterFinish,
        Self::NumericPartialPoolSurvivesRefillAndSweptPrefixHoles,
        Self::NumericRefusalsPreserveDestinationsCountersAndRequests,
        Self::DroppingOpenNumericRunPoisonRetainsBacking,
        Self::ForgottenOpenNumericRunRefusesPublicationNativeAndFinish,
        Self::PoisonedNumericCloseRetainsRunsAndCountersOnRepeat,
        Self::NumericStoppedSerialAllocationsRebaseBothClassFrontiers,
        Self::NumericStoppedSerialAllocationsFillInteriorHolesBeforeOwnerTails,
        Self::FullyDeadNumericRetainedPagesRefreezeWithFullCapacity,
        Self::NumericExhaustionRequestsAreOncePerOwnerClassAndGeneration,
        Self::NumericRequestEpochDoesNotRepeatAfterAnUnobservedPool,
        Self::OwnerConsReserveRefusesUseAfterHeapDrop,
        Self::OwnerConsReserveGrantsOnlyConsStorage,
        Self::OwnerConsReserveSessionReturnsClosedBookThenCollects,
        Self::OwnerConsReserveRefusesForeignBookWithoutMutation,
        Self::OwnerConsReserveRefusesEveryForeignHeapOperation,
        Self::OwnerConsReserveDropAndForgetKeepCollectionExcluded,
        Self::OwnerConsReserveTwoIdleStopsHoldNoReserve,
        Self::OwnerConsReservePacingChargesCarvedCellsAndSubtractsTheOpenTail,
        Self::OwnerConsReserveTheCapIsAReserveMiss,
        Self::OwnerConsReserveTwoOwnersCarveDisjointRunsWithoutMovingTheHeapWindow,
        Self::OwnerConsReserveAMovedBlockTailIsNotRewound,
        Self::OwnerConsReserveAnOldStoreKeepsTheOwnerConsAndTheUnrootedConsIsReclaimed,
        Self::OwnerConsReserveMainGcReturnsTheBookThenCollects,
        Self::OwnerConsReserveOrdinaryGcRefusesALiveLease,
        Self::OwnerConsReservePoisonedAllocatorIsPublication,
        Self::OwnerConsReserveLeftoverCountIsPublication,
        Self::OwnerConsReserveOrdinaryRequestRoutesThroughStop,
        Self::OwnerConsReserveResumeOfALiveLeaseIsNotBusy,
        Self::OwnerConsReserveSessionReturnKeepsHandedCellAndReclaimsTail,
        Self::OwnerConsReserveRetireRootedReclaimsOneRegistration,
        Self::OwnerConsReserveRetireQueueHygieneSkipsUnleasedAndDrainsAtStop,
        Self::OwnerConsReserveRetireRequiresClosedBookAndReclaimsCarvedTail,
        Self::OwnerConsReserveFinishRefusesLiveLeaseUntilReclaimed,
        Self::OwnerConsReserveReclaimBusyRequeuesForLaterApply,
        Self::OwnerConsReserveReclaimPoisonedAllocatorRequeues,
        Self::OwnerConsReserveReclaimRefusesWrongThread,
    ];
}

/// Dispatcher for the cons-allocation shells. The core shell performs the
/// serial setup, admits each era, and supplies the baton World walk; the
/// bodies and their assertions stay here.
pub(crate) fn run_cons_allocation_test<K, W>(
    case: ConsAllocationCase,
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    empty_outer_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>)),
    roots_nested_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    roots_era_then_words: &mut dyn FnMut(
        &mut dyn for<'e> FnMut(
            ParallelHeapEra<'e, K>,
            RootsFixture,
        ) -> (QuiescentCollector<'e>, K, [usize; 2]),
    ),
    partial_era_then: &mut dyn FnMut(
        &mut dyn for<'e> FnMut(
            ParallelHeapEra<'e, K>,
            CreditsFixture,
        ) -> (QuiescentCollector<'e>, K, CreditsResult),
    ),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    match case {
        ConsAllocationCase::TwoOwnerConsAllocatorStress => {
            two_owner_cons_allocator_stress(roots_era, world_roots)
        }
        ConsAllocationCase::ConsWithoutCatalogRefusesBeforeChargingOrProducingARoot => {
            cons_without_catalog_refuses_before_charging_or_producing_a_root(roots_era, world_roots)
        }
        ConsAllocationCase::ForeignConsOperandRefusesBeforeAnyGrantAndCorrectInputsStillWork => {
            foreign_cons_operand_refuses_before_any_grant_and_correct_inputs_still_work(
                roots_era,
                world_roots,
            )
        }
        ConsAllocationCase::DroppingOpenConsRegionPoisonRetainsStorageAndRefusesPublication => {
            dropping_open_cons_region_poison_retains_storage_and_refuses_publication(
                roots_era,
                world_roots,
            )
        }
        ConsAllocationCase::CollectionRequiresReturnedCatalogAndResumeRequiresCurrentStop => {
            collection_requires_returned_catalog_and_resume_requires_current_stop(
                roots_era,
                world_roots,
            )
        }
        ConsAllocationCase::EarlierTailCloseNeverRewindsThroughAnotherOwnersLiveRegion => {
            earlier_tail_close_never_rewinds_through_another_owners_live_region(
                roots_era,
                world_roots,
            )
        }
        ConsAllocationCase::ReturnConsStorageWaitsOutABriefGateHolder => {
            return_cons_storage_waits_out_a_brief_gate_holder(roots_era, world_roots)
        }
        ConsAllocationCase::CollectWaitsOutAnOwnerMomentInItsReaderScan => {
            collect_waits_out_an_owner_moment_in_its_reader_scan(roots_era, world_roots)
        }
        ConsAllocationCase::ForgottenOpenConsRunRefusesPublicationNativeAndFinishWithoutWaiting => {
            forgotten_open_cons_run_refuses_publication_native_and_finish_without_waiting(
                roots_era,
                world_roots,
            )
        }
        ConsAllocationCase::ForeignStopCannotReturnOrResumeAConsCatalog => {
            foreign_stop_cannot_return_or_resume_a_cons_catalog(
                empty_outer_era,
                roots_nested_era,
                world_roots,
            );
        }
        ConsAllocationCase::ConsResultSurvivesFromPublishedHeadThenReclaimsAfterSerialRestore => {
            cons_result_survives_from_published_head_then_reclaims_after_serial_restore(
                roots_era_then_words,
                world_roots,
            );
        }
        ConsAllocationCase::PartialConsGrantRefundsOwnerAndGlobalCreditsThenMergesHistoryOnce => {
            partial_cons_grant_refunds_owner_and_global_credits_then_merges_history_once(
                partial_era_then,
                world_roots,
            );
        }
        ConsAllocationCase::NumericStorageTransfersIntoEraAndRoundTripsAtStops => {
            numeric_storage_transfers_into_era_and_round_trips_at_stops(roots_era, world_roots);
        }
        ConsAllocationCase::TwoOwnerNumericAllocationsSurviveInterleavedCatalogWork => {
            two_owner_numeric_allocations_survive_interleaved_catalog_work(roots_era, world_roots);
        }
        ConsAllocationCase::PartialNumericPageReclaimsTailWithoutStop => {
            partial_numeric_page_reclaims_tail_without_stop(roots_era, world_roots);
        }
        ConsAllocationCase::NumericMintReceiptSurvivesFailedOuterFinish => {
            numeric_mint_receipt_survives_failed_outer_finish(roots_era, world_roots);
        }
        ConsAllocationCase::NumericPartialPoolSurvivesRefillAndSweptPrefixHoles => {
            numeric_partial_pool_survives_refill_and_swept_prefix_holes(roots_era, world_roots);
        }
        ConsAllocationCase::NumericRefusalsPreserveDestinationsCountersAndRequests => {
            numeric_refusals_preserve_destinations_counters_and_requests(roots_era, world_roots);
        }
        ConsAllocationCase::DroppingOpenNumericRunPoisonRetainsBacking => {
            dropping_open_numeric_run_poison_retains_backing(roots_era, world_roots);
        }
        ConsAllocationCase::ForgottenOpenNumericRunRefusesPublicationNativeAndFinish => {
            forgotten_open_numeric_run_refuses_publication_native_and_finish(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::PoisonedNumericCloseRetainsRunsAndCountersOnRepeat => {
            poisoned_numeric_close_retains_runs_and_counters_on_repeat(roots_era, world_roots);
        }
        ConsAllocationCase::NumericStoppedSerialAllocationsRebaseBothClassFrontiers => {
            numeric_stopped_serial_allocations_rebase_both_class_frontiers(roots_era, world_roots);
        }
        ConsAllocationCase::NumericStoppedSerialAllocationsFillInteriorHolesBeforeOwnerTails => {
            numeric_stopped_serial_allocations_fill_interior_holes_before_owner_tails(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::FullyDeadNumericRetainedPagesRefreezeWithFullCapacity => {
            fully_dead_numeric_retained_pages_refreeze_with_full_capacity(roots_era, world_roots);
        }
        ConsAllocationCase::NumericExhaustionRequestsAreOncePerOwnerClassAndGeneration => {
            numeric_exhaustion_requests_are_once_per_owner_class_and_generation(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::NumericRequestEpochDoesNotRepeatAfterAnUnobservedPool => {
            numeric_request_epoch_does_not_repeat_after_an_unobserved_pool(roots_era, world_roots);
        }
        ConsAllocationCase::OwnerConsReserveRefusesUseAfterHeapDrop => {
            owner_reserve_cases::owner_cons_reserve_refuses_use_after_heap_drop(roots_era);
        }
        ConsAllocationCase::OwnerConsReserveRefusesEveryForeignHeapOperation => {
            owner_reserve_cases::owner_cons_reserve_refuses_every_foreign_heap_operation(roots_era, roots_nested_era);
        }
        ConsAllocationCase::OwnerConsReserveDropAndForgetKeepCollectionExcluded => {
            owner_reserve_cases::owner_cons_reserve_drop_and_forget_keep_collection_excluded(roots_era, world_roots);
        }
        ConsAllocationCase::OwnerConsReserveRefusesForeignBookWithoutMutation => {
            owner_reserve_cases::owner_cons_reserve_refuses_foreign_book_without_mutation(roots_era);
        }
        ConsAllocationCase::OwnerConsReserveSessionReturnsClosedBookThenCollects => {
            owner_reserve_cases::owner_cons_reserve_session_returns_closed_book_then_collects(roots_era, world_roots);
        }
        ConsAllocationCase::OwnerConsReserveMainGcReturnsTheBookThenCollects => {
            owner_reserve_cases::owner_cons_reserve_main_gc_returns_the_book_then_collects(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::OwnerConsReserveOrdinaryGcRefusesALiveLease => {
            owner_reserve_cases::owner_cons_reserve_ordinary_gc_refuses_a_live_lease(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::OwnerConsReservePoisonedAllocatorIsPublication => {
            owner_reserve_cases::owner_cons_reserve_poisoned_allocator_is_publication(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::OwnerConsReserveLeftoverCountIsPublication => {
            owner_reserve_cases::owner_cons_reserve_leftover_count_is_publication(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::OwnerConsReserveOrdinaryRequestRoutesThroughStop => {
            owner_reserve_cases::owner_cons_reserve_ordinary_request_routes_through_stop(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::OwnerConsReserveResumeOfALiveLeaseIsNotBusy => {
            owner_reserve_cases::owner_cons_reserve_resume_of_a_live_lease_is_not_busy(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::OwnerConsReserveSessionReturnKeepsHandedCellAndReclaimsTail => {
            owner_reserve_cases::owner_cons_reserve_session_return_keeps_handed_cell_and_reclaims_tail(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::OwnerConsReserveRetireRootedReclaimsOneRegistration => {
            owner_reserve_cases::owner_cons_reserve_retire_rooted_reclaims_one_registration(
                roots_era,
            );
        }
        ConsAllocationCase::OwnerConsReserveRetireQueueHygieneSkipsUnleasedAndDrainsAtStop => {
            owner_reserve_cases::owner_cons_reserve_retire_queue_hygiene_skips_unleased_and_drains_at_stop(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::OwnerConsReserveRetireRequiresClosedBookAndReclaimsCarvedTail => {
            owner_reserve_cases::owner_cons_reserve_retire_requires_closed_book_and_reclaims_carved_tail(
                roots_era,
                world_roots,
            );
        }
        ConsAllocationCase::OwnerConsReserveFinishRefusesLiveLeaseUntilReclaimed => {
            owner_reserve_cases::owner_cons_reserve_finish_refuses_live_lease_until_reclaimed(
                roots_era,
            );
        }
        ConsAllocationCase::OwnerConsReserveReclaimBusyRequeuesForLaterApply => {
            owner_reserve_cases::owner_cons_reserve_reclaim_busy_requeues_for_later_apply(
                roots_era,
            );
        }
        ConsAllocationCase::OwnerConsReserveReclaimPoisonedAllocatorRequeues => {
            owner_reserve_cases::owner_cons_reserve_reclaim_poisoned_allocator_requeues(roots_era);
        }
        ConsAllocationCase::OwnerConsReserveReclaimRefusesWrongThread => {
            owner_reserve_cases::owner_cons_reserve_reclaim_refuses_wrong_thread(roots_era);
        }
        ConsAllocationCase::OwnerConsReserveGrantsOnlyConsStorage => {
            owner_reserve_cases::owner_cons_reserve_grants_only_cons_storage(roots_era);
        }
        ConsAllocationCase::OwnerConsReserveTwoIdleStopsHoldNoReserve
        | ConsAllocationCase::OwnerConsReservePacingChargesCarvedCellsAndSubtractsTheOpenTail
        | ConsAllocationCase::OwnerConsReserveTheCapIsAReserveMiss
        | ConsAllocationCase::OwnerConsReserveTwoOwnersCarveDisjointRunsWithoutMovingTheHeapWindow
        | ConsAllocationCase::OwnerConsReserveAMovedBlockTailIsNotRewound
        | ConsAllocationCase::OwnerConsReserveAnOldStoreKeepsTheOwnerConsAndTheUnrootedConsIsReclaimed => {
            owner_reserve_cases::owner_cons_reserve_existing_cons_case(case, roots_era, world_roots);
        }
    }
}

fn check_numeric_arena_placeholder(arena: &ArenaStorageSnapshot, class: &str) {
    assert!(
        arena.pages.is_empty(),
        "{class} pages stayed in the serial shell"
    );
    assert_eq!(
        arena.pages_capacity, 0,
        "{class} page descriptor allocation stayed behind"
    );
    assert!(
        arena.page_index.is_empty(),
        "{class} page index stayed in the serial shell"
    );
    assert_eq!(
        arena.page_index_capacity, 0,
        "{class} page index allocation stayed behind"
    );
    assert!(
        arena.spare_storage.is_empty(),
        "{class} spare storage stayed in the serial shell"
    );
    assert_eq!(
        arena.spare_storage_capacity, 0,
        "{class} spare descriptor allocation stayed behind"
    );
    assert_eq!(
        arena.spare_unused_floor, 0,
        "{class} spare history stayed behind"
    );
    assert_eq!(
        arena.partial_head,
        usize::MAX,
        "{class} partial chain stayed behind"
    );
}

fn check_numeric_roots(heap: &TaggedHeap, roots: &[SharedRoot; 2]) {
    let float = roots[0].materialize(heap).unwrap().value();
    let bignum = roots[1].materialize(heap).unwrap().value();
    assert!(heap.owns_heap_value_for_test(float));
    assert!(heap.owns_heap_value_for_test(bignum));
    assert_eq!(float.as_float(), Some(17.25));
    assert_eq!(
        bignum.as_bignum().unwrap().to_string(),
        "9223372036854775807"
    );
}

fn numeric_storage_transfers_into_era_and_round_trips_at_stops<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, (numbers, nil)| {
        let initial_shape = {
            let mut collector = era.core.collector.borrow_mut();
            let heap = collector.as_mut().unwrap().registration_heap_mut();
            check_numeric_roots(heap, &numbers);
            (
                heap.float_arena.storage_snapshot_for_test(),
                heap.bignum_arena.storage_snapshot_for_test(),
            )
        };
        assert!(!initial_shape.0.pages.is_empty());
        assert!(!initial_shape.1.pages.is_empty());
        era.start_cons_storage().unwrap();
        let placeholders = era
            .core
            .suspended_cons
            .borrow()
            .as_ref()
            .unwrap()
            .numeric_storage_snapshot_for_test();
        check_numeric_arena_placeholder(&placeholders.0, "float");
        check_numeric_arena_placeholder(&placeholders.1, "bignum");

        let mut owner_a = activate(era.register(1730, 0).unwrap(), era.heap_identity(), 1730);
        let mut owner_b = activate(era.register(1731, 0).unwrap(), era.heap_identity(), 1731);
        owner_a.enter_native(&[numbers[0].clone(), nil]).unwrap();
        owner_b
            .enter_native(std::slice::from_ref(&numbers[1]))
            .unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| {
            check_numeric_roots(heap, &numbers);
            assert_eq!(
                heap.float_arena.storage_snapshot_for_test(),
                initial_shape.0
            );
            assert_eq!(
                heap.bignum_arena.storage_snapshot_for_test(),
                initial_shape.1
            );
        })
        .unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        let collected_shape = era
            .with_stopped_heap_for_test(&stopped, |heap| {
                check_numeric_roots(heap, &numbers);
                (
                    heap.float_arena.storage_snapshot_for_test(),
                    heap.bignum_arena.storage_snapshot_for_test(),
                )
            })
            .unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        owner_a.leave_native().unwrap();
        owner_b.leave_native().unwrap();

        owner_a
            .enter_native(std::slice::from_ref(&numbers[0]))
            .unwrap();
        owner_b
            .enter_native(std::slice::from_ref(&numbers[1]))
            .unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| {
            check_numeric_roots(heap, &numbers);
            assert_eq!(
                heap.float_arena.storage_snapshot_for_test(),
                collected_shape.0
            );
            assert_eq!(
                heap.bignum_arena.storage_snapshot_for_test(),
                collected_shape.1
            );
        })
        .unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| check_numeric_roots(heap, &numbers))
            .unwrap();
        complete(stopped);
        owner_a.leave_native().unwrap();
        owner_b.leave_native().unwrap();
        retire(owner_a);
        retire(owner_b);
        drop(numbers);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn two_owner_cons_allocator_stress<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use contended::{activate, apply, complete, retire, retry, retry_registration, with_run};

    roots_era(&mut |era, (cars, nil)| {
        const PHASES: usize = 2;
        const PER_OWNER: usize = 4200; // exceeds a real cons block per owner

        era.start_cons_storage().unwrap();
        let heap = era.heap_identity();
        let permits = [
            era.register(1701, 0).unwrap(),
            era.register(1702, 0).unwrap(),
        ];
        let active = Arc::new(AtomicUsize::new(0));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let (acked_tx, acked_rx) = mpsc::channel();
        thread::scope(|scope| {
            let _abort = PoisonOnUnwind(era.registry());
            // Senders are inside the scope closure: coordinator unwind
            // disconnects actors before thread::scope attempts to join.
            let mut commands = Vec::new();
            for (index, (permit, car)) in permits.into_iter().zip(cars).enumerate() {
                let (go_tx, go_rx) = mpsc::channel();
                commands.push(go_tx);
                let ready = ready_tx.clone();
                let results = result_tx.clone();
                let acked = acked_tx.clone();
                let active = Arc::clone(&active);
                let nil = nil.clone();
                scope.spawn(move || {
                    let mut owner = activate(permit, heap, 1701 + index as u64);
                    let mut all_roots = Vec::new();
                    for phase in 0..PHASES {
                        let allocated = with_run(&mut owner, |run| {
                            active.fetch_add(1, Ordering::SeqCst);
                            ready.send((index, phase)).unwrap();
                            if go_rx.recv().is_err() {
                                active.fetch_sub(1, Ordering::SeqCst);
                                return None;
                            }
                            let mut allocated = Vec::with_capacity(PER_OWNER);
                            for sequence in 0..PER_OWNER {
                                allocated.push(retry_registration(|| run.cons(&car, &nil)));
                                if sequence % 31 == 0 {
                                    thread::yield_now();
                                }
                            }
                            active.fetch_sub(1, Ordering::SeqCst);
                            Some(allocated)
                        });
                        let Some(allocated) = allocated else { return };
                        all_roots.extend(allocated.iter().cloned());
                        results.send((index, phase, allocated)).unwrap();
                        let epoch = retry(|| owner.wait_request());
                        retry(|| owner.acknowledge(epoch, &all_roots));
                        // Owner-exclusive tails return at the ACK, not at the
                        // run close: signal only once the catalog is coherent.
                        acked.send(()).unwrap();
                        apply(owner.begin_restore_retry(false).unwrap());
                    }
                    drop(all_roots);
                    retire(owner);
                });
            }
            drop(ready_tx);
            drop(result_tx);
            drop(acked_tx);
            let mut addresses = HashSet::new();
            for phase in 0..PHASES {
                let first = receive(&ready_rx);
                let second = receive(&ready_rx);
                assert_eq!((first.1, second.1), (phase, phase));
                assert_ne!(first.0, second.0);
                assert_eq!(active.load(Ordering::SeqCst), 2);
                for go in &commands {
                    go.send(()).unwrap();
                }
                let responses = [receive(&result_rx), receive(&result_rx)];
                // Owners park in wait_request until the keeper requests the
                // epoch; the ACK barrier can only drain after that request.
                retry(|| era.rendezvous().request());
                // Both ACKs publish every held tail suffix before this
                // snapshot can observe a coherent zero-charge catalog.
                for _ in 0..2 {
                    receive(&acked_rx);
                }
                let catalog = era.cons_catalog_snapshot_for_test().unwrap();
                assert!(catalog.installed && catalog.metadata_coherent);
                assert_eq!(catalog.active_regions, 0);
                assert!(
                    catalog.source_grants[2] > 0,
                    "stress installs real fresh blocks"
                );
                let mut stopped = retry(|| era.stop());
                retry_registration(|| era.return_cons_storage(&stopped));
                retry(|| {
                    era.with_stopped_heap_for_test(&stopped, |heap| {
                        check_catalog(heap);
                        for (index, returned_phase, roots) in &responses {
                            assert_eq!(*returned_phase, phase);
                            for root in roots {
                                assert!(
                                    addresses.insert(root.cons_word_for_test()),
                                    "overlapping live cells"
                                );
                                check_pair(heap, root, 1701 + *index as i64);
                            }
                        }
                    })
                });
                let report = retry(|| era.collect_stopped(&mut stopped, world_roots));
                assert!(report.major);
                retry(|| {
                    era.with_stopped_heap_for_test(&stopped, |heap| {
                        check_catalog(heap);
                        for (index, _, roots) in &responses {
                            for root in roots {
                                check_pair(heap, root, 1701 + *index as i64);
                            }
                        }
                    })
                });
                if phase + 1 != PHASES {
                    retry_registration(|| era.resume_cons_storage(&stopped));
                }
                complete(stopped);
            }
        });
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn cons_without_catalog_refuses_before_charging_or_producing_a_root<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, (cars, nil)| {
        let mut owner = activate(era.register(1711, 0).unwrap(), era.heap_identity(), 1711);
        with_run(&mut owner, |run| {
            let before = (
                run.state().hot.allocated_count,
                run.state().hot.bytes_since_gc,
            );
            assert_eq!(
                run.cons(&cars[0], &nil).unwrap_err(),
                RegistrationError::Publication
            );
            assert_eq!(
                (
                    run.state().hot.allocated_count,
                    run.state().hot.bytes_since_gc
                ),
                before
            );
            assert!(run.cons_region_for_test().is_none());
        });
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn foreign_cons_operand_refuses_before_any_grant_and_correct_inputs_still_work<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    let foreign_heap = TaggedHeap::new();
    // SAFETY: NIL is immediate and live; this root carries a real foreign brand.
    let foreign = unsafe { SharedRoot::new(&foreign_heap, Value::NIL) };
    roots_era(&mut |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1712, 0).unwrap(), era.heap_identity(), 1712);
        let root = with_run(&mut owner, |run| {
            let before = (
                run.state().hot.allocated_count,
                run.state().hot.bytes_since_gc,
            );
            for (car, cdr) in [(&foreign, &nil), (&cars[0], &foreign)] {
                assert_eq!(
                    run.cons(car, cdr).unwrap_err(),
                    RegistrationError::WrongHeap
                );
                assert_eq!(
                    (
                        run.state().hot.allocated_count,
                        run.state().hot.bytes_since_gc
                    ),
                    before
                );
                assert!(run.cons_region_for_test().is_none());
            }
            run.cons(&cars[0], &nil).unwrap()
        });
        owner.enter_native(std::slice::from_ref(&root)).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| check_pair(heap, &root, 1701))
            .unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        drop(root);
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn dropping_open_cons_region_poison_retains_storage_and_refuses_publication<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1713, 0).unwrap(), era.heap_identity(), 1713);
        let root = {
            let mut run = owner.run_access().unwrap();
            let root = run.cons(&cars[0], &nil).unwrap();
            assert!(run.cons_region_for_test().is_some());
            drop(run); // deliberately omit explicit close; Drop cannot block
            root
        };
        assert_eq!(
            owner.enter_native(std::slice::from_ref(&root)),
            Err(RendezvousError::Poisoned)
        );
        assert_eq!(era.start_cons_storage(), Err(RegistrationError::Poisoned));
        drop((owner, root));
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::OpenRegion
        );
        // The fixture intentionally does not resume the suspended Context.
        // Abandoned era/owner retain backing instead of accepting a fake stop.
    });
}

fn collection_requires_returned_catalog_and_resume_requires_current_stop<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1714, 0).unwrap(), era.heap_identity(), 1714);
        let root = with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
        owner.enter_native(std::slice::from_ref(&root)).unwrap();
        let mut stopped = era.stop().unwrap();
        assert_eq!(
            era.collect_stopped(&mut stopped, world_roots).unwrap_err(),
            RendezvousError::Publication
        );
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        let second = with_run(&mut owner, |run| run.cons(&cars[1], &nil).unwrap());
        owner.enter_native(&[root.clone(), second.clone()]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| {
            check_pair(heap, &root, 1701);
            check_pair(heap, &second, 1702);
        })
        .unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        drop((root, second));
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn earlier_tail_close_never_rewinds_through_another_owners_live_region<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let heap = era.heap_identity();
        let mut first = activate(era.register(1715, 0).unwrap(), heap, 1715);
        let mut second = activate(era.register(1716, 0).unwrap(), heap, 1716);
        // Serial bootstrap GC may leave ready free nodes. Consume them through
        // the REAL lasting API until a fresh-source witness, rather than
        // assuming the first grant happens to be a bump tail.
        let seeds = with_run(&mut first, |run| {
            let mut seeds = Vec::new();
            for _ in 0..100_000 {
                seeds.push(run.cons(&cars[0], &nil).unwrap());
                if era.cons_catalog_snapshot_for_test().unwrap().source_grants[2] != 0 {
                    return seeds;
                }
            }
            panic!("bounded bootstrap free storage never reached a fresh block")
        });
        let before = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(before.metadata_coherent);
        // The seeding owner still holds its exclusive tail credit: closed
        // regions no longer return storage until the owner ACKs.
        assert!(before.active_regions >= 1);
        let mut a = first.run_access().unwrap();
        let a_root = a.cons(&cars[0], &nil).unwrap();
        let a_bounds = a.cons_region_for_test().unwrap();
        let mut b = second.run_access().unwrap();
        let b_root = b.cons(&cars[1], &nil).unwrap();
        let b_bounds = b.cons_region_for_test().unwrap();
        let during = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(during.active_regions >= 2, "each owner holds a tail");
        assert!(
            (during.source_grants[0] + during.source_grants[1] + during.source_grants[2])
                >= (before.source_grants[0] + before.source_grants[1] + before.source_grants[2])
                    + 2,
            "both owners carved from newly lent tails"
        );
        // Owners never share a tail any more: their exclusive ranges are
        // disjoint by construction instead of by grant ordering.
        assert!(
            a_bounds.1 <= b_bounds.0 || b_bounds.1 <= a_bounds.0,
            "owner tails are disjoint"
        );
        // A bootstrap free fragment may hand a whole one-cell run, so an
        // open region can already be empty after its first cons; only the
        // handed ranges and their disjointness are protocol invariants.
        a.close_cons_region().unwrap();
        let after_a = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(after_a.active_regions >= 1);
        // The local rewind returns nothing through the shared gate.
        assert_eq!(after_a.free_count, during.free_count);
        assert!(a.cons_region_for_test().is_none());
        // The earlier suffix is reusable while B still retains its own live
        // region: A's next cell is the immediate tail successor of its own
        // initialized prefix, never a cell of B or the free list.
        let reused = a.cons(&cars[0], &nil).unwrap();
        let reused_word = reused.cons_word_for_test();
        if a_bounds.2 >= 1 {
            assert_eq!(
                reused_word,
                a_root.cons_word_for_test() + std::mem::size_of::<crate::tagged::gc::ConsCell>(),
                "the closed region's suffix serves the next local carve"
            );
        }
        // Whatever the fragment geometry, the next local carve never serves
        // a cell inside B's live exclusive region.
        assert!(reused_word < b_bounds.0 || reused_word >= b_bounds.1);
        a.close_cons_region().unwrap();
        b.close_cons_region().unwrap();
        assert!(a.cons_region_for_test().is_none() && b.cons_region_for_test().is_none());
        drop((a, b));
        let mut first_roots = seeds;
        first_roots.extend([a_root.clone(), reused.clone()]);
        first.enter_native(&first_roots).unwrap();
        second.enter_native(std::slice::from_ref(&b_root)).unwrap();
        let acked = era.cons_catalog_snapshot_for_test().unwrap();
        assert_eq!(acked.active_regions, 0, "both ACKs published every tail");
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| {
            check_catalog(heap);
            check_pair(heap, &a_root, 1701);
            check_pair(heap, &b_root, 1702);
            check_pair(heap, &reused, 1701);
        })
        .unwrap();
        complete(stopped);
        first.leave_native().unwrap();
        second.leave_native().unwrap();
        drop((first_roots, a_root, b_root, reused));
        retire(first);
        retire(second);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn return_cons_storage_waits_out_a_brief_gate_holder<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1718, 0).unwrap(), era.heap_identity(), 1718);
        let root = with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
        owner.enter_native(std::slice::from_ref(&root)).unwrap();
        let mut stopped = era.stop().unwrap();
        // The keeper's stop check waits out a brief gate holder instead of
        // refusing Busy (L6.7 F3). The holder lets go once the keeper has met
        // the held gate, or once the call has returned without meeting it.
        // The return completes only after the holder lets go.
        let released = std::sync::atomic::AtomicBool::new(false);
        thread::scope(|scope| {
            use super::test_fences::gate_contention_for_test;
            let registry = era.registry();
            let released = &released;
            let (held_tx, held_rx) = mpsc::channel();
            let (returned_tx, returned_rx) = mpsc::channel::<()>();
            let before = gate_contention_for_test();
            let actor = scope.spawn(move || {
                let gate = registry.gate.state.lock().unwrap();
                held_tx.send(()).unwrap();
                while gate_contention_for_test() == before && returned_rx.try_recv().is_err() {
                    thread::yield_now();
                }
                released.store(true, Ordering::Release);
                drop(gate);
            });
            receive(&held_rx);
            let returned = era.return_cons_storage(&stopped);
            let returned_after_release = released.load(Ordering::Acquire);
            // Release BEFORE assertions so a failure never strands the join.
            let _ = returned_tx.send(());
            actor.join().unwrap();
            assert!(
                gate_contention_for_test() > before,
                "the return met the held gate"
            );
            assert!(
                returned_after_release,
                "the return completed only after the holder let go"
            );
            assert_eq!(returned, Ok(()), "the stop check waits out the holder");
            assert!(!registry.park.poisoned.load(Ordering::Acquire));
        });
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| check_pair(heap, &root, 1701))
            .unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        drop(root);
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// collect's reader scan runs outside the gate (root-table locks never
/// overlap it). An owner's gate-and-entry moment there is waited out, not
/// returned as Busy (L6.7 F3).
fn collect_waits_out_an_owner_moment_in_its_reader_scan<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1719, 0).unwrap(), era.heap_identity(), 1719);
        let root = with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
        owner.enter_native(std::slice::from_ref(&root)).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        let entry = owner.entry();
        let collected = thread::scope(|scope| {
            use super::test_fences::{
                gate_contention_for_test, install_owner_moment_hook_for_test,
            };
            let registry = era.registry();
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (held_tx, held_rx) = mpsc::channel::<()>();
            let (returned_tx, returned_rx) = mpsc::channel::<()>();
            let before = gate_contention_for_test();
            // An owner's moment: the gate, then its entry, exactly when the
            // keeper starts the scan with the gate released.
            let holder = scope.spawn(move || {
                receive(&go_rx);
                let gate = registry.gate.state.lock().unwrap();
                let state = entry.state.lock().unwrap();
                held_tx.send(()).unwrap();
                while gate_contention_for_test() == before && returned_rx.try_recv().is_err() {
                    thread::yield_now();
                }
                drop(state);
                drop(gate);
            });
            install_owner_moment_hook_for_test(Box::new(move || {
                go_tx.send(()).unwrap();
                receive(&held_rx);
            }));
            let collected = era.collect_stopped(&mut stopped, world_roots);
            // Release BEFORE assertions so a failure never strands the join.
            let _ = returned_tx.send(());
            holder.join().unwrap();
            assert!(
                gate_contention_for_test() > before,
                "the reader scan met the owner's moment"
            );
            collected
        });
        collected.expect("collect waits out the owner's moment");
        era.with_stopped_heap_for_test(&stopped, |heap| check_pair(heap, &root, 1701))
            .unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        drop(root);
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn forgotten_open_cons_run_refuses_publication_native_and_finish_without_waiting<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1719, 0).unwrap(), era.heap_identity(), 1719);
        let mut run = owner.run_access().unwrap();
        let root = run.cons(&cars[0], &nil).unwrap();
        assert!(run.cons_region_for_test().is_some());
        // Unlike the Drop regression, no destructor can publish poison or
        // return this grant. Only the canonical live-loan ledger remains.
        std::mem::forget(run);
        assert!(owner.entry.admitted());
        assert!(!era.registry().park.poisoned.load(Ordering::Acquire));
        let before = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(before.installed && before.metadata_coherent);
        // One lent tail (or a multi-run batch) stays charged until the owner
        // publishes; the forgotten run loan blocks that publication.
        assert!(before.active_regions >= 1);
        let epoch = std::num::NonZeroU64::MIN;
        assert_eq!(
            owner.begin_publication(epoch).unwrap().publish(),
            Err(RegistrationError::OutstandingLoan)
        );
        assert_eq!(
            owner.enter_native(std::slice::from_ref(&root)),
            Err(RendezvousError::Loan)
        );
        let failure = owner.finish(epoch).unwrap_err();
        assert_eq!(failure.reason, RegistrationError::OutstandingLoan);
        let owner = failure.into_owner();
        let after = era.cons_catalog_snapshot_for_test().unwrap();
        assert_eq!(
            after, before,
            "refusals preserve the sole grant and backing"
        );
        // Never call stop: this forgotten owner can no longer acknowledge.
        // Forget its wrapper, then exercise the actual era failure retention.
        std::mem::forget(owner);
        let failure = era.finish().unwrap_err();
        assert_eq!(failure.reason, RegistrationError::OpenRegion);
        drop(failure);
        drop(root);
    });
}

fn foreign_stop_cannot_return_or_resume_a_cons_catalog<K, W>(
    empty_outer_era: &mut dyn FnMut(i64, &mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>)),
    roots_nested_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    empty_outer_era(0, &mut |foreign| {
        let mut foreign_stop = foreign.stop().unwrap();
        roots_nested_era(&mut |era, (cars, nil)| {
            era.start_cons_storage().unwrap();
            let before = era.cons_catalog_snapshot_for_test().unwrap();
            assert_eq!(
                era.return_cons_storage(&foreign_stop),
                Err(RegistrationError::WrongHeap)
            );
            assert_eq!(
                era.resume_cons_storage(&foreign_stop),
                Err(RegistrationError::WrongHeap)
            );
            let after = era.cons_catalog_snapshot_for_test().unwrap();
            assert_eq!(after.blocks, before.blocks);
            assert_eq!(after.live_count, before.live_count);
            assert_eq!(after.free_count, before.free_count);
            assert!(after.installed && after.metadata_coherent);
            // A real allocation and matching stop still succeed after
            // refusal, on the World whose frozen census was captured last.
            let mut owner = activate(era.register(1717, 0).unwrap(), era.heap_identity(), 1717);
            let root = with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
            owner.enter_native(std::slice::from_ref(&root)).unwrap();
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            era.collect_stopped(&mut stopped, world_roots).unwrap();
            era.with_stopped_heap_for_test(&stopped, |heap| {
                check_catalog(heap);
                check_pair(heap, &root, 1701);
            })
            .unwrap();
            complete(stopped);
            owner.leave_native().unwrap();
            drop(root);
            retire(owner);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        });

        foreign_stop.trace(|_, _| Ok(())).unwrap();
        complete(foreign_stop);
        let (collector, keeper) = foreign.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn cons_result_survives_from_published_head_then_reclaims_after_serial_restore<K, W>(
    roots_era_then_words: &mut dyn FnMut(
        &mut dyn for<'e> FnMut(
            ParallelHeapEra<'e, K>,
            RootsFixture,
        ) -> (QuiescentCollector<'e>, K, [usize; 2]),
    ),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era_then_words(&mut |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1710, 1).unwrap(), era.heap_identity(), 1710);
        let (head, words) = with_run(&mut owner, |run| {
            let child = run.cons(&cars[0], &nil).unwrap();
            let head = run.cons(&child, &nil).unwrap();
            let words = [head.cons_word_for_test(), child.cons_word_for_test()];
            drop(child); // only the head's car retains the child
            (head, words)
        });
        owner.enter_native(std::slice::from_ref(&head)).unwrap();
        drop(head); // only the authoritative publication retains the graph
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| {
            for word in words {
                assert!(heap.owns_heap_value_for_test(unsafe { Value::from_bits(word) }));
            }
            // SAFETY: exact stopped publication retains the complete graph.
            unsafe {
                let head = &*Value::from_bits(words[0]).as_cons_ptr().unwrap();
                assert_eq!(head.car.bits(), words[1]);
                let child = &*head.car.as_cons_ptr().unwrap();
                assert_eq!(child.car.as_fixnum(), Some(1701));
            }
        })
        .unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        with_run(&mut owner, |run| run.state().root_scratch.clear());
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        (collector, keeper, words)
    });
}

fn partial_cons_grant_refunds_owner_and_global_credits_then_merges_history_once<K, W>(
    partial_era_then: &mut dyn FnMut(
        &mut dyn for<'e> FnMut(
            ParallelHeapEra<'e, K>,
            CreditsFixture,
        ) -> (QuiescentCollector<'e>, K, CreditsResult),
    ),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::{ConsCell, MemoryUseCountSlot};
    partial_era_then(
        &mut |era, ((cars, nil), base_live, base_memory, base_bytes)| {
            let table_charge = era.symbol_table_admission_charge_for_test();
            assert!(
                table_charge.get() > 0,
                "admission retains the published table"
            );
            era.start_cons_storage().unwrap();
            let before = era.cons_catalog_snapshot_for_test().unwrap();
            assert_eq!(before.live_count, base_live);
            let mut owner = activate(era.register(1720, 0).unwrap(), era.heap_identity(), 1720);
            let mut run = owner.run_access().unwrap();
            assert_eq!(
                table_charge,
                run.initial_table_statistics_for_test()
                    .unwrap()
                    .retained_bytes,
                "admission charged exactly the published table's retained storage"
            );
            let mut allocated = Vec::new();
            // Bootstrap free lists may offer one-cell runs. Keep actual cells
            // rooted until a genuine partial region is observed; never inject
            // a fake charge or assume an allocator source/layout.
            let unused = loop {
                assert!(allocated.len() < 100_000, "bounded real partial grant");
                allocated.push(run.cons(&cars[0], &nil).unwrap());
                let unused = run.cons_region_for_test().unwrap().2;
                if unused != 0 {
                    break unused;
                }
            };
            let handed = allocated.len();
            let charged = handed + unused;
            let slot = MemoryUseCountSlot::ConsCells.index();
            {
                let hot = &run.state().hot;
                assert_eq!(hot.allocated_count, charged);
                assert_eq!(hot.bytes_since_gc, charged * size_of::<ConsCell>());
                assert_eq!(hot.memory_use_counts[slot], charged as u64);
                assert_eq!(hot.bytes_banked_at_resets, 0);
                assert_eq!(hot.pacing, Default::default());
            }
            let during = era.cons_catalog_snapshot_for_test().unwrap();
            // The whole lent tail batch is charged globally until the ACK.
            assert!(during.active_regions >= 1);
            assert!(during.live_count >= base_live + charged);
            run.close_cons_region().unwrap();
            {
                let hot = &run.state().hot;
                assert_eq!(hot.allocated_count, handed);
                assert_eq!(hot.bytes_since_gc, handed * size_of::<ConsCell>());
                assert_eq!(hot.memory_use_counts[slot], handed as u64);
                assert_eq!(hot.bytes_banked_at_resets, 0);
                assert_eq!(hot.pacing, Default::default());
            }
            assert!(run.cons_region_for_test().is_none());
            let closed = era.cons_catalog_snapshot_for_test().unwrap();
            // A local close moves no global credit: the suffix stays in the
            // owner's exclusive tail until publication.
            assert!(closed.active_regions >= 1);
            assert_eq!(closed.live_count, during.live_count);
            drop(run);
            let mut expected_memory = base_memory;
            expected_memory[slot] = expected_memory[slot].wrapping_add(handed as u64);
            // The cons owner refunds only its unused region. Owner 0 also
            // retains admission's separately charged immutable pointer table;
            // both histories merge into the stopped cumulative byte total.
            let expected_bytes = base_bytes
                + (handed * size_of::<ConsCell>()) as u64
                + u64::try_from(table_charge.get()).unwrap();
            owner.enter_native(&allocated).unwrap();
            let acked = era.cons_catalog_snapshot_for_test().unwrap();
            assert_eq!(acked.active_regions, 0, "the ACK published the tail");
            assert_eq!(
                acked.live_count,
                base_live + handed,
                "publication refunds exactly the unhanded suffix"
            );
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            let report = era.collect_stopped(&mut stopped, world_roots).unwrap();
            assert_eq!(report.memory_use_counts, expected_memory);
            assert_eq!(report.total_allocated_bytes, expected_bytes);
            complete(stopped);
            owner.leave_native().unwrap();
            with_run(&mut owner, |run| {
                let hot = &run.state().hot;
                assert_eq!(hot.allocated_count, 0, "GC rebases live balance to owner 0");
                assert_eq!(hot.bytes_since_gc, 0);
                assert_eq!(
                    hot.bytes_banked_at_resets,
                    (handed * size_of::<ConsCell>()) as u64
                );
                assert_eq!(hot.memory_use_counts[slot], handed as u64);
            });
            drop(allocated);
            retire(owner);
            let (collector, keeper) = era.finish().unwrap();
            (
                collector,
                keeper,
                (report.allocated_count, expected_memory, expected_bytes),
            )
        },
    );
}

fn two_owner_numeric_allocations_survive_interleaved_catalog_work<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::{MemoryUseCountSlot, NumericAllocError, NumericClass, OwnerRootSlot};
    use crate::tagged::header::{BignumObj, FloatObj};
    use crate::value::HashTableTest;
    use malachite::integer::Integer;
    use std::mem::size_of;

    fn counters(run: &mut RunAccess<'_, '_>) -> (usize, usize, [u64; 7]) {
        let hot = &run.state().hot;
        (
            hot.allocated_count,
            hot.bytes_since_gc,
            hot.memory_use_counts,
        )
    }

    fn assert_charge(
        run: &mut RunAccess<'_, '_>,
        before: (usize, usize, [u64; 7]),
        bytes: usize,
        float: bool,
    ) {
        let mut expected_memory = before.2;
        if float {
            let slot = MemoryUseCountSlot::Floats.index();
            expected_memory[slot] = expected_memory[slot].wrapping_add(1);
        }
        assert_eq!(
            counters(run),
            (before.0 + 1, before.1 + bytes, expected_memory)
        );
    }

    fn prepare<'era>(owner: &mut Mutator<'era>, nil: &SharedRoot) -> Vec<OwnerRootSlot<'era>> {
        let mut run = owner.run_access().unwrap();
        let mut homes = (0..8)
            .map(|_| run.root_slot(nil).unwrap())
            .collect::<Vec<_>>();
        let before = run.owner_gc_snapshot_for_test();
        assert_eq!(
            run.float_into(1.25, &mut homes[0]),
            Err(NumericAllocError::Exhausted(NumericClass::Float))
        );
        assert_eq!(run.owner_gc_snapshot_for_test(), before);
        assert_eq!(
            run.bignum_into(Integer::from(i64::MAX), &mut homes[1]),
            Err(NumericAllocError::Exhausted(NumericClass::Bignum))
        );
        assert_eq!(run.owner_gc_snapshot_for_test(), before);
        assert_eq!(homes[0].word_for_test(&run), Value::NIL.bits());
        assert_eq!(homes[1].word_for_test(&run), Value::NIL.bits());
        run.close_numeric_runs().unwrap();
        run.close_cons_region().unwrap();
        drop(run);
        homes
    }

    fn allocate_pair(
        run: &mut RunAccess<'_, '_>,
        homes: &mut [OwnerRootSlot<'_>],
        float_index: usize,
        bignum_index: usize,
        float: f64,
        bignum: i64,
    ) {
        let before_float = counters(run);
        run.float_into(float, &mut homes[float_index]).unwrap();
        assert_charge(run, before_float, size_of::<FloatObj>(), true);
        let before_bignum = counters(run);
        run.bignum_into(Integer::from(bignum), &mut homes[bignum_index])
            .unwrap();
        assert_charge(run, before_bignum, size_of::<BignumObj>(), false);
        assert!(!run.state().numeric.is_closed());
    }

    fn interleave_catalog_work(run: &mut RunAccess<'_, '_>, homes: &[OwnerRootSlot<'_>]) {
        assert!(!run.state().numeric.is_closed());
        // Cloning a handle aliases the same canonical census home; it creates
        // no additional root and avoids borrowing the homes slice twice.
        let mut cons_home = homes[2].clone();
        run.cons_into(&homes[0], &homes[1], &mut cons_home).unwrap();
        run.make_hash_table_into(HashTableTest::Eq, 4, &homes[3])
            .unwrap();
        assert!(!run.state().numeric.is_closed());
    }

    fn check_pair(
        run: &mut RunAccess<'_, '_>,
        homes: &[OwnerRootSlot<'_>],
        float_index: usize,
        bignum_index: usize,
        float: f64,
        bignum: i64,
    ) {
        let float_word = homes[float_index].word_for_test(run);
        let bignum_word = homes[bignum_index].word_for_test(run);
        // SAFETY: these fresh words came from this admitted owner's checked
        // canonical homes after restore. Collection cannot overlap the live
        // run, and the current detached catalog retains initialized immutable
        // numeric payloads. No raw Value or payload borrow leaves this helper.
        unsafe {
            let float_value = Value::from_bits(float_word);
            let bignum_value = Value::from_bits(bignum_word);
            assert_eq!(float_value.as_float(), Some(float));
            assert_eq!(
                bignum_value.as_bignum().unwrap().to_string(),
                bignum.to_string()
            );
        }
    }

    roots_era(&mut |era, (_cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner_a = activate(era.register(1740, 8).unwrap(), era.heap_identity(), 1740);
        let mut owner_b = activate(era.register(1741, 8).unwrap(), era.heap_identity(), 1741);
        let mut homes_a = prepare(&mut owner_a, &nil);
        let mut homes_b = prepare(&mut owner_b, &nil);

        // No pool existed on first entry. The ordinary stop/return/collection/
        // resume protocol consumes both classes' real exhaustion requests.
        owner_a.enter_native(&[]).unwrap();
        owner_b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        owner_a.leave_native().unwrap();
        owner_b.leave_native().unwrap();

        {
            // These are two distinct actual owner records. Keeping both run
            // borrows live exercises disjoint raw numeric page views while
            // ordinary cons/table work re-enters the shared catalog gate.
            let mut run_a = owner_a.run_access().unwrap();
            let mut run_b = owner_b.run_access().unwrap();
            allocate_pair(&mut run_a, &mut homes_a, 0, 1, 1.25, i64::MAX);
            allocate_pair(&mut run_b, &mut homes_b, 0, 1, 2.5, i64::MAX - 1);
            interleave_catalog_work(&mut run_a, &homes_a);
            interleave_catalog_work(&mut run_b, &homes_b);
            run_a.close_numeric_runs().unwrap();
            assert!(run_a.state().numeric.is_closed());
            assert!(!run_b.state().numeric.is_closed());
            // A's settlement must not invalidate B's still-live bitmap fields.
            allocate_pair(&mut run_b, &mut homes_b, 4, 5, 3.75, i64::MAX - 2);
            run_a.close_cons_region().unwrap();
            run_b.close_numeric_runs().unwrap();
            run_b.close_cons_region().unwrap();
        }

        // The result homes, not additional SharedRoots or raw Values, are the
        // sole roots for all six numeric objects and both cons/table pairs.
        owner_a.enter_native(&[]).unwrap();
        owner_b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        owner_a.leave_native().unwrap();
        owner_b.leave_native().unwrap();
        {
            let mut run = owner_a.run_access().unwrap();
            check_pair(&mut run, &homes_a, 0, 1, 1.25, i64::MAX);
            run.clear_root_slots().unwrap();
            run.close_numeric_runs().unwrap();
            run.close_cons_region().unwrap();
        }
        {
            let mut run = owner_b.run_access().unwrap();
            check_pair(&mut run, &homes_b, 0, 1, 2.5, i64::MAX - 1);
            check_pair(&mut run, &homes_b, 4, 5, 3.75, i64::MAX - 2);
            run.clear_root_slots().unwrap();
            run.close_numeric_runs().unwrap();
            run.close_cons_region().unwrap();
        }
        drop((homes_a, homes_b));

        // Verification resumed the catalog. A final ordinary cleanup stop
        // returns it and drains the now-empty owner censuses before finish.
        owner_a.enter_native(&[]).unwrap();
        owner_b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        complete(stopped);
        owner_a.leave_native().unwrap();
        owner_b.leave_native().unwrap();
        retire(owner_a);
        retire(owner_b);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn partial_numeric_page_reclaims_tail_without_stop<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::NumericAllocError;
    use crate::tagged::gc::NumericClass;
    use crate::tagged::gc::OwnerRootSlot;
    use crate::tagged::gc::arena_pages::PagedObject;
    use crate::tagged::header::FloatObj;

    fn allocate_and_close(
        owner: &mut Mutator<'_>,
        home: &mut OwnerRootSlot<'_>,
        number: f64,
    ) -> usize {
        let mut run = owner.run_access().unwrap();
        run.float_into(number, home).unwrap();
        // SAFETY: the checked canonical home belongs to this admitted run.
        // Only the address escapes, never a value or a payload reference.
        let address = unsafe {
            Value::from_bits(home.word_for_test(&run))
                .as_float_ptr()
                .unwrap() as usize
        };
        let charged = run.owner_gc_snapshot_for_test();
        run.close_all_runs().unwrap();
        assert_eq!(run.owner_gc_snapshot_for_test(), charged);
        run.close_all_runs().unwrap();
        assert_eq!(run.owner_gc_snapshot_for_test(), charged);
        address
    }

    roots_era(&mut |era, (_cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut a = activate(era.register(1750, 2).unwrap(), era.heap_identity(), 1750);
        let mut b = activate(era.register(1751, 1).unwrap(), era.heap_identity(), 1751);
        let (mut a0, mut a1) = {
            let mut run = a.run_access().unwrap();
            let mut first = run.root_slot(&nil).unwrap();
            let second = run.root_slot(&nil).unwrap();
            let before = run.owner_gc_snapshot_for_test();
            assert_eq!(
                run.float_into(1.25, &mut first),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            assert_eq!(run.owner_gc_snapshot_for_test(), before);
            run.close_all_runs().unwrap();
            (first, second)
        };
        let mut b0 = {
            let mut run = b.run_access().unwrap();
            let result = run.root_slot(&nil).unwrap();
            run.close_all_runs().unwrap();
            result
        };
        let requested = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(requested.pending_float_requests, 1);
        assert_eq!(requested.pending_bignum_requests, 0);
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();
        let minted = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(minted.pending_float_requests, 0);
        assert_eq!(minted.consumed_float_requests, 1);
        assert_ne!(minted.generation, 0);

        let first = allocate_and_close(&mut a, &mut a0, 1.25);
        let second = allocate_and_close(&mut b, &mut b0, 2.5);
        let third = allocate_and_close(&mut a, &mut a1, 3.75);
        assert_eq!(second, first + FloatObj::SLOT_BYTES);
        assert_eq!(third, second + FloatObj::SLOT_BYTES);
        let reused = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(reused.generation, minted.generation);
        assert_eq!(reused.pending_float_requests, 0);
        assert_eq!(reused.pending_bignum_requests, 0);
        assert_eq!(
            reused.current_float_slots, 3,
            "count only each claim's delta"
        );
        assert_eq!(reused.active_regions, 0);

        for owner in [&mut a, &mut b] {
            let mut run = owner.run_access().unwrap();
            run.clear_root_slots().unwrap();
            run.close_all_runs().unwrap();
        }
        drop((a0, a1, b0));
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();
        retire(a);
        retire(b);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn numeric_mint_receipt_survives_failed_outer_finish<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::arena_pages::PagedObject;
    use crate::tagged::gc::{NumericAllocError, NumericClass};
    use crate::tagged::header::FloatObj;

    let mut retained_pages = None;
    roots_era(&mut |era, (_cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut a = activate(era.register(1760, 1).unwrap(), era.heap_identity(), 1760);
        let mut b = activate(era.register(1761, 1).unwrap(), era.heap_identity(), 1761);
        for owner in [&mut a, &mut b] {
            let mut run = owner.run_access().unwrap();
            let mut result = run.root_slot(&nil).unwrap();
            assert_eq!(
                run.float_into(1.0, &mut result),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            run.close_all_runs().unwrap();
        }
        let pending = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(pending.pending_float_requests, 2);
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();

        // A real native actor changes only registry metadata after the bank's
        // successful internal commit. The unchanged outer finish must refuse.
        thread::scope(|scope| {
            let registry = era.registry();
            let (committed_tx, committed_rx) = mpsc::sync_channel(0);
            let (poisoned_tx, poisoned_rx) = mpsc::sync_channel(0);
            let actor = scope.spawn(move || {
                committed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                registry.poison();
                poisoned_tx.send(()).unwrap();
            });
            era.registry()
                .cons_catalog
                .observe_next_numeric_commit_for_test(Box::new(move || {
                    committed_tx.send(()).unwrap();
                    poisoned_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                }))
                .unwrap();
            assert_eq!(
                era.resume_cons_storage(&stopped),
                Err(RegistrationError::Poisoned)
            );
            actor.join().unwrap();
        });
        let retained = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert!(retained.installed);
        assert_ne!(retained.generation, 0);
        assert_eq!(retained.pending_float_requests, 0);
        assert_eq!(
            retained.consumed_float_requests,
            pending.pending_float_requests
        );
        assert_eq!(
            retained.consumed_bignum_requests,
            pending.pending_bignum_requests
        );
        assert_eq!(retained.active_regions, 0);
        assert!(matches!(a.run_access(), Err(RegistrationError::Poisoned)));
        let pages = FloatObj::live_page_counter().load(Ordering::Acquire);
        assert!(pages >= 2);
        drop(stopped);
        drop(a);
        drop(b);
        let failure = era.finish().unwrap_err();
        drop(failure);
        assert_eq!(FloatObj::live_page_counter().load(Ordering::Acquire), pages);
        retained_pages = Some(pages);
    });
    // The core fixture's serial Context has now dropped too. Terminal retention
    // still owns the actual minted page storage, not just a copied receipt.
    assert_eq!(
        FloatObj::live_page_counter().load(Ordering::Acquire),
        retained_pages.unwrap()
    );
}

fn numeric_partial_pool_survives_refill_and_swept_prefix_holes<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::arena_pages::PagedObject;
    use crate::tagged::gc::{NumericAllocError, NumericClass};
    use crate::tagged::header::FloatObj;

    roots_era(&mut |era, (_cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut a = activate(era.register(1770, 5).unwrap(), era.heap_identity(), 1770);
        let mut b = activate(era.register(1771, 1).unwrap(), era.heap_identity(), 1771);
        let (mut a0, mut a1, mut a2, mut a3, mut a4) = {
            let mut run = a.run_access().unwrap();
            let mut first = run.root_slot(&nil).unwrap();
            let second = run.root_slot(&nil).unwrap();
            let third = run.root_slot(&nil).unwrap();
            assert_eq!(
                run.float_into(1.0, &mut first),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            run.close_all_runs().unwrap();
            let fourth = run.root_slot(&nil).unwrap();
            let fifth = run.root_slot(&nil).unwrap();
            (first, second, third, fourth, fifth)
        };
        let mut b0 = {
            let mut run = b.run_access().unwrap();
            let home = run.root_slot(&nil).unwrap();
            run.close_all_runs().unwrap();
            home
        };
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();

        let (live_word, higher_live_word, dead_word, first_address) = {
            let mut arun = a.run_access().unwrap();
            let mut brun = b.run_access().unwrap();
            arun.float_into(1.25, &mut a0).unwrap();
            arun.float_into(2.5, &mut a1).unwrap();
            arun.float_into(3.5, &mut a2).unwrap();
            arun.float_into(4.5, &mut a3).unwrap();
            let live_word = a0.word_for_test(&arun);
            let higher_live_word = a2.word_for_test(&arun);
            let dead_word = a1.word_for_test(&arun);
            // SAFETY: this scoped admitted read only copies the initialized
            // float address from its canonical home; no value borrow escapes.
            let address = unsafe { Value::from_bits(live_word).as_float_ptr().unwrap() as usize };
            arun.clear_owner_root_for_test(&a1).unwrap();
            arun.clear_owner_root_for_test(&a3).unwrap();
            // A still holds its partial run. B records the additional demand.
            assert_eq!(
                brun.float_into(3.75, &mut b0),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            arun.close_all_runs().unwrap();
            brun.close_all_runs().unwrap();
            (live_word, higher_live_word, dead_word, address)
        };
        assert_eq!(
            era.registry()
                .cons_catalog
                .numeric_catalog_snapshot_for_test()
                .pending_float_requests,
            1
        );
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| {
            // SAFETY: ownership probes are bit-first on this stopped heap.
            // They never interpret the dead clear-bit slot as a payload.
            unsafe {
                assert!(heap.owns_heap_value_for_test(Value::from_bits(live_word)));
                assert!(heap.owns_heap_value_for_test(Value::from_bits(higher_live_word)));
                assert!(!heap.owns_heap_value_for_test(Value::from_bits(dead_word)));
            }
        })
        .unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();
        {
            let mut arun = a.run_access().unwrap();
            let mut brun = b.run_access().unwrap();
            arun.float_into(5.5, &mut a4).unwrap();
            // Both claims stay live: sequentially reusing a single page would
            // conceal the additional-demand progress requirement.
            brun.float_into(5.75, &mut b0).unwrap();
            // SAFETY: both words are checked canonical homes of live runs.
            let after = unsafe {
                Value::from_bits(a4.word_for_test(&arun))
                    .as_float_ptr()
                    .unwrap() as usize
            };
            // Highest live slot is two. Reclaim the dead tail at three,
            // while the genuine interior hole at one stays serial-only.
            assert_eq!(after, first_address + 3 * FloatObj::SLOT_BYTES);
            assert_ne!(
                after,
                first_address + FloatObj::SLOT_BYTES,
                "a swept prefix hole is never lent to an owner"
            );
            arun.close_all_runs().unwrap();
            brun.close_all_runs().unwrap();
        }
        let resumed = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(resumed.pending_float_requests, 0);
        assert_eq!(resumed.current_float_slots, 2);
        for owner in [&mut a, &mut b] {
            let mut run = owner.run_access().unwrap();
            run.clear_root_slots().unwrap();
            run.close_all_runs().unwrap();
        }
        drop((a0, a1, a2, a3, a4, b0));
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();
        retire(a);
        retire(b);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn numeric_refusals_preserve_destinations_counters_and_requests<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::{NumericAllocError, NumericClass};
    use malachite::integer::Integer;

    let foreign_heap = TaggedHeap::new();
    // SAFETY: NIL is immediate and live; the real heap gives this destination
    // a foreign brand without constructing or decoding a managed value.
    let mut foreign = unsafe { SharedRoot::new(&foreign_heap, Value::NIL) };
    roots_era(&mut |era, (_cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut a = activate(era.register(1780, 2).unwrap(), era.heap_identity(), 1780);
        let mut b = activate(era.register(1781, 1).unwrap(), era.heap_identity(), 1781);
        let mut wrong_owner = {
            let mut run = b.run_access().unwrap();
            let home = run.root_slot(&nil).unwrap();
            run.close_all_runs().unwrap();
            home
        };
        let (mut home, mut big_home) = {
            let mut run = a.run_access().unwrap();
            let mut stale = run.root_slot(&nil).unwrap();
            run.clear_root_slots().unwrap();
            let mut home = run.root_slot(&nil).unwrap();
            let mut big_home = run.root_slot(&nil).unwrap();
            macro_rules! refused {
                ($call:expr, $reason:expr) => {{
                    let before = run.owner_gc_snapshot_for_test();
                    let catalog = era
                        .registry()
                        .cons_catalog
                        .numeric_catalog_snapshot_for_test();
                    assert_eq!($call, Err(NumericAllocError::Registration($reason)));
                    assert_eq!(run.owner_gc_snapshot_for_test(), before);
                    assert!(run.state().numeric.is_closed());
                    assert_eq!(
                        era.registry()
                            .cons_catalog
                            .numeric_catalog_snapshot_for_test(),
                        catalog
                    );
                }};
            }
            // Each destination check must precede even a class request. The
            // pool is deliberately unprepared throughout these refusals.
            refused!(
                run.float_into(1.25, &mut foreign),
                RegistrationError::WrongHeap
            );
            refused!(
                run.bignum_into(Integer::from(i64::MAX), &mut foreign),
                RegistrationError::WrongHeap
            );
            refused!(
                run.float_into(1.25, &mut wrong_owner),
                RegistrationError::WrongOwner
            );
            refused!(
                run.bignum_into(Integer::from(i64::MAX), &mut wrong_owner),
                RegistrationError::WrongOwner
            );
            refused!(
                run.float_into(1.25, &mut stale),
                RegistrationError::StaleRegistration
            );
            refused!(
                run.bignum_into(Integer::from(i64::MAX), &mut stale),
                RegistrationError::StaleRegistration
            );
            assert_eq!(foreign.cons_word_for_test(), Value::NIL.bits());
            assert_eq!(home.word_for_test(&run), Value::NIL.bits());
            assert_eq!(big_home.word_for_test(&run), Value::NIL.bits());
            let original = (
                run.state().hot.allocated_count,
                run.state().hot.bytes_since_gc,
            );
            for (objects, bytes) in [(usize::MAX, original.1), (original.0, usize::MAX)] {
                // The existing private owner state is the overflow fence; no
                // shipping setter or alternate allocation path is introduced.
                run.state().hot.allocated_count = objects;
                run.state().hot.bytes_since_gc = bytes;
                refused!(run.float_into(1.25, &mut home), RegistrationError::Capacity);
                refused!(
                    run.bignum_into(Integer::from(i64::MAX), &mut big_home),
                    RegistrationError::Capacity
                );
                run.state().hot.allocated_count = original.0;
                run.state().hot.bytes_since_gc = original.1;
            }
            let before = run.owner_gc_snapshot_for_test();
            assert_eq!(
                run.float_into(1.25, &mut home),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            assert_eq!(
                run.bignum_into(Integer::from(i64::MAX), &mut big_home),
                Err(NumericAllocError::Exhausted(NumericClass::Bignum))
            );
            assert_eq!(run.owner_gc_snapshot_for_test(), before);
            run.close_all_runs().unwrap();
            (home, big_home)
        };
        let requested = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(requested.pending_float_requests, 1);
        assert_eq!(requested.pending_bignum_requests, 1);
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();
        {
            let mut run = a.run_access().unwrap();
            run.float_into(1.25, &mut home).unwrap();
            run.bignum_into(Integer::from(i64::MAX), &mut big_home)
                .unwrap();
            assert_ne!(home.word_for_test(&run), Value::NIL.bits());
            assert_ne!(big_home.word_for_test(&run), Value::NIL.bits());
            run.close_all_runs().unwrap();
            run.clear_root_slots().unwrap();
        }
        {
            let mut run = b.run_access().unwrap();
            assert_eq!(wrong_owner.word_for_test(&run), Value::NIL.bits());
            run.clear_root_slots().unwrap();
            run.close_all_runs().unwrap();
        }
        drop((home, big_home, wrong_owner));
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();
        retire(a);
        retire(b);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[derive(Clone, Copy)]
enum NumericTerminalMode {
    Drop,
    Forget,
    PoisonClose,
}

fn dropping_open_numeric_run_poison_retains_backing<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    numeric_terminal_run_obligation(roots_era, world_roots, NumericTerminalMode::Drop);
}

fn forgotten_open_numeric_run_refuses_publication_native_and_finish<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    numeric_terminal_run_obligation(roots_era, world_roots, NumericTerminalMode::Forget);
}

fn poisoned_numeric_close_retains_runs_and_counters_on_repeat<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    numeric_terminal_run_obligation(roots_era, world_roots, NumericTerminalMode::PoisonClose);
}

fn numeric_terminal_run_obligation<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
    mode: NumericTerminalMode,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::arena_pages::PagedObject;
    use crate::tagged::gc::{NumericAllocError, NumericClass};
    use crate::tagged::header::{BignumObj, FloatObj};
    use malachite::integer::Integer;

    // These are native terminal-retention rows. The intentionally abandoned
    // bank must survive even the serial fixture's Context destructor.
    let mut retained_pages = None;
    roots_era(&mut |era, (_cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1790, 2).unwrap(), era.heap_identity(), 1790);
        let (mut float_home, mut big_home) = {
            let mut run = owner.run_access().unwrap();
            let mut float_home = run.root_slot(&nil).unwrap();
            let mut big_home = run.root_slot(&nil).unwrap();
            assert_eq!(
                run.float_into(1.25, &mut float_home),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            assert_eq!(
                run.bignum_into(Integer::from(i64::MAX), &mut big_home),
                Err(NumericAllocError::Exhausted(NumericClass::Bignum))
            );
            run.close_all_runs().unwrap();
            (float_home, big_home)
        };
        owner.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        drop((float_home, big_home));
        retire(owner);
        // A fresh owner claims the unkeyed pool minted above. Unlike the
        // requester, it has never participated in an A5 stop, so the legacy
        // publication probe reaches the loan fence instead of the earlier
        // protocol-participation Busy fence.
        let mut owner = activate(era.register(1791, 2).unwrap(), era.heap_identity(), 1791);
        let (mut float_home, mut big_home) = {
            let mut run = owner.run_access().unwrap();
            let homes = (run.root_slot(&nil).unwrap(), run.root_slot(&nil).unwrap());
            run.close_all_runs().unwrap();
            homes
        };
        let mut run = owner.run_access().unwrap();
        run.float_into(1.25, &mut float_home).unwrap();
        run.bignum_into(Integer::from(i64::MAX), &mut big_home)
            .unwrap();
        assert!(!run.state().numeric.is_closed());
        assert!(
            run.cons_region_for_test().is_none(),
            "only numeric books can block this publication"
        );
        assert!(matches!(
            run.state().validate_publication(),
            Err(super::image::ImageError::RegionsOpen)
        ));
        let before = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(before.active_regions, 2);
        let pages = (
            FloatObj::live_page_counter().load(Ordering::Acquire),
            BignumObj::live_page_counter().load(Ordering::Acquire),
        );
        let epoch = std::num::NonZeroU64::MIN;
        if matches!(mode, NumericTerminalMode::Forget) {
            std::mem::forget(run);
            assert!(owner.entry.admitted());
            assert!(!era.registry().park.poisoned.load(Ordering::Acquire));
            assert_eq!(
                owner.begin_publication(epoch).unwrap().publish(),
                Err(RegistrationError::OutstandingLoan)
            );
            assert_eq!(owner.enter_native(&[]), Err(RendezvousError::Loan));
            let failure = owner.finish(epoch).unwrap_err();
            assert_eq!(failure.reason, RegistrationError::OutstandingLoan);
            let owner = failure.into_owner();
            assert_eq!(
                era.registry()
                    .cons_catalog
                    .numeric_catalog_snapshot_for_test(),
                before
            );
            // No matching stop is possible after forgetting the admitted run.
            // Forget this wrapper too; the actual era finish retains authority.
            std::mem::forget(owner);
        } else {
            if matches!(mode, NumericTerminalMode::PoisonClose) {
                era.registry().poison();
                let hot = run.owner_gc_snapshot_for_test();
                for _ in 0..2 {
                    assert_eq!(run.close_numeric_runs(), Err(RegistrationError::Poisoned));
                    assert_eq!(run.owner_gc_snapshot_for_test(), hot);
                    assert!(!run.state().numeric.is_closed());
                    assert_eq!(
                        era.registry()
                            .cons_catalog
                            .numeric_catalog_snapshot_for_test(),
                        before
                    );
                }
            }
            drop(run); // deliberately do not settle either numeric class
            assert!(owner.entry.admitted());
            assert!(era.registry().park.poisoned.load(Ordering::Acquire));
            assert_eq!(owner.enter_native(&[]), Err(RendezvousError::Poisoned));
            assert!(matches!(
                owner.begin_publication(epoch),
                Err(RegistrationError::Poisoned)
            ));
            assert_eq!(
                era.registry()
                    .cons_catalog
                    .numeric_catalog_snapshot_for_test(),
                before
            );
            drop(owner);
        }
        drop((float_home, big_home));
        let failure = era.finish().unwrap_err();
        assert_eq!(failure.reason, RegistrationError::OpenRegion);
        drop(failure);
        assert_eq!(
            (
                FloatObj::live_page_counter().load(Ordering::Acquire),
                BignumObj::live_page_counter().load(Ordering::Acquire)
            ),
            pages
        );
        retained_pages = Some(pages);
    });
    assert_eq!(
        (
            FloatObj::live_page_counter().load(Ordering::Acquire),
            BignumObj::live_page_counter().load(Ordering::Acquire)
        ),
        retained_pages.unwrap()
    );
}

fn numeric_exhaustion_requests_are_once_per_owner_class_and_generation<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::arena_pages::ObjectPage;
    use crate::tagged::gc::{NumericAllocError, NumericClass, OwnerRootSlot};
    use crate::tagged::header::{BignumObj, FloatObj};
    use malachite::integer::Integer;

    fn repeat_empty(
        owner: &mut Mutator<'_>,
        float: &mut OwnerRootSlot<'_>,
        bignum: &mut OwnerRootSlot<'_>,
    ) {
        let mut run = owner.run_access().unwrap();
        let before = run.owner_gc_snapshot_for_test();
        for _ in 0..7 {
            assert_eq!(
                run.float_into(1.25, float),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            assert_eq!(
                run.bignum_into(Integer::from(i64::MAX), bignum),
                Err(NumericAllocError::Exhausted(NumericClass::Bignum))
            );
            assert_eq!(run.owner_gc_snapshot_for_test(), before);
            // Closing an empty book must not erase its outstanding request.
            run.close_all_runs().unwrap();
        }
    }

    roots_era(&mut |era, (_cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1796, 2).unwrap(), era.heap_identity(), 1796);
        let (mut float, mut bignum) = {
            let mut run = owner.run_access().unwrap();
            let pair = (run.root_slot(&nil).unwrap(), run.root_slot(&nil).unwrap());
            run.close_all_runs().unwrap();
            pair
        };
        repeat_empty(&mut owner, &mut float, &mut bignum);
        let requested = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(
            (
                requested.pending_float_requests,
                requested.pending_bignum_requests
            ),
            (1, 1)
        );
        assert_eq!(requested.generation, 0);

        // A native round trip has no refreeze and cannot rearm either class.
        owner.enter_native(&[]).unwrap();
        owner.leave_native().unwrap();
        repeat_empty(&mut owner, &mut float, &mut bignum);
        let same_epoch = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(
            (
                same_epoch.pending_float_requests,
                same_epoch.pending_bignum_requests
            ),
            (1, 1)
        );
        let mut previous_generation = 0;

        // The first measured interval is empty, so the first mint is one
        // page/class. Filling it sizes the next mint to two pages/class.
        // Exhaust each real pool, then prove the old owner rearms exactly once
        // in its new generation, rather than retaining a permanent requested bit.
        for pages in [1, 2] {
            owner.enter_native(&[]).unwrap();
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            era.collect_stopped(&mut stopped, world_roots).unwrap();
            era.resume_cons_storage(&stopped).unwrap();
            complete(stopped);
            owner.leave_native().unwrap();
            let installed = era
                .registry()
                .cons_catalog
                .numeric_catalog_snapshot_for_test();
            assert!(installed.generation > previous_generation);
            previous_generation = installed.generation;
            assert_eq!(
                (
                    installed.pending_float_requests,
                    installed.pending_bignum_requests
                ),
                (0, 0)
            );
            assert_eq!(
                (
                    installed.consumed_float_requests,
                    installed.consumed_bignum_requests
                ),
                (1, 1)
            );
            {
                let mut run = owner.run_access().unwrap();
                for index in 0..pages * ObjectPage::<FloatObj>::SLOTS {
                    run.float_into(index as f64, &mut float).unwrap();
                }
                for _ in 0..pages * ObjectPage::<BignumObj>::SLOTS {
                    run.bignum_into(Integer::from(i64::MAX), &mut bignum)
                        .unwrap();
                }
                run.close_all_runs().unwrap();
            }
            repeat_empty(&mut owner, &mut float, &mut bignum);
            let rearmed = era
                .registry()
                .cons_catalog
                .numeric_catalog_snapshot_for_test();
            assert_eq!(rearmed.generation, previous_generation);
            assert_eq!(
                (
                    rearmed.pending_float_requests,
                    rearmed.pending_bignum_requests
                ),
                (1, 1)
            );
        }

        {
            let mut run = owner.run_access().unwrap();
            run.clear_root_slots().unwrap();
            run.close_all_runs().unwrap();
        }
        drop((float, bignum));
        owner.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn numeric_request_epoch_does_not_repeat_after_an_unobserved_pool<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::arena_pages::ObjectPage;
    use crate::tagged::gc::{NumericAllocError, NumericClass};
    use crate::tagged::header::{BignumObj, FloatObj};
    use malachite::integer::Integer;

    roots_era(&mut |era, (_cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut a = activate(era.register(1797, 2).unwrap(), era.heap_identity(), 1797);
        let mut b = activate(era.register(1798, 2).unwrap(), era.heap_identity(), 1798);
        let (mut a_float, mut a_bignum) = {
            let mut run = a.run_access().unwrap();
            let mut float = run.root_slot(&nil).unwrap();
            let mut bignum = run.root_slot(&nil).unwrap();
            assert_eq!(
                run.float_into(1.0, &mut float),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            assert_eq!(
                run.bignum_into(Integer::from(i64::MAX), &mut bignum),
                Err(NumericAllocError::Exhausted(NumericClass::Bignum))
            );
            run.close_all_runs().unwrap();
            (float, bignum)
        };
        let (mut b_float, mut b_bignum) = {
            let mut run = b.run_access().unwrap();
            let pair = (run.root_slot(&nil).unwrap(), run.root_slot(&nil).unwrap());
            run.close_all_runs().unwrap();
            pair
        };
        let pending = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(
            (
                pending.pending_float_requests,
                pending.pending_bignum_requests
            ),
            (1, 1)
        );
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        // A deliberately stays Native for the entire minted generation. Its
        // first pending facts must become stale without observing a new grant.
        b.leave_native().unwrap();
        {
            let mut run = b.run_access().unwrap();
            for index in 0..ObjectPage::<FloatObj>::SLOTS {
                run.float_into(index as f64, &mut b_float).unwrap();
            }
            for _ in 0..ObjectPage::<BignumObj>::SLOTS {
                run.bignum_into(Integer::from(i64::MAX), &mut b_bignum)
                    .unwrap();
            }
            // Stop exactly at each page end: an extra allocation attempt here
            // would create a real request and hide the no-pool transition.
            run.close_all_runs().unwrap();
            run.clear_root_slots().unwrap();
        }
        drop((b_float, b_bignum));
        let no_demand = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(
            (
                no_demand.pending_float_requests,
                no_demand.pending_bignum_requests
            ),
            (0, 0)
        );
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        // Every prior owner page was full, so no partial tail or pending
        // request can mint a pool. This is a new no-pool installation epoch.
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        {
            let mut run = a.run_access().unwrap();
            let before = run.owner_gc_snapshot_for_test();
            assert_eq!(
                run.float_into(2.0, &mut a_float),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            assert_eq!(
                run.bignum_into(Integer::from(i64::MAX), &mut a_bignum),
                Err(NumericAllocError::Exhausted(NumericClass::Bignum))
            );
            assert_eq!(run.owner_gc_snapshot_for_test(), before);
            run.close_all_runs().unwrap();
        }
        // One attempt per class is a passing control on the old implementation.
        // The naive Requested(0) fix instead suppresses both new requests here.
        let rearmed = era
            .registry()
            .cons_catalog
            .numeric_catalog_snapshot_for_test();
        assert_eq!(
            (
                rearmed.pending_float_requests,
                rearmed.pending_bignum_requests
            ),
            (1, 1)
        );
        {
            let mut run = a.run_access().unwrap();
            run.clear_root_slots().unwrap();
            run.close_all_runs().unwrap();
        }
        drop((a_float, a_bignum));
        a.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();
        retire(a);
        retire(b);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[derive(Clone, Copy)]
enum NumericSerialWindowCase {
    Append,
    InteriorHole,
    FullyDead,
}

fn numeric_stopped_serial_allocations_rebase_both_class_frontiers<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    numeric_serial_window_case(roots_era, world_roots, NumericSerialWindowCase::Append);
}

fn numeric_stopped_serial_allocations_fill_interior_holes_before_owner_tails<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    numeric_serial_window_case(
        roots_era,
        world_roots,
        NumericSerialWindowCase::InteriorHole,
    );
}

fn fully_dead_numeric_retained_pages_refreeze_with_full_capacity<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    numeric_serial_window_case(roots_era, world_roots, NumericSerialWindowCase::FullyDead);
}

fn numeric_serial_window_case<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
    case: NumericSerialWindowCase,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::gc::arena_pages::{OBJECT_PAGE_BYTES, ObjectPage, PagedObject};
    use crate::tagged::gc::{NumericAllocError, NumericClass};
    use crate::tagged::header::{BignumObj, FloatObj};
    use malachite::integer::Integer;

    roots_era(&mut |era, (_cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut a = activate(era.register(1800, 8).unwrap(), era.heap_identity(), 1800);
        let mut b = activate(era.register(1801, 2).unwrap(), era.heap_identity(), 1801);
        let mut a_homes = {
            let mut run = a.run_access().unwrap();
            let mut homes = (0..8)
                .map(|_| run.root_slot(&nil).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                run.float_into(1.0, &mut homes[0]),
                Err(NumericAllocError::Exhausted(NumericClass::Float))
            );
            assert_eq!(
                run.bignum_into(Integer::from(i64::MAX), &mut homes[1]),
                Err(NumericAllocError::Exhausted(NumericClass::Bignum))
            );
            run.close_all_runs().unwrap();
            homes
        };
        let mut b_homes = {
            let mut run = b.run_access().unwrap();
            let homes = (0..2)
                .map(|_| run.root_slot(&nil).unwrap())
                .collect::<Vec<_>>();
            run.close_all_runs().unwrap();
            homes
        };
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();

        let (first_float, first_bignum) = {
            let mut arun = a.run_access().unwrap();
            let mut brun = b.run_access().unwrap();
            let count = if matches!(case, NumericSerialWindowCase::Append) {
                1
            } else {
                4
            };
            for i in 0..count {
                arun.float_into(1.25 + i as f64, &mut a_homes[2 * i])
                    .unwrap();
                arun.bignum_into(Integer::from(i64::MAX - i as i64), &mut a_homes[2 * i + 1])
                    .unwrap();
            }
            // SAFETY: these words are checked canonical homes of this live
            // admitted run. Only integer addresses cross the stopped boundary.
            let addresses = unsafe {
                (
                    Value::from_bits(a_homes[0].word_for_test(&arun))
                        .as_float_ptr()
                        .unwrap() as usize,
                    Value::from_bits(a_homes[1].word_for_test(&arun))
                        .as_veclike_ptr()
                        .unwrap() as usize,
                )
            };
            match case {
                NumericSerialWindowCase::Append => {}
                NumericSerialWindowCase::InteriorHole => {
                    // Slots zero and two stay live. Slot one is a genuine
                    // interior hole; slot three is a reclaimable dead tail.
                    for i in [1, 3] {
                        arun.clear_owner_root_for_test(&a_homes[2 * i]).unwrap();
                        arun.clear_owner_root_for_test(&a_homes[2 * i + 1]).unwrap();
                    }
                }
                NumericSerialWindowCase::FullyDead => arun.clear_root_slots().unwrap(),
            }
            // B cannot claim while A holds either physical page. One stopped
            // refill must preserve A's page and add fresh demand for B.
            if !matches!(case, NumericSerialWindowCase::InteriorHole) {
                assert_eq!(
                    brun.float_into(20.0, &mut b_homes[0]),
                    Err(NumericAllocError::Exhausted(NumericClass::Float))
                );
                assert_eq!(
                    brun.bignum_into(Integer::from(i64::MAX - 20), &mut b_homes[1]),
                    Err(NumericAllocError::Exhausted(NumericClass::Bignum))
                );
            }
            arun.close_all_runs().unwrap();
            brun.close_all_runs().unwrap();
            addresses
        };
        if matches!(case, NumericSerialWindowCase::FullyDead) {
            // Recreate valid empty homes after clear_root_slots changed the
            // generation. The retired handles carry no payload authority.
            a_homes = {
                let mut run = a.run_access().unwrap();
                let homes = (0..8)
                    .map(|_| run.root_slot(&nil).unwrap())
                    .collect::<Vec<_>>();
                run.close_all_runs().unwrap();
                homes
            };
        }
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        if matches!(case, NumericSerialWindowCase::InteriorHole) {
            // First establish a real swept interior hole without minting an
            // empty B page that could become the serial partial-list head.
            era.resume_cons_storage(&stopped).unwrap();
            complete(stopped);
            a.leave_native().unwrap();
            b.leave_native().unwrap();
            {
                let mut arun = a.run_access().unwrap();
                let mut brun = b.run_access().unwrap();
                // Slot three was a dead tail; the owner may reclaim it. Slot
                // one remains a serial-only hole below the high live slot.
                arun.float_into(30.5, &mut a_homes[6]).unwrap();
                arun.bignum_into(Integer::from(i64::MAX - 30), &mut a_homes[7])
                    .unwrap();
                // SAFETY: current canonical homes of this admitted owner.
                unsafe {
                    assert_eq!(
                        Value::from_bits(a_homes[6].word_for_test(&arun))
                            .as_float_ptr()
                            .unwrap() as usize,
                        first_float + 3 * FloatObj::SLOT_BYTES
                    );
                    assert_eq!(
                        Value::from_bits(a_homes[7].word_for_test(&arun))
                            .as_veclike_ptr()
                            .unwrap() as usize,
                        first_bignum + 3 * BignumObj::SLOT_BYTES
                    );
                }
                assert_eq!(
                    brun.float_into(20.0, &mut b_homes[0]),
                    Err(NumericAllocError::Exhausted(NumericClass::Float))
                );
                assert_eq!(
                    brun.bignum_into(Integer::from(i64::MAX - 20), &mut b_homes[1]),
                    Err(NumericAllocError::Exhausted(NumericClass::Bignum))
                );
                arun.close_all_runs().unwrap();
                brun.close_all_runs().unwrap();
            }
            a.enter_native(&[]).unwrap();
            b.enter_native(&[]).unwrap();
            stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            // Deliberately no second GC: cold thaw must put the existing
            // lowest clear slot at the free-list head. Generic sweep order
            // is outside this contract and cannot mask this assertion.
        }
        let serial = if matches!(case, NumericSerialWindowCase::FullyDead) {
            None
        } else {
            // Validate the exact matching stopped/restored collector before
            // borrowing its actual serial heap. This test lives inside the
            // private registration module; no shipping authority API is added.
            era.with_stopped_heap_for_test(&stopped, |_| ()).unwrap();
            // This is actual serial stopped work, so temporarily restore the
            // keeper's own published GC image as well as its storage. Rejoin
            // its allocation facts before validating or completing this stop.
            let legacy_entry = {
                let gate = era.core.registry().lock().unwrap();
                Arc::clone(
                    gate.entries
                        .iter()
                        .find(|entry| entry.key.owner == 0)
                        .unwrap(),
                )
            };
            let legacy = entry_state(&legacy_entry).unwrap().image.take().unwrap();
            let mut collector = era.core.collector.borrow_mut();
            let heap = collector.as_mut().unwrap().registration_heap_mut();
            legacy.restore_legacy(heap).unwrap();
            let float = heap.alloc_float(40.5);
            // SAFETY: the stopped serial heap owns this initialized object.
            // Root it before the second real serial allocation.
            let float = unsafe { SharedRoot::new(heap, float) };
            let big = heap.alloc_bignum(Integer::from(i64::MAX - 40));
            // SAFETY: the same unique stopped heap owns this initialized
            // bignum. Shared roots survive after this heap borrow ends.
            let big = unsafe { SharedRoot::new(heap, big) };
            heap.close_alloc_regions();
            let legacy = PublishedGcImage::from_legacy(heap).unwrap();
            let mut state = entry_state(&legacy_entry).unwrap();
            assert!(state.image.replace(legacy).is_none());
            Some((float, big))
        };
        // Current broken code refuses here after a legitimate serial bit was
        // set beyond the saved frontier. The corrected frontier comes from
        // actual stopped allocation bits, not the old owner-used count.
        era.resume_cons_storage(&stopped).unwrap();
        if matches!(case, NumericSerialWindowCase::InteriorHole) {
            // This second stop deliberately performs no GC. Record its
            // successful stopped trace before completing the epoch, just as
            // the collection above recorded the first stop's trace.
            stopped.trace(|_, _| Ok(())).unwrap();
        }
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();
        {
            let mut arun = a.run_access().unwrap();
            let mut brun = b.run_access().unwrap();
            arun.float_into(50.75, &mut a_homes[0]).unwrap();
            arun.bignum_into(Integer::from(i64::MAX - 50), &mut a_homes[1])
                .unwrap();
            // Keep both physical claims live so a single reused page cannot
            // conceal a missing additional-demand page for the other owner.
            brun.float_into(60.25, &mut b_homes[0]).unwrap();
            brun.bignum_into(Integer::from(i64::MAX - 60), &mut b_homes[1])
                .unwrap();
            // SAFETY: both product payloads are rooted by current canonical
            // homes while admitted runs exclude collection.
            let (after_float, after_bignum) = unsafe {
                (
                    Value::from_bits(a_homes[0].word_for_test(&arun))
                        .as_float_ptr()
                        .unwrap() as usize,
                    Value::from_bits(a_homes[1].word_for_test(&arun))
                        .as_veclike_ptr()
                        .unwrap() as usize,
                )
            };
            match case {
                NumericSerialWindowCase::Append | NumericSerialWindowCase::InteriorHole => {
                    let expected = if matches!(case, NumericSerialWindowCase::Append) {
                        2
                    } else {
                        4
                    };
                    assert_eq!(after_float, first_float + expected * FloatObj::SLOT_BYTES);
                    assert_eq!(
                        after_bignum,
                        first_bignum + expected * BignumObj::SLOT_BYTES
                    );
                    let serial = serial.as_ref().unwrap();
                    // SAFETY: the checked same-heap shared homes retain the
                    // stopped serial results throughout these admitted reads.
                    unsafe {
                        let float = Value::from_bits(serial.0.cons_word_for_test());
                        let big = Value::from_bits(serial.1.cons_word_for_test());
                        assert_eq!(
                            float.as_float_ptr().unwrap() as usize,
                            first_float + FloatObj::SLOT_BYTES
                        );
                        assert_eq!(
                            big.as_veclike_ptr().unwrap() as usize,
                            first_bignum + BignumObj::SLOT_BYTES
                        );
                        assert_eq!(float.as_float(), Some(40.5));
                        assert_eq!(
                            big.as_bignum().unwrap().to_string(),
                            (i64::MAX - 40).to_string()
                        );
                    }
                }
                NumericSerialWindowCase::FullyDead => {
                    assert_eq!(
                        after_float % OBJECT_PAGE_BYTES,
                        0,
                        "fully dead retained float page restarts at slot zero"
                    );
                    assert_eq!(
                        after_bignum % OBJECT_PAGE_BYTES,
                        0,
                        "fully dead retained bignum page restarts at slot zero"
                    );
                    // Full capacity is proved through real rooted commits,
                    // not a mutable descriptor or a test allocator double.
                    for i in 1..ObjectPage::<FloatObj>::SLOTS {
                        arun.float_into(i as f64, &mut a_homes[0]).unwrap();
                    }
                    for i in 1..ObjectPage::<BignumObj>::SLOTS {
                        arun.bignum_into(Integer::from(i64::MAX - i as i64), &mut a_homes[1])
                            .unwrap();
                    }
                    assert_eq!(
                        arun.float_into(70.0, &mut a_homes[0]),
                        Err(NumericAllocError::Exhausted(NumericClass::Float))
                    );
                    assert_eq!(
                        arun.bignum_into(Integer::from(i64::MAX - 70), &mut a_homes[1]),
                        Err(NumericAllocError::Exhausted(NumericClass::Bignum))
                    );
                }
            }
            arun.close_all_runs().unwrap();
            brun.close_all_runs().unwrap();
        }
        for owner in [&mut a, &mut b] {
            let mut run = owner.run_access().unwrap();
            run.clear_root_slots().unwrap();
            run.close_all_runs().unwrap();
        }
        drop((a_homes, b_homes, serial));
        a.enter_native(&[]).unwrap();
        b.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        complete(stopped);
        a.leave_native().unwrap();
        b.leave_native().unwrap();
        retire(a);
        retire(b);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}
