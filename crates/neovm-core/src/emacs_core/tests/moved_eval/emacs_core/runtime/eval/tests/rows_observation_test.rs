//! Partial metadata receipts only; no row admission or source lease here.
use crate::emacs_core::eval::*;
use crate::window::DisplayFrameManagerExt;
use neovm_value::display_vocab::FrameId;
use neovm_value::tagged::collection_reads::{capture, capture_normalized};
use neovm_value::value::Value;

fn ready() -> (Context, FrameId) {
    let mut context = crate::test_utils::new_context();
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut context, false);
    // These receipts require GEN1, independently of the test runner's policy.
    // SAFETY: serial fixture heap; no collector or compiled leaf runs across
    // the GEN1 policy change.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            context.tagged_heap_mut(),
            true,
        )
    };
    unsafe { context.tagged_heap_mut() }.set_gc_threshold(usize::MAX);
    context.gc_collect_exact();
    let source = context.buffer_manager().current_buffer().unwrap().id();
    let frame = context
        .frame_manager_mut()
        .create_frame("rows-domain", 400, 160, source);
    context
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("x")));
    context.sync_runtime_faces_for_frame(frame);
    context.setup_thread_locals();
    assert!(
        context.tagged_heap().tagged_heap_generational_enabled(),
        "GEN1 test policy required"
    );
    (context, frame)
}

#[test]
fn rows_observation_owned_keys_and_mutation_journal_are_both_required() {
    let (context, frame) = ready();
    let cons = Value::cons(Value::fixnum(97), Value::NIL);
    let vector = Value::vector(vec![Value::fixnum(1)]);
    let string = Value::string("abc");
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    let (_, reads) = capture(|| {
        assert_eq!(cons.cons_car(), Value::fixnum(97));
        assert_eq!(vector.as_vector_data().unwrap().len(), 1);
        assert_eq!(string.as_utf8_str(), Some("abc"));
    });
    let proof = context
        .finish_static_rows_observation(epoch, reads.as_ref().unwrap())
        .unwrap();
    assert!(proof.current_and_observe(&context));
    cons.set_car(Value::fixnum(98));
    assert!(!proof.current_and_observe(&context));
}

#[test]
fn rows_observation_includes_normalization_reads_before_the_output_walk() {
    let (mut context, frame) = ready();
    let normalized = Value::cons(Value::fixnum(1), Value::NIL);
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    let (_, reads) = capture_normalized(
        &mut context,
        |_| {
            assert_eq!(normalized.cons_car(), Value::fixnum(1));
        },
        |_| (),
    );
    let proof = context
        .finish_static_rows_observation(epoch, reads.as_ref().unwrap())
        .unwrap();
    normalized.set_car(Value::fixnum(2));
    assert!(
        !proof.current_and_observe(&context),
        "normalization is inside the owned-domain interval"
    );
}

#[test]
fn rows_observation_gc_rejects_before_any_stale_dependency_replay() {
    let (mut context, frame) = ready();
    let cons = Value::cons(Value::fixnum(97), Value::NIL);
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    let (_, reads) = capture(|| cons.cons_car());
    context.gc_collect_exact();
    assert!(matches!(
        context.finish_static_rows_observation(epoch, reads.as_ref().unwrap()),
        Err(RowsObservationRefusal::CollectionChanged)
    ));
    // The old key may be dead. No header access or replay follows.
}

#[test]
fn rows_observation_driver_and_partial_collection_refuse_before_replay() {
    let (mut context, frame) = ready();
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    neovm_eval::test_support::authority::set_gc_driver_active_for_test(&mut context, true);
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::DriverActive)
    );
    neovm_eval::test_support::authority::set_gc_driver_active_for_test(&mut context, false);
    let count = context.tagged_heap().gc_collections();
    // SAFETY: serial fixture heap, quiescent; no concurrent marker runs.
    unsafe {
        neovm_value::test_support::authority::begin_collection_for_test(context.tagged_heap_mut())
    };
    assert_eq!(context.tagged_heap().gc_collections(), count);
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::CollectorBusy)
    );
    // Existing recovery driver supplies every Context root before finishing.
    context.gc_collect_exact();
}

#[test]
fn rows_observation_replaced_or_inactive_context_refuses_before_saved_keys() {
    let (mut first, frame) = ready();
    let epoch = first.begin_static_rows_observation(frame).unwrap();
    let (second, _) = ready();
    assert_eq!(
        first.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::InactiveContext)
    );
    assert_eq!(
        second.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::RuntimeChanged)
    );
    first.setup_thread_locals();
    assert!(first.static_rows_observation_current(&epoch).is_ok());
}

#[test]
fn rows_observation_foreign_dependency_dropped_during_capture_stays_refused() {
    let (context, frame) = ready();
    let mut foreign = Box::new(neovm_value::tagged::gc::TaggedHeap::new());
    let cons = foreign.alloc_cons(Value::fixnum(7), Value::NIL);
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    let (_, reads) = capture(|| {
        assert_eq!(cons.cons_car(), Value::fixnum(7));
        drop(foreign);
    });
    assert!(matches!(
        context.finish_static_rows_observation(epoch, reads.as_ref().unwrap()),
        Err(RowsObservationRefusal::UnknownOwner)
    ));
}

#[test]
fn rows_observation_unknown_box_dependency_refuses() {
    let (mut context, frame) = ready();
    let table = context.eval_str("(make-hash-table)").unwrap();
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    let (_, reads) = capture(|| {
        assert!(table.as_hash_table().is_some());
    });
    assert!(matches!(
        context.finish_static_rows_observation(epoch, reads.as_ref().unwrap()),
        Err(RowsObservationRefusal::UnknownOwner)
    ));
}

#[test]
fn rows_observation_journal_overflow_refuses_even_disjoint_mutations() {
    let (context, frame) = ready();
    let observed = Value::cons(Value::fixnum(1), Value::NIL);
    let other = Value::cons(Value::NIL, Value::NIL);
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    let (_, reads) = capture(|| observed.cons_car());
    let proof = context
        .finish_static_rows_observation(epoch, reads.as_ref().unwrap())
        .unwrap();
    for i in 0..8193 {
        other.set_car(Value::fixnum(i));
    }
    assert!(!proof.current_and_observe(&context));
}

#[test]
fn rows_observation_graphical_warm_frame_is_required_at_both_ends() {
    let (mut context, frame) = ready();
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    context
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(None);
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::TtyOrUnknownFrame)
    );
    assert!(matches!(
        context.begin_static_rows_observation(frame),
        Err(RowsObservationRefusal::TtyOrUnknownFrame)
    ));
}

#[test]
fn rows_observation_cold_faces_and_test_only_gen0_switch_refuse() {
    let (mut context, frame) = ready();
    *context.face_change_count_mut() += 1;
    assert!(matches!(
        context.begin_static_rows_observation(frame),
        Err(RowsObservationRefusal::ColdFaces)
    ));
    context.sync_runtime_faces_for_frame(frame);
    // Production enabled is immutable after construction. This cfg(test)
    // setter tests the admission predicate, not a supported policy transition.
    // SAFETY: serial fixture heap; no collector or compiled leaf runs across
    // the GEN1 policy change.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            context.tagged_heap_mut(),
            false,
        )
    };
    assert!(matches!(
        context.begin_static_rows_observation(frame),
        Err(RowsObservationRefusal::GenerationalDisabled)
    ));
    // SAFETY: serial fixture heap; no collector or compiled leaf runs across
    // the GEN1 policy change.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            context.tagged_heap_mut(),
            true,
        )
    };
}

struct RefusingHost;
impl crate::emacs_core::display_host::DisplayHost for RefusingHost {
    fn realize_gui_frame(
        &mut self,
        _: crate::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        panic!("admission must not enter the installed host")
    }
    fn resize_gui_frame(
        &mut self,
        _: crate::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        panic!("admission must not enter the installed host")
    }
}

#[test]
fn rows_observation_any_display_host_refuses_without_host_entry() {
    let (mut context, frame) = ready();
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    context.display_host = Some(Box::new(RefusingHost));
    assert!(matches!(
        context.begin_static_rows_observation(frame),
        Err(RowsObservationRefusal::DisplayHostInstalled)
    ));
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::DisplayHostInstalled)
    );
}

#[test]
fn rows_observation_dump_authority_is_the_actual_retained_mapping_not_temporary_or_new_image() {
    use crate::emacs_core::pdump::mmap_image_test_support as mmap_image;
    use crate::emacs_core::pdump::mmap_image_test_support::{DumpSectionKind, ImageSection};
    let directory = tempfile::tempdir().unwrap();
    let first_path = directory.path().join("rows-origin-first.pdump");
    let second_path = directory.path().join("rows-origin-second.pdump");
    for path in [&first_path, &second_path] {
        mmap_image::write_image(
            path,
            &[ImageSection {
                kind: DumpSectionKind::HeapImage,
                flags: 0,
                bytes: b"numeric authority fixture",
            }],
        )
        .unwrap();
    }
    let (mut context, frame) = ready();
    let temporary = mmap_image::load_image(&first_path).unwrap();
    let without_image = context.begin_static_rows_observation(frame).unwrap();
    assert!(
        without_image.dump_range_for_test().is_none(),
        "an unrelated live mapping grants no authority"
    );
    drop(temporary); // Really unmapped; no object/key is read from that payload.
    assert!(
        context
            .static_rows_observation_current(&without_image)
            .is_ok()
    );
    let retained: &'static mmap_image::LoadedMmapImage =
        Box::leak(Box::new(mmap_image::load_image(&first_path).unwrap()));
    // SAFETY: `retained` is a real process-retained mmap with a 'static lease
    // that outlives this Context and is never remapped while installed.
    unsafe {
        neovm_eval::test_support::authority::install_pdump_image_for_test(
            &mut context,
            Some(retained),
        )
    };
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    let range = epoch.dump_range_for_test().unwrap();
    let payload = retained
        .section(DumpSectionKind::HeapImage)
        .unwrap()
        .as_ptr() as usize;
    assert!(range.contains(&payload));
    assert!(!range.contains(&range.end));
    assert!(context.static_rows_observation_current(&epoch).is_ok());
    // SAFETY: the replacement is a real process-retained mmap with a 'static
    // lease that outlives this Context.
    unsafe {
        neovm_eval::test_support::authority::install_pdump_image_for_test(
            &mut context,
            Some(Box::leak(Box::new(
                mmap_image::load_image(&second_path).unwrap(),
            ))),
        )
    };
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::RuntimeChanged)
    );
    // These are real mmap owners, not fabricated Lisp objects. This test
    // checks authority acquisition/bounds/replacement, never projects arbitrary
    // file bytes as a Lisp pointer or claims a mapped dependency positive.
}

#[test]
fn rows_observation_real_retained_mapped_cons_dependency_matches_only_captured_image() {
    use crate::emacs_core::pdump::mmap_image_test_support as mmap_image;
    use crate::emacs_core::pdump::mmap_image_test_support::{DumpSectionKind, ImageSection};
    use neovm_value::tagged::header::ConsCell;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rows-origin-cons.pdump");
    // ConsCell is the pinned two-tagged-word #[repr(C)] layout. Encode actual
    // valid scalar car/cdr words rather than casting arbitrary file payload.
    let mut payload = Vec::new();
    payload.extend_from_slice(&Value::fixnum(17).bits().to_ne_bytes());
    payload.extend_from_slice(&Value::NIL.bits().to_ne_bytes());
    assert_eq!(payload.len(), std::mem::size_of::<ConsCell>());
    mmap_image::write_image(
        &path,
        &[ImageSection {
            kind: DumpSectionKind::HeapImage,
            flags: 0,
            bytes: &payload,
        }],
    )
    .unwrap();
    let retained: &'static mmap_image::LoadedMmapImage =
        Box::leak(Box::new(mmap_image::load_image(&path).unwrap()));
    let cell = retained
        .section(DumpSectionKind::HeapImage)
        .unwrap()
        .as_ptr()
        .cast::<ConsCell>();
    assert_eq!(cell as usize % std::mem::align_of::<ConsCell>(), 0);
    let (mut context, frame) = ready();
    // SAFETY: the real writable mmap is deliberately process-retained; its
    // aligned single cell contains the two valid scalar words encoded above.
    // No callback or collection separates registration and pointer creation.
    unsafe {
        context
            .tagged_heap_mut()
            .register_mapped_cons_range(cell.cast_mut(), 1);
    }
    // SAFETY: the same aligned, initialized process-retained cell is live;
    // neither scalar word points to another allocation and no GC intervenes.
    let cons = unsafe { Value::from_cons_ptr(cell) };
    // A valid mapped owner need not be THIS Context's retained image. The
    // first scope refuses that unknown mapping rather than using dump lo/hi.
    let no_authority = context.begin_static_rows_observation(frame).unwrap();
    let (_, reads) = capture(|| assert_eq!(cons.cons_car(), Value::fixnum(17)));
    assert!(matches!(
        context.finish_static_rows_observation(no_authority, reads.as_ref().unwrap()),
        Err(RowsObservationRefusal::UnknownOwner)
    ));
    // SAFETY: `retained` is a real process-retained mmap with a 'static lease
    // that outlives this Context and is never remapped while installed.
    unsafe {
        neovm_eval::test_support::authority::install_pdump_image_for_test(
            &mut context,
            Some(retained),
        )
    };
    let epoch = context.begin_static_rows_observation(frame).unwrap();
    let (_, reads) = capture(|| assert_eq!(cons.cons_car(), Value::fixnum(17)));
    let metadata = context
        .finish_static_rows_observation(epoch, reads.as_ref().unwrap())
        .unwrap();
    assert!(metadata.current_and_observe(&context));
    // No mutation/GC/header replacement occurs. This tests a real valid
    // mapped dependency, not geometry or arbitrary caller-made pointer bits.
}
