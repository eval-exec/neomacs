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

impl<'a> HashTableWrite<'a> {
    pub(super) fn new(table: &'a mut LispHashTable) -> Self {
        Self {
            table,
            _thread: PhantomData,
        }
    }

    /// Overwrites touch only the value slot: no capacity read, growth
    /// reporter, or allocation-accounting heap lookup is needed.
    #[inline]
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

    /// Probe once, then unlink the found entry. Spare free-list capacity
    /// requires no allocation census or allocation-accounting owner lookup.
    #[inline]
    pub fn remove(&mut self, key: Value, symbols_with_pos_enabled: bool) -> Result<(), Flow> {
        let Some((hash, slot)) =
            self.table
                .data
                .try_entry_by_value(key, self.table.test, symbols_with_pos_enabled)?
        else {
            return Ok(());
        };
        if self.table.data.free_slots.len() == self.table.data.free_slots.capacity() {
            self.table
                .data
                .remove_found_with_growth(hash, slot, charge_growth);
        } else {
            self.table.data.remove_found(hash, slot);
        }
        Ok(())
    }
}

impl HashTableStorage {
    /// Retain the original out-of-line insertion boundary. Inlining the
    /// allocating body into the native hot loop displaced journal inlining
    /// and added nine instructions to the measured fixed-key overwrite path.
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
    #[inline]
    pub(super) fn remove_found(&mut self, hash: u64, slot: usize) -> Option<Value> {
        let removed = self
            .index
            .remove_matching(hash, |entry| entry.slot == slot)
            .expect("the entry just found is in the index");
        // Use the removed key so user-hash memo maps retain their invariant.
        self.forget_user_hash(&removed.key);
        self.remove_slot(slot)
    }

    /// Only a full reusable-slot Vec can allocate during removal. Measure
    /// before unlinking: hashbrown's usable index capacity may fall on erase,
    /// and the serial contract charges the positive net materialized delta.
    #[cold]
    #[inline(never)]
    fn remove_found_with_growth(&mut self, hash: u64, slot: usize, mut report: impl FnMut(usize)) {
        let before = self.materialized_storage_bytes();
        self.remove_found(hash, slot);
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
