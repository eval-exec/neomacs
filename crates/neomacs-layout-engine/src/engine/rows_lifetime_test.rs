//! Actual automatic producer tests, not hand-made coverage/body seals.
//! The positive assertion must really pass before
//! this scope can be described as eligible; no cache admission is exercised.
use super::*;
use neovm_core::emacs_core::value::Value;
use std::num::NonZeroUsize;

fn rows() -> WindowLayoutQueryScope {
    WindowLayoutQueryScope::Rows {
        start: LispCharPos1::ONE,
        count: NonZeroUsize::new(4).unwrap(),
    }
}

fn fixture(warm: bool) -> (Context, FrameId, WindowId, BufferId) {
    let mut context = Context::new();
    let source = context.buffer_manager().current_buffer().unwrap().id();
    context
        .buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert(&"short numeric row\n".repeat(12));
    let frame = context
        .frame_manager_mut()
        .create_frame("rows-owned-domain", 400, 160, source);
    context
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("x")));
    let window = context.frame_manager().get(frame).unwrap().selected_window;
    // This deliberately small graphical numeric fixture has no host/metrics,
    // chrome, glyphless or char-table dependency. It represents no TTY claim.
    context
        .eval_str(
            r#"(setq mode-line-format nil header-line-format nil tab-line-format nil
          fontification-functions (list 'ignore) glyphless-char-display nil
          buffer-display-table nil standard-display-table nil char-width-table nil)
          (put-text-property (point-min) (point-max) 'fontified t)
          (goto-char 1) (set-window-start nil 1 t)"#,
        )
        .unwrap();
    if warm {
        context.sync_runtime_faces_for_frame(frame);
    }
    (context, frame, window, source)
}

fn query(
    context: &mut Context,
    engine: &mut LayoutEngine,
    frame: FrameId,
    window: WindowId,
    scope: WindowLayoutQueryScope,
) -> (WindowLayoutQuery, Vec<Report>) {
    let mut reports = Vec::new();
    let mut sink = |_: &mut Context, report| reports.push(report);
    let query = engine
        .query_window_layout_observed(context, frame, window, scope, Some(&mut sink))
        .expect("the actual canonical numeric query converges");
    drop(sink);
    (query, reports)
}

#[test]
fn rows_lifetime_automatic_numeric_gen1_positive_has_real_geometry_and_drops_lease() {
    let (mut context, frame, window, source) = fixture(true);
    // Assert the real constructor; never flip a synthetic certificate bit.
    context
        .begin_static_rows_observation(frame)
        .expect("warm graphical GEN1 fixture must be admitted");
    let mut engine = LayoutEngine::new_without_font_metrics();
    let (geometry, reports) = query(&mut context, &mut engine, frame, window, rows());
    let numeric = geometry.geometry().unwrap();
    assert!(!numeric.rows.is_empty());
    assert!(numeric.point_count() > 0);
    assert_eq!(
        reports,
        vec![Report::Certified {
            rows: numeric.rows.len(),
            points: numeric.point_count()
        }],
        "this is the actual canonical capture and actual Stage1 NoDemand witness"
    );
    // A retained Ref would panic here; the diagnostic proof must be consumed.
    context
        .buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert("after observation");
}

#[test]
fn rows_lifetime_automatic_tty_and_cold_face_scopes_refuse() {
    let (mut context, frame, window, _) = fixture(true);
    context
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(None);
    let mut engine = LayoutEngine::new_without_font_metrics();
    let (_, reports) = query(&mut context, &mut engine, frame, window, rows());
    assert_eq!(
        reports,
        vec![Report::Refused(Refusal::Core(
            RowsObservationRefusal::TtyOrUnknownFrame
        ))]
    );
    let (mut context, frame, window, _) = fixture(false);
    let mut engine = LayoutEngine::new_without_font_metrics();
    let (_, reports) = query(&mut context, &mut engine, frame, window, rows());
    assert_eq!(
        reports,
        vec![Report::Refused(Refusal::Core(
            RowsObservationRefusal::ColdFaces
        ))]
    );
}

#[test]
fn rows_lifetime_font_metrics_service_refuses_before_capture() {
    let (context, frame, window, _) = fixture(true);
    let engine = crate::font::metrics::test_catalog::layout_engine();
    assert!(matches!(
        Producer::begin(&engine, &context, frame, window, rows()),
        Err(Refusal::FontServices)
    ));
}

#[test]
fn rows_lifetime_actual_fontifier_callback_cannot_mint_no_demand() {
    let (mut context, frame, window, _) = fixture(true);
    context
        .eval_str(
            r#"(setq rows-lifetime-hook-calls 0 fontification-functions
       (list (lambda (pos) (setq rows-lifetime-hook-calls (1+ rows-lifetime-hook-calls)))))
       (put-text-property 1 2 'fontified nil)"#,
        )
        .unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let (_, reports) = query(&mut context, &mut engine, frame, window, rows());
    assert!(
        context
            .eval_str("rows-lifetime-hook-calls")
            .unwrap()
            .as_int()
            .unwrap()
            > 0
    );
    assert_eq!(reports, vec![Report::Refused(Refusal::Fontification)]);
}

#[test]
fn rows_lifetime_actual_fontifier_gc_runs_before_any_source_read_lease() {
    let (mut context, frame, window, source) = fixture(true);
    context
        .eval_str(
            r#"(setq rows-lifetime-hook-calls 0 rows-lifetime-hook-gc-done nil
       fontification-functions
       (list (lambda (pos)
         (setq rows-lifetime-hook-calls (1+ rows-lifetime-hook-calls))
         (garbage-collect)
         (setq rows-lifetime-hook-gc-done t))))
       (put-text-property 1 2 'fontified nil)"#,
        )
        .unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let (_, reports) = query(&mut context, &mut engine, frame, window, rows());
    assert!(
        context
            .eval_str("rows-lifetime-hook-calls")
            .unwrap()
            .as_int()
            .unwrap()
            > 0
    );
    assert!(
        context
            .eval_str("rows-lifetime-hook-gc-done")
            .unwrap()
            .is_truthy(),
        "the real built-in garbage-collect must return before hook completion"
    );
    assert!(matches!(reports.as_slice(), [Report::Refused(_)]));
    context
        .buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert("after hook GC");
    // No BufferText Ref was acquired before this real Lisp hook collected.
}

#[test]
fn rows_lifetime_actual_when_display_refuses_dynamic_producer() {
    let (mut context, frame, window, _) = fixture(true);
    context
        .eval_str(
            r#"(setq rows-lifetime-when-count 0)
      (put-text-property 1 2 'display
        '(when (progn (setq rows-lifetime-when-count (1+ rows-lifetime-when-count)) t) . "X"))"#,
        )
        .unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let (_, reports) = query(&mut context, &mut engine, frame, window, rows());
    assert!(
        context
            .eval_str("rows-lifetime-when-count")
            .unwrap()
            .as_int()
            .unwrap()
            > 0
    );
    assert_eq!(
        reports,
        vec![Report::Refused(Refusal::DynamicOrRetry)],
        "the actual WHEN must close the static producer seal before origin matching"
    );
}

#[test]
fn rows_lifetime_ordinary_query_purposes_do_not_construct_snapshot_or_report() {
    for scope in [
        WindowLayoutQueryScope::Viewport,
        WindowLayoutQueryScope::Position {
            target: LispCharPos1::ONE,
        },
    ] {
        let (mut context, frame, window, _) = fixture(true);
        let mut engine = LayoutEngine::new_without_font_metrics();
        let (_, reports) = query(&mut context, &mut engine, frame, window, scope);
        assert!(reports.is_empty());
    }
}

#[test]
fn rows_lifetime_existing_cache_hit_gets_no_new_certificate_or_admission_path() {
    let (mut context, frame, window, _) = fixture(true);
    let mut engine = LayoutEngine::new_without_font_metrics();
    let (_, first) = query(&mut context, &mut engine, frame, window, rows());
    assert!(matches!(first.as_slice(), [Report::Certified { .. }]));
    let (_, second) = query(&mut context, &mut engine, frame, window, rows());
    assert!(
        second.is_empty(),
        "only a real canonical Rows miss snapshots owners"
    );
}
