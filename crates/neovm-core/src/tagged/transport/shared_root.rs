//! Rooted transport of one Lisp value between the mutators of one heap.

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use super::root_table::{RootLease, TracedWord};
use crate::tagged::gc::{HeapIdentity, TaggedHeap, current_tagged_heap_identity};
use crate::tagged::value::TaggedValue;

/// A Lisp value detached from any one mutator thread.
///
/// A raw [`TaggedValue`] is thread-confined: it stays valid only while the
/// mutator holding it keeps it reachable. A `SharedRoot` keeps its object
/// alive through its heap's root table, so it may be sent to, stored in, or
/// dropped on any thread; non-mutator holders (renderer snapshots, worker
/// queues) store this instead of a raw word. Only a mutator of the same heap
/// turns it back into a local value, through [`SharedRoot::materialize`].
///
/// Clones share one root. It retires when the last clone drops, on any thread,
/// without locking or blocking. Individually admitted fixnums, `nil` and `t`
/// need no table cell; batch admission roots its private vector once.
#[derive(Clone)]
pub struct SharedRoot {
    heap: HeapIdentity,
    transport: Transport,
}

#[derive(Clone)]
enum Transport {
    /// A word the collector never traces from a root: a fixnum, `nil` or `t`.
    Untraced(usize),
    /// A heap object or a symbol, kept alive by its lease.
    Rooted(Arc<RootLease>),
    /// A child of one private, immutable vector protected by the lease.
    /// The cached word preserves child identity without handing out the
    /// backing vector or dereferencing its payload on a receiving thread.
    BatchElement {
        lease: Arc<RootLease>,
        child: ChildIdentity,
    },
}

/// Identity of a child of an unexposed immutable vector. Constructed only
/// during audited batch admission; its accompanying lease protects the child.
/// The word is compared opaquely off-mutator and materialized only on its heap.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
struct ChildIdentity(usize);

static_assertions::assert_impl_all!(ChildIdentity: Send, Sync, Copy, fmt::Debug);
static_assertions::assert_eq_size!(ChildIdentity, usize);
const _: () = {
    assert!(std::mem::align_of::<ChildIdentity>() == std::mem::align_of::<usize>());
    assert!(std::mem::offset_of!(ChildIdentity, 0) == 0);
};

/// A value materialized from a [`SharedRoot`] on its heap's mutator.
///
/// [`SharedRoot::materialize`] ties `'r` to both the root, which keeps the
/// object alive, and the heap borrow that proves the caller is that heap's
/// mutator. Like every raw value it is confined to this thread.
#[must_use = "materializing a shared root has no effect besides producing its value"]
pub struct LocalRoot<'r> {
    value: TaggedValue,
    _borrows: PhantomData<&'r ()>,
}

/// Why a [`SharedRoot`] could not be created or materialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SharedRootError {
    /// The root belongs to another heap; values never cross heaps.
    #[error("shared root of heap {owner:?} materialized on heap {mutator:?}")]
    ForeignHeap {
        owner: HeapIdentity,
        mutator: HeapIdentity,
    },
    /// This thread has no tagged heap installed, so it is not a mutator.
    #[error("no tagged heap is installed on this thread")]
    NoInstalledHeap,
}

static_assertions::assert_impl_all!(SharedRoot: Send, Sync, Clone, fmt::Debug);
static_assertions::assert_impl_all!(Transport: Send, Sync, Clone);
static_assertions::assert_impl_all!(SharedRootError: Send, Sync, Copy, std::error::Error);
static_assertions::assert_not_impl_any!(LocalRoot<'static>: Send, Sync);

impl SharedRoot {
    /// Share `value`, a value held by `heap`'s mutator.
    ///
    /// # Safety
    /// The caller must be a mutator of `heap`. `value` must be live, and any
    /// traced object it names must belong to this heap's allocation domain.
    /// Keep `value` reachable until registration completes. Every subsequent
    /// collection of that heap must trace its shared-root table while this
    /// root can be materialized. The root does not own the heap: its backing
    /// storage must remain alive for every materialization.
    ///
    /// During an active mark, reachability means coverage by the collector's
    /// start roots, a SATB-protected source home, or allocate-black births.
    /// Merely retaining an unregistered Rust copy of a value is insufficient.
    ///
    /// Raw values carry no heap brand, so `&TaggedHeap` cannot prove these
    /// admission requirements on its own.
    pub unsafe fn new(heap: &TaggedHeap, value: TaggedValue) -> Self {
        Self::in_heap(heap.heap_identity(), value)
    }

    /// Share `value` from the heap installed on this thread, for callers
    /// that reach their heap only through the thread's installed view.
    ///
    /// # Safety
    /// The caller must satisfy [`SharedRoot::new`]'s admission and lifetime
    /// requirements for the installed heap. Installed TLS identifies the
    /// mutator's heap; it does not prove that an unbranded `value` came from
    /// that heap or is still live.
    pub unsafe fn from_current_heap(value: TaggedValue) -> Result<Self, SharedRootError> {
        let heap = Self::current_heap()?;
        Ok(Self::in_heap(heap, value))
    }

    /// Admit several values through one private, immutable Lisp vector root.
    /// Each returned handle materializes its original child, never the vector.
    ///
    /// # Safety
    /// The caller must satisfy [`SharedRoot::new`]'s live same-heap admission
    /// and lifetime requirements for every value and the installed mutator.
    /// Keep every input reachable until this method returns. The installed
    /// heap's collector must trace the shared-root table. This method keeps
    /// the freshly allocated backing vector private and never mutates it.
    pub unsafe fn batch_from_current_heap(
        values: &[TaggedValue],
    ) -> Result<Vec<Self>, SharedRootError> {
        let heap = Self::current_heap()?;
        if values.is_empty() {
            return Ok(Vec::new());
        }
        // This allocator invokes neither Lisp callbacks nor a collection.
        // The caller keeps inputs reachable until the vector's lease is live.
        let vector = TaggedValue::vector(values.to_vec());
        let word = TracedWord::of(vector).expect("a freshly allocated vector is traced");
        let lease = Arc::new(RootLease::register(heap, word));
        Ok(values
            .iter()
            .map(|value| Self {
                heap,
                transport: Transport::BatchElement {
                    lease: Arc::clone(&lease),
                    child: ChildIdentity(value.bits()),
                },
            })
            .collect())
    }

    /// Whether two handles share a collector lease, possibly selecting
    /// different children of the same private vector.
    pub fn shares_backing_root(&self, other: &Self) -> bool {
        self.heap == other.heap
            && self
                .lease()
                .zip(other.lease())
                .is_some_and(|(left, right)| Arc::ptr_eq(left, right))
    }

    /// Coalesce rooted children into one lease on their installed mutator.
    ///
    /// Returns `None` if no coalescing is needed. All input handles remain
    /// live throughout allocation, and are unchanged on failure.
    ///
    /// # Errors
    /// [`SharedRootError::NoInstalledHeap`] on a non-mutator, or
    /// [`SharedRootError::ForeignHeap`] for an input from another heap.
    pub fn coalesce_on_current_mutator(
        roots: &[&Self],
    ) -> Result<Option<Vec<Self>>, SharedRootError> {
        let Some(first) = roots.first() else {
            return Ok(None);
        };
        let heap = Self::current_heap()?;
        for root in roots {
            if root.heap != heap {
                return Err(SharedRootError::ForeignHeap {
                    owner: root.heap,
                    mutator: heap,
                });
            }
        }
        if roots.len() == 1
            || roots.iter().all(|root| root.shares_backing_root(first))
            || roots.iter().all(|root| root.lease().is_none())
        {
            return Ok(None);
        }
        let values: Vec<_> = roots
            .iter()
            .map(|root| TaggedValue::from_bits(root.word()))
            .collect();
        // SAFETY: each checked input belongs to the installed heap, and its
        // borrowed lease keeps its child alive across allocation. Only this
        // method's private vector constructor sees the aggregate value.
        unsafe { Self::batch_from_current_heap(&values) }.map(Some)
    }

    fn current_heap() -> Result<HeapIdentity, SharedRootError> {
        current_tagged_heap_identity()
            .and_then(HeapIdentity::from_legacy_word)
            .ok_or(SharedRootError::NoInstalledHeap)
    }

    fn lease(&self) -> Option<&Arc<RootLease>> {
        match &self.transport {
            Transport::Untraced(_) => None,
            Transport::Rooted(lease) | Transport::BatchElement { lease, .. } => Some(lease),
        }
    }

    fn in_heap(heap: HeapIdentity, value: TaggedValue) -> Self {
        let transport = match TracedWord::of(value) {
            Some(word) => Transport::Rooted(Arc::new(RootLease::register(heap, word))),
            None => Transport::Untraced(value.bits()),
        };
        Self { heap, transport }
    }

    /// The heap whose mutators may materialize this root.
    pub fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }

    /// The local value, on `heap`'s mutator.
    ///
    /// # Errors
    /// [`SharedRootError::ForeignHeap`] when `heap` is not the heap this root
    /// was shared from.
    pub fn materialize<'r>(
        &'r self,
        heap: &'r TaggedHeap,
    ) -> Result<LocalRoot<'r>, SharedRootError> {
        let mutator = heap.heap_identity();
        if mutator != self.heap {
            return Err(SharedRootError::ForeignHeap {
                owner: self.heap,
                mutator,
            });
        }
        Ok(LocalRoot {
            value: TaggedValue::from_bits(self.word()),
            _borrows: PhantomData,
        })
    }

    /// The local value when this thread has the root's heap installed, for
    /// crate code that reaches its heap only through the installed view. The
    /// installed identity is this thread's proof that it is that heap's
    /// mutator, and the root keeps the object alive while `&self` lives.
    pub(crate) fn value_on_current_mutator(&self) -> Option<TaggedValue> {
        let installed = current_tagged_heap_identity().and_then(HeapIdentity::from_legacy_word)?;
        (installed == self.heap).then(|| TaggedValue::from_bits(self.word()))
    }

    /// Whether both roots hold the same object of the same heap (Lisp `eq`).
    pub fn is_same_object(&self, other: &Self) -> bool {
        self.heap == other.heap && self.word() == other.word()
    }

    fn word(&self) -> usize {
        match &self.transport {
            Transport::Untraced(word) => *word,
            Transport::Rooted(lease) => lease.word().value().bits(),
            Transport::BatchElement { child, .. } => child.0,
        }
    }
}

impl fmt::Debug for SharedRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rooted = !matches!(self.transport, Transport::Untraced(_));
        f.debug_struct("SharedRoot")
            .field("heap", &self.heap)
            .field("word", &format_args!("{:#x}", self.word()))
            .field("rooted", &rooted)
            .finish()
    }
}

impl LocalRoot<'_> {
    /// The value. A copy taken out of the guard is still thread-confined but
    /// no longer kept alive by the root once the guard's borrow ends.
    #[inline]
    pub fn value(&self) -> TaggedValue {
        self.value
    }
}

impl fmt::Debug for LocalRoot<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LocalRoot").field(&self.value).finish()
    }
}
