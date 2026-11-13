//! Root-owning heap fixture. Split from the Context-coupled synthetic barrier
//! so a test-support consumer can install a heap without naming the evaluator.
use super::*;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

pub struct TestHeap {
    owner: AliasableBox<TaggedHeap>,
}

impl TestHeap {
    pub fn new() -> Self {
        Self::from_heap(TaggedHeap::new())
    }

    /// Finish address-independent setup before installing a custom heap.
    pub(crate) fn from_heap(heap: TaggedHeap) -> Self {
        let fixture = Self {
            owner: AliasableBox::new(heap),
        };
        fixture.install();
        fixture
    }

    /// Copy the provenance root without borrowing any heap bytes.
    pub fn root(&self) -> NonNull<TaggedHeap> {
        self.owner.root()
    }

    /// Refresh TLS from the root after a phase/barrier-window change.
    pub fn install(&self) {
        set_tagged_heap_root(self.root());
    }

    #[cfg(test)]
    pub(in crate::tagged::gc) fn with_mutator_gc_for_test<R>(
        &mut self,
        id: super::mutator_gc::TestMutatorGcId,
        body: impl FnOnce(NonNull<TaggedHeap>) -> R,
    ) -> Result<R, super::mutator_gc::TestMutatorSwitchError> {
        self.owner.with_mutator_gc_for_test(id, body)
    }
}

impl Deref for TestHeap {
    type Target = TaggedHeap;

    fn deref(&self) -> &TaggedHeap {
        self.owner.get()
    }
}

impl DerefMut for TestHeap {
    fn deref_mut(&mut self) -> &mut TaggedHeap {
        self.owner.get_mut()
    }
}
