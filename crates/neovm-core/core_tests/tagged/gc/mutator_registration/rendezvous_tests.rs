//! Rooted-rendezvous shells.
//! The shell opens the real rooted or plain mutator era; the bodies and their
//! assertions stay in value. One #[test] per former value row, generated from
//! the same list the completeness check compares with the typed case ALL.
use crate::emacs_core::eval::Context;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use crate::tagged::transport::SharedRoot;
use neovm_eval::emacs_core::eval::Baton;
use neovm_value::test_support::authority::{
    ProofEra, RootedRendezvousCase, run_rooted_mutator_test_for_test,
};

fn rooted_era_impl(body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, Vec<SharedRoot>)) {
    Context::with_test_rooted_mutator_era(|era, roots| body(era, roots));
}

fn mutator_era_impl(_flag: i64, body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)) {
    Context::with_test_mutator_era(|era| body(era));
}

fn mapping_runner(name: &'static str) {
    let case = match name {
        "stale_ack_identity_generation_duplicate_and_missing_image_are_refused" => {
            RootedRendezvousCase::StaleAckIdentityGenerationDuplicateAndMissingImageAreRefused
        }
        "two_owners_pause_before_ack_and_while_restoring_across_two_epochs" => {
            RootedRendezvousCase::TwoOwnersPauseBeforeAckAndWhileRestoringAcrossTwoEpochs
        }
        "native_owner_sleeps_two_epochs_latest_image_and_synthetic_work_once" => {
            RootedRendezvousCase::NativeOwnerSleepsTwoEpochsLatestImageAndSyntheticWorkOnce
        }
        "native_idle_restore_debt_queues_request_before_any_new_epoch" => {
            RootedRendezvousCase::NativeIdleRestoreDebtQueuesRequestBeforeAnyNewEpoch
        }
        "retiring_image_and_starting_owner_keep_exact_frozen_membership" => {
            RootedRendezvousCase::RetiringImageAndStartingOwnerKeepExactFrozenMembership
        }
        "live_worker_loan_blocks_ack_and_preserves_running_owner" => {
            RootedRendezvousCase::LiveWorkerLoanBlocksAckAndPreservesRunningOwner
        }
        "native_publication_is_not_stopped_until_root_and_image_commit" => {
            RootedRendezvousCase::NativePublicationIsNotStoppedUntilRootAndImageCommit
        }
        "native_return_waits_for_active_stop_before_restoration" => {
            RootedRendezvousCase::NativeReturnWaitsForActiveStopBeforeRestoration
        }
        "completion_requires_trace_and_every_explicit_reader_finish" => {
            RootedRendezvousCase::CompletionRequiresTraceAndEveryExplicitReaderFinish
        }
        "stale_registration_exhaustion_and_frozen_removal_refuse_before_trace" => {
            RootedRendezvousCase::StaleRegistrationExhaustionAndFrozenRemovalRefuseBeforeTrace
        }
        "forgotten_reader_refuses_resume_and_retains_roots" => {
            RootedRendezvousCase::ForgottenReaderRefusesResumeAndRetainsRoots
        }
        "unfinished_reader_drop_refuses_resume_and_retains_roots" => {
            RootedRendezvousCase::UnfinishedReaderDropRefusesResumeAndRetainsRoots
        }
        "external_poison_between_predicate_and_wait_is_event_driven" => {
            RootedRendezvousCase::ExternalPoisonBetweenPredicateAndWaitIsEventDriven
        }
        "lost_poison_notification_between_predicate_and_wait_fails_closed" => {
            RootedRendezvousCase::LostPoisonNotificationBetweenPredicateAndWaitFailsClosed
        }
        other => panic!("unknown rooted mapping row: {other}"),
    };
    run(case);
}

fn run(case: RootedRendezvousCase) {
    // The eval symbol fixture boots a bare Context; install the canonical
    // core eval hooks first, the same boot new_context performs.
    let _hooks = crate::emacs_core::eval_hook_installers::install_eval_hooks();
    neovm_value::test_support::authority::install_rooted_mapping_runner_for_test(mapping_runner);
    run_rooted_mutator_test_for_test(case, rooted_era_impl, mutator_era_impl);
}

macro_rules! shells {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            run(RootedRendezvousCase::$case);
        })*
        const SHELL_CASES: &[RootedRendezvousCase] = &[$(RootedRendezvousCase::$case),*];
    };
}

shells! {
    two_owners_pause_before_ack_and_while_restoring_across_two_epochs => TwoOwnersPauseBeforeAckAndWhileRestoringAcrossTwoEpochs,
    stale_ack_identity_generation_duplicate_and_missing_image_are_refused => StaleAckIdentityGenerationDuplicateAndMissingImageAreRefused,
    live_worker_loan_blocks_ack_and_preserves_running_owner => LiveWorkerLoanBlocksAckAndPreservesRunningOwner,
    native_owner_sleeps_two_epochs_latest_image_and_synthetic_work_once => NativeOwnerSleepsTwoEpochsLatestImageAndSyntheticWorkOnce,
    native_idle_restore_debt_queues_request_before_any_new_epoch => NativeIdleRestoreDebtQueuesRequestBeforeAnyNewEpoch,
    native_return_waits_for_active_stop_before_restoration => NativeReturnWaitsForActiveStopBeforeRestoration,
    retiring_image_and_starting_owner_keep_exact_frozen_membership => RetiringImageAndStartingOwnerKeepExactFrozenMembership,
    rooted_protocol_stress_two_owners_many_epochs => RootedProtocolStressTwoOwnersManyEpochs,
    native_publication_is_not_stopped_until_root_and_image_commit => NativePublicationIsNotStoppedUntilRootAndImageCommit,
    a6_ht_b9_table_scope_gate_contention_retries_before_job => A6HtB9TableScopeGateContentionRetriesBeforeJob,
    a6_ht_b9_table_scope_body_error_stays_terminal_after_image_mutation => A6HtB9TableScopeBodyErrorStaysTerminalAfterImageMutation,
    a6_ht_b9_table_scope_keeper_gate_reentry_refuses_without_retry => A6HtB9TableScopeKeeperGateReentryRefusesWithoutRetry,
    trace_callback_error_poison_is_visible_before_guard_drop => TraceCallbackErrorPoisonIsVisibleBeforeGuardDrop,
    trace_callback_panic_poison_retains_published_images => TraceCallbackPanicPoisonRetainsPublishedImages,
    dropped_stopped_guard_wakes_owner_waiting_for_completion => DroppedStoppedGuardWakesOwnerWaitingForCompletion,
    unfinished_reader_drop_refuses_resume_and_retains_roots => UnfinishedReaderDropRefusesResumeAndRetainsRoots,
    forgotten_reader_refuses_resume_and_retains_roots => ForgottenReaderRefusesResumeAndRetainsRoots,
    completion_requires_trace_and_every_explicit_reader_finish => CompletionRequiresTraceAndEveryExplicitReaderFinish,
    reader_callback_reentry_refuses_run_access_without_waiting => ReaderCallbackReentryRefusesRunAccessWithoutWaiting,
    lost_poison_notification_between_predicate_and_wait_fails_closed => LostPoisonNotificationBetweenPredicateAndWaitFailsClosed,
    external_poison_between_predicate_and_wait_is_event_driven => ExternalPoisonBetweenPredicateAndWaitIsEventDriven,
    mutex_unwind_recovers_guard_before_publishing_terminal_poison => MutexUnwindRecoversGuardBeforePublishingTerminalPoison,
    condvar_reacquire_recovers_poisoned_guard_without_recursive_lock => CondvarReacquireRecoversPoisonedGuardWithoutRecursiveLock,
    stale_registration_exhaustion_and_frozen_removal_refuse_before_trace => StaleRegistrationExhaustionAndFrozenRemovalRefuseBeforeTrace,
    forgotten_run_access_blocks_finish_and_ack_then_owner_drop_poisons => ForgottenRunAccessBlocksFinishAndAckThenOwnerDropPoisons,
    legacy_epoch_only_publication_and_restore_cannot_bypass_rooted_rendezvous => LegacyEpochOnlyPublicationAndRestoreCannotBypassRootedRendezvous,
    callback_panic_outside_trace_immediately_poisons_root_and_image_readers => CallbackPanicOutsideTraceImmediatelyPoisonsRootAndImageReaders,
    keeper_running_owner_stop_refuses_and_can_ack_afterward => KeeperRunningOwnerStopRefusesAndCanAckAfterward,
    keeper_native_restore_refuses_active_stop_and_keeps_image => KeeperNativeRestoreRefusesActiveStopAndKeepsImage,
    keeper_starting_owner_waits_refuse_until_bootstrap_opens => KeeperStartingOwnerWaitsRefuseUntilBootstrapOpens,
    already_published_owner_wait_request_refuses => AlreadyPublishedOwnerWaitRequestRefuses,
    keeper_resume_debt_refuses_next_stop_until_owner_applies => KeeperResumeDebtRefusesNextStopUntilOwnerApplies,
}

#[test]
fn shell_cases_cover_all_rooted_rendezvous_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = RootedRendezvousCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per value row");
}
