//! Structured, low-overhead observation of interactive user commands.
//!
//! The command loop captures a start marker only after a complete key sequence
//! has been read.  Consequently `total_wall_us` measures work owned by one
//! command iteration; it never includes the user's think time between keys of
//! a multi-key sequence.  It is still deliberately wall time: a recursive
//! command such as `execute-extended-command` includes time spent in its nested
//! minibuffer command loop, and `depth` makes that nesting explicit.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use strum::IntoStaticStr;

use crate::buffer::BufferId;
use crate::window::FrameId;

use super::error::Flow;

static NEXT_COMMAND_ID: AtomicU64 = AtomicU64::new(1);

/// The instant at which a complete key sequence became an executable command.
///
/// Construction is gated by the INFO subscriber, so ordinary WARN-level runs
/// pay neither clock-reading nor command-description allocation costs.
#[derive(Debug)]
pub(crate) struct UserCommandObservationStart(Instant);

impl UserCommandObservationStart {
    pub(crate) fn capture_if_enabled() -> Option<Self> {
        tracing::enabled!(tracing::Level::INFO).then(|| Self(Instant::now()))
    }
}

/// Stable identity attached to both records for one command iteration.
#[derive(Debug)]
pub(crate) struct UserCommandIdentity {
    context_id: u64,
    depth: usize,
    keys: String,
    original_command: String,
    command: String,
    frame_id: Option<FrameId>,
    buffer_id: Option<BufferId>,
}

impl UserCommandIdentity {
    pub(crate) fn new(
        context_id: u64,
        depth: usize,
        keys: String,
        original_command: String,
        command: String,
        frame_id: Option<FrameId>,
        buffer_id: Option<BufferId>,
    ) -> Self {
        Self {
            context_id,
            depth,
            keys,
            original_command,
            command,
            frame_id,
            buffer_id,
        }
    }
}

/// Every non-local result that command execution can return.
///
/// Matching [`Flow`] exhaustively makes a new VM control-flow variant a
/// compile-time decision for command observability instead of silently
/// classifying it as an ordinary error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum UserCommandOutcome {
    Completed,
    Signaled,
    Threw,
    ThreadBlocked,
    Shutdown,
    NotExecuted,
}

impl UserCommandOutcome {
    pub(crate) fn from_flow(flow: &Flow) -> Self {
        match flow {
            Flow::Signal(_) => Self::Signaled,
            Flow::Throw(_) => Self::Threw,
            Flow::ThreadBlocked(_) => Self::ThreadBlocked,
            Flow::Shutdown(_) => Self::Shutdown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum CommandFinalization {
    Completed,
    Unwound,
}

/// Typed start marker for the `command-execute` portion of an iteration.
#[derive(Debug)]
pub(crate) struct CommandExecutionStart(Instant);

/// One correlated start/complete pair for an interactive command.
///
/// Completion is emitted from `Drop`, so a Lisp signal, throw, cooperative
/// thread handoff, shutdown, or failure in post-command finalization cannot
/// leave a misleading start record with no terminal event.
#[derive(Debug)]
pub(crate) struct UserCommandObservation {
    id: u64,
    identity: UserCommandIdentity,
    started_at: Instant,
    command_execute_wall: Option<Duration>,
    outcome: UserCommandOutcome,
    finalization: CommandFinalization,
}

impl UserCommandObservation {
    pub(crate) fn begin(start: UserCommandObservationStart, identity: UserCommandIdentity) -> Self {
        let id = NEXT_COMMAND_ID.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            command_id = id,
            context_id = identity.context_id,
            depth = identity.depth,
            keys = %identity.keys,
            original_command = %identity.original_command,
            command = %identity.command,
            frame_id = ?identity.frame_id.map(|frame_id| frame_id.0),
            buffer_id = ?identity.buffer_id.map(|buffer_id| buffer_id.0),
            "user_command_start"
        );
        Self {
            id,
            identity,
            started_at: start.0,
            command_execute_wall: None,
            outcome: UserCommandOutcome::NotExecuted,
            finalization: CommandFinalization::Unwound,
        }
    }

    pub(crate) fn begin_execution(&self) -> CommandExecutionStart {
        CommandExecutionStart(Instant::now())
    }

    pub(crate) fn finish_execution(
        &mut self,
        start: CommandExecutionStart,
        outcome: UserCommandOutcome,
    ) {
        self.command_execute_wall = Some(start.0.elapsed());
        self.outcome = outcome;
    }

    pub(crate) fn complete_finalization(&mut self) {
        self.finalization = CommandFinalization::Completed;
    }
}

impl Drop for UserCommandObservation {
    fn drop(&mut self) {
        tracing::info!(
            command_id = self.id,
            context_id = self.identity.context_id,
            depth = self.identity.depth,
            keys = %self.identity.keys,
            original_command = %self.identity.original_command,
            command = %self.identity.command,
            frame_id = ?self.identity.frame_id.map(|frame_id| frame_id.0),
            buffer_id = ?self.identity.buffer_id.map(|buffer_id| buffer_id.0),
            outcome = %<&'static str>::from(self.outcome),
            finalization = %<&'static str>::from(self.finalization),
            total_wall_us = duration_micros(self.started_at.elapsed()),
            command_execute_wall_us = ?self.command_execute_wall.map(duration_micros),
            "user_command_complete"
        );
    }
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}
