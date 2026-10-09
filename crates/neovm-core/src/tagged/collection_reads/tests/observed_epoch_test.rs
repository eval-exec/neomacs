//! Real repeated reads and reclamation must leave a precise live-owner gate.
//!
//! Each nextest invocation runs one test in its own process. Generation mode
//! is changed only while constructing a private heap, then immediately restored.
//! Certificates remain on this mutator; explicit roots retain their owners at GC.

use super::*;
use crate::heap_types::LispString;
use crate::tagged::gc::{BarrierWindow, TaggedHeap, set_tagged_heap};
use crate::tagged::value::TAG_MASK;

struct ObservedMode;

impl ObservedMode {
    fn begin() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        Self
    }
}

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

#[derive(Clone, Copy, Debug)]
enum OwnerKind {
    Cons,
    Vector,
    Record,
    String,
}

impl OwnerKind {
    fn allocate(self, heap: &mut TaggedHeap) -> TaggedValue {
        match self {
            Self::Cons => heap.alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL),
            Self::Vector => heap.alloc_vector(vec![TaggedValue::make_int(97)]),
            Self::Record => heap.alloc_record(vec![TaggedValue::make_int(97)]),
            Self::String => heap.alloc_string(LispString::from_utf8("abc")),
        }
    }

    fn read(self, owner: TaggedValue) -> TaggedValue {
        match self {
            Self::Cons => owner.cons_car(),
            Self::Vector => owner.as_vector_data().expect("vector")[0],
            Self::Record => owner.as_record_data().expect("record")[0],
            Self::String => TaggedValue::make_int(i64::from(
                owner.as_str_owned().expect("string").as_bytes()[0],
            )),
        }
    }

    fn mutate(self, owner: TaggedValue) {
        let supplied = TaggedValue::make_int(122);
        match self {
            Self::Cons => owner.set_car(supplied),
            Self::Vector => assert!(crate::tagged::mutate::set_vector_slot(owner, 0, supplied)),
            Self::Record => assert!(crate::tagged::mutate::set_record_slot(owner, 0, supplied)),
            Self::String => assert!(crate::tagged::mutate::set_string_byte_same_char_count(
                owner, 0, b'z'
            )),
        }
    }
}

fn heap_with_generation(generational: bool) -> Box<TaggedHeap> {
    struct RestoreGeneration(Option<std::ffi::OsString>);
    impl Drop for RestoreGeneration {
        fn drop(&mut self) {
            // This isolated test owns the process environment and runs no
            // concurrent constructors while its private heap is initialized.
            unsafe {
                match self.0.take() {
                    Some(value) => std::env::set_var("NEOVM_GC_GENERATIONAL", value),
                    None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
                }
            }
        }
    }
    let _restore = RestoreGeneration(std::env::var_os("NEOVM_GC_GENERATIONAL"));
    unsafe {
        if generational {
            std::env::set_var("NEOVM_GC_GENERATIONAL", "1");
        } else {
            std::env::remove_var("NEOVM_GC_GENERATIONAL");
        }
    }
    let heap = Box::new(TaggedHeap::new());
    assert_eq!(heap.generational_enabled(), generational);
    heap
}

fn repeated_real_reads(kind: OwnerKind) {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = heap_with_generation(false);
    set_tagged_heap(&mut heap);
    let owner = kind.allocate(&mut heap);
    let (result, certificate) = capture(|| {
        OBSERVED_OWNER_PUBLICATIONS.with(|count| count.set(0));
        let mut result = TaggedValue::NIL;
        for _ in 0..1024 {
            result = kind.read(owner);
        }
        assert_eq!(
            OBSERVED_OWNER_PUBLICATIONS.with(Cell::get),
            1,
            "repeated reads of one live owner must reuse its publication: {kind:?}"
        );
        result
    });
    assert_eq!(result, TaggedValue::make_int(97));
    let certificate = certificate.expect("unchanged repeated reads are coherent");
    assert!(certificate.unchanged());
    kind.mutate(owner);
    assert_eq!(kind.read(owner), TaggedValue::make_int(122));
    assert!(!certificate.unchanged(), "the dependency is still retained");
}

fn reclaimed_endpoints_contract(kind: OwnerKind, generational: bool) {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = heap_with_generation(generational);
    set_tagged_heap(&mut heap);
    let mut owners = [
        kind.allocate(&mut heap),
        kind.allocate(&mut heap),
        kind.allocate(&mut heap),
    ];
    owners.sort_unstable_by_key(|owner| owner.bits() & !TAG_MASK);
    let [first, middle, last] = owners;
    let addresses = owners.map(|owner| owner.bits() & !TAG_MASK);
    assert!(addresses[0] < addresses[1] && addresses[1] < addresses[2]);
    let (_, all_reads) = capture(|| {
        for owner in owners {
            assert_eq!(kind.read(owner), TaggedValue::make_int(97));
        }
    });
    // Only the middle certificate will remain meaningful after collection;
    // the explicit GC root below retains that owner's actual storage.
    drop(all_reads.expect("three coherent owners"));
    let (_, middle_reads) = capture(|| kind.read(middle));
    let middle_reads = middle_reads.expect("rooted middle dependency");
    let before = compiled_observation_window();
    for address in addresses {
        assert!(before.0 <= address && address < before.1);
    }

    heap.collect_exact(std::iter::once(middle));
    assert!(!heap.owns_heap_value_for_test(first));
    assert!(heap.owns_heap_value_for_test(middle));
    assert!(!heap.owns_heap_value_for_test(last));
    let published = heap.jit_barrier_window_for_test();
    if generational {
        assert_eq!(
            published,
            BarrierWindow::NONE,
            "GEN1 keeps its ordinary gate"
        );
    } else {
        assert!(published.covers(addresses[1]));
        assert!(
            !published.covers(addresses[0]),
            "reclaimed low endpoint: {kind:?}"
        );
        assert!(
            !published.covers(addresses[2]),
            "reclaimed high endpoint: {kind:?}"
        );
    }
    let after = compiled_observation_window();
    assert_eq!(
        after,
        (addresses[1], addresses[1] + 1),
        "only the rooted observed owner may keep the local envelope wide: {kind:?}"
    );
    assert!(
        middle_reads.unchanged(),
        "GC preserves this live dependency"
    );
    kind.mutate(middle);
    assert_eq!(kind.read(middle), TaggedValue::make_int(122));
    assert!(
        !middle_reads.unchanged(),
        "later owner mutation still invalidates"
    );
}

#[test]
fn observed_repeated_cons_reads_publish_owner_once_per_capture() {
    repeated_real_reads(OwnerKind::Cons);
}

#[test]
fn recent_read_reclamation_acknowledges_epoch_in_every_journal_mode() {
    struct RestoreMode;
    impl Drop for RestoreMode {
        fn drop(&mut self) {
            force_compiled_journal_for_test(None);
        }
    }
    crate::test_utils::init_test_tracing();
    let _restore = RestoreMode;
    let mut heap = heap_with_generation(false);
    set_tagged_heap(&mut heap);
    for mode in [
        CompiledJournalMode::Observed,
        CompiledJournalMode::Off,
        CompiledJournalMode::Eager,
    ] {
        // Reclaim an actually observed, unrooted cell so the sweep must
        // publish a new epoch even when the measured policy is Off/Eager.
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        let garbage = OwnerKind::Cons.allocate(&mut heap);
        capture(|| garbage.cons_car());
        force_compiled_journal_for_test(Some(mode));
        let owner = OwnerKind::Cons.allocate(&mut heap);
        capture(|| {
            owner.cons_car();
            let before = super::super::gc::collection_observation_epoch();
            heap.collect_exact(std::iter::once(owner));
            let after = super::super::gc::collection_observation_epoch();
            assert_ne!(before, after, "a real sweep must publish reclamation");
            assert_eq!(
                recently_observed(owner.bits()),
                mode != CompiledJournalMode::Observed,
                "Observed must reject a stale hit; Off/Eager retain legacy deduplication"
            );
            assert_eq!(
                RECENT_READS.with(|recent| recent.epoch.get()),
                after,
                "the cold reclamation edge must acknowledge every mode"
            );
            assert!(recently_observed(owner.bits()), "same-epoch second hit");
        });
    }
}

#[test]
fn observed_repeated_vector_reads_publish_owner_once_per_capture() {
    repeated_real_reads(OwnerKind::Vector);
}

#[test]
fn observed_repeated_string_reads_publish_owner_once_per_capture() {
    repeated_real_reads(OwnerKind::String);
}

#[test]
fn observed_gen0_cons_gc_contracts_reclaimed_envelope_endpoints() {
    reclaimed_endpoints_contract(OwnerKind::Cons, false);
}

#[test]
fn observed_gen0_vector_gc_contracts_reclaimed_envelope_endpoints() {
    reclaimed_endpoints_contract(OwnerKind::Vector, false);
}

#[test]
fn observed_gen0_record_gc_contracts_reclaimed_envelope_endpoints() {
    reclaimed_endpoints_contract(OwnerKind::Record, false);
}

#[test]
fn observed_gen0_string_gc_contracts_reclaimed_envelope_endpoints() {
    reclaimed_endpoints_contract(OwnerKind::String, false);
}

#[test]
fn observed_gen1_cons_gc_contracts_reclaimed_envelope_endpoints() {
    reclaimed_endpoints_contract(OwnerKind::Cons, true);
}

#[test]
fn observed_gen1_vector_gc_contracts_reclaimed_envelope_endpoints() {
    reclaimed_endpoints_contract(OwnerKind::Vector, true);
}

#[test]
fn observed_gen1_record_gc_contracts_reclaimed_envelope_endpoints() {
    reclaimed_endpoints_contract(OwnerKind::Record, true);
}

#[test]
fn observed_gen1_string_gc_contracts_reclaimed_envelope_endpoints() {
    reclaimed_endpoints_contract(OwnerKind::String, true);
}
