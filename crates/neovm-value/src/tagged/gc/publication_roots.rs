//! Heap-owned plist heads for published payloads.
//!
//! The key is an id this heap mints. The collector reads the values and never
//! a publisher address. Unregister removes the id. Dropping a publisher
//! without unregister leaves the id in place, so the heads stay reachable.
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

/// One `Vec<TaggedValue>` per publisher id, plus the next id to mint.
#[derive(Default)]
pub(crate) struct PublishedPayloadRoots {
    next_id: u64,
    entries: FxHashMap<PublisherId, Vec<TaggedValue>>,
}

impl PublishedPayloadRoots {
    pub(crate) fn values(&self) -> impl Iterator<Item = &Vec<TaggedValue>> {
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

    fn append(&mut self, id: PublisherId, value: TaggedValue) {
        self.entries
            .get_mut(&id)
            .expect("payload roots are appended only for a registered publisher")
            .push(value);
    }

    #[cfg(test)]
    fn len_of(&self, id: PublisherId) -> usize {
        self.entries.get(&id).map(Vec::len).unwrap_or(0)
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

    /// Append every head `payload` yields. Callers run this before the
    /// publication Release store, under the same exclusive heap borrow as
    /// registration.
    pub(crate) fn append_payload_roots<P: PayloadRoots>(&mut self, id: PublisherId, payload: &P) {
        let roots = &mut self.published_payload_roots;
        payload.for_each_root(&mut |value| roots.append(id, value));
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
