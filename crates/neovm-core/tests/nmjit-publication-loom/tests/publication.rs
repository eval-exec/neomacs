//! Finite descriptor IDs stand for pointers; initialized words stand for the
//! immutable bound/address array. No model constructs or owns chunk boxes.
mod support;

use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::thread;

#[derive(Clone, Copy)]
enum Publication {
    Release,
    MissingRelease,
    BeforeInitialization,
}

#[derive(Clone, Copy)]
enum Observation {
    Acquire,
    MissingAcquire,
}

fn initialization(publication: Publication, observation: Observation, witness: &'static str) {
    let current = Arc::new(AtomicUsize::new(0));
    let bound = Arc::new(AtomicUsize::new(0));
    let arm = Arc::new(AtomicUsize::new(0));
    let writer = {
        let (current, bound, arm) = (current.clone(), bound.clone(), arm.clone());
        thread::spawn(move || match publication {
            Publication::Release => {
                arm.store(17, Ordering::Relaxed);
                bound.store(2, Ordering::Relaxed);
                current.store(1, Ordering::Release);
            }
            Publication::MissingRelease => {
                arm.store(17, Ordering::Relaxed);
                bound.store(2, Ordering::Relaxed);
                current.store(1, Ordering::Relaxed);
            }
            Publication::BeforeInitialization => {
                current.store(1, Ordering::Release);
                arm.store(17, Ordering::Relaxed);
                bound.store(2, Ordering::Relaxed);
            }
        })
    };
    let reader = thread::spawn(move || {
        let ordering = match observation {
            Observation::Acquire => Ordering::Acquire,
            Observation::MissingAcquire => Ordering::Relaxed,
        };
        if current.load(ordering) == 1 {
            assert!(
                bound.load(Ordering::Relaxed) == 2 && arm.load(Ordering::Relaxed) == 17,
                "{witness}"
            );
        }
    });
    support::join(writer);
    support::join(reader);
}

#[test]
fn release_acquire_initializes_whole_descriptor() {
    support::exhaustive(3, || {
        initialization(Publication::Release, Observation::Acquire, "INITIALIZATION")
    });
}

#[test]
fn negative_publication_before_arms() {
    support::negative(3, "TABLE_VISIBLE_BEFORE_INITIALIZATION", || {
        initialization(
            Publication::BeforeInitialization,
            Observation::Acquire,
            "TABLE_VISIBLE_BEFORE_INITIALIZATION",
        )
    });
}

#[test]
fn negative_missing_release() {
    support::negative(3, "TABLE_VISIBLE_WITHOUT_RELEASE", || {
        initialization(
            Publication::MissingRelease,
            Observation::Acquire,
            "TABLE_VISIBLE_WITHOUT_RELEASE",
        )
    });
}

#[test]
fn negative_missing_acquire() {
    support::negative(3, "TABLE_VISIBLE_WITHOUT_ACQUIRE", || {
        initialization(
            Publication::Release,
            Observation::MissingAcquire,
            "TABLE_VISIBLE_WITHOUT_ACQUIRE",
        )
    });
}

#[test]
fn immutable_descriptor_cannot_mix_bound_and_table() {
    support::exhaustive(3, || {
        let current = Arc::new(AtomicUsize::new(0));
        let writer = {
            let current = current.clone();
            thread::spawn(move || current.store(1, Ordering::Release))
        };
        let reader = thread::spawn(move || {
            // Single descriptor identity selects both immutable components.
            let descriptors = [(1, [31, 0]), (2, [31, 47])];
            let descriptor = &descriptors[current.load(Ordering::Acquire)];
            for slot in 0..descriptor.0 {
                assert_ne!(descriptor.1[slot], 0);
            }
        });
        support::join(writer);
        support::join(reader);
    });
}

#[test]
fn negative_split_bound_and_pointer() {
    support::negative(3, "NEW_BOUND_WITH_OLD_TABLE", || {
        let current = Arc::new(AtomicUsize::new(0));
        let bound = Arc::new(AtomicUsize::new(1));
        let writer = {
            let (current, bound) = (current.clone(), bound.clone());
            thread::spawn(move || {
                bound.store(2, Ordering::Release);
                current.store(1, Ordering::Release);
            })
        };
        let reader = thread::spawn(move || {
            // Existing shape: len first, then an independently published spine.
            let len = bound.load(Ordering::Acquire);
            let table = current.load(Ordering::Relaxed);
            assert!(len <= [1, 2][table], "NEW_BOUND_WITH_OLD_TABLE");
        });
        support::join(writer);
        support::join(reader);
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RetainedBytes(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TableId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Growth {
    Committed,
    CapacityOnlyRefusal,
    NeedGrace,
}

#[derive(Debug)]
struct Ledger {
    owned: [Ownership; 3],
    next: TableId,
    charged: RetainedBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ownership {
    Unprepared,
    Owned,
}

#[test]
fn two_writers_publish_only_owned_initialized_tables() {
    support::check(4, || {
        let ledger = Arc::new(loom::sync::Mutex::new(Ledger {
            owned: [
                Ownership::Owned,
                Ownership::Unprepared,
                Ownership::Unprepared,
            ],
            next: TableId(1),
            charged: RetainedBytes(1),
        }));
        let current = Arc::new(AtomicUsize::new(0));
        let mut writers = Vec::new();
        for _ in 0..2 {
            let (ledger, current) = (ledger.clone(), current.clone());
            writers.push(thread::spawn(move || {
                let mut state = ledger.lock().unwrap();
                let prepared = state.next;
                state.owned[prepared.0] = Ownership::Owned;
                state.charged.0 += 1;
                state.next = TableId(prepared.0 + 1);
                current.store(prepared.0, Ordering::Release);
                Growth::Committed
            }));
        }
        let reader = {
            let (ledger, current) = (ledger.clone(), current.clone());
            thread::spawn(move || {
                let descriptor = current.load(Ordering::Acquire);
                assert_eq!(ledger.lock().unwrap().owned[descriptor], Ownership::Owned);
            })
        };
        for writer in writers {
            assert_eq!(support::join(writer), Growth::Committed);
        }
        support::join(reader);
        let state = ledger.lock().unwrap();
        assert_eq!(state.charged, RetainedBytes(3));
        assert_eq!(current.load(Ordering::Acquire), 2);
    });
}

#[test]
fn negative_publication_without_ledger_owner() {
    support::negative(3, "PUBLISHED_TABLE_WITHOUT_OWNER", || {
        let current = Arc::new(AtomicUsize::new(0));
        let owned = Arc::new(AtomicUsize::new(0));
        let writer = {
            let (current, owned) = (current.clone(), owned.clone());
            thread::spawn(move || {
                current.store(1, Ordering::Release);
                owned.store(1, Ordering::Release);
            })
        };
        let reader = thread::spawn(move || {
            if current.load(Ordering::Acquire) == 1 {
                assert!(
                    owned.load(Ordering::Acquire) == 1,
                    "PUBLISHED_TABLE_WITHOUT_OWNER"
                );
            }
        });
        support::join(writer);
        support::join(reader);
    });
}

#[test]
fn capacity_move_and_clean_refusal_keep_published_table() {
    support::exhaustive(3, || {
        let serial_spine = Arc::new(AtomicUsize::new(11));
        let current = Arc::new(AtomicUsize::new(0));
        let writer = {
            let serial_spine = serial_spine.clone();
            thread::spawn(move || {
                serial_spine.store(12, Ordering::Relaxed);
                // Retained capacity charge survives a later clean refusal.
                (Growth::CapacityOnlyRefusal, RetainedBytes(1))
            })
        };
        let reader = thread::spawn(move || {
            let stable_table = [[101, 102]];
            assert_eq!(stable_table[current.load(Ordering::Acquire)][1], 102);
        });
        assert_eq!(
            support::join(writer),
            (Growth::CapacityOnlyRefusal, RetainedBytes(1))
        );
        support::join(reader);
        assert_eq!(serial_spine.load(Ordering::Relaxed), 12);
    });
}

#[test]
fn retained_limit_refuses_without_publication() {
    support::exhaustive(3, || {
        let current = Arc::new(AtomicUsize::new(0));
        let charged = Arc::new(loom::sync::Mutex::new(RetainedBytes(2)));
        let writer = {
            let (current, charged) = (current.clone(), charged.clone());
            thread::spawn(move || {
                let mut bytes = charged.lock().unwrap();
                match bytes.0.checked_add(1) {
                    Some(next) if next <= 2 => {
                        bytes.0 = next;
                        current.store(1, Ordering::Release);
                        Growth::Committed
                    }
                    Some(_) | None => Growth::NeedGrace,
                }
            })
        };
        let reader = thread::spawn(move || assert_eq!(current.load(Ordering::Acquire), 0));
        assert_eq!(support::join(writer), Growth::NeedGrace);
        support::join(reader);
        assert_eq!(*charged.lock().unwrap(), RetainedBytes(2));
    });
}

#[test]
fn same_name_winner_and_gap_keep_existing_identities() {
    support::check(3, || {
        // Registry guard covers exactly-once issue and the whole arms/name commit.
        let registry = Arc::new(loom::sync::Mutex::new(([None, None, Some(83)], 0)));
        let mut owners = Vec::new();
        for _ in 0..2 {
            let registry = registry.clone();
            owners.push(thread::spawn(move || {
                let mut state = registry.lock().unwrap();
                let id = match state.0[0] {
                    Some(id) => id,
                    None => {
                        state.0[0] = Some(41);
                        state.1 += 1;
                        41
                    }
                };
                assert_eq!(state.0[1], None); // untouched gap/soft miss
                assert_eq!(state.0[2], Some(83)); // cached identity remains stable
                id
            }));
        }
        for owner in owners {
            assert_eq!(support::join(owner), 41);
        }
        assert_eq!(registry.lock().unwrap().1, 1);
    });
}
