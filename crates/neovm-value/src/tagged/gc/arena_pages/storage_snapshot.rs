//! Owned metadata for stopped numeric-storage transfer tests.
//! No page, bitmap, payload, or arena reference leaves this snapshot.

use super::{ObjectArena, PagedObject};

#[derive(Debug, PartialEq, Eq)]
pub(in crate::tagged::gc) struct PageStorageSnapshot {
    pub(in crate::tagged::gc) base: usize,
    pub(in crate::tagged::gc) next_index: usize,
    pub(in crate::tagged::gc) alloc_bits: Vec<usize>,
    pub(in crate::tagged::gc) allocated: usize,
    pub(in crate::tagged::gc) free_head: usize,
    pub(in crate::tagged::gc) next_partial: usize,
    pub(in crate::tagged::gc) on_partial: bool,
    pub(in crate::tagged::gc) retired: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::tagged::gc) struct ArenaStorageSnapshot {
    pub(in crate::tagged::gc) pages: Vec<PageStorageSnapshot>,
    pub(in crate::tagged::gc) pages_capacity: usize,
    pub(in crate::tagged::gc) page_index: Vec<(usize, usize)>,
    pub(in crate::tagged::gc) page_index_capacity: usize,
    pub(in crate::tagged::gc) spare_storage: Vec<usize>,
    pub(in crate::tagged::gc) spare_storage_capacity: usize,
    pub(in crate::tagged::gc) spare_unused_floor: usize,
    pub(in crate::tagged::gc) partial_head: usize,
    pub(in crate::tagged::gc) chunk_map_identity: Option<usize>,
}

impl<T: PagedObject> ObjectArena<T> {
    pub(in crate::tagged::gc) fn storage_snapshot_for_test(&self) -> ArenaStorageSnapshot {
        let mut page_index = self
            .page_index_by_base
            .iter()
            .map(|(&base, &index)| (base, index))
            .collect::<Vec<_>>();
        page_index.sort_unstable();
        ArenaStorageSnapshot {
            pages: self
                .pages
                .iter()
                .map(|page| PageStorageSnapshot {
                    base: page.base_addr(),
                    next_index: page.next_index,
                    alloc_bits: page.alloc_bits.to_vec(),
                    allocated: page.allocated,
                    free_head: page.free_head,
                    next_partial: page.next_partial,
                    on_partial: page.on_partial,
                    retired: page.retired,
                })
                .collect(),
            pages_capacity: self.pages.capacity(),
            page_index,
            page_index_capacity: self.page_index_by_base.capacity(),
            spare_storage: self
                .spare_storage
                .iter()
                .map(|&storage| storage as usize)
                .collect(),
            spare_storage_capacity: self.spare_storage.capacity(),
            spare_unused_floor: self.spare_unused_floor,
            partial_head: self.partial_head,
            chunk_map_identity: self
                .chunk_map
                .as_ref()
                .map(|map| std::sync::Arc::as_ptr(map) as usize),
        }
    }
}
