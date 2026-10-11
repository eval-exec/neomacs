//! Admission and storage retention for the legacy single-writer scan protocol.

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use super::TaggedHeap;

/// A thread-confined admission to one heap's quiescent snapshot capture.
///
/// This is the legacy serialized-writer protocol. A parallel World must produce
/// this admission from its stop-all/writer capability, rather than assuming that
/// an ordinary heap borrow excludes writers reaching the heap through TLS.
#[must_use = "the admission keeps the heap exclusively borrowed during capture"]
pub(crate) struct SingleMutatorWorld<'h> {
    heap: &'h mut TaggedHeap,
    _thread: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(SingleMutatorWorld<'static>: Send, Sync);
static_assertions::assert_impl_all!(SingleMutatorWorld<'static>: std::fmt::Debug);

impl<'h> SingleMutatorWorld<'h> {
    /// Admit the heap's owner to a start snapshot.
    ///
    /// # Safety
    /// The caller is the heap's only writer, including through legacy TLS raw
    /// pointers. Capture runs without Lisp callbacks or allocation safepoints.
    /// The matching obarray has that same exclusive writer. Until this mark
    /// finishes or is abandoned, writes use SATB, vector retirement and symbol
    /// seqlock publication; storage is retained through the marker's last read.
    pub(crate) unsafe fn from_heap(heap: &'h mut TaggedHeap) -> Self {
        Self {
            heap,
            _thread: PhantomData,
        }
    }

    pub(crate) fn heap_identity(&self) -> usize {
        self.heap.identity()
    }

    pub(crate) fn heap(&self) -> &TaggedHeap {
        self.heap
    }
}

impl std::fmt::Debug for SingleMutatorWorld<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleMutatorWorld")
            .field("heap_identity", &self.heap_identity())
            .finish()
    }
}

/// The unique owner of a stable allocation's scan-retention identity.
///
/// Only this owner and its admitted leases hold the private Arc. Checking Arc
/// uniqueness proves that all leases have ended before storage is reclaimed.
/// The owner is not cloneable, and leases never expose or downgrade the Arc.
#[repr(transparent)]
pub(crate) struct ScanStorageOwner {
    storage: Arc<()>,
}

static_assertions::assert_impl_all!(ScanStorageOwner: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ScanStorageOwner: Clone);
const _: () = {
    assert!(std::mem::size_of::<ScanStorageOwner>() == std::mem::size_of::<usize>());
    assert!(std::mem::align_of::<ScanStorageOwner>() == std::mem::align_of::<usize>());
};

impl ScanStorageOwner {
    pub(crate) fn new() -> Self {
        Self {
            storage: Arc::new(()),
        }
    }

    /// True when a scan lease still requires this owner's storage.
    ///
    /// Arc's uniqueness check synchronizes with completed lease destruction.
    /// An outstanding lease causes the owner to retain its allocation instead
    /// of waiting. Private Arc construction prevents future readers after the
    /// owner has entered its exclusive destruction path.
    pub(crate) fn has_leases(&mut self) -> bool {
        Arc::get_mut(&mut self.storage).is_none()
    }
}

impl std::fmt::Debug for ScanStorageOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanStorageOwner")
            .field(
                "leases",
                &Arc::strong_count(&self.storage).saturating_sub(1),
            )
            .finish()
    }
}

/// A marker reader's lease on an owner-retained, stable storage allocation.
///
/// The lease holds the owner's private Arc through its final raw-pointer read.
/// Its ordinary Arc destructor ends retention; no second reader counter or
/// callback is needed. This lease is neither cloneable nor shareable.
#[must_use = "dropping the lease ends this reader's storage retention"]
pub(crate) struct ScanStorageLease {
    storage: Arc<()>,
    _exclusive_reader: PhantomData<Cell<()>>,
}

static_assertions::assert_impl_all!(ScanStorageLease: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ScanStorageLease: Sync, Clone);

impl ScanStorageLease {
    pub(crate) fn capture(owner: &ScanStorageOwner, _: &SingleMutatorWorld<'_>) -> Self {
        Self {
            storage: Arc::clone(&owner.storage),
            _exclusive_reader: PhantomData,
        }
    }
}

impl std::fmt::Debug for ScanStorageLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanStorageLease")
            .field(
                "leases",
                &Arc::strong_count(&self.storage).saturating_sub(1),
            )
            .finish()
    }
}
