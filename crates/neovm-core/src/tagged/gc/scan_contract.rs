//! Admission and storage retention for the legacy single-writer scan protocol.

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// A marker reader's lease on an owner-retained, stable storage allocation.
///
/// The owner observes the counter with Acquire before freeing its storage.
/// Destruction releases the reader only after its final raw-pointer access.
/// This lease is neither cloneable nor shareable; one marker owns each lease.
#[must_use = "dropping the lease ends this reader's storage retention"]
pub(crate) struct ScanStorageLease {
    readers: Arc<AtomicUsize>,
    _exclusive_reader: PhantomData<Cell<()>>,
}

static_assertions::assert_impl_all!(ScanStorageLease: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ScanStorageLease: Sync);

impl ScanStorageLease {
    pub(crate) fn capture(readers: &Arc<AtomicUsize>, _: &SingleMutatorWorld<'_>) -> Self {
        readers.fetch_add(1, Ordering::Relaxed);
        Self {
            readers: Arc::clone(readers),
            _exclusive_reader: PhantomData,
        }
    }
}

impl std::fmt::Debug for ScanStorageLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanStorageLease")
            .field("readers", &self.readers.load(Ordering::Acquire))
            .finish()
    }
}

impl Drop for ScanStorageLease {
    fn drop(&mut self) {
        self.readers.fetch_sub(1, Ordering::Release);
    }
}
