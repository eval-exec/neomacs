//! A completed sparse fontifier walk must survive bounded source retries.
use super::*;
use std::{cell::Cell, rc::Rc};

fn sparse_exhaustion_frame() -> (
    Context,
    neovm_core::window::FrameId,
    BufferId,
    neovm_core::window::WindowId,
    neovm_core::window::WindowId,
    i64,
) {
    // Existing sparse-fontifier regression's real post-walk coverage seam.
    const PREFIX: &str = "seed\nBEFORE\n";
    let hidden = "hidden\n".repeat(10_000);
    let tail = (PREFIX.chars().count() + hidden.chars().count() + 1) as i64;
    let (mut eval, frame, buffer, window) =
        incr_editing_frame(&format!("{PREFIX}{hidden}TAIL\n"), 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let sibling_buffer = eval
        .buffer_manager_mut()
        .create_buffer("*832-sparse-sibling*");
    let sibling_body = eval.buffer_manager_mut().get_mut(sibling_buffer).unwrap();
    sibling_body.insert("independent sibling body\n");
    for name in [
        "header-line-format",
        "tab-line-format",
        "fontification-functions",
    ] {
        sibling_body.set_buffer_local(name, Value::NIL);
    }
    let sibling_format = eval
        .eval_str(
            r#"'((:eval (progn
      (setq sparse-sibling-chrome (1+ sparse-sibling-chrome)) "SIBLING")))"#,
        )
        .unwrap();
    eval.buffer_manager_mut()
        .get_mut(sibling_buffer)
        .unwrap()
        .set_buffer_local("mode-line-format", sibling_format);
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
    eval.eval_str(&format!(
        r#"(progn
      (setq buffer-invisibility-spec t sparse-events nil sparse-calls nil
            sparse-font-count 0 sparse-chrome-count 0 sparse-sibling-chrome 0
            header-line-format nil tab-line-format nil)
      (put-text-property (point-min) (point-max) 'fontified t)
      (put-text-property 13 {tail} 'invisible t)
      (put-text-property {tail} (1+ {tail}) 'fontified nil)
      (put-text-property 6 12 'face 'bold)
      (setq mode-line-format '((:eval (progn
        (setq sparse-events (cons 'chrome sparse-events)
              sparse-chrome-count (1+ sparse-chrome-count)) "SPARSE"))))
      (setq fontification-functions (list (lambda (position)
        (setq sparse-events (cons 'font sparse-events)
              sparse-calls (cons position sparse-calls)
              sparse-font-count (1+ sparse-font-count))
        (put-text-property 6 12 'face
          (if (= (% sparse-font-count 2) 0) 'bold 'italic))
        nil))))"#
    ))
    .expect("install repeatedly changing declining sparse fontifier");
    let neovm_core::window::Window::Leaf { force_start, .. } = eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    else {
        panic!("source leaf");
    };
    *force_start = true;
    (eval, frame, buffer, window, sibling, tail)
}

fn sparse_count(eval: &mut Context, name: &str) -> i64 {
    eval.eval_str(name).unwrap().as_fixnum().unwrap()
}

#[test]
fn exhausted_sparse_fontifier_publishes_one_complete_two_window_attempt() {
    let (mut eval, frame, buffer, window, sibling, tail) = sparse_exhaustion_frame();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let result = engine.redisplay_frame_attempt(&mut eval, frame);
    let events = neovm_core::emacs_core::value::list_to_vec(
        &eval.eval_str("(reverse sparse-events)").unwrap(),
    )
    .unwrap();
    let calls = neovm_core::emacs_core::value::list_to_vec(&eval.eval_str("sparse-calls").unwrap())
        .unwrap();
    // Receipt BEFORE Prepared: bad early-prepass placement cannot count as RED.
    assert_eq!(
        events.len(),
        4,
        "exact two-attempt callback sequence (chrome, font, chrome, font): {events:?}"
    );
    assert_eq!(
        events.first(),
        Some(&Value::symbol("chrome")),
        "first sparse callback follows real body/chrome: {events:?}"
    );
    assert!(
        calls.len() == 2 && calls.iter().all(|p| *p == Value::fixnum(tail)),
        "only the actual visible sparse stop repeats once (2 calls): {calls:?}"
    );
    assert!(
        eval.eval_str(&format!("(get-text-property {tail} 'fontified)"))
            .unwrap()
            .is_nil(),
        "the hook never marks its stop"
    );
    let chrome_count = sparse_count(&mut eval, "sparse-chrome-count");
    assert_eq!(
        chrome_count, 2,
        "repeating identical sparse stop must exhaust after 1 retry (2 chrome evals, not 13): {chrome_count}"
    );
    let FrameLayoutAttempt::Prepared(output) = result else {
        panic!(
            "complete sparse retries must publish a whole frame at exhaustion; events={events:?}, calls={calls:?}"
        );
    };
    let sibling_chrome = sparse_count(&mut eval, "sparse-sibling-chrome");
    assert_eq!(
        sibling_chrome, 2,
        "later sibling chrome must finish on both attempts without repeating 13x: {sibling_chrome}"
    );
    let state = output.state();
    let ids: std::collections::HashSet<_> = state
        .window_matrices
        .iter()
        .map(|entry| entry.window_id)
        .collect();
    assert_eq!(
        ids.len(),
        state.window_matrices.len(),
        "unique closed grids"
    );
    for (id, required, forbidden) in [
        (window, "BEFORE", "hidden"),
        (sibling, "independent sibling body", "TAIL"),
    ] {
        let entry = state
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == id.0 as i64)
            .unwrap();
        let text = entry
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .map(|row| glyphs_logical_text(&row.glyphs[GlyphArea::Text.index()]))
            .collect::<Vec<_>>()
            .join("");
        assert!(
            text.contains(required),
            "complete selected body {id:?}: {text:?}"
        );
        assert!(
            !text.contains(forbidden),
            "no hidden/mixed sibling: {text:?}"
        );
        if id == window {
            assert!(text.contains("TAIL"), "real sparse tail: {text:?}");
        }
    }
    let cursor = state
        .phys_cursor
        .as_ref()
        .expect("ordinary selected cursor");
    assert_eq!(cursor.window_id.get(), window.0 as i64);
    assert!(cursor.width > 0.0 && cursor.height > 0.0);
    assert!(state.validate_spatial_projections().is_ok());
    assert!(!state.presented_hit_index.is_empty());
    let retained = engine
        .retained_window_matrices
        .get(&DisplayWindowId::new(window.0 as i64))
        .unwrap();
    assert!(
        retained.key.props_modified_tick
            < eval
                .buffer_manager()
                .get(buffer)
                .unwrap()
                .props_modified_tick(),
        "original source evidence must not be rebased after the hook"
    );
    let id = neovm_core::window::geometry::PresentationId::new(state.presentation_id.get());
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .activate_display_presentation(id)
        .unwrap();
    for leaf in [window, sibling] {
        let frame_state = eval.frame_manager().get(frame).unwrap();
        assert!(frame_state.redisplay_snapshot(leaf).is_some());
        let output = frame_state
            .find_window(leaf)
            .unwrap()
            .display()
            .unwrap()
            .redisplay_output()
            .expect("ordinary accepted window output");
        assert_eq!(output.generation, id, "one whole-frame generation");
        assert!(output.gnu_accuracy.is_some());
    }
}

#[test]
fn exhausted_sparse_fontifier_settles_idle_and_preserves_new_work() {
    let _policy = neovm_core::emacs_core::eval::RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, _, _, _, _) = sparse_exhaustion_frame();
    let requests = Rc::new(Cell::new(0));
    let observed = requests.clone();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed.set(observed.get() + 1);
        let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(eval, frame)
        else {
            panic!("complete sparse exhaustion must yield ordinary output");
        };
        let id = neovm_core::window::geometry::PresentationId::new(output.presentation_id.get());
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .activate_display_presentation(id)
            .unwrap();
    }));
    eval.redisplay_with_force(false).unwrap();
    assert_eq!(requests.get(), 1);
    let callbacks = sparse_count(&mut eval, "sparse-font-count");
    assert!(callbacks >= 2, "genuine repeated source mutation");
    for _ in 0..3 {
        eval.redisplay_with_force(false).unwrap();
    }
    assert_eq!(
        requests.get(),
        1,
        "settle the owned episode instead of idle spin"
    );
    assert_eq!(sparse_count(&mut eval, "sparse-font-count"), callbacks);
    eval.eval_str("(goto-char 2)").unwrap();
    eval.redisplay_with_force(false).unwrap();
    assert_eq!(
        requests.get(),
        2,
        "new external point work starts a new episode"
    );
    assert!(sparse_count(&mut eval, "sparse-font-count") > callbacks);
}
