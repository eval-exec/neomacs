//! Runtime-owned storage behind permanent image objects still paces Lisp GC.
//!
//! GNU alloc.c counts hash_table_allocated_bytes separately from the dumped
//! hash object. Thawing/growing a dumped table allocates live backing arrays,
//! even though its image-resident header never enters the ordinary sweep.

use super::*;

/// Charge a completed serial backing allocation only on its allocating path.
/// Shared storage receives this reporter from its published mutation capability;
/// detached builders and sealed owners keep their own allocation accounting.
#[cold]
#[inline(never)]
pub(crate) fn note_serial_backing_growth(bytes: usize) {
    if bytes == 0 {
        return;
    }
    with_store_state(|store| {
        // SAFETY: this trusted reporter runs inside a serialized mutation with
        // no callback or safepoint. The installed owner stays live; project
        // only its allocation word, disjoint from the external table payload
        // and the shared StoreState tail. Detached stores remain uncharged.
        unsafe { super::string_pacing::charge_backing(store.heap.get(), bytes) };
    });
}

#[derive(Default)]
pub(super) struct ImageHashTableRegistry {
    tables: rustc_hash::FxHashSet<usize>,
    #[cfg(test)]
    visited: std::cell::Cell<usize>,
}

#[cfg(feature = "gc-memory-telemetry")]
impl ImageHashTableRegistry {
    pub(super) fn backing_bytes(&self) -> usize {
        self.tables.capacity().saturating_mul(size_of::<usize>())
    }
}

impl TaggedHeap {
    /// # Safety
    /// Loader admission proves this is a complete live vectorlike header,
    /// with stable image ownership until reclaim or heap teardown.
    pub(super) unsafe fn register_image_hash_table(&mut self, header: *const VecLikeHeader) {
        // SAFETY: the caller's loader admission proves the header is readable.
        if unsafe { (*header).type_tag } == VecLikeType::HashTable {
            self._satb_layout_reserve
                .image_hash_tables_mut()
                .tables
                .insert(header as usize);
        }
    }

    pub(super) fn unregister_image_hash_table(&mut self, header: *const VecLikeHeader) {
        self._satb_layout_reserve
            .image_hash_tables_mut()
            .tables
            .remove(&(header as usize));
    }

    #[cfg(test)]
    fn image_hash_census_visited_for_test(&self) -> usize {
        self._satb_layout_reserve.image_hash_tables().visited.get()
    }

    /// Verify the incremental index before dereferencing any indexed pointer.
    /// Only tests and the explicit partition verifier pay for the full walk.
    fn verify_image_hash_table_registry(&self) {
        let mut expected = rustc_hash::FxHashSet::default();
        for &address in &self.non_cons_object_addrs {
            let header = address as *const GcHeader;
            // SAFETY: the stopped exact owned registry contains initialized,
            // live headers after sweep; no mutator advances during this walk.
            unsafe {
                if (*header).flags.image_resident()
                    && (*header).kind == HeapObjectKind::VecLike
                    && (*header.cast::<VecLikeHeader>()).type_tag == VecLikeType::HashTable
                {
                    expected.insert(address);
                }
            }
        }
        for object in &self.mapped_veclike_objects {
            // SAFETY: loader-registered headers remain live until heap teardown.
            if unsafe { (*object.header).type_tag } == VecLikeType::HashTable {
                expected.insert(object.header as usize);
            }
        }
        assert_eq!(
            self._satb_layout_reserve.image_hash_tables().tables,
            expected,
            "image hash registry differs from loader provenance"
        );
    }

    /// Post-sweep pacing visits only registered image hash tables. In
    /// particular, minor GC never filters unrelated owned or mapped headers.
    pub(super) fn image_hash_table_storage_bytes(&self) -> usize {
        if cfg!(test) || std::env::var("NEOVM_GC_VERIFY_PARTITION").as_deref() == Ok("1") {
            self.verify_image_hash_table_registry();
        }
        let registry = self._satb_layout_reserve.image_hash_tables();
        #[cfg(test)]
        registry.visited.set(0);
        let mut bytes = 0usize;
        for &address in &registry.tables {
            #[cfg(test)]
            registry.visited.set(registry.visited.get() + 1);
            // SAFETY: loader admission inserts only image hash tables; owned
            // reclaim removes them before freeing. Mapped entries outlive the
            // heap. Post-sweep admission retains the stopped table adapter.
            bytes = bytes.saturating_add(unsafe {
                self.collector_tables()
                    .inspect(address as *mut HashTableObj, |table| {
                        table.data.materialized_storage_bytes()
                    })
            });
        }
        bytes
    }
}

/// Serial backing growth is charged around the public mutation/hydration
/// path. Sealed owners call the shared inactive body directly and already
/// charge their own state, so that body deliberately has no such guard.
pub(crate) struct SerialHashTableStorageCharge {
    table: *mut HashTableObj,
    before: usize,
}

impl SerialHashTableStorageCharge {
    /// # Safety
    /// The caller supplies the same serial access contract as [`Self::new`].
    pub(crate) unsafe fn for_value(value: TaggedValue) -> Option<Self> {
        if value.veclike_type()? != VecLikeType::HashTable {
            return None;
        }
        let table = value.as_veclike_ptr()?.cast_mut().cast::<HashTableObj>();
        // SAFETY: the type check establishes a table; access and lifetime
        // remain the caller's serial mutation contract.
        Some(unsafe { Self::new(table) })
    }

    /// # Safety
    /// The caller owns serial table access (and any active Tier-H mutation
    /// lease). The initialized table must outlive this guard, with no Lisp
    /// callbacks, collection, or escaping mutable payload borrow before Drop.
    pub(crate) unsafe fn new(table: *mut HashTableObj) -> Self {
        Self {
            table,
            // SAFETY: the caller's exclusive serial access precedes any
            // mutation and holds no competing mutable payload reference.
            before: unsafe { (*table).table.data.materialized_storage_bytes() },
        }
    }
}

impl Drop for SerialHashTableStorageCharge {
    fn drop(&mut self) {
        // SAFETY: the guard's contract keeps the table live; the caller's
        // payload borrow has ended, including when a host closure unwinds.
        let after = unsafe { (*self.table).table.data.materialized_storage_bytes() };
        let grown = after.saturating_sub(self.before);
        if grown != 0 {
            with_tagged_heap(|heap| heap.note_allocation_bytes(grown));
        }
    }
}

#[cfg(test)]
#[path = "tests/image_hash_pacing_test.rs"]
mod image_hash_pacing_tests;
