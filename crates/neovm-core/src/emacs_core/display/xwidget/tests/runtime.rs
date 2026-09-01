use super::eval::{Context, DisplayHost, GuiFrameHostRequest, XwidgetScriptRequestId};
use super::intern::resolve_sym;
use super::value::{Value, eq_value, list_to_vec};
use crate::heap_types::LispString;
use crate::keyboard::{FrontendWebProcessFailure, FrontendWebValue, FrontendWebViewEvent};
use neomacs_display_protocol::WebViewId;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
enum XwidgetHostEvent {
    Create {
        id: u32,
        width: u32,
        height: u32,
    },
    LoadUri {
        id: u32,
        uri: String,
    },
    Resize {
        id: u32,
        width: u32,
        height: u32,
    },
    ExecuteScript {
        id: u32,
        request: u64,
        script: String,
    },
    Destroy {
        id: u32,
    },
}

#[derive(Clone, Default)]
struct RecordingXwidgetDisplayHost {
    events: Arc<Mutex<Vec<XwidgetHostEvent>>>,
}

impl DisplayHost for RecordingXwidgetDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn create_webkit_xwidget(&self, id: WebViewId, width: u32, height: u32) -> Result<(), String> {
        self.events
            .lock()
            .expect("xwidget host events")
            .push(XwidgetHostEvent::Create {
                id: id.get(),
                width,
                height,
            });
        Ok(())
    }

    fn load_webkit_xwidget_uri(&self, id: WebViewId, uri: LispString) -> Result<(), String> {
        self.events
            .lock()
            .expect("xwidget host events")
            .push(XwidgetHostEvent::LoadUri {
                id: id.get(),
                uri: String::from_utf8_lossy(uri.as_bytes()).into_owned(),
            });
        Ok(())
    }

    fn resize_webkit_xwidget(&self, id: WebViewId, width: u32, height: u32) -> Result<(), String> {
        self.events
            .lock()
            .expect("xwidget host events")
            .push(XwidgetHostEvent::Resize {
                id: id.get(),
                width,
                height,
            });
        Ok(())
    }

    fn execute_webkit_xwidget_script(
        &self,
        id: WebViewId,
        request: XwidgetScriptRequestId,
        script: LispString,
    ) -> Result<(), String> {
        self.events
            .lock()
            .expect("xwidget host events")
            .push(XwidgetHostEvent::ExecuteScript {
                id: id.get(),
                request: request.get(),
                script: String::from_utf8_lossy(script.as_bytes()).into_owned(),
            });
        Ok(())
    }

    fn destroy_webkit_xwidget(&self, id: WebViewId) -> Result<(), String> {
        self.events
            .lock()
            .expect("xwidget host events")
            .push(XwidgetHostEvent::Destroy { id: id.get() });
        Ok(())
    }
}

fn eval(ctx: &mut Context, source: &str) -> Value {
    ctx.eval_str(source).expect("xwidget form should evaluate")
}

fn xwidget_context() -> Context {
    let mut ctx = Context::new();
    ctx.provide_value(Value::symbol("xwidget"), None)
        .expect("provide xwidget in minimal test runtime");
    ctx
}

#[test]
fn stale_ready_event_cannot_replace_the_current_webview_generation() {
    let id = WebViewId::new(41);
    let mut ctx = xwidget_context();

    assert!(
        !ctx.xwidgets
            .apply_frontend_event(&FrontendWebViewEvent::Ready { id, generation: 2 })
            .redisplay_needed()
    );
    assert!(
        !ctx.xwidgets
            .apply_frontend_event(&FrontendWebViewEvent::Ready { id, generation: 1 })
            .redisplay_needed()
    );
    assert!(
        ctx.xwidgets
            .apply_frontend_event(&FrontendWebViewEvent::TitleChanged {
                id,
                generation: 2,
                title: "current".to_owned(),
            })
            .redisplay_needed()
    );
}

#[test]
fn make_xwidget_builds_gnu_model_and_info_vector() {
    crate::test_utils::init_test_tracing();
    let mut ctx = xwidget_context();

    let xwidget = eval(&mut ctx, r#"(make-xwidget 'webkit "Title" 320 200)"#);
    assert!(xwidget.is_xwidget());
    assert!(eval(&mut ctx, "(xwidget-live-p (car xwidget-list))").is_t());

    let info = eval(&mut ctx, "(xwidget-info (car xwidget-list))");
    let slots = info
        .as_vector_data()
        .expect("xwidget-info vector")
        .as_slice();
    assert_eq!(slots.len(), 4);
    assert_eq!(slots[0], Value::symbol("webkit"));
    assert_eq!(slots[1].as_runtime_string_owned().as_deref(), Some("Title"));
    assert_eq!(slots[2], Value::fixnum(320));
    assert_eq!(slots[3], Value::fixnum(200));

    let listed = eval(&mut ctx, "(car (get-buffer-xwidgets (current-buffer)))");
    assert!(eq_value(&listed, &xwidget));
}

#[test]
fn make_xwidget_uses_sixth_arg_as_buffer_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ctx = xwidget_context();

    let result = eval(
        &mut ctx,
        r#"
(let ((current (current-buffer))
      (with-arguments (make-xwidget 'webkit "Args" 10 20 '(ignored args)))
      (with-buffer (make-xwidget 'webkit "Buffer" 10 20 nil "xwidget-target")))
  (list (eq (xwidget-buffer with-arguments) current)
        (buffer-name (xwidget-buffer with-buffer))))
"#,
    );
    let items = list_to_vec(&result).expect("result list");
    assert!(items[0].is_t());
    assert_eq!(items[1].as_utf8_str(), Some("xwidget-target"));
}

#[test]
fn xwidget_public_list_is_not_the_internal_owner_list() {
    crate::test_utils::init_test_tracing();
    let mut ctx = xwidget_context();

    let xwidget = eval(&mut ctx, r#"(make-xwidget 'webkit "Title" 10 20)"#);
    eval(&mut ctx, "(setq xwidget-list nil)");
    let listed = eval(&mut ctx, "(get-buffer-xwidgets (current-buffer))");
    let items = list_to_vec(&listed).expect("proper xwidget list");
    assert_eq!(items.len(), 1);
    assert!(eq_value(&items[0], &xwidget));
}

#[test]
fn xwidget_view_lookup_defaults_an_omitted_or_nil_window_to_the_selected_window() {
    crate::test_utils::init_test_tracing();
    let mut ctx = xwidget_context();

    let result = eval(
        &mut ctx,
        r#"
(let ((xw (make-xwidget 'webkit "Title" 10 20)))
  (list (xwidget-view-lookup xw)
        (xwidget-view-lookup xw nil)))
"#,
    );

    assert_eq!(
        list_to_vec(&result).expect("lookup results"),
        vec![Value::NIL; 2]
    );
}

#[test]
fn xwidget_plist_query_flag_resize_and_kill_follow_gnu_slots() {
    crate::test_utils::init_test_tracing();
    let mut ctx = xwidget_context();

    eval(
        &mut ctx,
        r#"(setq xw-test (make-xwidget 'webkit "Title" 10 20))"#,
    );
    let result = eval(
        &mut ctx,
        r#"
(progn
  (set-xwidget-plist xw-test '(a 1 b 2))
  (set-xwidget-query-on-exit-flag xw-test nil)
  (xwidget-resize xw-test 30 40)
  (list (xwidget-plist xw-test)
        (xwidget-query-on-exit-flag xw-test)
        (xwidget-size-request xw-test)))
"#,
    );
    let items = list_to_vec(&result).expect("proper result list");
    assert_eq!(
        list_to_vec(&items[0]).expect("plist"),
        vec![
            Value::symbol("a"),
            Value::fixnum(1),
            Value::symbol("b"),
            Value::fixnum(2),
        ]
    );
    assert!(items[1].is_nil());
    assert_eq!(
        list_to_vec(&items[2]).expect("size"),
        vec![Value::fixnum(30), Value::fixnum(40)]
    );

    let killed = eval(
        &mut ctx,
        r#"
(progn
  (kill-xwidget xw-test)
  (list (xwidget-live-p xw-test)
        (xwidget-buffer xw-test)
        (get-buffer-xwidgets (current-buffer))))
"#,
    );
    assert_eq!(
        list_to_vec(&killed).expect("kill result"),
        vec![Value::NIL, Value::NIL, Value::NIL]
    );
}

#[test]
fn make_xwidget_accepts_only_gnu_webkit_type() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();

    let err = ctx
        .eval_str(r#"(make-xwidget 'video "Title" 10 20)"#)
        .expect_err("GNU make-xwidget accepts only webkit");
    let super::error::EvalError::Signal { symbol, data, .. } = err else {
        panic!("make-xwidget should signal error");
    };
    assert_eq!(resolve_sym(symbol), "error");
    assert_eq!(data, vec![Value::string("Bad xwidget type")]);
}

#[test]
fn make_xwidget_requires_interned_gnu_webkit_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();

    let err = ctx
        .eval_str(r#"(make-xwidget (make-symbol "webkit") "Title" 10 20)"#)
        .expect_err("GNU make-xwidget compares against Qwebkit by identity");
    let super::error::EvalError::Signal { symbol, data, .. } = err else {
        panic!("make-xwidget should signal error");
    };
    assert_eq!(resolve_sym(symbol), "error");
    assert_eq!(data, vec![Value::string("Bad xwidget type")]);
}

#[test]
fn xwidget_webkit_lifecycle_uses_gnu_model_id() {
    crate::test_utils::init_test_tracing();
    let host = RecordingXwidgetDisplayHost::default();
    let events = Arc::clone(&host.events);
    let mut ctx = xwidget_context();
    ctx.set_display_host(Box::new(host));

    let result = eval(
        &mut ctx,
        r#"
(progn
  (setq xw-test (make-xwidget 'webkit "Title" 10 20))
  (xwidget-webkit-goto-uri xw-test "https://example.com")
  (xwidget-resize xw-test 30 40)
  (prog1
      (list (xwidget-webkit-uri xw-test)
            (xwidget-webkit-title xw-test))
    (kill-xwidget xw-test)))
"#,
    );

    let values = list_to_vec(&result).expect("result list");
    assert_eq!(values[0].as_utf8_str(), Some("https://example.com"));
    assert_eq!(values[1].as_utf8_str(), Some(""));

    assert_eq!(
        *events.lock().expect("xwidget host events"),
        vec![
            XwidgetHostEvent::Create {
                id: 1,
                width: 10,
                height: 20,
            },
            XwidgetHostEvent::LoadUri {
                id: 1,
                uri: "https://example.com".to_owned(),
            },
            XwidgetHostEvent::Resize {
                id: 1,
                width: 30,
                height: 40,
            },
            XwidgetHostEvent::Destroy { id: 1 },
        ]
    );
}

/// Dispatch establishes the immediate optimistic progress value. Measured
/// frontend events are covered separately by the generation-state test.
#[test]
fn xwidget_webkit_load_progress_is_dispatched_not_measured() {
    crate::test_utils::init_test_tracing();
    let mut ctx = xwidget_context();
    ctx.set_display_host(Box::new(RecordingXwidgetDisplayHost::default()));

    let result = eval(
        &mut ctx,
        r#"
(let ((xw (make-xwidget 'webkit "Title" 10 20)))
  (prog1
      (list (xwidget-webkit-estimated-load-progress xw)
            (progn (xwidget-webkit-goto-uri xw "https://example.com")
                   (xwidget-webkit-estimated-load-progress xw)))
    (kill-xwidget xw)))
"#,
    );
    let values = list_to_vec(&result).expect("result list");
    assert_eq!(values[0].as_float(), Some(0.0), "before any navigation");
    assert_eq!(values[1].as_float(), Some(1.0), "once one is dispatched");
}

/// GNU retains FUN until asynchronous JavaScript success, then invokes it on
/// the evaluator event loop with the converted result.
#[test]
fn xwidget_webkit_execute_script_accepts_fun_and_routes_both_forms() {
    crate::test_utils::init_test_tracing();
    let host = RecordingXwidgetDisplayHost::default();
    let events = Arc::clone(&host.events);
    let mut ctx = xwidget_context();
    ctx.set_display_host(Box::new(host));

    let result = eval(
        &mut ctx,
        r#"
(let ((xw (make-xwidget 'webkit "Title" 10 20)))
  (prog1
      (list (condition-case e
                (xwidget-webkit-execute-script xw "1 + 1" (lambda (_value) nil))
              (error (car e)))
            (xwidget-webkit-execute-script xw "window.scrollTo(0, 0)"))
    (kill-xwidget xw)))
"#,
    );
    let values = list_to_vec(&result).expect("result list");
    assert!(values[0].is_nil(), "GNU accepts FUN and returns nil");
    assert!(values[1].is_nil(), "without FUN the subr returns nil");

    let recorded = events.lock().expect("xwidget host events");
    let scripts: Vec<&XwidgetHostEvent> = recorded
        .iter()
        .filter(|e| matches!(e, XwidgetHostEvent::ExecuteScript { .. }))
        .collect();
    assert_eq!(
        scripts,
        vec![
            &XwidgetHostEvent::ExecuteScript {
                id: 1,
                request: 1,
                script: "1 + 1".to_owned(),
            },
            &XwidgetHostEvent::ExecuteScript {
                id: 1,
                request: 2,
                script: "window.scrollTo(0, 0)".to_owned(),
            },
        ],
        "both callback and fire-and-forget scripts reach the host"
    );
}

#[test]
fn xwidget_webkit_execute_script_invokes_fun_after_success() {
    crate::test_utils::init_test_tracing();
    let host = RecordingXwidgetDisplayHost::default();
    let mut ctx = xwidget_context();
    ctx.set_display_host(Box::new(host));

    eval(
        &mut ctx,
        r##"
(setq xw-script-result 'pending
      xw-script-test (make-xwidget 'webkit "Title" 10 20))
"##,
    );
    ctx.apply_xwidget_frontend_event(&FrontendWebViewEvent::Ready {
        id: WebViewId::new(1),
        generation: 1,
    })
    .unwrap();
    let result = eval(
        &mut ctx,
        r##"
(xwidget-webkit-execute-script
 xw-script-test
 "1 + 1"
 (lambda (value) (setq xw-script-result value)))
"##,
    );
    assert!(result.is_nil());

    assert!(
        ctx.apply_xwidget_frontend_event(&FrontendWebViewEvent::ScriptFinished {
            view: WebViewId::new(1),
            generation: 1,
            request: 1,
            result: Ok(FrontendWebValue::Number(2.0)),
        })
        .unwrap(),
        "callback execution can mutate displayed Lisp state"
    );
    assert_eq!(eval(&mut ctx, "xw-script-result").as_float(), Some(2.0));
    eval(&mut ctx, "(kill-xwidget xw-script-test)");
}

#[test]
fn web_process_failure_discards_pending_script_callbacks() {
    crate::test_utils::init_test_tracing();
    let mut ctx = xwidget_context();
    ctx.set_display_host(Box::new(RecordingXwidgetDisplayHost::default()));
    eval(
        &mut ctx,
        r##"
(setq xw-script-result 'pending
      xw-script-test (make-xwidget 'webkit "Title" 10 20))
"##,
    );
    ctx.apply_xwidget_frontend_event(&FrontendWebViewEvent::Ready {
        id: WebViewId::new(1),
        generation: 1,
    })
    .unwrap();
    eval(
        &mut ctx,
        r##"
(xwidget-webkit-execute-script
 xw-script-test
 "slowOperation()"
 (lambda (value) (setq xw-script-result value)))
"##,
    );

    assert!(
        !ctx.apply_xwidget_frontend_event(&FrontendWebViewEvent::ProcessFailed {
            id: WebViewId::new(1),
            generation: 1,
            failure: FrontendWebProcessFailure::Crashed,
        })
        .unwrap()
    );
    assert!(
        !ctx.apply_xwidget_frontend_event(&FrontendWebViewEvent::ScriptFinished {
            view: WebViewId::new(1),
            generation: 1,
            request: 1,
            result: Ok(FrontendWebValue::String("too late".to_owned())),
        })
        .unwrap()
    );
    assert!(eq_value(
        &eval(&mut ctx, "xw-script-result"),
        &Value::symbol("pending")
    ));
    eval(&mut ctx, "(kill-xwidget xw-script-test)");
}
