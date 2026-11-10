//! Concurrent leaf-claim shells.
//! The shell supplies the edit marker-chain host; the bodies and their
//! assertions stay in value.
use crate::buffer::buffer_text::BufferText;
use neovm_edit::test_support::authority::chain_splice_at_head_for_test;
use neovm_value::tagged::header::MarkerObj;
use neovm_value::test_support::authority::{
    ConcurrentLeafClaimCase, run_concurrent_leaf_claim_test_for_test,
};

fn new_marker_state() -> *mut () {
    Box::into_raw(Box::new(BufferText::new())) as *mut ()
}

unsafe fn marker_splice(state: *mut (), marker: *mut MarkerObj) {
    let text = unsafe { &*(state as *const BufferText) };
    // SAFETY: `marker` is a live unchained marker in this serial fixture, and
    // `state` is the text this shell allocated.
    unsafe { chain_splice_at_head_for_test(text, marker) };
}

unsafe fn marker_head(state: *mut ()) -> *mut *mut MarkerObj {
    let text = unsafe { &*(state as *const BufferText) };
    // SAFETY: the value body calls this from the same exclusive fixture that
    // used to call `BufferText::markers_head_slot_raw`.
    unsafe { text.markers_head_slot_raw() }
}

unsafe fn marker_walk(state: *mut ()) -> Vec<*mut MarkerObj> {
    let text = unsafe { &*(state as *const BufferText) };
    text.chain_walk_collect()
}

unsafe fn marker_drop(state: *mut ()) {
    drop(unsafe { Box::from_raw(state as *mut BufferText) });
}

fn run(case: ConcurrentLeafClaimCase) {
    run_concurrent_leaf_claim_test_for_test(
        case,
        new_marker_state,
        marker_splice,
        marker_head,
        marker_walk,
        marker_drop,
    );
}

macro_rules! shells {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            run(ConcurrentLeafClaimCase::$case);
        })*
        const SHELL_CASES: &[ConcurrentLeafClaimCase] = &[$(ConcurrentLeafClaimCase::$case),*];
    };
}

shells! {
    concurrent_leaf_claims_off_keeps_the_original_defer_path_and_allocates_no_counter => ConcurrentLeafClaimsOffKeepsTheOriginalDeferPathAndAllocatesNoCounter,
    concurrent_leaf_claims_obey_generation_parity_and_once_only_promotion => ConcurrentLeafClaimsObeyGenerationParityAndOnceOnlyPromotion,
    concurrent_leaf_claims_refuse_post_start_pages_without_touching_the_header => ConcurrentLeafClaimsRefusePostStartPagesWithoutTouchingTheHeader,
    concurrent_leaf_claims_keep_mapped_and_residual_box_symbol_positions_deferred => ConcurrentLeafClaimsKeepMappedAndResidualBoxSymbolPositionsDeferred,
    concurrent_leaf_symbol_positions_route_both_fields_and_skip_born_black_payloads => ConcurrentLeafSymbolPositionsRouteBothFieldsAndSkipBornBlackPayloads,
    concurrent_leaf_claims_preserve_weak_symbol_liveness_and_reclaim_dead_objects => ConcurrentLeafClaimsPreserveWeakSymbolLivenessAndReclaimDeadObjects,
    concurrent_leaf_claims_do_not_read_marker_chain_payloads => ConcurrentLeafClaimsDoNotReadMarkerChainPayloads,
    concurrent_leaf_claim_rmw_has_one_winner_across_threads => ConcurrentLeafClaimRmwHasOneWinnerAcrossThreads,
    concurrent_leaf_claims_lost_worker_result_fails_closed_with_generations_off => ConcurrentLeafClaimsLostWorkerResultFailsClosedWithGenerationsOff,
}

#[test]
fn shell_cases_cover_all_concurrent_leaf_claim_case_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = ConcurrentLeafClaimCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per former value row");
}
