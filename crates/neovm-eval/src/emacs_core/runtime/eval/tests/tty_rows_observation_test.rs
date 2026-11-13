//! Core-only partial TTY receipts. Actual Layout callback/coverage seals remain
//! separate; these tests never manufacture a reusable geometry certificate.
use super::*;
use crate::emacs_core::display_host::{DisplayHost, InertRowsHostAuthority};
use crate::emacs_core::terminal::pure::{self, TerminalRuntimeConfig};
use crate::face::realization::BasicFace;
use neovm_value::tagged::collection_reads::{capture, capture_normalized};
use neovm_value::value::Value;

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        pure::reset_terminal_host();
        pure::reset_terminal_runtime();
    }
}

fn tty_config() -> TerminalRuntimeConfig {
    TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_owned()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    )
    .into()
}

fn ready() -> (Context, FrameId, RestoreTerminal) {
    let mut context = Context::new();
    let restore = RestoreTerminal;
    context.gc_stress = false;
    // These receipts require GEN1, independently of the test runner's policy.
    // SAFETY: serial fixture setup; no young-generation objects or
    // remembered-set state exist under the new mode.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            &mut context.tagged_heap,
            true,
        )
    };
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.gc_collect_exact();
    pure::configure_terminal_runtime(tty_config());
    let source = context.buffer_manager().current_buffer().unwrap().id();
    let frame = context
        .frames
        .create_frame("real-termcap-rows", 120, 40, source);
    assert!(context.frame_is_tty_face_target(frame));
    assert!(
        crate::face::realization::enabled(),
        "this positive family requires the default lifecycle"
    );
    context.sync_runtime_faces_for_frame(frame);
    context.setup_thread_locals();
    assert!(
        context.tagged_heap.tagged_heap_generational_enabled(),
        "GEN1 test policy required"
    );
    assert!(
        context
            .frames
            .get(frame)
            .unwrap()
            .face_realization
            .basic(BasicFace::Default)
            .is_some()
    );
    (context, frame, restore)
}

// Every state has the same inherited absent image/font services. Only the
// non-geometry popup/frame methods below are overridden.
struct InertHost(u8);
impl DisplayHost for InertHost {
    fn realize_gui_frame(
        &mut self,
        _: crate::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        panic!("metadata admission must not invoke frame services")
    }
    fn resize_gui_frame(
        &mut self,
        _: crate::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        panic!("metadata admission must not invoke frame services")
    }
}

struct UnknownHost;
impl DisplayHost for UnknownHost {
    fn realize_gui_frame(
        &mut self,
        _: crate::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(
        &mut self,
        _: crate::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        Ok(())
    }
    fn image_catalog(&self) -> Option<&dyn crate::emacs_core::image_catalog::ImageCatalog> {
        panic!("an unknown host must not be probed to establish authority")
    }
    fn image_catalog_shared(
        &self,
    ) -> Option<std::rc::Rc<dyn crate::emacs_core::image_catalog::ImageCatalog>> {
        panic!("an unknown host must not be probed to establish authority")
    }
}

fn inert_authority() -> InertRowsHostAuthority {
    // SAFETY: InertHost inherits constant absent image/font accessors in all
    // states. Its u8 is never consulted by those services; none calls Lisp,
    // reenters, collects, destroys owners or exposes external geometry.
    // The popup/frame methods are outside this Core-only closed query scope.
    unsafe { InertRowsHostAuthority::for_type::<InertHost>() }
}

#[test]
fn tty_rows_observation_real_warm_termcap_accepts_owned_normalization_reads() {
    let (mut context, frame, _restore) = ready();
    let cons = Value::cons(Value::fixnum(7), Value::NIL);
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    let (_, reads) = capture_normalized(
        &mut context,
        |context| {
            assert!(
                !context.sync_runtime_faces_for_frame(frame),
                "actual normalization was already warm"
            );
            assert_eq!(cons.cons_car(), Value::fixnum(7));
        },
        |_| (),
    );
    let receipt = context
        .finish_static_rows_observation(epoch, reads.as_ref().unwrap())
        .unwrap();
    assert!(receipt.current_and_observe(&context));
    cons.set_car(Value::fixnum(8));
    assert!(!receipt.current_and_observe(&context));
}

#[test]
fn tty_rows_observation_legacy_constructor_keeps_real_tty_refusal() {
    let (mut context, frame, _restore) = ready();
    assert!(matches!(
        context.begin_static_rows_observation(frame),
        Err(RowsObservationRefusal::TtyOrUnknownFrame)
    ));
    context.display_host = Some(Box::new(InertHost(0)));
    assert!(matches!(
        context.begin_static_rows_observation(frame),
        Err(RowsObservationRefusal::DisplayHostInstalled)
    ));
}

#[test]
fn tty_rows_observation_approved_actual_type_needs_no_host_virtual_probe() {
    let (mut context, frame, _restore) = ready();
    context.display_host = Some(Box::new(InertHost(0)));
    let authority = inert_authority();
    let epoch = context
        .begin_tty_rows_observation(frame, Some(&authority))
        .unwrap();
    assert!(context.static_rows_observation_current(&epoch).is_ok());
    // The sealed default must identify the implementor, not Box<dyn DisplayHost>.
    assert!(authority.accepts(context.display_host.as_deref().unwrap()));
}

#[test]
fn tty_rows_observation_unknown_host_refuses_without_service_entry() {
    let (mut context, frame, _restore) = ready();
    context.display_host = Some(Box::new(UnknownHost));
    assert!(matches!(
        context.begin_tty_rows_observation(frame, None),
        Err(RowsObservationRefusal::DisplayHostInstalled)
    ));
}

#[test]
fn tty_rows_observation_forwarded_other_type_authority_refuses() {
    let (mut context, frame, _restore) = ready();
    context.display_host = Some(Box::new(UnknownHost));
    let authority = inert_authority();
    assert!(matches!(
        context.begin_tty_rows_observation(frame, Some(&authority)),
        Err(RowsObservationRefusal::DisplayHostInstalled)
    ));
}

#[test]
fn tty_rows_observation_replaced_unknown_host_refuses_before_saved_keys() {
    let (mut context, frame, _restore) = ready();
    context.display_host = Some(Box::new(InertHost(0)));
    let epoch = context
        .begin_tty_rows_observation(frame, Some(&inert_authority()))
        .unwrap();
    context.display_host = Some(Box::new(UnknownHost));
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::HostTypeChanged)
    );
}

#[test]
fn tty_rows_observation_removed_or_newly_installed_host_refuses() {
    let (mut context, frame, _restore) = ready();
    let absent = context.begin_tty_rows_observation(frame, None).unwrap();
    context.display_host = Some(Box::new(InertHost(0)));
    assert_eq!(
        context.static_rows_observation_current(&absent),
        Err(RowsObservationRefusal::HostTypeChanged)
    );
    let installed = context
        .begin_tty_rows_observation(frame, Some(&inert_authority()))
        .unwrap();
    context.display_host = None;
    assert_eq!(
        context.static_rows_observation_current(&installed),
        Err(RowsObservationRefusal::HostTypeChanged)
    );
}

#[test]
fn tty_rows_observation_same_approved_type_all_states_contract_is_explicit() {
    let (mut context, frame, _restore) = ready();
    context.display_host = Some(Box::new(InertHost(0)));
    let epoch = context
        .begin_tty_rows_observation(frame, Some(&inert_authority()))
        .unwrap();
    // Replacement is allowed here only by the audited type's ALL-states
    // service contract. No pointer/vtable/allocator-reuse assumption is used.
    let replacement = InertHost(u8::MAX);
    assert_eq!(replacement.0, u8::MAX);
    context.display_host = Some(Box::new(replacement));
    assert!(context.static_rows_observation_current(&epoch).is_ok());
}

#[test]
fn tty_rows_observation_realization_retirement_rejects_same_face_counter() {
    let (mut context, frame, _restore) = ready();
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    let counter = context.face_change_count;
    let cache = &mut context.frames.get_mut(frame).unwrap().face_realization;
    let table = cache.table(counter).unwrap().clone();
    let (source, paint) = cache.basic(BasicFace::Default).unwrap().clone();
    cache.invalidate();
    cache.prepare(counter, table);
    cache.insert_basic(BasicFace::Default, source, paint);
    assert_eq!(context.face_change_count, counter);
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::FaceRealizationChanged)
    );
}

#[test]
fn tty_rows_observation_table_replacement_rejects_same_cache_epoch() {
    let (mut context, frame, _restore) = ready();
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    let counter = context.face_change_count;
    let cache = &mut context.frames.get_mut(frame).unwrap().face_realization;
    let realization = cache.realization_epoch().unwrap();
    cache.replace_definitions(crate::face::FaceTable::new());
    assert!(cache.retains_epoch(&realization));
    assert_eq!(context.face_change_count, counter);
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::FaceRealizationChanged)
    );
}

#[test]
fn tty_rows_observation_definition_stamp_stays_mandatory_when_epoch_survives() {
    let (mut context, frame, _restore) = ready();
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    let realization = context
        .frames
        .get(frame)
        .unwrap()
        .face_realization
        .realization_epoch()
        .unwrap();
    crate::emacs_core::xfaces::realization::record_face_change_for(
        &mut context,
        crate::face::realization::FaceChangeScope::DefinitionsOnly(&[frame]),
        Some("default"),
    );
    assert!(
        context
            .frames
            .get(frame)
            .unwrap()
            .face_realization
            .retains_epoch(&realization)
    );
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::RuntimeChanged)
    );
}

#[test]
fn tty_rows_observation_cold_source_and_missing_basic_refuse() {
    let (mut context, frame, _restore) = ready();
    let counter = context.face_change_count;
    let cache = &mut context.frames.get_mut(frame).unwrap().face_realization;
    let table = cache.table(counter).unwrap().clone();
    cache.prepare(counter, table); // Real native lifetime, but no basic roles.
    assert!(matches!(
        context.begin_tty_rows_observation(frame, None),
        Err(RowsObservationRefusal::ColdFaces)
    ));
    context
        .frames
        .get_mut(frame)
        .unwrap()
        .face_realization
        .invalidate();
    assert!(matches!(
        context.begin_tty_rows_observation(frame, None),
        Err(RowsObservationRefusal::ColdFaces)
    ));
}

#[test]
fn tty_rows_observation_initial_and_window_system_are_not_termcap_authority() {
    let (mut context, frame, _restore) = ready();
    pure::reset_terminal_runtime();
    assert!(matches!(
        context.begin_tty_rows_observation(frame, None),
        Err(RowsObservationRefusal::TtyOrUnknownFrame)
    ));
    pure::configure_terminal_runtime(tty_config());
    context
        .frames
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("x")));
    assert!(matches!(
        context.begin_tty_rows_observation(frame, None),
        Err(RowsObservationRefusal::TtyOrUnknownFrame)
    ));
}

#[test]
fn tty_rows_observation_terminal_output_or_identity_change_refuses() {
    let (mut context, frame, _restore) = ready();
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    pure::reset_terminal_runtime();
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::TerminalChanged)
    );
    pure::configure_terminal_runtime(tty_config());
    pure::ensure_terminal_runtime_owner(27, "other-real-tty", tty_config());
    context.frames.get_mut(frame).unwrap().terminal_id = 27;
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::TerminalChanged)
    );
}

#[test]
fn tty_rows_observation_completed_gc_refuses_before_dead_key_classification() {
    let (mut context, frame, _restore) = ready();
    let cons = Value::cons(Value::fixnum(7), Value::NIL);
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    let (_, reads) = capture(|| cons.cons_car());
    context.gc_collect_exact();
    assert!(matches!(
        context.finish_static_rows_observation(epoch, reads.as_ref().unwrap()),
        Err(RowsObservationRefusal::CollectionChanged)
    ));
    // The old key is never projected again; no source lease exists here.
}

#[test]
fn tty_rows_observation_partial_gc_and_driver_obligations_remain_mandatory() {
    let (mut context, frame, _restore) = ready();
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    context.gc_driver_active = true;
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::DriverActive)
    );
    context.gc_driver_active = false;
    context.tagged_heap.begin_collection();
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::CollectorBusy)
    );
    context.gc_collect_exact();
}

#[test]
fn tty_rows_observation_function_change_keeps_full_freshness_gate_separate() {
    let (mut context, frame, _restore) = ready();
    let source = context.buffers.current_buffer_id().unwrap();
    let window = context.frames.get(frame).unwrap().selected_window;
    let before = context
        .window_display_snapshot_freshness(frame, window, source)
        .unwrap();
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    context.eval_str("(fset 'tty-color-desc (lambda (&rest ignored) (error \"warm metadata must not call the new definition\")))").unwrap();
    let after = context
        .window_display_snapshot_freshness(frame, window, source)
        .unwrap();
    assert_ne!(
        before, after,
        "full function freshness must still reject a row hit"
    );
    // This receipt checks physical owner/cache life only. It neither invokes
    // the new function nor substitutes for the unchanged full freshness gate.
    assert!(context.static_rows_observation_current(&epoch).is_ok());
}

#[test]
fn tty_rows_observation_palette_storage_is_part_of_native_table_identity() {
    let (mut context, frame, _restore) = ready();
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    let counter = context.face_change_count;
    let cache = &mut context.frames.get_mut(frame).unwrap().face_realization;
    let mut table = cache.table(counter).unwrap().clone();
    let realization = cache.realization_epoch().unwrap();
    table.set_tty_palette(neomacs_display_protocol::TtyPalette::default());
    cache.replace_definitions(table);
    assert!(cache.retains_epoch(&realization));
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::FaceRealizationChanged)
    );
}

#[test]
fn tty_rows_observation_same_palette_faces_clone_full_prepare_retires_epoch() {
    let (mut context, frame, _restore) = ready();
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    let counter = context.face_change_count;
    let cache = &mut context.frames.get_mut(frame).unwrap().face_realization;
    let realization = cache.realization_epoch().unwrap();
    let mut replacement = cache.table(counter).unwrap().clone();
    let palette = replacement.rows_snapshot_identity();
    let (source, paint) = cache.basic(BasicFace::Default).unwrap().clone();
    replacement.define(
        "rows-cloned-definitions",
        crate::face::Face::new("rows-cloned-definitions"),
    );
    cache.prepare(counter, replacement);
    cache.insert_basic(BasicFace::Default, source, paint);
    assert!(
        palette.matches(cache.table(counter).unwrap()),
        "the cloned table deliberately shares its palette"
    );
    assert!(
        !cache.retains_epoch(&realization),
        "full prepare retires the actual native lifetime"
    );
    assert_eq!(context.face_change_count, counter);
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::FaceRealizationChanged)
    );
}

#[test]
fn tty_rows_observation_recorded_incremental_refresh_rejects_by_existing_count() {
    let (mut context, frame, _restore) = ready();
    let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
    let counter = context.face_change_count;
    let cache = &context.frames.get(frame).unwrap().face_realization;
    let realization = cache.realization_epoch().unwrap();
    let palette = cache.table(counter).unwrap().rows_snapshot_identity();
    crate::emacs_core::xfaces::realization::record_face_change_for(
        &mut context,
        crate::face::realization::FaceChangeScope::DefinitionsOnly(&[frame]),
        Some("default"),
    );
    assert!(
        context.sync_runtime_faces_for_frame(frame),
        "the real recorded dirty source must be installed"
    );
    let cache = &context.frames.get(frame).unwrap().face_realization;
    assert!(cache.retains_epoch(&realization));
    assert!(
        palette.matches(cache.table(context.face_change_count).unwrap()),
        "the actual incremental builder preserves its palette"
    );
    assert_eq!(context.face_change_count, counter + 1);
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::RuntimeChanged)
    );
}

#[test]
fn tty_rows_observation_reentrant_global_merge_rejects_by_palette_at_equal_count_epoch() {
    use crate::emacs_core::{composite, font, xfaces};
    use std::cell::RefCell;
    use std::rc::Rc;

    let (mut context, frame, _restore) = ready();
    // A supported non-default functional height is inherited by default.
    // Validation happens while unarmed; the outer real global merge later
    // invokes the same Lisp function inside its recorded native lifecycle.
    context.eval_str("(progn (setq rows-palette-probe-armed nil) (fset 'rows-palette-probe-height (lambda (height) (if rows-palette-probe-armed (font-shape-gstring rows-palette-probe-gstring nil)) height)))").unwrap();
    xfaces::builtin_internal_make_lisp_face(
        &mut context,
        vec![Value::symbol("rows-palette-probe-inherited")],
    )
    .unwrap();
    xfaces::builtin_internal_set_lisp_face_attribute(
        &mut context,
        vec![
            Value::symbol("rows-palette-probe-inherited"),
            Value::keyword(":height"),
            Value::symbol("rows-palette-probe-height"),
            Value::make_frame(frame.0),
        ],
    )
    .unwrap();
    xfaces::builtin_internal_set_lisp_face_attribute(
        &mut context,
        vec![
            Value::symbol("default"),
            Value::keyword(":inherit"),
            Value::symbol("rows-palette-probe-inherited"),
            Value::T,
        ],
    )
    .unwrap();
    // Keep the later global-merge parameter-publication branch out of this
    // palette-specific fixture through supported default setters, so another
    // recorded frame-parameter change cannot obscure the equal-count edge.
    for attribute in [":foreground", ":background"] {
        xfaces::builtin_internal_set_lisp_face_attribute(
            &mut context,
            vec![
                Value::symbol("default"),
                Value::keyword(attribute),
                Value::symbol("unspecified"),
                Value::T,
            ],
        )
        .unwrap();
    }
    context.sync_runtime_faces_for_frame(frame);
    let opened = font::build_font_object(&crate::face::Face::new("default"));
    let gstring = composite::composition_get_gstring(
        &mut context,
        vec![
            Value::fixnum(0),
            Value::fixnum(2),
            opened,
            Value::string("ab"),
        ],
    )
    .unwrap();
    context
        .obarray
        .set_symbol_value("rows-palette-probe-gstring", gstring);
    let captured = Rc::new(RefCell::new(None));
    let callback_capture = Rc::clone(&captured);
    context.font_shape_fn = Some(Box::new(move |context, _, _| {
        // This is the actual installed font-shaping callback reached by Lisp,
        // not a test-only producer seal or a fabricated epoch. The callback
        // warms the recorded dirty table before taking a real Core receipt.
        assert!(context.sync_runtime_faces_for_frame(frame));
        let epoch = context.begin_tty_rows_observation(frame, None).unwrap();
        let cache = &context.frames.get(frame).unwrap().face_realization;
        let realization = cache.realization_epoch().unwrap();
        let palette = cache
            .table(context.face_change_count)
            .unwrap()
            .rows_snapshot_identity();
        assert!(context.static_rows_observation_current(&epoch).is_ok());
        assert!(callback_capture.borrow().is_none());
        *callback_capture.borrow_mut() =
            Some((epoch, realization, palette, context.face_change_count));
        font::GstringShapeOutcome::NotShapable
    }));
    assert!(
        context
            .frames
            .get(frame)
            .unwrap()
            .face_realization
            .default_lface()
            .is_some(),
        "the actual immediate cached-default branch must be reachable"
    );
    context
        .obarray
        .set_symbol_value("rows-palette-probe-armed", Value::T);
    xfaces::builtin_internal_merge_in_global_face(
        &mut context,
        vec![Value::symbol("default"), Value::make_frame(frame.0)],
    )
    .unwrap();
    let (epoch, realization, palette, captured_count) = captured
        .borrow_mut()
        .take()
        .expect("the inherited Lisp height must reach the actual shaper callback");
    assert_eq!(context.face_change_count, captured_count);
    let cache = &context.frames.get(frame).unwrap().face_realization;
    assert!(cache.retains_epoch(&realization));
    assert!(
        !palette.matches(cache.table(captured_count).unwrap()),
        "the outer full builder must replace the palette after the intermediate receipt"
    );
    assert_eq!(
        context.static_rows_observation_current(&epoch),
        Err(RowsObservationRefusal::FaceRealizationChanged)
    );
    // Core-only lifetime metadata: no Layout seal, source lease or row hit is
    // manufactured, and no receipt is consumed across collection.
}
