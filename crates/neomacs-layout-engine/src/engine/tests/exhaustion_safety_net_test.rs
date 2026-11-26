//! GNU-style publish-at-exhaustion safety net and prepass recollection rows.
use super::*;

fn window_role_text(
    state: &neomacs_display_protocol::glyph_matrix::FrameDisplayState,
    window: neovm_core::window::WindowId,
    role: GlyphRowRole,
) -> String {
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .expect("window matrix entry");
    entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == role)
        .map(|row| glyphs_logical_text(&row.glyphs[GlyphArea::Text.index()]))
        .collect::<Vec<_>>()
        .join("")
}

fn assert_all_t_and_inhibit_nil(eval: &mut Context, list_var: &str) {
    let items = neovm_core::emacs_core::value::list_to_vec(&eval.eval_str(list_var).unwrap())
        .expect("recorded inhibit-redisplay list");
    assert!(!items.is_empty(), "callback must have run: {list_var}");
    assert!(
        items.iter().all(|v| v.is_t()),
        "inhibit-redisplay must be bound to t inside callback ({list_var}): {items:?}"
    );
    assert!(
        eval.eval_str("inhibit-redisplay").unwrap().is_nil(),
        "inhibit-redisplay must unwind to nil after redisplay"
    );
}

#[test]
fn class1_prepass_mutating_fontifier_recollects_and_publishes_inserted_text() {
    let (mut eval, frame, buffer_first, first_win) =
        incr_editing_frame("first sibling\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let second_buf = eval.buffer_manager_mut().create_buffer("*832-prepass*");
    eval.buffer_manager_mut()
        .get_mut(second_buf)
        .unwrap()
        .insert("hello world\n");
    let second_win = eval
        .frame_manager_mut()
        .split_window(
            frame,
            first_win,
            neovm_core::window::SplitDirection::Horizontal,
            second_buf,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split");
    let sibling_hl_fmt = eval
        .eval_str(
            r#"(progn
          (setq inhibit-redisplay nil sibling-hl-calls 0 fn-calls 0 fn-inhibit nil
                mode-line-format nil header-line-format nil tab-line-format nil)
          '((:eval (progn (setq sibling-hl-calls (1+ sibling-hl-calls)) "SIB"))))"#,
        )
        .unwrap();
    eval.buffer_manager_mut()
        .get_mut(buffer_first)
        .unwrap()
        .set_buffer_local("header-line-format", sibling_hl_fmt);
    let fontifier = eval
        .eval_str(
            r#"(list (lambda (_pos)
                       (setq fn-calls (1+ fn-calls)
                             fn-inhibit (cons inhibit-redisplay fn-inhibit))
                       (if (<= fn-calls 3)
                           (save-excursion
                             (goto-char (point-max))
                             (insert (format "[F%d]" fn-calls))))
                       (put-text-property (point-min) (point-max) 'fontified t)
                       t))"#,
        )
        .unwrap();
    eval.buffer_manager_mut()
        .get_mut(second_buf)
        .unwrap()
        .set_buffer_local("fontification-functions", fontifier);

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let outcome = engine.redisplay_frame_attempt(&mut eval, frame);
    let FrameLayoutAttempt::Prepared(state) = outcome else {
        panic!("inserting prepass fontifier must recollect and publish, not abort");
    };
    assert_eq!(
        eval.eval_str("fn-calls").unwrap().as_fixnum(),
        Some(1),
        "prepass recollection must walk immediately without re-running prepass"
    );
    assert_eq!(
        eval.eval_str("sibling-hl-calls").unwrap().as_fixnum(),
        Some(1),
        "prepass leaf-local recollection must not burn a frame retry on earlier siblings"
    );
    assert_eq!(
        engine.last_layout_stats().exhaustion_snapshot_passes,
        0,
        "converging prepass recollection must not run ExhaustionSnapshot"
    );
    let body = window_role_text(&state, second_win, GlyphRowRole::Text);
    assert!(body.contains("hello world"), "body text: {body:?}");
    assert!(
        body.contains("[F1]"),
        "body must render text inserted during prepass: {body:?}"
    );
    assert!(state.validate_spatial_projections().is_ok());
    assert_all_t_and_inhibit_nil(&mut eval, "fn-inhibit");
}

/// Declining fontifier that inserts text on every call. GNU Emacs 31.1
/// (`handle_fontified_prop`, `src/xdisp.c:4780`) updates `it->end_charpos = ZV`
/// and caps at `:fn-calls 3` / `:win-end-cached 21` on this buffer; Neomacs
/// recollects prepass inputs and publishes at retry exhaustion with `[F1]`
/// visible (side-effect call count parity is a separate follow-up).
#[test]
fn class1_prepass_declining_inserting_fontifier_publishes_at_exhaustion() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("hello world\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r#"(progn
      (setq inhibit-redisplay nil fn-calls 0 fn-inhibit nil
            mode-line-format nil header-line-format nil tab-line-format nil)
      (setq fontification-functions
            (list (lambda (_pos)
                    (setq fn-calls (1+ fn-calls)
                          fn-inhibit (cons inhibit-redisplay fn-inhibit))
                    (save-excursion
                      (goto-char (point-max))
                      (insert (format "[F%d]" fn-calls)))
                    nil))))"#,
    )
    .expect("install declining inserting fontifier");

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let outcome = engine.redisplay_frame_attempt(&mut eval, frame);
    let FrameLayoutAttempt::Prepared(state) = outcome else {
        panic!("declining inserting fontifier must publish rather than abort");
    };
    let body = window_role_text(&state, window, GlyphRowRole::Text);
    assert!(body.contains("hello world"), "body text: {body:?}");
    assert!(
        body.contains("[F1]"),
        "published snapshot must include inserted fontifier text: {body:?}"
    );
    assert!(state.validate_spatial_projections().is_ok());
    assert_all_t_and_inhibit_nil(&mut eval, "fn-inhibit");
}

/// Exact GNU Emacs 31.1 `mode-line-mutates` probe: a mode-line `:eval` that
/// inserts `"m1 "` into the displayed buffer and returns `"ML:1"`.
/// GNU draws mode lines AFTER the body (`display_mode_lines`, `xdisp.c:21443`),
/// so its attested X11 frame shows body `"seed"` WITHOUT `"m1 "` and mode-line
/// `"ML:1"` with `:ml-calls 1`. Neomacs matches GNU exactly on this single-pass
/// path without exhausting or entering `ExhaustionSnapshot`.
#[test]
fn class2_mode_line_mutating_buffer_matches_gnu_pre_chrome_body() {
    let (mut eval, frame, buffer, window) = incr_editing_frame("seed\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let ml_format = eval
        .eval_str(
            r#"(progn
          (setq inhibit-redisplay nil ml-calls 0 ml-inhibit nil
                header-line-format nil tab-line-format nil)
          '((:eval (progn
                     (setq ml-calls (1+ ml-calls)
                           ml-inhibit (cons inhibit-redisplay ml-inhibit))
                     (save-excursion
                       (goto-char (point-max))
                       (insert (format "m%d " ml-calls)))
                     (format "ML:%d" ml-calls)))))"#,
        )
        .expect("build GNU mode-line-mutates format");
    {
        let buf = eval.buffer_manager_mut().get_mut(buffer).unwrap();
        buf.set_buffer_local("mode-line-format", ml_format);
        buf.set_buffer_local("header-line-format", Value::NIL);
        buf.set_buffer_local("tab-line-format", Value::NIL);
    }

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(state) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("GNU mode-line-mutates probe must publish on the first pass");
    };
    assert_eq!(
        eval.eval_str("ml-calls").unwrap().as_fixnum(),
        Some(1),
        "must match GNU :ml-calls 1"
    );
    let stats = engine.last_layout_stats();
    assert_eq!(stats.exhaustion_snapshot_passes, 0);
    assert_eq!(stats.prepass_freshness_rechecks, 0);
    assert_eq!(stats.chrome_exhaustion_captures, 0);
    let body = window_role_text(&state, window, GlyphRowRole::Text);
    assert!(
        body.contains("seed"),
        "body must contain pre-chrome seed: {body:?}"
    );
    assert!(
        !body.contains("m1"),
        "published body must be from before chrome :eval mutation (matching GNU): {body:?}"
    );
    let mode_line = window_role_text(&state, window, GlyphRowRole::ModeLine);
    assert!(
        mode_line.contains("ML:1"),
        "published mode-line must show ML:1: {mode_line:?}"
    );
    assert!(state.validate_spatial_projections().is_ok());
    assert_all_t_and_inhibit_nil(&mut eval, "ml-inhibit");
}

/// Class 2 (`SourceInvalidated` exhaustion): a mode-line `:eval` that switches
/// the window's buffer every attempt exhausts the retry budget and publishes
/// via `ExhaustionSnapshot`, reusing the last evaluated mode-line string and
/// quiescing idle redisplay. Note: unlike GNU's single-pass `display_mode_lines`
/// (which keeps the pre-chrome body), `ExhaustionSnapshot` re-walks the window's
/// live buffer at exhaustion (documented Neomacs degraded approximation).
#[test]
fn class2_mode_line_eval_source_invalidated_publishes_last_evaluated_mode_line() {
    let _policy = neovm_core::emacs_core::eval::RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, buffer_a, window) = incr_editing_frame("seed-a\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let buffer_b = eval.buffer_manager_mut().create_buffer("*832-ml-alt*");
    eval.buffer_manager_mut()
        .get_mut(buffer_b)
        .unwrap()
        .insert("seed-b\n");
    eval.set_variable("ml-win", Value::make_window(window.0));
    eval.set_variable("ml-buf-a", Value::make_buffer(buffer_a));
    eval.set_variable("ml-buf-b", Value::make_buffer(buffer_b));
    let ml_format = eval
        .eval_str(
            r#"(progn
          (setq inhibit-redisplay nil ml-calls 0 ml-inhibit nil
                header-line-format nil tab-line-format nil)
          '((:eval (progn
                     (setq ml-calls (1+ ml-calls)
                           ml-inhibit (cons inhibit-redisplay ml-inhibit))
                     (save-excursion
                       (goto-char (point-max))
                       (insert (format "m%d " ml-calls)))
                     (set-window-buffer
                       ml-win
                       (if (eq (window-buffer ml-win) ml-buf-a) ml-buf-b ml-buf-a))
                     (format "ML:%d" ml-calls)))))"#,
        )
        .expect("build mutating + source-invalidating mode-line format");
    for buf_id in [buffer_a, buffer_b] {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.set_buffer_local("mode-line-format", ml_format);
        buf.set_buffer_local("header-line-format", Value::NIL);
        buf.set_buffer_local("tab-line-format", Value::NIL);
    }

    let requests = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let observed_requests = requests.clone();
    let published = std::rc::Rc::new(std::cell::RefCell::new(None));
    let published_cb = published.clone();
    let engine_cell = std::rc::Rc::new(std::cell::RefCell::new({
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        engine
    }));
    let engine_cb = engine_cell.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed_requests.set(observed_requests.get() + 1);
        let mut engine = engine_cb.borrow_mut();
        let outcome = engine.redisplay_frame_attempt(eval, frame);
        let FrameLayoutAttempt::Prepared(state) = outcome else {
            panic!("chrome SourceInvalidated exhaustion must publish via ExhaustionSnapshot");
        };
        *published_cb.borrow_mut() = Some(state);
    }));

    eval.gnu_mark_windows_all();
    eval.redisplay_with_force(false)
        .expect("class2 exhaustion redisplay");
    assert_eq!(requests.get(), 1);
    let calls = eval.eval_str("ml-calls").unwrap().as_fixnum().unwrap();
    assert_eq!(
        calls, 2,
        "class2 exhaustion must stop after one converging retry instead of repeating 13x: {calls}"
    );

    // Idle quiesce check (N2): subsequent idle redisplays must not spin.
    for _ in 0..3 {
        eval.redisplay_with_force(false)
            .expect("idle redisplay after class2 exhaustion");
    }
    assert_eq!(
        requests.get(),
        1,
        "class2 exhaustion publish must quiesce idle redisplay without spinning"
    );
    assert_eq!(
        eval.eval_str("ml-calls").unwrap().as_fixnum(),
        Some(calls),
        "idle redisplays must not re-run mode-line :eval"
    );

    let state = published.borrow_mut().take().expect("published state");
    let mut engine = engine_cell.borrow_mut();
    assert_eq!(
        engine.last_layout_stats().exhaustion_snapshot_passes,
        1,
        "exactly one exhaustion snapshot pass must run"
    );
    let body = window_role_text(&state, window, GlyphRowRole::Text);
    assert!(
        body.contains("seed-a") || body.contains("seed-b"),
        "published body must show a valid buffer seed: {body:?}"
    );
    let mode_line = window_role_text(&state, window, GlyphRowRole::ModeLine);
    let expected_ml = format!("ML:{calls}");
    assert!(
        mode_line.contains(&expected_ml),
        "published mode-line must show last evaluated string {expected_ml:?}, not blank: {mode_line:?}"
    );
    assert!(
        engine.retained_window_matrices.is_empty(),
        "degraded ExhaustionSnapshot pass must not poison retained matrices"
    );
    assert!(state.validate_spatial_projections().is_ok());
    assert_all_t_and_inhibit_nil(&mut eval, "ml-inhibit");

    // Next normal redisplay must rebuild cleanly with callbacks re-enabled
    // (no poisoned retained matrix or chrome_memo).
    let ok_fmt = eval.eval_str("'((:eval \"RECOVERED\"))").unwrap();
    for buf_id in [buffer_a, buffer_b] {
        eval.buffer_manager_mut()
            .get_mut(buf_id)
            .unwrap()
            .set_buffer_local("mode-line-format", ok_fmt);
    }
    let FrameLayoutAttempt::Prepared(recovered) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("normal redisplay after ExhaustionSnapshot must recover cleanly");
    };
    assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 0);
    assert_eq!(engine.last_layout_stats().prepass_freshness_rechecks, 0);
    assert_eq!(engine.last_layout_stats().chrome_exhaustion_captures, 0);
    assert_eq!(engine.last_layout_stats().reused_rows, 0);
    let recovered_ml = window_role_text(&recovered, window, GlyphRowRole::ModeLine);
    assert!(
        recovered_ml.contains("RECOVERED"),
        "recovered redisplay must evaluate fresh mode-line: {recovered_ml:?}"
    );
}

#[test]
fn class3_post_walk_display_when_mutating_inputs_publishes_at_exhaustion() {
    const PREFIX: &str = "seed\nBEFORE\n";
    let hidden = "hidden\n".repeat(10_000);
    let tail = (PREFIX.chars().count() + hidden.chars().count() + 1) as i64;
    let (mut eval, frame, _buffer, window) =
        incr_editing_frame(&format!("{PREFIX}{hidden}TAIL\n"), 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(&format!(
        r#"(progn
      (setq inhibit-redisplay nil buffer-invisibility-spec t
            when-calls 0 when-inhibit nil when-ml-calls 0
            header-line-format nil tab-line-format nil
            mode-line-format '((:eval (progn
                                        (setq when-ml-calls (1+ when-ml-calls))
                                        "WHEN-ML"))))
      (put-text-property 13 {tail} 'invisible t)
      (put-text-property {tail} (+ {tail} 4) 'display
        '(when (progn
                 (setq when-calls (1+ when-calls)
                       when-inhibit (cons inhibit-redisplay when-inhibit))
                 (save-excursion
                   (goto-char (point-max))
                   (insert (format "w%d " when-calls)))
                 t)
           "REPL")))"#
    ))
    .expect("install post-walk mutating display when form");

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let outcome = engine.redisplay_frame_attempt(&mut eval, frame);
    let FrameLayoutAttempt::Prepared(state) = outcome else {
        panic!("post-walk display-when exhaustion must publish via ExhaustionSnapshot");
    };
    let calls = eval.eval_str("when-calls").unwrap().as_fixnum().unwrap();
    assert_eq!(
        calls, 2,
        "post-walk display-when exhaustion must stop after one retry: {calls}"
    );
    let ml_calls = eval.eval_str("when-ml-calls").unwrap().as_fixnum().unwrap();
    assert_eq!(
        ml_calls, 2,
        "post-walk display-when exhaustion must not repeat innocent mode-line :eval 13x: {ml_calls}"
    );
    assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 1);
    let body = window_role_text(&state, window, GlyphRowRole::Text);
    assert!(body.contains("BEFORE"), "body text: {body:?}");
    let mode_line = window_role_text(&state, window, GlyphRowRole::ModeLine);
    assert!(mode_line.contains("WHEN-ML"), "mode-line: {mode_line:?}");
    assert!(state.validate_spatial_projections().is_ok());
    assert_all_t_and_inhibit_nil(&mut eval, "when-inhibit");
}

#[test]
fn class4_face_mutation_in_header_line_publishes_with_sealed_faces() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("face mutation body\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r#"(progn
      (setq inhibit-redisplay nil face-calls 0 face-inhibit nil
            mode-line-format nil tab-line-format nil)
      (setq header-line-format
            '((:eval (progn
                       (setq face-calls (1+ face-calls)
                             face-inhibit (cons inhibit-redisplay face-inhibit))
                       (internal-set-lisp-face-attribute
                         'bold :underline (if (= (% face-calls 2) 1) t nil) 0)
                       (format "FACE:%d" face-calls))))))"#,
    )
    .expect("install face-mutating header-line");

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let outcome = engine.redisplay_frame_attempt(&mut eval, frame);
    let FrameLayoutAttempt::Prepared(state) = outcome else {
        panic!("face-mutating header-line must publish at exhaustion with fresh sealed faces");
    };
    let calls = eval.eval_str("face-calls").unwrap().as_fixnum().unwrap();
    assert_eq!(
        calls, 2,
        "face-mutating header-line exhaustion stops after 1 retry (2 calls; this unsettled-appearance shape gives 3 in GNU 31.1): {calls}"
    );
    assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 1);
    let header = window_role_text(&state, window, GlyphRowRole::HeaderLine);
    let expected = format!("FACE:{calls}");
    assert!(
        header.contains(&expected),
        "header must retain last evaluated string {expected:?}: {header:?}"
    );
    assert!(state.validate_spatial_projections().is_ok());
    assert_all_t_and_inhibit_nil(&mut eval, "face-inhibit");
}

#[test]
fn class4_unreached_sibling_chrome_falls_back_to_inhibit_lisp() {
    let (mut eval, frame, buffer_a, window) = incr_editing_frame("first window\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let buffer_alt = eval.buffer_manager_mut().create_buffer("*832-w1-alt*");
    eval.buffer_manager_mut()
        .get_mut(buffer_alt)
        .unwrap()
        .insert("first window alt\n");
    let sibling_buffer = eval.buffer_manager_mut().create_buffer("*832-unreached*");
    eval.buffer_manager_mut()
        .get_mut(sibling_buffer)
        .unwrap()
        .insert("second window body\n");
    let sibling = eval
        .frame_manager_mut()
        .split_window(
            frame,
            window,
            neovm_core::window::SplitDirection::Horizontal,
            sibling_buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    eval.set_variable("w1-win", Value::make_window(window.0));
    eval.set_variable("w1-buf-a", Value::make_buffer(buffer_a));
    eval.set_variable("w1-buf-b", Value::make_buffer(buffer_alt));
    let w1_ml_fmt = eval
        .eval_str(
            r#"(progn
          (setq w1-calls 0 w2-evals 0
                header-line-format nil tab-line-format nil)
          '((:eval (progn
                     (setq w1-calls (1+ w1-calls))
                     (set-window-buffer
                       w1-win
                       (if (eq (window-buffer w1-win) w1-buf-a) w1-buf-b w1-buf-a))
                     (format "W1:%d" w1-calls)))))"#,
        )
        .expect("build w1 failing chrome");
    let w2_ml_fmt = eval
        .eval_str(r#"'("STATIC-" (:eval (progn (setq w2-evals (1+ w2-evals)) "DYNAMIC")))"#)
        .expect("build w2 chrome");
    for buf_id in [buffer_a, buffer_alt] {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.set_buffer_local("mode-line-format", w1_ml_fmt);
        buf.set_buffer_local("header-line-format", Value::NIL);
        buf.set_buffer_local("tab-line-format", Value::NIL);
    }
    {
        let buf = eval.buffer_manager_mut().get_mut(sibling_buffer).unwrap();
        buf.set_buffer_local("mode-line-format", w2_ml_fmt);
        buf.set_buffer_local("header-line-format", Value::NIL);
        buf.set_buffer_local("tab-line-format", Value::NIL);
    }

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let outcome = engine.redisplay_frame_attempt(&mut eval, frame);
    let FrameLayoutAttempt::Prepared(state) = outcome else {
        panic!("two-window exhaustion must publish both windows");
    };
    assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 1);
    let w1_calls = eval.eval_str("w1-calls").unwrap().as_fixnum().unwrap();
    assert_eq!(
        w1_calls, 2,
        "w1 exhaustion must stop after one retry instead of repeating 13x: {w1_calls}"
    );
    assert_eq!(
        eval.eval_str("w2-evals").unwrap().as_fixnum(),
        Some(0),
        "ExhaustionSnapshot fallback must inhibit :eval on unreached sibling"
    );
    let w1_ml = window_role_text(&state, window, GlyphRowRole::ModeLine);
    assert!(
        w1_ml.contains(&format!("W1:{w1_calls}")),
        "reached window keeps last evaluated mode-line: {w1_ml:?}"
    );
    let w2_ml = window_role_text(&state, sibling, GlyphRowRole::ModeLine);
    assert!(
        w2_ml.contains("STATIC-"),
        "unreached sibling keeps static mode-line segments: {w2_ml:?}"
    );
    assert!(
        !w2_ml.contains("DYNAMIC"),
        "unreached sibling :eval must return nil under InhibitLisp: {w2_ml:?}"
    );
    let w2_body = window_role_text(&state, sibling, GlyphRowRole::Text);
    assert!(
        w2_body.contains("second window body"),
        "unreached sibling body must be drawn: {w2_body:?}"
    );
}

#[test]
fn controls_non_local_flow_strict_snapshot_and_ordinary_perf() {
    // nextest isolates each test in a process; enable GNU mode-line flow policy
    // so the throw check exercises deferred non-local exits (mode_line_flow_test.rs).
    unsafe { std::env::set_var("NEOVM_MODE_LINE_FLOW", "1") };
    // 1. Ordinary converging redisplay (with mode-line + header-line) pays zero
    //    prepass freshness rechecks, zero chrome captures, and zero exhaustion passes.
    {
        let (mut eval, frame, buffer, window) = incr_editing_frame("plain text\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        let ml = eval.eval_str("'(\"ML: \" \"%b\")").unwrap();
        let hl = eval.eval_str("'(\"HL: \" \"%b\")").unwrap();
        {
            let buf = eval.buffer_manager_mut().get_mut(buffer).unwrap();
            buf.set_buffer_local("mode-line-format", ml);
            buf.set_buffer_local("header-line-format", hl);
            buf.set_buffer_local("tab-line-format", Value::NIL);
        }
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        let outcome = engine.redisplay_frame_attempt(&mut eval, frame);
        let FrameLayoutAttempt::Prepared(state) = outcome else {
            panic!("ordinary frame must prepare");
        };
        let stats = engine.last_layout_stats();
        assert_eq!(
            stats.exhaustion_snapshot_passes, 0,
            "ordinary converging frame must not run ExhaustionSnapshot"
        );
        assert_eq!(
            stats.prepass_freshness_rechecks, 0,
            "ordinary converging frame without fontification callbacks must not recheck prepass freshness"
        );
        assert_eq!(
            stats.chrome_exhaustion_captures, 0,
            "zero-retry converging frame must not clone/root chrome outputs"
        );
        assert!(window_role_text(&state, window, GlyphRowRole::Text).contains("plain text"));
    }

    // 2. Non-local throw from mode-line :eval still aborts without ExhaustionSnapshot.
    // A real `catch` is required: GNU treats an unmatched throw as an ordinary
    // `no-catch` signal, which `dsafe_eval` catches and logs.
    {
        let (mut eval, frame, buffer, _window) = incr_editing_frame("throw body\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.eval_str("(setq noninteractive nil inhibit-redisplay nil)")
            .unwrap();
        let throw_fmt = eval.eval_str("'((:eval (throw 'abort-tag 42)))").unwrap();
        {
            let buf = eval.buffer_manager_mut().get_mut(buffer).unwrap();
            buf.set_buffer_local("mode-line-format", throw_fmt);
            buf.set_buffer_local("header-line-format", Value::NIL);
            buf.set_buffer_local("tab-line-format", Value::NIL);
        }
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        let ran = std::rc::Rc::new(std::cell::Cell::new(false));
        let ran_cb = ran.clone();
        eval.redisplay_fn = Some(Box::new(move |eval| {
            ran_cb.set(true);
            let outcome = engine.redisplay_frame_attempt(eval, frame);
            assert!(
                matches!(outcome, FrameLayoutAttempt::Aborted),
                "non-local throw must abort, never fall back to ExhaustionSnapshot"
            );
            assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 0);
            assert!(
                eval.has_mode_line_display_flow(),
                "deferred non-local flow must remain pending"
            );
        }));
        let caught = eval
            .eval_str("(catch 'abort-tag (redisplay t) 'did-not-throw)")
            .expect("catch must receive the thrown value");
        assert!(ran.get(), "redisplay callback must run");
        assert_eq!(caught.as_fixnum(), Some(42));
    }

    // 3. Snapshot stays strict and does not run ExhaustionSnapshot.
    {
        let (mut eval, frame, buffer_a, window) = incr_editing_frame("snap body\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        let buffer_b = eval.buffer_manager_mut().create_buffer("*832-snap-alt*");
        eval.buffer_manager_mut()
            .get_mut(buffer_b)
            .unwrap()
            .insert("snap alt\n");
        eval.set_variable("snap-win", Value::make_window(window.0));
        eval.set_variable("snap-buf-a", Value::make_buffer(buffer_a));
        eval.set_variable("snap-buf-b", Value::make_buffer(buffer_b));
        let snap_fmt = eval
            .eval_str(
                r#"(progn
              (setq snap-calls 0)
              '((:eval (progn
                         (setq snap-calls (1+ snap-calls))
                         (set-window-buffer
                           snap-win
                           (if (eq (window-buffer snap-win) snap-buf-a) snap-buf-b snap-buf-a))
                         "SNAP"))))"#,
            )
            .unwrap();
        for buf_id in [buffer_a, buffer_b] {
            let buf = eval.buffer_manager_mut().get_mut(buf_id).unwrap();
            buf.set_buffer_local("mode-line-format", snap_fmt);
            buf.set_buffer_local("header-line-format", Value::NIL);
            buf.set_buffer_local("tab-line-format", Value::NIL);
        }
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        let outcome = engine.snapshot_frame_attempt(&mut eval, frame);
        assert!(
            matches!(outcome, FrameLayoutAttempt::Aborted),
            "LayoutPurpose::Snapshot must stay strict"
        );
        assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 0);
    }

    // 4. SynchronousQuery (`window-end`) stays strict: never recollects prepass
    //    or enters ExhaustionSnapshot on an exhausting mutating fontifier.
    {
        let (mut eval, frame, _buffer, window) = incr_editing_frame("query body\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.eval_str(
            r#"(progn
          (setq q-calls 0 mode-line-format nil header-line-format nil tab-line-format nil)
          (setq fontification-functions
                (list (lambda (_pos)
                        (setq q-calls (1+ q-calls))
                        (save-excursion
                          (goto-char (point-max))
                          (insert (format "[Q%d]" q-calls)))
                        nil))))"#,
        )
        .expect("install exhausting fontifier for query control");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        let result = engine.query_window_layout(
            &mut eval,
            frame,
            window,
            neovm_core::window::WindowLayoutQueryScope::Viewport,
        );
        assert!(
            matches!(
                result,
                Err(neovm_core::window::WindowLayoutQueryFailure::DidNotConverge)
            ),
            "SynchronousQuery must stay strict and return DidNotConverge: {result:?}"
        );
        assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 0);
    }
}

/// N3 runtime guard: during `ExhaustionSnapshot`, every known Lisp seam is
/// skipped except TTY face realization advice (`realization.rs:777`). If
/// unexpected Lisp mutates `face_change_count` during the snapshot pass, the
/// post-snapshot guard must reject publication, abort cleanly, and settle
/// `NoSafe` without accepting.
#[test]
fn n3_exhaustion_snapshot_aborts_on_unexpected_tty_face_mutation() {
    use neovm_core::emacs_core::terminal::pure;
    use neovm_core::emacs_core::xfaces::realization::XfacesRealizationContextExt;
    struct RestoreTty;
    impl Drop for RestoreTty {
        fn drop(&mut self) {
            pure::reset_terminal_host();
            pure::reset_terminal_runtime();
        }
    }
    let _policy = neovm_core::emacs_core::eval::RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, buffer_a, window) = incr_editing_frame("tty n3 body a\n", 800, 240);
    let _restore_tty = RestoreTty;
    pure::configure_terminal_runtime(pure::TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_owned()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    assert!(eval.frame_is_tty_face_target(frame));
    let buffer_b = eval.buffer_manager_mut().create_buffer("*832-n3-b*");
    eval.buffer_manager_mut()
        .get_mut(buffer_b)
        .unwrap()
        .insert("tty n3 body b\n");
    eval.set_variable("n3-win", Value::make_window(window.0));
    eval.set_variable("n3-buf-a", Value::make_buffer(buffer_a));
    eval.set_variable("n3-buf-b", Value::make_buffer(buffer_b));
    // Switch buffers inside header-line :eval so retries exhaust (`SourceInvalidated`),
    // and on the final converging attempt (`n3-hl-calls == 2`) apply a fresh named
    // TTY color (`n3-cold-color`) to the buffer being switched INTO. That color is
    // realized for the first time during `ExhaustionSnapshot`'s body walk, invoking
    // `tty-color-desc` which mutates `bold` (bumping `face_change_count`).
    let hl_fmt = eval
        .eval_str(
            r##"(progn
          (setq inhibit-redisplay nil n3-hl-calls 0 n3-tty-mutated nil
                tty-defined-color-alist nil
                mode-line-format nil tab-line-format nil)
          (fset 'tty-color-desc
                (lambda (color &optional _frame)
                  (if (and (equal color "n3-cold-color") (null n3-tty-mutated))
                      (progn
                        (setq n3-tty-mutated t)
                        (internal-set-lisp-face-attribute 'bold :underline t 0)))
                  (list color 4 12345 23456 34567)))
          '((:eval (progn
                     (setq n3-hl-calls (1+ n3-hl-calls))
                     (let ((next (if (eq (window-buffer n3-win) n3-buf-a)
                                     n3-buf-b
                                   n3-buf-a)))
                       (if (= n3-hl-calls 2)
                           (save-current-buffer
                             (set-buffer next)
                             (put-text-property 1 5 'face '(:foreground "n3-cold-color"))))
                       (set-window-buffer n3-win next))
                     (format "N3:%d" n3-hl-calls)))))"##,
        )
        .expect("install exhausting header-line and mutating tty-color-desc");
    for buf_id in [buffer_a, buffer_b] {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).unwrap();
        buf.set_buffer_local("header-line-format", hl_fmt);
        buf.set_buffer_local("mode-line-format", Value::NIL);
        buf.set_buffer_local("tab-line-format", Value::NIL);
    }
    let accepted_before = eval.gnu_redisplay_hooks_accepted_serial();

    let aborted = std::rc::Rc::new(std::cell::Cell::new(false));
    let aborted_cb = aborted.clone();
    let mut engine = LayoutEngine::new_without_font_metrics();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        let outcome = engine.redisplay_frame_attempt(eval, frame);
        let calls = eval.eval_str("n3-hl-calls").unwrap().as_fixnum();
        let mutated = eval.eval_str("n3-tty-mutated").unwrap().is_truthy();
        assert!(
            matches!(outcome, FrameLayoutAttempt::Aborted),
            "N3 guard must abort ExhaustionSnapshot when face_change_count moves mid-snapshot (calls={calls:?}, mutated={mutated}, passes={})",
            engine.last_layout_stats().exhaustion_snapshot_passes
        );
        aborted_cb.set(true);
    }));

    eval.gnu_mark_windows_all();
    eval.redisplay_with_force(false)
        .expect("N3 guard abort settles cleanly");
    assert!(aborted.get(), "redisplay callback must run");
    assert!(
        eval.eval_str("n3-tty-mutated").unwrap().is_truthy(),
        "tty-color-desc mutation must actually have fired during ExhaustionSnapshot"
    );
    assert_eq!(
        eval.gnu_redisplay_hooks_accepted_serial(),
        accepted_before,
        "N3 guard abort must not accept the rejected snapshot"
    );
    assert!(
        !eval.frame_redisplay_pending(frame),
        "N3 guard abort must settle NoSafe for the frame"
    );
}

/// R1 safety half: a converging visible-pass fontifier that hides text on round 1
/// (like Org descriptive links / `org-hide-emphasis-markers`), pulling a brand-new
/// unfontified tail span into the viewport on round 2, must complete BOTH visible
/// rounds and converge cleanly without `ExhaustionSnapshot` or stale tail rows.
#[test]
fn converging_two_round_visible_fontification_converges_without_snapshot() {
    const PREFIX: &str = "seed\nBEFORE\n";
    let hidden1 = "hidden1\n".repeat(2_000);
    const MID: &str = "MID\n";
    let fold2 = "fold2\n".repeat(40);
    const TAIL: &str = "TAIL\n";
    let text = format!("{PREFIX}{hidden1}{MID}{fold2}{TAIL}");
    let hidden1_start = (PREFIX.chars().count() + 1) as i64;
    let mid_start = hidden1_start + hidden1.chars().count() as i64;
    let fold2_start = mid_start + MID.chars().count() as i64;
    let tail_start = fold2_start + fold2.chars().count() as i64;

    let (mut eval, frame, _buffer, window) = incr_editing_frame(&text, 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(&format!(
        r#"(progn
      (setq buffer-invisibility-spec t fn-positions nil two-round-ml-calls 0
            header-line-format nil tab-line-format nil
            mode-line-format '((:eval (progn
                                        (setq two-round-ml-calls (1+ two-round-ml-calls))
                                        "OK"))))
      (put-text-property (point-min) (point-max) 'fontified t)
      (put-text-property {hidden1_start} {mid_start} 'invisible t)
      (put-text-property {mid_start} (+ {mid_start} 4) 'fontified nil)
      (put-text-property {tail_start} (+ {tail_start} 5) 'fontified nil)
      (setq fontification-functions
            (list (lambda (pos)
                    (setq fn-positions (cons pos fn-positions))
                    (cond
                     ((= pos {mid_start})
                      (put-text-property {mid_start} (+ {mid_start} 4) 'face 'bold)
                      (put-text-property {mid_start} (+ {mid_start} 4) 'fontified t)
                      (put-text-property {fold2_start} {tail_start} 'invisible t))
                     ((= pos {tail_start})
                      (put-text-property {tail_start} (+ {tail_start} 5) 'face 'italic)
                      (put-text-property {tail_start} (+ {tail_start} 5) 'fontified t)))))))"#
    ))
    .expect("install two-round converging visible fontifier");

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(state) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("two-round converging visible fontification must publish");
    };
    assert_eq!(
        engine.last_layout_stats().exhaustion_snapshot_passes,
        0,
        "converging two-round fontification must not fall back to ExhaustionSnapshot"
    );
    assert_eq!(
        eval.eval_str("two-round-ml-calls").unwrap().as_fixnum(),
        Some(3),
        "must execute round 1 (MID), round 2 (TAIL), and converged round 3 (not publish stale round 2)"
    );
    let positions = neovm_core::emacs_core::value::list_to_vec(
        &eval.eval_str("(reverse fn-positions)").unwrap(),
    )
    .unwrap();
    assert_eq!(
        positions,
        vec![Value::fixnum(mid_start), Value::fixnum(tail_start)],
        "both distinct visible rounds must run in order: {positions:?}"
    );
    assert!(
        eval.eval_str(&format!("(get-text-property {mid_start} 'fontified)"))
            .unwrap()
            .is_t(),
        "round 1 span must remain fontified"
    );
    assert!(
        eval.eval_str(&format!("(get-text-property {tail_start} 'fontified)"))
            .unwrap()
            .is_t(),
        "round 2 newly exposed span must be fontified before publishing"
    );
    let body = window_role_text(&state, window, GlyphRowRole::Text);
    assert!(body.contains("MID"), "body must show MID: {body:?}");
    assert!(
        body.contains("TAIL"),
        "body must show newly exposed TAIL: {body:?}"
    );
    assert!(
        !body.contains("fold2"),
        "collapsed fold2 must be hidden: {body:?}"
    );
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .expect("window matrix entry");
    let tail_row = entry
        .matrix
        .rows
        .iter()
        .find(|row| {
            row.enabled
                && row.role == GlyphRowRole::Text
                && glyphs_logical_text(&row.glyphs[GlyphArea::Text.index()]).contains("TAIL")
        })
        .expect("published TAIL row");
    let tail_glyphs: Vec<_> = tail_row.glyphs[GlyphArea::Text.index()]
        .iter()
        .filter(|g| matches!(g.glyph_type, GlyphType::Char { ch } if "TAIL".contains(ch)))
        .collect();
    assert_eq!(tail_glyphs.len(), 4, "expected 4 TAIL glyphs");
    assert!(
        tail_glyphs
            .iter()
            .all(|g| state.faces.get(&g.face_id).is_some_and(|f| f.is_italic())),
        "published TAIL glyphs must carry the round-2 italic face (proving converged round 3 published)"
    );
    assert!(
        engine
            .retained_window_matrices
            .contains_key(&DisplayWindowId::new(window.0 as i64)),
        "converged multi-round fontification must retain its matrix normally"
    );
}

/// R3 Case F: a mode-line `:eval` whose height grows once (`:height 2.0`)
/// must converge cleanly without `ExhaustionSnapshot` and publish the taller
/// mode-line row (Neo evaluates 2 times vs GNU 31.1's 3 due to GNU's extra
/// `SET_FRAME_GARBAGED` `retry_frame` pass).
#[test]
fn converging_mode_line_height_growth_converges_without_snapshot() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("height test\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r#"(setq header-line-format nil tab-line-format nil mode-line-format '("INIT"))"#,
    )
    .expect("settle baseline mode-line");

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(base_state) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("baseline frame must prepare");
    };
    let mode_line_height = |state: &neomacs_display_protocol::glyph_matrix::FrameDisplayState| {
        state
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == window.0 as i64)
            .expect("window matrix")
            .matrix
            .rows
            .iter()
            .find(|row| row.enabled && row.role == GlyphRowRole::ModeLine)
            .expect("enabled mode-line row")
            .height_px
    };
    let base_height = mode_line_height(&base_state);

    eval.eval_str(
        r#"(progn
      (setq f-calls 0)
      (setq mode-line-format
            '((:eval (progn
                       (setq f-calls (1+ f-calls))
                       (propertize (format "TALL:%d" f-calls)
                                   'face '(:height 2.0)))))))"#,
    )
    .expect("install tall mode-line");
    let FrameLayoutAttempt::Prepared(tall_state) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("height-growing mode-line must converge and prepare");
    };
    assert_eq!(
        engine.last_layout_stats().exhaustion_snapshot_passes,
        0,
        "converging mode-line height growth must not enter ExhaustionSnapshot"
    );
    let calls = eval.eval_str("f-calls").unwrap().as_fixnum().unwrap();
    assert_eq!(
        calls, 2,
        "one height-discovery retry + one settled pass: {calls}"
    );
    let tall_height = mode_line_height(&tall_state);
    assert!(
        tall_height > base_height,
        "published mode-line height must grow: base={base_height}, tall={tall_height}"
    );
    let ml = window_role_text(&tall_state, window, GlyphRowRole::ModeLine);
    assert!(
        ml.contains("TALL:2"),
        "mode-line must show converged TALL:2: {ml:?}"
    );
}

/// R3 Case G: a mode-line `:eval` oscillating between `:height 2.0` and `:height 1.0`
/// (unbounded `fonts_changed` loop in GNU 31.1, probe G = 22 until test cap) is
/// detected as a repeated `(assumed, measured)` transition cycle on pass 3
/// (`A -> B`, `B -> A`, `A -> B` repeats) and published via `ExhaustionSnapshot`.
#[test]
fn oscillating_mode_line_height_publishes_via_exhaustion_snapshot() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("height osc test\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r#"(setq header-line-format nil tab-line-format nil mode-line-format '("INIT"))"#,
    )
    .expect("settle baseline mode-line");

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(_) = engine.redisplay_frame_attempt(&mut eval, frame) else {
        panic!("baseline frame must prepare");
    };

    eval.eval_str(
        r#"(progn
      (setq g-calls 0)
      (setq mode-line-format
            '((:eval (progn
                       (setq g-calls (1+ g-calls))
                       (propertize (format "OSC:%d" g-calls)
                                   'face (list :height (if (= (% g-calls 2) 1) 2.0 1.0))))))))"#,
    )
    .expect("install oscillating-height mode-line");
    let FrameLayoutAttempt::Prepared(state) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("oscillating mode-line height must publish via ExhaustionSnapshot");
    };
    assert_eq!(
        engine.last_layout_stats().exhaustion_snapshot_passes,
        1,
        "oscillating mode-line height must trigger ExhaustionSnapshot on cycle detection"
    );
    let calls = eval.eval_str("g-calls").unwrap().as_fixnum().unwrap();
    assert_eq!(
        calls, 3,
        "repeated (A, B) transition cycle must be detected on pass 3 (vs GNU's unbounded 22): {calls}"
    );
    let ml = window_role_text(&state, window, GlyphRowRole::ModeLine);
    assert!(
        ml.contains("OSC:3"),
        "ExhaustionSnapshot must reuse last captured OSC:3 mode-line: {ml:?}"
    );

    // LOW-7: strict Snapshot must still abort on a genuine A <-> B height
    // oscillation (W3's single-excursion tolerance must not weaken cycle abort).
    // Use a fresh engine starting from the 1.0x baseline height so pass 1
    // measures 2.0x (A -> B), pass 2 measures 1.0x (B -> A), and pass 3 repeats
    // (A -> B) and aborts.
    eval.eval_str("(setq g-calls 0)").unwrap();
    let mut snap_engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut snap_engine);
    let snap_outcome = snap_engine.snapshot_frame_attempt(&mut eval, frame);
    assert!(
        matches!(snap_outcome, FrameLayoutAttempt::Aborted),
        "strict Snapshot must abort on true A<->B chrome height oscillation"
    );
    assert_eq!(
        snap_engine.last_layout_stats().exhaustion_snapshot_passes,
        0,
        "strict Snapshot must not enter ExhaustionSnapshot on height cycle"
    );
    assert_eq!(
        eval.eval_str("g-calls").unwrap().as_fixnum(),
        Some(3),
        "strict Snapshot detects repeated (A, B) transition on pass 3"
    );
}

/// W2: a repeating declining sparse fontifier on one sibling window must not
/// cut off another sibling window that needs 3 visible rounds to converge
/// (`MID` collapses `fold_b1` -> exposes `TAIL` -> collapses `fold_b2` ->
/// exposes `TAIL2`). Tested in both sibling orders (declining-first and
/// progressing-first) so neither first-window-only nor last-window-only
/// recording can pass.
fn run_w2_declining_and_three_round_progressing_siblings(declining_first: bool) {
    const PREFIX: &str = "seed\nBEFORE\n";
    let hidden_a = "hidden-a\n".repeat(2_000);
    const TAIL_A: &str = "TAIL-A\n";
    let text_a = format!("{PREFIX}{hidden_a}{TAIL_A}");
    let hidden_a_start = (PREFIX.chars().count() + 1) as i64;
    let tail_a_start = hidden_a_start + hidden_a.chars().count() as i64;

    let hidden_b = "hidden-b\n".repeat(2_000);
    const MID_B: &str = "MID\n";
    let fold_b1 = "fold-b1\n".repeat(40);
    const TAIL_B: &str = "TAIL\n";
    let fold_b2 = "fold-b2\n".repeat(40);
    const TAIL2_B: &str = "FINAL\n";
    let text_b = format!("{PREFIX}{hidden_b}{MID_B}{fold_b1}{TAIL_B}{fold_b2}{TAIL2_B}");
    let hidden_b_start = (PREFIX.chars().count() + 1) as i64;
    let mid_b_start = hidden_b_start + hidden_b.chars().count() as i64;
    let fold_b1_start = mid_b_start + MID_B.chars().count() as i64;
    let tail_b_start = fold_b1_start + fold_b1.chars().count() as i64;
    let fold_b2_start = tail_b_start + TAIL_B.chars().count() as i64;
    let tail2_b_start = fold_b2_start + fold_b2.chars().count() as i64;

    let (first_text, second_text) = if declining_first {
        (&text_a, &text_b)
    } else {
        (&text_b, &text_a)
    };
    let (mut eval, frame, first_buf, first_win) = incr_editing_frame(first_text, 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let second_buf = eval.buffer_manager_mut().create_buffer(if declining_first {
        "*w2-progress-b*"
    } else {
        "*w2-declining-a*"
    });
    eval.buffer_manager_mut()
        .get_mut(second_buf)
        .unwrap()
        .insert(second_text);
    let second_win = eval
        .frame_manager_mut()
        .split_window(
            frame,
            first_win,
            neovm_core::window::SplitDirection::Horizontal,
            second_buf,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split two siblings");
    let (buf_a, buf_b, win_b) = if declining_first {
        (first_buf, second_buf, second_win)
    } else {
        (second_buf, first_buf, first_win)
    };

    eval.set_variable("w2-buf-a", Value::make_buffer(buf_a));
    eval.set_variable("w2-buf-b", Value::make_buffer(buf_b));
    eval.eval_str(&format!(
        r#"(progn
      (setq w2-a-calls 0 w2-b-positions nil)
      ;; Declining sibling A: sparse hook always mutates face at tail_a
      (save-current-buffer
        (set-buffer w2-buf-a)
        (set (make-local-variable 'buffer-invisibility-spec) t)
        (set (make-local-variable 'header-line-format) nil)
        (set (make-local-variable 'tab-line-format) nil)
        (set (make-local-variable 'mode-line-format) '("A-ML"))
        (put-text-property (point-min) (point-max) 'fontified t)
        (put-text-property {hidden_a_start} {tail_a_start} 'invisible t)
        (put-text-property {tail_a_start} (1+ {tail_a_start}) 'fontified nil)
        (put-text-property 6 12 'face 'bold)
        (set (make-local-variable 'fontification-functions)
             (list (lambda (_pos)
                     (setq w2-a-calls (1+ w2-a-calls))
                     (put-text-property 6 12 'face
                       (if (= (% w2-a-calls 2) 0) 'bold 'italic))
                     nil))))
      ;; Progressing sibling B: 3 visible rounds (MID -> TAIL -> FINAL)
      (save-current-buffer
        (set-buffer w2-buf-b)
        (set (make-local-variable 'buffer-invisibility-spec) t)
        (set (make-local-variable 'header-line-format) nil)
        (set (make-local-variable 'tab-line-format) nil)
        (set (make-local-variable 'mode-line-format) '("B-ML"))
        (put-text-property (point-min) (point-max) 'fontified t)
        (put-text-property {hidden_b_start} {mid_b_start} 'invisible t)
        (put-text-property {mid_b_start} (+ {mid_b_start} 4) 'fontified nil)
        (put-text-property {tail_b_start} (+ {tail_b_start} 5) 'fontified nil)
        (put-text-property {tail2_b_start} (+ {tail2_b_start} 6) 'fontified nil)
        (set (make-local-variable 'fontification-functions)
             (list (lambda (pos)
                     (setq w2-b-positions (cons pos w2-b-positions))
                     (cond
                      ((= pos {mid_b_start})
                       (put-text-property {mid_b_start} (+ {mid_b_start} 4) 'face 'bold)
                       (put-text-property {mid_b_start} (+ {mid_b_start} 4) 'fontified t)
                       (put-text-property {fold_b1_start} {tail_b_start} 'invisible t))
                      ((= pos {tail_b_start})
                       (put-text-property {tail_b_start} (+ {tail_b_start} 5) 'face 'bold)
                       (put-text-property {tail_b_start} (+ {tail_b_start} 5) 'fontified t)
                       (put-text-property {fold_b2_start} {tail2_b_start} 'invisible t))
                      ((= pos {tail2_b_start})
                       (put-text-property {tail2_b_start} (+ {tail2_b_start} 6) 'face 'italic)
                       (put-text-property {tail2_b_start} (+ {tail2_b_start} 6) 'fontified t))))))))"#
    ))
    .expect("install W2 declining + 3-round converging sibling hooks");

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(state) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("two-sibling source-only retry episode must publish");
    };
    assert_eq!(
        engine.last_layout_stats().exhaustion_snapshot_passes,
        0,
        "source-only retries publish the assembled frame directly without ExhaustionSnapshot"
    );
    let b_positions = neovm_core::emacs_core::value::list_to_vec(
        &eval.eval_str("(reverse w2-b-positions)").unwrap(),
    )
    .unwrap();
    assert_eq!(
        b_positions,
        vec![
            Value::fixnum(mid_b_start),
            Value::fixnum(tail_b_start),
            Value::fixnum(tail2_b_start),
        ],
        "progressing sibling B must complete all 3 visible rounds (declining_first={declining_first}): {b_positions:?}"
    );
    let entry_b = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == win_b.0 as i64)
        .expect("progressing sibling B matrix");
    let final_row_b = entry_b
        .matrix
        .rows
        .iter()
        .find(|row| {
            row.enabled
                && row.role == GlyphRowRole::Text
                && glyphs_logical_text(&row.glyphs[GlyphArea::Text.index()]).contains("FINAL")
        })
        .expect("published FINAL row in sibling B (requires round 3 to not be truncated)");
    let final_glyphs_b: Vec<_> = final_row_b.glyphs[GlyphArea::Text.index()]
        .iter()
        .filter(|g| matches!(g.glyph_type, GlyphType::Char { ch } if "FINAL".contains(ch)))
        .collect();
    assert_eq!(
        final_glyphs_b.len(),
        5,
        "expected 5 FINAL glyphs in sibling B"
    );
    assert!(
        final_glyphs_b
            .iter()
            .all(|g| state.faces.get(&g.face_id).is_some_and(|f| f.is_italic())),
        "sibling B's published FINAL glyphs must carry the round-3 italic face (declining_first={declining_first})"
    );
    let a_calls = eval.eval_str("w2-a-calls").unwrap().as_fixnum().unwrap();
    assert!(
        (4..=5).contains(&a_calls),
        "declining sibling A must still terminate well below 13x once B settles (declining_first={declining_first}): {a_calls}"
    );
}

#[test]
fn w2_repeating_earlier_sibling_does_not_truncate_progressing_later_sibling() {
    run_w2_declining_and_three_round_progressing_siblings(true);
    run_w2_declining_and_three_round_progressing_siblings(false);
}

/// W3: a stateful mode-line `:eval` that makes a single height excursion
/// (`A -> B -> A` then holds at `A`) must converge cleanly in 3 passes for both
/// `Redisplay` (no `ExhaustionSnapshot`) and strict `Snapshot` (no `Aborted`),
/// because no `(assumed, measured)` transition pair repeats.
#[test]
fn w3_single_chrome_height_excursion_converges_for_redisplay_and_snapshot() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("excursion test\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r#"(setq header-line-format nil tab-line-format nil mode-line-format '("INIT"))"#,
    )
    .expect("settle baseline mode-line");

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(_) = engine.redisplay_frame_attempt(&mut eval, frame) else {
        panic!("baseline frame must prepare");
    };

    eval.eval_str(
        r#"(progn
      (setq ex-calls 0)
      (setq mode-line-format
            '((:eval (progn
                       (setq ex-calls (1+ ex-calls))
                       (propertize (format "EX:%d" ex-calls)
                                   'face (list :height (if (= ex-calls 1) 2.0 1.0))))))))"#,
    )
    .expect("install single-excursion mode-line");

    // 1. Live Redisplay converges in 3 calls without ExhaustionSnapshot.
    let FrameLayoutAttempt::Prepared(redisplay_state) =
        engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("single height excursion must converge in Redisplay");
    };
    assert_eq!(
        engine.last_layout_stats().exhaustion_snapshot_passes,
        0,
        "single height excursion A->B->A->A must converge without ExhaustionSnapshot"
    );
    assert_eq!(
        eval.eval_str("ex-calls").unwrap().as_fixnum(),
        Some(3),
        "single excursion takes 3 evaluations: A->B (1), B->A (2), A->A converged (3)"
    );
    let ml = window_role_text(&redisplay_state, window, GlyphRowRole::ModeLine);
    assert!(
        ml.contains("EX:3"),
        "mode-line must show converged EX:3: {ml:?}"
    );
    assert!(
        engine
            .retained_window_matrices
            .contains_key(&DisplayWindowId::new(window.0 as i64)),
        "converged single excursion must retain its matrix normally"
    );

    // 2. Strict Snapshot also converges in 3 calls instead of aborting
    //    (using a fresh force-mode-line-update so it actually evaluates chrome
    //    instead of reusing Part 1's clean retained matrix).
    eval.eval_str("(progn (setq ex-calls 0) (force-mode-line-update t))")
        .unwrap();
    let FrameLayoutAttempt::Prepared(snapshot_state) =
        engine.snapshot_frame_attempt(&mut eval, frame)
    else {
        panic!("single height excursion must also converge under strict Snapshot");
    };
    assert_eq!(engine.last_layout_stats().exhaustion_snapshot_passes, 0);
    assert_eq!(
        eval.eval_str("ex-calls").unwrap().as_fixnum(),
        Some(3),
        "strict Snapshot must complete the same 3-pass convergence"
    );
    let snap_ml = window_role_text(&snapshot_state, window, GlyphRowRole::ModeLine);
    assert!(
        snap_ml.contains("EX:3"),
        "snapshot mode-line must show converged EX:3: {snap_ml:?}"
    );
}
