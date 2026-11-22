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
                debug_assert_eq!(hash, stored_hash(&hash_key, key));
                let slot = self
                    .table
                    .data
                    .alloc_slot_with_growth(key, value, &mut charge_growth);
                // The capability's fixed serial-owner extent justifies the
                // trusted reporter's lazy owner lookup. Shared storage itself
                // neither consults TLS nor selects an allocation owner.
                self.table.data.index.insert_new_with_growth(
                    hash,
                    hash_key,
                    slot,
                    &mut charge_growth,
                );
            }
        }
        Ok(())
    }
}

impl HashTableStorage {
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
