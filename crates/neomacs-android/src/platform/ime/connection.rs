//! UI-thread connection authority. No JNI or evaluator state lives here.

use neomacs_app::frontend_event::ImeSessionId;

/// Issued by one connection owner; never relabel an existing connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConnectionId {
    session: ImeSessionId,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConnectionError {
    Retired,
    IdentityExhausted,
    BatchDepthExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BatchState {
    Idle,
    InProgress,
}

struct ActiveConnection {
    id: ConnectionId,
    batch_depth: u32,
}

/// One view's connection authority. Replacement retires the old generation.
#[derive(Default)]
pub(super) struct Connections {
    generation: u64,
    active: Option<ActiveConnection>,
}

impl Connections {
    pub(super) fn open(&mut self, session: ImeSessionId) -> Result<ConnectionId, ConnectionError> {
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(ConnectionError::IdentityExhausted)?;
        let id = ConnectionId {
            session,
            generation,
        };
        self.generation = generation;
        self.active = Some(ActiveConnection { id, batch_depth: 0 });
        Ok(id)
    }

    pub(super) fn validate(&self, id: ConnectionId) -> Result<(), ConnectionError> {
        if self.active.as_ref().is_some_and(|active| active.id == id) {
            Ok(())
        } else {
            Err(ConnectionError::Retired)
        }
    }

    pub(super) fn begin_batch(&mut self, id: ConnectionId) -> Result<(), ConnectionError> {
        self.validate(id)?;
        let active = self.active.as_mut().expect("validated active connection");
        active.batch_depth = active
            .batch_depth
            .checked_add(1)
            .ok_or(ConnectionError::BatchDepthExhausted)?;
        Ok(())
    }

    /// Like GNU Emacs, tolerate an unmatched end without underflow. This
    /// tracks notification grouping, not atomic editor transactions.
    pub(super) fn end_batch(&mut self, id: ConnectionId) -> Result<BatchState, ConnectionError> {
        self.validate(id)?;
        let active = self.active.as_mut().expect("validated active connection");
        active.batch_depth = active.batch_depth.saturating_sub(1);
        Ok(if active.batch_depth == 0 {
            BatchState::Idle
        } else {
            BatchState::InProgress
        })
    }

    /// Retire current authority. Cleanup of the caller's JNI resources is
    /// separate and must still happen when this returns false for an old ID.
    pub(super) fn close(&mut self, id: ConnectionId) -> bool {
        if self.validate(id).is_ok() {
            self.active = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
#[path = "tests/connection.rs"]
mod tests;
