//! Closed fixture permission for keeper-only weak sweeping. Membership is not
//! liveness: every use first inventories the actual live heap catalog.

use super::super::TaggedHeap;
use super::*;
#[cfg(test)]
use crate::tagged::header::HashTableObj;
use crate::tagged::header::{GcHeader, HeapObjectKind, VecLikeHeader, VecLikeType};
use crate::tagged::value::TaggedValue;
use rustc_hash::FxHashSet;
use std::sync::Weak;

pub(super) struct WeakSweepPermit {
    heap: HeapIdentity,
    registry: Option<Weak<Registry>>,
    tables: FxHashSet<usize>,
    _thread: PhantomData<Rc<()>>,
}

impl WeakSweepPermit {
    /// Only the serial keeper's explicit fixture admission can mint permission.
    /// Validate catalog membership before interpreting any caller-supplied word.
    #[cfg(test)]
    pub(super) fn mint<K: EraKeeper>(
        heap: &TaggedHeap,
        keeper: &K,
        tables: &[TaggedValue],
    ) -> Result<Self, RegistrationError> {
        if heap.heap_identity() != keeper.parallel_heap_identity() {
            return Err(RegistrationError::WrongHeap);
        }
        keeper.keeper_designator()?;
        let mut allowed = FxHashSet::default();
        for table in tables {
            let address = table.bits() & !crate::tagged::value::TAG_MASK;
            if !heap.non_cons_object_addrs.contains(&address) {
                return Err(RegistrationError::Publication);
            }
            // SAFETY: catalog membership retains this initialized owned header;
            // mint runs under the sole serial collector/keeper authority.
            unsafe {
                let header = &*(address as *const GcHeader);
                if header.kind != HeapObjectKind::VecLike
                    || (*(address as *const VecLikeHeader)).type_tag != VecLikeType::HashTable
                    || table.gc_veclike_type() != Some(VecLikeType::HashTable)
                {
                    return Err(RegistrationError::Publication);
                }
                let eligible =
                    heap.collector_tables()
                        .inspect(address as *mut HashTableObj, |ht| {
                            ht.weakness.is_some()
                                && ht.data.pending_entries().is_none()
                                && ht.test_depth.load(Ordering::Acquire) == 0
                                && ht.data.authority.load(Ordering::Acquire) == 0
                        });
                if !eligible || header.generation.permanent() || !header.flags.boxed() {
                    return Err(RegistrationError::Publication);
                }
            }
            allowed.insert(address);
        }
        Ok(Self {
            heap: heap.heap_identity(),
            registry: None,
            tables: allowed,
            _thread: PhantomData,
        })
    }

    #[cfg(test)]
    pub(super) fn bind(mut self, registry: &Arc<Registry>) -> Self {
        debug_assert_eq!(self.heap, registry.heap);
        debug_assert!(self.registry.is_none());
        self.registry = Some(Arc::downgrade(registry));
        self
    }

    pub(super) fn covers(&self, heap: &TaggedHeap, address: usize) -> bool {
        self.heap == heap.heap_identity() && self.tables.contains(&address)
    }

    /// This is a new mint of actual stop authority, not a cached payload loan.
    /// Run before image transfer, marking, or a CollectorTables raw visit that
    /// would otherwise panic on retained reader/writer authority.
    pub(super) fn validate_stopped(
        &self,
        heap: &TaggedHeap,
        stopped: &AllStopped<'_>,
        registry: &Arc<Registry>,
    ) -> Result<(), CollectionError> {
        if self.heap != heap.heap_identity()
            || !self
                .registry
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|bound| Arc::ptr_eq(&bound, registry))
            || !stopped.matches_registry(registry)
        {
            return Err(RendezvousError::Identity);
        }
        for &address in &heap.non_cons_object_addrs {
            // SAFETY: the actual stopped catalog retains initialized storage;
            // no owner, worker, callback or collector mutation overlaps here.
            let table = unsafe {
                let header = &*(address as *const GcHeader);
                if header.kind != HeapObjectKind::VecLike
                    || (*(address as *const VecLikeHeader)).type_tag != VecLikeType::HashTable
                {
                    continue;
                }
                TaggedValue::from_veclike_ptr(address as *const VecLikeHeader)
            };
            let lease = KeeperTableMut::enter(stopped, table)?;
            let accepted = lease.inspect(|ht| {
                ht.weakness.is_none()
                    || (self.covers(heap, address)
                        && ht.data.pending_entries().is_none()
                        && ht.test_depth.load(Ordering::Acquire) == 0)
            });
            if !accepted {
                return Err(RendezvousError::Publication);
            }
        }
        Ok(())
    }
}

static_assertions::assert_not_impl_any!(WeakSweepPermit: Send, Sync, Clone, Copy);
