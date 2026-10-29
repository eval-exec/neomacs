//! Automatic ledger routing through the real canonical coordinator.
//! The explicit sink exists only in cfg(test): no TLS/global/production field.
use super::*;
use neovm_core::buffer::LispCharPos1;
use std::num::NonZeroUsize;

fn fixture(text: &str) -> (Context, FrameId, WindowId, BufferId) {
    let mut ctx = Context::new();
    let source = ctx.buffer_manager().current_buffer().unwrap().id();
    ctx.buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert(text);
    let frame = ctx
        .frame_manager_mut()
        .create_frame("rows-demand", 400, 160, source);
    ctx.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(None);
    let window = ctx.frame_manager().get(frame).unwrap().selected_window;
    ctx.eval_str(r#"(setq mode-line-format nil header-line-format nil tab-line-format nil
                         rows-demand-calls nil
                         fontification-functions
                         (list (lambda (pos) (setq rows-demand-calls (cons pos rows-demand-calls)))))
                   (put-text-property (point-min) (point-max) 'fontified t)
                   (goto-char 1) (set-window-start nil 1 t)"#).unwrap();
    (ctx, frame, window, source)
}

fn rows() -> WindowLayoutQueryScope {
    WindowLayoutQueryScope::Rows {
        start: LispCharPos1::ONE,
        count: NonZeroUsize::new(4).unwrap(),
    }
}

fn canonical(
    ctx: &mut Context,
    frame: FrameId,
    window: WindowId,
    scope: WindowLayoutQueryScope,
) -> (
    neovm_core::window::WindowLayoutQuery,
    Vec<xdisp::RedisplayFontificationWitness>,
    bool,
) {
    let mut engine = LayoutEngine::new_without_font_metrics();
    // Match the actual public query's canonical-miss setup. Skip its cache
    // lookup deliberately: these tests target the producer, never admission.
    engine.query_body_cache_state = crate::engine::QueryBodyCacheState::Produced;
    engine.query_restart_rows.clear();
    engine.query_row_coverage = crate::buffer_source::render_attempt::QueryRowCoverage::Complete;
    engine.query_target_prefix_policy = false;
    engine.mini_preparation_height = None;
    let mut observations = Vec::new();
    let mut sink = |witness| observations.push(witness);
    let (query, _collections) = neovm_core::tagged::collection_reads::capture_normalized(
        ctx,
        |ctx| {
            ctx.sync_runtime_faces_for_frame(frame);
        },
        |ctx| {
            engine.layout_frame_rust_for_purpose_inner(
                ctx,
                frame,
                LayoutPurpose::SynchronousQuery {
                    window_id: window,
                    scope,
                },
                None,
                Some(&mut sink),
            )
        },
    );
    drop(sink);
    (
        query.expect("canonical query converges"),
        observations,
        engine.query_body_cache_state == crate::engine::QueryBodyCacheState::Produced,
    )
}

#[test]
fn rows_automatic_prepass_produces_positive_typed_coverage_without_callbacks() {
    let (mut ctx, frame, window, source) = fixture(&"λ short row\n".repeat(12));
    let (query, mut observed, _) = canonical(&mut ctx, frame, window, rows());
    assert_eq!(
        observed.len(),
        1,
        "exactly one accepted-producer observation"
    );
    let xdisp::RedisplayFontificationWitness::NoDemand(coverage) = observed.pop().unwrap() else {
        panic!("real automatic prepass should cover the short Rows footprint");
    };
    let eob = ctx
        .buffer_manager()
        .get(source)
        .unwrap()
        .point_max_char_pos();
    for point in query.geometry().unwrap().iter_points() {
        let position = point.buffer_pos.to_char_pos();
        if position != eob {
            assert!(coverage.covers(source, CharRange::from_start_len(position, CharLen::new(1))));
        }
    }
    assert!(ctx.eval_str("rows-demand-calls").unwrap().is_nil());
}

#[test]
fn rows_automatic_sparse_inspection_refuses_unobserved_truthy_folded_tail() {
    let hidden = "hidden\n".repeat(2000);
    let tail = 7 + hidden.chars().count();
    let (mut ctx, frame, window, _) = fixture(&format!("first\n{hidden}TAIL\n"));
    ctx.eval_str(&format!(
        "(setq buffer-invisibility-spec t) (put-text-property 7 {tail} 'invisible t)"
    ))
    .unwrap();
    let (query, observed, _) = canonical(&mut ctx, frame, window, rows());
    assert!(
        query
            .geometry()
            .unwrap()
            .iter_points()
            .any(|p| p.buffer_pos.as_i64() >= tail as i64),
        "real Rows must reach the post-fold source"
    );
    assert!(matches!(
        observed.as_slice(),
        [xdisp::RedisplayFontificationWitness::Unknown]
    ));
    assert!(ctx.eval_str("rows-demand-calls").unwrap().is_nil());
}

#[test]
fn rows_automatic_sparse_callback_is_recorded_when_it_does_not_trigger_retry() {
    let hidden = "hidden\n".repeat(2000);
    let tail = 7 + hidden.chars().count();
    let (mut ctx, frame, window, _) = fixture(&format!("first\n{hidden}TAIL\n"));
    ctx.eval_str(&format!(
        "(setq buffer-invisibility-spec t) (put-text-property 7 {tail} 'invisible t) \
        (put-text-property {tail} (point-max) 'fontified nil)"
    ))
    .unwrap();
    // The prepass is already fontified. Only the actual sparse ensure enters
    // this recording-only hook; no source/geometry mutation requests a retry.
    let (_, observed, _) = canonical(&mut ctx, frame, window, rows());
    assert!(
        matches!(
            observed.as_slice(),
            [xdisp::RedisplayFontificationWitness::CallbacksRan]
        ),
        "sparse ensure must feed the same actual ledger: {observed:?}"
    );
    let first = ctx
        .eval_str("(car rows-demand-calls)")
        .unwrap()
        .as_int()
        .unwrap();
    assert!(
        first >= tail as i64,
        "callback must be outside the contiguous prepass"
    );
}

#[test]
fn rows_automatic_fontifier_mutation_retry_aborts_positive_observation() {
    let (mut ctx, frame, window, _) = fixture(&"short row\n".repeat(12));
    ctx.eval_str(
        r#"(put-text-property 1 2 'fontified nil)
       (setq fontification-functions
         (list (lambda (pos)
           (setq rows-demand-calls (cons pos rows-demand-calls))
           (put-text-property (point-min) (point-max) 'fontified t))))"#,
    )
    .unwrap();
    let (_, observed, _) = canonical(&mut ctx, frame, window, rows());
    assert!(
        matches!(
            observed.as_slice(),
            [xdisp::RedisplayFontificationWitness::Aborted]
        ),
        "completed retry cannot certify spans from its earlier attempt: {observed:?}"
    );
    assert_eq!(
        ctx.eval_str("(length rows-demand-calls)").unwrap().as_int(),
        Some(1)
    );
}

#[test]
fn rows_automatic_overlay_refuses_and_other_query_purposes_stay_inactive() {
    let (mut ctx, frame, window, _) = fixture(&"short row\n".repeat(12));
    ctx.eval_str("(make-overlay 1 2)").unwrap();
    let (_, observed, _) = canonical(&mut ctx, frame, window, rows());
    assert!(matches!(
        observed.as_slice(),
        [xdisp::RedisplayFontificationWitness::Unknown]
    ));
    for scope in [
        WindowLayoutQueryScope::Viewport,
        WindowLayoutQueryScope::Position {
            target: LispCharPos1::ONE,
        },
    ] {
        let (_, observed, _) = canonical(&mut ctx, frame, window, scope);
        assert!(observed.is_empty(), "only synchronous Rows observes demand");
    }
}

#[test]
fn rows_automatic_memory_full_and_eob_only_queries_never_publish_no_demand() {
    let (mut ctx, frame, window, source) = fixture(&"short row\n".repeat(12));
    ctx.eval_str("(setq memory-full t)").unwrap();
    let (_, observed, _) = canonical(&mut ctx, frame, window, rows());
    assert!(matches!(
        observed.as_slice(),
        [xdisp::RedisplayFontificationWitness::Unknown]
    ));
    ctx.eval_str("(setq memory-full nil)").unwrap();
    let eob = ctx
        .buffer_manager()
        .get(source)
        .unwrap()
        .point_max_lisp_char_pos();
    let (_, observed, _) = canonical(
        &mut ctx,
        frame,
        window,
        WindowLayoutQueryScope::Rows {
            start: eob,
            count: NonZeroUsize::new(1).unwrap(),
        },
    );
    assert!(matches!(
        observed.as_slice(),
        [xdisp::RedisplayFontificationWitness::Unknown]
    ));
}
