//! Pointer-free proof of the closed, single-Context World construction policy.
//!
//! The driver's original `Vm.ctx` borrow supplies actual access authority and
//! lifetime. These types supply only affine policy/phase tokens: they contain no
//! owner address, allocation root, manager loan, or independently usable view.
//! They do not encode owner identity or distinguish different activations.
//! This is synchronous sole-owner execution, not a second-owner admission or
//! switch protocol. Such admission must replace this construction policy rather
//! than manufacture a second mutable view beneath it.

use std::fmt;
use std::marker::PhantomData;

/// Private construction fact retained by World, without a Context/layout word.
///
/// Only the closed World implementation can construct this fact. Its factory
/// publishes exactly one set of manager views to one Context. Allocation leases
/// may be duplicated for those views; their count is not a Context-owner count.
#[derive(Debug)]
pub(super) struct SoleContextConstruction {
    _confined: PhantomData<*mut ()>,
}

impl SoleContextConstruction {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self {
            _confined: PhantomData,
        }
    }

    /// Issue a pointer-free registration for this closed construction policy.
    ///
    /// It grants no independent manager access. The resulting VM must keep its
    /// original owner borrow and may only rejoin that continuation.
    #[inline(always)]
    pub(super) fn registration(&self) -> SoleOwnerRegistration {
        SoleOwnerRegistration {
            _confined: PhantomData,
        }
    }
}

/// Affine registration issued only by the World's sealed sole-owner policy.
///
/// This cannot keep an allocation alive or obtain a manager. Its absence of a
/// pointer deliberately prevents a stale registration from becoming a second
/// access path; actual access remains through the driver's original owner.
#[derive(Debug)]
#[must_use = "consume the registration into the original sole-owner VM"]
pub(crate) struct SoleOwnerRegistration {
    _confined: PhantomData<*mut ()>,
}

impl SoleOwnerRegistration {
    /// Install the construction policy in the VM that borrows the original owner.
    ///
    /// This transfers only a pointer-free policy token, not owner identity or a
    /// manager loan. The actual `Vm.ctx` reference remains access authority.
    #[inline(always)]
    pub(crate) fn into_vm(self) -> SoleOwnerVm {
        SoleOwnerVm {
            _confined: PhantomData,
        }
    }
}

/// The sealed construction policy retained by the original borrowed VM.
///
/// Issuing a driver token supplies no access and performs no ownership check.
/// The VM's private entry discipline and its actual Context borrow determine
/// which activation owns operands; this zero-sized token does not prove a
/// globally unique live-driver count or a caller-chosen lifetime identity.
#[derive(Debug)]
#[must_use = "retain the construction policy in the original borrowed VM"]
pub(crate) struct SoleOwnerVm {
    _confined: PhantomData<*mut ()>,
}

impl SoleOwnerVm {
    /// Issue the affine token at a private VM driver entry.
    #[inline(always)]
    pub(crate) fn driver(&self) -> SoleOwnerDriver {
        SoleOwnerDriver::new()
    }
}

/// Running-phase policy token for synchronous sole-owner execution.
///
/// The VM pairs it with its unpublished operand cursor. Consuming publication
/// removes that pair before an escape; the private published cursor rejoins only
/// through the original `Vm.ctx`. No token has a manager or root accessor.
///
/// These plain tokens intentionally carry no generative lifetime. They can be
/// transported as ordinary values; the real Context borrow, not this token,
/// enforces owner access. Drop performs no publication, barrier, allocation,
/// assertion, or blocking work.
#[derive(Debug)]
#[must_use = "pair the running token with the unpublished operand cursor"]
pub(crate) struct SoleOwnerDriver {
    _confined: PhantomData<*mut ()>,
}

impl SoleOwnerDriver {
    #[inline(always)]
    fn new() -> Self {
        Self {
            _confined: PhantomData,
        }
    }

    /// Consume the running token after the VM publishes its operand cursor.
    ///
    /// This method changes only policy state; actual root publication belongs to
    /// the VM's paired consuming cursor operation, not this pointer-free token.
    #[inline(always)]
    pub(crate) fn into_published(self) -> PublishedSoleOwnerDriver {
        PublishedSoleOwnerDriver {
            _confined: PhantomData,
        }
    }
}

/// Published-phase policy token, consumed by the original VM's reacquisition.
///
/// Affine transport prevents reuse of a consumed published token. It does not
/// refresh headers or determine owner identity: only the returning original VM
/// may pair the result with newly acquired operand slots and current views.
#[derive(Debug)]
#[must_use = "consume the published token through original-VM reacquisition"]
pub(crate) struct PublishedSoleOwnerDriver {
    _confined: PhantomData<*mut ()>,
}

impl PublishedSoleOwnerDriver {
    /// Consume the published token when the original VM reacquires its cursor.
    #[inline(always)]
    pub(crate) fn into_running(self) -> SoleOwnerDriver {
        SoleOwnerDriver::new()
    }
}

static_assertions::assert_not_impl_any!(SoleContextConstruction: Send, Sync, Copy, Clone);
static_assertions::assert_not_impl_any!(SoleOwnerRegistration: Send, Sync, Copy, Clone);
static_assertions::assert_not_impl_any!(SoleOwnerVm: Send, Sync, Copy, Clone);
static_assertions::assert_not_impl_any!(SoleOwnerDriver: Send, Sync, Copy, Clone);
static_assertions::assert_not_impl_any!(PublishedSoleOwnerDriver: Send, Sync, Copy, Clone);
static_assertions::assert_impl_all!(SoleContextConstruction: fmt::Debug);
static_assertions::assert_impl_all!(SoleOwnerRegistration: fmt::Debug);
static_assertions::assert_impl_all!(SoleOwnerVm: fmt::Debug);
static_assertions::assert_impl_all!(SoleOwnerDriver: fmt::Debug);
static_assertions::assert_impl_all!(PublishedSoleOwnerDriver: fmt::Debug);

const _: () = {
    assert!(std::mem::size_of::<SoleContextConstruction>() == 0);
    assert!(std::mem::size_of::<SoleOwnerRegistration>() == 0);
    assert!(std::mem::size_of::<SoleOwnerVm>() == 0);
    assert!(std::mem::size_of::<SoleOwnerDriver>() == 0);
    assert!(std::mem::size_of::<PublishedSoleOwnerDriver>() == 0);
    assert!(std::mem::align_of::<SoleContextConstruction>() == 1);
    assert!(std::mem::align_of::<SoleOwnerRegistration>() == 1);
    assert!(std::mem::align_of::<SoleOwnerVm>() == 1);
    assert!(std::mem::align_of::<SoleOwnerDriver>() == 1);
    assert!(std::mem::align_of::<PublishedSoleOwnerDriver>() == 1);
    assert!(!std::mem::needs_drop::<SoleContextConstruction>());
    assert!(!std::mem::needs_drop::<SoleOwnerRegistration>());
    assert!(!std::mem::needs_drop::<SoleOwnerVm>());
    assert!(!std::mem::needs_drop::<SoleOwnerDriver>());
    assert!(!std::mem::needs_drop::<PublishedSoleOwnerDriver>());
};
