//! `&mut Context` transitions for the dormant selection arm.
//!
//! The arm lives on this owner's slab. A dormant slot is uninstalled and
//! stays uninstalled until an [`ActivationAuthority`] holder runs activation.
//! The clipboard stubs do not call these methods, and they do not read
//! `NEOVM_THREADS`, the environment, or TLS.

use super::Context;
use super::activation::{
    ActivateError, ActivationAuthority, JoinError, JoinedDrainedWorker, RollbackError,
    SelectionPhase,
};

impl Context {
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
    use super::Context;

    fn authority() -> ActivationAuthority {
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
        ctx.activate_selection_local(&authority()).unwrap();
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
        ctx.activate_selection_routed(&authority()).unwrap();
        assert_eq!(ctx.selection_phase(), SelectionPhase::Routed);
        let joined = ctx.join_selection_routed().unwrap();
        assert_eq!(ctx.selection_phase(), SelectionPhase::AwaitingRollback);
        ctx.rollback_selection_routed(joined).unwrap();
        assert_eq!(ctx.selection_phase(), SelectionPhase::Uninstalled);
    }
}
