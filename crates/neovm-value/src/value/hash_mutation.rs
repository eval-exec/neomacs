//! A serial hash write capability, separate from unrestricted replacement.
//!
//! Shared storage reports capacity changes to an explicit caller. Detached
//! builders and sealed owners use the unmetered bodies; the former are billed
//! on heap allocation, and TableMut remains the latter's sole growth biller.

use super::*;
use crate::tagged::gc::note_serial_backing_growth as charge_growth;
use std::marker::PhantomData;
use std::rc::Rc;

/// Operations that cannot replace or expose the table's backing storage.
/// Construction is confined to a barriered, inactive serial mutation extent.
/// Its installed serialized owner stays live and unchanged for that extent;
/// callbacks, GC, heap installation changes and owner retirement are excluded.
/// A completed growth reports immediately, so a later unwind cannot lose it.
#[doc(hidden)]
pub struct HashTableWrite<'a> {
    table: &'a mut LispHashTable,
    _thread: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(HashTableWrite<'static>: Send, Sync, Clone, Copy);

/// A table identity whose read-access hydration has completed. Only the
/// existing inactive typed Value accessor can construct this private module's
/// witness, and it consumes the witness before invoking the typed callback.
/// The marker ties the copied handle to that accessor and forbids thread transfer;
/// the witness proves hydration only. The accessor's unsafe contract also
/// requires Tier-H to remain inactive and excludes Lisp callbacks, GC or
/// safepoints, heap installation changes and owner retirement until return.
pub(crate) struct HydratedHashTable<'a> {
    value: Value,
    _scope: PhantomData<(&'a Value, Rc<()>)>,
}

static_assertions::assert_not_impl_any!(HydratedHashTable<'static>: Send, Sync, Clone, Copy);

impl<'a> HydratedHashTable<'a> {
    #[inline]
    fn for_serial_write(value: &'a Value) -> Option<Self> {
        value.as_hash_table()?;
        Some(Self {
            value: *value,
            _scope: PhantomData,
        })
    }

    #[inline]
    pub(crate) fn into_value(self) -> Value {
        self.value
    }
}

impl Value {
    /// Edit through operations that report positive net backing growth.
    /// Unlike the unrestricted mutation closure, this capability cannot
    /// replace the table or expose its storage, so overwrites need no census.
    ///
    /// # Safety
    /// Tier-H must remain inactive until return. The closure must not invoke
    /// Lisp or collect, and the mode decision must not span a GC safepoint.
    /// The installed serialized owner must stay live and unchanged throughout
    /// the closure: no heap installation change or owner retirement is allowed.
    #[inline]
    #[doc(hidden)]
    pub unsafe fn with_hash_table_write_inactive<R>(
        self,
        f: impl FnOnce(&mut HashTableWrite<'_>) -> R,
    ) -> Option<R> {
        // The existing cold accessor charges lazy hydration before any
        // mutable payload loan. No witness or constructor escapes this module;
        // the callback receives only the typed write capability below.
        let hydrated = HydratedHashTable::for_serial_write(&self)?;
        mutate::with_hydrated_hash_table_mut_inactive(
            hydrated,
            // Keep the constructor forwarding in the payload loan. An
            // opaque closure ABI spills its environment and result even
            // when the inner mutation body has already been inlined.
            #[inline(always)]
            |table| f(&mut HashTableWrite::new(table)),
        )
    }
}

impl<'a> HashTableWrite<'a> {
    pub(super) fn new(table: &'a mut LispHashTable) -> Self {
        Self {
            table,
            _thread: PhantomData,
        }
    }

    /// Overwrites touch only the value slot: no capacity read, growth
    /// reporter, or allocation-accounting heap lookup is needed.
    /// Keep the capability wrapper in the native probe's original extent.
    #[inline(always)]
    pub fn put(
        &mut self,
        key: Value,
        value: Value,
        symbols_with_pos_enabled: bool,
    ) -> Result<(), Flow> {
        let test = self.table.test;
        match self
            .table
            .data
            .try_probe_for_insert(key, test, symbols_with_pos_enabled)?
        {
            HashProbe::Found(slot) => {
                if let Some(stored) = self.table.data.slot_value_mut(slot) {
                    *stored = value;
                }
            }
            HashProbe::Absent(hash) => {
                self.table.maybe_resize_for_insert(true);
                let hash_key = key.to_hash_key_for_insert(test, symbols_with_pos_enabled);
                self.table.data.insert_absent_with_growth(
                    hash,
                    hash_key,
                    key,
                    value,
                    charge_growth,
                );
            }
        }
        Ok(())
    }

    /// The canonical free-list push owns its capacity check. Observe actual
    /// growth afterward rather than prechecking and repeating the push check.
    /// Retain the underlying removal operation's original inline placement.
    #[inline(always)]
    pub fn remove(&mut self, key: Value, symbols_with_pos_enabled: bool) -> Result<(), Flow> {
        let test = self.table.test;
        self.table.data.remove_by_value_with_growth(
            key,
            test,
            symbols_with_pos_enabled,
            charge_growth,
        )
    }
}

impl HashTableStorage {
    /// Keep allocating insertion behind the existing absent-key boundary.
    /// Its capacity accounting stays separate from the overwrite slot store.
    #[inline(never)]
    pub(super) fn insert_absent_with_growth(
        &mut self,
        hash: u64,
        hash_key: HashKey,
        key: Value,
        value: Value,
        mut report: impl FnMut(usize),
    ) {
        debug_assert_eq!(hash, stored_hash(&hash_key, key));
        let slot = self.alloc_slot_with_growth(key, value, &mut report);
        self.index
            .insert_new_with_growth(hash, hash_key, slot, &mut report);
    }

    /// The unmetered removal body, also used by detached and sealed callers.
    /// The caller already found this exact slot; no comparison is repeated.
    /// This body formerly lived inside the always-inline try_remove_by_value;
    /// factoring it must preserve that hot-path boundary, not add an ABI thunk.
    #[inline(always)]
    pub(super) fn remove_found(&mut self, hash: u64, slot: usize) -> Option<Value> {
        let removed = self
            .index
            .remove_matching(hash, |entry| entry.slot == slot)
            .expect("the entry just found is in the index");
        // Use the removed key so user-hash memo maps retain their invariant.
        self.forget_user_hash(&removed.key);
        self.remove_slot(slot)
    }

    /// The canonical removal owns the only free-list push and its capacity
    /// check. A miss stays unmetered. Preserve the pre-erase index capacity:
    /// tombstones may lower it, offsetting a growing free-list allocation.
    #[inline]
    fn remove_by_value_with_growth(
        &mut self,
        key: Value,
        test: HashTableTest,
        symbols_with_pos_enabled: bool,
        mut report: impl FnMut(usize),
    ) -> Result<(), Flow> {
        // Identity lookup cannot signal. Keep it in this removal extent;
        // only the structural comparison needs the fallible lookup result.
        let found = match test {
            HashTableTest::Eq | HashTableTest::Eql => {
                self.identity_entry_by_value(key, test, symbols_with_pos_enabled)
            }
            HashTableTest::Equal => {
                self.equal_entry_by_value(key, equal_value_hash(key), symbols_with_pos_enabled)?
            }
        };
        let Some((hash, slot)) = found else {
            return Ok(());
        };
        let index_before = self.index.capacity();
        let free_before = self.free_slots.capacity();
        self.remove_found(hash, slot);
        if self.free_slots.capacity() > free_before {
            self.charge_removal_growth(index_before, free_before, &mut report);
        }
        Ok(())
    }

    /// Removal cannot change slot capacity. Reconstruct the original whole
    /// materialized-domain snapshot only after the free-list actually grows,
    /// retaining the same saturating byte calculation and index-loss offset.
    #[cold]
    #[inline(never)]
    fn charge_removal_growth(
        &self,
        index_before: usize,
        free_before: usize,
        report: &mut impl FnMut(usize),
    ) {
        let before = HashTableStorageCapacities {
            index: index_before,
            slots: self.slots.capacity(),
            free_slots: free_before,
        }
        .materialized_bytes();
        let grown = self.materialized_storage_bytes().saturating_sub(before);
        if grown != 0 {
            report(grown);
        }
    }

    /// With a no-op reporter this is also the detached/sealed allocation
    /// body. Only an exhausted Vec takes a capacity snapshot.
    #[inline]
    pub(super) fn alloc_slot_with_growth(
        &mut self,
        key: Value,
        value: Value,
        report: &mut impl FnMut(usize),
    ) -> usize {
        if let Some(slot) = self.free_slots.pop() {
            debug_assert!(self.slots[slot].is_none());
            self.slots[slot] = Some(HashTableEntry { key, value });
            slot
        } else {
            let slot = self.slots.len();
            if slot == self.slots.capacity() {
                let before = vec_storage_bytes(&self.slots);
                self.slots.push(Some(HashTableEntry { key, value }));
                let grown = vec_storage_bytes(&self.slots).saturating_sub(before);
                if grown != 0 {
                    report(grown);
                }
            } else {
                self.slots.push(Some(HashTableEntry { key, value }));
            }
            slot
        }
    }
}

#[cfg(test)]
#[path = "tests/hash_mutation_test.rs"]
mod tests;
