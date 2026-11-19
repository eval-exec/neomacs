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

// Test-only views stay in this child module, so the production ledger
// carries no test-only items.
fn runs(ledger: &SortedAddressLedger) -> (&[usize], &[usize]) {
    (&ledger.bulk, &ledger.hot)
}

fn len(ledger: &SortedAddressLedger) -> usize {
    ledger.bulk.len() + ledger.hot.len()
}

#[test]
fn inserting_below_the_retained_bulk_is_bounded() {
    const BULK: usize = 200_000;
    const INSERTED: usize = 20_000;

    let mut ledger = SortedAddressLedger::new();
    for index in 0..BULK {
        assert!(ledger.insert(BASE + index * STRIDE));
    }
    let (bulk, hot) = runs(&ledger);
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
        let (inserted, elements) = ledger.insert_inner::<true>(below(index));
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

    assert_eq!(len(&ledger), BULK + INSERTED);
    for index in 0..BULK {
        assert!(ledger.contains(BASE + index * STRIDE));
    }
    for index in 0..INSERTED {
        assert!(ledger.contains(below(index)));
    }
    let (bulk, hot) = runs(&ledger);
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

/// Deterministic xorshift64 so the model test needs no extra dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn model_neighbors(
    model: &std::collections::BTreeSet<usize>,
    address: usize,
) -> (Option<usize>, Option<usize>) {
    (
        model.range(..address).next_back().copied(),
        model.range(address..).next().copied(),
    )
}

/// The two-run ledger must answer every query exactly as one ordered set
/// would, with hot and bulk addresses interleaved, across many merges and
/// across reclamation that filters both runs. The seed spreads addresses so
/// a hot-run address usually sits between two bulk addresses, and the
/// queried predecessor and successor then come from different runs.
#[test]
fn two_run_ledger_matches_an_ordered_set_model() {
    const SPAN: usize = 1 << 16;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut ledger = SortedAddressLedger::new();
    let mut model = std::collections::BTreeSet::new();
    let mut seam_queries = 0usize;
    for round in 0..40 {
        for _ in 0..(HOT_ADDRESS_LIMIT + HOT_ADDRESS_LIMIT / 3) {
            let address = rng.below(SPAN) * STRIDE;
            assert_eq!(ledger.insert(address), model.insert(address));
        }
        assert_eq!(len(&ledger), model.len());
        let (bulk, hot) = runs(&ledger);
        assert!(bulk.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(hot.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(hot.len() < HOT_ADDRESS_LIMIT, "a full hot run must merge");
        assert!(
            hot.iter()
                .all(|address| bulk.binary_search(address).is_err())
        );
        let mut all: Vec<usize> = ledger.iter().collect();
        all.sort_unstable();
        assert!(all.iter().eq(model.iter()));

        for _ in 0..512 {
            // Probe both observed addresses and the gaps beside them.
            let probe = rng.below(SPAN) * STRIDE + rng.below(2) * (STRIDE / 2);
            assert_eq!(ledger.contains(probe), model.contains(&probe));
            if model.contains(&probe) {
                continue;
            }
            let expected = model_neighbors(&model, probe);
            assert_eq!(
                ledger.neighbors(probe),
                expected,
                "round {round} probe {probe:#x}"
            );
            if let (Some(previous), Some(next)) = expected {
                let in_hot = |address: usize| hot.binary_search(&address).is_ok();
                if in_hot(previous) != in_hot(next) {
                    seam_queries += 1;
                }
            }
        }

        // Reclamation filters both runs in place; the survivors must still
        // answer every query exactly.
        let modulus = 2 + round % 5;
        ledger.retain(|address| !(address / STRIDE).is_multiple_of(modulus));
        model.retain(|address| !(address / STRIDE).is_multiple_of(modulus));
        assert_eq!(len(&ledger), model.len());
    }
    assert!(
        seam_queries > 0,
        "the model test must exercise predecessor/successor pairs split across the two runs"
    );
}
