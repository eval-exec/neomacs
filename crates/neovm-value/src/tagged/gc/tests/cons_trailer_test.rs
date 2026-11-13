//! The old bitmap and its per-cell age-byte mirror: independent owner
//! updates, and promotions that keep the two equal.

use super::*;
use std::sync::{Arc, Barrier};

fn trailer() -> Arc<ConsBlockTrailer> {
    Arc::new(ConsBlockTrailer {
        mark: std::array::from_fn(|_| AtomicUsize::new(0)),
        old: std::array::from_fn(|_| AtomicU64::new(0)),
        age: std::array::from_fn(|_| AtomicU8::new(ConsAge::Young as u8)),
    })
}

#[test]
fn concurrent_disjoint_owner_updates_preserve_every_age_of_a_shared_word() {
    crate::test_utils::init_test_tracing();
    let trailer = trailer();
    let start = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|worker| {
            let trailer = trailer.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                for _ in 0..128 {
                    for index in worker * 8..(worker + 1) * 8 {
                        trailer.set_old(index);
                        assert!(trailer.is_old(index));
                        trailer.clear_old(index);
                        assert!(!trailer.is_old(index));
                        trailer.set_old(index);
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(trailer.old_word(0), u64::MAX);
    assert_eq!(trailer.old_word(1), 0);
    assert!(trailer.ages_match_old_bits());
}

#[test]
fn age_bytes_mirror_every_old_bit() {
    crate::test_utils::init_test_tracing();
    let trailer = trailer();
    // A deterministic scatter that sets every lane and bit position of some
    // word, including the first and last cell of the block.
    let old = |index: usize| index.wrapping_mul(0x9e37_79b9) >> 7 & 1 != 0 || index % 61 == 0;
    for index in (0..CONS_BLOCK_SIZE).filter(|&index| old(index)) {
        trailer.set_old(index);
    }
    trailer.set_old(CONS_BLOCK_SIZE - 1);
    assert!(trailer.ages_match_old_bits());
    for word in 0..CONS_MARK_WORDS {
        let gathered = trailer.old_word(word);
        for bit in 0..64 {
            let index = word * 64 + bit;
            let expected = index < CONS_BLOCK_SIZE && trailer.is_old(index);
            assert_eq!(gathered >> bit & 1 != 0, expected, "cell {index}");
        }
    }
    assert_eq!(
        trailer.count_old(CONS_BLOCK_SIZE),
        (0..CONS_BLOCK_SIZE)
            .filter(|&index| trailer.is_old(index))
            .count()
    );
}

#[test]
fn stopped_world_promotion_preserves_old_and_marked_populations() {
    crate::test_utils::init_test_tracing();
    let trailer = trailer();
    trailer.set_old(5);
    trailer.set_old(CONS_BLOCK_SIZE - 1);
    assert!(trailer.mark_cell(6));
    assert!(trailer.mark_cell(5));
    assert!(trailer.try_mark(65));
    assert!(!trailer.try_mark(65));
    assert_eq!(trailer.count_marked(CONS_BLOCK_SIZE), 3);
    // There are no live workers or mutators in this test: the same exclusive
    // extent that the stop-all handshake establishes for a collector.
    assert_eq!(trailer.promote_marked_world_stopped(CONS_BLOCK_SIZE), 2);
    for index in [5, 6, 65, CONS_BLOCK_SIZE - 1] {
        assert!(trailer.is_old(index));
    }
    assert!(!trailer.is_old(7));
    assert!(trailer.ages_match_old_bits());
    assert_eq!(trailer.promote_marked_world_stopped(CONS_BLOCK_SIZE), 0);
    trailer.clear_marks_world_stopped(CONS_BLOCK_SIZE);
    assert_eq!(trailer.count_marked(CONS_BLOCK_SIZE), 0);
    for index in [5, 6, 65, CONS_BLOCK_SIZE - 1] {
        assert!(trailer.is_old(index));
    }
}

#[test]
fn major_promotion_replaces_age_with_liveness() {
    crate::test_utils::init_test_tracing();
    let trailer = trailer();
    trailer.set_old(5);
    trailer.set_old(70);
    assert!(trailer.mark_cell(70));
    assert!(trailer.mark_cell(71));
    assert_eq!(trailer.promote_major_world_stopped(), 1);
    assert!(
        !trailer.is_old(5),
        "an unmarked old cell is dead after a major"
    );
    assert!(trailer.is_old(70));
    assert!(trailer.is_old(71));
    assert_eq!(trailer.count_old(CONS_BLOCK_SIZE), 2);
    assert!(trailer.ages_match_old_bits());
}

#[test]
fn promotions_spread_mark_words_into_age_bytes() {
    crate::test_utils::init_test_tracing();
    let trailer = trailer();
    let marked = |index: usize| index.wrapping_mul(0x85eb_ca6b) >> 9 & 1 != 0;
    for index in (0..CONS_BLOCK_SIZE).filter(|&index| marked(index)) {
        assert!(trailer.mark_cell(index));
    }
    let expected = (0..CONS_BLOCK_SIZE).filter(|&index| marked(index)).count();
    assert_eq!(
        trailer.promote_marked_world_stopped(CONS_BLOCK_SIZE),
        expected
    );
    assert!(trailer.ages_match_old_bits());
    for index in 0..CONS_BLOCK_SIZE {
        assert_eq!(trailer.is_old(index), marked(index), "cell {index}");
    }
    // A major keeps only the cells marked again: every third survivor.
    trailer.clear_marks_world_stopped(CONS_BLOCK_SIZE);
    let survives = |index: usize| marked(index) && index % 3 == 0;
    for index in (0..CONS_BLOCK_SIZE).filter(|&index| survives(index)) {
        assert!(trailer.mark_cell(index));
    }
    assert_eq!(trailer.promote_major_world_stopped(), 0);
    assert!(trailer.ages_match_old_bits());
    for index in 0..CONS_BLOCK_SIZE {
        assert_eq!(trailer.is_old(index), survives(index), "cell {index}");
    }
}
