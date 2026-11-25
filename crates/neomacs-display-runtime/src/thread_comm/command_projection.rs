//! Owned render-command publication; the raw channel stays in this module.
//!
//! This is a native host projection, not a Lisp-worker host capability. Its
//! callers belong to the frontend effect owners. A future hosted worker gets
//! HostClient instead. Clones share the existing channel; no TLS or extra state
//! is introduced and the ordinary publication operations remain identical.

use super::RenderCommand;
use crossbeam_channel::{Receiver, SendError, SendTimeoutError, Sender, TrySendError};
use std::num::NonZeroUsize;
use std::time::Duration;

/// Outcome of a liveness-bounded publish.
///
/// Unlike the plain [`RenderCommandSender::send`], the caller keeps the
/// command on timeout, so it can re-check whether the renderer is still alive
/// and either retry or give up without losing the command silently.
pub enum SendTimeoutOutcome {
    /// The command is committed to the channel in FIFO order.
    Sent,
    /// The timeout elapsed before the channel had room; the command is
    /// returned uncommitted.
    TimedOut(RenderCommand),
    /// No receiver remains; the command is returned uncommitted.
    Disconnected(RenderCommand),
}

/// Opaque publication endpoint for the owned native command protocol.
///
/// No raw sender constructor, getter, conversion or Deref is exposed. Native
/// effect owners can publish commands without obtaining channel internals.
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct RenderCommandSender {
    sender: Sender<RenderCommand>,
}

impl RenderCommandSender {
    pub(super) fn default_channel() -> (Self, Receiver<RenderCommand>) {
        let (sender, receiver) = crossbeam_channel::bounded(super::COMMAND_CHANNEL_CAPACITY);
        (Self { sender }, receiver)
    }

    /// Construct an owned command channel with a nonzero bounded capacity.
    pub fn bounded(capacity: NonZeroUsize) -> (Self, Receiver<RenderCommand>) {
        let (sender, receiver) = crossbeam_channel::bounded(capacity.get());
        (Self { sender }, receiver)
    }

    /// Construct an owned command channel without a capacity bound.
    pub fn unbounded() -> (Self, Receiver<RenderCommand>) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (Self { sender }, receiver)
    }

    /// Replace a frame's complete desired auxiliary snapshot. Only an empty
    /// slot queues a wake. Notification and insertion share the lock, so the
    /// receiver cannot consume a wake before its payload exists. The caller
    /// notifies the native event loop only after this method returns. `true`
    /// means a new wake was queued; `false` replaced an already pending slot.
    /// A full/disconnected queue returns the uncommitted payload unchanged.
    pub fn publish_scroll_coverage(
        &self,
        pending: &std::sync::Mutex<
            std::collections::HashMap<
                neomacs_display_protocol::DisplayFrameId,
                neomacs_display_protocol::scroll_coverage::AuxiliaryScrollCoverage,
            >,
        >,
        update: neomacs_display_protocol::scroll_coverage::AuxiliaryScrollCoverage,
    ) -> Result<
        bool,
        TrySendError<neomacs_display_protocol::scroll_coverage::AuxiliaryScrollCoverage>,
    > {
        let mut pending = pending.lock().unwrap();
        let frame = update.frame;
        if let std::collections::hash_map::Entry::Occupied(mut entry) = pending.entry(frame) {
            entry.insert(update);
            return Ok(false);
        }
        // Never block with this guard: the renderer can be waiting to take an
        // earlier frame's slot while the bounded command queue is full.
        match self.try_send(RenderCommand::Window(
            super::WindowCommand::PublishScrollCoverage { frame },
        )) {
            Ok(()) => {
                pending.insert(frame, update);
                Ok(true)
            }
            Err(TrySendError::Full(_)) => Err(TrySendError::Full(update)),
            Err(TrySendError::Disconnected(_)) => Err(TrySendError::Disconnected(update)),
        }
    }

    /// Publish once, waiting for room when the bounded channel is full; a
    /// disconnected channel returns the uncommitted command.
    #[inline(always)]
    pub fn send(&self, command: RenderCommand) -> Result<(), SendError<RenderCommand>> {
        self.sender.send(command)
    }

    /// Publish without blocking, preserving the channel's existing semantics.
    #[inline(always)]
    pub fn try_send(&self, command: RenderCommand) -> Result<(), TrySendError<RenderCommand>> {
        self.sender.try_send(command)
    }

    /// Publish once, waiting at most `timeout` for room. Used by a caller
    /// that must not stall Lisp forever on a wedged renderer: a timeout
    /// returns the command so the caller can observe liveness and retry or
    /// report failure. This is not the old ordering deferral: the command is
    /// never handed to another thread or a different path, and on success it
    /// is in the same FIFO position a blocking send would have taken.
    pub fn send_timeout(&self, command: RenderCommand, timeout: Duration) -> SendTimeoutOutcome {
        match self.sender.send_timeout(command, timeout) {
            Ok(()) => SendTimeoutOutcome::Sent,
            Err(SendTimeoutError::Timeout(command)) => SendTimeoutOutcome::TimedOut(command),
            Err(SendTimeoutError::Disconnected(command)) => {
                SendTimeoutOutcome::Disconnected(command)
            }
        }
    }
}

static_assertions::assert_eq_size!(RenderCommandSender, Sender<RenderCommand>);
static_assertions::assert_eq_align!(RenderCommandSender, Sender<RenderCommand>);
static_assertions::assert_impl_all!(RenderCommandSender: Send, Sync, Clone, std::fmt::Debug);
