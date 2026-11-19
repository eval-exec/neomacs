//! Attribute insertion after pruning, independently of GC or Lisp allocation.
//! Numeric ledger identities are never dereferenced here. Simulated pruning
//! exercises the same retain/rebuild operations as the reclamation edge; real
//! mark/reuse and cross-heap reclamation are covered by the existing families.
use super::super::allocation_tests::measured;
use super::*;

const WARMUP: usize = 16;
const MEASURED: usize = 32;

fn identity(index: usize, count: usize, epoch: usize, retain_half: bool) -> usize {
    let cohort = if retain_half && index <= count / 2 {
        0
    } else {
        epoch + 1
    };
    let tag = if index % 2 == 0 {
        TAG_VECLIKE
    } else {
        TAG_CONS
    };
    ((cohort + 1) << 20) | (index << 8) | tag
}

#[test]
fn collection_reads_envelope_turnover_allocation_counters() {
    for count in [24, 512] {
        for retain_half in [false, true] {
            let mut envelope = ObservedEnvelope::new();
            let mut total = (0, 0);
            for epoch in 0..WARMUP + MEASURED {
                // Replace all owners, or retain an unchanged live half while
                // replacing the other half. Pruning deliberately uses retain,
                // rather than clear, so hash-table tombstones are exercised.
                envelope
                    .owners
                    .retain(|&bits| retain_half && (bits >> 20) == 1);
                envelope.rebuild(BarrierWindow::NONE);
                let (_, allocations) = measured(|| {
                    // Descending insertion also checks that the retained
                    // ordered representation does not depend on read order.
                    for index in (1..=count).rev() {
                        let geometry = if index % 2 == 0 {
                            NativeWindowCollectionOwner::Slots
                        } else {
                            NativeWindowCollectionOwner::Cons
                        };
                        envelope.insert(identity(index, count, epoch, retain_half), geometry);
                    }
                });
                assert_eq!(envelope.owners.len(), count);
                let ledger_len =
                    |ledger: &SortedAddressLedger| ledger.bulk.len() + ledger.hot.len();
                assert_eq!(ledger_len(&envelope.addresses), count);
                assert_eq!(ledger_len(&envelope.slot_addresses), count / 2);
                // Both runs stay sorted and disjoint; the hot run may hold the
                // whole population until it reaches the merge limit.
                let (bulk, hot) = (&envelope.addresses.bulk, &envelope.addresses.hot);
                assert!(bulk.windows(2).all(|pair| pair[0] < pair[1]));
                assert!(hot.windows(2).all(|pair| pair[0] < pair[1]));
                let mut observed: Vec<usize> = envelope.addresses.iter().collect();
                observed.sort_unstable();
                assert!(observed.windows(2).all(|pair| pair[0] < pair[1]));
                if epoch >= WARMUP {
                    assert_eq!(
                        allocations,
                        (0, 0),
                        "bounded turnover must reuse all envelope storage"
                    );
                    total.0 += allocations.0;
                    total.1 += allocations.1;
                }
            }
            eprintln!(
                "obsalloc envelope owners={count} retained_half={retain_half} turnovers={MEASURED} steady={total:?} (calls, requested bytes)"
            );
        }
    }
}
