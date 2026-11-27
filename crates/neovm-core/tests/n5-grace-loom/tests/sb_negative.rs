//! Negative Loom control for E6-P.4 #16434: downgrading the `SeqCst`
//! Store-Buffering pair in `PublicationLedger::view` / `release_retained` /
//! `drain_predecessors_after_grace` back to `Acquire`/`Acquire`/`Release`/`Acquire`
//! (`OUT_DIR/publication_ledger_weak_sb.inc`) must trip
//! `SB_UAF_RECLAIMED_BEHIND_LIVE_TABLE_VIEW` in the handshake-free model.
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

#[allow(dead_code)]
mod ledger {
    include!(concat!(env!("OUT_DIR"), "/publication.inc"));
}
use ledger::{PublishedSymbolTables, TableLimits, TablePublicationError};

use loom::sync::Arc;
use std::num::NonZeroUsize;

mod sync {
    pub use loom::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
}

#[allow(dead_code)]
pub mod tagged {
    #[allow(dead_code)]
    pub mod gc {
        #[allow(dead_code)]
        pub mod publication {
            mod sync {
                pub use loom::sync::atomic::{AtomicPtr, AtomicU32, Ordering, fence};
                pub use loom::sync::{Mutex, MutexGuard};
                pub use std::sync::TryLockError;
            }
            include!(concat!(env!("OUT_DIR"), "/publication_types.inc"));
            #[allow(dead_code)]
            mod ledger {
                include!(concat!(env!("OUT_DIR"), "/publication_ledger_weak_sb.inc"));
            }
            pub(crate) use ledger::{
                Admission, Commit, OpenPublication, PublicationLedger, PublicationView,
            };
        }
    }
}

const CHUNK_SLOTS: usize = 4;
const INITIAL_CHUNKS: usize = 5;
const GROWN_CHUNKS: usize = 6;

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

fn address_pool_bits() -> Vec<usize> {
    (0..GROWN_CHUNKS + 1)
        .map(|_| Box::leak(Box::new([0u8; 16])).as_ptr() as usize)
        .collect()
}

fn materialize(bits: &[usize]) -> Vec<SymbolChunkAddress> {
    bits.iter()
        .map(|bits| {
            let root =
                std::ptr::NonNull::<()>::new(*bits as *mut ()).expect("pool address is nonnull");
            // SAFETY: the leaked pool outlives the test and is never dereferenced.
            unsafe { SymbolChunkAddress::from_allocation(root) }
        })
        .collect()
}

#[test]
fn negative_weak_sb_ordering_reclaims_behind_live_table_view() {
    let bits = address_pool_bits();
    support::negative(2, "SB_UAF_RECLAIMED_BEHIND_LIVE_TABLE_VIEW", move || {
        let pool = materialize(&bits);
        let initial = build_table(PointerTableGeneration::INITIAL, INITIAL_CHUNKS, &pool);
        let tables = Arc::new(
            PublishedSymbolTables::new(initial, TableLimits::default())
                .expect("bootstrap within limits"),
        );
        let initial_ptr = {
            let v = tables.view();
            v.table() as *const PublishedSymbolTable as usize
        };

        let drainer_tables = Arc::clone(&tables);
        let drainer_bits = bits.clone();
        let drainer = loom::thread::spawn(move || {
            let pool = materialize(&drainer_bits);
            drainer_tables
                .try_prepare(shape(GROWN_CHUNKS))
                .result
                .expect("growth")
                .publish(pool[..GROWN_CHUNKS].iter().copied())
                .expect("prefix-stable growth");
            // SAFETY: negative control exercising the weakened ordering.
            unsafe { drainer_tables.reclaim_retired_after_grace() }
        });

        let view = tables.view();
        let seen_ptr = view.table() as *const PublishedSymbolTable as usize;

        let drain_outcome = support::join(drainer);
        match drain_outcome {
            Ok(report) => {
                assert_eq!(report.reclaimed.get(), 1);
                if seen_ptr == initial_ptr {
                    panic!("SB_UAF_RECLAIMED_BEHIND_LIVE_TABLE_VIEW");
                }
            }
            Err(TablePublicationError::Busy) => {}
            Err(other) => panic!("unexpected drain error: {other:?}"),
        }
        drop(view);
    });
}
