//! N5 grace model: the REAL N3 ledger composed with activation release
//! points (P13-PREP deliverable 1; N5 design rev 2 section 2).
//!
//! The N3 adapter is included byte-for-byte through the build-time seam
//! (`build.rs` -> `OUT_DIR/publication.inc`). Its `super::sync` import
//! resolves to the crate-root Loom backend below. The generic owner is
//! included beside it (`publication_types.inc`, `publication_ledger.inc`);
//! that owner's `super::sync` is the nested Loom backend, so the root store,
//! the metadata mutex, and the adapter's load are Loom cells.
//! Other `std::` paths stay the real standard library. Nothing about the
//! ledger is reimplemented here. The descriptor vocabulary comes
//! from the named stand-in (`support::vocab.rs`), which mirrors
//! `src/tagged/gc/symbol_table.rs` field-for-field; the ledger never
//! dereferences chunk entries, so modeled reads observe only publication
//! ordering, retention, and refusal.
//!
//! Composition (N5 rev 2 section 2's during-grace rules made executable):
//!
//! * WRITER  — the symbol-authority publication path: refuses a smaller
//!   shape (retained N3 witness), re-publishes the same shape (the
//!   `Unchanged` path every intern attempt takes), then performs ONE growth
//!   publication (5 -> 6 chunks), which retires the initial descriptor.
//! * FRAME   — a mutator with two bounded activations. Each activation
//!   enters through the entry gate (N5 v1: entry validation; a stale
//!   binding takes the StaleBinding path and re-captures through the
//!   current view), reads the bound descriptor while the JIT view is live,
//!   and exits at the release point where publication and the world stop
//!   may interleave before any re-entry.
//! * STOP    — the world stop: freeze (no activation enters the published
//!   path unvalidated), the JIT-quiescence extension of the stop proof
//!   (every live view returns before the receipt is minted), receipt
//!   minted from the publication current at grace end, then the retired
//!   set is reclaimed.
//!
//! Negative controls — each flips exactly ONE edge of the stop proof and
//! must trip exactly its named witness, with all detectors armed:
//!
//! | edge flipped                    | witness                            |
//! |---------------------------------|------------------------------------|
//! | `entry_validation = false`      | RESUME_USES_PRE_GRACE_POINTER      |
//! | `quiescence_wait = false`       | ACK_WITH_LIVE_JIT_VIEW             |
//! | `receipt_gates_reclaim = false` | OLD_TABLE_RECLAIMED_BEFORE_GRACE   |
//!
//! Detector arming (why each weakened run trips only its own witness): the
//! read detector (frame: the bound descriptor was freed) is always armed
//! and names the reclaim edge when the entry gate held, the resume edge
//! when it did not; the receipt-time detector (stop: a live JIT view at
//! receipt) is armed whenever the entry gate holds, because the gate plus
//! the wait are exactly the two edges that establish it — the gate-less
//! config breaks the protocol upstream and its failure belongs to the
//! resume witness. The premature-free config never reaches the receipt, so
//! the two cannot cross-fire. `TryLockError::Poisoned` remains compiled
//! against the real `std` error type but is model-dead: Loom's mutex never
//! poisons.
//!
//! Exploration bounds: `support::check` runs every execution with at most
//! `N5_PREEMPTIONS` (default 2) preemptions, `max_branches = 4096`,
//! unbounded permutations; receipts print the actual bound. Two preemptions
//! suffice for every witness: the deepest witness interleaving needs the
//! scheduler to switch away from a runnable thread exactly twice (frame ->
//! writer -> stop, with the frame resumed to read). The N3 ledger
//! witnesses below (shape refusal, unchanged publication, both NeedGrace
//! refusals) are retained as asserted outcomes, composed with the frame
//! model rather than flipped: the read-only ledger cannot have its own
//! in-file edges dropped here, and those refusals are what the grace
//! period's backpressure contract is built on.
//!
//! Harness plumbing note: the vocabulary's `SymbolChunkAddress` carries a
//! `NonNull` and is deliberately not `Send`, so the chunk-address pool
//! crosses Loom thread boundaries only as bits and is materialized inside
//! each thread — the values are never dereferenced, which is the same
//! property the real ledger's own `Send`/`Sync` impls vouch for.
mod support;

#[path = "support/intern.rs"]
pub mod intern;
#[path = "support/symbol.rs"]
pub mod symbol;

pub use support::vocab::{
    OwnedSymbolTable, PointerTableGeneration, PreparedSymbolTable, PublishedSymbolTable,
    SymbolChunkAddress, SymbolChunkIndex, SymbolTableShape, TableError,
};
use support::vocab::{SymbolChunkCount, SymbolChunkSlots, SymbolSlotCount};

// The real ledger's whole API compiles here; the parts this model does
// not call are dead code by design (read-only consumption).
#[allow(dead_code)]
mod ledger {
    //! The real `publication.rs`, pasted read-only. `super::sync` is the
    //! crate-root Loom backend. A same-module `mod std` does not capture
    //! that path.
    include!(concat!(env!("OUT_DIR"), "/publication.inc"));
}
use ledger::{
    PublicationChange, PublishedSymbolTables, RetainedTableBytes, RetiredTableCount, TableLimit,
    TableLimits, TablePublicationError,
};

use loom::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use loom::sync::{Arc, Condvar, Mutex};
use std::num::NonZeroUsize;

/// Loom backend at the production `symbol_table::sync` parent seam.
/// The adapter's view loads through this module. The owner's mutex is the
/// nested backend below; `TryLockError` there stays real `std`.
mod sync {
    pub use loom::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
}

/// Generic owner. Its `sync` is the same Loom backend, so the root store and
/// the adapter's load are one modeled atomic. The mutex lives in this owner.
#[allow(dead_code)]
pub mod tagged {
    #[allow(dead_code)]
    pub mod gc {
        #[allow(dead_code)]
        pub mod publication {
            mod sync {
                pub use loom::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
                pub use loom::sync::{Mutex, MutexGuard};
                pub use std::sync::TryLockError;
            }
            include!(concat!(env!("OUT_DIR"), "/publication_types.inc"));
            #[allow(dead_code)]
            mod ledger {
                include!(concat!(env!("OUT_DIR"), "/publication_ledger.inc"));
            }
            pub(crate) use ledger::{
                Admission, Commit, OpenPublication, PublicationLedger, PublicationView,
            };
        }
    }
}

/// Chunk slots per descriptor chunk; the real backing uses `OBARRAY_CHUNK`.
const CHUNK_SLOTS: usize = 4;
const INITIAL_CHUNKS: usize = 5;
const GROWN_CHUNKS: usize = 6;
/// Two activations per frame keep the explored space inside the stated
/// preemption bound while still spanning a release-point interleaving.
const ACTIVATIONS: usize = 2;

/// The three edges of the stop proof this model exists to check.
#[derive(Clone, Copy)]
struct Config {
    /// N5 v1 entry validation: a stale or stopped frame must not enter the
    /// published path (StaleBinding resumes owner-locally, re-capturing).
    entry_validation: bool,
    /// The JIT-quiescence extension: every live JIT view returns before the
    /// stop receipt is minted.
    quiescence_wait: bool,
    /// Reclaiming the retired set happens only after a minted receipt, that
    /// is, only after grace was actually established.
    receipt_gates_reclaim: bool,
}
const PROVEN: Config = Config {
    entry_validation: true,
    quiescence_wait: true,
    receipt_gates_reclaim: true,
};

fn shape(chunks: usize) -> SymbolTableShape {
    SymbolTableShape::new(
        SymbolSlotCount::from(chunks * CHUNK_SLOTS),
        SymbolChunkCount::from(chunks),
        SymbolChunkSlots::from(NonZeroUsize::new(CHUNK_SLOTS).expect("constant is nonzero")),
    )
    .expect("exact coverage")
}

fn build_table(
    generation: PointerTableGeneration,
    chunks: usize,
    pool: &[SymbolChunkAddress],
) -> OwnedSymbolTable {
    PreparedSymbolTable::new(shape(chunks), generation)
        .expect("descriptor allocation")
        .initialize(pool[..chunks].iter().copied())
        .expect("exact address count")
}

/// A prefix-stable chunk-address pool as raw bits. The backing boxes are
/// leaked OUTSIDE the model body: the addresses are plain nonowning values
/// (never dereferenced, never freed), so they need no per-execution
/// identity, and a shared pool is what makes the growth publication's
/// prefix (`starts_with`) check succeed.
fn address_pool_bits() -> Vec<usize> {
    (0..GROWN_CHUNKS + 1)
        .map(|_| Box::leak(Box::new([0u8; 16])).as_ptr() as usize)
        .collect()
}

/// Materialize the pool inside one thread (see the plumbing note above).
fn materialize(bits: &[usize]) -> Vec<SymbolChunkAddress> {
    bits.iter()
        .map(|bits| {
            let root =
                std::ptr::NonNull::<()>::new(*bits as *mut ()).expect("pool address is nonnull");
            // SAFETY: the pool leaked its backing before the model ran and it
            // is never freed, so every materialization stays valid for every
            // published table; the address is never dereferenced.
            unsafe { SymbolChunkAddress::from_allocation(root) }
        })
        .collect()
}

struct GateState {
    stop_in_progress: bool,
    live_views: usize,
}

struct World {
    tables: PublishedSymbolTables,
    gate: Mutex<GateState>,
    quiesced: Condvar,
    /// Grace ended: the retired set was reclaimed.
    freed: AtomicBool,
    /// Tables retired before grace end (generations below this) are freed.
    freed_through: AtomicU64,
}

fn world(bits: &[usize]) -> World {
    let pool = materialize(bits);
    let initial = build_table(PointerTableGeneration::INITIAL, INITIAL_CHUNKS, &pool);
    World {
        tables: PublishedSymbolTables::new(initial, TableLimits::default())
            .expect("bootstrap within production limits"),
        gate: Mutex::new(GateState {
            stop_in_progress: false,
            live_views: 0,
        }),
        quiesced: Condvar::new(),
        freed: AtomicBool::new(false),
        freed_through: AtomicU64::new(0),
    }
}

/// What one activation entered through: the baked view of a publication.
#[derive(Clone, Copy)]
struct Binding {
    generation: PointerTableGeneration,
    chunk0: SymbolChunkAddress,
}

fn capture(tables: &PublishedSymbolTables) -> Binding {
    let view = tables.view();
    let table = view.table();
    Binding {
        generation: table.generation(),
        chunk0: table
            .chunk(SymbolChunkIndex::from(0))
            .expect("every shape has chunk 0"),
    }
}

fn read_witness(config: Config) -> &'static str {
    if config.entry_validation {
        "OLD_TABLE_RECLAIMED_BEFORE_GRACE"
    } else {
        "RESUME_USES_PRE_GRACE_POINTER"
    }
}

fn frame(config: Config, world: &World) {
    let mut binding = capture(&world.tables);
    for _ in 0..ACTIVATIONS {
        // Activation boundary — the entry gate. Entry validation happens
        // HERE, atomic with entry: the binding is compared against the
        // publication current at entry (a stale binding takes the
        // StaleBinding path and re-captures through the current view), and
        // a stopped frame is refused. The first draft of this model
        // validated BEFORE the gate and the composed run caught it as a
        // grace violation — a frame could pass a staleness check, then let
        // publication and the entire stop interleave, and enter after
        // grace end on a pre-grace binding. That is precisely the
        // entry-atomicity N5 v1 requires; the detector that caught it is
        // the one that stays armed below.
        let running = {
            let mut state = world.gate.lock().expect("loom mutexes do not poison");
            if config.entry_validation && state.stop_in_progress {
                false
            } else {
                if config.entry_validation {
                    let current = capture(&world.tables);
                    if current.generation != binding.generation {
                        // StaleBinding: resume owner-locally, re-capturing
                        // through the current view.
                        binding = current;
                    }
                }
                state.live_views += 1;
                true
            }
        };
        if !running {
            break; // stopped at a release point; run-to-return honored
        }
        // In-activation read through the bound descriptor.
        let _probe = binding.chunk0.as_ptr();
        if world.freed.load(Ordering::Acquire)
            && binding.generation.get() < world.freed_through.load(Ordering::Acquire)
        {
            panic!("{}", read_witness(config));
        }
        // Activation exit — the release point. Publication and the world
        // stop may interleave here before any re-entry is validated.
        {
            let mut state = world.gate.lock().expect("loom mutexes do not poison");
            state.live_views -= 1;
            world.quiesced.notify_one();
        }
    }
}

fn writer(world: &World, pool: &[SymbolChunkAddress]) -> PublicationChange {
    // Retained N3 witness: the ledger refuses a non-growth shape.
    let shrink = world.tables.try_prepare(shape(INITIAL_CHUNKS - 1)).result;
    assert!(
        matches!(shrink, Err(TablePublicationError::Table(TableError::Shape))),
        "ledger must refuse a smaller shape"
    );
    // Every intern attempt republishes; same shape is `Unchanged`.
    let unchanged = world
        .tables
        .try_prepare(shape(INITIAL_CHUNKS))
        .result
        .expect("the model's only ledger writer is this thread")
        .publish(pool[..INITIAL_CHUNKS].iter().copied())
        .expect("initial addresses still current");
    assert!(matches!(unchanged, PublicationChange::Unchanged { .. }));
    // Growth: retires the initial descriptor (the grace-period subject).
    world
        .tables
        .try_prepare(shape(GROWN_CHUNKS))
        .result
        .expect("the model's only ledger writer is this thread")
        .publish(pool[..GROWN_CHUNKS].iter().copied())
        .expect("prefix-stable growth")
}

fn stop(config: Config, world: &World) {
    if config.receipt_gates_reclaim {
        // Freeze: no activation enters the published path unvalidated.
        {
            let mut state = world.gate.lock().expect("loom mutexes do not poison");
            state.stop_in_progress = true;
        }
        if config.quiescence_wait {
            // JIT-quiescence extension: every live JIT view returns before
            // the receipt is minted (bounded: frames run to return).
            let mut state = world.gate.lock().expect("loom mutexes do not poison");
            while state.live_views > 0 {
                state = world
                    .quiesced
                    .wait(state)
                    .expect("loom mutexes do not poison");
            }
        }
        // The stop receipt, minted from the publication current at grace end.
        let _receipt = capture(&world.tables).generation;
        // Stop-proof invariant, armed while the entry gate holds: no live
        // JIT view exists at receipt time.
        if config.entry_validation {
            let state = world.gate.lock().expect("loom mutexes do not poison");
            assert!(state.live_views == 0, "ACK_WITH_LIVE_JIT_VIEW");
        }
    }
    // Grace end: the retired set is reclaimed. Gated by the receipt this is
    // post-quiescence; ungated, the free races live views and in-flight
    // activations (the OLD_TABLE_RECLAIMED_BEFORE_GRACE trigger).
    let through = capture(&world.tables).generation.get();
    world.freed_through.store(through, Ordering::Release);
    world.freed.store(true, Ordering::Release);
    if config.receipt_gates_reclaim {
        let mut state = world.gate.lock().expect("loom mutexes do not poison");
        state.stop_in_progress = false;
    }
}

/// The composed scenario. `verify` adds the outcome asserts that hold only
/// when no edge was flipped (the positive controls).
fn composed(config: Config, verify: bool) -> impl Fn() + Send + Sync + 'static {
    let bits = address_pool_bits();
    move || {
        let world = Arc::new(world(&bits));
        let writer_world = Arc::clone(&world);
        let frame_world = Arc::clone(&world);
        let stop_world = Arc::clone(&world);
        let writer_bits = bits.clone();
        let writer = loom::thread::spawn(move || {
            let pool = materialize(&writer_bits);
            writer(&writer_world, &pool)
        });
        let frame = loom::thread::spawn(move || frame(config, &frame_world));
        let stop = loom::thread::spawn(move || stop(config, &stop_world));
        let writer_change = support::join(writer);
        support::join(frame);
        support::join(stop);
        if verify {
            assert!(matches!(writer_change, PublicationChange::Published { .. }));
            let statistics = world.tables.stats().expect("uncontended after joins");
            assert_eq!(statistics.generation.get(), 2);
            assert_eq!(statistics.retired.get(), 1);
            assert!(world.freed.load(Ordering::Acquire));
        }
    }
}

#[test]
fn positive_grace_end_composes_with_publication() {
    support::check(3, composed(PROVEN, true));
}

#[test]
fn negative_resume_uses_pre_grace_pointer() {
    support::negative(
        3,
        "RESUME_USES_PRE_GRACE_POINTER",
        composed(
            Config {
                entry_validation: false,
                ..PROVEN
            },
            false,
        ),
    );
}

#[test]
fn negative_ack_with_live_jit_view() {
    support::negative(
        3,
        "ACK_WITH_LIVE_JIT_VIEW",
        composed(
            Config {
                quiescence_wait: false,
                ..PROVEN
            },
            false,
        ),
    );
}

#[test]
fn negative_old_table_reclaimed_before_grace() {
    support::negative(
        3,
        "OLD_TABLE_RECLAIMED_BEFORE_GRACE",
        composed(
            Config {
                receipt_gates_reclaim: false,
                ..PROVEN
            },
            false,
        ),
    );
}

/// Single-threaded container for the retained N3 witnesses: the real ledger
/// under the Loom shim, built with the caller's limits.
fn single(
    limits: TableLimits,
    body: impl Fn(&PublishedSymbolTables, &[SymbolChunkAddress]) + Send + Sync + 'static,
) -> impl Fn() + Send + Sync + 'static {
    let bits = address_pool_bits();
    move || {
        let pool = materialize(&bits);
        let initial = build_table(PointerTableGeneration::INITIAL, INITIAL_CHUNKS, &pool);
        let tables = PublishedSymbolTables::new(initial, limits).expect("bootstrap within limits");
        body(&tables, &pool);
    }
}

/// Retained N3 witness, composed: backpressure is a refusal, not a change.
/// With the retired-count limit at one, the second growth is refused with
/// `NeedGrace(RetiredTables)`, reserves nothing (`charged == 0` — the count
/// check precedes the reserve), leaves the publication state unchanged,
/// and interning continues through the current table. The refusal is
/// therefore not steady state: the stop that drains the retired set is the
/// only thing the writer is waiting on. (The drain itself is N5 code; the
/// read-only ledger has no such method, which is exactly why the grace
/// period owns it.)
#[test]
fn positive_need_grace_refusal_is_backpressure_not_steady_state() {
    support::check(
        1,
        single(
            TableLimits::new(
                RetainedTableBytes::new(64 * 1024 * 1024),
                RetiredTableCount::new(1),
            ),
            |tables, pool| {
                let grown = tables
                    .try_prepare(shape(GROWN_CHUNKS))
                    .result
                    .expect("first growth within limits")
                    .publish(pool[..GROWN_CHUNKS].iter().copied())
                    .expect("prefix-stable growth");
                assert!(matches!(grown, PublicationChange::Published { .. }));
                let refused = tables.try_prepare(shape(GROWN_CHUNKS + 1));
                assert!(matches!(
                    refused.result,
                    Err(TablePublicationError::NeedGrace(TableLimit::RetiredTables))
                ));
                assert_eq!(refused.charged.get(), 0);
                let statistics = tables.stats().expect("uncontended");
                assert_eq!(statistics.generation.get(), 2);
                assert_eq!(statistics.retired.get(), 1);
                // Interning continues through the current table after refusal.
                let unchanged = tables
                    .try_prepare(shape(GROWN_CHUNKS))
                    .result
                    .expect("refusal released the ledger")
                    .publish(pool[..GROWN_CHUNKS].iter().copied())
                    .expect("current addresses still valid");
                assert!(matches!(unchanged, PublicationChange::Unchanged { .. }));
            },
        ),
    );
}

/// Retained N3 witness: the byte-limit refusal runs AFTER the retired-slot
/// reserve, so a refusing attempt may retain capacity — the documented
/// asymmetry with the count refusal (whose check precedes the reserve and
/// whose charge is exactly zero, asserted above). Whether the reserve
/// actually grows the Vec is allocator policy, so the model asserts the
/// invariant that is not policy: the charge equals the retained-capacity
/// delta the ledger's own statistics report, the refusal changes no
/// publication state, and interning continues through the current table.
#[test]
fn positive_byte_limit_refusal_is_charged() {
    use std::mem::size_of;
    let table_bytes = |chunks: usize| {
        chunks * size_of::<SymbolChunkAddress>() + size_of::<PublishedSymbolTable>()
    };
    support::check(
        1,
        single(TableLimits::default(), move |tables, pool| {
            let grown = tables
                .try_prepare(shape(GROWN_CHUNKS))
                .result
                .expect("first growth within default limits")
                .publish(pool[..GROWN_CHUNKS].iter().copied())
                .expect("prefix-stable growth");
            assert!(matches!(grown, PublicationChange::Published { .. }));
            // Tighten the byte limit to one byte below what the next growth
            // needs (observed from the ledger's own accounting, not derived
            // from allocator internals).
            let before = tables.stats().expect("uncontended");
            let tight = before.retained_bytes.get() + table_bytes(GROWN_CHUNKS + 1) - 1;
            tables
                .set_limits_for_test(ledger::TestTableLimits::Override(TableLimits::new(
                    RetainedTableBytes::new(tight),
                    RetiredTableCount::new(1024),
                )))
                .expect("uncontended");
            let refused = tables.try_prepare(shape(GROWN_CHUNKS + 1));
            assert!(matches!(
                refused.result,
                Err(TablePublicationError::NeedGrace(TableLimit::RetainedBytes))
            ));
            let after = tables.stats().expect("uncontended");
            // The charge is the retained-capacity delta, nothing else.
            assert_eq!(
                refused.charged.get(),
                after.retained_bytes.get() - before.retained_bytes.get()
            );
            // The refusal changed no publication state.
            assert_eq!(before.generation, after.generation);
            assert_eq!(before.retired, after.retired);
            // Interning continues through the current table.
            let unchanged = tables
                .try_prepare(shape(GROWN_CHUNKS))
                .result
                .expect("refusal released the ledger")
                .publish(pool[..GROWN_CHUNKS].iter().copied())
                .expect("current addresses still valid");
            assert!(matches!(unchanged, PublicationChange::Unchanged { .. }));
        }),
    );
}

/// Retained N3 witness at construction: no table may be accepted whose own
/// bytes already exceed the retained-byte limit.
#[test]
fn positive_initial_publication_over_byte_limit_refuses() {
    let bits = address_pool_bits();
    support::check(1, move || {
        let pool = materialize(&bits);
        let initial = build_table(PointerTableGeneration::INITIAL, INITIAL_CHUNKS, &pool);
        let result = PublishedSymbolTables::new(
            initial,
            TableLimits::new(RetainedTableBytes::new(1), RetiredTableCount::new(1024)),
        )
        .map(|_| ());
        assert!(matches!(
            result,
            Err(TablePublicationError::NeedGrace(TableLimit::RetainedBytes))
        ));
    });
}

/// Real-drain grace model (E7-P.0 hardening): exercises the production
/// `PublishedSymbolTables::reclaim_retired_after_grace` + `PublicationLedger::drain_predecessors_after_grace`
/// under Loom after quiescing live activations, proving retired predecessors are
/// actually drained and `NeedGrace(RetiredTables)` is unblocked without UAF.
#[test]
fn positive_real_grace_reclaim_drains_predecessors_and_unblocks_need_grace() {
    let bits = address_pool_bits();
    support::check(2, move || {
        let pool = materialize(&bits);
        let initial = build_table(PointerTableGeneration::INITIAL, INITIAL_CHUNKS, &pool);
        let tables = PublishedSymbolTables::new(
            initial,
            TableLimits::new(
                RetainedTableBytes::new(64 * 1024 * 1024),
                RetiredTableCount::new(1),
            ),
        )
        .expect("bootstrap within limits");
        // Retire gen 1 so retired == 1 and next growth hits NeedGrace(RetiredTables).
        let first = tables
            .try_prepare(shape(GROWN_CHUNKS))
            .result
            .expect("first growth within cap")
            .publish(pool[..GROWN_CHUNKS].iter().copied())
            .expect("prefix-stable growth");
        assert!(matches!(first, PublicationChange::Published { .. }));
        assert!(matches!(
            tables.try_prepare(shape(GROWN_CHUNKS + 1)).result,
            Err(TablePublicationError::NeedGrace(TableLimit::RetiredTables))
        ));

        let world = Arc::new(World {
            tables,
            gate: Mutex::new(GateState {
                stop_in_progress: false,
                live_views: 0,
            }),
            quiesced: Condvar::new(),
            freed: AtomicBool::new(false),
            freed_through: AtomicU64::new(0),
        });
        let frame_world = Arc::clone(&world);
        let stop_world = Arc::clone(&world);

        let frame = loom::thread::spawn(move || {
            let entered = {
                let mut state = frame_world.gate.lock().expect("loom mutex");
                if state.stop_in_progress {
                    false
                } else {
                    state.live_views += 1;
                    true
                }
            };
            if entered {
                let view = frame_world.tables.view();
                assert_eq!(view.table().generation().get(), 2);
                assert_eq!(view.table().chunk_count().get(), GROWN_CHUNKS);
                drop(view);
                let mut state = frame_world.gate.lock().expect("loom mutex");
                state.live_views -= 1;
                frame_world.quiesced.notify_one();
            }
        });

        let stop = loom::thread::spawn(move || {
            {
                let mut state = stop_world.gate.lock().expect("loom mutex");
                state.stop_in_progress = true;
                while state.live_views > 0 {
                    state = stop_world.quiesced.wait(state).expect("loom mutex");
                }
                assert_eq!(state.live_views, 0);
            }
            // SAFETY: stop_in_progress == true and live_views == 0; every frame
            // activation has dropped its TableView and returned.
            let report = unsafe { stop_world.tables.reclaim_retired_after_grace() }
                .expect("quiesced grace drain must succeed");
            assert_eq!(report.reclaimed.get(), 1);
            assert!(report.freed_bytes.get() > 0);
            let mut state = stop_world.gate.lock().expect("loom mutex");
            state.stop_in_progress = false;
        });

        support::join(frame);
        support::join(stop);

        let stats = world.tables.stats().expect("uncontended after join");
        assert_eq!(stats.retired.get(), 0);
        assert_eq!(stats.generation.get(), 2);

        // Post-grace growth past the previous cap now succeeds.
        let unblocked = world
            .tables
            .try_prepare(shape(GROWN_CHUNKS + 1))
            .result
            .expect("grace drain unblocked RetiredTables cap")
            .publish(pool[..GROWN_CHUNKS + 1].iter().copied())
            .expect("prefix-stable growth after grace");
        assert!(matches!(unblocked, PublicationChange::Published { .. }));
        assert_eq!(world.tables.view().table().generation().get(), 3);
    });
}

/// Real-drain Loom model: a live `TableView` held by another thread keeps
/// `ledger.view_depth > 0`, forcing `reclaim_retired_after_grace` to refuse
/// `Busy` and retain the retired descriptor until the view drops.
#[test]
fn positive_real_grace_drain_refuses_busy_under_concurrent_table_view() {
    let bits = address_pool_bits();
    support::check(2, move || {
        let pool = materialize(&bits);
        let initial = build_table(PointerTableGeneration::INITIAL, INITIAL_CHUNKS, &pool);
        let tables = Arc::new(
            PublishedSymbolTables::new(initial, TableLimits::default())
                .expect("bootstrap within limits"),
        );
        let sync_state = Arc::new((Mutex::new((false, false)), Condvar::new()));

        let reader_tables = Arc::clone(&tables);
        let reader_sync = Arc::clone(&sync_state);
        let reader = loom::thread::spawn(move || {
            let old_view = reader_tables.view();
            assert_eq!(old_view.table().generation().get(), 1);
            {
                let mut flags = reader_sync.0.lock().expect("loom mutex");
                flags.0 = true;
                reader_sync.1.notify_one();
                while !flags.1 {
                    flags = reader_sync.1.wait(flags).expect("loom mutex");
                }
            }
            // Old descriptor must remain valid while `old_view` is live.
            assert_eq!(old_view.table().generation().get(), 1);
            assert_eq!(old_view.table().chunk_count().get(), INITIAL_CHUNKS);
            drop(old_view);
        });

        {
            let mut flags = sync_state.0.lock().expect("loom mutex");
            while !flags.0 {
                flags = sync_state.1.wait(flags).expect("loom mutex");
            }
        }
        // Grow while reader holds `old_view` (retires gen 1).
        tables
            .try_prepare(shape(GROWN_CHUNKS))
            .result
            .expect("growth while view live")
            .publish(pool[..GROWN_CHUNKS].iter().copied())
            .expect("prefix-stable growth");
        // SAFETY: testing fail-closed Busy refusal while `old_view` holds view_depth > 0.
        assert_eq!(
            unsafe { tables.reclaim_retired_after_grace() },
            Err(TablePublicationError::Busy)
        );
        assert_eq!(tables.stats().expect("stats").retired.get(), 1);
        {
            let mut flags = sync_state.0.lock().expect("loom mutex");
            flags.1 = true;
            sync_state.1.notify_one();
        }
        support::join(reader);

        // SAFETY: `reader` joined and dropped `old_view`; zero live views remain.
        let report = unsafe { tables.reclaim_retired_after_grace() }
            .expect("reclaim succeeds once TableView drops");
        assert_eq!(report.reclaimed.get(), 1);
        assert_eq!(tables.stats().expect("stats").retired.get(), 0);
    });
}

/// Real-drain Loom model: concurrent writer holding the metadata mutex via
/// `try_prepare` and grace drain racing on the same lock must never drop or
/// corrupt predecessors — every interleaving either succeeds cleanly or refuses
/// `Busy` while retaining all owners.
#[test]
fn positive_real_grace_drain_refuses_busy_under_contended_writer_mutex() {
    let bits = address_pool_bits();
    support::check(2, move || {
        let pool = materialize(&bits);
        let initial = build_table(PointerTableGeneration::INITIAL, INITIAL_CHUNKS, &pool);
        let tables = Arc::new(
            PublishedSymbolTables::new(initial, TableLimits::default())
                .expect("bootstrap within limits"),
        );
        tables
            .try_prepare(shape(GROWN_CHUNKS))
            .result
            .expect("initial growth")
            .publish(pool[..GROWN_CHUNKS].iter().copied())
            .expect("prefix-stable growth");

        let writer_tables = Arc::clone(&tables);
        let writer = loom::thread::spawn(move || {
            match writer_tables.try_prepare(shape(GROWN_CHUNKS)).result {
                Ok(open) => {
                    drop(open);
                }
                Err(TablePublicationError::Busy) => {}
                Err(other) => panic!("unexpected prepare error: {other:?}"),
            }
        });

        let drain_tables = Arc::clone(&tables);
        let drainer = loom::thread::spawn(move || {
            // SAFETY: no view is live; concurrent writer only contends on the
            // metadata mutex, where `drain_predecessors_after_grace` fails closed
            // with `Busy`.
            unsafe { drain_tables.reclaim_retired_after_grace() }
        });

        support::join(writer);
        let outcome = support::join(drainer);
        match outcome {
            Ok(report) => {
                assert_eq!(report.reclaimed.get(), 1);
                assert_eq!(tables.stats().expect("stats").retired.get(), 0);
            }
            Err(TablePublicationError::Busy) => {
                assert_eq!(tables.stats().expect("stats").retired.get(), 1);
                // SAFETY: all threads joined and zero views are live.
                let retry = unsafe { tables.reclaim_retired_after_grace() }
                    .expect("uncontended retry must drain");
                assert_eq!(retry.reclaimed.get(), 1);
                assert_eq!(tables.stats().expect("stats").retired.get(), 0);
            }
            Err(other) => panic!("unexpected drain error: {other:?}"),
        }
    });
}
