//! Heap-owned plist heads for published payloads.
//!
//! The key is an id this heap mints. The collector reads the values and never
//! a publisher address. Each value is one generation per descriptor: the heads
//! `for_each_root` yielded when that descriptor was appended. Unregister
//! removes the id. Dropping a publisher without unregister leaves the id in
//! place, so the heads stay reachable. [`TaggedHeap::reclaim_payload_generation`]
//! removes the generation with that id and no other. An id this publisher
//! does not hold removes nothing.
//!
//! Root enumeration is not part of the generic publication owner. The
//! thread-safety fixture and the n5-grace loom model compile that owner on
//! its own. Payloads that publish heads implement [`PayloadRoots`] here.

use super::TaggedHeap;
use super::publication::{PayloadGeneration, PublicationPayload};
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

/// Generation ids for one publisher, in append order, parallel to that
/// publisher's `Vec<GenerationRoots>`.
///
/// The id is not a slot. A removal does not renumber the ids that remain, so
/// an id a caller dropped cannot name a later generation. Keeping the ids
/// beside the entries leaves [`GenerationRoots`] and the entries map unchanged
/// for the seed walk.
struct PublisherGenerations {
    order: Vec<PayloadGeneration>,
}

/// Publisher-id counter, the one heap-wide generation counter, and the
/// per-publisher id lists. Boxed behind [`PublishedPayloadRoots`] so the
/// entries map keeps the size and offset the cold-gc pin measured.
struct PublisherCounters {
    next_publisher: u64,
    /// One counter for the whole heap, so an id belongs to exactly one
    /// publisher and is never reused.
    next_generation: PayloadGeneration,
    by_publisher: FxHashMap<PublisherId, PublisherGenerations>,
}

/// One `Vec<GenerationRoots>` per publisher id.
///
/// `entries` is the map the seed flattens, with the value type and offset it
/// had before generation ids were tracked, so the seed walk stays one flatten
/// per level. Boxing the counters keeps this struct at the cold-gc pin's
/// size.
#[repr(C)]
pub(crate) struct PublishedPayloadRoots {
    entries: FxHashMap<PublisherId, Vec<GenerationRoots>>,
    counters: Box<PublisherCounters>,
}

#[cfg(all(target_arch = "x86_64", target_pointer_width = "64"))]
const _: () = {
    assert!(std::mem::size_of::<PublishedPayloadRoots>() == 40);
    assert!(std::mem::offset_of!(PublishedPayloadRoots, entries) == 0);
};

impl Default for PublishedPayloadRoots {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            counters: Box::new(PublisherCounters {
                next_publisher: 0,
                next_generation: PayloadGeneration::from_raw(0),
                by_publisher: FxHashMap::default(),
            }),
        }
    }
}

impl PublishedPayloadRoots {
    pub(crate) fn values(&self) -> impl Iterator<Item = &Vec<GenerationRoots>> {
        self.entries.values()
    }

    fn register(&mut self) -> PublisherId {
        let id = PublisherId(self.counters.next_publisher);
        self.counters.next_publisher = self
            .counters
            .next_publisher
            .checked_add(1)
            .expect("publisher id space");
        self.entries.insert(id, Vec::new());
        self.counters
            .by_publisher
            .insert(id, PublisherGenerations { order: Vec::new() });
        id
    }

    fn unregister(&mut self, id: PublisherId) {
        self.entries.remove(&id);
        self.counters.by_publisher.remove(&id);
    }

    /// Push one generation and return its heap-wide id. The id is not a slot,
    /// and a later removal does not renumber the generations that remain.
    fn append_generation(&mut self, id: PublisherId, heads: Vec<TaggedValue>) -> PayloadGeneration {
        let generation = self.counters.next_generation;
        self.counters.next_generation = generation.checked_next();
        let order = &mut self
            .counters
            .by_publisher
            .get_mut(&id)
            .expect("payload roots are appended only for a registered publisher")
            .order;
        debug_assert_eq!(order.len(), self.entries[&id].len());
        order.push(generation);
        self.entries
            .get_mut(&id)
            .expect("payload roots are appended only for a registered publisher")
            .push(GenerationRoots { heads });
        generation
    }

    /// Remove the generation named `generation` and no other.
    ///
    /// A generation this publisher does not hold, including one another
    /// publisher minted, returns [`AbsentPayloadGeneration`] and leaves the
    /// vec untouched. In debug builds that miss also asserts.
    fn reclaim_generation(
        &mut self,
        id: PublisherId,
        generation: PayloadGeneration,
    ) -> Result<(), AbsentPayloadGeneration> {
        let held = &self
            .counters
            .by_publisher
            .get(&id)
            .expect("a payload generation is reclaimed only for a registered publisher")
            .order;
        let generations = self
            .entries
            .get(&id)
            .expect("a payload generation is reclaimed only for a registered publisher");
        debug_assert_eq!(held.len(), generations.len());
        let Some(index) = held.iter().position(|held| *held == generation) else {
            debug_assert!(
                false,
                "reclaim of a payload generation this publisher does not hold"
            );
            return Err(AbsentPayloadGeneration);
        };
        self.counters
            .by_publisher
            .get_mut(&id)
            .expect("a payload generation is reclaimed only for a registered publisher")
            .order
            .remove(index);
        self.entries
            .get_mut(&id)
            .expect("a payload generation is reclaimed only for a registered publisher")
            .remove(index);
        Ok(())
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

/// `reclaim_generation` was asked for a generation this publisher does not
/// hold. It dropped nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AbsentPayloadGeneration;

impl TaggedHeap {
    pub(crate) fn register_payload_publisher(&mut self) -> PublisherId {
        self.published_payload_roots.register()
    }

    pub(crate) fn unregister_payload_publisher(&mut self, id: PublisherId) {
        self.published_payload_roots.unregister(id)
    }

    /// Append one generation of every head `payload` yields. Callers run this
    /// before the publication Release store, under the same exclusive heap
    /// borrow as registration. The returned id is the generation just pushed.
    pub(crate) fn append_payload_roots<P: PayloadRoots>(
        &mut self,
        id: PublisherId,
        payload: &P,
    ) -> PayloadGeneration {
        let mut heads = Vec::new();
        payload.for_each_root(&mut |value| heads.push(value));
        self.published_payload_roots.append_generation(id, heads)
    }

    /// Remove the generation named `generation` and no other.
    ///
    /// The payload that appended this id is still alive, so the words in the
    /// vec are exactly the heads it yielded. A stale id, or one another
    /// publisher minted, removes nothing.
    #[cfg_attr(not(test), expect(dead_code, reason = "P10-b slices 2/3 wire refresh"))]
    pub(crate) fn reclaim_payload_generation(
        &mut self,
        id: PublisherId,
        generation: PayloadGeneration,
    ) -> Result<(), AbsentPayloadGeneration> {
        self.published_payload_roots
            .reclaim_generation(id, generation)
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
