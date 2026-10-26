//! Attempt-local ownership of a prepared edit suffix during its source walk.

use super::{TextWindowOutputTarget, WindowOutputEmitter};
use crate::buffer_source::row_lifecycle::SourceLineRestart;
use crate::incremental_layout::edit_sync::{
    EditSyncReached, PreparedEditSuffix, RestartFrontierRefusal, RestartInterest, RestartSelection,
    SelectedEditSync,
};
use crate::types::LayoutCharPos0;

#[derive(Debug)]
pub(super) enum SyncAttempt {
    Inactive,
    Armed(PreparedEditSuffix),
    Selected(SelectedEditSync),
    Invalid(SyncStateError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum SyncStateError {
    #[error("edit suffix already armed")]
    AlreadyArmed,
    #[error("an earlier edit frontier was selected before source retry")]
    SelectedBeforeRetry,
    #[error("the builder reached a frontier not owned by this emitter")]
    UnownedBuilderReach,
    #[error("the builder and emitter both selected a frontier")]
    ConflictingReach,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartSyncDecision {
    Continue,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartSyncState {
    Continuing,
    Selected,
    Invalid,
}

impl WindowOutputEmitter {
    /// Initialize the sole preparation on a freshly constructed local emitter.
    pub(crate) fn arm_edit_sync(&mut self, preparation: PreparedEditSuffix) {
        self.sync_attempt = match self.sync_attempt {
            SyncAttempt::Inactive => SyncAttempt::Armed(preparation),
            SyncAttempt::Armed(_) | SyncAttempt::Selected(_) | SyncAttempt::Invalid(_) => {
                SyncAttempt::Invalid(SyncStateError::AlreadyArmed)
            }
        };
    }

    pub(crate) fn restart_sync_interest(&self, next: LayoutCharPos0) -> RestartInterest {
        // Position/pixel queries retain their own target and restart protocol.
        if self.query_target.is_some() || self.collect_query_restarts {
            return RestartInterest::NotApplicable;
        }
        match &self.sync_attempt {
            SyncAttempt::Armed(preparation) => preparation.restart_interest(next),
            SyncAttempt::Inactive | SyncAttempt::Selected(_) | SyncAttempt::Invalid(_) => {
                RestartInterest::NotApplicable
            }
        }
    }

    /// Consume the producer proof immediately, without Lisp or source mutation.
    pub(crate) fn try_restart_sync(
        &mut self,
        restart: SourceLineRestart<'_>,
        mut output: TextWindowOutputTarget<'_>,
    ) -> RestartSyncDecision {
        let current = std::mem::replace(&mut self.sync_attempt, SyncAttempt::Inactive);
        match current {
            SyncAttempt::Armed(preparation) => match preparation.select_restart(restart) {
                RestartSelection::Selected(selected) => {
                    self.sync_attempt = if output.builder().finish_edit_sync().is_none() {
                        SyncAttempt::Selected(selected)
                    } else {
                        SyncAttempt::Invalid(SyncStateError::ConflictingReach)
                    };
                    RestartSyncDecision::Stopped
                }
                RestartSelection::Refused {
                    preparation,
                    reason:
                        RestartFrontierRefusal::NoIndependentFrontier
                        | RestartFrontierRefusal::FrontierNotReached,
                } => {
                    self.sync_attempt = SyncAttempt::Armed(preparation);
                    RestartSyncDecision::Continue
                }
            },
            SyncAttempt::Inactive => RestartSyncDecision::Continue,
            SyncAttempt::Selected(selected) => {
                self.sync_attempt = SyncAttempt::Selected(selected);
                RestartSyncDecision::Stopped
            }
            SyncAttempt::Invalid(error) => {
                self.sync_attempt = SyncAttempt::Invalid(error);
                RestartSyncDecision::Stopped
            }
        }
    }

    pub(crate) fn restart_sync_state(&self) -> RestartSyncState {
        match self.sync_attempt {
            SyncAttempt::Inactive | SyncAttempt::Armed(_) => RestartSyncState::Continuing,
            SyncAttempt::Selected(_) => RestartSyncState::Selected,
            SyncAttempt::Invalid(_) => RestartSyncState::Invalid,
        }
    }

    pub(crate) fn take_edit_sync_for_retry(
        &mut self,
    ) -> Result<Option<PreparedEditSuffix>, SyncStateError> {
        match std::mem::replace(&mut self.sync_attempt, SyncAttempt::Inactive) {
            SyncAttempt::Inactive => Ok(None),
            SyncAttempt::Armed(preparation) => Ok(Some(preparation)),
            SyncAttempt::Selected(_) => Err(SyncStateError::SelectedBeforeRetry),
            SyncAttempt::Invalid(error) => Err(error),
        }
    }

    pub(crate) fn finish_edit_sync(
        &mut self,
        reached: Option<EditSyncReached>,
    ) -> Result<Option<SelectedEditSync>, SyncStateError> {
        match (
            std::mem::replace(&mut self.sync_attempt, SyncAttempt::Inactive),
            reached,
        ) {
            (SyncAttempt::Inactive, None) => Ok(None),
            (SyncAttempt::Inactive, Some(_)) => Err(SyncStateError::UnownedBuilderReach),
            (SyncAttempt::Armed(preparation), Some(reached)) => {
                Ok(Some(preparation.select_conservative(reached)))
            }
            (SyncAttempt::Armed(_), None) => Ok(None),
            (SyncAttempt::Selected(selected), None) => Ok(Some(selected)),
            (SyncAttempt::Selected(_), Some(_)) => Err(SyncStateError::ConflictingReach),
            (SyncAttempt::Invalid(error), None | Some(_)) => Err(error),
        }
    }
}

#[cfg(test)]
#[path = "restart_sync/tests/restart_sync_test.rs"]
mod tests;
