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

/// One condition-snapshot ledger.
///
/// The heap is an argument to [`Self::mint`], [`Self::refresh`], and
/// [`Self::unregister`]. This owner does not borrow it, so the obarray and a
/// collection can reach the same heap through the TLS root while the publisher
/// is alive. The heap stores the minted id, not an address of this owner.
///
/// [`Self::unregister`] removes the id. Drop without that call leaves the
/// roots registered. A leaked id keeps the published heads alive, which is
/// the safe failure of the heap-owned root table.
pub(crate) struct PublishedConditionSnapshots {
    ledger: PublicationLedger<ConditionPayload>,
    /// `None` after [`Self::unregister`]. Drop does not remove the entry.
    id: Option<PublisherId>,
}

impl PublishedConditionSnapshots {
    pub(crate) fn mint(
        heap: &mut TaggedHeap,
        obarray: &Obarray,
        limits: RetentionLimits,
    ) -> Result<Self, PublicationError<()>> {
        let id = heap.register_payload_publisher();
        let initial = ConditionPayload::capture(obarray);
        match PublicationLedger::create_noting(initial, limits, |payload| {
            heap.append_payload_roots(id, payload);
        }) {
            Ok(ledger) => Ok(Self {
                ledger,
                id: Some(id),
            }),
            Err(error) => {
                heap.unregister_payload_publisher(id);
                Err(error)
            }
        }
    }

    pub(crate) fn refresh(
        &mut self,
        heap: &mut TaggedHeap,
        obarray: &Obarray,
    ) -> Result<PublicationChange<u64>, PublicationError<()>> {
        let next = ConditionPayload::capture(obarray);
        let id = self.id.expect("a live publisher is registered");
        let accounted = self.ledger.account(|admission| {
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

    /// Remove this publisher's roots.
    ///
    /// Drop without this call leaks the id. The collector still copies the
    /// values, so the leak keeps heads alive and does not dangle.
    pub(crate) fn unregister(mut self, heap: &mut TaggedHeap) {
        if let Some(id) = self.id.take() {
            heap.unregister_payload_publisher(id);
        }
    }

    pub(crate) fn view(&self) -> PublicationView<'_, ConditionPayload> {
        self.ledger.view()
    }

    /// The table `view` loaded from this owner.
    ///
    /// The address is read from the view, whose lifetime is the owner's, so
    /// no raw pointer crosses this API. The returned reference is tied to
    /// this owner. The owner retains the box after the payload becomes a
    /// predecessor, and the allocation does not move. `PublicationView` is
    /// not `Copy` and borrows this owner, so a view cannot cross [`Self::refresh`].
    #[cfg(test)]
    pub(crate) fn table_at<'a>(
        &'a self,
        view: &PublicationView<'_, ConditionPayload>,
    ) -> &'a ConditionSnapshotTable {
        let address = view.published_address();
        // SAFETY: `view` names a box this owner published. Publication does
        // not reclaim a payload, and moving that box into the predecessor
        // vector does not move the allocation. This reference cannot outlive
        // `self`, which retains the box.
        unsafe { &*address.cast::<ConditionSnapshotTable>() }
    }

    #[cfg(test)]
    fn registry_len(&self, heap: &TaggedHeap) -> usize {
        let id = self.id.expect("a live publisher is registered");
        heap.payload_root_count_for_test(id)
    }

    #[cfg(test)]
    fn cons_head_at_epoch(&self, epoch: u64, symbol: SymId) -> Option<TaggedValue> {
        self.ledger.with_retained(|current, predecessors| {
            std::iter::once(current)
                .chain(predecessors.iter())
                .find(|payload| payload.table.epoch() == epoch)
                .and_then(|payload| payload.table.cons_head(symbol))
        })
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

impl Drop for PublishedConditionSnapshots {
    fn drop(&mut self) {
        let Some(id) = self.id.take() else {
            return;
        };
        // The id stays in the heap table. This owner does not borrow the heap,
        // so Drop cannot unregister it. Keeping the roots is the safe failure.
        tracing::warn!(
            target: "neovm::gc::publication",
            ?id,
            "published condition snapshots dropped without unregister; payload roots leak"
        );
        if !std::thread::panicking() {
            debug_assert!(
                false,
                "PublishedConditionSnapshots dropped without unregister; payload roots leak"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::{intern, make_uninterned_symbol};
    use crate::symbol::plist_core::plist_get;
    use crate::tagged::gc::TestHeap;
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

    #[test]
    fn overwritten_plist_head_survives_a_full_collection() {
        let mut heap = TestHeap::new();
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
        let (before_address, before_epoch) = {
            let view = published.view();
            let captured = published.table_at(&view);
            assert_eq!(captured.len(), obarray.iter_symbols().count());
            assert_eq!(captured.epoch(), plist_epoch());
            assert!(captured.cons_head(symbol).is_some());
            let bare_entry = captured.snapshot(bare).unwrap();
            assert_eq!(bare_entry.plist_head, TaggedValue::fixnum(5));
            assert!(captured.cons_head(bare).is_none());
            assert_eq!(published.registry_len(&heap), 1);
            assert_eq!(
                published.registry_len(&heap),
                published.committed_root_count()
            );

            obarray.set_symbol_plist_id(symbol, new_head);
            heap.collect_exact(std::iter::once(new_head));
            assert!(heap.owns_heap_value_for_test(old_head));
            assert!(heap.owns_heap_value_for_test(new_head));
            assert!(!heap.owns_heap_value_for_test(garbage));
            assert_eq!(
                plist_get(captured.cons_head(symbol).unwrap(), &prop),
                Some(TaggedValue::fixnum(42))
            );
            assert_eq!(obarray.symbol_plist_id(symbol), new_head);
            (view.published_address() as usize, captured.epoch())
        };

        let published_change = published.refresh(&mut heap, &obarray).unwrap();
        let PublicationChange::Published { generation, .. } = published_change else {
            panic!("the overwritten head must publish a new table");
        };
        assert!(generation > before_epoch);
        assert_eq!(
            plist_get(
                published.cons_head_at_epoch(before_epoch, symbol).unwrap(),
                &prop
            ),
            Some(TaggedValue::fixnum(42))
        );
        {
            let view = published.view();
            let current = published.table_at(&view);
            assert_ne!(view.published_address() as usize, before_address);
            assert!(current.epoch() > before_epoch);
            assert_eq!(
                plist_get(current.cons_head(symbol).unwrap(), &prop),
                Some(TaggedValue::fixnum(43))
            );
        }
        assert_eq!(published.registry_len(&heap), 2);
        assert_eq!(
            published.registry_len(&heap),
            published.committed_root_count()
        );

        let unchanged = published.refresh(&mut heap, &obarray).unwrap();
        assert_eq!(unchanged, PublicationChange::Unchanged { generation });
        assert_eq!(published.registry_len(&heap), 2);
        let words = size_of::<TaggedValue>();
        let quote = published.statistics().unwrap().retained_bytes.get();
        assert!(quote >= published.registry_len(&heap) * words);
        published.unregister(&mut heap);
    }

    #[test]
    fn need_grace_keeps_the_previous_plist_head_across_a_collection() {
        let mut heap = TestHeap::new();
        let mut obarray = Obarray::new();
        let symbol = make_uninterned_symbol("p10-a2-need-grace-symbol");
        let prop = TaggedValue::from_sym_id(intern("p10-a2-need-grace-prop"));
        let first = pair(&mut heap, prop, TaggedValue::fixnum(1));
        let second = pair(&mut heap, prop, TaggedValue::fixnum(2));
        let refused = pair(&mut heap, prop, TaggedValue::fixnum(3));
        obarray.set_symbol_plist_id(symbol, first);

        let mut published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, retired_once()).unwrap();
        let first_epoch = {
            let view = published.view();
            published.table_at(&view).epoch()
        };
        obarray.set_symbol_plist_id(symbol, second);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        let (current_address, current_epoch) = {
            let view = published.view();
            let current = published.table_at(&view);
            (view.published_address() as usize, current.epoch())
        };
        obarray.set_symbol_plist_id(symbol, refused);
        let error = published.refresh(&mut heap, &obarray).unwrap_err();
        assert_eq!(
            error,
            PublicationError::NeedGrace(super::super::RetentionLimit::Retired)
        );
        {
            let view = published.view();
            assert_eq!(view.published_address() as usize, current_address);
            assert_eq!(published.table_at(&view).epoch(), current_epoch);
        }
        assert_eq!(published.registry_len(&heap), 2);

        heap.collect_exact(std::iter::empty());
        assert!(heap.owns_heap_value_for_test(first));
        assert!(heap.owns_heap_value_for_test(second));
        assert!(!heap.owns_heap_value_for_test(refused));
        assert_eq!(
            plist_get(
                published.cons_head_at_epoch(first_epoch, symbol).unwrap(),
                &prop
            ),
            Some(TaggedValue::fixnum(1))
        );
        assert_eq!(
            plist_get(
                published.cons_head_at_epoch(current_epoch, symbol).unwrap(),
                &prop
            ),
            Some(TaggedValue::fixnum(2))
        );
        obarray.set_symbol_plist_id(symbol, second);
        published.unregister(&mut heap);
    }

    #[test]
    fn unregistering_the_publisher_makes_the_old_head_collectable() {
        let mut heap = TestHeap::new();
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

        let published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, RetentionLimits::production())
                .unwrap();
        obarray.set_symbol_plist_id(symbol, TaggedValue::NIL);
        heap.collect_exact(std::iter::once(table));
        assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
        assert!(heap.owns_heap_value_for_test(head));
        {
            let view = published.view();
            assert_eq!(
                plist_get(published.table_at(&view).cons_head(symbol).unwrap(), &prop),
                Some(TaggedValue::fixnum(7))
            );
        }
        published.unregister(&mut heap);

        heap.collect_exact(std::iter::once(table));
        assert_eq!(table.as_hash_table().unwrap().data.len(), 0);
        assert!(!heap.owns_heap_value_for_test(head));
    }

    #[test]
    fn termination_reseed_traces_a_head_committed_after_concurrent_begin() {
        let mut heap = TestHeap::new();
        // SAFETY: this thread is the only mutator. No other mutator runs.
        unsafe { heap.publish_barrier_window_for_test() };
        heap.install();
        let mut obarray = Obarray::new();
        let symbol = make_uninterned_symbol("p10-a2-reseed-symbol");
        let prop = TaggedValue::from_sym_id(intern("p10-a2-reseed-prop"));
        let head = pair(&mut heap, prop, TaggedValue::fixnum(11));
        let garbage = heap.alloc_cons(TaggedValue::fixnum(99), TaggedValue::NIL);

        let mut published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, RetentionLimits::production())
                .unwrap();
        assert_eq!(published.registry_len(&heap), 0);
        heap.concurrent_begin();
        let start_roots = heap.handshake.last_start_runtime_roots;
        obarray.set_symbol_plist_id(symbol, head);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        assert_eq!(published.registry_len(&heap), 1);
        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        assert_eq!(heap.handshake.last_term_runtime_roots, start_roots + 1);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
        assert!(heap.owns_heap_value_for_test(head));
        assert!(!heap.owns_heap_value_for_test(garbage));
        {
            let view = published.view();
            assert_eq!(
                plist_get(published.table_at(&view).cons_head(symbol).unwrap(), &prop),
                Some(TaggedValue::fixnum(11))
            );
        }
        published.unregister(&mut heap);
    }
}
