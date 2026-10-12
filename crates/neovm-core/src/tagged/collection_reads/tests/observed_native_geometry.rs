//! Only existing-object native collection stores constrain their address gate.
//!
//! Real reference pseudovectors retain marks, dependencies and replay proofs;
//! their immutable ID headers cannot widen a cons/vector/record gate. Strings
//! retain their independent exact native mark guard rather than this window.
//! The isolated nextest process owns its mutator-local forced journal mode.

use super::super::{
    CompiledJournalMode, STATE, capture, compiled_observation_gate, force_compiled_journal_for_test,
};
use crate::buffer::BufferId;
use crate::heap_types::LispString;
use crate::tagged::gc::{BarrierWindow, TaggedHeap, collection_observed_metadata, set_tagged_heap};
use crate::tagged::header::VecLikeType;
use crate::tagged::value::{TAG_MASK, TaggedValue};

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

fn address(owner: TaggedValue) -> usize {
    owner.bits() & !TAG_MASK
}

#[test]
fn observed_reference_headers_retain_publication_without_native_geometry() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let owners = [
        heap.alloc_buffer(BufferId(7001)),
        heap.alloc_window(7002),
        heap.alloc_frame(7003),
        heap.alloc_window_configuration(vec![TaggedValue::make_int(97)]),
    ];
    let expected = [
        VecLikeType::Buffer,
        VecLikeType::Window,
        VecLikeType::Frame,
        VecLikeType::WindowConfiguration,
    ];
    let (_, reads) = capture(|| {
        for (&owner, &kind) in owners.iter().zip(&expected) {
            assert_eq!(owner.veclike_type(), Some(kind));
        }
    });
    let reads = reads.expect("real immutable-header dependencies are coherent");
    for owner in owners {
        assert!(collection_observed_metadata(owner.bits()));
        assert!(STATE.with(|state| state.borrow().observed.already_published(owner.bits())));
    }
    assert_eq!(
        STATE.with(|state| state.borrow().observed.full_bounds()),
        (usize::MAX, 0),
        "reference-only pseudovectors must not widen native collection-store geometry",
    );
    assert!(
        compiled_observation_gate(BarrierWindow::NONE)
            .observed
            .iter()
            .all(|range| *range == BarrierWindow::NONE),
    );
    assert!(reads.unchanged());
    let (_, replay) = capture(|| {
        assert!(reads.unchanged_and_observe());
        STATE.with(|state| {
            assert!(
                state
                    .borrow()
                    .captures
                    .last()
                    .unwrap()
                    .publication_epoch
                    .is_some()
            );
        });
    });
    assert!(
        replay
            .expect("non-native ledger facts remain replay-qualified")
            .unchanged()
    );
}

#[test]
fn observed_gc_rebuild_keeps_non_native_ledger_and_only_native_geometry() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let retained = heap.alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL);
    let reference = heap.alloc_buffer(BufferId(7101));
    let discarded = heap.alloc_vector(vec![TaggedValue::make_int(7)]);
    let (_, reads) = capture(|| {
        assert_eq!(retained.cons_car(), TaggedValue::make_int(97));
        assert_eq!(reference.veclike_type(), Some(VecLikeType::Buffer));
    });
    let reads = reads.expect("retained owners supply a real certificate");
    let _ = capture(|| {
        assert_eq!(
            discarded.as_vector_data().unwrap()[0],
            TaggedValue::make_int(7)
        )
    });
    heap.collect_exact([retained, reference].into_iter());
    assert!(heap.owns_heap_value_for_test(retained));
    assert!(heap.owns_heap_value_for_test(reference));
    assert!(!heap.owns_heap_value_for_test(discarded));
    assert!(!collection_observed_metadata(discarded.bits()));
    let gate = compiled_observation_gate(BarrierWindow::NONE);
    assert_eq!(
        STATE.with(|state| state.borrow().observed.full_bounds()),
        (address(retained), address(retained) + 1),
        "rebuild must reuse cached geometry eligibility without reading old headers",
    );
    assert!(
        gate.observed
            .iter()
            .any(|range| range.covers(address(retained)))
    );
    assert!(
        gate.observed
            .iter()
            .all(|range| !range.covers(address(reference)))
    );
    assert!(STATE.with(|state| state.borrow().observed.already_published(reference.bits())));
    assert!(collection_observed_metadata(reference.bits()));
    assert!(reads.unchanged());
    let (_, replay) = capture(|| {
        assert!(reads.unchanged_and_observe());
        STATE.with(|state| {
            assert!(
                state
                    .borrow()
                    .captures
                    .last()
                    .unwrap()
                    .publication_epoch
                    .is_some()
            );
        });
    });
    let replay = replay.expect("full publication facts survive GC and replay");
    retained.set_car(TaggedValue::make_int(23));
    assert!(!reads.unchanged());
    assert!(!replay.unchanged());
}

#[test]
fn observed_window_geometry_includes_cons_vector_and_record_owners() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let owners = [
        heap.alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL),
        heap.alloc_vector(vec![TaggedValue::make_int(97)]),
        heap.alloc_record(vec![TaggedValue::make_int(97)]),
    ];
    let (_, reads) = capture(|| {
        for owner in owners {
            assert!(owner.heap_ptr().is_some());
        }
    });
    let reads = reads.expect("all native-store owners are coherent");
    let gate = compiled_observation_gate(BarrierWindow::NONE);
    for owner in owners {
        assert!(collection_observed_metadata(owner.bits()));
        assert!(STATE.with(|state| state.borrow().observed.already_published(owner.bits())));
        assert!(
            gate.observed
                .iter()
                .any(|range| range.covers(address(owner))),
            "every existing-object inline window user must constrain the gate",
        );
    }
    assert!(reads.unchanged());
}

#[cfg(feature = "jit")]
#[test]
fn observed_string_keeps_exact_mark_and_compiled_invalidation_without_window_geometry() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut context = crate::emacs_core::eval::Context::new();
    context.gc_stress = false;
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.setup_thread_locals();
    let owner = context
        .tagged_heap
        .alloc_string(LispString::from_utf8("abc"));
    let (_, reads) = capture(|| assert_eq!(owner.as_str_owned().unwrap(), "abc"));
    let reads = reads.expect("the retained string supplies a real certificate");
    assert!(collection_observed_metadata(owner.bits()));
    assert!(STATE.with(|state| state.borrow().observed.already_published(owner.bits())));
    assert_eq!(
        STATE.with(|state| state.borrow().observed.full_bounds()),
        (usize::MAX, 0),
        "string native stores use the exact header mark and must not widen common geometry",
    );
    assert!(
        compiled_observation_gate(BarrierWindow::NONE)
            .observed
            .iter()
            .all(|range| *range == BarrierWindow::NONE),
    );
    let (_, replay) = capture(|| {
        assert!(reads.unchanged_and_observe());
        STATE.with(|state| {
            assert!(
                state
                    .borrow()
                    .captures
                    .last()
                    .unwrap()
                    .publication_epoch
                    .is_some()
            );
        });
    });
    let replay = replay.expect("string publication remains replay-qualified");
    let value = TaggedValue::make_int(i64::from(b'z'));
    let vmctx = (&mut context as *mut crate::emacs_core::eval::Context).cast::<u8>();
    assert_eq!(
        crate::emacs_core::jit::compile::neovm_jit_aset_collection(
            vmctx,
            owner.bits() as i64,
            TaggedValue::make_int(0).bits() as i64,
            value.bits() as i64,
        ),
        value.bits() as i64,
    );
    assert_eq!(owner.as_str_owned().unwrap(), "zbc");
    assert!(!reads.unchanged());
    assert!(!replay.unchanged());
}
