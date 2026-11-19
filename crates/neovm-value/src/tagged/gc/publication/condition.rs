//! Condition-snapshot publication (P10-a2 / A2-S3+S4).
//!
//! The table's epoch is the universal plist clock, read once before the head
//! reads. Each entry is `{symbol, plist_head}`. A non-cons head is stored and
//! is not an entry for lookup, matching `Obarray::symbol_plist_snapshot_id`.
//! Heap-object heads are copied into the heap root table when the payload
//! becomes current. Immediate heads are not: they are not swept.

use super::roots::PublisherId;
use super::{
    Commit, PublicationChange, PublicationError, PublicationLedger, PublicationPayload,
    PublicationView, RetainedBytes, RetentionLimits,
};
use crate::intern::SymId;
use crate::symbol::{Obarray, plist_epoch};
use crate::tagged::gc::TaggedHeap;
use crate::tagged::value::TaggedValue;
use rustc_hash::FxHashMap;
use std::mem::size_of;

/// One symbol's plist head at a table's epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConditionSnapshot {
    pub(crate) symbol: SymId,
    pub(crate) plist_head: TaggedValue,
}

/// Immutable plist-head map. A refresh builds a new table.
#[derive(Debug)]
pub(crate) struct ConditionSnapshotTable {
    epoch: u64,
    heads: FxHashMap<SymId, TaggedValue>,
}

impl ConditionSnapshotTable {
    /// Epoch first, then one `plist.get()` per present symbol, uninterned
    /// slots included.
    fn capture(obarray: &Obarray) -> Self {
        let epoch = plist_epoch();
        let mut heads = FxHashMap::default();
        for (symbol, slot) in obarray.iter_symbols() {
            heads.insert(symbol, slot.plist.get());
        }
        Self { epoch, heads }
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(crate) fn len(&self) -> usize {
        self.heads.len()
    }

    pub(crate) fn snapshot(&self, symbol: SymId) -> Option<ConditionSnapshot> {
        self.heads
            .get(&symbol)
            .copied()
            .map(|plist_head| ConditionSnapshot { symbol, plist_head })
    }

    /// `None` when the symbol is absent or the stored head is not a cons.
    pub(crate) fn cons_head(&self, symbol: SymId) -> Option<TaggedValue> {
        let head = self.heads.get(&symbol).copied()?;
        head.is_cons().then_some(head)
    }

    fn root_count(&self) -> usize {
        self.heads
            .values()
            .filter(|head| head.is_heap_object())
            .count()
    }

    fn table_bytes(&self) -> usize {
        size_of::<Self>().saturating_add(
            self.heads
                .capacity()
                .saturating_mul(size_of::<(SymId, TaggedValue)>()),
        )
    }

    fn quote_bytes(&self) -> usize {
        self.table_bytes()
            .saturating_add(self.root_count().saturating_mul(size_of::<TaggedValue>()))
    }
}

struct ConditionPayload {
    table: Box<ConditionSnapshotTable>,
}

impl ConditionPayload {
    fn capture(obarray: &Obarray) -> Self {
        Self {
            table: Box::new(ConditionSnapshotTable::capture(obarray)),
        }
    }

    fn same_snapshot(&self, other: &Self) -> bool {
        self.table.epoch == other.table.epoch
            && self.table.heads.len() == other.table.heads.len()
            && self.table.heads.iter().all(|(symbol, head)| {
                other
                    .table
                    .heads
                    .get(symbol)
                    .is_some_and(|previous| previous.bits() == head.bits())
            })
    }
}

impl PublicationPayload for ConditionPayload {
    type Error = ();
    type Generation = u64;

    fn published_address(&self) -> *mut u8 {
        std::ptr::from_ref(self.table.as_ref()).cast_mut().cast()
    }

    fn retained_bytes(&self) -> RetainedBytes {
        RetainedBytes::new(self.table.quote_bytes())
    }

    fn storage_overhead() -> usize {
        size_of::<Self>()
    }

    fn generation(&self) -> u64 {
        self.table.epoch()
    }

    fn for_each_root(&self, visit: &mut dyn FnMut(TaggedValue)) {
        for head in self.table.heads.values().copied() {
            if head.is_heap_object() {
                visit(head);
            }
        }
    }

    fn prepare(_current: &Self) -> Result<super::PayloadPlan<Self>, Self::Error> {
        // The refreshed table is built from the obarray, which `prepare`
        // cannot see. Refresh commits through `Commit::Replace`.
        Ok(super::PayloadPlan::Unchanged)
    }
}

enum RefreshPlan {
    Keep,
    Replace(ConditionPayload),
}

/// One condition-snapshot ledger. The heap borrow is how Drop unregisters
/// the id. The heap does not store an address of this owner. Forgetting the
/// owner skips Drop and leaves the roots in place.
pub(crate) struct PublishedConditionSnapshots<'heap> {
    ledger: PublicationLedger<ConditionPayload>,
    id: PublisherId,
    heap: &'heap mut TaggedHeap,
}

impl<'heap> PublishedConditionSnapshots<'heap> {
    pub(crate) fn mint(
        heap: &'heap mut TaggedHeap,
        obarray: &Obarray,
        limits: RetentionLimits,
    ) -> Result<Self, PublicationError<()>> {
        let id = heap.register_payload_publisher();
        let initial = ConditionPayload::capture(obarray);
        match PublicationLedger::create_noting(initial, limits, |payload| {
            heap.append_payload_roots(id, payload);
        }) {
            Ok(ledger) => Ok(Self { ledger, id, heap }),
            Err(error) => {
                heap.unregister_payload_publisher(id);
                Err(error)
            }
        }
    }

    pub(crate) fn refresh(
        &mut self,
        obarray: &Obarray,
    ) -> Result<PublicationChange<u64>, PublicationError<()>> {
        let next = ConditionPayload::capture(obarray);
        let Self { ledger, id, heap } = self;
        let id = *id;
        let accounted = ledger.account(|admission| {
            if admission.current().same_snapshot(&next) {
                return Ok(RefreshPlan::Keep);
            }
            admission.admit(next.retained_bytes())?;
            Ok(RefreshPlan::Replace(next))
        });
        accounted.result?.finish_noting(
            |plan, _current| match plan {
                RefreshPlan::Keep => Ok(Commit::Keep),
                RefreshPlan::Replace(next) => Ok(Commit::Replace(next)),
            },
            |payload| heap.append_payload_roots(id, payload),
        )
    }

    pub(crate) fn view(&self) -> PublicationView<'_, ConditionPayload> {
        self.ledger.view()
    }

    /// The table a view loaded. The address stays valid while this owner
    /// retains the payload, including after that payload becomes a predecessor.
    ///
    /// # Safety
    /// `address` is `view().published_address()` from this owner, and this
    /// owner is still alive.
    #[cfg(test)]
    pub(crate) fn table_at(&self, address: *mut u8) -> &ConditionSnapshotTable {
        let _retain = self;
        // SAFETY: the caller passes an address this ledger published. The box
        // allocation does not move when the payload joins the predecessor vector.
        unsafe { &*address.cast::<ConditionSnapshotTable>() }
    }

    #[cfg(test)]
    fn registry_len(&self) -> usize {
        self.heap.payload_root_count_for_test(self.id)
    }

    #[cfg(test)]
    fn committed_root_count(&self) -> usize {
        self.ledger.with_retained(|current, predecessors| {
            let mut count = 0;
            current.for_each_root(&mut |_| count += 1);
            for predecessor in predecessors {
                predecessor.for_each_root(&mut |_| count += 1);
            }
            count
        })
    }

    #[cfg(test)]
    fn statistics(&self) -> Result<super::PublicationStatistics<u64>, PublicationError<()>> {
        self.ledger.statistics()
    }
}

impl Drop for PublishedConditionSnapshots<'_> {
    fn drop(&mut self) {
        self.heap.unregister_payload_publisher(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::{intern, make_uninterned_symbol};
    use crate::symbol::plist_core::plist_get;
    use crate::tagged::gc::set_tagged_heap;
    use crate::value::{HashTableTest, HashTableWeakness, LispHashTable};

    fn retired_once() -> RetentionLimits {
        RetentionLimits::new(
            RetentionLimits::DEFAULT_BYTES,
            super::super::RetiredCount::new(1),
        )
    }

    fn pair(heap: &mut TaggedHeap, prop: TaggedValue, value: TaggedValue) -> TaggedValue {
        let tail = heap.alloc_cons(value, TaggedValue::NIL);
        heap.alloc_cons(prop, tail)
    }

    fn install() -> TaggedHeap {
        let mut heap = TaggedHeap::new();
        set_tagged_heap(&mut heap);
        heap
    }

    #[test]
    fn overwritten_plist_head_survives_a_full_collection() {
        let mut heap = install();
        let mut obarray = Obarray::new();
        let symbol = make_uninterned_symbol("p10-a2-surviving-symbol");
        let bare = make_uninterned_symbol("p10-a2-fixnum-plist");
        let prop = TaggedValue::from_sym_id(intern("p10-a2-surviving-prop"));
        let old_head = pair(&mut heap, prop, TaggedValue::fixnum(42));
        let new_head = pair(&mut heap, prop, TaggedValue::fixnum(43));
        let garbage = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
        obarray.set_symbol_plist_id(symbol, old_head);
        obarray.set_symbol_plist_id(bare, TaggedValue::fixnum(5));
        assert!(!obarray.is_global_member(symbol));

        let mut published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, retired_once()).unwrap();
        let before = published.view().published_address();
        let captured = published.table_at(before);
        assert_eq!(captured.len(), obarray.iter_symbols().count());
        assert_eq!(captured.epoch(), plist_epoch());
        assert!(captured.cons_head(symbol).is_some());
        let bare_entry = captured.snapshot(bare).unwrap();
        assert_eq!(bare_entry.plist_head, TaggedValue::fixnum(5));
        assert!(captured.cons_head(bare).is_none());
        assert_eq!(published.registry_len(), 1);
        assert_eq!(published.registry_len(), published.committed_root_count());

        obarray.set_symbol_plist_id(symbol, new_head);
        published.heap.collect_exact(std::iter::once(new_head));
        assert!(published.heap.owns_heap_value_for_test(old_head));
        assert!(published.heap.owns_heap_value_for_test(new_head));
        assert!(!published.heap.owns_heap_value_for_test(garbage));
        assert_eq!(
            plist_get(published.table_at(before).cons_head(symbol).unwrap(), &prop),
            Some(TaggedValue::fixnum(42))
        );
        assert_eq!(obarray.symbol_plist_id(symbol), new_head);

        let published_change = published.refresh(&obarray).unwrap();
        let PublicationChange::Published { generation, .. } = published_change else {
            panic!("the overwritten head must publish a new table");
        };
        assert!(generation > published.table_at(before).epoch());
        assert_eq!(
            plist_get(published.table_at(before).cons_head(symbol).unwrap(), &prop),
            Some(TaggedValue::fixnum(42))
        );
        let current = published.view().published_address();
        assert_ne!(current, before);
        assert_eq!(
            plist_get(
                published.table_at(current).cons_head(symbol).unwrap(),
                &prop
            ),
            Some(TaggedValue::fixnum(43))
        );
        assert_eq!(published.registry_len(), 2);
        assert_eq!(published.registry_len(), published.committed_root_count());

        let unchanged = published.refresh(&obarray).unwrap();
        assert_eq!(unchanged, PublicationChange::Unchanged { generation });
        assert_eq!(published.registry_len(), 2);
        let words = size_of::<TaggedValue>();
        let quote = published.statistics().unwrap().retained_bytes.get();
        assert!(quote >= published.registry_len() * words);
    }

    #[test]
    fn need_grace_keeps_the_previous_plist_head_across_a_collection() {
        let mut heap = install();
        let mut obarray = Obarray::new();
        let symbol = make_uninterned_symbol("p10-a2-need-grace-symbol");
        let prop = TaggedValue::from_sym_id(intern("p10-a2-need-grace-prop"));
        let first = pair(&mut heap, prop, TaggedValue::fixnum(1));
        let second = pair(&mut heap, prop, TaggedValue::fixnum(2));
        let refused = pair(&mut heap, prop, TaggedValue::fixnum(3));
        obarray.set_symbol_plist_id(symbol, first);

        let mut published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, retired_once()).unwrap();
        let first_address = published.view().published_address();
        obarray.set_symbol_plist_id(symbol, second);
        assert!(matches!(
            published.refresh(&obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        let current = published.view().published_address();
        obarray.set_symbol_plist_id(symbol, refused);
        let error = published.refresh(&obarray).unwrap_err();
        assert_eq!(
            error,
            PublicationError::NeedGrace(super::super::RetentionLimit::Retired)
        );
        assert_eq!(published.view().published_address(), current);
        assert_eq!(published.registry_len(), 2);

        published.heap.collect_exact(std::iter::empty());
        assert!(published.heap.owns_heap_value_for_test(first));
        assert!(published.heap.owns_heap_value_for_test(second));
        assert!(!published.heap.owns_heap_value_for_test(refused));
        assert_eq!(
            plist_get(
                published.table_at(first_address).cons_head(symbol).unwrap(),
                &prop
            ),
            Some(TaggedValue::fixnum(1))
        );
        assert_eq!(
            plist_get(
                published.table_at(current).cons_head(symbol).unwrap(),
                &prop
            ),
            Some(TaggedValue::fixnum(2))
        );
        obarray.set_symbol_plist_id(symbol, second);
    }

    #[test]
    fn unregistering_the_publisher_makes_the_old_head_collectable() {
        let mut heap = install();
        let mut obarray = Obarray::new();
        let symbol = make_uninterned_symbol("p10-a2-unregistered-symbol");
        let prop = TaggedValue::from_sym_id(intern("p10-a2-unregistered-prop"));
        let head = pair(&mut heap, prop, TaggedValue::fixnum(7));
        let table = heap.alloc_hash_table(LispHashTable::new_with_options(
            HashTableTest::Eq,
            4,
            Some(HashTableWeakness::Key),
            1.5,
            0.8,
        ));
        crate::tagged::mutate::with_hash_table_mut(table, |hash| {
            hash.insert(
                head.to_hash_key(&HashTableTest::Eq),
                head,
                TaggedValue::fixnum(9),
            );
        });
        obarray.set_symbol_plist_id(symbol, head);

        {
            let mut published = PublishedConditionSnapshots::mint(
                &mut heap,
                &obarray,
                RetentionLimits::production(),
            )
            .unwrap();
            obarray.set_symbol_plist_id(symbol, TaggedValue::NIL);
            published.heap.collect_exact(std::iter::once(table));
            assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
            assert!(published.heap.owns_heap_value_for_test(head));
            let address = published.view().published_address();
            assert_eq!(
                plist_get(
                    published.table_at(address).cons_head(symbol).unwrap(),
                    &prop
                ),
                Some(TaggedValue::fixnum(7))
            );
        }

        heap.collect_exact(std::iter::once(table));
        assert_eq!(table.as_hash_table().unwrap().data.len(), 0);
        assert!(!heap.owns_heap_value_for_test(head));
    }

    #[test]
    fn termination_reseed_traces_a_head_committed_after_concurrent_begin() {
        let mut heap = install();
        // SAFETY: this thread holds the only heap borrow. No other mutator runs.
        unsafe { heap.publish_barrier_window_for_test() };
        let mut obarray = Obarray::new();
        let symbol = make_uninterned_symbol("p10-a2-reseed-symbol");
        let prop = TaggedValue::from_sym_id(intern("p10-a2-reseed-prop"));
        let head = pair(&mut heap, prop, TaggedValue::fixnum(11));
        let garbage = heap.alloc_cons(TaggedValue::fixnum(99), TaggedValue::NIL);

        let mut published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, RetentionLimits::production())
                .unwrap();
        assert_eq!(published.registry_len(), 0);
        published.heap.concurrent_begin();
        let start_roots = published.heap.handshake.last_start_runtime_roots;
        obarray.set_symbol_plist_id(symbol, head);
        assert!(matches!(
            published.refresh(&obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        assert_eq!(published.registry_len(), 1);
        published.heap.launch_concurrent_mark();
        while !published.heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        published.heap.join_concurrent_mark();
        published.heap.reseed_runtime_and_remembered_roots();
        assert_eq!(
            published.heap.handshake.last_term_runtime_roots,
            start_roots + 1
        );
        let bytes_before = published.heap.live_bytes();
        published.heap.incremental_drain_all();
        published
            .heap
            .incremental_finish(bytes_before, std::time::Instant::now());
        published.heap.finish_incremental_sweep_now();
        assert!(!published.heap.sweep_in_progress());
        assert!(published.heap.owns_heap_value_for_test(head));
        assert!(!published.heap.owns_heap_value_for_test(garbage));
        let address = published.view().published_address();
        assert_eq!(
            plist_get(
                published.table_at(address).cons_head(symbol).unwrap(),
                &prop
            ),
            Some(TaggedValue::fixnum(11))
        );
    }
}
