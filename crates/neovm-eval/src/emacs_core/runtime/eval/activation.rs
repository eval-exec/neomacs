//! Dormant selection activation.
//!
//! `OwnedLocalMain` and `OwnedRoutedWorker` are minted only by the
//! transitions on [`SelectionActivation`]. Their capability is the E2 seal:
//! a public struct, a private `PhantomData<Rc<()>>`, no public constructor,
//! and neither `Send`, `Sync`, `Clone`, nor `Copy`. [`InlineSerialMain`]
//! elsewhere stays the only clipboard arm, and it needs no capability.
//! Nothing in this module reads `NEOVM_THREADS`, the environment, or TLS.

use std::marker::PhantomData;
use std::rc::Rc;

/// Non-cloneable evidence that an owned arm was minted by this module.
#[derive(Debug)]
#[doc(hidden)]
pub struct ActivationCapability {
    _confined: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(ActivationCapability: Send, Sync, Clone, Copy);

impl ActivationCapability {
    fn mint() -> Self {
        Self {
            _confined: PhantomData,
        }
    }
}

/// Right to flip an owner's installed arm. Only this module can mint it.
/// Startup or hosted admission will hold the one value. A builtin cannot.
#[derive(Debug)]
#[doc(hidden)]
pub struct ActivationAuthority {
    _confined: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(ActivationAuthority: Send, Sync, Clone, Copy);

impl ActivationAuthority {
    /// # Safety
    /// The caller is this module and keeps the value off every builtin path.
    /// Production has no caller until the runtime activation path is reviewed.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "startup or hosted admission will mint this once; builtins cannot call it"
        )
    )]
    unsafe fn mint() -> Self {
        Self {
            _confined: PhantomData,
        }
    }

    fn accept(&self) {
        let _ = self._confined;
    }

    #[cfg(test)]
    pub(crate) fn mint_for_test() -> Self {
        // SAFETY: this function exists only in test builds.
        unsafe { Self::mint() }
    }
}

/// Local owned driver. Constructible only through [`SelectionActivation::activate_local`].
#[derive(Debug)]
#[doc(hidden)]
pub struct OwnedLocalMain {
    _capability: ActivationCapability,
}

static_assertions::assert_not_impl_any!(OwnedLocalMain: Send, Sync, Clone, Copy);

impl OwnedLocalMain {
    fn mint() -> Self {
        Self {
            _capability: ActivationCapability::mint(),
        }
    }
}

/// Routed owned driver. Constructible only through [`SelectionActivation::activate_routed`].
#[derive(Debug)]
#[doc(hidden)]
pub struct OwnedRoutedWorker {
    _capability: ActivationCapability,
}

static_assertions::assert_not_impl_any!(OwnedRoutedWorker: Send, Sync, Clone, Copy);

impl OwnedRoutedWorker {
    fn mint() -> Self {
        Self {
            _capability: ActivationCapability::mint(),
        }
    }

    /// Consumes the worker. The result is the only proof that this handle
    /// has been joined and drained. `owner` is the address of the activation
    /// that held it, so a proof from another owner cannot clear this one.
    fn join_and_drain(self, generation: u64, owner: usize) -> JoinedDrainedWorker {
        JoinedDrainedWorker {
            _worker: self,
            generation,
            owner,
        }
    }
}

/// Linear proof that a routed worker was taken out of its owner and joined.
/// Rollback consumes it. It cannot be forged outside this module.
#[derive(Debug)]
#[must_use = "the joined worker must be consumed by rollback"]
#[doc(hidden)]
pub struct JoinedDrainedWorker {
    _worker: OwnedRoutedWorker,
    generation: u64,
    owner: usize,
}

static_assertions::assert_not_impl_any!(JoinedDrainedWorker: Send, Sync, Clone, Copy);

/// Which arm the owner currently holds. `Uninstalled` holds neither token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum SelectionPhase {
    Uninstalled,
    Local,
    Routed,
    AwaitingRollback,
}

#[derive(Debug)]
enum InstalledArm {
    Uninstalled,
    Local(OwnedLocalMain),
    Routed(OwnedRoutedWorker),
    AwaitingRollback { generation: u64 },
}

/// Owner-owned selection arm. Activation and rollback take `&mut self`.
/// A fresh value is uninstalled and allocates nothing.
#[derive(Debug)]
#[doc(hidden)]
pub struct SelectionActivation {
    arm: InstalledArm,
    next_generation: u64,
}

static_assertions::assert_not_impl_any!(SelectionActivation: Send, Sync, Clone, Copy);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum ActivateError {
    AlreadyInstalled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum JoinError {
    NotRouted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum RollbackError {
    NotLocal,
    NotAwaitingRollback,
    ForeignJoin,
}

impl SelectionActivation {
    /// No owned arm. The serial installer does not consult this value.
    #[doc(hidden)]
    pub const fn dormant() -> Self {
        Self {
            arm: InstalledArm::Uninstalled,
            next_generation: 0,
        }
    }

    #[doc(hidden)]
    pub fn phase(&self) -> SelectionPhase {
        match self.arm {
            InstalledArm::Uninstalled => SelectionPhase::Uninstalled,
            InstalledArm::Local(_) => SelectionPhase::Local,
            InstalledArm::Routed(_) => SelectionPhase::Routed,
            InstalledArm::AwaitingRollback { .. } => SelectionPhase::AwaitingRollback,
        }
    }

    #[doc(hidden)]
    pub fn activate_local(&mut self, authority: &ActivationAuthority) -> Result<(), ActivateError> {
        authority.accept();
        if !matches!(self.arm, InstalledArm::Uninstalled) {
            return Err(ActivateError::AlreadyInstalled);
        }
        self.arm = InstalledArm::Local(OwnedLocalMain::mint());
        Ok(())
    }

    #[doc(hidden)]
    pub fn activate_routed(
        &mut self,
        authority: &ActivationAuthority,
    ) -> Result<(), ActivateError> {
        authority.accept();
        if !matches!(self.arm, InstalledArm::Uninstalled) {
            return Err(ActivateError::AlreadyInstalled);
        }
        self.arm = InstalledArm::Routed(OwnedRoutedWorker::mint());
        Ok(())
    }

    /// Takes the routed worker out. The owner then holds no worker; the
    /// returned proof is the joined handle.
    #[doc(hidden)]
    pub fn join_routed(&mut self) -> Result<JoinedDrainedWorker, JoinError> {
        let owner = std::ptr::from_mut(self).addr();
        match std::mem::replace(&mut self.arm, InstalledArm::Uninstalled) {
            InstalledArm::Routed(worker) => {
                self.next_generation = self.next_generation.wrapping_add(1);
                let generation = self.next_generation;
                let joined = worker.join_and_drain(generation, owner);
                self.arm = InstalledArm::AwaitingRollback { generation };
                Ok(joined)
            }
            other => {
                self.arm = other;
                Err(JoinError::NotRouted)
            }
        }
    }

    #[doc(hidden)]
    pub fn rollback_local(&mut self) -> Result<(), RollbackError> {
        match std::mem::replace(&mut self.arm, InstalledArm::Uninstalled) {
            InstalledArm::Local(local) => {
                drop(local);
                Ok(())
            }
            InstalledArm::Uninstalled => Ok(()),
            other => {
                self.arm = other;
                Err(RollbackError::NotLocal)
            }
        }
    }

    /// Consumes `joined`. On success the arm is [`SelectionPhase::Uninstalled`],
    /// which contains no hosted worker.
    #[doc(hidden)]
    pub fn rollback_routed(
        &mut self,
        joined: JoinedDrainedWorker,
    ) -> Result<(), (RollbackError, JoinedDrainedWorker)> {
        let owner = std::ptr::from_mut(self).addr();
        match self.arm {
            InstalledArm::AwaitingRollback { generation }
                if generation == joined.generation && owner == joined.owner =>
            {
                drop(joined);
                self.arm = InstalledArm::Uninstalled;
                Ok(())
            }
            InstalledArm::AwaitingRollback { .. } => Err((RollbackError::ForeignJoin, joined)),
            _ => Err((RollbackError::NotAwaitingRollback, joined)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority() -> ActivationAuthority {
        ActivationAuthority::mint_for_test()
    }

    #[test]
    fn dormant_arm_is_uninstalled() {
        let arm = SelectionActivation::dormant();
        assert_eq!(arm.phase(), SelectionPhase::Uninstalled);
    }

    #[test]
    fn local_activation_and_rollback_clear_the_arm() {
        let mut arm = SelectionActivation::dormant();
        arm.activate_local(&authority()).unwrap();
        assert_eq!(arm.phase(), SelectionPhase::Local);
        arm.rollback_local().unwrap();
        assert_eq!(arm.phase(), SelectionPhase::Uninstalled);
    }

    #[test]
    fn second_activation_is_refused_until_rollback() {
        let mut arm = SelectionActivation::dormant();
        arm.activate_local(&authority()).unwrap();
        assert_eq!(
            arm.activate_routed(&authority()),
            Err(ActivateError::AlreadyInstalled)
        );
        assert_eq!(
            arm.activate_local(&authority()),
            Err(ActivateError::AlreadyInstalled)
        );
        assert!(matches!(arm.join_routed(), Err(JoinError::NotRouted)));
        arm.rollback_local().unwrap();
        arm.activate_routed(&authority()).unwrap();
        assert_eq!(arm.phase(), SelectionPhase::Routed);
    }

    #[test]
    fn routed_rollback_consumes_the_joined_handle() {
        let mut arm = SelectionActivation::dormant();
        arm.activate_routed(&authority()).unwrap();
        assert_eq!(arm.rollback_local(), Err(RollbackError::NotLocal));
        assert_eq!(arm.phase(), SelectionPhase::Routed);
        let joined = arm.join_routed().unwrap();
        assert_eq!(arm.phase(), SelectionPhase::AwaitingRollback);
        assert_eq!(
            arm.activate_local(&authority()),
            Err(ActivateError::AlreadyInstalled)
        );
        assert!(matches!(arm.join_routed(), Err(JoinError::NotRouted)));
        arm.rollback_routed(joined).unwrap();
        assert_eq!(arm.phase(), SelectionPhase::Uninstalled);
        arm.rollback_local().unwrap();
    }

    #[test]
    fn foreign_joined_handle_does_not_clear_another_owner() {
        let mut first = SelectionActivation::dormant();
        let mut second = SelectionActivation::dormant();
        first.activate_routed(&authority()).unwrap();
        second.activate_routed(&authority()).unwrap();
        let first_joined = first.join_routed().unwrap();
        let second_joined = second.join_routed().unwrap();
        let (error, returned) = first.rollback_routed(second_joined).unwrap_err();
        assert_eq!(error, RollbackError::ForeignJoin);
        assert_eq!(first.phase(), SelectionPhase::AwaitingRollback);
        first.rollback_routed(first_joined).unwrap();
        second.rollback_routed(returned).unwrap();
        assert_eq!(first.phase(), SelectionPhase::Uninstalled);
        assert_eq!(second.phase(), SelectionPhase::Uninstalled);
    }

    #[test]
    fn live_routed_worker_has_no_rollback_proof() {
        let mut arm = SelectionActivation::dormant();
        arm.activate_routed(&authority()).unwrap();
        let mut other = SelectionActivation::dormant();
        other.activate_routed(&authority()).unwrap();
        let foreign = other.join_routed().unwrap();
        let (error, foreign) = arm.rollback_routed(foreign).unwrap_err();
        assert_eq!(error, RollbackError::NotAwaitingRollback);
        assert_eq!(arm.phase(), SelectionPhase::Routed);
        other.rollback_routed(foreign).unwrap();
        let joined = arm.join_routed().unwrap();
        arm.rollback_routed(joined).unwrap();
    }
}
