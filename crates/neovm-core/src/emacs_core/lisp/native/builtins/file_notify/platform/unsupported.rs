//! Inert adapter for targets without native local file notification.
//!
//! No Lisp subrs accept [`UnsupportedRequest`], so the uninhabited request and
//! event types make accidental native use impossible while allowing common
//! evaluator reset, GC-root, and event-drain hooks to remain portable.

use super::super::model::{
    Backend, BackendEvent, DrainBatch, RemoveWatchOutcome, WatchId, WatchRegistration,
};
use crate::emacs_core::error::Flow;
use crate::emacs_core::process::WaitNotifier;
use crate::emacs_core::value::Value;
use std::path::Path;

#[derive(Default)]
pub(in super::super) struct UnsupportedBackend;

pub(in super::super) enum UnsupportedRequest {}
pub(in super::super) enum UnsupportedEvent {}

impl BackendEvent for UnsupportedEvent {
    fn watch_id(&self) -> &WatchId {
        match *self {}
    }

    fn into_lisp(
        self,
        _ctx: &crate::emacs_core::eval::Context,
        _registration: WatchRegistration,
    ) -> Value {
        match self {}
    }
}

impl Backend for UnsupportedBackend {
    type Request = UnsupportedRequest;
    type Event = UnsupportedEvent;

    fn add_watch(
        &mut self,
        _path: &Path,
        request: Self::Request,
        _notifier: Option<WaitNotifier>,
    ) -> Result<WatchId, Flow> {
        match request {}
    }

    fn remove_watch(&mut self, _watch_id: &WatchId) -> RemoveWatchOutcome {
        RemoveWatchOutcome::NotFound
    }

    fn valid_p(&self, _watch_id: &WatchId) -> bool {
        false
    }

    fn drain_events(&mut self) -> Result<DrainBatch<Self::Event>, Flow> {
        Ok(DrainBatch {
            events: Vec::new(),
            terminated: Vec::new(),
            failure: None,
        })
    }

    fn has_watches(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "tests/unsupported.rs"]
mod tests;
