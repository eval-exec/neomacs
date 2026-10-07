//! Real public snapshot ownership regressions. Each fixture owns a scoped
//! numeric hook policy before Context creation and until that Context drops.
//! No process environment or Lisp state is shared across mutators.

use super::super::super::frame_layout::{
    REDISPLAY_RUNTIME, install_frame_snapshot_fn, run_tty_layout_tree,
};
use super::initialized_redisplay_test_frame;
use neovm_core::buffer::LispCharPos1;
use neovm_core::emacs_core::eval::RedisplayHookPolicyGuard;
use neovm_core::emacs_core::{Context, Value};
use neovm_core::heap_types::LispString;
use neovm_core::window::{FrameId, FrameVisibility, Window, WindowEndState, WindowId};

/// One test exclusively owns this Context and numeric frame/window IDs.
/// The numeric policy guard stays on the owning test thread and drops after
/// the Context. No Lisp owner or mutable cache is shared with another mutator.
struct SnapshotFixture {
    eval: Context,
    visible_frame: FrameId,
    hidden_frame: FrameId,
    hidden_window: WindowId,
    _policy: RedisplayHookPolicyGuard,
}

/// Numeric observations copied while the fixture Context is exclusively
/// owned; they retain no heap Values and may be compared after another walk.
#[derive(Debug, PartialEq, Eq)]
struct WindowPositions {
    start: LispCharPos1,
    point: LispCharPos1,
    force_start: bool,
    end: WindowEndState,
}

fn window_positions(eval: &Context, frame: FrameId, window: WindowId) -> WindowPositions {
    let Window::Leaf {
        window_start,
        point,
        force_start,
        window_end,
        ..
    } = eval
        .frame_manager()
        .get(frame)
        .and_then(|frame| frame.find_window(window))
        .expect("live fixture leaf")
    else {
        panic!("fixture window must be a leaf");
    };
    WindowPositions {
        start: *window_start,
        point: *point,
        force_start: *force_start,
        end: *window_end,
    }
}

fn fixture(gnu: bool) -> SnapshotFixture {
    let policy = if gnu {
        RedisplayHookPolicyGuard::gnu()
    } else {
        RedisplayHookPolicyGuard::legacy()
    };
    REDISPLAY_RUNTIME.with(|runtime| runtime.disable_cosmic_metrics());
    let (mut eval, _visible_buffer, visible_frame, visible_window) =
        initialized_redisplay_test_frame(
            "snapshot-visible",
            80,
            24,
            "visible snapshot source\n",
            80,
        );
    assert_eq!(eval.gnu_redisplay_hooks_policy_enabled(), gnu);
    {
        let frame = eval.frame_manager_mut().get_mut(visible_frame).unwrap();
        frame.char_width = 1.0;
        frame.char_height = 1.0;
    }
    // This is the production Context transaction's native frontend callback:
    // it visits only the selected visible frame tree and accepts its output.
    eval.redisplay_fn = Some(Box::new(|eval| {
        let _ = run_tty_layout_tree(eval);
    }));
    install_frame_snapshot_fn(&mut eval);

    let hidden_buffer = eval
        .buffer_manager_mut()
        .create_buffer("snapshot-hidden-buffer");
    {
        let buffer = eval.buffer_manager_mut().get_mut(hidden_buffer).unwrap();
        buffer.insert(&"hidden snapshot source\n".repeat(80));
        buffer.set_buffer_local("header-line-format", Value::string("SNAPSHOT-HEADER"));
        buffer.set_buffer_local("mode-line-format", Value::string("SNAPSHOT-MODE"));
    }
    let hidden_frame =
        eval.frame_manager_mut()
            .create_frame("snapshot-hidden", 80, 24, hidden_buffer);
    let hidden_window = {
        let frame = eval.frame_manager_mut().get_mut(hidden_frame).unwrap();
        frame.initial = false;
        frame.visibility = FrameVisibility::Invisible;
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.selected_window
    };
    assert_eq!(
        eval.frame_manager().selected_frame().map(|frame| frame.id),
        Some(visible_frame),
        "the public forced redisplay must own the other visible tree"
    );
    eval.set_variable(
        "d5-snapshot-hidden-buffer",
        Value::make_buffer(hidden_buffer),
    );
    eval.set_variable(
        "d5-snapshot-hidden-frame",
        Value::make_frame(hidden_frame.0),
    );
    eval.set_variable(
        "d5-snapshot-hidden-window",
        Value::make_window(hidden_window.0),
    );
    eval.set_variable(
        "d5-snapshot-visible-window",
        Value::make_window(visible_window.0),
    );
    // Establish the forced start before installing the local scroll hook, so
    // the only callback opportunity under test is the snapshot row producer.
    eval.eval_str(
        "(progn
           (set-window-start d5-snapshot-hidden-window 11)
           (set-window-point d5-snapshot-hidden-window 1000)
           (setq d5-snapshot-hook-count 0))",
    )
    .expect("prepare hidden viewport before local hook installation");
    SnapshotFixture {
        eval,
        visible_frame,
        hidden_frame,
        hidden_window,
        _policy: policy,
    }
}

fn install_hidden_hook(eval: &mut Context, throwing: bool) {
    let source = if throwing {
        "(progn
           (setq d5-snapshot-caller-buffer (current-buffer))
           (set-buffer (window-buffer d5-snapshot-hidden-window))
           (set (make-local-variable 'window-scroll-functions)
                (list (lambda (window _start)
                        (setq d5-snapshot-hook-count (1+ d5-snapshot-hook-count))
                        (set-window-start window 100 t)
                        (throw 'd5-snapshot-owner 'd5-unowned-snapshot-hook))))
           (set-buffer d5-snapshot-caller-buffer))"
    } else {
        "(progn
           (setq d5-snapshot-caller-buffer (current-buffer))
           (set-buffer (window-buffer d5-snapshot-hidden-window))
           (set (make-local-variable 'window-scroll-functions)
                (list (lambda (_window _start)
                        (setq d5-snapshot-hook-count (1+ d5-snapshot-hook-count)))))
           (set-buffer d5-snapshot-caller-buffer))"
    };
    eval.eval_str(source)
        .expect("install hidden buffer-local hook");
}

fn hook_count(eval: &mut Context) -> i64 {
    eval.eval_str("d5-snapshot-hook-count")
        .expect("numeric hook observation")
        .as_fixnum()
        .expect("fixnum hook count")
}

#[test]
fn gnu_hidden_frame_snapshot_does_not_run_or_defer_scroll_hooks() {
    let mut f = fixture(true);
    install_hidden_hook(&mut f.eval, true);
    let before = window_positions(&f.eval, f.hidden_frame, f.hidden_window);
    assert!(
        before.force_start,
        "the fixture must exercise a committed-start site"
    );
    assert_eq!(
        before.end,
        WindowEndState::Unrecorded,
        "the hidden frame has never been redisplayed"
    );
    // The real Lisp catch makes the hook's Throw valid. It must not turn into
    // a no-catch Signal that safe hooks would demote before the defer seam.
    let result = f.eval.eval_str(
        "(catch 'd5-snapshot-owner
           (condition-case nil
               (neomacs--frame-snapshot d5-snapshot-hidden-frame 'text)
             (error 'd5-snapshot-error)))",
    );
    assert!(
        !f.eval.redisplay_hook_flow_pending(),
        "a renderer-inert snapshot must not leave a Flow for a later redisplay: {result:?}"
    );
    let text = result
        .expect("public hidden-frame snapshot")
        .as_str_owned()
        .expect("snapshot must return text rather than a hook payload or generic error");
    assert_eq!(hook_count(&mut f.eval), 0);
    assert_eq!(
        window_positions(&f.eval, f.hidden_frame, f.hidden_window),
        before
    );
    assert!(
        text.contains("snapshot-hidden-buffer"),
        "explicit hidden-frame output: {text}"
    );
    assert!(
        text.contains("SNAPSHOT-HEADER"),
        "fresh header chrome: {text}"
    );
    assert!(
        text.contains("SNAPSHOT-MODE"),
        "fresh mode-line chrome: {text}"
    );
    f.eval
        .eval_str("(redisplay t)")
        .expect("following visible redisplay must not receive a delayed snapshot throw");
    assert!(!f.eval.redisplay_hook_flow_pending());
    assert_eq!(
        f.eval
            .frame_manager()
            .selected_frame()
            .map(|frame| frame.id),
        Some(f.visible_frame)
    );
    assert_eq!(hook_count(&mut f.eval), 0);
    assert_eq!(
        window_positions(&f.eval, f.hidden_frame, f.hidden_window),
        before
    );
}

#[test]
fn legacy_hidden_frame_snapshot_preserves_scroll_hook_policy() {
    let mut f = fixture(false);
    install_hidden_hook(&mut f.eval, false);
    let before = window_positions(&f.eval, f.hidden_frame, f.hidden_window);
    assert!(before.force_start);
    let text = f
        .eval
        .eval_str("(neomacs--frame-snapshot d5-snapshot-hidden-frame 'text)")
        .expect("legacy public snapshot")
        .as_str_owned()
        .expect("legacy snapshot text");
    assert!(
        hook_count(&mut f.eval) > 0,
        "legacy snapshot still runs its old start hook"
    );
    assert!(!f.eval.redisplay_hook_flow_pending());
    assert!(text.contains("SNAPSHOT-HEADER") && text.contains("SNAPSHOT-MODE"));
    assert_ne!(
        window_positions(&f.eval, f.hidden_frame, f.hidden_window).end,
        WindowEndState::Unrecorded,
        "legacy snapshot retains its existing window-end publication"
    );
}

#[test]
fn gnu_visible_frame_redisplay_still_propagates_scroll_hook_throw() {
    let mut f = fixture(true);
    f.eval
        .eval_str(
            "(progn
               (set-window-start d5-snapshot-visible-window 100)
               (setq d5-snapshot-hook-count 0)
               (setq window-scroll-functions
                     (list (lambda (_window _start)
                             (setq d5-snapshot-hook-count (1+ d5-snapshot-hook-count))
                             (throw 'd5-owned-redisplay 'd5-owned-layout-flow)))))",
        )
        .expect("install visible committed-start hook after start mutation");
    let result = f
        .eval
        .eval_str("(catch 'd5-owned-redisplay (redisplay t) 'd5-hook-was-not-run)")
        .expect("full transaction must propagate the hook's valid throw immediately");
    assert_eq!(result.as_symbol_name(), Some("d5-owned-layout-flow"));
    assert_eq!(hook_count(&mut f.eval), 1);
    assert!(!f.eval.redisplay_hook_flow_pending());
}

#[test]
fn gnu_hidden_frame_snapshot_keeps_fresh_chrome_and_echo_source() {
    let mut f = fixture(true);
    let mini = f
        .eval
        .frame_manager()
        .get(f.hidden_frame)
        .and_then(|frame| frame.minibuffer_window)
        .expect("hidden mini window");
    let mini_buffer = f
        .eval
        .buffer_manager_mut()
        .create_buffer("snapshot-live-mini-buffer");
    f.eval
        .buffer_manager_mut()
        .get_mut(mini_buffer)
        .unwrap()
        .insert("UNRELATED-LIVE-MINI");
    f.eval
        .eval_form(Value::list(vec![
            Value::symbol("set-window-buffer"),
            Value::make_window(mini.0),
            Value::make_buffer(mini_buffer),
        ]))
        .expect("distinct hidden live-mini source");
    f.eval
        .set_current_message(Some(LispString::from_utf8("SNAPSHOT-ECHO-CONTROL")));
    let text = f
        .eval
        .eval_str("(neomacs--frame-snapshot d5-snapshot-hidden-frame 'text)")
        .expect("public hidden-frame echo snapshot")
        .as_str_owned()
        .expect("snapshot text");
    assert!(
        text.contains("SNAPSHOT-HEADER") && text.contains("SNAPSHOT-MODE"),
        "fresh chrome on cold hidden frame: {text}"
    );
    assert!(
        text.contains("SNAPSHOT-ECHO-CONTROL"),
        "snapshot uses the current echo buffer: {text}"
    );
    assert!(
        !text.contains("UNRELATED-LIVE-MINI"),
        "the inactive live mini buffer must not replace echo: {text}"
    );
    assert!(!f.eval.redisplay_hook_flow_pending());
    // Change chrome after the first snapshot. A second public snapshot must
    // render this new row, not suppress chrome like a window-end query.
    f.eval
        .eval_str(
            "(progn
           (setq d5-snapshot-caller-buffer (current-buffer))
           (set-buffer d5-snapshot-hidden-buffer)
           (setq header-line-format \"SNAPSHOT-NEW-HEADER\")
           (set-buffer d5-snapshot-caller-buffer))",
        )
        .expect("update local chrome through the public variable setter");
    let fresh = f
        .eval
        .eval_str("(neomacs--frame-snapshot d5-snapshot-hidden-frame 'text)")
        .expect("public refreshed chrome snapshot")
        .as_str_owned()
        .expect("fresh snapshot text");
    assert!(
        fresh.contains("SNAPSHOT-NEW-HEADER"),
        "updated snapshot chrome: {fresh}"
    );
    assert!(
        fresh.contains("SNAPSHOT-ECHO-CONTROL"),
        "updated snapshot keeps echo source: {fresh}"
    );
}
