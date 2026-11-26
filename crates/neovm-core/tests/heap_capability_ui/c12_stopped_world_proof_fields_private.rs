//! value-gc C12 regression guard: `StoppedWorldProof`'s fields are private to
//! its leaf module; external code cannot forge a proof via struct literal.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HeapIdentity(u64);

struct YoungLoans;

impl YoungLoans {
    fn has_outstanding_young_loans(&self) -> bool {
        false
    }
}

struct TaggedHeap {
    _satb_layout_reserve: YoungLoans,
}

impl TaggedHeap {
    fn concurrent_mark_running(&self) -> bool {
        false
    }
    fn has_unfinished_concurrent_mark(&self) -> bool {
        false
    }
    fn has_owner_cons_reserves(&self) -> bool {
        false
    }
    fn heap_identity(&self) -> HeapIdentity {
        HeapIdentity(1)
    }
    fn mark_launch_epoch(&self) -> u64 {
        0
    }
}

struct StoppedWorldEntry;

impl StoppedWorldEntry {
    fn enter(_heap: &TaggedHeap) -> Self {
        Self
    }
}

mod scan_contract {
    pub struct ParallelStoppedMutatorsProof<'epoch>(std::marker::PhantomData<&'epoch ()>);

    impl ParallelStoppedMutatorsProof<'_> {
        pub fn heap_identity(&self) -> super::HeapIdentity {
            super::HeapIdentity(1)
        }
    }
}

#[path = "../../../neovm-value/src/tagged/gc/stopped_world_proof.rs"]
mod stopped_world_proof;

fn main() {
    let _ = stopped_world_proof::StoppedWorldProof {
        heap_identity: HeapIdentity(1),
        mark_launch_epoch: 0,
        _scope: std::marker::PhantomData,
    };
}
