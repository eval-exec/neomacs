//! Neo-term: GPU-accelerated terminal emulator for Neomacs.
//!
//! Uses `rio-vt` for VT parsing and terminal state,
//! renders cells directly via the wgpu pipeline.

pub mod colors;
pub mod content;
pub mod view;

pub use content::TerminalContent;
pub use neovm_core::emacs_core::display_host::{
    TerminalDisplayTarget, TerminalFloatPlacement, TerminalGridSize, TerminalId,
};
pub use view::{TerminalManager, TerminalView};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use parking_lot::FairMutex;
use rio_vt::crosswords::Crosswords;

type LiveTerminal = Arc<FairMutex<Crosswords<view::NeomacsEventProxy>>>;

#[derive(Clone)]
enum TerminalState {
    Pending,
    Live(LiveTerminal),
    Failed(Arc<str>),
    Destroying(Option<LiveTerminal>),
}

/// Per-editor terminal registry shared by the Lisp and render threads.
///
/// The registry is intentionally opaque: users cannot mistake a reserved ID
/// for a live terminal or remove a terminal without completing its lifecycle.
#[derive(Clone, Default)]
pub struct SharedTerminals {
    inner: Arc<Mutex<HashMap<TerminalId, TerminalState>>>,
}

/// A reserved terminal ID. Dropping an uncommitted reservation rolls it back,
/// so a failed renderer-channel send cannot leave a phantom terminal behind.
#[must_use = "commit the reservation after the create command is queued"]
pub struct TerminalReservation {
    terminals: SharedTerminals,
    id: TerminalId,
    committed: bool,
}

impl TerminalReservation {
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for TerminalReservation {
    fn drop(&mut self) {
        if !self.committed {
            self.terminals.remove_if_pending(self.id);
        }
    }
}

/// A transition from an active terminal to `Destroying`. Dropping an
/// uncommitted transition restores the exact previous state.
#[must_use = "commit the transition after the destroy command is queued"]
pub struct TerminalDestroyTransition {
    terminals: SharedTerminals,
    id: TerminalId,
    previous: Option<TerminalState>,
}

impl TerminalDestroyTransition {
    pub fn commit(mut self) {
        self.previous = None;
    }
}

impl Drop for TerminalDestroyTransition {
    fn drop(&mut self) {
        let Some(previous) = self.previous.take() else {
            return;
        };
        self.terminals.lock().insert(self.id, previous);
    }
}

/// Create one terminal registry to share between the Lisp display host and
/// the render loop. Keeping this state explicit avoids a process-global
/// registry and lets independent editor instances remain isolated.
pub fn new_shared_terminals() -> SharedTerminals {
    SharedTerminals::default()
}

impl SharedTerminals {
    fn lock(&self) -> MutexGuard<'_, HashMap<TerminalId, TerminalState>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn reserve(&self, id: TerminalId) -> Result<TerminalReservation, String> {
        let mut terminals = self.lock();
        if terminals.contains_key(&id) {
            return Err(format!("neo-term terminal id {id} is already reserved"));
        }
        terminals.insert(id, TerminalState::Pending);
        drop(terminals);
        Ok(TerminalReservation {
            terminals: self.clone(),
            id,
            committed: false,
        })
    }

    fn remove_if_pending(&self, id: TerminalId) {
        let mut terminals = self.lock();
        if matches!(terminals.get(&id), Some(TerminalState::Pending)) {
            terminals.remove(&id);
        }
    }

    pub(crate) fn mark_live(&self, id: TerminalId, terminal: LiveTerminal) -> Result<(), String> {
        let mut terminals = self.lock();
        let state = terminals
            .get_mut(&id)
            .ok_or_else(|| format!("unknown neo-term terminal id {id}"))?;
        match state {
            TerminalState::Pending => {
                *state = TerminalState::Live(terminal);
                Ok(())
            }
            TerminalState::Destroying(slot) if slot.is_none() => {
                *slot = Some(terminal);
                Ok(())
            }
            TerminalState::Live(_) | TerminalState::Failed(_) | TerminalState::Destroying(_) => {
                Err(format!("neo-term terminal {id} is not awaiting creation"))
            }
        }
    }

    pub(crate) fn mark_failed(&self, id: TerminalId, error: impl Into<Arc<str>>) {
        let mut terminals = self.lock();
        if matches!(
            terminals.get(&id),
            Some(TerminalState::Pending | TerminalState::Destroying(None))
        ) {
            terminals.insert(id, TerminalState::Failed(error.into()));
        }
    }

    pub(crate) fn mark_destroy_failed(&self, id: TerminalId, error: impl Into<Arc<str>>) {
        let mut terminals = self.lock();
        if matches!(terminals.get(&id), Some(TerminalState::Destroying(_))) {
            terminals.insert(id, TerminalState::Failed(error.into()));
        }
    }

    pub fn require_active(&self, id: TerminalId) -> Result<(), String> {
        match self.lock().get(&id) {
            Some(TerminalState::Pending | TerminalState::Live(_)) => Ok(()),
            Some(TerminalState::Failed(error)) => {
                Err(format!("neo-term terminal {id} creation failed: {error}"))
            }
            Some(TerminalState::Destroying(_)) => {
                Err(format!("neo-term terminal {id} is being destroyed"))
            }
            None => Err(format!("unknown neo-term terminal id {id}")),
        }
    }

    pub fn begin_destroy(&self, id: TerminalId) -> Result<TerminalDestroyTransition, String> {
        let mut terminals = self.lock();
        let previous = terminals
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("unknown neo-term terminal id {id}"))?;
        let terminal = match &previous {
            TerminalState::Pending => None,
            TerminalState::Live(terminal) => Some(Arc::clone(terminal)),
            // Failed creation still owns a reserved ID. Allow the evaluator's
            // failure hook to retire that record through the normal destroy
            // acknowledgement path, but never treat it as active.
            TerminalState::Failed(_) => None,
            TerminalState::Destroying(_) => {
                return Err(format!("neo-term terminal {id} is being destroyed"));
            }
        };
        terminals.insert(id, TerminalState::Destroying(terminal));
        drop(terminals);
        Ok(TerminalDestroyTransition {
            terminals: self.clone(),
            id,
            previous: Some(previous),
        })
    }

    pub(crate) fn complete_destroy(&self, id: TerminalId) {
        self.lock().remove(&id);
    }

    /// Snapshot visible text without exposing rio-vt's grid across the host
    /// boundary. `None` means creation is still pending; failure and unknown
    /// IDs remain distinct errors.
    pub fn visible_text(&self, id: TerminalId) -> Result<Option<String>, String> {
        let terminal = match self.lock().get(&id) {
            Some(TerminalState::Pending) => return Ok(None),
            Some(TerminalState::Live(terminal)) => Arc::clone(terminal),
            Some(TerminalState::Failed(error)) => {
                return Err(format!("neo-term terminal {id} creation failed: {error}"));
            }
            Some(TerminalState::Destroying(Some(terminal))) => Arc::clone(terminal),
            Some(TerminalState::Destroying(None)) => return Ok(None),
            None => return Err(format!("unknown neo-term terminal id {id}")),
        };
        let terminal = terminal.lock();
        let cols = terminal.columns();
        let rows = terminal.screen_lines();
        if cols == 0 || rows == 0 {
            return Ok(Some(String::new()));
        }
        Ok(Some(content::extract_text(
            &*terminal,
            0,
            0,
            rows - 1,
            cols - 1,
        )))
    }
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    fn id() -> TerminalId {
        TerminalId::new(41).expect("nonzero terminal id")
    }

    #[test]
    fn uncommitted_create_reservation_cannot_leave_a_pending_terminal() {
        let terminals = new_shared_terminals();
        let reservation = terminals.reserve(id()).expect("reserve terminal");
        assert_eq!(
            terminals.visible_text(id()).expect("pending terminal"),
            None
        );

        drop(reservation);

        assert!(
            terminals
                .visible_text(id())
                .unwrap_err()
                .contains("unknown neo-term terminal id")
        );
    }

    #[test]
    fn creation_failure_is_not_observable_as_an_eternally_pending_terminal() {
        let terminals = new_shared_terminals();
        terminals.reserve(id()).expect("reserve terminal").commit();

        terminals.mark_failed(id(), "shell executable not found");

        let error = terminals.visible_text(id()).unwrap_err();
        assert!(error.contains("creation failed"), "{error}");
        assert!(error.contains("shell executable not found"), "{error}");
        assert_eq!(terminals.require_active(id()).unwrap_err(), error);

        terminals
            .begin_destroy(id())
            .expect("failed terminal remains destroyable")
            .commit();
        terminals.complete_destroy(id());
        assert!(terminals.visible_text(id()).is_err());
    }

    #[test]
    fn failed_destroy_queueing_restores_the_previous_active_state() {
        let terminals = new_shared_terminals();
        terminals.reserve(id()).expect("reserve terminal").commit();
        let transition = terminals.begin_destroy(id()).expect("begin destroy");
        assert!(
            terminals
                .require_active(id())
                .unwrap_err()
                .contains("being destroyed")
        );

        drop(transition);

        terminals
            .require_active(id())
            .expect("pending state restored");
    }

    #[test]
    fn committed_destroy_stays_typed_until_the_renderer_acknowledges_it() {
        let terminals = new_shared_terminals();
        terminals.reserve(id()).expect("reserve terminal").commit();
        terminals
            .begin_destroy(id())
            .expect("begin destroy")
            .commit();

        assert!(
            terminals
                .require_active(id())
                .unwrap_err()
                .contains("being destroyed")
        );
        assert_eq!(terminals.visible_text(id()).expect("destroy pending"), None);

        terminals.complete_destroy(id());
        assert!(
            terminals
                .visible_text(id())
                .unwrap_err()
                .contains("unknown neo-term terminal id")
        );
    }
}
