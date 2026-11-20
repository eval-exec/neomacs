//! Heap-owned plist heads for published payloads.
//!
//! The key is an id this heap mints. The collector reads the values and never
//! a publisher address. Each value is one generation per descriptor: the heads
//! `for_each_root` yielded when that descriptor was appended. Unregister
//! removes the id. Dropping a publisher without unregister leaves the id in
//! place, so the heads stay reachable. [`TaggedHeap::reclaim_payload_generation`]
//! removes one generation and no other.
//!
//! Root enumeration is not part of the generic publication owner. The
//! thread-safety fixture and the n5-grace loom model compile that owner on
//! its own. Payloads that publish heads implement [`PayloadRoots`] here.

use super::TaggedHeap;
use super::publication::PublicationPayload;
use crate::tagged::value::TaggedValue;
use rustc_hash::FxHashMap;

/// Heads a payload appends when it becomes current.
///
/// [`PublicationPayload`] does not name heap words. The heap registry is the
/// only caller, and it requires this bound.
pub(crate) trait PayloadRoots: PublicationPayload {
    fn for_each_root(&self, visit: &mut dyn FnMut(TaggedValue));
}

/// Identity of one payload publisher. Not an address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PublisherId(u64);

/// Heap-object heads of one published descriptor.
pub(crate) struct GenerationRoots {
    heads: Vec<TaggedValue>,
}

impl<'a> IntoIterator for &'a GenerationRoots {
    type Item = &'a TaggedValue;
    type IntoIter = std::slice::Iter<'a, TaggedValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.heads.iter()
    }
}

/// One `Vec<GenerationRoots>` per publisher id, plus the next id to mint.
///
/// The outer `Vec` is the same width as the flat `Vec<TaggedValue>` it
/// replaced, so this struct's size does not move the cold-gc pin.
#[derive(Default)]
pub(crate) struct PublishedPayloadRoots {
    next_id: u64,
    entries: FxHashMap<PublisherId, Vec<GenerationRoots>>,
}

impl PublishedPayloadRoots {
    pub(crate) fn values(&self) -> impl Iterator<Item = &Vec<GenerationRoots>> {
        self.entries.values()
    }

    fn register(&mut self) -> PublisherId {
        let id = PublisherId(self.next_id);
        self.next_id = self.next_id.checked_add(1).expect("publisher id space");
        self.entries.insert(id, Vec::new());
        id
    }

    fn unregister(&mut self, id: PublisherId) {
        self.entries.remove(&id);
    }

    /// Push one generation and return its index. Later removals shift higher
    /// indices; the caller that recorded the index fixes those up.
    fn append_generation(&mut self, id: PublisherId, heads: Vec<TaggedValue>) -> usize {
        let generations = self
            .entries
            .get_mut(&id)
            .expect("payload roots are appended only for a registered publisher");
        let index = generations.len();
        generations.push(GenerationRoots { heads });
        index
    }

    fn reclaim_generation(&mut self, id: PublisherId, generation_index: usize) {
        let generations = self
            .entries
            .get_mut(&id)
            .expect("a payload generation is reclaimed only for a registered publisher");
        generations.remove(generation_index);
    }

    #[cfg(test)]
    fn len_of(&self, id: PublisherId) -> usize {
        self.entries
            .get(&id)
            .map(|generations| {
                generations
                    .iter()
                    .map(|generation| generation.heads.len())
                    .sum()
            })
            .unwrap_or(0)
    }

    #[cfg(any(test, feature = "test-support"))]
    fn publisher_count(&self) -> usize {
        self.entries.len()
    }
}

impl TaggedHeap {
    pub(crate) fn register_payload_publisher(&mut self) -> PublisherId {
        self.published_payload_roots.register()
    }

    pub(crate) fn unregister_payload_publisher(&mut self, id: PublisherId) {
        self.published_payload_roots.unregister(id)
    }

    /// Append one generation of every head `payload` yields. Callers run this
    /// before the publication Release store, under the same exclusive heap
    /// borrow as registration. The returned index is the slot just pushed.
    pub(crate) fn append_payload_roots<P: PayloadRoots>(
        &mut self,
        id: PublisherId,
        payload: &P,
    ) -> usize {
        let mut heads = Vec::new();
        payload.for_each_root(&mut |value| heads.push(value));
        self.published_payload_roots.append_generation(id, heads)
    }

    /// Remove one generation's inner vec and no other.
    ///
    /// The payload that appended this index is still alive, so the words in
    /// the vec are exactly the heads it yielded. Higher indices shift down.
    #[cfg_attr(not(test), expect(dead_code, reason = "P10-b slices 2/3 wire refresh"))]
    pub(crate) fn reclaim_payload_generation(&mut self, id: PublisherId, generation_index: usize) {
        self.published_payload_roots
            .reclaim_generation(id, generation_index);
    }

    #[cfg(test)]
    pub(crate) fn payload_root_count_for_test(&self, id: PublisherId) -> usize {
        self.published_payload_roots.len_of(id)
    }

    /// How many publisher ids this heap still holds.
    ///
    /// Compiled only for this crate's tests and the `test-support` feature.
    /// Product profiles do not build it.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn payload_publisher_count_for_test(&self) -> usize {
        self.published_payload_roots.publisher_count()
    }
}
