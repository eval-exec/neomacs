use crate::emacs_core::Context;
use crate::emacs_core::eval::redisplay_hooks::*;
use crate::emacs_core::eval::*;
use crate::window::DisplayFrameManagerExt;
use neovm_value::display_vocab::FrameId;
use neovm_value::value::Value;

fn mini_only_context() -> (Context, FrameId) {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let frame = eval
        .frame_manager_mut()
        .create_frame("mini-only", 80, 25, buffer);
    let state = eval
        .frame_manager_mut()
        .get_mut(frame)
        .expect("created frame");
    state.minibuffer_window = Some(state.root_window().id());
    state.minibuffer_leaf = None;
    state.visibility = crate::window::FrameVisibility::Visible;
    (eval, frame)
}

fn hook_events(eval: &Context) -> Vec<Value> {
    let mut events = Vec::new();
    let mut tail = eval
        .obarray()
        .symbol_value_copied("mini-core-log")
        .expect("event log");
    while tail.is_cons() {
        events.push(tail.cons_car());
        tail = tail.cons_cdr();
    }
    events.reverse();
    events
}

#[test]
fn selected_mini_only_fallback_observes_dynamic_policy_and_precedes_hooks() {
    crate::test_utils::init_test_tracing();
    let _policy = RedisplayHookPolicyGuard::gnu();
    // A normal inactive mini has no selected-mini request, even with sizing
    // enabled. Its preparation producer must not be entered at all.
    {
        let mut ordinary = crate::test_utils::new_context();
        let (frames, buffers) = ordinary.frame_manager_and_buffers_mut();
        crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(frames, buffers);
        ordinary
            .eval_str("(setq resize-mini-frames t)")
            .expect("ordinary policy");
        ordinary.redisplay_prepare_fn = Some(Box::new(|_, _| {
            panic!("inactive ordinary mini must stay on the fast path")
        }));
        ordinary.redisplay_fn = Some(Box::new(|_| {}));
        ordinary
            .redisplay_with_force_flow(true)
            .expect("ordinary redisplay");
        assert!(ordinary.redisplay_prepare_fn.is_some());
    }

    let (mut eval, frame) = mini_only_context();
    assert!(!eval.minibuffer_is_active());
    assert!(!eval.has_current_message());
    assert!(eval.redisplay_prepare_fn.is_none());
    eval.eval_str(
        "(progn
           (setq mini-core-log nil mini-core-resize-count 0
                 mini-core-resize-arg nil mini-core-dynamic nil
                 resize-mini-frames nil
                 pre-redisplay-function (lambda (_targets) (setq mini-core-log (cons 'pre mini-core-log)))
                 window-state-change-functions
                 (list (lambda (_frame) (setq mini-core-log (cons 'change mini-core-log)))))
           (fset 'window--resize-mini-frame
                 (lambda (frame)
                   (setq mini-core-resize-count (1+ mini-core-resize-count)
                         mini-core-resize-arg frame
                         mini-core-dynamic (and resize-mini-frames inhibit-redisplay)
                         mini-core-log (cons 'resize mini-core-log)))))",
    ).expect("mini-only callbacks");
    eval.redisplay_fn = Some(Box::new(|eval| {
        let tail = eval
            .obarray()
            .symbol_value_copied("mini-core-log")
            .expect("event log");
        eval.obarray_mut()
            .set_symbol_value("mini-core-log", Value::cons(Value::symbol("paint"), tail));
    }));
    eval.eval_str("(let ((resize-mini-frames t)) (redisplay t))")
        .expect("mini-only redisplay");
    assert_eq!(
        eval.obarray().symbol_value_copied("mini-core-resize-count"),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        eval.obarray().symbol_value_copied("mini-core-resize-arg"),
        Some(Value::make_frame(frame.0))
    );
    assert_eq!(
        eval.obarray().symbol_value_copied("mini-core-dynamic"),
        Some(Value::T)
    );
    assert_eq!(
        eval.obarray().symbol_value_copied("resize-mini-frames"),
        Some(Value::NIL)
    );
    let events = hook_events(&eval);
    let event_index = |name| {
        events
            .iter()
            .position(|event| *event == Value::symbol(name))
            .expect("event occurred")
    };
    assert!(event_index("pre") < event_index("resize"));
    assert!(event_index("resize") < event_index("change"));
    assert!(event_index("change") < event_index("paint"));
}

#[test]
fn mini_only_fallback_throw_restores_transaction_without_paint() {
    crate::test_utils::init_test_tracing();
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, _) = mini_only_context();
    eval.eval_str(
        "(progn
           (setq resize-mini-frames nil)
           (fset 'window--resize-mini-frame
                 (lambda (_frame) (throw 'mini-core-transfer 'mini-core-payload))))",
    )
    .expect("throwing mini callback");
    let paints = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = paints.clone();
    eval.redisplay_fn = Some(Box::new(move |_| observed.set(observed.get() + 1)));
    let form = crate::emacs_core::value_reader::read_all(
        "(let ((resize-mini-frames t)) (redisplay t))",
        eval.variables(),
    )
    .expect("dynamic mini form")
    .pop()
    .expect("one form");
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(form);
    let bindings = eval.specpdl_len_for_test();
    // Install the same handler as sf_catch_value before entering a safe
    // callback. Without it GNU throw signals no-catch, which safe_funcall
    // correctly demotes; the unit must observe a valid nonlocal transfer.
    let handlers = eval.condition_stack_for_test().len();
    eval.push_condition_frame(ConditionFrame::Catch {
        tag: Value::symbol("mini-core-transfer"),
        resume: ResumeTarget::InterpreterCatch,
    });
    let result = eval.eval_value(&form);
    eval.pop_condition_frame();
    assert_eq!(eval.condition_stack_for_test().len(), handlers);
    let flow = result.expect_err("mini sizing throw escapes");
    let thrown = flow.as_throw().expect("ordinary nonlocal throw");
    assert_eq!(thrown.tag, Value::symbol("mini-core-transfer"));
    assert_eq!(thrown.value, Value::symbol("mini-core-payload"));
    assert_eq!(paints.get(), 0);
    assert!(!eval.gnu_redisplay_hooks_active());
    assert!(
        eval.gnu_redisplay_hooks_dimensions_empty(),
        "change hooks must not record a pass after earlier sizing throws"
    );
    assert_eq!(eval.gnu_redisplay_hooks_windows_are_all(), true);
    assert!(eval.redisplay_fn.is_some());
    assert!(eval.redisplay_prepare_fn.is_none());
    assert_eq!(eval.specpdl_len_for_test(), bindings);
    assert_eq!(
        eval.obarray().symbol_value_copied("resize-mini-frames"),
        Some(Value::NIL)
    );
    assert_eq!(
        eval.obarray().symbol_value_copied("inhibit-redisplay"),
        Some(Value::NIL)
    );
    eval.restore_specpdl_roots(roots);
}
