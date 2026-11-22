//! `&mut Context` transitions for the selection arm.
//!
//! The arm lives on this owner's slab. A dormant slot is uninstalled and
//! stays uninstalled until an [`ActivationAuthority`] holder runs activation.
//! The hosted flip installs the local arm from `Context::set_display_host`
//! (`activate_host_session`); the clipboard stubs read the installed phase
//! through the builtins' `selection_step::installed`. Nothing here reads
//! `NEOVM_THREADS`, the environment, or TLS.

use super::Context;
use super::activation::{
    ActivateError, ActivationAuthority, JoinError, JoinedDrainedWorker, RollbackError,
    SelectionPhase,
};

/// Why a hosted session did not consume the World's activation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::emacs_core::eval) enum HostSessionActivationError {
    /// The World does not run Green Lisp threads; the serial installer stays.
    NotGreen,
    /// Only the main owner may consume the World's one authority.
    NotMainOwner,
}

impl Context {
    /// Cold hosted activation: the cut-4 flip.
    ///
    /// Runs at host attach. Only a Green World activates, so the default
    /// GUI/TTY path stays on the serial installer until S3. The main owner is
    /// the only consumer of the World's one authority: the value authorizes
    /// this one installation and is then dropped; the installed arm holds no
    /// host instance, so a second attach keeps the arm and sees the current
    /// host. A worker is refused before the take.
    #[cold]
    pub(in crate::emacs_core::eval) fn activate_host_session(
        &mut self,
    ) -> Result<(), HostSessionActivationError> {
        if self.threading_mode() != super::admission::ThreadingMode::Green {
            return Err(HostSessionActivationError::NotGreen);
        }
        if matches!(self.green_key(), Some(super::green::ThreadKey::Worker(_))) {
            return Err(HostSessionActivationError::NotMainOwner);
        }
        match self.selection_phase() {
            SelectionPhase::Local => return Ok(()),
            // Only a routed owner can reach these; a routed owner is never
            // this main activation path.
            SelectionPhase::Routed | SelectionPhase::AwaitingRollback => return Ok(()),
            SelectionPhase::Uninstalled => {}
        }
        let Some(authority) = self.take_activation_authority() else {
            // The one authority was consumed by another owner, or this is a
            // second World path. Only the main owner may consume it.
            return Err(HostSessionActivationError::NotMainOwner);
        };
        match self.activate_selection_local(&authority) {
            Ok(()) | Err(ActivateError::AlreadyInstalled) => Ok(()),
        }
    }

    /// Installed phase, or [`SelectionPhase::Uninstalled`] when this owner
    /// has no slab. Does not allocate.
    #[doc(hidden)]
    pub fn selection_phase(&self) -> SelectionPhase {
        self.owner
            .slab()
            .map(|slab| slab.selection_activation.phase())
            .unwrap_or(SelectionPhase::Uninstalled)
    }

    #[doc(hidden)]
    pub fn activate_selection_local(
        &mut self,
        authority: &ActivationAuthority,
    ) -> Result<(), ActivateError> {
        self.ensure_owner_slab()
            .selection_activation
            .activate_local(authority)
    }

    #[doc(hidden)]
    pub fn activate_selection_routed(
        &mut self,
        authority: &ActivationAuthority,
    ) -> Result<(), ActivateError> {
        self.ensure_owner_slab()
            .selection_activation
            .activate_routed(authority)
    }

    #[doc(hidden)]
    pub fn join_selection_routed(&mut self) -> Result<JoinedDrainedWorker, JoinError> {
        match self.owner.slab_mut() {
            Some(slab) => slab.selection_activation.join_routed(),
            None => Err(JoinError::NotRouted),
        }
    }

    #[doc(hidden)]
    pub fn rollback_selection_local(&mut self) -> Result<(), RollbackError> {
        match self.owner.slab_mut() {
            Some(slab) => slab.selection_activation.rollback_local(),
            None => Ok(()),
        }
    }

    #[doc(hidden)]
    pub fn rollback_selection_routed(
        &mut self,
        joined: JoinedDrainedWorker,
    ) -> Result<(), (RollbackError, JoinedDrainedWorker)> {
        match self.owner.slab_mut() {
            Some(slab) => slab.selection_activation.rollback_routed(joined),
            None => Err((RollbackError::NotAwaitingRollback, joined)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::activation::{ActivationAuthority, SelectionPhase};
    use super::{Context, HostSessionActivationError};

    fn activation_gate() -> ActivationAuthority {
        ActivationAuthority::mint_for_test()
    }

    #[test]
    fn fresh_context_does_not_allocate_an_owner_slab() {
        let ctx = Context::new(crate::test_utils::unit_runtime());
        assert!(ctx.owner.is_dormant());
        assert_eq!(ctx.selection_phase(), SelectionPhase::Uninstalled);
        assert!(ctx.owner.is_dormant());
    }

    #[test]
    fn local_activation_lives_on_the_owner_slab() {
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        ctx.activate_selection_local(&activation_gate()).unwrap();
        assert!(!ctx.owner.is_dormant());
        assert_eq!(ctx.selection_phase(), SelectionPhase::Local);
        ctx.rollback_selection_local().unwrap();
        assert_eq!(ctx.selection_phase(), SelectionPhase::Uninstalled);
        assert!(!ctx.owner.is_dormant());
    }

    #[test]
    fn routed_rollback_leaves_no_worker_on_the_owner() {
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        assert!(matches!(
            ctx.join_selection_routed(),
            Err(super::super::activation::JoinError::NotRouted)
        ));
        ctx.activate_selection_routed(&activation_gate()).unwrap();
        assert_eq!(ctx.selection_phase(), SelectionPhase::Routed);
        let joined = ctx.join_selection_routed().unwrap();
        assert_eq!(ctx.selection_phase(), SelectionPhase::AwaitingRollback);
        ctx.rollback_selection_routed(joined).unwrap();
        assert_eq!(ctx.selection_phase(), SelectionPhase::Uninstalled);
    }

    #[test]
    fn world_activation_authority_is_taken_once() {
        let ctx = Context::new(crate::test_utils::unit_runtime());
        assert!(
            ctx.take_activation_authority().is_some(),
            "the World mints its one authority"
        );
        assert!(
            ctx.take_activation_authority().is_none(),
            "a second take returns None"
        );
    }

    #[test]
    fn default_mode_hosted_stays_uninstalled() {
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        assert_eq!(
            ctx.activate_host_session(),
            Err(HostSessionActivationError::NotGreen)
        );
        assert_eq!(ctx.selection_phase(), SelectionPhase::Uninstalled);
        assert!(
            ctx.take_activation_authority().is_some(),
            "a refused activation consumes nothing"
        );
    }

    #[test]
    fn green_cold_activation_installs_local_and_reattach_keeps_it() {
        use crate::emacs_core::eval::admission::ThreadingMode;

        let mut ctx =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        assert_eq!(ctx.activate_host_session(), Ok(()));
        assert_eq!(ctx.selection_phase(), SelectionPhase::Local);
        // Re-attach: the one authority is gone, the arm stays. The arm holds
        // no host instance, so the current host is whatever the Context has.
        assert!(ctx.take_activation_authority().is_none());
        assert_eq!(ctx.activate_host_session(), Ok(()));
        assert_eq!(ctx.selection_phase(), SelectionPhase::Local);
        ctx.display_host = None;
        assert_eq!(ctx.activate_host_session(), Ok(()));
        assert_eq!(ctx.selection_phase(), SelectionPhase::Local);
    }

    #[test]
    fn worker_context_cannot_activate() {
        use crate::emacs_core::eval::admission::ThreadingMode;

        let mut ctx =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        // Model the session: the main owner consumes the World's one
        // authority. A worker that reaches this path sees an uninstalled
        // phase and an empty slot, and is refused rather than re-minting.
        assert!(ctx.take_activation_authority().is_some());
        assert_eq!(
            ctx.activate_host_session(),
            Err(HostSessionActivationError::NotMainOwner)
        );
        assert_eq!(ctx.selection_phase(), SelectionPhase::Uninstalled);
    }
}
