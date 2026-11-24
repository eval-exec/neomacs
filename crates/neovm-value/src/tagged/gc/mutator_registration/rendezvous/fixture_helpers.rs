//! Shared rendezvous test fixtures and the typed case list.
//!
//! Used by the value-side rooted body modules and by the legacy `tests`
//! module that still hosts libval's mapping rows.
#![forbid(unsafe_code)]

use super::super::EraKeeper;
use super::*;
pub(crate) use crate::tagged::gc::ProofEra as ParallelHeapEra;
pub(crate) use crate::tagged::value::TaggedValue;
pub(crate) use std::sync::mpsc;
pub(crate) use std::sync::mpsc::Receiver;
pub(crate) use std::thread;
pub(crate) use std::time::Duration;

/// Opener slots are plain fn pointers: the self-wait rows move them into a
/// spawned keeper actor, which a `&mut dyn` opener could not cross.
pub(crate) type RootedEraFn<K> =
    for<'a> fn(&'a mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, Vec<SharedRoot>));
pub(crate) type MutatorEraFn<K> =
    for<'a> fn(i64, &'a mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>));

/// One row per former value lib-test. The runner matches this exhaustively;
/// the core shell lists one #[test] per variant and checks the list against
/// `ALL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RootedRendezvousCase {
    TwoOwnersPauseBeforeAckAndWhileRestoringAcrossTwoEpochs,
    StaleAckIdentityGenerationDuplicateAndMissingImageAreRefused,
    LiveWorkerLoanBlocksAckAndPreservesRunningOwner,
    NativeOwnerSleepsTwoEpochsLatestImageAndSyntheticWorkOnce,
    NativeIdleRestoreDebtQueuesRequestBeforeAnyNewEpoch,
    NativeReturnWaitsForActiveStopBeforeRestoration,
    RetiringImageAndStartingOwnerKeepExactFrozenMembership,
    RootedProtocolStressTwoOwnersManyEpochs,
    NativePublicationIsNotStoppedUntilRootAndImageCommit,
    A6HtB9TableScopeGateContentionRetriesBeforeJob,
    A6HtB9TableScopeBodyErrorStaysTerminalAfterImageMutation,
    A6HtB9TableScopeKeeperGateReentryRefusesWithoutRetry,
    TraceCallbackErrorPoisonIsVisibleBeforeGuardDrop,
    TraceCallbackPanicPoisonRetainsPublishedImages,
    DroppedStoppedGuardWakesOwnerWaitingForCompletion,
    UnfinishedReaderDropRefusesResumeAndRetainsRoots,
    ForgottenReaderRefusesResumeAndRetainsRoots,
    CompletionRequiresTraceAndEveryExplicitReaderFinish,
    ReaderCallbackReentryRefusesRunAccessWithoutWaiting,
    LostPoisonNotificationBetweenPredicateAndWaitFailsClosed,
    ExternalPoisonBetweenPredicateAndWaitIsEventDriven,
    MutexUnwindRecoversGuardBeforePublishingTerminalPoison,
    CondvarReacquireRecoversPoisonedGuardWithoutRecursiveLock,
    StaleRegistrationExhaustionAndFrozenRemovalRefuseBeforeTrace,
    ForgottenRunAccessBlocksFinishAndAckThenOwnerDropPoisons,
    LegacyEpochOnlyPublicationAndRestoreCannotBypassRootedRendezvous,
    CallbackPanicOutsideTraceImmediatelyPoisonsRootAndImageReaders,
    KeeperRunningOwnerStopRefusesAndCanAckAfterward,
    KeeperNativeRestoreRefusesActiveStopAndKeepsImage,
    KeeperStartingOwnerWaitsRefuseUntilBootstrapOpens,
    AlreadyPublishedOwnerWaitRequestRefuses,
    KeeperResumeDebtRefusesNextStopUntilOwnerApplies,
    FirstRestoredOwnerWaitsForTheFinalPeerDebt,
    PoisonWakesARestoredOwnerWaitingForPeerDebt,
    StopAndUnrestoredOwnerRefuseInsteadOfWaitingForThemselves,
    NativeReceiptOutlastsAGateHolderAfterTheCommit,
    StoppedKeeperWaitsOutABriefGateHolder,
    KeeperTraceWaitsOutABriefGateHolder,
    KeeperCompleteWaitsOutABriefGateHolder,
    KeeperCompleteRetriesAnEntryHeldThroughTheReaderScan,
    KeeperTakesSummariesThroughItsStop,
    ABrokenCensusUnderAStopIsNotBusy,
    KeeperWorldRootCheckWaitsOutABriefGateHolder,
    KeeperRootReaderWaitsOutAnOwnerMomentOnItsEntry,
    KeeperReenteringAnEntryItHoldsRefusesAtOnce,
    KeeperGrantsTheSerialHeadOnlyInsideItsTracedStop,
    KeeperBoundedStopPendsAndKeepsItsEpoch,
}

impl RootedRendezvousCase {
    pub const ALL: &'static [Self] = &[
        Self::TwoOwnersPauseBeforeAckAndWhileRestoringAcrossTwoEpochs,
        Self::StaleAckIdentityGenerationDuplicateAndMissingImageAreRefused,
        Self::LiveWorkerLoanBlocksAckAndPreservesRunningOwner,
        Self::NativeOwnerSleepsTwoEpochsLatestImageAndSyntheticWorkOnce,
        Self::NativeIdleRestoreDebtQueuesRequestBeforeAnyNewEpoch,
        Self::NativeReturnWaitsForActiveStopBeforeRestoration,
        Self::RetiringImageAndStartingOwnerKeepExactFrozenMembership,
        Self::RootedProtocolStressTwoOwnersManyEpochs,
        Self::NativePublicationIsNotStoppedUntilRootAndImageCommit,
        Self::A6HtB9TableScopeGateContentionRetriesBeforeJob,
        Self::A6HtB9TableScopeBodyErrorStaysTerminalAfterImageMutation,
        Self::A6HtB9TableScopeKeeperGateReentryRefusesWithoutRetry,
        Self::TraceCallbackErrorPoisonIsVisibleBeforeGuardDrop,
        Self::TraceCallbackPanicPoisonRetainsPublishedImages,
        Self::DroppedStoppedGuardWakesOwnerWaitingForCompletion,
        Self::UnfinishedReaderDropRefusesResumeAndRetainsRoots,
        Self::ForgottenReaderRefusesResumeAndRetainsRoots,
        Self::CompletionRequiresTraceAndEveryExplicitReaderFinish,
        Self::ReaderCallbackReentryRefusesRunAccessWithoutWaiting,
        Self::LostPoisonNotificationBetweenPredicateAndWaitFailsClosed,
        Self::ExternalPoisonBetweenPredicateAndWaitIsEventDriven,
        Self::MutexUnwindRecoversGuardBeforePublishingTerminalPoison,
        Self::CondvarReacquireRecoversPoisonedGuardWithoutRecursiveLock,
        Self::StaleRegistrationExhaustionAndFrozenRemovalRefuseBeforeTrace,
        Self::ForgottenRunAccessBlocksFinishAndAckThenOwnerDropPoisons,
        Self::LegacyEpochOnlyPublicationAndRestoreCannotBypassRootedRendezvous,
        Self::CallbackPanicOutsideTraceImmediatelyPoisonsRootAndImageReaders,
        Self::KeeperRunningOwnerStopRefusesAndCanAckAfterward,
        Self::KeeperNativeRestoreRefusesActiveStopAndKeepsImage,
        Self::KeeperStartingOwnerWaitsRefuseUntilBootstrapOpens,
        Self::AlreadyPublishedOwnerWaitRequestRefuses,
        Self::KeeperResumeDebtRefusesNextStopUntilOwnerApplies,
        Self::FirstRestoredOwnerWaitsForTheFinalPeerDebt,
        Self::PoisonWakesARestoredOwnerWaitingForPeerDebt,
        Self::StopAndUnrestoredOwnerRefuseInsteadOfWaitingForThemselves,
        Self::NativeReceiptOutlastsAGateHolderAfterTheCommit,
        Self::StoppedKeeperWaitsOutABriefGateHolder,
        Self::KeeperTraceWaitsOutABriefGateHolder,
        Self::KeeperCompleteWaitsOutABriefGateHolder,
        Self::KeeperCompleteRetriesAnEntryHeldThroughTheReaderScan,
        Self::KeeperTakesSummariesThroughItsStop,
        Self::ABrokenCensusUnderAStopIsNotBusy,
        Self::KeeperWorldRootCheckWaitsOutABriefGateHolder,
        Self::KeeperRootReaderWaitsOutAnOwnerMomentOnItsEntry,
        Self::KeeperReenteringAnEntryItHoldsRefusesAtOnce,
        Self::KeeperGrantsTheSerialHeadOnlyInsideItsTracedStop,
        Self::KeeperBoundedStopPendsAndKeepsItsEpoch,
    ];
}

pub(crate) fn receive<T>(receiver: &Receiver<T>) -> T {
    receiver
        .recv_timeout(Duration::from_secs(30))
        .expect("fixture actor must reach the requested protocol boundary")
}

pub(crate) fn active(request: Request) -> NonZeroU64 {
    match request {
        Request::Active(epoch) => epoch,
        Request::Queued => panic!("fixture expected active request"),
    }
}

pub(crate) fn retry<T>(mut operation: impl FnMut() -> Result<T, RendezvousError>) -> T {
    loop {
        match operation() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) => thread::yield_now(),
            Err(error) => panic!("unexpected rendezvous refusal: {error:?}"),
        }
    }
}

pub(crate) fn activate<'era>(
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
            Err(failure) => panic!("unexpected activation refusal: {:?}", failure.reason),
        }
    }
}

pub(crate) fn apply(image: ResumeGcImage<'_, '_>) {
    // Use the product's ownership-preserving admission policy. A Busy refusal
    // precedes restoration; this never retries a predicate wait or assertion.
    image.apply_wait().unwrap();
}

/// Serial histories and intervals whose actors are already paused must succeed
/// on the first admission. Existing concurrent callers retain their old policy;
/// their activation, ACK and retirement admission gaps are audited separately.
#[derive(Clone, Copy)]
pub(crate) enum ObservationAdmission {
    Once,
    ExistingConcurrentRetry,
}

impl ObservationAdmission {
    fn attempt<T>(self, mut operation: impl FnMut() -> Result<T, RendezvousError>) -> T {
        match self {
            Self::Once => operation().unwrap(),
            Self::ExistingConcurrentRetry => retry(operation),
        }
    }
}

pub(crate) fn complete(mut stopped: AllStopped<'_>) {
    loop {
        match stopped.complete() {
            Ok(()) => return,
            Err(failure) if failure.reason == RendezvousError::Busy => {
                stopped = failure.into_stopped();
                thread::yield_now();
            }
            Err(failure) => panic!("unexpected completion refusal: {:?}", failure.reason),
        }
    }
}

pub(crate) fn retire(mut owner: Mutator<'_>, roots: &[SharedRoot]) {
    loop {
        match owner.retire_rooted(roots) {
            Ok(()) => return,
            Err(failure) if failure.reason == RendezvousError::Busy => {
                owner = failure.into_owner();
                thread::yield_now();
            }
            Err(failure) => panic!("unexpected retirement refusal: {:?}", failure.reason),
        }
    }
}

pub(crate) fn park(owner: &mut Mutator<'_>, roots: &[SharedRoot]) -> u64 {
    let epoch = retry(|| owner.wait_request());
    retry(|| owner.acknowledge(epoch, roots));
    let image = owner.begin_restore_retry(false).unwrap();
    let returned = image.epoch();
    apply(image);
    returned
}

// Observers obey the same gate-before-entry order as mutating owners. An
// ungated test-only inspector can otherwise manufacture a post-restore Busy.
pub(crate) fn inspect_entry<R>(
    rv: Rendezvous<'_>,
    entry: &MutatorEntry,
    mut inspect: impl FnMut(&EntryState) -> R,
) -> R {
    retry(|| {
        let _gate = rv.registry.lock()?;
        let state = entry_state(entry)?;
        Ok(inspect(&state))
    })
}

pub(crate) fn seed(owner: &mut Mutator<'_>, marker: i64) {
    loop {
        match owner.run_access() {
            Ok(mut run) => {
                let local = run.state();
                local.hot.remset.push(TaggedValue::fixnum(marker));
                local.hot.bytes_since_gc = marker as usize;
                break;
            }
            Err(RegistrationError::Busy) => thread::yield_now(),
            Err(error) => panic!("unexpected synthetic run refusal: {error:?}"),
        }
    }
}

pub(crate) fn read_words(stopped: &AllStopped<'_>, key: MutatorKey) -> Vec<usize> {
    read_words_at(ObservationAdmission::ExistingConcurrentRetry, stopped, key)
}

pub(crate) fn read_words_at(
    admission: ObservationAdmission,
    stopped: &AllStopped<'_>,
    key: MutatorKey,
) -> Vec<usize> {
    admission.attempt(|| {
        stopped.with_root_reader(key, |reader| {
            let mut words = Vec::new();
            reader.append_words(&mut words);
            reader.finish().unwrap();
            words
        })
    })
}

pub(crate) fn trace_owners(stopped: &mut AllStopped<'_>, keys: &[MutatorKey]) -> Vec<Vec<usize>> {
    trace_owners_at(ObservationAdmission::ExistingConcurrentRetry, stopped, keys)
}

pub(crate) fn trace_owners_at(
    admission: ObservationAdmission,
    stopped: &mut AllStopped<'_>,
    keys: &[MutatorKey],
) -> Vec<Vec<usize>> {
    admission.attempt(|| {
        stopped.trace(|guard, _proof| {
            let mut result = Vec::new();
            for key in keys {
                let words = read_words_at(admission, guard, *key);
                assert_eq!(words.len(), 1);
                assert_ne!(words[0], 0);
                result.push(words);
                admission.attempt(|| {
                    guard.with_owner_image(*key, |image| {
                        assert!(image.roots_empty());
                        assert!(image.young_empty());
                    })
                });
            }
            if result.len() == 2 {
                assert_ne!(result[0], result[1], "each owner published its own root");
            }
            Ok(result)
        })
    })
}

pub(crate) fn finish_era<K: EraKeeper>(era: ParallelHeapEra<'_, K>) {
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
}
