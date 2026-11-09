//! Sealed, rooted hash-table shell allocation during admitted overlap.
//! Storage is ordinary Rust heap memory; only shell list/catalog publication
//! uses the existing cold detached catalog. The Sim allocator stays unchanged.

use super::super::MemoryUseCountSlot;
use super::super::mark_word::MarkWord;
use super::{OwnerRootSlot, RegistrationError, RunAccess, TableRef};
use crate::tagged::header::{HashTableObj, VecLikeHeader, VecLikeType};
use crate::tagged::value::TaggedValue;
use crate::value::{HashTableTest, LispHashTable};

impl RunAccess<'_, '_> {
    /// Construct a strong table with a builtin test, rooted before return.
    /// No arbitrary table payload or raw value can enter this sealed path.
    #[cold]
    #[inline(never)]
    pub fn make_hash_table_into(
        &mut self,
        test: HashTableTest,
        size: usize,
        destination: &OwnerRootSlot<'_>,
    ) -> Result<(), RegistrationError> {
        self.cons_state().check_boundary()?;
        destination.check(self)?;
        let size = i64::try_from(size).map_err(|_| RegistrationError::Capacity)?;
        let table = LispHashTable::new_with_options(test, size, None, 1.5, 0.8125);
        self.install_hash_table(table, destination)
    }

    /// Copy under the source's sealed reader authority, release that authority
    /// before the catalog mutex, then root the independent shell. Source and
    /// destination may be cloned handles to the same home.
    #[cold]
    #[inline(never)]
    pub fn copy_hash_table_into(
        &mut self,
        source: &OwnerRootSlot<'_>,
        destination: &OwnerRootSlot<'_>,
    ) -> Result<(), RegistrationError> {
        self.copy_hash_table_into_with(source, destination, |_| {})
    }

    /// Test-only observation of the actual copying lease, before its snapshot.
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn copy_hash_table_into_observed(
        &mut self,
        source: &OwnerRootSlot<'_>,
        destination: &OwnerRootSlot<'_>,
        observe: impl FnOnce(&TableRef<'_>),
    ) -> Result<(), RegistrationError> {
        self.copy_hash_table_into_with(source, destination, observe)
    }

    fn copy_hash_table_into_with(
        &mut self,
        source: &OwnerRootSlot<'_>,
        destination: &OwnerRootSlot<'_>,
        observe: impl FnOnce(&TableRef<'_>),
    ) -> Result<(), RegistrationError> {
        self.cons_state().check_boundary()?;
        source.check(self)?;
        destination.check(self)?;
        let value = self
            .owner
            .local
            .as_ref()
            .expect("resident owner roots")
            .root_scratch[source.index]
            .value();
        let table = {
            let lease = TableRef::enter(self, value)?;
            observe(&lease);
            lease.snapshot_for_copy()?
        };
        // This run cannot ACK while the ordinary owned clone is temporarily
        // unrooted. No collection, Lisp callback or safepoint occurs here.
        self.install_hash_table(table, destination)
    }

    fn install_hash_table(
        &mut self,
        table: LispHashTable,
        destination: &OwnerRootSlot<'_>,
    ) -> Result<(), RegistrationError> {
        let bytes =
            std::mem::size_of::<HashTableObj>().saturating_add(table.data.known_storage_bytes());
        let hot = &self.owner.local.as_ref().expect("resident owner state").hot;
        hot.allocated_count
            .checked_add(1)
            .ok_or(RegistrationError::Capacity)?;
        hot.bytes_since_gc
            .checked_add(bytes)
            .ok_or(RegistrationError::Capacity)?;
        let object = Box::new(HashTableObj {
            header: VecLikeHeader::new(VecLikeType::HashTable),
            table,
        });
        let value: TaggedValue = self
            .owner
            .registry
            .cons_catalog
            .link_hash_table(self.owner.entry.key, object)?;
        // The sole charge includes the shell and its initial/copied backing.
        // These are the same owner counters the registry publishes and sums;
        // later edits charge only their growth through TableMut::edit.
        let local = self.owner.local.as_mut().expect("resident owner state");
        local.hot.allocated_count += 1;
        local.hot.bytes_since_gc += bytes;
        let slot = MemoryUseCountSlot::VectorCells.index();
        local.hot.memory_use_counts[slot] = local.hot.memory_use_counts[slot].wrapping_add(1);
        local.root_scratch[destination.index] = MarkWord::of(value);
        Ok(())
    }
}
