//! Inert keeper capability while the real cons catalog is outside the heap.
use super::*;
use std::ptr::NonNull;

/// Owns the original exclusive loan, without storing a Rust heap reference.
/// It cannot dereference, cross threads, clone, or automatically resume.
#[must_use = "return the closed catalog before reconstructing collector authority"]
pub(in crate::tagged::gc) struct SuspendedConsCollector<'heap> {
    heap: NonNull<TaggedHeap>,
    identity: HeapIdentity,
    _loan: PhantomData<&'heap mut TaggedHeap>,
    _owner: PhantomData<Rc<()>>,
}
impl<'heap> QuiescentCollector<'heap> {
    #[cold]
    #[inline(never)]
    pub(in crate::tagged::gc) fn suspend_cons_storage(self) -> SuspendedConsCollector<'heap> {
        // The suspended capability retains the original loan. Its Drop
        // must not restore serial metadata while an era owns the catalog.
        let mut this = std::mem::ManuallyDrop::new(self);
        let identity = this.heap.heap_identity();
        SuspendedConsCollector {
            heap: NonNull::from(&mut *this.heap),
            identity,
            _loan: PhantomData,
            _owner: PhantomData,
        }
    }
}
impl<'heap> SuspendedConsCollector<'heap> {
    pub(in crate::tagged::gc) fn heap_identity(&self) -> HeapIdentity {
        self.identity
    }

    /// # Safety
    /// The keeper has blocked every new run through the registry transition,
    /// verified zero existing run/region loans under stop-all (or before any
    /// activation), and retains original heap/World backing. No storage or
    /// field borrow derived from the old collector survives. The catalog is
    /// reunited before any heap walk; the new collector is suspended again
    /// before admitting another storage run.
    #[cold]
    #[inline(never)]
    pub(in crate::tagged::gc) unsafe fn resume(self) -> QuiescentCollector<'heap> {
        QuiescentCollector {
            // SAFETY: the caller discharges the unique phase-transition
            // contract above; this is the sole original heap designator.
            heap: unsafe { &mut *self.heap.as_ptr() },
            _owner: PhantomData,
        }
    }
}
static_assertions::assert_not_impl_any!(SuspendedConsCollector<'static>: Send, Sync, Clone, Copy);
