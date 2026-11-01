//! Grace-end backpressure bound — the count-based measurement
//! (P13-PREP deliverable 2; becomes N5 design rev 2's section 2 addendum).
//!
//! The REAL ledger runs natively here: the same byte-for-byte include as
//! the Loom model, but against real `std` (no shim), single-threaded, with
//! the symbol authority's writer rule replayed exactly as the engine
//! applies it — desired chunks = max(current, ceil(minimum_slots /
//! OBARRAY_CHUNK)) with one publication attempt per intern, where
//! `minimum_slots` is the new symbol's id + 1 (`intern.rs`'s writer path;
//! `OBARRAY_CHUNK = 4096`, `symbol/mod.rs`). What is counted per stop
//! interval (an "era": the span between two world stops) is therefore the
//! real `TableStatistics` telemetry of the real ledger — retirements,
//! refusals by kind, retained bytes — driven by the P12 row workloads.
//!
//! Honest scope statement (recorded in the design addendum with these
//! numbers): the engine's `TableStatistics` accessor is `#[cfg(test)]` and
//! exposes no Elisp subr, so the P12 binary itself cannot be polled for
//! these counts without changing the main tree, which P13-PREP forbids.
//! The counts below are deterministic functions of the writer rule and the
//! row workloads (they are replayed, not sampled); the P12 instruction
//! baselines are cited for workload identity, not as the source of the
//! counts. When N5 lands, the drain/reclaim it adds makes the per-era
//! reset observable in-engine and this replay becomes an in-engine churn
//! test ("refusal is not steady state").
//!
//! Rows (P12 measurement set, threads-bench):
//! * `intern_fresh_10k` — 10_000 fresh interns from the 5-chunk bootstrap
//!   base (17_085 interned ids): exactly 2 retirements, 0 refusals.
//! * era continuation — a further 10_000 fresh interns: exactly 3 more
//!   retirements (retired accumulates across eras until a stop drains it),
//!   still 0 refusals at production limits.
//! * `many_threads_{100,300,600,1000}` — no fresh interning (the P12
//!   marginal was thread setup, 2.2-2.65M instructions per thread): 0
//!   retirements, 0 refusals, every attempt `Unchanged`.
//! * forced limits — the retired-count limit set to 2: growth stops AT the
//!   limit with `NeedGrace(RetiredTables)`, interning continues through
//!   the current table (`Unchanged`), and nothing is charged for the
//!   count refusal.
#[path = "support/vocab.rs"]
mod vocab;

pub use vocab::{
    OwnedSymbolTable, PointerTableGeneration, PreparedSymbolTable, PublishedSymbolTable,
    SymbolChunkAddress, SymbolTableShape, TableError,
};
use vocab::{SymbolChunkCount, SymbolChunkSlots, SymbolSlotCount};

// The real ledger's whole API compiles here; the parts this model does
// not call are dead code by design (read-only consumption).
#[allow(dead_code)]
mod ledger_native {
    //! The real `publication.rs` against real `std`: native, not modeled.
    include!(concat!(env!("OUT_DIR"), "/publication.inc"));
}
use ledger_native::{
    PublicationChange, PublishedSymbolTables, RetainedTableBytes, RetiredTableCount, TableLimit,
    TableLimits, TablePublicationError,
};

/// `OBARRAY_CHUNK` (symbol/mod.rs): slots per descriptor chunk.
const OBARRAY_CHUNK: usize = 4096;
/// The bootstrap base of the P12 measurement binary: 17_085 interned ids,
/// 5 chunks. (5 = ceil(17_085 / 4096).)
const BOOTSTRAP_IDS: u64 = 17_085;

#[derive(Default)]
struct Counters {
    attempts: usize,
    published: usize,
    unchanged: usize,
    refused_tables: usize,
    refused_bytes: usize,
    other: usize,
}
impl Counters {
    fn retirements(&self) -> usize {
        self.published
    }
}

fn shape(chunks: usize) -> SymbolTableShape {
    SymbolTableShape::new(
        SymbolSlotCount::from(chunks * OBARRAY_CHUNK),
        SymbolChunkCount::from(chunks),
        SymbolChunkSlots::from(
            std::num::NonZeroUsize::new(OBARRAY_CHUNK).expect("constant is nonzero"),
        ),
    )
    .expect("exact coverage")
}

/// Append-only prefix-stable pool: retired and current tables always agree
/// on the addresses of their shared prefix.
fn ensure_pool(pool: &mut Vec<SymbolChunkAddress>, chunks: usize) {
    while pool.len() < chunks {
        let root = Box::leak(Box::new([0u8; 16]));
        // SAFETY: the leaked box is never freed and the address is never
        // dereferenced by the ledger (it stores and compares it only).
        pool.push(unsafe {
            SymbolChunkAddress::from_allocation(std::ptr::NonNull::from(root).cast())
        });
    }
}

/// One publication attempt, exactly as the symbol authority makes one per
/// intern: derive the desired shape from the CURRENT table and the new
/// symbol's minimum coverage, then prepare and publish.
fn attempt(
    tables: &PublishedSymbolTables,
    pool: &mut Vec<SymbolChunkAddress>,
    minimum: usize,
    counters: &mut Counters,
) {
    let current = tables.view().table().chunk_count().get();
    let chunks = current.max(minimum.div_ceil(OBARRAY_CHUNK));
    ensure_pool(pool, chunks);
    counters.attempts += 1;
    let prepared = tables.try_prepare(shape(chunks));
    match prepared.result {
        Ok(publication) => match publication.publish(pool[..chunks].iter().copied()) {
            Ok(PublicationChange::Published { .. }) => counters.published += 1,
            Ok(PublicationChange::Unchanged { .. }) => counters.unchanged += 1,
            Err(_) => counters.other += 1,
        },
        Err(TablePublicationError::NeedGrace(TableLimit::RetiredTables)) => {
            counters.refused_tables += 1
        }
        Err(TablePublicationError::NeedGrace(TableLimit::RetainedBytes)) => {
            counters.refused_bytes += 1
        }
        Err(_) => counters.other += 1,
    }
}

/// An era of fresh interning: `count` new symbols, ids ascending from
/// `next_id`.
fn intern_fresh(
    tables: &PublishedSymbolTables,
    pool: &mut Vec<SymbolChunkAddress>,
    next_id: &mut u64,
    count: u64,
    counters: &mut Counters,
) {
    for _ in 0..count {
        let minimum = usize::try_from(*next_id + 1).expect("id space fits the host");
        *next_id += 1;
        attempt(tables, pool, minimum, counters);
    }
}

/// An era with no fresh interning (the many_threads rows): threads
/// republish the current shape and nothing retires.
fn republish_only(
    tables: &PublishedSymbolTables,
    pool: &mut Vec<SymbolChunkAddress>,
    attempts: usize,
    counters: &mut Counters,
) {
    let coverage = tables.view().table().chunk_count().get() * OBARRAY_CHUNK;
    for _ in 0..attempts {
        attempt(tables, pool, coverage, counters);
    }
}

fn bootstrap(limits: TableLimits) -> (PublishedSymbolTables, Vec<SymbolChunkAddress>, u64) {
    let mut pool = Vec::new();
    let initial_chunks = usize::try_from(BOOTSTRAP_IDS)
        .expect("id space fits the host")
        .div_ceil(OBARRAY_CHUNK);
    ensure_pool(&mut pool, initial_chunks);
    let initial = PreparedSymbolTable::new(shape(initial_chunks), PointerTableGeneration::INITIAL)
        .expect("descriptor allocation")
        .initialize(pool[..initial_chunks].iter().copied())
        .expect("exact address count");
    let tables = PublishedSymbolTables::new(initial, limits).expect("bootstrap within limits");
    (tables, pool, BOOTSTRAP_IDS)
}

fn row(counters: &Counters, name: &str) {
    println!(
        "P13-PREP row {name}: attempts={} retirements={} unchanged={} need_grace_tables={} \
         need_grace_bytes={} other={}",
        counters.attempts,
        counters.retirements(),
        counters.unchanged,
        counters.refused_tables,
        counters.refused_bytes,
        counters.other,
    );
}

fn assert_bound(next_id_start: u64, fresh: u64, retirements: usize) {
    let start = usize::try_from(next_id_start).expect("id space fits the host");
    let end = start + usize::try_from(fresh).expect("id space fits the host");
    assert_eq!(
        retirements,
        end.div_ceil(OBARRAY_CHUNK) - start.div_ceil(OBARRAY_CHUNK),
        "retirements per stop interval are cadence-independent: exactly the chunk delta"
    );
}

#[test]
fn p12_rows_retirements_and_refusals_per_stop_interval() {
    let (tables, mut pool, mut next_id) = bootstrap(TableLimits::default());

    // intern_fresh_10k (P12 baseline arm a814fdb50f, net median
    // 238,771,872 instructions/rep, 220 runs, 0 rc-fail): 2 retirements.
    let mut row_counters = Counters::default();
    intern_fresh(&tables, &mut pool, &mut next_id, 10_000, &mut row_counters);
    assert_bound(BOOTSTRAP_IDS, 10_000, row_counters.retirements());
    assert_eq!(row_counters.retirements(), 2);
    assert_eq!(row_counters.refused_tables + row_counters.refused_bytes, 0);
    assert_eq!(row_counters.other, 0);
    assert_eq!(row_counters.unchanged, row_counters.attempts - 2);
    let statistics = tables.stats().expect("uncontended");
    assert_eq!(statistics.retired.get(), 2);
    assert_eq!(statistics.generation.get(), 3);
    row(&row_counters, "intern_fresh_10k");

    // Era continuation (no drain between stops): a further 10_000 fresh
    // interns retire exactly the further chunk delta (3) and the retired
    // set ACCUMULATES (5 total) — bounded per interval, unbounded across
    // intervals until a stop's grace end drains it.
    let mut era2 = Counters::default();
    let era2_start = next_id;
    intern_fresh(&tables, &mut pool, &mut next_id, 10_000, &mut era2);
    assert_bound(era2_start, 10_000, era2.retirements());
    assert_eq!(era2.retirements(), 3);
    assert_eq!(era2.refused_tables + era2.refused_bytes, 0);
    assert_eq!(era2.other, 0);
    let statistics = tables.stats().expect("uncontended");
    assert_eq!(statistics.retired.get(), 5);
    row(&era2, "intern_fresh_10k_era2_accumulating");

    // many_threads_{100,300,600,1000}: thread setup only, no fresh interning.
    for threads in [100, 300, 600, 1000] {
        let mut threads_row = Counters::default();
        republish_only(&tables, &mut pool, 8 * threads, &mut threads_row);
        assert_eq!(threads_row.retirements(), 0);
        assert_eq!(threads_row.refused_tables + threads_row.refused_bytes, 0);
        assert_eq!(threads_row.other, 0);
        assert_eq!(threads_row.unchanged, threads_row.attempts);
        row(&threads_row, &format!("many_threads_{threads}"));
    }
    let statistics = tables.stats().expect("uncontended");
    assert_eq!(statistics.retired.get(), 5);
}

/// The forced-refusal scenario behind "refusal is not steady state": with
/// the retired-count limit reached, growth is refused without a charge and
/// interning continues through the current table. The refusal ends exactly
/// when a world stop's grace end drains the retired set — the N5 reclaim
/// this measurement cannot call (the read-only ledger has no drain), which
/// is the addendum's point: the writer's backpressure is a stop request,
/// never data loss.
#[test]
fn forced_limits_refusal_is_not_steady_state() {
    let (tables, mut pool, mut next_id) = bootstrap(TableLimits::new(
        RetainedTableBytes::new(64 * 1024 * 1024),
        RetiredTableCount::new(2),
    ));

    let mut era1 = Counters::default();
    intern_fresh(&tables, &mut pool, &mut next_id, 10_000, &mut era1);
    assert_eq!(era1.retirements(), 2);
    assert_eq!(era1.refused_tables + era1.refused_bytes, 0);
    row(&era1, "forced_limits_era1");

    // The next growth is refused: retired.len() has reached the limit, the
    // refusal is uncharged, and every in-capacity intern still succeeds.
    let mut era2 = Counters::default();
    let before = tables.stats().expect("uncontended");
    intern_fresh(&tables, &mut pool, &mut next_id, 10_000, &mut era2);
    let after = tables.stats().expect("uncontended");
    assert_eq!(era2.retirements(), 0);
    assert!(era2.refused_tables > 0);
    assert_eq!(era2.refused_bytes, 0);
    assert_eq!(era2.other, 0);
    assert!(era2.unchanged > 0);
    // Refusal changed nothing: same generation, same retired set, same cap.
    assert_eq!(before.generation, after.generation);
    assert_eq!(before.retired, after.retired);
    row(&era2, "forced_limits_era2_refused");
}
