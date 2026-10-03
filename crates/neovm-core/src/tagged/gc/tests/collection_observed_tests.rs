//! Sticky observation lifetime and address ownership, independent of GC mode.

use super::*;
use crate::tagged::gc::ConsBlock;
use crate::tagged::header::ConsCdrOrNext;
use crate::tagged::value::TaggedValue;

fn make_block(count: usize) -> ConsBlock {
    let mut block = ConsBlock::new();
    assert_eq!(block.reserve_tail(count), (0, count));
    for index in 0..count {
        // No Lisp allocation or collector callback can interrupt this fixture.
        unsafe {
            block.cells_ptr().add(index).write(ConsCell {
                car: TaggedValue::NIL,
                cdr_or_next: ConsCdrOrNext {
                    cdr: TaggedValue::NIL,
                },
            });
        }
    }
    block
}

fn cell_bits(block: &ConsBlock, index: usize) -> usize {
    // SAFETY: callers select a initialized, live cell of the retained block.
    unsafe { TaggedValue::from_cons_ptr(block.cells_ptr().add(index)) }.bits()
}

#[repr(C, align(16))]
struct EightAlignedConses {
    prefix: u64,
    cells: [ConsCell; 2],
}

impl EightAlignedConses {
    fn new() -> Box<Self> {
        Box::new(Self {
            prefix: 0,
            cells: [ConsCell {
                car: TaggedValue::NIL,
                cdr_or_next: ConsCdrOrNext {
                    cdr: TaggedValue::NIL,
                },
            }; 2],
        })
    }

    fn bits(&self, index: usize) -> usize {
        // SAFETY: the fixture retains its two ordinary, eight-aligned cells.
        unsafe { TaggedValue::from_cons_ptr(&self.cells[index]) }.bits()
    }
}

impl Drop for EightAlignedConses {
    fn drop(&mut self) {
        // Foreign allocations have no ConsBlock destructor. This fixture owns
        // the cell lifetimes and clears only its exact metadata addresses;
        // other tests may own cons cells in the same allocator granule.
        for index in 0..self.cells.len() {
            let (base, word, mask) = cons_address_bit(self.bits(index) & !TAG_MASK);
            let registry = CONS_OBSERVED
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if let Some(bitmap) = registry.get(&base) {
                bitmap.words[word].fetch_and(!mask, Ordering::Release);
            }
        }
    }
}

#[test]
fn observed_cons_marks_distinguish_eight_aligned_foreign_cells() {
    crate::test_utils::init_test_tracing();
    let cells = EightAlignedConses::new();
    let first = cells.bits(0);
    let second = cells.bits(1);
    assert_eq!((first & !TAG_MASK) % 16, 8);
    assert!(!collection_observed(first));
    assert!(!collection_observed(second));
    assert!(mark_collection_observed(first));
    assert!(collection_observed(first));
    assert!(!collection_observed(second));
    assert!(!mark_collection_observed(first));
    assert!(mark_collection_observed(second));
    assert!(collection_observed(second));
}

#[test]
fn gen0_sweep_preserves_live_cons_observations_and_clears_dead_addresses() {
    crate::test_utils::init_test_tracing();
    let mut block = make_block(97);
    let observed = [0, 1, 31, 32, 63, 64, 95, 96].map(|index| (index, cell_bits(&block, index)));
    let live = [1, 32, 63, 95];
    for (_, bits) in observed {
        assert!(mark_collection_observed(bits));
    }
    for index in live {
        block.mark_cell_offset(index * std::mem::size_of::<ConsCell>());
    }
    let mut free_list = std::ptr::null_mut();
    assert_eq!(block.sweep(&mut free_list), live.len());
    for (index, bits) in observed {
        // Cons queries use the old address as a metadata key, never as a
        // dereferenced cell. This also checks the dead/free lifetime seam.
        assert_eq!(collection_observed(bits), live.contains(&index));
    }
    assert!(!free_list.is_null());
}

#[test]
fn gen1_sweep_preserves_untraced_old_and_marked_young_cons_observations() {
    crate::test_utils::init_test_tracing();
    let mut block = make_block(65);
    let observed = [0, 1, 31, 32, 63, 64].map(|index| (index, cell_bits(&block, index)));
    for (_, bits) in observed {
        assert!(mark_collection_observed(bits));
    }
    block.trailer().set_old(0);
    block.trailer().set_unlogged(0);
    block.trailer().set_old(63);
    block.trailer().set_unlogged(63);
    block.mark_cell_offset(std::mem::size_of::<ConsCell>());
    block.mark_cell_offset(64 * std::mem::size_of::<ConsCell>());
    let live = [0, 1, 63, 64];
    let mut free_list = std::ptr::null_mut();
    assert_eq!(block.sweep_generational(&mut free_list), live.len());
    for (index, bits) in observed {
        assert_eq!(collection_observed(bits), live.contains(&index));
    }
    assert!(block.trailer().is_old(0));
    assert!(block.trailer().is_unlogged(0));
    assert!(block.trailer().is_old(63));
    assert!(block.trailer().is_unlogged(63));
}

#[test]
fn cons_observation_is_cleared_before_block_storage_is_released() {
    crate::test_utils::init_test_tracing();
    let block = make_block(1);
    let bits = cell_bits(&block, 0);
    assert!(mark_collection_observed(bits));
    assert!(collection_observed(bits));
    drop(block);
    // A stale Cons identity can safely query the side bitmap after storage
    // dies; the query never accesses the old allocation or its trailer.
    assert!(!collection_observed(bits));
}

#[test]
fn shared_cons_observation_publication_has_exactly_one_first_observer() {
    crate::test_utils::init_test_tracing();
    let block = make_block(1);
    let bits = cell_bits(&block, 0);
    let first_observers = std::thread::scope(|scope| {
        let observers: Vec<_> = (0..8)
            .map(|_| scope.spawn(move || mark_collection_observed(bits)))
            .collect();
        observers
            .into_iter()
            .map(|observer| observer.join().expect("observation thread"))
            .filter(|&first| first)
            .count()
    });
    assert_eq!(first_observers, 1);
    assert!(collection_observed(bits));
}
