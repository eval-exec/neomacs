//! Atomic trailer words preserve independent owner bits and grant only one
//! logging claim when several threads race on the same owner.

use super::*;
use std::sync::{Arc, Barrier};

fn trailer() -> Arc<ConsBlockTrailer> {
    Arc::new(ConsBlockTrailer {
        mark: std::array::from_fn(|_| AtomicUsize::new(0)),
        old: std::array::from_fn(|_| AtomicU64::new(0)),
        unlogged: std::array::from_fn(|_| AtomicU64::new(0)),
    })
}

#[test]
fn concurrent_disjoint_owner_updates_preserve_every_bit_of_a_shared_word() {
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
                        trailer.set_unlogged(index);
                        assert!(trailer.is_unlogged(index));
                        assert!(trailer.try_claim_unlogged(index));
                        assert!(!trailer.try_claim_unlogged(index));
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(trailer.old_word(0), u64::MAX);
    assert_eq!(trailer.unlogged_word(0), 0);
    assert_eq!(trailer.old_word(1), 0);
}

#[test]
fn exactly_one_concurrent_claim_wins_without_touching_the_neighbour() {
    crate::test_utils::init_test_tracing();
    let trailer = trailer();
    let phase = Arc::new(Barrier::new(9));
    let wins = Arc::new(AtomicUsize::new(0));
    trailer.set_old(14);
    trailer.set_unlogged(14);
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let trailer = trailer.clone();
            let phase = phase.clone();
            let wins = wins.clone();
            std::thread::spawn(move || {
                for _ in 0..64 {
                    phase.wait();
                    trailer.set_old(13);
                    if trailer.try_claim_unlogged(13) {
                        wins.fetch_add(1, Ordering::Relaxed);
                    }
                    phase.wait();
                }
            })
        })
        .collect();
    for _ in 0..64 {
        wins.store(0, Ordering::Relaxed);
        trailer.set_unlogged(13);
        phase.wait();
        phase.wait();
        assert_eq!(wins.load(Ordering::Relaxed), 1);
        assert_eq!(trailer.old_word(0), (1 << 13) | (1 << 14));
        assert_eq!(trailer.unlogged_word(0), 1 << 14);
    }
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
fn stopped_world_promotion_and_reset_preserve_old_and_marked_populations() {
    crate::test_utils::init_test_tracing();
    let trailer = trailer();
    trailer.set_old(5);
    trailer.set_old(CONS_BLOCK_SIZE - 1);
    assert!(trailer.mark_cell(6));
    assert!(trailer.try_mark(65));
    assert!(!trailer.try_mark(65));
    assert_eq!(trailer.count_marked(CONS_BLOCK_SIZE), 2);
    // There are no live workers or mutators in this test: the same exclusive
    // extent that the stop-all handshake establishes for a collector.
    trailer.promote_marked_world_stopped(CONS_BLOCK_SIZE);
    for index in [5, 6, 65, CONS_BLOCK_SIZE - 1] {
        assert!(trailer.is_old(index));
        assert!(trailer.try_claim_unlogged(index));
        assert!(!trailer.is_unlogged(index));
    }
    assert!(!trailer.is_old(7));
    trailer.reset_unlogged_world_stopped(CONS_BLOCK_SIZE);
    for index in [5, 6, 65, CONS_BLOCK_SIZE - 1] {
        assert!(trailer.is_unlogged(index));
    }
    trailer.clear_marks_world_stopped(CONS_BLOCK_SIZE);
    assert_eq!(trailer.count_marked(CONS_BLOCK_SIZE), 0);
    for index in [5, 6, 65, CONS_BLOCK_SIZE - 1] {
        assert!(trailer.is_old(index));
        assert!(trailer.is_unlogged(index));
    }
}
