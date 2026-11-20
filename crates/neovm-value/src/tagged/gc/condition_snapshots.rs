//! Condition-snapshot publication (P10-a2 / A2-S3+S4).
//!
//! The table's epoch is the universal plist clock, read once before the head
//! reads. Each entry is `{symbol, plist_head}`. A non-cons head is stored and
//! is not an entry for lookup, matching `Obarray::symbol_plist_snapshot_id`.
//! Heap-object heads are copied into the heap root table when the payload
//! becomes current. Immediate heads are not: they are not swept.

use super::publication::{
    Commit, PayloadPlan, PublicationChange, PublicationError, PublicationLedger,
    PublicationPayload, PublicationStatistics, PublicationView, RetainedBytes, RetentionLimits,
};
use super::publication_roots::{PayloadRoots, PublisherId};
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

    fn prepare(_current: &Self) -> Result<PayloadPlan<Self>, Self::Error> {
        // The refreshed table is built from the obarray, which `prepare`
        // cannot see. Refresh commits through `Commit::Replace`.
        Ok(PayloadPlan::Unchanged)
    }
}

impl PayloadRoots for ConditionPayload {
    fn for_each_root(&self, visit: &mut dyn FnMut(TaggedValue)) {
        for head in self.table.heads.values().copied() {
            if head.is_heap_object() {
                visit(head);
            }
        }
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
            heap.append_payload_roots(id, payload)
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
        &self,
        heap: &mut TaggedHeap,
        obarray: &Obarray,
    ) -> Result<PublicationChange<u64>, PublicationError<()>> {
        let next = ConditionPayload::capture(obarray);
        let id = self.id.expect("a live publisher is registered");
        let accounted = self.ledger.account(|admission| {
            if admission.current().same_snapshot(&next) {
                return Ok(RefreshPlan::Keep);
            }
            // One predecessor, before the quote, and only at the retiree cap.
            // A live view skips this; `admit` then returns `NeedGrace`.
            let extra = next.retained_bytes().get();
            admission.reclaim_oldest_predecessor(extra, |generation| {
                heap.reclaim_payload_generation(id, generation)
                    .expect("the oldest predecessor generation is still registered");
            });
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

    /// The table `view` loaded.
    ///
    /// The result borrows `view`. A caller cannot pass one publisher's view
    /// and keep the table after that view's owner is dropped. The address
    /// stays inside this function.
    #[cfg(test)]
    pub(crate) fn table_at<'a>(
        view: &'a PublicationView<'_, ConditionPayload>,
    ) -> &'a ConditionSnapshotTable {
        let address = view.published_address();
        // SAFETY: the view retains the loaded address for its own lifetime,
        // and reclaim does not run while any view of that ledger is alive.
        // The address does not escape `table_at`.
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
    fn statistics(&self) -> Result<PublicationStatistics<u64>, PublicationError<()>> {
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

/// Why an explicit admission did not install a publisher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionSnapshotAdmitError {
    /// This admission is already `Active`.
    AlreadyActive,
    /// The ledger refused the initial table for capacity.
    Capacity,
    /// The ledger refused the initial table for a retention cap.
    NeedGrace,
    /// The publication metadata lock was busy.
    Busy,
    /// The publication metadata lock was poisoned.
    Poisoned,
    /// The payload adapter refused the initial table.
    Payload,
}

/// Public box payload for [`ConditionSnapshotAdmission::Active`].
///
/// The ledger stays crate-private. Eval can move this owner and cannot
/// mint or refresh it except through the admission methods.
pub struct ConditionSnapshotPublisher {
    inner: PublishedConditionSnapshots,
}

/// Serial admission of one condition-snapshot publisher.
///
/// `Dormant` is the constructor state and does not mint. `Active` is one
/// boxed publisher, so the enum is one pointer. A second [`Self::admit`]
/// returns [`ConditionSnapshotAdmitError::AlreadyActive`] and does not mint
/// another id. [`Self::unregister`] on `Dormant` does nothing.
pub enum ConditionSnapshotAdmission {
    Dormant,
    Active(Box<ConditionSnapshotPublisher>),
}

const _: () = assert!(size_of::<ConditionSnapshotAdmission>() == size_of::<*const ()>());
static_assertions::assert_not_impl_any!(ConditionSnapshotAdmission: Send, Sync);

impl ConditionSnapshotAdmission {
    /// Install one publisher on `heap` for the symbols in `obarray`.
    ///
    /// Uses [`RetentionLimits::production`]. Does not refresh later stores.
    pub fn admit(
        &mut self,
        heap: &mut TaggedHeap,
        obarray: &Obarray,
    ) -> Result<(), ConditionSnapshotAdmitError> {
        if !matches!(self, Self::Dormant) {
            return Err(ConditionSnapshotAdmitError::AlreadyActive);
        }
        let published =
            PublishedConditionSnapshots::mint(heap, obarray, RetentionLimits::production())
                .map_err(ConditionSnapshotAdmitError::from_publication)?;
        *self = Self::Active(Box::new(ConditionSnapshotPublisher { inner: published }));
        Ok(())
    }

    /// Remove this publisher's id. `Dormant` does not mint and does not
    /// touch the heap table.
    pub fn unregister(self, heap: &mut TaggedHeap) {
        let Self::Active(publisher) = self else {
            return;
        };
        publisher.inner.unregister(heap);
    }

    pub fn is_dormant(&self) -> bool {
        matches!(self, Self::Dormant)
    }
}

impl ConditionSnapshotAdmitError {
    fn from_publication(error: PublicationError<()>) -> Self {
        match error {
            PublicationError::Payload(()) => Self::Payload,
            PublicationError::Capacity => Self::Capacity,
            PublicationError::NeedGrace(_) => Self::NeedGrace,
            PublicationError::Busy => Self::Busy,
            PublicationError::Poisoned => Self::Poisoned,
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

    fn retired(count: usize) -> RetentionLimits {
        RetentionLimits::new(
            RetentionLimits::DEFAULT_BYTES,
            super::super::publication::RetiredCount::new(count),
        )
    }

    fn retired_once() -> RetentionLimits {
        retired(1)
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

        let published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, retired_once()).unwrap();
        let (before_address, before_epoch) = {
            let view = published.view();
            let captured = PublishedConditionSnapshots::table_at(&view);
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
            let current = PublishedConditionSnapshots::table_at(&view);
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

        let published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, retired_once()).unwrap();
        let first_epoch = {
            let view = published.view();
            PublishedConditionSnapshots::table_at(&view).epoch()
        };
        obarray.set_symbol_plist_id(symbol, second);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        let (current_address, current_epoch) = {
            let view = published.view();
            let current = PublishedConditionSnapshots::table_at(&view);
            (view.published_address() as usize, current.epoch())
        };
        // The cap reclaims one predecessor when no view is alive. This view
        // is what keeps the third refresh at `NeedGrace`.
        let held = published.view();
        obarray.set_symbol_plist_id(symbol, refused);
        let error = published.refresh(&mut heap, &obarray).unwrap_err();
        assert_eq!(
            error,
            PublicationError::NeedGrace(super::super::publication::RetentionLimit::Retired)
        );
        assert_eq!(held.published_address() as usize, current_address);
        {
            let view = published.view();
            assert_eq!(view.published_address() as usize, current_address);
            assert_eq!(
                PublishedConditionSnapshots::table_at(&view).epoch(),
                current_epoch
            );
        }
        drop(held);
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
                plist_get(
                    PublishedConditionSnapshots::table_at(&view)
                        .cons_head(symbol)
                        .unwrap(),
                    &prop
                ),
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

        let published =
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
                plist_get(
                    PublishedConditionSnapshots::table_at(&view)
                        .cons_head(symbol)
                        .unwrap(),
                    &prop
                ),
                Some(TaggedValue::fixnum(11))
            );
        }
        published.unregister(&mut heap);
    }

    /// Section 2. After one reclaim, a head that only the freed descriptor
    /// walked is collectable. A head that current or a kept predecessor still
    /// yields stays owned, including a head that also sat in the freed vec.
    #[test]
    fn reclaiming_one_predecessor_keeps_only_heads_a_live_descriptor_walks() {
        let mut heap = TestHeap::new();
        let mut obarray = Obarray::new();
        let freed_symbol = make_uninterned_symbol("p10-b-freed");
        let shared_symbol = make_uninterned_symbol("p10-b-shared");
        let kept_symbol = make_uninterned_symbol("p10-b-kept");
        let live_symbol = make_uninterned_symbol("p10-b-live");
        let prop = TaggedValue::from_sym_id(intern("p10-b-prop"));
        let freed = pair(&mut heap, prop, TaggedValue::fixnum(1));
        let shared = pair(&mut heap, prop, TaggedValue::fixnum(2));
        let kept = pair(&mut heap, prop, TaggedValue::fixnum(3));
        let live = pair(&mut heap, prop, TaggedValue::fixnum(4));
        let current_head = pair(&mut heap, prop, TaggedValue::fixnum(5));
        let garbage = heap.alloc_cons(TaggedValue::fixnum(9), TaggedValue::NIL);

        obarray.set_symbol_plist_id(freed_symbol, freed);
        obarray.set_symbol_plist_id(shared_symbol, shared);
        let published = PublishedConditionSnapshots::mint(&mut heap, &obarray, retired(2)).unwrap();

        obarray.set_symbol_plist_id(freed_symbol, TaggedValue::NIL);
        obarray.set_symbol_plist_id(kept_symbol, kept);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));

        obarray.set_symbol_plist_id(kept_symbol, TaggedValue::NIL);
        obarray.set_symbol_plist_id(live_symbol, live);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));

        obarray.set_symbol_plist_id(live_symbol, current_head);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));

        heap.collect_exact(std::iter::empty());
        assert!(
            !heap.owns_heap_value_for_test(freed),
            "a head only the freed descriptor walked stays owned"
        );
        assert!(
            heap.owns_heap_value_for_test(shared),
            "a head a kept generation still yields was collected"
        );
        assert!(
            heap.owns_heap_value_for_test(kept),
            "a kept predecessor's head was collected"
        );
        assert!(
            heap.owns_heap_value_for_test(live),
            "a kept predecessor's head was collected"
        );
        assert!(
            heap.owns_heap_value_for_test(current_head),
            "the current descriptor's head was collected"
        );
        assert!(!heap.owns_heap_value_for_test(garbage));
        assert_eq!(
            published.registry_len(&heap),
            published.committed_root_count()
        );
        published.unregister(&mut heap);
    }

    /// The retiree cap still refuses while a view is alive. Dropping the view
    /// lets the next replace reclaim one predecessor and publish.
    #[test]
    fn need_grace_while_a_view_holds_the_exhausted_retiree_cap() {
        let mut heap = TestHeap::new();
        let mut obarray = Obarray::new();
        let symbol = make_uninterned_symbol("p10-b-view-cap");
        let prop = TaggedValue::from_sym_id(intern("p10-b-view-prop"));
        let first = pair(&mut heap, prop, TaggedValue::fixnum(1));
        let second = pair(&mut heap, prop, TaggedValue::fixnum(2));
        let refused = pair(&mut heap, prop, TaggedValue::fixnum(3));
        obarray.set_symbol_plist_id(symbol, first);
        let published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, retired_once()).unwrap();

        let held = published.view();
        obarray.set_symbol_plist_id(symbol, second);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        let current_address = {
            let view = published.view();
            view.published_address() as usize
        };
        obarray.set_symbol_plist_id(symbol, refused);
        let error = published.refresh(&mut heap, &obarray).unwrap_err();
        assert_eq!(
            error,
            PublicationError::NeedGrace(super::super::publication::RetentionLimit::Retired)
        );
        {
            let view = published.view();
            assert_eq!(view.published_address() as usize, current_address);
        }
        assert_eq!(published.registry_len(&heap), 2);
        drop(held);

        heap.collect_exact(std::iter::empty());
        assert!(heap.owns_heap_value_for_test(first));
        assert!(heap.owns_heap_value_for_test(second));
        assert!(!heap.owns_heap_value_for_test(refused));

        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        heap.collect_exact(std::iter::empty());
        assert!(
            !heap.owns_heap_value_for_test(first),
            "dropping the view let reclaim free the oldest head"
        );
        assert!(heap.owns_heap_value_for_test(second));
        assert!(heap.owns_heap_value_for_test(refused));
        published.unregister(&mut heap);
    }

    /// Begin, publish a predecessor during the cycle, reclaim it, then reseed.
    /// Termination reads the post-reclaim map: the live head is owned and the
    /// reclaimed head is not.
    #[test]
    fn reseed_after_reclaim_traces_the_live_head_and_not_the_freed_one() {
        let mut heap = TestHeap::new();
        // SAFETY: this thread is the only mutator. No other mutator runs.
        unsafe { heap.publish_barrier_window_for_test() };
        heap.install();
        let mut obarray = Obarray::new();
        let symbol = make_uninterned_symbol("p10-b-reseed");
        let prop = TaggedValue::from_sym_id(intern("p10-b-reseed-prop"));
        let freed = pair(&mut heap, prop, TaggedValue::fixnum(1));
        let kept = pair(&mut heap, prop, TaggedValue::fixnum(2));
        let live = pair(&mut heap, prop, TaggedValue::fixnum(3));
        let garbage = heap.alloc_cons(TaggedValue::fixnum(99), TaggedValue::NIL);

        // No heap head yet, so begin does not gray `freed`.
        obarray.set_symbol_plist_id(symbol, TaggedValue::fixnum(1));
        let published =
            PublishedConditionSnapshots::mint(&mut heap, &obarray, retired_once()).unwrap();
        assert_eq!(published.registry_len(&heap), 0);
        heap.concurrent_begin();
        let start_roots = heap.handshake.last_start_runtime_roots;

        obarray.set_symbol_plist_id(symbol, freed);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        obarray.set_symbol_plist_id(symbol, kept);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        obarray.set_symbol_plist_id(symbol, live);
        assert!(matches!(
            published.refresh(&mut heap, &obarray).unwrap(),
            PublicationChange::Published { .. }
        ));
        assert_eq!(published.registry_len(&heap), 2);

        heap.launch_concurrent_mark();
        while !heap.concurrent_mark_done() {
            std::thread::yield_now();
        }
        heap.join_concurrent_mark();
        heap.reseed_runtime_and_remembered_roots();
        assert_eq!(heap.handshake.last_term_runtime_roots, start_roots + 2);
        let bytes_before = heap.live_bytes();
        heap.incremental_drain_all();
        heap.incremental_finish(bytes_before, std::time::Instant::now());
        heap.finish_incremental_sweep_now();
        assert!(!heap.sweep_in_progress());
        assert!(
            heap.owns_heap_value_for_test(live),
            "termination reseed missed the live head"
        );
        assert!(
            heap.owns_heap_value_for_test(kept),
            "termination reseed missed the kept predecessor"
        );
        assert!(
            !heap.owns_heap_value_for_test(freed),
            "termination reseed traced the reclaimed predecessor"
        );
        assert!(!heap.owns_heap_value_for_test(garbage));
        published.unregister(&mut heap);
    }

    /// A stale id, and an id this publisher never minted, must not remove a
    /// generation that is still live. The live head stays a traced root.
    #[test]
    fn stale_or_foreign_generation_cannot_drop_a_live_head() {
        let mut heap = TestHeap::new();
        let prop = TaggedValue::from_sym_id(intern("p10-b-gen-prop"));
        let only_live = pair(&mut heap, prop, TaggedValue::fixnum(1));
        let freed = pair(&mut heap, prop, TaggedValue::fixnum(2));
        let kept = pair(&mut heap, prop, TaggedValue::fixnum(3));
        let only_symbol = make_uninterned_symbol("p10-b-gen-only");
        let freed_symbol = make_uninterned_symbol("p10-b-gen-freed");
        let kept_symbol = make_uninterned_symbol("p10-b-gen-kept");

        let mut only_obarray = Obarray::new();
        only_obarray.set_symbol_plist_id(only_symbol, only_live);
        let only_id = heap.register_payload_publisher();
        let _only_generation =
            heap.append_payload_roots(only_id, &ConditionPayload::capture(&only_obarray));
        only_obarray.set_symbol_plist_id(only_symbol, TaggedValue::NIL);

        let mut stacked_obarray = Obarray::new();
        stacked_obarray.set_symbol_plist_id(freed_symbol, freed);
        let stacked_id = heap.register_payload_publisher();
        let freed_generation =
            heap.append_payload_roots(stacked_id, &ConditionPayload::capture(&stacked_obarray));
        stacked_obarray.set_symbol_plist_id(freed_symbol, TaggedValue::NIL);
        stacked_obarray.set_symbol_plist_id(kept_symbol, kept);
        let _kept_generation =
            heap.append_payload_roots(stacked_id, &ConditionPayload::capture(&stacked_obarray));
        stacked_obarray.set_symbol_plist_id(kept_symbol, TaggedValue::NIL);

        let foreign = super::super::publication::PayloadGeneration::from_raw(u64::MAX);
        assert!(
            !removed_generation(&mut heap, only_id, foreign),
            "a foreign generation reclaimed a slot"
        );
        assert_eq!(heap.payload_root_count_for_test(only_id), 1);
        assert!(
            !removed_generation(&mut heap, stacked_id, foreign),
            "a foreign generation reclaimed a slot"
        );
        assert_eq!(heap.payload_root_count_for_test(stacked_id), 2);

        heap.reclaim_payload_generation(stacked_id, freed_generation)
            .expect("the freed generation is still registered");
        assert_eq!(heap.payload_root_count_for_test(stacked_id), 1);
        assert!(
            !removed_generation(&mut heap, stacked_id, freed_generation),
            "a stale generation reclaimed a slot"
        );
        assert_eq!(heap.payload_root_count_for_test(stacked_id), 1);

        heap.collect_exact(std::iter::empty());
        assert!(
            heap.owns_heap_value_for_test(only_live),
            "a foreign generation dropped a live head"
        );
        assert!(
            heap.owns_heap_value_for_test(kept),
            "a stale generation dropped a live head"
        );
        assert!(
            !heap.owns_heap_value_for_test(freed),
            "the reclaimed generation's head stayed owned"
        );
        heap.unregister_payload_publisher(only_id);
        heap.unregister_payload_publisher(stacked_id);
    }

    /// `true` when reclaim removed a generation. A debug assert on an absent
    /// id unwinds; that is still a refusal, and the roots are unchanged
    /// because the assert runs before the removal.
    fn removed_generation(
        heap: &mut TestHeap,
        id: super::super::publication_roots::PublisherId,
        generation: super::super::publication::PayloadGeneration,
    ) -> bool {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            heap.reclaim_payload_generation(id, generation)
        }));
        match outcome {
            Err(_) => false,
            Ok(Err(_)) => false,
            Ok(Ok(())) => true,
        }
    }
}
