//! Owned render-command publication; the raw channel stays in this module.
//!
//! This is a native host projection, not a Lisp-worker host capability. Its
//! callers belong to the frontend effect owners. A future hosted worker gets
//! HostClient instead. Clones share the existing channel; no TLS or extra state
//! is introduced and the ordinary publication operations remain identical.

use super::RenderCommand;
use crossbeam_channel::{Receiver, SendError, Sender, TrySendError};
use std::num::NonZeroUsize;

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

    /// Publish once; a disconnected channel returns the uncommitted command.
    #[inline(always)]
    pub fn send(&self, command: RenderCommand) -> Result<(), SendError<RenderCommand>> {
        self.sender.send(command)
    }

    /// Publish without blocking, preserving the channel's existing semantics.
    #[inline(always)]
    pub fn try_send(&self, command: RenderCommand) -> Result<(), TrySendError<RenderCommand>> {
        self.sender.try_send(command)
    }
}

static_assertions::assert_eq_size!(RenderCommandSender, Sender<RenderCommand>);
static_assertions::assert_eq_align!(RenderCommandSender, Sender<RenderCommand>);
static_assertions::assert_impl_all!(RenderCommandSender: Send, Sync, Clone, std::fmt::Debug);
