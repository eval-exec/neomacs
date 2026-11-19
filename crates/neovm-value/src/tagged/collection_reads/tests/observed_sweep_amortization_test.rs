//! An incremental sweep must not revalidate the whole observed ledger per slice.
//!
//! Every slice that clears an observed mark advances the shared
//! epoch, and each slice republishes the barrier window. Validating every
//! ledger identity on each such slice made one sweep cost O(slices * ledger)
//! and wedged a long-lived session's main thread inside the sweep. Retained
//! dead identities must still never suppress a reused address's new mark.

use super::*;
use crate::tagged::gc::{TaggedHeap, collection_observed_metadata, set_tagged_heap};
use rustc_hash::FxHashSet;

const OWNERS: usize = 64 * 1024;

struct ObservedMode;

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn ledger_counters() -> (usize, usize, usize) {
    with_state(|state| {
        let state = state.borrow();
        (
            state.observed.validated_identities_for_test(),
            state.observed.geometry_rebuilds_for_test(),
            state.observed.owner_count_for_test(),
        )
    })
}

// One observed dead owner per 64, spread across the young sweep list so that
// every slice clears marks but the total stays below the prune threshold.
#[test]
fn incremental_sweep_slices_amortize_observed_ledger_validation() {
    sweep_contract(64, 1);
}

// Half of the owners die: the amortized prune must still run during the sweep
// and keep the retained dead identities to a bounded fraction of the ledger.
#[test]
fn incremental_sweep_prunes_a_mostly_dead_observed_ledger() {
    sweep_contract(2, 4);
}

fn sweep_contract(garbage_stride: usize, max_rebuilds: usize) {
    crate::test_utils::init_test_tracing();
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    let mut heap = Box::new(TaggedHeap::new());
    heap.set_gc_threshold(usize::MAX);
    set_tagged_heap(&mut heap);
    let owners: Vec<TaggedValue> = (0..OWNERS)
        .map(|index| heap.alloc_vector(vec![TaggedValue::make_int(index as i64)]))
        .collect();
    let (_, reads) = capture(|| {
        for owner in &owners {
            owner.as_vector_data().expect("vector")[0];
        }
    });
    drop(reads);
    let live: Vec<TaggedValue> = owners
        .iter()
        .enumerate()
        .filter(|(index, _)| index % garbage_stride != 0)
        .map(|(_, owner)| *owner)
        .collect();
    let garbage: FxHashSet<usize> = owners
        .iter()
        .enumerate()
        .filter(|(index, _)| index % garbage_stride == 0)
        .map(|(_, owner)| owner.bits())
        .collect();
    let (_, live_reads) = capture(|| live[0].as_vector_data().expect("vector")[0]);
    let live_reads = live_reads.expect("rooted live dependency");
    let (validated_before, rebuilds_before, ledger_before) = ledger_counters();
    assert!(ledger_before >= OWNERS);

    // SAFETY: this test exclusively owns the quiescent private heap.
    unsafe { heap.begin_collection() };
    for owner in &live {
        heap.seed_root(*owner);
    }
    heap.incremental_drain_all();
    let bytes_before = heap.live_bytes();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    let started = std::time::Instant::now();
    let mut slices = 0usize;
    while !heap.incremental_sweep_slice(1) {
        slices += 1;
    }
    let elapsed = started.elapsed();
    let (validated_after, rebuilds_after, ledger_after) = ledger_counters();
    let validated = validated_after - validated_before;
    let rebuilds = rebuilds_after - rebuilds_before;
    eprintln!(
        "observed sweep: stride={garbage_stride} ledger={ledger_before} slices={slices} validated={validated} \
         rebuilds={rebuilds} ledger_after={ledger_after} elapsed={elapsed:?}"
    );
    assert!(slices >= 16, "the sweep must span many slices: {slices}");
    for bits in &garbage {
        assert!(
            !collection_observed_metadata(*bits),
            "garbage was reclaimed"
        );
    }
    // Every slice above cleared observed marks. Per-slice validation would
    // scan about slices * ledger identities; amortized pruning is bounded by
    // the ledger plus PRUNE_DEBT_RATIO per cleared mark.
    assert!(
        validated <= ledger_before + 4 * garbage.len(),
        "sweep slices revalidated the whole ledger: {validated} identities over {slices} slices"
    );
    assert!(
        rebuilds <= max_rebuilds,
        "sweep slices rebuilt the geometry {rebuilds} times"
    );
    assert!(
        ledger_after <= ledger_before,
        "the ledger never grows from reclamation"
    );
    // Dead identities retained between prunes stay below the debt ratio.
    let retained_dead = ledger_after - live.len();
    assert!(
        retained_dead * 4 <= ledger_after + 4,
        "{retained_dead} dead identities retained in a ledger of {ledger_after}"
    );
    assert!(
        live_reads.unchanged(),
        "the sweep preserves live dependencies"
    );
    for owner in &live {
        assert!(collection_observed_metadata(owner.bits()));
    }

    // Reuse the reclaimed slots. A retained dead identity at the same bits
    // must neither count as published nor suppress the new owner's mark.
    let mut reused = 0usize;
    for index in 0..2 * garbage.len() {
        let owner = heap.alloc_vector(vec![TaggedValue::make_int(index as i64)]);
        if !garbage.contains(&owner.bits()) {
            continue;
        }
        reused += 1;
        assert!(!collection_observed_metadata(owner.bits()));
        assert!(
            !with_state(|state| state.borrow().observed.already_published(owner.bits())),
            "a dead lifetime's ledger entry is not a publication of its reuse"
        );
        let (_, reads) = capture(|| owner.as_vector_data().expect("vector")[0]);
        let reads = reads.expect("fresh live dependency");
        assert!(
            collection_observed_metadata(owner.bits()),
            "the reused address must be marked again"
        );
        assert!(reads.unchanged());
        assert!(crate::tagged::mutate::set_vector_slot(
            owner,
            0,
            TaggedValue::make_int(-1)
        ));
        assert!(!reads.unchanged(), "the reused owner's writes invalidate");
    }
    assert!(
        reused > 0,
        "the allocator reused at least one reclaimed slot"
    );
    assert!(crate::tagged::mutate::set_vector_slot(
        live[0],
        0,
        TaggedValue::make_int(-2)
    ));
    assert!(
        !live_reads.unchanged(),
        "the live dependency still invalidates"
    );
}
