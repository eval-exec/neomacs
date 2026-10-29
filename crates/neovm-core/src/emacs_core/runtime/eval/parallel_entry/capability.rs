//! Dormant production-entry capability vocabulary.
//!
//! There are deliberately no constructors, World/Context getters, heap access,
//! admission, publication, or transitions here. E2 must supply and separately
//! prove their runtime provenance. These types only express the borrow/brand
//! obligations that those future boundaries must preserve. Equal lifetime
//! brands mean equal provenance only when minted by that future generative
//! boundary; a runtime integer identity is never promoted to a brand here.

use std::marker::PhantomData;
use std::rc::Rc;

/// Invariance prevents shortening independent provenance to a common lifetime.
type Invariant<'id> = PhantomData<fn(&'id mut ()) -> &'id mut ()>;
type ThreadConfined = PhantomData<Rc<()>>;

#[allow(dead_code)] // Dormant E1 contract; E2 owns runtime construction.
pub struct HeapIdentity<'heap> {
    brand: Invariant<'heap>,
}

#[allow(dead_code)] // Dormant E1 contract; E2 owns runtime construction.
pub struct OwnerIdentity<'owner> {
    brand: Invariant<'owner>,
}

/// The borrow ends before a serial mutation may invalidate binding shape.
#[allow(dead_code)] // Dormant E1 contract; E2 owns runtime construction.
pub struct BindingShape<'shape> {
    brand: Invariant<'shape>,
}

/// Placeholder for destination-thread-owned execution storage, not admission.
///
/// An E2 constructor must bind all three brands to actual retained authorities.
/// There is no way to create this value in E1, including from a thread handle.
#[allow(dead_code)] // Dormant E1 contract; E2 owns runtime construction.
pub struct OwnerState<'heap, 'owner, 'shape> {
    heap: Invariant<'heap>,
    owner: Invariant<'owner>,
    shape: Invariant<'shape>,
    confined: ThreadConfined,
}

#[allow(dead_code)] // Dormant E1 contract; E2 owns runtime construction.
pub struct ParallelExecution<'loan, 'heap, 'owner, 'shape> {
    loan: PhantomData<&'loan mut ()>,
    heap: Invariant<'heap>,
    owner: Invariant<'owner>,
    shape: Invariant<'shape>,
    confined: ThreadConfined,
}

/// A native reservation has no heap-facing methods. This is not proof of a
/// published native extent: E2/E5 must connect it to Proof 1's actual protocol.
#[allow(dead_code)] // Dormant E1 contract; E2 owns runtime construction.
pub struct NativeExecution<'loan, 'heap, 'owner> {
    loan: PhantomData<&'loan mut ()>,
    heap: Invariant<'heap>,
    owner: Invariant<'owner>,
    confined: ThreadConfined,
}

/// Private-field placeholder, never constructed by E1. Its eventual constructor
/// must own the real matching stopped ticket and exclusive serial loan.
#[allow(dead_code)] // Dormant E1 contract; E2 owns runtime construction.
pub struct WorldBatonGuard<'heap, 'owner> {
    heap: Invariant<'heap>,
    owner: Invariant<'owner>,
    confined: ThreadConfined,
}

#[allow(dead_code)] // Dormant E1 contract; E2 owns runtime construction.
pub struct BatonWitness<'guard, 'heap, 'owner> {
    guard: Invariant<'guard>,
    heap: Invariant<'heap>,
    owner: Invariant<'owner>,
    confined: ThreadConfined,
}

/// A branded serial reservation, with no legacy Context/World access in E1.
#[allow(dead_code)] // Dormant E1 contract; E2 owns runtime construction.
pub struct SerialContext<'loan, 'guard, 'heap, 'owner> {
    loan: PhantomData<&'loan mut ()>,
    guard: Invariant<'guard>,
    heap: Invariant<'heap>,
    owner: Invariant<'owner>,
    confined: ThreadConfined,
}

#[allow(dead_code)] // Borrow contracts are compiled now; no E1 owner is constructed.
impl<'heap, 'owner, 'shape> OwnerState<'heap, 'owner, 'shape> {
    pub fn parallel(&mut self) -> ParallelExecution<'_, 'heap, 'owner, 'shape> {
        ParallelExecution {
            loan: PhantomData,
            heap: PhantomData,
            owner: PhantomData,
            shape: PhantomData,
            confined: PhantomData,
        }
    }

    pub fn native(&mut self) -> NativeExecution<'_, 'heap, 'owner> {
        NativeExecution {
            loan: PhantomData,
            heap: PhantomData,
            owner: PhantomData,
            confined: PhantomData,
        }
    }

    pub fn serial<'loan, 'guard>(
        &'loan mut self,
        _: &'loan BatonWitness<'guard, 'heap, 'owner>,
    ) -> SerialContext<'loan, 'guard, 'heap, 'owner> {
        SerialContext {
            loan: PhantomData,
            guard: PhantomData,
            heap: PhantomData,
            owner: PhantomData,
            confined: PhantomData,
        }
    }
}

#[allow(dead_code)] // Provenance checks are exercised by compile-contract fixtures.
impl<'heap, 'owner, 'shape> ParallelExecution<'_, 'heap, 'owner, 'shape> {
    /// Require matching generative heap provenance, without dereferencing it.
    pub fn require_heap(&self, _: &HeapIdentity<'heap>) {}

    pub fn require_owner(&self, _: &OwnerIdentity<'owner>) {}

    pub fn require_shape(&self, _: &BindingShape<'shape>) {}
}

#[allow(dead_code)] // Guard construction and actual stopped authority belong to E2.
impl<'heap, 'owner> WorldBatonGuard<'heap, 'owner> {
    /// The higher-ranked guard lifetime prevents returning the witness as R.
    /// This mints no live World authority: the guard has no E1 constructor.
    pub fn with_witness<R>(
        &mut self,
        body: impl for<'guard> FnOnce(BatonWitness<'guard, 'heap, 'owner>) -> R,
    ) -> R {
        body(BatonWitness {
            guard: PhantomData,
            heap: PhantomData,
            owner: PhantomData,
            confined: PhantomData,
        })
    }
}

static_assertions::assert_not_impl_any!(OwnerState<'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ParallelExecution<'static, 'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(NativeExecution<'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(WorldBatonGuard<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(BatonWitness<'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SerialContext<'static, 'static, 'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::const_assert_eq!(
    std::mem::size_of::<OwnerState<'static, 'static, 'static>>(),
    0
);
static_assertions::const_assert_eq!(
    std::mem::size_of::<ParallelExecution<'static, 'static, 'static, 'static>>(),
    0
);
static_assertions::const_assert_eq!(
    std::mem::size_of::<NativeExecution<'static, 'static, 'static>>(),
    0
);
static_assertions::const_assert_eq!(std::mem::size_of::<WorldBatonGuard<'static, 'static>>(), 0);
static_assertions::const_assert_eq!(
    std::mem::size_of::<BatonWitness<'static, 'static, 'static>>(),
    0
);
static_assertions::const_assert_eq!(
    std::mem::size_of::<SerialContext<'static, 'static, 'static, 'static>>(),
    0
);
