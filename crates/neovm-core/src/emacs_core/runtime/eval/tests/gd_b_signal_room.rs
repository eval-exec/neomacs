use super::*;

#[cfg(feature = "jit")]
use super::gd_b_native::{RawCallTestPolicy, assert_native_frames_warmed};

#[test]
fn depth_limit_signal_hook_has_reserved_evaluator_room() {
    let mut ev = Context::new();
    ev.eval_str(
        "(defvar gd-b-depth-seen nil)
         (setq signal-hook-function
               (lambda (_kind _data)
                 (setq gd-b-depth-seen max-lisp-eval-depth)))",
    )
    .unwrap();
    ev.set_variable("max-lisp-eval-depth", Value::fixnum(100));
    ev.depth = 100;
    let result = ev.dispatch_signal_flow_cold(signal("error", vec![Value::string("depth")]));
    ev.depth = 0;
    assert!(result.is_err());
    assert_eq!(ev.eval_str("gd-b-depth-seen").unwrap(), Value::fixnum(120));
    assert_eq!(ev.max_depth, 100);
    assert_eq!(
        ev.eval_str("lisp-eval-depth-reserve").unwrap(),
        Value::fixnum(200)
    );

    ev.eval_str(
        "(setq signal-hook-function nil)
         (defalias 'gd-b-depth-handler
           (lambda (_error)
             (setq gd-b-depth-seen
                   (list max-lisp-eval-depth lisp-eval-depth-reserve))))",
    )
    .unwrap();
    let handler = ev.obarray.symbol_function("gd-b-depth-handler").unwrap();
    ev.push_condition_frame(ConditionFrame::HandlerBind {
        conditions: Value::symbol("error"),
        handler,
        mute_span: 0,
    });
    ev.depth = 100;
    assert!(
        ev.dispatch_signal_flow_cold(signal("error", Vec::new()))
            .is_err()
    );
    ev.depth = 0;
    ev.pop_condition_frame();
    assert_eq!(
        format_eval_result(&ev.eval_str("gd-b-depth-seen")),
        "OK (300 0)"
    );
    assert_eq!(ev.max_depth, 100);
    assert_eq!(
        ev.eval_str("lisp-eval-depth-reserve").unwrap(),
        Value::fixnum(200)
    );

    ev.eval_str(
        "(setq debugger
               (lambda (&rest _args)
                 (list max-lisp-eval-depth lisp-eval-depth-reserve)))",
    )
    .unwrap();
    ev.depth = 100;
    let result = ev.call_debugger(vec![Value::symbol("error"), Value::NIL]);
    ev.depth = 0;
    assert_eq!(
        crate::emacs_core::print::print_value(&result.unwrap()),
        "(300 0)"
    );
    assert_eq!(ev.max_depth, 100);
    assert_eq!(
        ev.eval_str("lisp-eval-depth-reserve").unwrap(),
        Value::fixnum(200)
    );
}

#[test]
fn nested_signal_hook_replacement_restores_evaluator_reserve() {
    let mut ev = Context::new();
    ev.eval_str(
        "(defvar gd-b-depth-count 0)
         (setq signal-hook-function
               (lambda (_kind _data)
                 (if (= (setq gd-b-depth-count (1+ gd-b-depth-count)) 1)
                   (signal 'file-error '(\"from hook\")))))",
    )
    .unwrap();
    ev.set_variable("max-lisp-eval-depth", Value::fixnum(100));
    let spec_base = ev.specpdl.len();
    ev.depth = 100;
    let result = ev.dispatch_signal_flow_cold(signal("error", vec![Value::string("depth")]));
    assert_eq!(ev.depth, 100);
    assert_eq!(ev.max_depth, 100);
    assert_eq!(ev.specpdl.len(), spec_base);
    assert!(matches!(
        result.kinded(),
        Err(FlowKind::Signal(sig)) if sig.symbol_name() == "file-error"
    ));
    ev.depth = 0;
    ev.set_variable("signal-hook-function", Value::NIL);
    assert_eq!(ev.eval_str("gd-b-depth-count").unwrap(), Value::fixnum(2));
    assert_eq!(
        ev.eval_str("lisp-eval-depth-reserve").unwrap(),
        Value::fixnum(200)
    );
}

#[test]
fn signal_hook_throw_restores_evaluator_reserve() {
    let mut ev = Context::new();
    ev.eval_str(
        "(setq signal-hook-function
               (lambda (_kind _data) (throw 'gd-b-hook-exit 'escaped)))",
    )
    .unwrap();
    ev.push_condition_frame(ConditionFrame::Catch {
        tag: Value::symbol("gd-b-hook-exit"),
        resume: ResumeTarget::InterpreterCatch,
    });
    ev.set_variable("max-lisp-eval-depth", Value::fixnum(100));
    let spec_base = ev.specpdl.len();
    ev.depth = 100;
    let result = ev.dispatch_signal_flow_cold(signal("error", vec![Value::string("depth")]));
    assert_eq!(ev.depth, 100);
    assert_eq!(ev.max_depth, 100);
    assert_eq!(ev.specpdl.len(), spec_base);
    assert!(matches!(
        result.kinded(),
        Err(FlowKind::Throw(thrown))
            if thrown.tag == Value::symbol("gd-b-hook-exit")
                && thrown.value == Value::symbol("escaped")
    ));
    ev.depth = 0;
    ev.pop_condition_frame();
    ev.set_variable("signal-hook-function", Value::NIL);
    assert_eq!(
        ev.eval_str("lisp-eval-depth-reserve").unwrap(),
        Value::fixnum(200)
    );
}

#[test]
fn signal_hook_limit_change_returns_current_excess_to_reserve() {
    let mut ev = Context::new();
    ev.eval_str(
        "(setq signal-hook-function
               (lambda (_kind _data)
                 (setq max-lisp-eval-depth (+ max-lisp-eval-depth 7))))",
    )
    .unwrap();
    ev.set_variable("max-lisp-eval-depth", Value::fixnum(100));
    ev.depth = 100;
    let result = ev.dispatch_signal_flow_cold(signal("error", vec![Value::string("depth")]));
    assert!(result.is_err());
    assert_eq!(ev.depth, 100);
    assert_eq!(ev.max_depth, 100);
    ev.depth = 0;
    ev.set_variable("signal-hook-function", Value::NIL);
    assert_eq!(
        ev.eval_str("max-lisp-eval-depth").unwrap(),
        Value::fixnum(100)
    );
    assert_eq!(
        ev.eval_str("lisp-eval-depth-reserve").unwrap(),
        Value::fixnum(207)
    );
}

#[test]
fn localized_signal_depth_reserve_updates_the_current_binding_without_watchers() {
    let mut ev = Context::new();
    ev.eval_str(
        "(defvar gd-b-reserve-writes nil)
         (set (make-local-variable 'max-lisp-eval-depth) 100)
         (set (make-local-variable 'lisp-eval-depth-reserve) 200)
         (defalias 'gd-b-reserve-watch
           (lambda (_symbol _value operation _where)
             (setq gd-b-reserve-writes (cons operation gd-b-reserve-writes))))
         (add-variable-watcher 'max-lisp-eval-depth #'gd-b-reserve-watch)
         (add-variable-watcher 'lisp-eval-depth-reserve #'gd-b-reserve-watch)
         (setq debugger
               (lambda (&rest _args)
                 (list max-lisp-eval-depth lisp-eval-depth-reserve
                       gd-b-reserve-writes)))",
    )
    .unwrap();
    ev.depth = 50;
    let result = ev.call_debugger(vec![Value::symbol("error"), Value::NIL]);
    assert_eq!(ev.depth, 50);
    ev.depth = 0;
    assert_eq!(
        crate::emacs_core::print::print_value(&result.unwrap()),
        "(250 50 nil)"
    );
    assert_eq!(ev.max_depth, 100);
    assert_eq!(
        format_eval_result(
            &ev.eval_str("(list max-lisp-eval-depth lisp-eval-depth-reserve gd-b-reserve-writes)")
        ),
        "OK (100 200 nil)"
    );
}

#[test]
fn localized_signal_depth_restore_uses_the_buffer_selected_by_the_hook() {
    let mut ev = Context::new();
    ev.eval_str(
        "(defvar gd-b-reserve-source (get-buffer-create \" *gd-b-reserve-source*\"))
         (defvar gd-b-reserve-target (get-buffer-create \" *gd-b-reserve-target*\"))
         (defvar gd-b-reserve-seen nil)
         (set-buffer gd-b-reserve-source)
         (set (make-local-variable 'max-lisp-eval-depth) 100)
         (set (make-local-variable 'lisp-eval-depth-reserve) 200)
         (set-buffer gd-b-reserve-target)
         (set (make-local-variable 'max-lisp-eval-depth) 150)
         (set (make-local-variable 'lisp-eval-depth-reserve) 40)
         (set-buffer gd-b-reserve-source)
         (setq signal-hook-function
               (lambda (_kind _data)
                 (set-buffer gd-b-reserve-target)
                 (setq gd-b-reserve-seen
                       (list max-lisp-eval-depth lisp-eval-depth-reserve))
                 (setq max-lisp-eval-depth (+ max-lisp-eval-depth 7)
                       lisp-eval-depth-reserve (+ lisp-eval-depth-reserve 3))))",
    )
    .unwrap();
    ev.depth = 100;
    let result = ev.dispatch_signal_flow_cold(signal("error", vec![Value::string("depth")]));
    assert!(result.is_err());
    assert_eq!(ev.depth, 100);
    assert_eq!(ev.max_depth, 100);
    ev.depth = 0;
    ev.set_variable("signal-hook-function", Value::NIL);
    assert_eq!(
        format_eval_result(&ev.eval_str(
            "(list gd-b-reserve-seen
                   (eq (current-buffer) gd-b-reserve-target)
                   max-lisp-eval-depth lisp-eval-depth-reserve
                   (buffer-local-value 'max-lisp-eval-depth gd-b-reserve-source)
                   (buffer-local-value 'lisp-eval-depth-reserve gd-b-reserve-source))"
        )),
        "OK ((150 40) t 100 100 120 180)"
    );
}

/// GNU eval.c:268-271 leaves the reserve untouched when sufficient room exists;
/// eval.c:1968-1975 still calls the hook with the current buffer's bindings.
/// Warm the compiled reader in another buffer so skipping reserve-cache preload
/// must still let its BLV guard/fallback observe the current reserve.
#[cfg(feature = "jit")]
#[test]
fn compiled_no_borrow_signal_hook_reads_current_localized_reserve_without_watchers() {
    let _policy = RawCallTestPolicy::enter();
    let mut ev = runtime_startup_context();
    ev.eval_str(
        r#"(defvar gd-b-no-borrow-source
             (get-buffer-create " *gd-b-no-borrow-source*"))
           (defvar gd-b-no-borrow-target
             (get-buffer-create " *gd-b-no-borrow-target*"))
           (defvar gd-b-no-borrow-seen nil)
           (defvar gd-b-no-borrow-writes nil)
           (setq signal-hook-function nil debug-on-error nil debug-on-signal nil)
           (set-buffer gd-b-no-borrow-source)
           (set (make-local-variable 'max-lisp-eval-depth) 2048)
           (set (make-local-variable 'lisp-eval-depth-reserve) 13)
           (set-buffer gd-b-no-borrow-target)
           (set (make-local-variable 'max-lisp-eval-depth) 4096)
           (set (make-local-variable 'lisp-eval-depth-reserve) 29)
           (set-buffer gd-b-no-borrow-source)
           (defalias 'gd-b-no-borrow-watch
             (lambda (_symbol _value operation _where)
               (setq gd-b-no-borrow-writes
                     (cons operation gd-b-no-borrow-writes))))
           (add-variable-watcher 'max-lisp-eval-depth #'gd-b-no-borrow-watch)
           (add-variable-watcher 'lisp-eval-depth-reserve #'gd-b-no-borrow-watch)
           (defalias 'gd-b-no-borrow-hook
             (eval '(byte-compile
                      (lambda (_kind _data)
                        (setq gd-b-no-borrow-seen
                              (list (eq (current-buffer) gd-b-no-borrow-target)
                                    max-lisp-eval-depth lisp-eval-depth-reserve))))
                   t))
           ;; Admit this same named observer natively while its BLV cache is in source.
           ;; These successful calls create no signal and perform no watched writes.
           (dotimes (_ 2500) (gd-b-no-borrow-hook nil nil))
           (setq gd-b-no-borrow-seen nil gd-b-no-borrow-writes nil)"#,
    )
    .unwrap();
    assert_native_frames_warmed(&ev, &["gd-b-no-borrow-hook"]);
    // Buffer switching does not preload the reserve BLV. Leave the hook's
    // last reserve read in source, and do not inspect it before dispatch.
    ev.eval_str("(set-buffer gd-b-no-borrow-target)").unwrap();
    let state = (
        ev.depth,
        ev.specpdl.len(),
        ev.condition_stack.len(),
        ev.bc_buf.len(),
    );
    let result = ev.eval_str(
        r#"(let ((signal-hook-function #'gd-b-no-borrow-hook))
             (let ((result (condition-case nil
                               (signal 'error '("gdB no-borrow"))
                             (error 'caught))))
               (list result gd-b-no-borrow-seen gd-b-no-borrow-writes
                     (eq (current-buffer) gd-b-no-borrow-target)
                     max-lisp-eval-depth lisp-eval-depth-reserve
                     (buffer-local-value 'max-lisp-eval-depth gd-b-no-borrow-source)
                     (buffer-local-value 'lisp-eval-depth-reserve gd-b-no-borrow-source))))"#,
    );
    // GNU Emacs 31.1, confirmed by the no-borrow localized-reserve probe.
    assert_eq!(
        format_eval_result(&result),
        "OK (caught (t 4096 29) nil t 4096 29 2048 13)"
    );
    assert_eq!(
        (
            ev.depth,
            ev.specpdl.len(),
            ev.condition_stack.len(),
            ev.bc_buf.len(),
        ),
        state,
        "a no-borrow hook must not leave a depth, handler or root-stack extent"
    );
}
