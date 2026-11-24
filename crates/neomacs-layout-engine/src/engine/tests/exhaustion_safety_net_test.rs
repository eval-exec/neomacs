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
    assert!(calls >= 2, "must actually exhaust retries: {calls}");

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
            when-calls 0 when-inhibit nil
            header-line-format nil tab-line-format nil
            mode-line-format '("WHEN-ML"))
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
    assert!(calls >= 2, "must exhaust retries: {calls}");
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
    assert!(calls >= 2, "must exhaust retries: {calls}");
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
    assert!(w1_calls >= 2, "w1 must exhaust retries: {w1_calls}");
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
    // and on the final converging attempt (`n3-hl-calls == 13`) apply a fresh named
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
                       (if (= n3-hl-calls 13)
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
