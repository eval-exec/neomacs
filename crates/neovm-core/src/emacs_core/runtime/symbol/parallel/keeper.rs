//! Borrowed identity of the original managers retained by an eval keeper.
use super::{Obarray, RefCell, ThreadProjectionTable};
use crate::tagged::gc::HeapIdentity;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;

/// Identity only: this proof grants no manager reference or running access.
/// Its borrow retains the keeper, and it cannot cross an owner thread.
#[derive(Clone, Copy)]
pub(crate) struct KeeperWorldDesignator<'b> {
    heap: HeapIdentity,
    obarray: NonNull<Obarray>,
    projections: NonNull<RefCell<ThreadProjectionTable>>,
    retained: PhantomData<&'b ()>,
    owner_thread: PhantomData<Rc<()>>,
}

impl<'b> KeeperWorldDesignator<'b> {
    /// # Safety
    /// All three designators must belong to the same original World retained
    /// by the caller's currently held keeper. The chosen borrow must live no
    /// longer than that retained Baton borrow, on the keeper's owner thread.
    /// Only eval/world may mint this proof; consumers compare identities before
    /// obtaining any separate stopped-authority storage loan.
    pub(crate) unsafe fn new(
        heap: HeapIdentity,
        obarray: NonNull<Obarray>,
        projections: NonNull<RefCell<ThreadProjectionTable>>,
    ) -> Self {
        Self {
            heap,
            obarray,
            projections,
            retained: PhantomData,
            owner_thread: PhantomData,
        }
    }

    pub(crate) fn heap(&self) -> HeapIdentity {
        self.heap
    }
    pub(crate) fn obarray(&self) -> NonNull<Obarray> {
        self.obarray
    }
    pub(crate) fn projections(&self) -> NonNull<RefCell<ThreadProjectionTable>> {
        self.projections
    }
}

static_assertions::assert_not_impl_any!(KeeperWorldDesignator<'static>: Send, Sync, Default);
