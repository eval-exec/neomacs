//! Cold loan seam. Value-layer code never names an evaluator manager or its
//! scheduling token. Eval's World owner is the trusted implementation; this
//! module only forwards that owner.
//!
//! A private supertrait cannot be the seal. The owner type lives in another
//! crate, and a seal defined here would be unimplementable there. The trait
//! is therefore the same cross-crate shape as [`super::EraKeeper`]: public,
//! `unsafe`, and `#[doc(hidden)]`. Minting the identity words is additionally
//! confined to the eval World owner by the architecture seam guard.

use super::{AllStopped, HeapIdentity, RegistrationError, RendezvousError};
use crate::tagged::header::MarkerObj;
use crate::tagged::value::TaggedValue;
use std::any::Any;
use std::ptr::NonNull;

/// Identity of one retained World allocation.
///
/// The word is metadata. It is not a proof of allocation lifetime, not a
/// symbol-backing capability, and not interchangeable with [`HeapIdentity`]
/// or with the test-support pointer newtype of the same short name. Two
/// values compare equal when they name the same address. Copying is not
/// provided: a matching integer is minted only by the World owner, and
/// equality is a dynamic designator check.
#[derive(Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct WorldIdentity(usize);

impl WorldIdentity {
    /// # Safety
    /// `address` is the identity of a World allocation the caller retains
    /// for as long as a designator that stores this value is used to
    /// recognize that allocation. The word is not a pointer permission.
    /// Nothing may dereference it through this type.
    #[doc(hidden)]
    pub unsafe fn new(address: usize) -> Self {
        Self(address)
    }
}

/// Original manager allocation designators.
///
/// These name one retained World's allocation and its obarray and projection
/// managers. They do not borrow those managers and they do not authorize a
/// stopped census. The World word is not [`Clone`]: each call mints a fresh
/// token, and two tokens for the same allocation compare equal by address.
#[derive(Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct SymbolRootDesignators {
    world: WorldIdentity,
    obarray: NonNull<()>,
    projections: NonNull<()>,
}

impl SymbolRootDesignators {
    /// # Safety
    /// The three words name one retained World's allocation and that
    /// allocation's original obarray and projection managers. Storing them
    /// grants no borrow and no permission to dereference either manager.
    #[doc(hidden)]
    pub unsafe fn new(
        world: WorldIdentity,
        obarray: NonNull<()>,
        projections: NonNull<()>,
    ) -> Self {
        Self {
            world,
            obarray,
            projections,
        }
    }

    #[doc(hidden)]
    pub fn world(&self) -> &WorldIdentity {
        &self.world
    }

    #[doc(hidden)]
    pub fn obarray(&self) -> NonNull<()> {
        self.obarray
    }

    #[doc(hidden)]
    pub fn projections(&self) -> NonNull<()> {
        self.projections
    }
}

/// Retains the original World allocation and its serial scheduling authority.
///
/// # Safety
/// The implementor retains that allocation for the whole life of the value
/// and is the unique serial authority for it. `symbol_root_designators`
/// names that allocation's initialized managers and nothing else.
/// `into_retained_owner` transfers that same unique owner, without
/// replacement, duplication, or an early drop. A forwarding implementation
/// may wrap another implementor only when it delegates every method to that
/// same owner. This is not a constructor from identity words. Only the eval
/// World owner and the forwarding adapter in this module implement it.
#[doc(hidden)]
pub unsafe trait EraWorldLoan: Send + 'static {
    fn held_here(&self) -> bool;
    fn parallel_heap_identity(&self) -> HeapIdentity;
    fn parallel_epoch_retention(
        &self,
    ) -> Result<super::super::FacadeEpochRetention, super::super::FacadeMarkExclusionError>;
    fn symbol_root_designators(&self) -> SymbolRootDesignators;
    fn check_symbol_root_designators(
        &self,
        roots: &SymbolRootDesignators,
    ) -> Result<(), RegistrationError>;
    fn collect_parallel_world_roots(
        &self,
        stopped: &mut AllStopped<'_>,
        out: &mut Vec<TaggedValue>,
    ) -> Result<Vec<*mut *mut MarkerObj>, RendezvousError>;
    fn into_retained_owner(self: Box<Self>) -> Box<dyn Any + Send>;
}

/// Affine retention of one implementor. Not a manager view and not a heap view.
#[must_use]
#[doc(hidden)]
pub struct WorldLoan(Box<dyn EraWorldLoan>);

impl WorldLoan {
    #[doc(hidden)]
    pub fn new(owner: impl EraWorldLoan) -> Self {
        Self(Box::new(owner))
    }
}

impl std::fmt::Debug for WorldLoan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("WorldLoan").finish_non_exhaustive()
    }
}

// SAFETY: the wrapper retains the same owner it was given and forwards every
// operation to that owner, including the final transfer.
unsafe impl EraWorldLoan for WorldLoan {
    fn held_here(&self) -> bool {
        self.0.held_here()
    }

    fn parallel_heap_identity(&self) -> HeapIdentity {
        self.0.parallel_heap_identity()
    }

    fn parallel_epoch_retention(
        &self,
    ) -> Result<super::super::FacadeEpochRetention, super::super::FacadeMarkExclusionError> {
        self.0.parallel_epoch_retention()
    }

    fn symbol_root_designators(&self) -> SymbolRootDesignators {
        self.0.symbol_root_designators()
    }

    fn check_symbol_root_designators(
        &self,
        roots: &SymbolRootDesignators,
    ) -> Result<(), RegistrationError> {
        self.0.check_symbol_root_designators(roots)
    }

    fn collect_parallel_world_roots(
        &self,
        stopped: &mut AllStopped<'_>,
        out: &mut Vec<TaggedValue>,
    ) -> Result<Vec<*mut *mut MarkerObj>, RendezvousError> {
        self.0.collect_parallel_world_roots(stopped, out)
    }

    fn into_retained_owner(self: Box<Self>) -> Box<dyn Any + Send> {
        self.0.into_retained_owner()
    }
}

static_assertions::assert_impl_all!(WorldLoan: Send);
static_assertions::assert_not_impl_any!(WorldLoan: Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(WorldIdentity: Clone, Copy);
static_assertions::assert_not_impl_any!(SymbolRootDesignators: Clone, Copy);
