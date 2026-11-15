//! Issue #767: live publication must not shift the retained sorted bulk.
//!
//! The ledger serves exact predecessor/successor queries for the native-store
//! gate. A single sorted `Vec` made an insertion below the retained population
//! move the whole tail (~8.5 MB at 1.15M entries). The bounded hot run keeps
//! the same exact queries while bounding the per-insert move.

use super::{HOT_ADDRESS_LIMIT, SortedAddressLedger};

const BASE: usize = 0x10_0000;
const STRIDE: usize = 16;

fn below(index: usize) -> usize {
    BASE - (index + 1) * STRIDE
}

#[test]
fn inserting_below_the_retained_bulk_is_bounded() {
    const BULK: usize = 200_000;
    const INSERTED: usize = 20_000;

    let mut ledger = SortedAddressLedger::new();
    for index in 0..BULK {
        assert!(ledger.insert(BASE + index * STRIDE));
    }
    let (bulk, hot) = ledger.runs();
    let retained = bulk.len() + hot.len();
    assert_eq!(retained, BULK);
    assert!(
        bulk.len() + HOT_ADDRESS_LIMIT >= BULK,
        "a run that never merges would hide the retained bulk"
    );

    // The old single-Vec insert moved (retained - index) slots on every one of
    // these insertions, i.e. INSERTED * retained with an index near the front.
    let old_cost = INSERTED * retained;
    let mut moved = 0;
    for index in 0..INSERTED {
        let (inserted, elements) = ledger.insert_moved(below(index));
        assert!(inserted);
        moved += elements;
    }
    assert!(
        moved < old_cost / 8,
        "issue #767: {INSERTED} insertions below {retained} retained addresses \
         moved {moved} slots; the old single-Vec insertion moved {old_cost}"
    );
    eprintln!(
        "issue #767 ledger: retained={retained} inserted={INSERTED} moved={moved} \
         old_tail_shift={old_cost} reduction={:.1}x",
        old_cost as f64 / moved as f64
    );

    assert_eq!(ledger.len(), BULK + INSERTED);
    for index in 0..BULK {
        assert!(ledger.contains(BASE + index * STRIDE));
    }
    for index in 0..INSERTED {
        assert!(ledger.contains(below(index)));
    }
    let (bulk, hot) = ledger.runs();
    assert!(bulk.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(hot.windows(2).all(|pair| pair[0] < pair[1]));

    // Exact predecessor/successor queries must cross the hot/bulk seam. +1
    // points at the gap immediately above each observed address.
    assert_eq!(
        ledger.neighbors(BASE + 1),
        (Some(BASE), Some(BASE + STRIDE))
    );
    assert_eq!(ledger.neighbors(below(0) + 1), (Some(below(0)), Some(BASE)));
    assert_eq!(
        ledger.neighbors(below(INSERTED - 1) + 1),
        (Some(below(INSERTED - 1)), Some(below(INSERTED - 2)))
    );
}
