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
/// without locking or blocking. Fixnums, `nil` and `t` need no root and take
/// no table cell.
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
}

/// A value materialized from a [`SharedRoot`] on its heap's mutator.
///
/// The guard borrows the root, which keeps the object alive, and the heap,
/// which proves the caller is that heap's mutator. Like every raw value it is
/// confined to this thread.
#[must_use = "materializing a shared root has no effect besides producing its value"]
pub struct LocalRoot<'r> {
    value: TaggedValue,
    _borrows: PhantomData<(&'r SharedRoot, &'r TaggedHeap)>,
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
static_assertions::assert_impl_all!(SharedRootError: Send, Sync, Copy, std::error::Error);
static_assertions::assert_not_impl_any!(LocalRoot<'static>: Send, Sync);

impl SharedRoot {
    /// Share `value`, a value held by `heap`'s mutator.
    pub fn new(heap: &TaggedHeap, value: TaggedValue) -> Self {
        Self::in_heap(heap.heap_identity(), value)
    }

    /// Share `value` from the heap installed on this thread, for callers
    /// that reach their heap only through the thread's installed view.
    pub fn from_current_heap(value: TaggedValue) -> Result<Self, SharedRootError> {
        let heap = current_tagged_heap_identity()
            .and_then(HeapIdentity::from_legacy_word)
            .ok_or(SharedRootError::NoInstalledHeap)?;
        Ok(Self::in_heap(heap, value))
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
        }
    }
}

impl fmt::Debug for SharedRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rooted = matches!(self.transport, Transport::Rooted(_));
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
