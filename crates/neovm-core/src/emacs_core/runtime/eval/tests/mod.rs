use super::*;
use crate::buffer::EmacsByteRange;
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{ConditionFrame, ResumeTarget, SpecBinding};
use crate::emacs_core::format_eval_result;
use crate::emacs_core::subr::{FixedMin1, NativeFn, SubrArity, SubrSpec};
use crate::heap_types::LispString;
use crate::test_utils::{
    eval_with_ldefs_boot_autoloads, load_minimal_gnu_backquote_runtime, runtime_startup_context,
    runtime_startup_eval_all,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn eval_one(src: &str) -> String {
    let mut ev = Context::new();
    let result = ev.eval_str(src);
    format_eval_result(&result)
}

fn eval_one_lexical(src: &str) -> String {
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let result = ev.eval_str(src);
    format_eval_result(&result)
}

fn install_global_map_for_test(ev: &mut Context, global_map: Value) {
    ev.assign("global-map", global_map);
    ev.select_global_map(global_map);
}

#[test]
fn c_level_defsym_hook_names_are_in_global_obarray() {
    let ev = Context::new();

    for name in [
        "insert-in-front-hooks",
        "insert-behind-hooks",
        "long-line-optimizations-in-command-hooks",
    ] {
        assert!(
            ev.obarray().intern_soft(name).is_some(),
            "{name} should be globally interned like GNU DEFSYM"
        );
    }

    assert!(
        ev.obarray()
            .intern_soft("mouse-leave-buffer-hook")
            .is_some(),
        "mouse-leave-buffer-hook should be globally interned like GNU DEFVAR_LISP"
    );
    assert_eq!(
        *ev.obarray()
            .symbol_value("mouse-leave-buffer-hook")
            .unwrap_or(&Value::UNBOUND),
        Value::NIL
    );
}

#[test]
fn erase_buffer_is_disabled_by_c_level_initialization() {
    let mut ev = Context::new();

    assert_eq!(
        ev.eval_str("(get 'erase-buffer 'disabled)")
            .expect("erase-buffer disabled property probe"),
        Value::T
    );
}

#[test]
fn x_selection_hooks_match_gnu_xselect_startup_bindings() {
    let mut ev = Context::new();
    let result = ev
        .eval_str(
            r#"(list (boundp 'emacs-clipboard-manager-exit-hook)
                     (boundp 'x-lost-selection-functions)
                     (boundp 'x-sent-selection-functions)
                     x-lost-selection-functions
                     x-sent-selection-functions
                     (special-variable-p 'x-lost-selection-functions)
                     (special-variable-p 'x-sent-selection-functions))"#,
        )
        .expect("x selection hook probe should evaluate");

    assert_eq!(
        result,
        Value::list(vec![
            Value::NIL,
            Value::T,
            Value::T,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::T,
        ])
    );
}

#[test]
fn mapc_mapconcat_and_mapcan_signal_circular_list_like_gnu() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        eval_one(
            "(condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (mapc (lambda (_) nil) x)) (error (car e)))"
        ),
        "OK circular-list"
    );
    assert_eq!(
        eval_one(
            "(condition-case e (let ((x (list \"a\" \"b\"))) (setcdr (cdr x) x) (mapconcat 'identity x \"-\")) (error (car e)))"
        ),
        "OK circular-list"
    );
    assert_eq!(
        eval_one(
            "(condition-case e (let ((x (list 1 2))) (setcdr (cdr x) x) (mapcan (lambda (v) (list v)) x)) (error (car e)))"
        ),
        "OK circular-list"
    );
}

#[test]
fn sort_signals_circular_list_like_gnu() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        eval_one(
            "(condition-case e (let ((x (list 2 1))) (setcdr (cdr x) x) (sort x (function <))) (error (car e)))"
        ),
        "OK circular-list"
    );
}

fn eval_all(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    let forms = crate::emacs_core::value_reader::read_all(src, &test_ob()).expect("parse");
    // Root all parsed forms across the eval loop. Without rooting,
    // any intervening GC reclaims the cons cells in the unrooted
    // `forms` Vec<Value> (malloc heap, invisible to conservative
    // stack scanning).
    let roots = ev.save_specpdl_roots();
    for form in &forms {
        ev.push_specpdl_root(*form);
    }
    let result = forms
        .iter()
        .map(|form| {
            let result = ev.eval_form(*form);
            format_eval_result(&result)
        })
        .collect();
    ev.restore_specpdl_roots(roots);
    result
}

fn eval_one_with_frame(src: &str) -> String {
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    ev.frames.create_frame("F1", 800, 600, buf);
    // These tests exercise `make-frame`, which requires a usable terminal (in
    // production --batch deliberately has none, so it errors like GNU).
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    let result = ev.eval_str(src);
    format_eval_result(&result)
}

fn eval_all_with_subr(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    load_minimal_gnu_backquote_runtime(&mut ev);
    ev.eval_str_each(&src)
        .iter()
        .map(format_eval_result)
        .collect()
}

fn eval_one_with_subr(src: &str) -> String {
    eval_all_with_subr(src).into_iter().next().expect("result")
}

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

fn bootstrap_eval_one(src: &str) -> String {
    bootstrap_eval_all(src).into_iter().next().expect("result")
}

#[test]
fn symbols_with_pos_enabled_makes_lisp_comparison_primitives_transparent() {
    let result = eval_one(
        r#"(progn
             (setq symbols-with-pos-enabled t)
             (let* ((head (position-symbol 'indent 42))
                    (items (list 'indent))
                    (alist (list (cons 'indent 'ok)))
                    (rlist (list (cons 'ok 'indent))))
               (list
                (eq head 'indent)
                (eql head 'indent)
                (equal head 'indent)
                (memq head items)
                (memql head items)
                (member head items)
                (assq head alist)
                (assoc head alist)
                (rassq head rlist)
                (rassoc head rlist)
                (delq head (list 'a 'indent 'b))
                (delete head (list 'a 'indent 'b)))))"#,
    );

    assert_eq!(
        result,
        "OK (t t t (indent) (indent) (indent) (indent . ok) (indent . ok) (ok . indent) (ok . indent) (a b) (a b))"
    );
}

#[test]
fn memq_and_assq_signal_circular_list_like_gnu() {
    assert_eq!(
        eval_all(
            r#"(let ((x (list 1 2 3)))
                 (setcdr (cdr (cdr x)) x)
                 (condition-case err
                     (memq 9 x)
                   (error (car err))))
               (let ((x (list (cons 1 2) (cons 3 4))))
                 (setcdr (cdr x) x)
                 (condition-case err
                     (assq 9 x)
                   (error (car err))))"#
        ),
        vec!["OK circular-list", "OK circular-list"]
    );
}

#[test]
fn keywordp_treats_positioned_keywords_like_gnu_when_enabled() {
    let result = eval_one(
        r#"(let ((pos-kw (position-symbol :neo-keyword 42)))
             (list
              (let ((symbols-with-pos-enabled t))
                (list (symbolp pos-kw) (keywordp pos-kw) (eq pos-kw :neo-keyword)))
              (let ((symbols-with-pos-enabled nil))
                (list (symbolp pos-kw) (keywordp pos-kw) (eq pos-kw :neo-keyword)))))"#,
    );

    assert_eq!(result, "OK ((t t t) (nil nil nil))");
}

#[test]
fn positioned_lambda_arguments_bind_bare_symbol_references() {
    let result = eval_one(
        r#"(let* ((symbols-with-pos-enabled t)
                 (a (position-symbol 'a 11))
                 (b (position-symbol 'b 22))
                 (r (position-symbol 'r 33))
                 (opt (position-symbol '&optional 44))
                 (rest (position-symbol '&rest 55))
                 (arglist (list a opt b rest r))
                 (body (list 'list 'a 'b 'r))
                 (f (eval (list 'function (list 'lambda arglist body)) t)))
            (funcall f 1 2 3 4))"#,
    );

    assert_eq!(result, "OK (1 2 (3 4))");
}

#[test]
fn symbols_with_pos_enabled_makes_hash_table_keys_transparent() {
    let result = eval_one(
        r#"(progn
             (setq symbols-with-pos-enabled t)
             (let* ((head (position-symbol 'indent 42))
                    (eqtab (make-hash-table :test 'eq))
                    (eqltab (make-hash-table :test 'eql))
                    (equaltab (make-hash-table :test 'equal)))
               (puthash head 'pos eqtab)
               (puthash 'indent 'bare eqtab)
               (puthash head 'pos eqltab)
               (puthash 'indent 'bare eqltab)
               (puthash head 'pos equaltab)
               (puthash 'indent 'bare equaltab)
               (let ((before
                      (list
                       (hash-table-count eqtab)
                       (gethash head eqtab)
                       (gethash 'indent eqtab)
                       (hash-table-count eqltab)
                       (gethash head eqltab)
                       (gethash 'indent eqltab)
                       (hash-table-count equaltab)
                       (gethash head equaltab)
                       (gethash 'indent equaltab))))
                 (remhash head eqtab)
                 (remhash head eqltab)
                 (remhash head equaltab)
                 (append before
                         (list
                          (hash-table-count eqtab)
                          (hash-table-count eqltab)
                          (hash-table-count equaltab))))))"#,
    );

    assert_eq!(result, "OK (1 bare bare 1 bare bare 1 bare bare 0 0 0)");
}

#[test]
fn symbols_with_pos_enabled_makes_plist_keys_transparent() {
    let result = eval_one(
        r#"(let ((symbols-with-pos-enabled t)
                 (plist (list (position-symbol :group 1) 'mode-line-faces
                              (position-symbol :version 2) "30.1")))
             (list
              (plist-get plist :version)
              (plist-get plist (position-symbol :group 3))
              (eq (car (plist-member plist :version)) :version)
              (car (cdr (plist-member plist (position-symbol :version 4))))
              (progn
                (setq plist (plist-put plist :version "31.1"))
                (list (plist-get plist (position-symbol :version 5))
                      (length plist)))))"#,
    );

    assert_eq!(
        result,
        "OK (\"30.1\" mode-line-faces t \"30.1\" (\"31.1\" 4))"
    );
}

#[test]
fn get_honors_overriding_plist_environment() {
    let result = eval_one(
        r#"(progn
             (put 'neo-plist-probe 'pcase-macroexpander 'obarray)
             (list
              (let ((overriding-plist-environment
                     '((neo-plist-probe pcase-macroexpander override))))
                (get 'neo-plist-probe 'pcase-macroexpander))
              (let ((overriding-plist-environment
                     '((neo-plist-probe pcase-macroexpander nil))))
                (get 'neo-plist-probe 'pcase-macroexpander))))"#,
    );

    assert_eq!(result, "OK (override obarray)");
}

#[test]
fn get_and_put_accept_non_symbol_property_keys() {
    let result = eval_one(
        r#"(let ((key (copy-sequence "a")))
             (put 'neo-nonsymbol-prop key 7)
             (list
              (get 'neo-nonsymbol-prop key)
              (get 'neo-nonsymbol-prop (copy-sequence "a"))
              (symbol-plist 'neo-nonsymbol-prop)))"#,
    );

    assert_eq!(result, "OK (7 nil (\"a\" 7))");
}

#[test]
fn symbol_with_pos_property_keys_follow_gnu_eq_rules() {
    let result = eval_one(
        r#"(progn
             (put 'neo-swp-prop 'a 'bare)
             (list
              (let ((symbols-with-pos-enabled t))
                (put 'neo-swp-prop (position-symbol 'a 1) 'pos)
                (list
                 (get 'neo-swp-prop 'a)
                 (get 'neo-swp-prop (position-symbol 'a 2))
                 (length (symbol-plist 'neo-swp-prop))))
              (progn
                (setplist 'neo-swp-prop nil)
                (put 'neo-swp-prop 'a 'bare)
                (let ((symbols-with-pos-enabled nil))
                  (put 'neo-swp-prop (position-symbol 'a 1) 'pos)
                  (list
                   (get 'neo-swp-prop 'a)
                   (get 'neo-swp-prop (position-symbol 'a 2))
                   (length (symbol-plist 'neo-swp-prop)))))))"#,
    );

    assert_eq!(result, "OK ((pos pos 2) (bare nil 4))");
}

#[test]
fn skip_debugger_matches_raw_unibyte_ignored_error_regex() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    crate::emacs_core::errors::init_standard_errors(&mut ev.obarray);
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    ev.obarray
        .set_symbol_value("debug-ignored-errors", Value::list(vec![raw]));
    let sig = match crate::emacs_core::error::signal("error", vec![raw]) {
        Flow::Signal(sig) => sig,
        other => panic!("expected signal flow, got {other:?}"),
    };
    let conditions = ev.signal_conditions_value(&sig);
    assert!(
        ev.skip_debugger(&sig, &conditions)
            .expect("skip_debugger should evaluate")
    );
}

/// The command loop's log ranks an uncaught signal the way GNU's debugger
/// gate does (`skip_debugger', src/eval.c:2146-2180, over
/// `debug-ignored-errors', lisp/bindings.el:1038-1046): a quit or an ignored
/// condition is routine, anything else is a diagnostic.  Evil's
/// `end-of-line' (evil-macros.el: `put' error-conditions, `cl-pushnew' onto
/// `debug-ignored-errors') is the case that showed up as "ERROR ... End of
/// line" on every arrow key at the end of a line.
#[test]
fn command_error_severity_follows_gnus_debug_ignored_errors() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    crate::emacs_core::errors::init_standard_errors(&mut ev.obarray);
    let signal_of =
        |name: &str, data: Vec<Value>| match crate::emacs_core::error::signal(name, data) {
            Flow::Signal(sig) => sig,
            other => panic!("expected signal flow, got {other:?}"),
        };
    ev.eval_str(
        r#"(progn
             (put 'end-of-line 'error-conditions '(end-of-line error))
             (put 'end-of-line 'error-message "End of line")
             (put 'probe-child-error 'error-conditions '(probe-child-error user-error error))
             (put 'probe-child-error 'error-message "Child of user-error")
             (setq debug-ignored-errors nil))"#,
    )
    .expect("define evil's end-of-line error and a child of user-error");

    // With nothing ignored, evil's error ranks like any other.
    let end_of_line = signal_of("end-of-line", vec![]);
    assert_eq!(
        ev.command_error_severity(&end_of_line),
        CommandErrorSeverity::Diagnostic
    );

    // evil pushes the condition onto `debug-ignored-errors'; GNU's default
    // list already has `end-of-buffer' and `user-error'.
    ev.eval_str(
        r#"(setq debug-ignored-errors '(end-of-line end-of-buffer user-error "^Nothing at"))"#,
    )
    .expect("install the ignored conditions");
    assert_eq!(
        ev.command_error_severity(&end_of_line),
        CommandErrorSeverity::Routine
    );
    let user_error = signal_of("user-error", vec![Value::string("No recent files")]);
    assert_eq!(
        ev.command_error_severity(&user_error),
        CommandErrorSeverity::Routine,
        "user-error is GNU's main ignored condition"
    );
    // An entry matches through `error-conditions', not only the symbol.
    let child = signal_of("probe-child-error", vec![]);
    assert_eq!(
        ev.command_error_severity(&child),
        CommandErrorSeverity::Routine,
        "a condition inherited from user-error is ignored too"
    );
    // A string entry is a regexp over the rendered message.
    let by_message = signal_of("error", vec![Value::string("Nothing at point")]);
    assert_eq!(
        ev.command_error_severity(&by_message),
        CommandErrorSeverity::Routine
    );

    // A quit is routine whatever the list says (`signal_quit_p'), by the
    // symbol or by a `quit' condition such as `minibuffer-quit'.
    assert_eq!(
        ev.command_error_severity(&signal_of("quit", vec![])),
        CommandErrorSeverity::Routine
    );
    assert_eq!(
        ev.command_error_severity(&signal_of("minibuffer-quit", vec![])),
        CommandErrorSeverity::Routine
    );

    // A real error stays a diagnostic.
    let void = signal_of("void-function", vec![Value::symbol("no-such-command")]);
    assert_eq!(
        ev.command_error_severity(&void),
        CommandErrorSeverity::Diagnostic
    );
}

/// `command_loop_2' is GNU's sole recovery owner; nothing on its error path
/// may signal.  A string in `debug-ignored-errors' that is not a valid
/// regexp makes the matcher signal `invalid-regexp'; that ranks the original
/// error as a diagnostic instead of unwinding the command loop.
#[test]
fn command_error_severity_never_signals_on_a_bad_ignore_list() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    crate::emacs_core::errors::init_standard_errors(&mut ev.obarray);
    ev.eval_str(r#"(setq debug-ignored-errors '("[" end-of-buffer))"#)
        .expect("install a broken regexp entry");
    let sig = match crate::emacs_core::error::signal("end-of-buffer", vec![]) {
        Flow::Signal(sig) => sig,
        other => panic!("expected signal flow, got {other:?}"),
    };
    assert!(
        ev.skip_debugger(&sig, &ev.signal_conditions_value(&sig))
            .is_err(),
        "the matcher itself signals on the broken entry"
    );
    assert_eq!(
        ev.command_error_severity(&sig),
        CommandErrorSeverity::Diagnostic
    );
}

fn install_minimal_special_event_command_runtime(ev: &mut Context) {
    ev.eval_str(
        r#"
(fset 'command-execute
      (lambda (cmd &optional _record keys _special)
        (funcall cmd (aref keys 0))))
(fset 'handle-delete-frame
      (lambda (event)
        (setq neo-last-delete-frame-event event)
        nil))
(fset 'handle-focus-in
      (lambda (event)
        (internal-handle-focus-in event)))
(fset 'handle-focus-out
      (lambda (_event)
        nil))
"#,
    )
    .expect("eval forms");
}

fn find_bin(name: &str) -> String {
    for dir in &["/bin", "/usr/bin", "/run/current-system/sw/bin"] {
        let path = format!("{}/{}", dir, name);
        if std::path::Path::new(&path).exists() {
            return path;
        }
    }
    if let Ok(output) = std::process::Command::new("which").arg(name).output()
        && output.status.success()
    {
        return String::from_utf8_lossy(&output.stdout).trim().to_string();
    }
    name.to_string()
}

fn gnu_timer_after(delay: Duration, callback: &str) -> Value {
    let when = SystemTime::now()
        .checked_add(delay)
        .expect("timer deadline should fit in system time")
        .duration_since(UNIX_EPOCH)
        .expect("timer deadline should be after unix epoch");
    let secs = when.as_secs() as i64;

    Value::vector(vec![
        Value::NIL,
        Value::fixnum(secs >> 16),
        Value::fixnum(secs & 0xFFFF),
        Value::fixnum(when.subsec_micros() as i64),
        Value::NIL,
        Value::symbol(callback),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ])
}

fn gnu_timer_before(delay: Duration, callback: &str) -> Value {
    let when = SystemTime::now()
        .checked_sub(delay)
        .expect("timer deadline should fit in system time")
        .duration_since(UNIX_EPOCH)
        .expect("timer deadline should be after unix epoch");
    let secs = when.as_secs() as i64;

    Value::vector(vec![
        Value::NIL,
        Value::fixnum(secs >> 16),
        Value::fixnum(secs & 0xFFFF),
        Value::fixnum(when.subsec_micros() as i64),
        Value::NIL,
        Value::symbol(callback),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ])
}

fn gnu_idle_timer_after(delay: Duration, callback: &str) -> Value {
    let secs = delay.as_secs() as i64;

    Value::vector(vec![
        Value::NIL,
        Value::fixnum(secs >> 16),
        Value::fixnum(secs & 0xFFFF),
        Value::fixnum(delay.subsec_micros() as i64),
        Value::NIL,
        Value::symbol(callback),
        Value::NIL,
        Value::symbol("idle"),
        Value::fixnum(0),
        Value::NIL,
    ])
}

#[derive(Clone, Default)]
struct RecordingDisplayHost {
    primary_size: Option<GuiFrameHostSize>,
    opening_frame_pending: bool,
}

impl RecordingDisplayHost {
    fn opening_with_primary_size(width: u32, height: u32) -> Self {
        Self {
            primary_size: Some(GuiFrameHostSize { width, height }),
            opening_frame_pending: true,
        }
    }
}

impl DisplayHost for RecordingDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn opening_gui_frame_pending(&self) -> bool {
        self.opening_frame_pending
    }

    fn current_primary_window_size(&self) -> Option<GuiFrameHostSize> {
        self.primary_size
    }
}

struct VisualConfigRecordingDisplayHost {
    visual_calls: Rc<RefCell<Vec<neomacs_display_protocol::VisualConfig>>>,
}

impl DisplayHost for VisualConfigRecordingDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn set_visual_config(
        &mut self,
        config: neomacs_display_protocol::VisualConfig,
    ) -> Result<(), String> {
        self.visual_calls.borrow_mut().push(config);
        Ok(())
    }
}

#[test]
fn eval_with_explicit_lexenv_restores_outer_lexenv() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(let ((x 41)) (list (eval 'x '((x . 7))) x))"),
        "OK (7 41)"
    );
}

#[test]
fn neomacs_effect_api_sets_queries_and_resets_named_properties() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let effect_calls = Rc::new(RefCell::new(Vec::new()));
    ev.set_display_host(Box::new(VisualConfigRecordingDisplayHost {
        visual_calls: Rc::clone(&effect_calls),
    }));
    effect_calls.borrow_mut().clear();

    let value = ev
        .eval_str(
            r##"(progn
                 (neomacs-effect-set 'cursor-glow
                   :enabled t :color "#66ccff" :radius 48)
                 (let ((config (neomacs-effect-get 'cursor-glow)))
                   (list (plist-get config :enabled)
                         (plist-get config :color)
                         (plist-get config :radius))))"##,
        )
        .expect("named effect update should evaluate");
    assert_eq!(value.to_string(), r##"(t "#66CCFF" 48.0)"##);
    assert_eq!(effect_calls.borrow().len(), 1);
    assert!(effect_calls.borrow()[0].effects.cursor_glow.enabled);

    let reset = ev
        .eval_str(
            "(progn (neomacs-effect-reset 'cursor-glow)\
             (plist-get (neomacs-effect-get 'cursor-glow) :enabled))",
        )
        .expect("effect reset should evaluate");
    assert!(reset.is_nil());
    assert_eq!(effect_calls.borrow().len(), 2);
}

#[test]
fn neomacs_effect_profiles_validate_atomically() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let visual_calls = Rc::new(RefCell::new(Vec::new()));
    ev.set_display_host(Box::new(VisualConfigRecordingDisplayHost {
        visual_calls: Rc::clone(&visual_calls),
    }));
    visual_calls.borrow_mut().clear();

    let error = ev
        .eval_str(
            r#"(neomacs-effects-apply
                 '((cursor-glow :enabled t :radius 12)
                   (missing-effect :enabled t)))"#,
        )
        .unwrap_err();
    let _ = error;
    assert!(visual_calls.borrow().is_empty());

    let enabled = ev
        .eval_str("(plist-get (neomacs-effect-get 'cursor-glow) :enabled)")
        .unwrap();
    assert!(enabled.is_nil());

    ev.eval_str("(neomacs-effect-set 'cursor-glow :enabled t)")
        .unwrap();
    let replaced = ev
        .eval_str(
            r#"(progn
                 (neomacs-effects-apply '((rain-effect :enabled t)))
                 (list (plist-get (neomacs-effect-get 'cursor-glow) :enabled)
                       (plist-get (neomacs-effect-get 'rain-effect) :enabled)
                       (and (memq 'rain-effect (neomacs-effect-names)) t)))"#,
        )
        .unwrap();
    assert_eq!(replaced.to_string(), "(nil t t)");
    assert_eq!(visual_calls.borrow().len(), 2);
}

#[test]
fn named_visual_behavior_configs_replace_positional_animation_setters() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let value = ev
        .eval_str(
            r#"(progn
                 (neomacs-effect-set 'cursor-motion
                   :enabled t :speed 18.0 :style 'linear :duration 0.2)
                 (neomacs-effect-set 'scroll-transition
                   :effect 'page-curl :easing 'spring)
                 (neomacs-effect-set 'buffer-transition
                   :effect 'slide :axis 'horizontal :direction 'backward)
                 (list (plist-get (neomacs-effect-get 'cursor-motion) :style)
                       (plist-get (neomacs-effect-get 'scroll-transition) :effect)
                       (plist-get (neomacs-effect-get 'buffer-transition) :axis)
                       (plist-get (neomacs-effect-get 'buffer-transition) :direction)
                       (fboundp 'neomacs-set-cursor-animation)
                       (fboundp 'neomacs-set-cursor-blink)))"#,
        )
        .expect("named visual behavior settings should evaluate");

    assert_eq!(
        value.to_string(),
        "(linear page-curl horizontal backward nil nil)"
    );
}

#[test]
fn effect_discovery_distinguishes_cursor_profiles_from_global_behavior() {
    assert_eq!(
        eval_one(
            "(list (and (memq 'cursor-motion (neomacs-effect-names)) t)
                   (and (memq 'cursor-glow (neomacs-effect-names 'cursor)) t)
                   (and (memq 'cursor-motion (neomacs-effect-names 'cursor)) t)
                   (and (memq 'buffer-transition (neomacs-effect-names 'behavior)) t))"
        ),
        "OK (t t nil t)"
    );
}

#[test]
fn old_positional_cursor_effect_setters_are_not_registered() {
    assert_eq!(eval_one("(fboundp 'neomacs-set-cursor-glow)"), "OK nil");
}

#[test]
fn neomacs_buffer_text_backend_default_is_gap_and_new_buffers_can_opt_into_non_gap_backends() {
    crate::test_utils::init_test_tracing();
    for backend_kind in crate::buffer::BufferTextBackendKind::non_gap_implemented_variants() {
        let backend = backend_kind.symbol_name();
        assert_eq!(
            eval_one(&format!(
                r#"(list
                     (neomacs-default-buffer-text-backend)
                     (neomacs-buffer-text-backend)
                     (neomacs-set-default-buffer-text-backend '{backend})
                     (neomacs-buffer-text-backend)
                     (save-current-buffer
                       (set-buffer (get-buffer-create "{backend}-backend"))
                       (insert "abc")
                       (list (neomacs-buffer-text-backend) (buffer-string))))"#
            )),
            format!(r#"OK (gap-buffer gap-buffer {backend} gap-buffer ({backend} "abc"))"#)
        );
    }
}

#[test]
fn neomacs_set_buffer_text_backend_converts_current_shared_text_storage() {
    crate::test_utils::init_test_tracing();
    for backend_kind in crate::buffer::BufferTextBackendKind::non_gap_implemented_variants() {
        let backend = backend_kind.symbol_name();
        assert_eq!(
            eval_one(&format!(
                r#"(save-current-buffer
                     (let ((base (get-buffer-create "convert-base-{backend}")))
                       (set-buffer base)
                       (erase-buffer)
                       (insert "abécd")
                       (put-text-property 2 4 'face 'bold)
                       (let ((m (copy-marker 4))
                             (ind (make-indirect-buffer base "convert-ind-{backend}" t)))
                         (list
                          (neomacs-buffer-text-backend)
                          (neomacs-set-buffer-text-backend '{backend})
                          (neomacs-buffer-text-backend)
                          (save-current-buffer
                            (set-buffer ind)
                            (neomacs-buffer-text-backend))
                          (buffer-string)
                          (save-current-buffer
                            (set-buffer ind)
                            (buffer-string))
                          (get-text-property 3 'face)
                          (marker-position m)
                          (neomacs-set-buffer-text-backend 'gap-buffer)
                          (save-current-buffer
                            (set-buffer ind)
                            (neomacs-buffer-text-backend))))))"#
            )),
            format!(
                r#"OK (gap-buffer {backend} {backend} {backend} #("abécd" 1 3 (face bold)) #("abécd" 1 3 (face bold)) bold 4 gap-buffer gap-buffer)"#
            )
        );
    }
}

#[test]
fn neomacs_buffer_text_backend_rejects_non_symbol_and_unknown_kinds() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(list
                 (condition-case err
                     (neomacs-set-default-buffer-text-backend "piece-tree")
                   (error (car err)))
                 (condition-case err
                     (neomacs-set-default-buffer-text-backend 'missing-backend)
                   (error (car err)))
                 (neomacs-set-default-buffer-text-backend 'rope)
                 (neomacs-default-buffer-text-backend))"#
        ),
        "OK (wrong-type-argument error rope rope)"
    );
}

#[test]
fn eval_with_explicit_lexenv_shadows_special_reads_and_setq() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(progn
               (defvar ev-explicit-special 1)
               (list
                 (eval '(progn (setq ev-explicit-special 9) ev-explicit-special)
                       '((ev-explicit-special . 7)))
                 ev-explicit-special))"
        ),
        "OK (9 1)"
    );
}

#[test]
fn source_cons_macro_form_expands_via_value_expansion_path() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // Verify that the eval loop's macro dispatch correctly handles a
    // macro defined as a `(macro . FN)` cons cell (mirrors GNU
    // eval.c:2730 — a "macro function" cell that wraps a lambda).
    // The expansion itself must return the body unchanged (the
    // lambda is identity), so evaluating the expansion yields 3.
    //
    // The ordinary eval loop expands the macro for each evaluated call.
    ev.eval_str(
        "(fset 'source-cache-macro
                  (cons 'macro
                        (lambda (x)
                          x)))",
    )
    .expect("install macro");

    let first = ev.eval_str("(source-cache-macro (+ 1 2))");
    let second = ev.eval_str("(source-cache-macro (+ 1 2))");

    assert_eq!(format_eval_result(&first), "OK 3");
    assert_eq!(format_eval_result(&second), "OK 3");
}

#[test]
fn recursive_edit_without_input_receiver_still_runs_noninteractive_top_level() {
    crate::test_utils::init_test_tracing();

    let mut ev = Context::new();
    ev.set_variable("noninteractive", Value::T);
    let top_level = crate::emacs_core::value_reader::read_all(
        "(progn (setq neomacs--batch-no-input-probe 42) nil)",
        &test_ob(),
    )
    .expect("parse top-level form")
    .into_iter()
    .next()
    .expect("top-level form");
    ev.set_variable("top-level", top_level);

    let result = ev.recursive_edit();
    assert!(result.is_ok(), "batch recursive edit should exit cleanly");
    assert_eq!(
        ev.shutdown_request(),
        Some(crate::emacs_core::eval::ShutdownRequest {
            exit_code: 0,
            restart: false,
        })
    );
    assert_eq!(
        ev.obarray().symbol_value("neomacs--batch-no-input-probe"),
        Some(&Value::fixnum(42))
    );
}

#[test]
fn outer_command_loop_leaves_exit_unmatched_inside_keyboard_macro() {
    crate::test_utils::init_test_tracing();

    let mut ev = Context::new();
    ev.set_variable("noninteractive", Value::T);
    let top_level = crate::emacs_core::value_reader::read_all(
        r#"(progn
             (setq neo-caught-exit nil
                   neo-continued-after-macro nil)
             (fset 'command-execute
                   (lambda (command &optional _record _keys _special)
                     (funcall command)))
             (fset 'neo-throw-exit
                   (lambda ()
                     (interactive)
                     (setq neo-caught-exit
                           (condition-case err
                               (throw 'exit nil)
                             (error (list (car err) (cdr err)))))))
             (let ((global (make-sparse-keymap)))
               (use-global-map global)
               (define-key global "a" 'neo-throw-exit)
               (execute-kbd-macro "a")
               (setq neo-continued-after-macro t))
             nil)"#,
        &test_ob(),
    )
    .expect("parse top-level form")
    .into_iter()
    .next()
    .expect("top-level form");
    ev.set_variable("top-level", top_level);

    let result = ev.recursive_edit();

    assert!(result.is_ok(), "batch recursive edit should exit cleanly");
    assert_eq!(
        ev.eval_symbol("neo-caught-exit")
            .expect("command should record the caught no-catch signal"),
        Value::list(vec![
            Value::symbol("no-catch"),
            Value::list(vec![Value::symbol("exit"), Value::NIL]),
        ])
    );
    assert_eq!(
        ev.eval_symbol("neo-continued-after-macro")
            .expect("top-level should continue after the macro"),
        Value::T
    );
}

#[test]
fn clear_top_level_eval_state_discards_stale_named_call_cache_entries() {
    crate::test_utils::init_test_tracing();

    let mut ev = Context::new();
    ev.eval_str(r#"(autoload 'neomacs--stale-call-target "dummy-file" nil t)"#)
        .expect("autoload registration should succeed");
    let sym = intern("neomacs--stale-call-target");
    let epoch = ev.obarray.function_epoch();

    ev.named_call_cache.insert(
        sym,
        NamedCallCacheEntry {
            function_epoch: epoch,
            target: NamedCallTarget::Void,
        },
    );
    assert!(matches!(
        ev.resolve_named_call_target_by_id(sym),
        NamedCallTarget::Void
    ));

    ev.clear_top_level_eval_state();

    match ev.resolve_named_call_target_by_id(sym) {
        NamedCallTarget::Obarray(function) => {
            assert!(
                crate::emacs_core::autoload::is_autoload_value(&function),
                "expected autoload function cell, got {function}"
            );
        }
        other => panic!("expected autoload-backed named call target, got {other:?}"),
    }
}

#[test]
fn runtime_macro_expansion_repeats_across_equivalent_explicit_environments() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // `load-in-progress` must not change macro invocation semantics.
    ev.set_variable("load-in-progress", Value::T);
    ev.eval_str("(defvar runtime-cache-count 0)")
        .expect("defvar runtime-cache-count");
    ev.eval_str(
        "(defalias 'runtime-cache-macro
           (cons 'macro
                 (lambda (form)
                   (setq runtime-cache-count (1+ runtime-cache-count))
                   form)))",
    )
    .expect("install runtime-cache-macro");
    let definition = ev
        .obarray()
        .symbol_function("runtime-cache-macro")
        .expect("runtime-cache-macro definition");
    let arg = Value::list(vec![Value::symbol("+"), Value::fixnum(1), Value::fixnum(2)]);
    let form = Value::list(vec![Value::symbol("runtime-cache-macro"), arg]);
    let env1 = Value::list(vec![Value::cons(
        Value::symbol("context"),
        Value::symbol("marker"),
    )]);
    let env2 = Value::list(vec![Value::cons(
        Value::symbol("context"),
        Value::symbol("marker"),
    )]);

    let calls0 = ev.macro_expand_calls;

    let first = ev
        .expand_macro_for_macroexpand(form, definition, vec![arg], Some(env1))
        .expect("first runtime macro expansion");
    let second = ev
        .expand_macro_for_macroexpand(form, definition, vec![arg], Some(env2))
        .expect("second runtime macro expansion");

    assert!(equal_value(&first, &arg, 0));
    assert!(equal_value(&second, &arg, 0));
    assert_eq!(ev.macro_expand_calls - calls0, 2);
    assert_eq!(
        ev.obarray()
            .symbol_value("runtime-cache-count")
            .copied()
            .unwrap_or(Value::NIL),
        Value::fixnum(2)
    );
}

#[test]
fn runtime_macro_expansion_handles_raw_unibyte_strings_in_environment() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("load-in-progress", Value::T);
    ev.eval_str("(defvar runtime-cache-count 0)")
        .expect("defvar runtime-cache-count");
    ev.eval_str(
        "(defalias 'runtime-cache-macro
           (cons 'macro
                 (lambda (form)
                   (setq runtime-cache-count (1+ runtime-cache-count))
                   form)))",
    )
    .expect("install runtime-cache-macro");
    let definition = ev
        .obarray()
        .symbol_function("runtime-cache-macro")
        .expect("runtime-cache-macro definition");
    let arg = Value::list(vec![Value::symbol("+"), Value::fixnum(1), Value::fixnum(2)]);
    let form = Value::list(vec![Value::symbol("runtime-cache-macro"), arg]);
    let raw_unibyte = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let env1 = Value::list(vec![Value::cons(Value::symbol("context"), raw_unibyte)]);
    let env2 = Value::list(vec![Value::cons(Value::symbol("context"), raw_unibyte)]);

    let calls0 = ev.macro_expand_calls;

    let first = ev
        .expand_macro_for_macroexpand(form, definition, vec![arg], Some(env1))
        .expect("first runtime macro expansion");
    let second = ev
        .expand_macro_for_macroexpand(form, definition, vec![arg], Some(env2))
        .expect("second runtime macro expansion");

    assert!(equal_value(&first, &arg, 0));
    assert!(equal_value(&second, &arg, 0));
    assert_eq!(ev.macro_expand_calls - calls0, 2);
    assert_eq!(
        ev.obarray()
            .symbol_value("runtime-cache-count")
            .copied()
            .unwrap_or(Value::NIL),
        Value::fixnum(2)
    );
}

#[test]
fn runtime_macro_expansion_handles_raw_unibyte_string_arguments() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("load-in-progress", Value::T);
    ev.eval_str("(defvar runtime-cache-bytes-count 0)")
        .expect("defvar runtime-cache-bytes-count");
    ev.eval_str(
        "(defalias 'runtime-cache-bytes-macro
           (cons 'macro
                 (lambda (form)
                   (setq runtime-cache-bytes-count
                         (1+ runtime-cache-bytes-count))
                   form)))",
    )
    .expect("install runtime-cache-bytes-macro");

    let definition = ev
        .obarray()
        .symbol_function("runtime-cache-bytes-macro")
        .expect("runtime-cache-bytes-macro definition");
    let arg = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let form = Value::list(vec![Value::symbol("runtime-cache-bytes-macro"), arg]);

    let calls0 = ev.macro_expand_calls;

    let first = ev
        .expand_macro_for_macroexpand(form, definition, vec![arg], None)
        .expect("first raw-byte runtime macro expansion");
    let second = ev
        .expand_macro_for_macroexpand(form, definition, vec![arg], None)
        .expect("second raw-byte runtime macro expansion");

    assert!(equal_value(&first, &arg, 0));
    assert!(equal_value(&second, &arg, 0));
    assert_eq!(ev.macro_expand_calls - calls0, 2);
    assert_eq!(
        ev.obarray()
            .symbol_value("runtime-cache-bytes-count")
            .copied()
            .unwrap_or(Value::NIL),
        Value::fixnum(2)
    );
}

#[test]
fn catch_leaves_shared_condition_stack_balanced() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = ev.eval_str("(catch 'tag (throw 'tag 42))");
    assert_eq!(format_eval_result(&result), "OK 42");
    assert_eq!(ev.condition_stack_depth_for_test(), 0);
    assert!(ev.top_level_eval_state_is_clean());
}

#[test]
fn condition_case_leaves_shared_condition_stack_balanced() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = ev.eval_str("(condition-case err (signal 'error 1) (error err))");
    assert_eq!(format_eval_result(&result), "OK (error . 1)");
    assert_eq!(ev.condition_stack_depth_for_test(), 0);
    assert!(ev.top_level_eval_state_is_clean());
}

#[test]
fn condition_case_without_handlers_returns_body_value() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = ev.eval_str("(condition-case nil (progn 42))");
    assert_eq!(format_eval_result(&result), "OK 42");
    assert_eq!(ev.condition_stack_depth_for_test(), 0);
    assert!(ev.top_level_eval_state_is_clean());
}

#[test]
fn condition_case_value_path_catches_default_toplevel_value_signal() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = ev.eval_str(
        "(condition-case nil
            (default-toplevel-value 'vm-unbound-value-path)
          (error 'caught))",
    );
    assert_eq!(format_eval_result(&result), "OK caught");
    assert_eq!(ev.condition_stack_depth_for_test(), 0);
    assert!(ev.top_level_eval_state_is_clean());
}

#[test]
fn handler_bind_1_leaves_shared_condition_stack_balanced() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = ev.eval_str(
        r#"(condition-case err
           (handler-bind-1 (lambda () (signal 'error 1))
                           '(error)
                           (lambda (_data) 'handled))
         (error err))"#,
    );
    assert_eq!(format_eval_result(&result), "OK (error . 1)");
    assert_eq!(ev.condition_stack_depth_for_test(), 0);
    assert!(ev.top_level_eval_state_is_clean());
}

#[test]
fn handler_bind_1_runs_inside_signal_dynamic_extent() {
    crate::test_utils::init_test_tracing();
    // user-error is defined in subr.el, so this needs the bootstrap
    // runtime context.
    assert_eq!(
        bootstrap_eval_one(
            "(catch 'tag
               (handler-bind-1
                 (lambda ()
                   (list 'inner-catch
                         (catch 'tag
                           (user-error \"hello\"))))
                 '(error)
                 (lambda (_err) (throw 'tag 'err))))"
        ),
        "OK (inner-catch err)"
    );
}

#[test]
fn set_lexical_binding_syncs_top_level_lexenv_sentinel() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    assert!(ev.lexenv.is_nil());
    assert!(!ev.lexical_binding());

    ev.set_lexical_binding(true);
    assert!(ev.lexical_binding());
    assert!(ev.lexenv.is_cons());
    assert!(ev.lexenv.cons_car().is_t());
    assert!(ev.lexenv.cons_cdr().is_nil());

    ev.set_lexical_binding(false);
    assert!(!ev.lexical_binding());
    assert!(ev.lexenv.is_nil());
}

#[test]
fn set_lexical_binding_updates_visible_dynamic_binding() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let sym = intern("lexical-binding");

    let specpdl_count = ev.specpdl.len();
    ev.try_specbind(sym, Value::NIL)
        .expect("lexical-binding dynamic binding");
    assert!(ev.visible_variable_value_or_nil("lexical-binding").is_nil());

    ev.set_lexical_binding(true);
    assert!(ev.visible_variable_value_or_nil("lexical-binding").is_t());
    assert!(ev.lexical_binding());

    ev.unbind_to(specpdl_count);
    assert!(ev.visible_variable_value_or_nil("lexical-binding").is_nil());
}

#[test]
fn lexical_binding_is_local_if_set_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        "(list (local-variable-if-set-p 'lexical-binding)
               (local-variable-p 'lexical-binding)
               (progn
                 (set 'lexical-binding t)
                 (list lexical-binding
                       (local-variable-p 'lexical-binding)
                       (default-value 'lexical-binding))))",
    );

    assert_eq!(result, "OK (t nil (t t nil))");
}

#[test]
fn clear_top_level_eval_state_restores_top_level_lexenv_mode() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.lexenv = Value::list(vec![Value::symbol("vm-temp"), Value::T]);

    ev.clear_top_level_eval_state();

    // lexical_binding is restored from obarray default (which is nil for
    // a bare Context), not preserved across clear_top_level_eval_state.
    assert!(!ev.lexical_binding());
    assert!(ev.lexenv.is_nil());
    assert!(ev.top_level_eval_state_is_clean());
}

#[test]
fn handler_bind_1_mutes_lower_condition_handlers() {
    crate::test_utils::init_test_tracing();
    // user-error is defined in subr.el → bootstrap context required.
    assert_eq!(
        bootstrap_eval_one(
            "(condition-case nil
               (handler-bind-1
                 (lambda ()
                   (list 'result
                         (condition-case nil
                             (user-error \"hello\")
                           (wrong-type-argument 'inner-handler))))
                 '(error)
                 (lambda (_err) (signal 'wrong-type-argument nil)))
             (wrong-type-argument 'wrong-type-argument))"
        ),
        "OK wrong-type-argument"
    );
}

#[test]
fn handler_bind_1_handlers_do_not_apply_within_handlers() {
    crate::test_utils::init_test_tracing();
    // user-error is defined in subr.el → bootstrap context required.
    assert_eq!(
        bootstrap_eval_one(
            "(condition-case nil
               (handler-bind-1
                 (lambda () (user-error \"hello\"))
                 '(error)
                 (lambda (_err) (signal 'wrong-type-argument nil))
                 '(wrong-type-argument)
                 (lambda (_err) (user-error \"wrong-type-argument\")))
             (wrong-type-argument 'wrong-type-argument)
             (error 'plain-error))"
        ),
        "OK wrong-type-argument"
    );
}

#[test]
fn signal_hook_function_sees_raw_signal_payload_before_condition_case() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let (seen)
           (let ((signal-hook-function
                  (lambda (sym data)
                    (setq seen (cons sym data)))))
             (condition-case nil
                 (signal 'error 1)
               (error seen))))"#
        )),
        "OK (error . 1)"
    );
}

#[test]
fn signal_hook_function_runs_before_invalid_error_symbol_canonicalization() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(catch 'tag
           (let ((signal-hook-function
                  (lambda (sym data)
                    (throw 'tag (list sym data)))))
             (signal 'neomacs-invalid-signal 1)))"#
        )),
        "OK (neomacs-invalid-signal 1)"
    );
}

#[test]
fn signal_nil_symbol_with_non_list_payload_becomes_plain_error() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(condition-case err (signal nil 1) (error err))"),
        "OK (error . 1)"
    );
}

#[test]
fn signal_nil_symbol_with_nil_payload_becomes_plain_error() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(condition-case err (signal nil nil) (error err))"),
        "OK (error)"
    );
}

#[test]
fn signal_nil_error_object_uses_embedded_symbol_and_skips_signal_hook() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(let (seen)
           (let ((signal-hook-function
                  (lambda (&rest xs)
                    (setq seen xs))))
             (condition-case err
                 (signal nil '(error 1))
               (error (list err seen)))))"#
        )),
        "OK ((error 1) nil)"
    );
}

#[test]
fn signal_nil_error_object_with_invalid_symbol_reports_generic_invalid_error() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(condition-case err (signal nil '(bogus 1)) (error err))"),
        "OK (error \"Invalid error symbol\")"
    );
}

#[test]
fn evaluator_drop_leaves_symids_resolvable() {
    crate::test_utils::init_test_tracing();
    let sym = {
        let _ev = Context::new_minimal_vm_harness();
        crate::emacs_core::intern::intern("drop-stable-symbol")
    };
    assert_eq!(
        crate::emacs_core::intern::resolve_sym(sym),
        "drop-stable-symbol"
    );
}

#[test]
fn evaluator_reuses_hidden_internal_interpreter_environment_symbol() {
    crate::test_utils::init_test_tracing();
    let first = Context::new_minimal_vm_harness().internal_interpreter_environment_symbol;
    let second = Context::new_minimal_vm_harness().internal_interpreter_environment_symbol;
    assert_eq!(first, second);
}

#[test]
fn read_char_applies_resize_event_before_returning_next_keypress() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    assert_eq!(ev.frames.selected_frame().map(|frame| frame.id), Some(fid));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    tx.send(crate::keyboard::InputEvent::Resize {
        width: 700,
        height: 800,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    })
    .unwrap();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .unwrap();

    let event = ev.read_char().expect("read_char should return a keypress");
    assert_eq!(event, Value::fixnum('a' as i64));

    let frame = ev.frames.get(fid).expect("frame should still be live");
    assert_eq!(frame.width, 700);
    assert_eq!(frame.height, 800);
}

#[test]
fn read_char_exposes_raw_tty_escape_without_a_native_timeout() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(vec![0x1b], 0))
        .unwrap();

    assert_eq!(ev.read_char().unwrap(), Value::fixnum(0x1b));
}

#[test]
fn read_char_decodes_utf8_tty_input_split_across_host_reads() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    let bytes = "中".as_bytes();
    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(
        bytes[..2].to_vec(),
        0,
    ))
    .unwrap();
    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(
        bytes[2..].to_vec(),
        0,
    ))
    .unwrap();

    assert_eq!(ev.read_char().unwrap(), Value::fixnum('中' as i64));
}

#[test]
fn read_char_applies_keyboard_coding_system_to_raw_tty_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    crate::emacs_core::coding::builtin_set_keyboard_coding_system(
        &mut ev.coding_systems,
        vec![Value::symbol("iso-latin-9")],
    )
    .expect("set keyboard coding system");
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(vec![0xa4], 0))
        .unwrap();

    assert_eq!(ev.read_char().unwrap(), Value::fixnum('€' as i64));
}

#[test]
fn read_char_preserves_emacs_internal_raw_byte_characters() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    crate::emacs_core::coding::builtin_set_keyboard_coding_system(
        &mut ev.coding_systems,
        vec![Value::symbol("emacs-internal")],
    )
    .expect("set keyboard coding system");
    let raw = crate::emacs_core::emacs_char::EmacsChar::from_byte8(0xa4);
    let mut encoded = [0_u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let encoded_len = raw.char_string(&mut encoded);
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(
        encoded[..1].to_vec(),
        0,
    ))
    .unwrap();
    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(
        encoded[1..encoded_len].to_vec(),
        0,
    ))
    .unwrap();

    assert_eq!(
        ev.read_char().unwrap(),
        Value::fixnum(i64::from(raw.code()))
    );
}

#[test]
fn read_char_streams_multibyte_keyboard_coding_across_host_reads() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    crate::emacs_core::coding::builtin_set_keyboard_coding_system(
        &mut ev.coding_systems,
        vec![Value::symbol("chinese-big5")],
    )
    .expect("set keyboard coding system");
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(vec![0xa4], 0))
        .unwrap();
    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(vec![0x40], 0))
        .unwrap();

    assert_eq!(ev.read_char().unwrap(), Value::fixnum('一' as i64));
}

#[test]
fn read_key_sequence_translates_raw_tty_csi_through_input_decode_map() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("up")],
        Value::symbol("neomacs-test-up-command"),
    )
    .expect("define up command");
    let input_decode_map = ev
        .eval_symbol("input-decode-map")
        .expect("input-decode-map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        input_decode_map,
        &[
            Value::fixnum(0x1b),
            Value::fixnum('[' as i64),
            Value::fixnum('A' as i64),
        ],
        Value::vector(vec![Value::symbol("up")]),
    )
    .expect("define CSI translation");
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::raw_tty_bytes(
        b"\x1b[A".to_vec(),
        0,
    ))
    .unwrap();

    let (keys, binding) = ev.read_key_sequence().expect("read translated CSI");
    assert_eq!(keys, vec![Value::symbol("up")]);
    assert_eq!(binding, Value::symbol("neomacs-test-up-command"));
}

#[test]
fn read_char_switches_active_kboard_to_keypress_source_frame_terminal() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let primary = ev.frames.create_frame("F1", 960, 640, buf);
    ev.command_loop
        .keyboard
        .set_input_decode_map(Value::symbol("primary-map"));

    crate::emacs_core::terminal::pure::ensure_terminal_runtime_owner(
        7,
        "tty-7",
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::interactive(Some("xterm-256color".to_string()), neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256)),
    );
    let secondary = ev.frames.create_frame_on_terminal("F2", 7, 960, 640, buf);
    assert!(ev.frames.select_frame(primary));
    ev.sync_keyboard_terminal_owner();
    assert_eq!(ev.command_loop.keyboard.active_terminal_id(), 0);

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    tx.send(crate::keyboard::InputEvent::key_press_in_frame(
        crate::keyboard::KeyEvent::char('z'),
        secondary.0,
    ))
    .unwrap();

    let event = ev.read_char().expect("read_char should return a keypress");
    assert_eq!(event, Value::fixnum('z' as i64));
    assert_eq!(ev.command_loop.keyboard.active_terminal_id(), 7);
    assert_eq!(
        ev.command_loop.keyboard.input_decode_map(),
        Value::NIL,
        "raw key ingress should switch to the source frame terminal before key decoding state is used"
    );
}

#[test]
fn read_char_returns_unread_emacs_event_value_without_reencoding() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let meta_x = crate::keyboard::KeyEvent::char_with_mods('x', crate::keyboard::Modifiers::meta())
        .to_emacs_event_value();

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(meta_x);

    let event = ev
        .read_char()
        .expect("read_char should return unread event");
    assert_eq!(event, meta_x);
}

#[test]
fn read_char_returns_macro_playback_event_value_without_reencoding() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let return_event =
        crate::keyboard::KeyEvent::named(crate::keyboard::NamedKey::Return).to_emacs_event_value();

    ev.command_loop.keyboard.kboard.executing_kbd_macro = Some(vec![return_event]);
    ev.command_loop.keyboard.kboard.kbd_macro_index = 0;

    let event = ev
        .read_char()
        .expect("read_char should return executing macro event");
    assert_eq!(event, return_event);
    assert_eq!(ev.command_loop.keyboard.kboard.kbd_macro_index, 1);
}

#[test]
fn read_char_prefers_ready_keypress_over_due_timer_callback() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (fset 'read-char-priority-timer
                   (lambda () (setq read-char-priority-timer-fired t)))
             (setq read-char-priority-timer-fired nil))"#,
    )
    .expect("parse timer priority setup");
    ev.eval_str(
        r#"(progn
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6))))
             (fset 'read-char-priority-timer
                   (lambda () (setq read-char-priority-timer-fired t)))
             (setq read-char-priority-timer-fired nil))"#,
    )
    .expect("install timer priority setup");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "read-char-priority-timer",
        )]),
    );

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue ready keypress");
    ev.input_rx = Some(rx);

    let event = ev.read_char().expect("read_char should return keypress");
    assert_eq!(event, Value::fixnum('a' as i64));
    assert_eq!(
        ev.eval_symbol("read-char-priority-timer-fired")
            .expect("timer callback flag"),
        Value::NIL
    );

    ev.fire_pending_timers();
    assert_eq!(
        ev.eval_symbol("read-char-priority-timer-fired")
            .expect("timer callback flag after explicit service"),
        Value::T
    );
}

#[test]
fn read_char_prefers_ready_keypress_over_process_filter_callback() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (fset 'read-char-priority-filter
                   (lambda (_proc string)
                     (setq read-char-priority-filter-data string)))
             (setq read-char-priority-filter-data nil))"#,
    )
    .expect("install process priority setup");

    let pid = ev.processes.create_process(
        "read-char-priority".into(),
        Value::NIL,
        echo,
        vec!["out".into()],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    ev.processes
        .spawn_child(pid, false)
        .expect("spawn process priority child");
    crate::emacs_core::process::builtin_set_process_filter(
        &mut ev,
        vec![
            Value::make_process(pid),
            Value::symbol("read-char-priority-filter"),
        ],
    )
    .expect("install process priority filter");

    std::thread::sleep(Duration::from_millis(20));

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue ready keypress");
    ev.input_rx = Some(rx);

    let event = ev.read_char().expect("read_char should return keypress");
    assert_eq!(event, Value::fixnum('a' as i64));
    assert_eq!(
        ev.eval_symbol("read-char-priority-filter-data")
            .expect("process filter flag"),
        Value::NIL
    );

    crate::emacs_core::process::builtin_accept_process_output(
        &mut ev,
        vec![Value::make_process(pid), Value::make_float(0.1)],
    )
    .expect("accept-process-output should service process callback afterwards");
    assert_eq!(
        ev.eval_symbol("read-char-priority-filter-data")
            .expect("process filter flag after explicit service"),
        Value::string("out\n")
    );
}

#[test]
fn read_char_triggers_redisplay_after_resize_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    assert_eq!(ev.frames.selected_frame().map(|frame| frame.id), Some(fid));

    let redisplay_calls = Rc::new(RefCell::new(Vec::new()));
    let redisplay_calls_in_cb = redisplay_calls.clone();
    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let frame = ev
            .frames
            .selected_frame()
            .expect("selected frame during redisplay");
        redisplay_calls_in_cb
            .borrow_mut()
            .push((frame.width, frame.height));
    }));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    tx.send(crate::keyboard::InputEvent::Resize {
        width: 700,
        height: 800,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    })
    .unwrap();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .unwrap();

    let event = ev.read_char().expect("read_char should return a keypress");
    assert_eq!(event, Value::fixnum('a' as i64));
    assert_eq!(*redisplay_calls.borrow(), vec![(700, 800)]);
}

#[test]
fn read_char_redisplays_when_resize_arrives_after_pre_block_redisplay() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    assert_eq!(ev.frames.selected_frame().map(|frame| frame.id), Some(fid));

    let redisplay_calls = Rc::new(RefCell::new(Vec::new()));
    let redisplay_calls_in_cb = redisplay_calls.clone();

    let (tx, rx) = crossbeam_channel::unbounded();
    let tx_in_cb = tx.clone();
    let injected = Rc::new(RefCell::new(false));
    let injected_in_cb = injected.clone();

    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let frame = ev
            .frames
            .selected_frame()
            .expect("selected frame during redisplay");
        redisplay_calls_in_cb
            .borrow_mut()
            .push((frame.width, frame.height));

        if !*injected_in_cb.borrow() {
            *injected_in_cb.borrow_mut() = true;
            tx_in_cb
                .send(crate::keyboard::InputEvent::Resize {
                    width: 700,
                    height: 800,
                    scale_factor: 1.0,
                    emacs_frame_id: 0,
                })
                .expect("enqueue resize after first redisplay");
            tx_in_cb
                .send(crate::keyboard::InputEvent::key_press(
                    crate::keyboard::KeyEvent::char('a'),
                ))
                .expect("enqueue keypress after resize");
        }
    }));

    ev.input_rx = Some(rx);

    let event = ev.read_char().expect("read_char should return a keypress");
    assert_eq!(event, Value::fixnum('a' as i64));
    assert_eq!(*redisplay_calls.borrow(), vec![(960, 640), (700, 800)]);
}

#[test]
fn read_char_respects_inhibit_redisplay_during_input_wait() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value("inhibit-redisplay", Value::T);

    let redisplay_count = Rc::new(RefCell::new(0usize));
    let redisplay_count_in_cb = Rc::clone(&redisplay_count);
    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplay_count_in_cb.borrow_mut() += 1;
    }));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let notifier = ev.wait_notifier();
    // Keep one sender alive: dropping the last tx disconnects the channel,
    // which the input machinery treats as terminal-gone -> quit (timing flake;
    // see the sit-for soak fix).
    let _tx_keepalive = tx.clone();
    thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('a'),
        ))
        .expect("send keypress");
        if let Some(notifier) = notifier {
            notifier.notify().expect("wake command-input wait");
        }
    });

    let event = ev.read_char().expect("read_char should return keypress");
    assert_eq!(event, Value::fixnum('a' as i64));
    assert_eq!(*redisplay_count.borrow(), 0);
}

#[test]
fn redisplay_skips_callback_when_visible_state_is_unchanged() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let redisplay_count = Rc::new(RefCell::new(0usize));
    let redisplay_count_in_cb = Rc::clone(&redisplay_count);
    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplay_count_in_cb.borrow_mut() += 1;
    }));

    ev.redisplay();
    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 1);

    ev.set_current_message(Some(LispString::from_utf8("hello")));
    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 2);

    ev.apply(Value::symbol("force-mode-line-update"), vec![])
        .expect("force-mode-line-update should be callable");
    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 3);

    ev.redisplay_with_force(true);
    assert_eq!(*redisplay_count.borrow(), 4);
}

#[test]
fn redisplay_runs_resize_mini_frame_for_minibuffer_only_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame_id = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let root_window_id = ev
        .frames
        .get(frame_id)
        .expect("created frame")
        .root_window
        .id();
    {
        let frame = ev.frames.get_mut(frame_id).expect("created frame");
        frame.minibuffer_leaf = None;
        frame.minibuffer_window = Some(root_window_id);
        frame.visibility = crate::window::FrameVisibility::Visible;
    }

    ev.eval_str(
        r#"(progn
             (setq resize-mini-frames t
                   neo-resize-mini-frame-calls 0
                   neo-resize-mini-frame-arg nil)
             (fset 'window--resize-mini-frame
                   (lambda (frame)
                     (setq neo-resize-mini-frame-calls
                           (1+ neo-resize-mini-frame-calls))
                     (setq neo-resize-mini-frame-arg frame))))"#,
    )
    .expect("resize-mini-frame test setup should evaluate");
    ev.redisplay_fn = Some(Box::new(|_ev: &mut Context| {}));

    ev.redisplay_with_force(true);

    assert_eq!(
        ev.obarray().symbol_value("neo-resize-mini-frame-calls"),
        Some(&Value::fixnum(1))
    );
    assert_eq!(
        ev.obarray().symbol_value("neo-resize-mini-frame-arg"),
        Some(&Value::make_frame(frame_id.0))
    );
}

#[test]
fn overlay_property_change_invalidates_redisplay_signature() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str(r#"(progn (insert "x") (setq neo-test-overlay (make-overlay 1 2)))"#)
        .expect("overlay should be created");

    let buffer_id = ev
        .buffers
        .current_buffer_id()
        .expect("current buffer should exist");
    let before = ev
        .redisplay_buffer_signature(buffer_id)
        .expect("buffer should have a redisplay signature");

    ev.eval_str(r#"(overlay-put neo-test-overlay 'after-string "candidate")"#)
        .expect("overlay-put should evaluate");
    let after = ev
        .redisplay_buffer_signature(buffer_id)
        .expect("buffer should still have a redisplay signature");

    assert_ne!(before, after);
    assert_eq!(before.layout.modified_tick, after.layout.modified_tick);
    assert_ne!(
        before.layout.overlay_modified_tick,
        after.layout.overlay_modified_tick
    );
}

#[test]
fn redisplay_skips_callback_after_unwatched_symbol_value_change() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray
        .set_symbol_value("blink-cursor-blinks-done", Value::fixnum(1));

    let redisplay_count = Rc::new(RefCell::new(0usize));
    let redisplay_count_in_cb = Rc::clone(&redisplay_count);
    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplay_count_in_cb.borrow_mut() += 1;
    }));

    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 1);

    ev.eval_str("(setq blink-cursor-blinks-done (1+ blink-cursor-blinks-done))")
        .expect("blink counter setq should evaluate");
    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 1);
}

#[test]
fn set_buffer_redisplay_watcher_invalidates_redisplay() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let redisplay_count = Rc::new(RefCell::new(0usize));
    let redisplay_count_in_cb = Rc::clone(&redisplay_count);
    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplay_count_in_cb.borrow_mut() += 1;
    }));

    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 1);

    ev.eval_str(
        r#"(progn
             (add-variable-watcher 'line-spacing
                                   (symbol-function 'set-buffer-redisplay))
             (setq line-spacing 2))"#,
    )
    .expect("line-spacing watcher should evaluate");
    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 2);

    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 2);
}

/// Finding 6: setting a display-affecting buffer variable must mark
/// redisplay dirty all by itself (no variable-watcher required), so the
/// next redisplay actually repaints. Without the fix the unchanged
/// `RedisplaySignature` short-circuits the callback and the screen stays
/// stale until the next keystroke (the "Doom blank pane" class of bug).
#[test]
fn setq_display_var_invalidates_redisplay_without_watcher() {
    crate::test_utils::init_test_tracing();

    for form in [
        "(setq truncate-lines t)",
        "(setq tab-width 16)",
        "(setq header-line-format \"hdr\")",
        "(setq cursor-type 'bar)",
        "(setq selective-display 4)",
        "(setq word-wrap t)",
    ] {
        let mut ev = Context::new();
        let redisplay_count = Rc::new(RefCell::new(0usize));
        let redisplay_count_in_cb = Rc::clone(&redisplay_count);
        ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
            *redisplay_count_in_cb.borrow_mut() += 1;
        }));

        ev.redisplay();
        assert_eq!(
            *redisplay_count.borrow(),
            1,
            "baseline redisplay for {form}"
        );

        ev.eval_str(form)
            .unwrap_or_else(|e| panic!("{form} should evaluate: {e:?}"));
        ev.redisplay();
        assert_eq!(
            *redisplay_count.borrow(),
            2,
            "{form} must invalidate redisplay so the next redisplay repaints"
        );

        // Idempotent: a second redisplay with no further change is a no-op.
        ev.redisplay();
        assert_eq!(
            *redisplay_count.borrow(),
            2,
            "{form}: redisplay must not re-fire when nothing changed"
        );
    }
}

/// Setting a non-display variable must NOT invalidate redisplay — the
/// curated set keeps us from over-triggering repaints. This is the
/// complement of the test above and guards the "don't over-trigger"
/// requirement.
#[test]
fn setq_non_display_var_does_not_invalidate_redisplay() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray
        .set_symbol_value("neo-test-counter", Value::fixnum(0));

    let redisplay_count = Rc::new(RefCell::new(0usize));
    let redisplay_count_in_cb = Rc::clone(&redisplay_count);
    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplay_count_in_cb.borrow_mut() += 1;
    }));

    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 1);

    ev.eval_str("(setq neo-test-counter 99)")
        .expect("plain setq should evaluate");
    ev.redisplay();
    assert_eq!(
        *redisplay_count.borrow(),
        1,
        "a non-display variable set must not force a redisplay"
    );
}

/// Finding 6: changing the DEFAULT of a display variable
/// (`setq-default` / `set-default`) must also mark redisplay dirty.
#[test]
fn set_default_display_var_invalidates_redisplay() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let redisplay_count = Rc::new(RefCell::new(0usize));
    let redisplay_count_in_cb = Rc::clone(&redisplay_count);
    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplay_count_in_cb.borrow_mut() += 1;
    }));

    ev.redisplay();
    assert_eq!(*redisplay_count.borrow(), 1);

    ev.eval_str("(set-default 'truncate-lines t)")
        .expect("set-default should evaluate");
    ev.redisplay();
    assert_eq!(
        *redisplay_count.borrow(),
        2,
        "set-default of a display variable must invalidate redisplay"
    );
}

/// The curated predicate is the single source of truth and must classify
/// the documented display variables correctly while excluding ordinary
/// variables.
#[test]
fn variable_affects_display_classifies_curated_set() {
    use crate::buffer::buffer::variable_affects_display;
    for name in [
        "truncate-lines",
        "tab-width",
        "header-line-format",
        "mode-line-format",
        "cursor-type",
        "line-spacing",
        "buffer-display-table",
        "selective-display",
        "truncate-partial-width-windows",
        "word-wrap",
    ] {
        assert!(
            variable_affects_display(name),
            "{name} should be classified as display-affecting"
        );
    }
    for name in [
        "blink-cursor-blinks-done",
        "neo-test-counter",
        "default-directory",
        "buffer-read-only",
        "case-fold-search",
    ] {
        assert!(
            !variable_affects_display(name),
            "{name} should NOT be classified as display-affecting"
        );
    }
}

#[test]
fn read_char_does_not_redisplay_again_when_monitor_change_arrives_after_pre_block_redisplay() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let redisplay_count = Rc::new(RefCell::new(0usize));
    let redisplay_count_in_cb = Rc::clone(&redisplay_count);

    let (tx, rx) = crossbeam_channel::unbounded();
    let tx_in_cb = tx.clone();
    let injected = Rc::new(RefCell::new(false));
    let injected_in_cb = Rc::clone(&injected);

    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplay_count_in_cb.borrow_mut() += 1;

        if !*injected_in_cb.borrow() {
            *injected_in_cb.borrow_mut() = true;
            tx_in_cb
                .send(crate::keyboard::InputEvent::MonitorsChanged {
                    monitors: vec![crate::emacs_core::builtins::NeomacsMonitorInfo {
                        x: 0,
                        y: 0,
                        width: 2560,
                        height: 1440,
                        scale: 1.25,
                        width_mm: 600,
                        height_mm: 340,
                        name: Some("DP-1".to_string()),
                    }],
                })
                .expect("enqueue monitor change after first redisplay");
            tx_in_cb
                .send(crate::keyboard::InputEvent::key_press(
                    crate::keyboard::KeyEvent::char('a'),
                ))
                .expect("enqueue keypress after monitor change");
        }
    }));

    ev.input_rx = Some(rx);

    let event = ev.read_char().expect("read_char should return a keypress");
    assert_eq!(event, Value::fixnum('a' as i64));
    assert_eq!(*redisplay_count.borrow(), 1);
}

#[test]
fn redisplay_applies_pending_resize_before_callback() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    assert_eq!(ev.frames.selected_frame().map(|frame| frame.id), Some(fid));

    let redisplay_calls = Rc::new(RefCell::new(Vec::new()));
    let redisplay_calls_in_cb = redisplay_calls.clone();
    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let frame = ev
            .frames
            .selected_frame()
            .expect("selected frame during redisplay");
        redisplay_calls_in_cb
            .borrow_mut()
            .push((frame.width, frame.height));
    }));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 700,
        height: 800,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    })
    .unwrap();

    ev.redisplay();

    assert_eq!(*redisplay_calls.borrow(), vec![(700, 800)]);
}

#[test]
fn redisplay_syncs_opening_gui_frame_size_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    ev.frames
        .get_mut(fid)
        .expect("frame should exist")
        .set_window_system(Some(Value::symbol("x")));

    let redisplay_calls = Rc::new(RefCell::new(Vec::new()));
    let redisplay_calls_in_cb = redisplay_calls.clone();
    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let frame = ev
            .frames
            .selected_frame()
            .expect("selected frame during redisplay");
        redisplay_calls_in_cb
            .borrow_mut()
            .push((frame.width, frame.height));
    }));

    ev.set_display_host(Box::new(RecordingDisplayHost::opening_with_primary_size(
        1500, 1900,
    )));

    ev.redisplay();

    assert_eq!(*redisplay_calls.borrow(), vec![(1500, 1900)]);
}

#[test]
fn recursive_edit_runs_top_level_before_outer_command_loop_reads_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let _ = ev.eval_str_each("(setq top-level '(setq neo-top-level-hit t))");

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::WindowClose { emacs_frame_id: 0 })
        .expect("queue close request");
    drop(tx);

    ev.input_rx = Some(rx);
    ev.command_loop.running = true;

    let result = ev
        .recursive_edit_inner()
        .expect("outer command loop should exit cleanly");
    assert_eq!(result, Value::NIL);
    assert!(
        ev.eval_symbol("neo-top-level-hit")
            .expect("top-level probe should be bound")
            .is_truthy(),
        "expected recursive_edit to evaluate `top-level' before waiting for input"
    );
}

#[test]
fn nested_recursive_edit_propagates_top_level_and_runs_unwind_cleanup() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*nested-recursive-edit*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    assert!(ev.frames.select_frame(frame), "test needs a selected frame");
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    let (tx, rx) = crossbeam_channel::unbounded();
    drop(tx);

    ev.input_rx = Some(rx);
    ev.command_loop.running = true;
    // Simulate an already-active outer command loop. The recursive-edit
    // below must not catch the top-level throw meant to unwind it.
    ev.command_loop.recursive_depth = 1;
    ev.eval_str(
        r#"(progn
             (setq neo-recursive-edit-cleanup nil)
             (fset 'neo-throw-top-level
                   (lambda () (interactive) (top-level)))
             (fset 'command-execute
                   (lambda (cmd &optional _record _keys _special)
                     (funcall cmd))))"#,
    )
    .expect("install nested recursive-edit test command");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('q' as i64)],
        Value::symbol("neo-throw-top-level"),
    )
    .expect("define top-level test command");
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('q' as i64));

    let result = ev.eval_str(
        "(unwind-protect
             (recursive-edit)
           (setq neo-recursive-edit-cleanup t))",
    );

    assert!(matches!(
        result,
        Err(crate::emacs_core::error::EvalError::UncaughtThrow { tag, .. })
            if tag == Value::symbol("top-level")
    ));
    assert_eq!(
        ev.eval_symbol("neo-recursive-edit-cleanup")
            .expect("read recursive-edit cleanup marker"),
        Value::T
    );
}

#[test]
fn command_loop_runs_initial_post_command_hook_before_first_command() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    fn stop_command_loop_for_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "stop helper should not receive arguments");
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-command-loop-for-test",
        NativeFn::ContextVec(stop_command_loop_for_test),
        SubrArity::new(0, Some(0)),
    ));

    let scratch = ev.buffers.create_buffer("*command-loop-prologue*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    assert!(
        ev.frames.select_frame(frame),
        "command loop test should have a selected frame"
    );

    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(progn
             (setq neo-initial-post-command-count 0)
             (setq neo-initial-post-command-inhibit-redisplay t)
             (setq inhibit-redisplay t)
             (fset 'neo-initial-post-command-hook
                   (lambda ()
                     (setq neo-initial-post-command-count
                           (1+ neo-initial-post-command-count))
                     (setq inhibit-redisplay nil)
                     (setq neo-initial-post-command-inhibit-redisplay
                           inhibit-redisplay)
                     (setq post-command-hook nil)))
             (setq post-command-hook '(neo-initial-post-command-hook))
             (fset 'neo-exit-command
                   (lambda ()
                     (interactive)
                     (neo-stop-command-loop-for-test)))
             (fset 'command-execute
                   (lambda (cmd &optional _record _keys _special)
                     (funcall cmd))))"#,
    )
    .expect("setup command-loop prologue test");

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('q' as i64)],
        Value::symbol("neo-exit-command"),
    )
    .expect("define exit command");
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('q' as i64));
    ev.command_loop.running = true;

    let result = ev
        .recursive_edit_inner()
        .expect("recursive edit should exit through command");
    assert_eq!(result, Value::NIL);
    assert_eq!(
        ev.eval_symbol("neo-initial-post-command-count")
            .expect("post-command count"),
        Value::fixnum(1)
    );
    assert_eq!(
        ev.eval_symbol("neo-initial-post-command-inhibit-redisplay")
            .expect("inhibit-redisplay observed in hook"),
        Value::NIL
    );
    assert_eq!(
        ev.eval_symbol("inhibit-redisplay")
            .expect("inhibit-redisplay should be restored"),
        Value::T
    );
}

#[test]
fn read_char_requeues_keypress_and_throws_on_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);
    ev.obarray
        .set_symbol_value("throw-on-input", Value::symbol("tag"));

    let flow = ev
        .read_char()
        .expect_err("throw-on-input should interrupt read_char");
    assert!(matches!(
        flow,
        Flow::Throw(ref thrown)
            if thrown.tag == Value::symbol("tag") && thrown.value == Value::T
    ));

    ev.obarray.set_symbol_value("throw-on-input", Value::NIL);
    let event = ev.read_char().expect("keypress should remain queued");
    assert_eq!(event, Value::fixnum('a' as i64));
}

#[test]
fn read_char_window_close_honors_throw_on_input_before_quit() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::WindowClose { emacs_frame_id: 0 })
        .expect("queue close request");
    ev.input_rx = Some(rx);
    ev.obarray
        .set_symbol_value("throw-on-input", Value::symbol("tag"));

    let flow = ev
        .read_char()
        .expect_err("throw-on-input should interrupt read_char");
    assert!(matches!(
        flow,
        Flow::Throw(ref thrown)
            if thrown.tag == Value::symbol("tag") && thrown.value == Value::T
    ));

    ev.obarray.set_symbol_value("throw-on-input", Value::NIL);
    let flow = ev
        .read_char()
        .expect_err("window close should still quit afterwards");
    assert!(matches!(flow, Flow::Signal(ref sig) if sig.symbol_name() == "quit"));
}

#[test]
fn read_char_window_close_uses_special_event_map_handler_when_loaded() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffer_manager_mut().create_buffer("*scratch*");
    ev.buffer_manager_mut().set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    install_minimal_special_event_command_runtime(&mut ev);

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::WindowClose {
        emacs_frame_id: frame.0,
    })
    .expect("queue window close");
    ev.input_rx = Some(rx);
    ev.command_loop.running = true;

    let event = match ev.read_char_with_timeout(Some(Duration::from_millis(0))) {
        Ok(event) => event,
        Err(flow) => panic!(
            "window close should be consumed without error, got flow={flow:?} logged={:?}",
            ev.eval_symbol("neo-last-delete-frame-event")
        ),
    };
    assert_eq!(event, None);
    drop(tx);
    let logged = ev
        .eval_symbol("neo-last-delete-frame-event")
        .expect("delete-frame event should be logged");
    assert_eq!(
        logged,
        Value::list(vec![
            Value::symbol("delete-frame"),
            Value::list(vec![Value::make_frame(frame.0)]),
        ]),
    );
}

#[test]
fn read_char_disconnected_input_uses_noelisp_terminal_teardown() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::terminal::pure::reset_terminal_thread_locals();
    let mut ev = Context::new();
    let scratch = ev.buffer_manager_mut().create_buffer("*scratch*");
    ev.buffer_manager_mut().set_current(scratch);
    let _frame = ev.frame_manager_mut().create_frame_on_terminal(
        "F1",
        crate::emacs_core::terminal::pure::TERMINAL_ID,
        80,
        25,
        scratch,
    );
    let (tx, rx) = crossbeam_channel::unbounded::<crate::keyboard::InputEvent>();
    ev.input_rx = Some(rx);
    drop(tx);

    ev.eval_str(
        r#"
(setq hook-log nil)
(setq delete-terminal-functions
      (list (lambda (term)
              (setq hook-log
                    (cons (list 'terminal (terminal-live-p term)) hook-log)))))
(setq delete-frame-functions
      (list (lambda (frame)
              (setq hook-log
                    (cons (list 'before (frame-live-p frame)) hook-log)))))
(setq after-delete-frame-functions
      (list (lambda (frame)
              (setq hook-log
                    (cons (list 'after (frame-live-p frame)) hook-log)))))
"#,
    )
    .expect("install disconnected input hook setup");

    let flow = ev
        .read_char()
        .expect_err("disconnected input should unwind read_char");
    assert!(matches!(flow, Flow::Signal(ref sig) if sig.symbol_name() == "quit"));
    assert_eq!(
        ev.shutdown_request().map(|request| request.exit_code),
        Some(0)
    );
    assert!(ev.frame_manager().frame_list().is_empty());
    assert!(
        crate::emacs_core::terminal::pure::builtin_terminal_live_p(
            &mut ev,
            vec![crate::emacs_core::terminal::pure::terminal_handle_value()]
        )
        .unwrap()
        .is_nil(),
        "disconnected input should tear down the display terminal via noelisp delete"
    );
    assert_eq!(
        ev.eval_str("hook-log").expect("hook-log before flush"),
        Value::NIL
    );

    ev.flush_pending_safe_funcalls();

    let post_flush = ev
        .eval_str("(nreverse hook-log)")
        .expect("hook-log after flush");
    assert_eq!(
        format!("{}", post_flush),
        "((after nil) (before nil) (terminal nil))"
    );
}

#[test]
fn eval_list_form_throws_on_pending_host_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);
    ev.obarray
        .set_symbol_value("throw-on-input", Value::symbol("tag"));

    let result = ev.eval_str("(list 1 2)");
    assert!(matches!(
        result,
        Err(EvalError::UncaughtThrow { tag, value, .. })
            if tag == Value::symbol("tag") && value == Value::T
    ));

    ev.obarray.set_symbol_value("throw-on-input", Value::NIL);
    let event = ev.read_char().expect("keypress should remain queued");
    assert_eq!(event, Value::fixnum('a' as i64));
}

#[test]
fn presentation_retirement_does_not_interrupt_while_no_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::PresentationRetired { presentation: 7 })
        .expect("queue presentation retirement");
    ev.input_rx = Some(rx);
    ev.obarray
        .set_symbol_value("throw-on-input", Value::symbol("tag"));

    let result = ev
        .eval_str("(list 1 2)")
        .expect("renderer lifecycle acknowledgement must not interrupt evaluation");

    assert_eq!(format!("{result}"), "(1 2)");
}

#[test]
fn input_pending_services_presentation_retirement_without_redisplay() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let presentation = ev.begin_interaction_presentation();
    let interaction = ev.register_presented_mouse_target(
        presentation,
        crate::keyboard::PresentedMouseTarget {
            area: crate::keyboard::PresentedMouseArea::TabBar,
            posn_string: Value::cons(Value::string("tab"), Value::fixnum(0)),
        },
    );
    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::PresentationRetired { presentation });
    let redisplays = Rc::new(RefCell::new(0));
    let redisplays_in_callback = Rc::clone(&redisplays);
    ev.redisplay_fn = Some(Box::new(move |_| {
        *redisplays_in_callback.borrow_mut() += 1;
    }));

    let pending = crate::emacs_core::reader::builtin_input_pending_p(&mut ev, vec![])
        .expect("input-pending-p should service internal events");

    assert_eq!(pending, Value::NIL);
    assert_eq!(
        ev.resolve_presented_mouse_target(presentation, interaction),
        None
    );
    assert_eq!(*redisplays.borrow(), 0);
    assert!(ev.command_loop.keyboard.pending_input_events.is_empty());
}

#[test]
fn presentation_retirement_does_not_preempt_work_or_reset_idle_epoch() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.timer_start_idle();
    assert!(ev.idle_timer_running());
    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::PresentationRetired { presentation: 99 });

    let command_pending = ev
        .stage_pending_command_input_for_wait_request()
        .expect("internal event service should succeed");

    assert!(!command_pending);
    assert!(ev.idle_timer_running());
    assert!(ev.command_loop.keyboard.pending_input_events.is_empty());
}

#[test]
fn presentation_retirement_does_not_preempt_timer_batches() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let when = SystemTime::now()
        .checked_sub(Duration::from_millis(1))
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .expect("timer deadline");
    let timer = Value::vector(vec![
        Value::NIL,
        Value::fixnum((when.as_secs() as i64) >> 16),
        Value::fixnum((when.as_secs() as i64) & 0xFFFF),
        Value::fixnum(when.subsec_micros() as i64),
        Value::NIL,
        Value::symbol("neo-retirement-timer-callback"),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ]);
    ev.set_variable("neo-retirement-timer", timer);
    ev.eval_str(
        r#"(progn
             (setq neo-retirement-timer-count 0)
             (fset 'neo-retirement-timer-callback
                   (lambda ()
                     (setq neo-retirement-timer-count
                           (1+ neo-retirement-timer-count))
                     (if (< neo-retirement-timer-count 3)
                         (progn
                           (aset neo-retirement-timer 0 nil)
                           (setq timer-list (list neo-retirement-timer))))))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install timer-batch probe");
    ev.set_variable("timer-list", Value::list(vec![timer]));
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::PresentationRetired { presentation: 77 })
        .expect("queue renderer acknowledgement");
    ev.input_rx = Some(rx);

    let outcome = ev
        .wait_for_command_input(Some(std::time::Instant::now() + Duration::from_millis(50)))
        .expect("wait should reach its deadline");

    assert_eq!(
        outcome,
        crate::emacs_core::wait::CommandInputWaitOutcome::DeadlineElapsed
    );
    assert_eq!(
        ev.eval_symbol("neo-retirement-timer-count")
            .expect("timer count"),
        Value::fixnum(3),
        "the internal acknowledgement must not end the wait between timer batches"
    );
}

#[test]
fn retirement_wakeup_returns_to_same_blocking_read_without_redisplay_loop() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    let redisplays = Rc::new(RefCell::new(0));
    let redisplays_in_callback = Rc::clone(&redisplays);
    let retirement_tx = tx.clone();
    ev.redisplay_fn = Some(Box::new(move |_| {
        *redisplays_in_callback.borrow_mut() += 1;
        retirement_tx
            .send(crate::keyboard::InputEvent::PresentationRetired { presentation: 42 })
            .expect("queue renderer acknowledgement");
    }));

    let key_sender = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('x'),
        ))
        .expect("queue delayed key");
    });

    let event = ev
        .read_char_with_timeout(Some(std::time::Duration::from_secs(1)))
        .expect("read should succeed");
    key_sender.join().expect("key sender should finish");

    assert_eq!(event, Some(Value::fixnum('x' as i64)));
    assert_eq!(
        *redisplays.borrow(),
        1,
        "a lifecycle acknowledgement must resume the existing wait, not restart redisplay"
    );
}

#[test]
fn frame_native_width_syncs_pending_resize_without_read_char() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    ev.frames
        .get_mut(fid)
        .expect("frame should exist")
        .set_parameter(Value::symbol("window-system"), Value::symbol("x"));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 700,
        height: 800,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    })
    .unwrap();

    let width = crate::emacs_core::frame::builtin_frame_native_width(&mut ev, vec![])
        .expect("frame-native-width should succeed");
    let height = crate::emacs_core::frame::builtin_frame_native_height(&mut ev, vec![])
        .expect("frame-native-height should succeed");

    assert_eq!(width, Value::fixnum(700));
    assert_eq!(height, Value::fixnum(800));
}

#[test]
fn fire_pending_timers_does_not_service_pending_resize_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 700,
        height: 800,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    })
    .unwrap();

    ev.fire_pending_timers();

    let frame = ev.frames.get(fid).expect("frame should exist");
    assert_eq!(frame.width, 960);
    assert_eq!(frame.height, 640);
}

#[test]
fn wait_for_pending_resize_events_blocks_until_resize_and_preserves_keypress() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    assert_eq!(ev.frames.selected_frame().map(|frame| frame.id), Some(fid));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    // Keep one sender alive: dropping the last tx disconnects the channel,
    // which the input machinery treats as terminal-gone -> quit (timing flake;
    // see the sit-for soak fix).
    let _tx_keepalive = tx.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        tx.send(crate::keyboard::InputEvent::Resize {
            width: 700,
            height: 800,
            scale_factor: 1.0,
            emacs_frame_id: 0,
        })
        .expect("send resize");
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('r'),
        ))
        .expect("send following keypress");
    });

    assert!(ev.wait_for_pending_resize_events(Duration::from_secs(1)));
    let frame = ev.frames.get(fid).expect("frame should exist");
    assert_eq!(frame.width, 700);
    assert_eq!(frame.height, 800);

    let event = ev
        .read_char()
        .expect("following keypress should remain queued");
    assert_eq!(event, Value::fixnum('r' as i64));
}

#[test]
fn frame_native_width_syncs_pending_resize_behind_focus_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    ev.frames
        .get_mut(fid)
        .expect("frame should exist")
        .set_parameter(Value::symbol("window-system"), Value::symbol("x"));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Focus {
        focused: true,
        emacs_frame_id: 0,
    })
    .unwrap();
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 700,
        height: 800,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    })
    .unwrap();

    let width = crate::emacs_core::frame::builtin_frame_native_width(&mut ev, vec![])
        .expect("frame-native-width should succeed");
    let height = crate::emacs_core::frame::builtin_frame_native_height(&mut ev, vec![])
        .expect("frame-native-height should succeed");

    assert_eq!(width, Value::fixnum(700));
    assert_eq!(height, Value::fixnum(800));
}

#[test]
fn redisplay_applies_resize_already_queued_behind_focus_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    assert_eq!(ev.frames.selected_frame().map(|frame| frame.id), Some(fid));

    let redisplay_calls = Rc::new(RefCell::new(Vec::new()));
    let redisplay_calls_in_cb = redisplay_calls.clone();
    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let frame = ev
            .frames
            .selected_frame()
            .expect("selected frame during redisplay");
        redisplay_calls_in_cb
            .borrow_mut()
            .push((frame.width, frame.height));
    }));

    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::Focus {
            focused: true,
            emacs_frame_id: 0,
        });
    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::Resize {
            width: 700,
            height: 800,
            scale_factor: 1.0,
            emacs_frame_id: 0,
        });

    ev.redisplay();

    assert_eq!(*redisplay_calls.borrow(), vec![(700, 800)]);
    assert!(matches!(
        ev.command_loop.keyboard.pending_input_events.front(),
        Some(crate::keyboard::InputEvent::Focus {
            focused: true,
            emacs_frame_id: 0
        })
    ));
    assert_eq!(ev.command_loop.keyboard.pending_input_events.len(), 1);
}

#[test]
fn read_char_preserves_keypress_after_queued_focus_and_resize() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    assert_eq!(ev.frames.selected_frame().map(|frame| frame.id), Some(fid));

    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::Focus {
            focused: true,
            emacs_frame_id: 0,
        });
    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::Resize {
            width: 700,
            height: 800,
            scale_factor: 1.0,
            emacs_frame_id: 0,
        });
    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::char('a')),
    );

    let event = ev.read_char().expect("read_char should return a keypress");
    assert_eq!(event, Value::fixnum('a' as i64));

    let frame = ev.frames.get(fid).expect("frame should still be live");
    assert_eq!(frame.width, 700);
    assert_eq!(frame.height, 800);
}

#[test]
fn keyboard_runtime_starts_with_terminal_translation_maps_from_context_bootstrap() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();

    assert_eq!(
        ev.command_loop.keyboard.input_decode_map(),
        ev.eval_symbol("input-decode-map")
            .expect("input-decode-map should be bound")
    );
    assert_eq!(
        ev.command_loop.keyboard.local_function_key_map(),
        ev.eval_symbol("local-function-key-map")
            .expect("local-function-key-map should be bound")
    );
}

#[test]
fn assigning_terminal_translation_maps_updates_keyboard_runtime_owner() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let input_decode_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    let local_function_key_map = crate::emacs_core::keymap::make_sparse_list_keymap();

    ev.assign("input-decode-map", input_decode_map);
    ev.assign("local-function-key-map", local_function_key_map);

    assert_eq!(
        ev.command_loop.keyboard.input_decode_map(),
        input_decode_map
    );
    assert_eq!(
        ev.command_loop.keyboard.local_function_key_map(),
        local_function_key_map
    );
}

#[test]
fn read_key_sequence_prefers_bound_gui_return_before_ascii_fallback() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("return")],
        Value::symbol("gui-return-command"),
    )
    .expect("define symbolic Return binding");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('\r' as i64)],
        Value::symbol("ascii-ret-command"),
    )
    .expect("define ASCII RET binding");
    let local_function_key_map = ev
        .eval_symbol("local-function-key-map")
        .expect("local-function-key-map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        local_function_key_map,
        &[Value::symbol("return")],
        Value::vector(vec![Value::fixnum('\r' as i64)]),
    )
    .expect("define GNU Return fallback");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::named(
            crate::keyboard::NamedKey::Return,
        )),
    );

    let (keys, binding) = ev.read_key_sequence().expect("read GUI Return");

    assert_eq!(keys, vec![Value::symbol("return")]);
    assert_eq!(binding, Value::symbol("gui-return-command"));
}

#[test]
fn read_key_sequence_prefers_bound_gui_tab_before_ascii_fallback() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("tab")],
        Value::symbol("gui-tab-command"),
    )
    .expect("define symbolic Tab binding");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('\t' as i64)],
        Value::symbol("ascii-tab-command"),
    )
    .expect("define ASCII TAB binding");
    let local_function_key_map = ev
        .eval_symbol("local-function-key-map")
        .expect("local-function-key-map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        local_function_key_map,
        &[Value::symbol("tab")],
        Value::vector(vec![Value::fixnum('\t' as i64)]),
    )
    .expect("define GNU Tab fallback");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::named(
            crate::keyboard::NamedKey::Tab,
        )),
    );

    let (keys, binding) = ev.read_key_sequence().expect("read GUI Tab");

    assert_eq!(keys, vec![Value::symbol("tab")]);
    assert_eq!(binding, Value::symbol("gui-tab-command"));
}

#[test]
fn read_key_sequence_falls_back_from_unbound_gui_return_to_ascii_ret() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('\r' as i64)],
        Value::symbol("ascii-ret-command"),
    )
    .expect("define ASCII RET binding");
    let local_function_key_map = ev
        .eval_symbol("local-function-key-map")
        .expect("local-function-key-map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        local_function_key_map,
        &[Value::symbol("return")],
        Value::vector(vec![Value::fixnum('\r' as i64)]),
    )
    .expect("define GNU Return fallback");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::named(
            crate::keyboard::NamedKey::Return,
        )),
    );

    let (keys, binding) = ev.read_key_sequence().expect("read GUI Return");

    assert_eq!(keys, vec![Value::fixnum('\r' as i64)]);
    assert_eq!(binding, Value::symbol("ascii-ret-command"));
}

#[test]
fn read_key_sequence_falls_back_from_unbound_gui_tab_to_ascii_tab() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('\t' as i64)],
        Value::symbol("ascii-tab-command"),
    )
    .expect("define ASCII TAB binding");
    let local_function_key_map = ev
        .eval_symbol("local-function-key-map")
        .expect("local-function-key-map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        local_function_key_map,
        &[Value::symbol("tab")],
        Value::vector(vec![Value::fixnum('\t' as i64)]),
    )
    .expect("define GNU Tab fallback");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::named(
            crate::keyboard::NamedKey::Tab,
        )),
    );

    let (keys, binding) = ev.read_key_sequence().expect("read GUI Tab");

    assert_eq!(keys, vec![Value::fixnum('\t' as i64)]);
    assert_eq!(binding, Value::symbol("ascii-tab-command"));
}

#[test]
fn read_key_sequence_prefers_bound_gui_escape_before_ascii_fallback() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("escape")],
        Value::symbol("gui-escape-command"),
    )
    .expect("define symbolic Escape binding");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum(27)],
        Value::symbol("ascii-esc-command"),
    )
    .expect("define ASCII ESC binding");
    let local_function_key_map = ev
        .eval_symbol("local-function-key-map")
        .expect("local-function-key-map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        local_function_key_map,
        &[Value::symbol("escape")],
        Value::vector(vec![Value::fixnum(27)]),
    )
    .expect("define GNU Escape fallback");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::named(
            crate::keyboard::NamedKey::Escape,
        )),
    );

    let (keys, binding) = ev.read_key_sequence().expect("read GUI Escape");

    assert_eq!(keys, vec![Value::symbol("escape")]);
    assert_eq!(binding, Value::symbol("gui-escape-command"));
}

#[test]
fn read_key_sequence_falls_back_from_unbound_gui_escape_to_ascii_esc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum(27)],
        Value::symbol("ascii-esc-command"),
    )
    .expect("define ASCII ESC binding");
    let local_function_key_map = ev
        .eval_symbol("local-function-key-map")
        .expect("local-function-key-map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        local_function_key_map,
        &[Value::symbol("escape")],
        Value::vector(vec![Value::fixnum(27)]),
    )
    .expect("define GNU Escape fallback");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::named(
            crate::keyboard::NamedKey::Escape,
        )),
    );

    let (keys, binding) = ev.read_key_sequence().expect("read GUI Escape");

    assert_eq!(keys, vec![Value::fixnum(27)]);
    assert_eq!(binding, Value::symbol("ascii-esc-command"));
}

#[test]
fn read_key_sequence_function_translation_receives_prompt() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(progn
             (setq neomacs-test-read-key-sequence-prompt nil)
             (fset 'neomacs-test-read-key-sequence-command
                   (lambda () (interactive) 'ok))
             (fset 'neomacs-test-key-translation
                   (lambda (prompt)
                     (setq neomacs-test-read-key-sequence-prompt prompt)
                     [f1])))"#,
    )
    .expect("setup");

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("f1")],
        Value::symbol("neomacs-test-read-key-sequence-command"),
    )
    .expect("define translated command");

    let key_translation_map = ev
        .eval_symbol("key-translation-map")
        .expect("key-translation-map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        key_translation_map,
        &[Value::fixnum('a' as i64)],
        Value::symbol("neomacs-test-key-translation"),
    )
    .expect("define translation");

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('a' as i64));

    let (keys, binding) = ev
        .read_key_sequence_with_options(crate::keyboard::ReadKeySequenceOptions::new(
            Value::string("Prompt> "),
            false,
            false,
            false,
        ))
        .expect("read translated key sequence");

    assert_eq!(keys, vec![Value::symbol("f1")]);
    assert_eq!(
        binding,
        Value::symbol("neomacs-test-read-key-sequence-command")
    );

    let prompt = ev
        .eval_str("neomacs-test-read-key-sequence-prompt")
        .expect("prompt should evaluate");
    assert_eq!(prompt, Value::string("Prompt> "));
}

/// GNU keeps `read_key_sequence`'s PROMPT as the original Lisp string in the
/// kboard's `echo_prompt`.  In particular, `help--help-screen` relies on the
/// prompt's `face` intervals surviving while it waits for the next help key.
#[test]
fn read_key_sequence_prompt_preserves_text_properties_in_echo_area() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("noninteractive", Value::NIL);
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('a' as i64)],
        Value::symbol("neomacs-test-prompt-command"),
    )
    .expect("define prompt test command");

    let face = Value::symbol("help-key-binding");
    let prompt = Value::string_with_text_properties(
        "Prompt> ",
        vec![crate::emacs_core::value::StringTextPropertyRun {
            start: 0,
            end: "Prompt> ".len(),
            plist: Value::list(vec![Value::symbol("face"), face]),
        }],
    );
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('a' as i64));

    ev.read_key_sequence_with_options(crate::keyboard::ReadKeySequenceOptions::new(
        prompt, false, false, false,
    ))
    .expect("read key sequence");

    let message = ev.current_message_value().expect("prompt echo message");
    let string = message.as_lisp_string().expect("string echo message");
    assert_eq!(
        string
            .intervals()
            .get_property_at_char_pos(crate::buffer::CharPos0::ZERO, Value::symbol("face")),
        Some(face)
    );
}

#[test]
fn read_key_sequence_continues_through_pending_suffix_translation_prefix() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-test-suffix-translation-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('a' as i64), Value::symbol("f1")],
        Value::symbol("neomacs-test-suffix-translation-command"),
    )
    .expect("define suffix command");

    let input_decode_map = ev
        .eval_symbol("input-decode-map")
        .expect("input-decode-map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        input_decode_map,
        &[Value::fixnum('b' as i64), Value::fixnum('c' as i64)],
        Value::vector(vec![Value::symbol("f1")]),
    )
    .expect("define input-decode suffix translation");

    for event in [
        Value::fixnum('a' as i64),
        Value::fixnum('b' as i64),
        Value::fixnum('c' as i64),
    ] {
        ev.command_loop
            .keyboard
            .kboard
            .unread_events
            .push_back(event);
    }

    let (keys, binding) = ev
        .read_key_sequence()
        .expect("read suffix-translated sequence");
    assert_eq!(keys, vec![Value::fixnum('a' as i64), Value::symbol("f1")]);
    assert_eq!(
        binding,
        Value::symbol("neomacs-test-suffix-translation-command")
    );
}

#[test]
fn fast_read_key_sequence_does_not_publish_or_log_a_prefix_echo() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("noninteractive", Value::NIL);
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-test-prefix-target-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup prefix target command");

    let sequence =
        crate::keyboard::KeySequence::from_description("C-x C-f").expect("C-x C-f key sequence");
    let events = sequence
        .events
        .iter()
        .map(crate::keyboard::KeyEvent::to_emacs_event_value)
        .collect::<Vec<_>>();
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &events,
        Value::symbol("neomacs-test-prefix-target-command"),
    )
    .expect("define prefix command");
    for event in events {
        ev.command_loop
            .keyboard
            .kboard
            .unread_events
            .push_back(event);
    }

    let (_keys, binding) = ev.read_key_sequence().expect("read prefixed key sequence");
    assert_eq!(binding, Value::symbol("neomacs-test-prefix-target-command"));
    assert!(
        ev.current_message_text().is_none(),
        "GNU only publishes a prefix after echo-keystrokes elapses without input"
    );
    if let Some(messages_id) = ev.buffers.find_buffer_by_name("*Messages*") {
        let messages = ev.buffers.get(messages_id).expect("*Messages* live");
        assert!(
            !messages.buffer_string().contains("C-x"),
            "GNU prefix-key echo uses message3_nolog and must not log to *Messages*"
        );
    }
}

#[test]
fn prefix_without_a_blocking_input_source_is_not_eagerly_echoed() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("noninteractive", Value::NIL);
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.assign("help-char", Value::fixnum(8));
    ev.assign("help-event-list", Value::list(vec![Value::symbol("help")]));
    ev.assign("echo-keystrokes", Value::fixnum(1));
    ev.assign("echo-keystrokes-help", Value::T);
    ev.eval_str(
        r#"(fset 'neomacs-test-prefix-target-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup prefix target command");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum(' ' as i64), Value::fixnum('f' as i64)],
        Value::symbol("neomacs-test-prefix-target-command"),
    )
    .expect("define prefix command");
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum(' ' as i64));

    let _ = ev.read_key_sequence();

    assert_eq!(ev.current_message_text(), None);
}

#[test]
fn help_prefix_without_a_blocking_input_source_is_not_eagerly_echoed() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("noninteractive", Value::NIL);
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.assign("help-char", Value::fixnum(8));
    ev.assign("echo-keystrokes", Value::fixnum(1));
    ev.eval_str(
        r#"(fset 'neomacs-test-help-target-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup help target command");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum(8), Value::fixnum('?' as i64)],
        Value::symbol("neomacs-test-help-target-command"),
    )
    .expect("define help prefix command");
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum(8));

    let _ = ev.read_key_sequence();

    assert_eq!(ev.current_message_text(), None);
}

#[test]
fn read_key_sequence_shift_translates_uppercase_binding() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-test-shift-translation-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('a' as i64)],
        Value::symbol("neomacs-test-shift-translation-command"),
    )
    .expect("define lowercase command");

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('A' as i64));

    let (keys, binding) = ev.read_key_sequence().expect("read shifted key");

    assert_eq!(keys, vec![Value::fixnum('a' as i64)]);
    assert_eq!(
        binding,
        Value::symbol("neomacs-test-shift-translation-command")
    );
    assert_eq!(
        ev.eval_symbol("this-command-keys-shift-translated")
            .expect("shift translation flag"),
        Value::T
    );
}

#[test]
fn read_key_sequence_dont_downcase_last_restores_original_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-test-shift-translation-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('a' as i64)],
        Value::symbol("neomacs-test-shift-translation-command"),
    )
    .expect("define lowercase command");

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('A' as i64));

    let (keys, binding) = ev
        .read_key_sequence_with_options(crate::keyboard::ReadKeySequenceOptions::new(
            Value::NIL,
            false,
            true,
            false,
        ))
        .expect("read shifted key without downcasing");

    assert_eq!(keys, vec![Value::fixnum('A' as i64)]);
    assert_eq!(
        binding,
        Value::symbol("neomacs-test-shift-translation-command")
    );
    assert_eq!(
        ev.eval_symbol("this-command-keys-shift-translated")
            .expect("shift translation flag"),
        Value::NIL
    );
}

#[test]
fn read_key_sequence_undefined_shift_translation_restores_original_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.assign(
        "global-map",
        crate::emacs_core::keymap::make_sparse_list_keymap(),
    );

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('A' as i64));

    let (keys, binding) = ev.read_key_sequence().expect("read undefined shifted key");

    assert_eq!(keys, vec![Value::fixnum('A' as i64)]);
    assert_eq!(binding, Value::symbol("self-insert-command"));
    assert_eq!(
        ev.eval_symbol("this-command-keys-shift-translated")
            .expect("shift translation flag"),
        Value::NIL
    );
}

#[test]
fn read_key_sequence_shift_translates_shifted_function_key() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-test-shifted-function-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("f1")],
        Value::symbol("neomacs-test-shifted-function-command"),
    )
    .expect("define function-key command");

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::symbol("S-f1"));

    let (keys, binding) = ev
        .read_key_sequence()
        .expect("read shifted function-key sequence");

    assert_eq!(keys, vec![Value::symbol("f1")]);
    assert_eq!(
        binding,
        Value::symbol("neomacs-test-shifted-function-command")
    );
    assert_eq!(
        ev.eval_symbol("this-command-keys-shift-translated")
            .expect("shift translation flag"),
        Value::T
    );
}

#[test]
fn read_char_returns_lispy_switch_frame_for_focus_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    install_minimal_special_event_command_runtime(&mut ev);
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let target_buffer = ev.buffers.create_buffer("focus-target");
    let target_frame = ev.frames.create_frame("F2", 960, 640, target_buffer).0;

    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::Focus {
            focused: true,
            emacs_frame_id: target_frame,
        });

    let event = ev
        .read_char()
        .expect("read_char should surface switch-frame");
    assert_eq!(
        event,
        Value::list(vec![
            Value::symbol("switch-frame"),
            Value::make_frame(target_frame),
        ])
    );
}

#[test]
fn raw_tty_input_switches_to_the_frame_owned_by_that_terminal_before_the_character() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    install_minimal_special_event_command_runtime(&mut ev);
    let first = crate::emacs_core::window_cmds::make_frame_plain_on_terminal(
        &mut ev.frames,
        &mut ev.buffers,
        vec![Value::NIL],
        0,
        80,
        25,
    )
    .expect("first terminal frame");
    let first_on_secondary = crate::emacs_core::window_cmds::make_frame_plain_on_terminal(
        &mut ev.frames,
        &mut ev.buffers,
        vec![Value::NIL],
        17,
        80,
        25,
    )
    .expect("secondary terminal frame");
    let second_on_secondary = crate::emacs_core::window_cmds::make_frame_plain_on_terminal(
        &mut ev.frames,
        &mut ev.buffers,
        vec![Value::NIL],
        17,
        80,
        25,
    )
    .expect("second frame on secondary terminal");
    let first_id = first.as_frame_id().expect("first frame id");
    let first_secondary_id = first_on_secondary
        .as_frame_id()
        .expect("first secondary frame id");
    let second_id = second_on_secondary
        .as_frame_id()
        .expect("second secondary frame id");
    assert_ne!(first_secondary_id, second_id);
    ev.frames.select_frame(crate::window::FrameId(second_id));
    ev.frames.select_frame(crate::window::FrameId(first_id));
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.init_input_system(rx);
    tx.send(crate::keyboard::InputEvent::raw_tty_bytes_for_terminal(
        b"x".to_vec(),
        17,
    ))
    .expect("queue secondary TTY input");

    assert_eq!(
        ev.read_char().expect("switch-frame before character"),
        Value::list(vec![
            Value::symbol("switch-frame"),
            Value::make_frame(second_id),
        ])
    );
    assert_eq!(
        ev.read_char().expect("secondary TTY character"),
        Value::fixnum('x' as i64)
    );
}

#[test]
fn read_key_sequence_defers_switch_frame_until_after_current_key_sequence() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    install_minimal_special_event_command_runtime(&mut ev);
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let target_buffer = ev.buffers.create_buffer("focus-target");
    let target_frame = ev.frames.create_frame("F2", 960, 640, target_buffer).0;
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-test-switch-frame-deferred-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('a' as i64), Value::fixnum('b' as i64)],
        Value::symbol("neomacs-test-switch-frame-deferred-command"),
    )
    .expect("define command");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::char('a')),
    );
    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::Focus {
            focused: true,
            emacs_frame_id: target_frame,
        });
    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::char('b')),
    );

    let (keys, binding) = ev.read_key_sequence().expect("read key sequence");
    assert_eq!(
        keys,
        vec![Value::fixnum('a' as i64), Value::fixnum('b' as i64)]
    );
    assert_eq!(
        binding,
        Value::symbol("neomacs-test-switch-frame-deferred-command")
    );

    let deferred = ev
        .read_char()
        .expect("deferred switch-frame should be unread first");
    assert_eq!(
        deferred,
        Value::list(vec![
            Value::symbol("switch-frame"),
            Value::make_frame(target_frame),
        ])
    );
}

#[test]
fn read_key_sequence_can_return_switch_frame_at_sequence_start() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    install_minimal_special_event_command_runtime(&mut ev);
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let target_buffer = ev.buffers.create_buffer("focus-target");
    let target_frame = ev.frames.create_frame("F2", 960, 640, target_buffer).0;
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("switch-frame")],
        Value::symbol("handle-switch-frame"),
    )
    .expect("define switch-frame binding");

    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::Focus {
            focused: true,
            emacs_frame_id: target_frame,
        });

    let (keys, binding) = ev
        .read_key_sequence_with_options(crate::keyboard::ReadKeySequenceOptions::new(
            Value::NIL,
            false,
            false,
            true,
        ))
        .expect("read switch-frame sequence");

    assert_eq!(
        keys,
        vec![Value::list(vec![
            Value::symbol("switch-frame"),
            Value::make_frame(target_frame),
        ])]
    );
    assert_eq!(binding, Value::symbol("handle-switch-frame"));
}

#[test]
fn special_event_map_bootstraps_delete_frame_and_focus_handlers() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();
    let special_event_map = ev
        .eval_symbol("special-event-map")
        .expect("special-event-map should be bound");

    let delete_frame = crate::emacs_core::keymap::lookup_key_in_keymaps_in_obarray(
        ev.obarray(),
        &[special_event_map],
        &[Value::symbol("delete-frame")],
        true,
    );
    let focus_in = crate::emacs_core::keymap::lookup_key_in_keymaps_in_obarray(
        ev.obarray(),
        &[special_event_map],
        &[Value::symbol("focus-in")],
        true,
    );
    let focus_out = crate::emacs_core::keymap::lookup_key_in_keymaps_in_obarray(
        ev.obarray(),
        &[special_event_map],
        &[Value::symbol("focus-out")],
        true,
    );
    let file_notify = crate::emacs_core::keymap::lookup_key_in_keymaps_in_obarray(
        ev.obarray(),
        &[special_event_map],
        &[Value::symbol("file-notify")],
        true,
    );

    assert_eq!(delete_frame, Value::symbol("handle-delete-frame"));
    assert_eq!(focus_in, Value::symbol("handle-focus-in"));
    assert_eq!(focus_out, Value::symbol("handle-focus-out"));
    assert_eq!(file_notify, Value::symbol("file-notify-handle-event"));
}

#[test]
fn read_char_updates_monitor_snapshot_and_runs_display_monitor_hooks() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (setq monitor-hook-terminal nil)
             (setq display-monitors-changed-functions
                   (list (lambda (terminal)
                           (setq monitor-hook-terminal terminal)))))"#,
    )
    .expect("install display monitor hook");
    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MonitorsChanged {
            monitors: vec![crate::emacs_core::builtins::NeomacsMonitorInfo {
                x: 10,
                y: 20,
                width: 2560,
                height: 1440,
                scale: 1.25,
                width_mm: 600,
                height_mm: 340,
                name: Some("DP-1".to_string()),
            }],
        },
    );
    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::char('x')),
    );

    let event = ev
        .read_char()
        .expect("read_char should continue past monitor change event");
    assert_eq!(event, Value::fixnum('x' as i64));

    let snapshot = crate::emacs_core::builtins::neomacs_monitor_info_snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].name.as_deref(), Some("DP-1"));
    assert_eq!(snapshot[0].width, 2560);
    assert_eq!(snapshot[0].height, 1440);

    assert_eq!(
        ev.eval_str("monitor-hook-terminal")
            .expect("display monitor hook terminal"),
        crate::emacs_core::terminal::pure::terminal_handle_value()
    );
}

#[test]
fn read_char_returns_lispy_select_window_for_transport_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let fid = ev.frames.selected_frame().expect("selected frame").id;
    let w1 = ev.frames.get(fid).expect("frame").window_list()[0];
    let other_buffer = ev.buffers.create_buffer("select-window-target");
    let w2 = ev
        .frames
        .split_window(
            fid,
            w1,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");

    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::SelectWindow { window_id: w2 });

    let event = ev
        .read_char()
        .expect("read_char should surface select-window");
    assert_eq!(
        event,
        Value::list(vec![
            Value::symbol("select-window"),
            Value::list(vec![Value::make_window(w2.0)]),
        ])
    );
}

#[test]
fn read_key_sequence_defers_select_window_until_after_current_key_sequence() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let fid = ev.frames.selected_frame().expect("selected frame").id;
    let w1 = ev.frames.get(fid).expect("frame").window_list()[0];
    let other_buffer = ev.buffers.create_buffer("select-window-target");
    let w2 = ev
        .frames
        .split_window(
            fid,
            w1,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");

    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-test-select-window-deferred-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("parse");
    ev.eval_str(
        r#"(fset 'neomacs-test-select-window-deferred-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('a' as i64), Value::fixnum('b' as i64)],
        Value::symbol("neomacs-test-select-window-deferred-command"),
    )
    .expect("define command");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::char('a')),
    );
    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::SelectWindow { window_id: w2 });
    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::char('b')),
    );

    let (keys, binding) = ev.read_key_sequence().expect("read key sequence");
    assert_eq!(
        keys,
        vec![Value::fixnum('a' as i64), Value::fixnum('b' as i64)]
    );
    assert_eq!(
        binding,
        Value::symbol("neomacs-test-select-window-deferred-command")
    );

    let deferred = ev
        .read_char()
        .expect("deferred select-window should be unread first");
    assert_eq!(
        deferred,
        Value::list(vec![
            Value::symbol("select-window"),
            Value::list(vec![Value::make_window(w2.0)]),
        ])
    );
}

#[test]
fn read_key_sequence_can_return_select_window_at_sequence_start() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let fid = ev.frames.selected_frame().expect("selected frame").id;
    let w1 = ev.frames.get(fid).expect("frame").window_list()[0];
    let other_buffer = ev.buffers.create_buffer("select-window-target");
    let w2 = ev
        .frames
        .split_window(
            fid,
            w1,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");

    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-test-handle-select-window
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("select-window")],
        Value::symbol("neomacs-test-handle-select-window"),
    )
    .expect("define select-window binding");

    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::SelectWindow { window_id: w2 });

    let (keys, binding) = ev
        .read_key_sequence_with_options(crate::keyboard::ReadKeySequenceOptions::new(
            Value::NIL,
            false,
            false,
            true,
        ))
        .expect("read select-window sequence");

    assert_eq!(
        keys,
        vec![Value::list(vec![
            Value::symbol("select-window"),
            Value::list(vec![Value::make_window(w2.0)]),
        ])]
    );
    assert_eq!(binding, Value::symbol("neomacs-test-handle-select-window"));
}

#[test]
fn read_char_mouse_press_uses_clicked_window_geometry() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let fid = ev.frames.selected_frame().expect("selected frame").id;
    let w1 = ev.frames.get(fid).expect("frame").window_list()[0];
    let other_buffer = ev.buffers.create_buffer("mouse-click-target");
    let w2 = ev
        .frames
        .split_window(
            fid,
            w1,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");
    let _ = ev
        .buffers
        .replace_buffer_contents(other_buffer, &"x".repeat(96));

    let (click_x, click_y) = {
        let frame = ev.frames.get(fid).expect("frame after split");
        let bounds = *frame.find_window(w2).expect("clicked window").bounds();
        (bounds.x + 25.0, bounds.y + 10.0)
    };

    ev.frames
        .get_mut(fid)
        .expect("mutable frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: w2,
            cell_origin: Default::default(),
            regions: Default::default(),
            regions_materialized: true,
            body_rows: Vec::new(),
            text_area_left_offset: 5,
            mode_line_height: 0,
            header_line_height: 0,
            tab_line_height: 0,
            chrome_strings: Vec::new(),
            logical_cursor: None,
            phys_cursor: None,
            buffer_modiff: None,
            layout_freshness: None,
            window_end_record: None,
            points: vec![crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: crate::buffer::LispCharPos1::new(77),
                x: 20,
                y: 0,
                width: 8,
                height: 16,
                row: 0,
                col: 2,
            }],
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 0,
                end_col: 0,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(77)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(77)),
                fringe: Default::default(),
            }],
        }]);

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MousePress {
            button: crate::keyboard::MouseButton::Left,
            x: click_x,
            y: click_y,
            modifiers: crate::keyboard::Modifiers::none(),
            target_frame_id: fid.0,
        },
    );

    let event = ev.read_char().expect("read mouse press");
    let event_slots = crate::emacs_core::value::list_to_vec(&event).expect("event list");
    let position = event_slots[1];
    let position_slots = crate::emacs_core::value::list_to_vec(&position).expect("mouse posn list");

    assert_eq!(event_slots[0], Value::symbol("down-mouse-1"));
    assert_eq!(position_slots[0], Value::make_window(w2.0));
    assert_eq!(position_slots[1], Value::fixnum(77));
    assert_eq!(
        position_slots[2],
        Value::cons(Value::fixnum(20), Value::fixnum(10))
    );
    assert_eq!(position_slots[5], Value::fixnum(77));
    assert_eq!(
        position_slots[6],
        Value::cons(Value::fixnum(2), Value::fixnum(0))
    );
    assert_eq!(
        position_slots[9],
        Value::cons(Value::fixnum(8), Value::fixnum(16))
    );
}

#[test]
fn read_key_sequence_uses_clicked_window_local_map_for_mouse_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let fid = ev.frames.selected_frame().expect("selected frame").id;
    let w1 = ev.frames.get(fid).expect("frame").window_list()[0];
    let other_buffer = ev.buffers.create_buffer("mouse-click-binding");
    let w2 = ev
        .frames
        .split_window(
            fid,
            w1,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");
    let _ = ev
        .buffers
        .replace_buffer_contents(other_buffer, &"x".repeat(96));

    ev.eval_str(
        r#"(fset 'neomacs-mouse-click-target-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    let local_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    ev.buffers
        .set_buffer_local_map(other_buffer, local_map)
        .expect("buffer local map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        local_map,
        &[Value::symbol("mouse-1")],
        Value::symbol("neomacs-mouse-click-target-command"),
    )
    .expect("define mouse binding");

    let (click_x, click_y) = {
        let frame = ev.frames.get(fid).expect("frame after split");
        let bounds = *frame.find_window(w2).expect("clicked window").bounds();
        (bounds.x + 25.0, bounds.y + 10.0)
    };

    ev.frames
        .get_mut(fid)
        .expect("mutable frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: w2,
            cell_origin: Default::default(),
            regions: Default::default(),
            regions_materialized: true,
            body_rows: Vec::new(),
            text_area_left_offset: 5,
            mode_line_height: 0,
            header_line_height: 0,
            tab_line_height: 0,
            chrome_strings: Vec::new(),
            logical_cursor: None,
            phys_cursor: None,
            buffer_modiff: None,
            layout_freshness: None,
            window_end_record: None,
            points: vec![crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: crate::buffer::LispCharPos1::new(77),
                x: 20,
                y: 0,
                width: 8,
                height: 16,
                row: 0,
                col: 2,
            }],
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 0,
                end_col: 0,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(77)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(77)),
                fringe: Default::default(),
            }],
        }]);

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MouseRelease {
            button: crate::keyboard::MouseButton::Left,
            x: click_x,
            y: click_y,
            target_frame_id: fid.0,
        },
    );

    let (keys, binding) = ev.read_key_sequence().expect("read mouse sequence");
    let position = crate::emacs_core::value::list_to_vec(&keys[0]).expect("event list")[1];
    let position_slots = crate::emacs_core::value::list_to_vec(&position).expect("mouse posn list");

    assert_eq!(binding, Value::symbol("neomacs-mouse-click-target-command"));
    assert_eq!(position_slots[0], Value::make_window(w2.0));
    assert_eq!(position_slots[5], Value::fixnum(77));
}

#[test]
fn read_key_sequence_drops_unbound_down_mouse_before_bound_click() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let fid = ev.frames.selected_frame().expect("selected frame").id;
    let w1 = ev.frames.get(fid).expect("frame").window_list()[0];
    let other_buffer = ev.buffers.create_buffer("mouse-click-binding");
    let w2 = ev
        .frames
        .split_window(
            fid,
            w1,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");
    let _ = ev
        .buffers
        .replace_buffer_contents(other_buffer, &"x".repeat(96));

    ev.eval_str(
        r#"(fset 'neomacs-mouse-click-target-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    let local_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    ev.buffers
        .set_buffer_local_map(other_buffer, local_map)
        .expect("buffer local map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        local_map,
        &[Value::symbol("mouse-1")],
        Value::symbol("neomacs-mouse-click-target-command"),
    )
    .expect("define mouse binding");

    let (click_x, click_y) = {
        let frame = ev.frames.get(fid).expect("frame after split");
        let bounds = *frame.find_window(w2).expect("clicked window").bounds();
        (bounds.x + 25.0, bounds.y + 10.0)
    };

    ev.frames
        .get_mut(fid)
        .expect("mutable frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: w2,
            cell_origin: Default::default(),
            regions: Default::default(),
            regions_materialized: true,
            body_rows: Vec::new(),
            text_area_left_offset: 5,
            mode_line_height: 0,
            header_line_height: 0,
            tab_line_height: 0,
            chrome_strings: Vec::new(),
            logical_cursor: None,
            phys_cursor: None,
            buffer_modiff: None,
            layout_freshness: None,
            window_end_record: None,
            points: vec![crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: crate::buffer::LispCharPos1::new(77),
                x: 20,
                y: 0,
                width: 8,
                height: 16,
                row: 0,
                col: 2,
            }],
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 0,
                end_col: 0,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(77)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(77)),
                fringe: Default::default(),
            }],
        }]);

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MousePress {
            button: crate::keyboard::MouseButton::Left,
            x: click_x,
            y: click_y,
            modifiers: crate::keyboard::Modifiers::none(),
            target_frame_id: fid.0,
        },
    );
    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MouseRelease {
            button: crate::keyboard::MouseButton::Left,
            x: click_x,
            y: click_y,
            target_frame_id: fid.0,
        },
    );

    let (keys, binding) = ev.read_key_sequence().expect("read mouse sequence");
    let position = crate::emacs_core::value::list_to_vec(&keys[0]).expect("event list")[1];
    let position_slots = crate::emacs_core::value::list_to_vec(&position).expect("mouse posn list");

    assert_eq!(binding, Value::symbol("neomacs-mouse-click-target-command"));
    assert_eq!(
        keys,
        vec![Value::list(vec![Value::symbol("mouse-1"), position])]
    );
    assert_eq!(position_slots[0], Value::make_window(w2.0));
    assert_eq!(position_slots[5], Value::fixnum(77));
}

#[test]
fn read_key_sequence_drops_unbound_menu_bar_down_mouse_before_bound_click() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let fid = ev.frames.selected_frame().expect("selected frame").id;
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.menu_bar_height = 33;
        frame.char_width = 12.0;
    }

    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-menu-bar-click-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("menu-bar"), Value::symbol("mouse-1")],
        Value::symbol("neomacs-menu-bar-click-command"),
    )
    .expect("define menu-bar mouse binding");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MousePress {
            button: crate::keyboard::MouseButton::Left,
            x: 24.0,
            y: 14.0,
            modifiers: crate::keyboard::Modifiers::none(),
            target_frame_id: fid.0,
        },
    );
    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MouseRelease {
            button: crate::keyboard::MouseButton::Left,
            x: 24.0,
            y: 14.0,
            target_frame_id: fid.0,
        },
    );

    let (keys, binding) = ev
        .read_key_sequence()
        .expect("read menu-bar mouse sequence");
    let position = crate::emacs_core::value::list_to_vec(&keys[1]).expect("event list")[1];
    let position_slots = crate::emacs_core::value::list_to_vec(&position).expect("mouse posn list");

    assert_eq!(binding, Value::symbol("neomacs-menu-bar-click-command"));
    assert_eq!(keys[0], Value::symbol("menu-bar"));
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&keys[1]).expect("event list")[0],
        Value::symbol("mouse-1")
    );
    assert_eq!(position_slots[0], Value::NIL);
    assert_eq!(position_slots[1], Value::symbol("menu-bar"));
    assert_eq!(
        position_slots[2],
        Value::cons(Value::fixnum(2), Value::fixnum(14))
    );
}

#[test]
fn read_key_sequence_dispatches_gui_tool_bar_click_by_item_key() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));

    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    let tool_bar_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.obarray.set_symbol_value("tool-bar-map", tool_bar_map);
    ev.eval_str(
        r#"(fset 'neomacs-tool-bar-click-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");
    crate::emacs_core::keymap::list_keymap_define(
        tool_bar_map,
        Value::symbol("open-file"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Open File"),
            Value::symbol("neomacs-tool-bar-click-command"),
            Value::symbol(":image"),
            Value::string("open.svg"),
        ]),
    );
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("tool-bar"), Value::symbol("open-file")],
        Value::symbol("neomacs-tool-bar-click-command"),
    )
    .expect("define tool-bar binding");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::ToolBarClick {
            index: 0,
            emacs_frame_id: 0,
        },
    );

    let (keys, binding) = ev
        .read_key_sequence()
        .expect("read tool-bar click sequence");
    let event = crate::emacs_core::value::list_to_vec(&keys[1]).expect("event list");
    let position = crate::emacs_core::value::list_to_vec(&event[1]).expect("position list");

    assert_eq!(binding, Value::symbol("neomacs-tool-bar-click-command"));
    assert_eq!(keys[0], Value::symbol("tool-bar"));
    assert_eq!(event[0], Value::symbol("open-file"));
    assert_eq!(position[1], Value::symbol("tool-bar"));
}

#[test]
fn read_key_sequence_dispatches_gui_tool_bar_click_from_owning_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let primary = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let secondary_buffer = ev.buffer_manager_mut().create_buffer("secondary");
    let secondary = ev.frames.create_frame("F2", 960, 640, secondary_buffer);
    ev.frames.select_frame(primary);

    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    let primary_tool_bar_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    let secondary_tool_bar_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.obarray
        .set_symbol_value("tool-bar-map", primary_tool_bar_map);
    ev.buffer_manager_mut()
        .get_mut(secondary_buffer)
        .expect("secondary buffer")
        .set_buffer_local("tool-bar-map", secondary_tool_bar_map);
    ev.eval_str(
        r#"(fset 'neomacs-secondary-tool-bar-click-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");
    crate::emacs_core::keymap::list_keymap_define(
        primary_tool_bar_map,
        Value::symbol("primary-action"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Primary"),
            Value::symbol("ignore"),
            Value::symbol(":image"),
            Value::string("primary.svg"),
        ]),
    );
    crate::emacs_core::keymap::list_keymap_define(
        secondary_tool_bar_map,
        Value::symbol("secondary-action"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Secondary"),
            Value::symbol("neomacs-secondary-tool-bar-click-command"),
            Value::symbol(":image"),
            Value::string("secondary.svg"),
        ]),
    );
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("tool-bar"), Value::symbol("secondary-action")],
        Value::symbol("neomacs-secondary-tool-bar-click-command"),
    )
    .expect("define secondary tool-bar binding");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::ToolBarClick {
            index: 0,
            emacs_frame_id: secondary.0,
        },
    );

    let (keys, binding) = ev
        .read_key_sequence()
        .expect("read secondary tool-bar click sequence");
    let event = crate::emacs_core::value::list_to_vec(&keys[1]).expect("event list");
    let position = crate::emacs_core::value::list_to_vec(&event[1]).expect("position list");

    assert_eq!(
        binding,
        Value::symbol("neomacs-secondary-tool-bar-click-command")
    );
    assert_eq!(event[0], Value::symbol("secondary-action"));
    // GNU parity: for tool-bar (and tab-bar) clicks the posn's first slot is
    // the FRAME, not nil — GNU keyboard.c make_lispy_position: "Kludge alert:
    // for mouse events on the tab bar and tool bar, keyboard.c wants the
    // frame, not the special-purpose window". The old nil expectation dated
    // from when the owning-frame lookup failed in this harness; commit
    // 49bb9c04c made the lookup succeed, exposing the stale assertion.
    assert_eq!(position[0], Value::make_frame(secondary.0));
    assert_eq!(position[1], Value::symbol("tool-bar"));
}

#[test]
fn read_key_sequence_gui_tool_bar_frame_fallback_ignores_current_buffer_local_map() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let primary = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let secondary_buffer = ev.buffer_manager_mut().create_buffer("secondary");
    let secondary = ev.frames.create_frame("F2", 960, 640, secondary_buffer);
    ev.frames.select_frame(primary);

    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    let primary_local_tool_bar_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    let default_tool_bar_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.obarray
        .set_symbol_value("tool-bar-map", default_tool_bar_map);
    ev.buffer_manager_mut()
        .get_mut(crate::buffer::BufferId(1))
        .expect("primary buffer")
        .set_buffer_local("tool-bar-map", primary_local_tool_bar_map);
    ev.eval_str(
        r#"(fset 'neomacs-default-tool-bar-click-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");
    crate::emacs_core::keymap::list_keymap_define(
        primary_local_tool_bar_map,
        Value::symbol("primary-local-action"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Primary Local"),
            Value::symbol("ignore"),
            Value::symbol(":image"),
            Value::string("primary-local.svg"),
        ]),
    );
    crate::emacs_core::keymap::list_keymap_define(
        default_tool_bar_map,
        Value::symbol("default-action"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Default"),
            Value::symbol("neomacs-default-tool-bar-click-command"),
            Value::symbol(":image"),
            Value::string("default.svg"),
        ]),
    );
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("tool-bar"), Value::symbol("default-action")],
        Value::symbol("neomacs-default-tool-bar-click-command"),
    )
    .expect("define default tool-bar binding");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::ToolBarClick {
            index: 0,
            emacs_frame_id: secondary.0,
        },
    );

    let (keys, binding) = ev
        .read_key_sequence()
        .expect("read secondary tool-bar click sequence");
    let event = crate::emacs_core::value::list_to_vec(&keys[1]).expect("event list");

    assert_eq!(
        binding,
        Value::symbol("neomacs-default-tool-bar-click-command")
    );
    assert_eq!(event[0], Value::symbol("default-action"));
}

#[test]
fn read_key_sequence_dispatches_gui_menu_bar_click_with_frame_id() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let primary = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let secondary = ev
        .frames
        .create_frame("F2", 960, 640, crate::buffer::BufferId(1));
    ev.frames.select_frame(primary);

    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);
    ev.eval_str(
        r#"(fset 'neomacs-menu-bar-click-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("menu-bar"), Value::symbol("mouse-1")],
        Value::symbol("neomacs-menu-bar-click-command"),
    )
    .expect("define menu-bar binding");

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MenuBarClick {
            index: 2,
            key: "tools".to_string(),
            menu_x: 11.0,
            menu_y: 0.0,
            anchor_x: 128.0,
            anchor_y: 4.0,
            anchor_width: 64.0,
            anchor_height: 24.0,
            emacs_frame_id: secondary.0,
        },
    );

    let (keys, binding) = ev
        .read_key_sequence()
        .expect("read menu-bar click sequence");
    let event = crate::emacs_core::value::list_to_vec(&keys[1]).expect("event list");
    let position = crate::emacs_core::value::list_to_vec(&event[1]).expect("position list");

    assert_eq!(binding, Value::symbol("neomacs-menu-bar-click-command"));
    assert_eq!(keys[0], Value::symbol("menu-bar"));
    assert_eq!(event[0], Value::symbol("mouse-1"));
    assert_eq!(position[0], Value::NIL);
    assert_eq!(position[1], Value::symbol("menu-bar"));
    assert_eq!(
        position[2],
        Value::cons(Value::fixnum(11), Value::fixnum(0))
    );
    assert_eq!(
        position[8],
        Value::cons(Value::fixnum(128), Value::fixnum(4))
    );
    assert_eq!(
        ev.pending_menu_bar_popup_anchor
            .as_ref()
            .expect("pending anchor")
            .frame_id,
        secondary
    );
    assert_eq!(
        ev.pending_menu_bar_popup_anchor
            .as_ref()
            .expect("pending anchor")
            .menu_key
            .as_deref(),
        Some("tools")
    );
}

#[test]
fn read_key_sequence_drops_unbound_down_mouse_without_losing_keyboard_prefix() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);

    ev.eval_str(
        r#"(fset 'neomacs-prefixed-mouse-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    let prefix_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('a' as i64)],
        prefix_map,
    )
    .expect("define prefix");
    crate::emacs_core::keymap::list_keymap_define_seq(
        prefix_map,
        &[Value::symbol("mouse-1")],
        Value::symbol("neomacs-prefixed-mouse-command"),
    )
    .expect("define mouse binding");

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('a' as i64));
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::symbol("down-mouse-1"));
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::symbol("mouse-1"));

    let (keys, binding) = ev
        .read_key_sequence()
        .expect("read prefixed mouse sequence");

    assert_eq!(binding, Value::symbol("neomacs-prefixed-mouse-command"));
    assert_eq!(
        keys,
        vec![Value::fixnum('a' as i64), Value::symbol("mouse-1")]
    );
}

#[test]
fn read_key_sequence_reduces_unbound_triple_mouse_to_bound_click() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);

    ev.eval_str(
        r#"(fset 'neomacs-triple-mouse-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("mouse-1")],
        Value::symbol("neomacs-triple-mouse-command"),
    )
    .expect("define mouse binding");

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::symbol("triple-mouse-1"));

    let (keys, binding) = ev.read_key_sequence().expect("read triple mouse sequence");

    assert_eq!(binding, Value::symbol("neomacs-triple-mouse-command"));
    assert_eq!(keys, vec![Value::symbol("mouse-1")]);
}

#[test]
fn read_key_sequence_uses_clicked_window_buffer_local_minor_mode_maps() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let original_buffer = ev.buffers.current_buffer_id().expect("current buffer");
    let fid = ev.frames.selected_frame().expect("selected frame").id;
    let w1 = ev.frames.get(fid).expect("frame").window_list()[0];
    let other_buffer = ev.buffers.create_buffer("mouse-minor-mode-binding");
    let w2 = ev
        .frames
        .split_window(
            fid,
            w1,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");
    let _ = ev
        .buffers
        .replace_buffer_contents(other_buffer, &"x".repeat(96));

    ev.eval_str(
        r#"(fset 'neomacs-mouse-minor-mode-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    ev.obarray
        .set_symbol_value("neomacs-click-minor-mode", Value::NIL);
    ev.obarray
        .make_buffer_local("neomacs-click-minor-mode", true);
    ev.buffers
        .set_buffer_local_property(other_buffer, "neomacs-click-minor-mode", Value::T)
        .expect("buffer-local minor mode");

    let minor_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define_seq(
        minor_map,
        &[Value::symbol("mouse-1")],
        Value::symbol("neomacs-mouse-minor-mode-command"),
    )
    .expect("define minor mode binding");
    ev.assign(
        "minor-mode-map-alist",
        Value::list(vec![Value::cons(
            Value::symbol("neomacs-click-minor-mode"),
            minor_map,
        )]),
    );

    let (click_x, click_y) = {
        let frame = ev.frames.get(fid).expect("frame after split");
        let bounds = *frame.find_window(w2).expect("clicked window").bounds();
        (bounds.x + 25.0, bounds.y + 10.0)
    };

    ev.frames
        .get_mut(fid)
        .expect("mutable frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: w2,
            cell_origin: Default::default(),
            regions: Default::default(),
            regions_materialized: true,
            body_rows: Vec::new(),
            text_area_left_offset: 5,
            mode_line_height: 0,
            header_line_height: 0,
            tab_line_height: 0,
            chrome_strings: Vec::new(),
            logical_cursor: None,
            phys_cursor: None,
            buffer_modiff: None,
            layout_freshness: None,
            window_end_record: None,
            points: vec![crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: crate::buffer::LispCharPos1::new(77),
                x: 20,
                y: 0,
                width: 8,
                height: 16,
                row: 0,
                col: 2,
            }],
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 0,
                end_col: 0,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(77)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(77)),
                fringe: Default::default(),
            }],
        }]);

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MouseRelease {
            button: crate::keyboard::MouseButton::Left,
            x: click_x,
            y: click_y,
            target_frame_id: fid.0,
        },
    );

    let (keys, binding) = ev.read_key_sequence().expect("read mouse sequence");
    let position = crate::emacs_core::value::list_to_vec(&keys[0]).expect("event list")[1];
    let position_slots = crate::emacs_core::value::list_to_vec(&position).expect("mouse posn list");

    assert_eq!(binding, Value::symbol("neomacs-mouse-minor-mode-command"));
    assert_eq!(position_slots[0], Value::make_window(w2.0));
    assert_eq!(ev.buffers.current_buffer_id(), Some(original_buffer));
}

#[test]
fn read_key_sequence_prefixes_mode_line_mouse_click_for_lookup() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let fid = ev.frames.selected_frame().expect("selected frame").id;
    let w1 = ev.frames.get(fid).expect("frame").window_list()[0];
    let other_buffer = ev.buffers.create_buffer("mouse-mode-line-binding");
    let w2 = ev
        .frames
        .split_window(
            fid,
            w1,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");

    ev.eval_str(
        r#"(fset 'neomacs-mode-line-click-command
                  (lambda () (interactive) 'ok))"#,
    )
    .expect("setup");

    let local_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    ev.buffers
        .set_buffer_local_map(other_buffer, local_map)
        .expect("buffer local map");
    crate::emacs_core::keymap::list_keymap_define_seq(
        local_map,
        &[Value::symbol("mode-line"), Value::symbol("mouse-1")],
        Value::symbol("neomacs-mode-line-click-command"),
    )
    .expect("define mode-line mouse binding");

    let (click_x, click_y) = {
        let frame = ev.frames.get(fid).expect("frame after split");
        let bounds = *frame.find_window(w2).expect("clicked window").bounds();
        (bounds.x + 25.0, bounds.bottom() - 4.0)
    };

    ev.frames
        .get_mut(fid)
        .expect("mutable frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: w2,
            cell_origin: Default::default(),
            regions: Default::default(),
            regions_materialized: true,
            body_rows: Vec::new(),
            text_area_left_offset: 0,
            mode_line_height: 18,
            header_line_height: 0,
            tab_line_height: 0,
            chrome_strings: Vec::new(),
            logical_cursor: None,
            phys_cursor: None,
            buffer_modiff: None,
            layout_freshness: None,
            window_end_record: None,
            points: Vec::new(),
            rows: Vec::new(),
        }]);

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MouseRelease {
            button: crate::keyboard::MouseButton::Left,
            x: click_x,
            y: click_y,
            target_frame_id: fid.0,
        },
    );

    let (keys, binding) = ev.read_key_sequence().expect("read mode-line click");
    let position = crate::emacs_core::value::list_to_vec(&keys[1]).expect("event list")[1];
    let position_slots = crate::emacs_core::value::list_to_vec(&position).expect("mouse posn list");

    assert_eq!(binding, Value::symbol("neomacs-mode-line-click-command"));
    assert_eq!(keys[0], Value::symbol("mode-line"));
    assert_eq!(position_slots[0], Value::make_window(w2.0));
    assert_eq!(position_slots[1], Value::symbol("mode-line"));
}

#[test]
fn clear_current_message_runs_echo_area_clear_hook_once_when_message_present() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"
        (setq echo-clear-count 0)
        (setq echo-area-clear-hook
              (list (lambda ()
                      (setq echo-clear-count (1+ echo-clear-count)))))
        "#,
    )
    .expect("install echo-area-clear-hook");
    ev.set_current_message(Some(crate::heap_types::LispString::from_utf8("hello")));
    ev.clear_current_message();
    assert_eq!(ev.current_message_text(), None);

    assert_eq!(
        ev.eval_str("echo-clear-count").expect("echo-clear-count"),
        Value::fixnum(1)
    );

    ev.clear_current_message();
    assert_eq!(
        ev.eval_str("echo-clear-count").expect("echo-clear-count"),
        Value::fixnum(1)
    );
}

#[test]
fn update_active_region_selection_after_command_calls_gnu_owned_selection_surface() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str(
        r#"
(setq selection-capture nil
      post-select-capture nil)
(fset 'display-selections-p (lambda (&optional _display) t))
(fset 'region-active-p (lambda () t))
(fset 'gui-set-selection
      (lambda (type data)
        (setq selection-capture (list type data))
        nil))
(setq region-extract-function (lambda (_raw) "bcd")
      transient-mark-mode t
      mark-active t
      deactivate-mark nil
      select-active-regions t
      selection-inhibit-update-commands nil
      this-command 'region-test
      post-select-region-hook
      (list (lambda (text)
              (setq post-select-capture text))))
"#,
    )
    .expect("eval forms");

    ev.update_active_region_selection_after_command()
        .expect("update active region selection");

    let result = ev
        .eval_str("(list selection-capture post-select-capture saved-region-selection)")
        .expect("selection result");
    assert_eq!(
        format!("{}", result),
        "((PRIMARY \"bcd\") \"bcd\" nil)",
        "active-region update should set PRIMARY and run post-select-region-hook"
    );
}

#[test]
fn redisplay_preserves_non_resize_input_for_read_char() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    assert_eq!(ev.frames.selected_frame().map(|frame| frame.id), Some(fid));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .unwrap();

    ev.redisplay();

    let event = ev
        .read_char()
        .expect("read_char should return queued keypress");
    assert_eq!(event, Value::fixnum('a' as i64));
}

#[test]
fn fire_pending_timers_executes_lisp_callbacks() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("vm-timer-fired", Value::NIL);
    ev.eval_str(
        r#"(progn
           (fset 'vm-test-timer-callback
                 (lambda () (setq vm-timer-fired 'done)))
           (fset 'timer-event-handler
                 (lambda (timer)
                   (setq timer-list nil)
                   (funcall (aref timer 5)))))"#,
    )
    .expect("install timer handlers");

    let timer = Value::vector(vec![
        Value::NIL,
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::NIL,
        Value::symbol("vm-test-timer-callback"),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ]);
    ev.set_variable("timer-list", Value::list(vec![timer]));

    ev.fire_pending_timers();

    assert_eq!(
        ev.eval_symbol("vm-timer-fired")
            .expect("timer flag should be bound"),
        Value::symbol("done")
    );
}

#[test]
fn fire_pending_timers_requests_redisplay_after_callbacks() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("vm-timer-fired", Value::NIL);

    let redisplay_calls = Rc::new(RefCell::new(Vec::new()));
    let redisplay_calls_in_cb = redisplay_calls.clone();
    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        redisplay_calls_in_cb.borrow_mut().push(
            ev.eval_symbol("vm-timer-fired")
                .expect("timer flag during redisplay"),
        );
    }));

    ev.eval_str(
        r#"(progn
           (fset 'vm-test-timer-callback
                 (lambda () (setq vm-timer-fired 'done)))
           (fset 'timer-event-handler
                 (lambda (timer)
                   (setq timer-list nil)
                   (funcall (aref timer 5)))))"#,
    )
    .expect("install timer handlers");

    let timer = Value::vector(vec![
        Value::NIL,
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::NIL,
        Value::symbol("vm-test-timer-callback"),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ]);
    ev.set_variable("timer-list", Value::list(vec![timer]));

    ev.fire_pending_timers();

    assert_eq!(*redisplay_calls.borrow(), vec![Value::symbol("done")]);
}

#[test]
fn fire_pending_timers_prefers_more_overdue_ordinary_timer_over_idle_timer() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
           (setq vm-timer-order nil)
           (fset 'vm-ordinary-callback
                 (lambda ()
                   (setq vm-timer-order (append vm-timer-order '(ordinary)))))
           (fset 'vm-idle-callback
                 (lambda ()
                   (setq vm-timer-order (append vm-timer-order '(idle)))))
           (fset 'timer-event-handler
                 (lambda (timer)
                   (if (aref timer 7)
                       (setq timer-idle-list (delq timer timer-idle-list))
                     (setq timer-list (delq timer timer-list)))
                   (funcall (aref timer 5)))))"#,
    )
    .expect("install timer ordering setup");

    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(20),
            "vm-ordinary-callback",
        )]),
    );
    ev.set_variable(
        "timer-idle-list",
        Value::list(vec![gnu_idle_timer_after(
            Duration::from_millis(0),
            "vm-idle-callback",
        )]),
    );
    ev.timer_start_idle();
    thread::sleep(Duration::from_millis(5));

    ev.fire_pending_timers();

    assert_eq!(
        ev.eval_symbol("vm-timer-order")
            .expect("timer order should be recorded"),
        Value::list(vec![Value::symbol("ordinary"), Value::symbol("idle")])
    );
}

#[test]
fn fire_pending_timers_prefers_more_overdue_idle_timer_over_ordinary_timer() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
           (setq vm-timer-order nil)
           (fset 'vm-ordinary-callback
                 (lambda ()
                   (setq vm-timer-order (append vm-timer-order '(ordinary)))))
           (fset 'vm-idle-callback
                 (lambda ()
                   (setq vm-timer-order (append vm-timer-order '(idle)))))
           (fset 'timer-event-handler
                 (lambda (timer)
                   (if (aref timer 7)
                       (setq timer-idle-list (delq timer timer-idle-list))
                     (setq timer-list (delq timer timer-list)))
                   (funcall (aref timer 5)))))"#,
    )
    .expect("install timer ordering setup");

    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_after(
            Duration::from_millis(5),
            "vm-ordinary-callback",
        )]),
    );
    ev.set_variable(
        "timer-idle-list",
        Value::list(vec![gnu_idle_timer_after(
            Duration::from_millis(0),
            "vm-idle-callback",
        )]),
    );
    ev.timer_start_idle();
    thread::sleep(Duration::from_millis(20));

    ev.fire_pending_timers();

    assert_eq!(
        ev.eval_symbol("vm-timer-order")
            .expect("timer order should be recorded"),
        Value::list(vec![Value::symbol("idle"), Value::symbol("ordinary")])
    );
}

#[test]
fn next_input_wait_timeout_accounts_for_gnu_timer_list() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_after(Duration::from_millis(200), "ignore")]),
    );

    let timeout = ev
        .next_input_wait_timeout()
        .expect("gnu timer should bound read_char wait");

    assert!(timeout > Duration::ZERO);
    assert!(timeout <= Duration::from_millis(200));
}

#[test]
fn next_input_wait_timeout_chooses_earliest_timer_source() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable(
        "timer-list",
        Value::list(vec![
            gnu_timer_after(Duration::from_millis(50), "ignore"),
            gnu_timer_after(Duration::from_millis(250), "ignore"),
        ]),
    );

    let timeout = ev
        .next_input_wait_timeout()
        .expect("timers should bound read_char wait");

    assert!(timeout <= Duration::from_millis(100));
}

#[test]
fn next_input_wait_timeout_accounts_for_gnu_idle_timer_list_when_idle() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable(
        "timer-idle-list",
        Value::list(vec![gnu_idle_timer_after(
            Duration::from_millis(200),
            "ignore-idle",
        )]),
    );
    ev.timer_start_idle();

    let timeout = ev
        .next_input_wait_timeout()
        .expect("gnu idle timer should bound read_char wait");

    assert!(timeout > Duration::ZERO);
    assert!(timeout <= Duration::from_millis(200));
}

#[test]
fn read_char_fires_bootstrapped_gnu_run_with_timer_while_waiting_for_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();

    ev.eval_str(
        r#"(progn
           (setq vm-timer-fired nil)
           (run-with-timer
            0.01 nil
            (lambda () (setq vm-timer-fired 'done))))"#,
    )
    .expect("schedule GNU Lisp timer");

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    // Keep one sender alive: dropping the last tx disconnects the channel,
    // which the input machinery treats as terminal-gone -> quit (timing flake;
    // see the sit-for soak fix).
    let _tx_keepalive = tx.clone();
    thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('a'),
        ))
        .expect("send keypress");
    });

    let event = ev
        .read_char()
        .expect("read_char should return queued keypress");
    assert_eq!(event, Value::fixnum('a' as i64));
    assert_eq!(
        ev.eval_symbol("vm-timer-fired")
            .expect("timer flag should be bound"),
        Value::symbol("done")
    );
}

#[test]
fn read_char_fires_bootstrapped_gnu_run_with_idle_timer_while_waiting_for_input() {
    crate::test_utils::init_test_tracing();
    eprintln!("idle test: bootstrap");
    let mut ev = runtime_startup_context();

    eprintln!("idle test: parse forms");
    eprintln!("idle test: eval schedule");
    ev.eval_str(
        r#"(progn
           (setq vm-idle-fired nil)
           (setq vm-idle-snapshot nil)
           (run-with-idle-timer
            0.01 nil
            (lambda ()
              (setq vm-idle-fired 'done)
              (setq vm-idle-snapshot (current-idle-time)))))"#,
    )
    .expect("schedule GNU Lisp idle timer");

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    // Keep one sender alive: dropping the last tx disconnects the channel,
    // which the input machinery treats as terminal-gone -> quit (timing flake;
    // see the sit-for soak fix).
    let _tx_keepalive = tx.clone();
    eprintln!("idle test: spawn sender");
    thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('a'),
        ))
        .expect("send keypress");
    });

    eprintln!("idle test: read_char");
    let event = ev
        .read_char()
        .expect("read_char should return queued keypress");
    eprintln!("idle test: read_char returned {:?}", event);
    assert_eq!(event, Value::fixnum('a' as i64));
    assert_eq!(
        ev.eval_symbol("vm-idle-fired")
            .expect("idle timer flag should be bound"),
        Value::symbol("done")
    );
    let idle_snapshot = ev
        .eval_symbol("vm-idle-snapshot")
        .expect("idle snapshot should be bound");
    let idle_parts = list_to_vec(&idle_snapshot).expect("idle snapshot should be a time list");
    assert_eq!(idle_parts.len(), 4);
    assert!(idle_parts[0].as_int().is_some());
    assert!(idle_parts[1].as_int().is_some());
    assert!(idle_parts[2].as_int().is_some());
    assert_eq!(ev.current_idle_time_value(), Value::NIL);
}

/// A repeating idle timer runs once per idle epoch.  Reading real input ends
/// the current epoch; the following input wait must begin a fresh epoch and
/// make the timer eligible again.  Packages such as Super Save depend on this
/// lifecycle: their timer commonly fires once before a file is edited, then
/// must fire again after the editing command when Emacs becomes idle.
#[test]
fn repeating_idle_timer_rearms_after_user_input_starts_new_idle_epoch() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();

    // Retire the dump's first GC cycle (promotion + blackening) before the
    // timed part: the bootstrap used to run synchronously inside setup eval,
    // but the concurrent first cycle terminates at a LATER safe point — in a
    // debug build that ~80ms termination can land inside one of this test's
    // 80ms idle windows and eat the timer's epoch. This test is about idle
    // epochs, not GC pause placement.
    ev.eval_str("(garbage-collect)")
        .expect("retire bootstrap GC");
    ev.eval_str(
        r#"(progn
           (setq vm-repeating-idle-count 0)
           (run-with-idle-timer
            0.01 t
            (lambda ()
              (setq vm-repeating-idle-count
                    (1+ vm-repeating-idle-count)))))"#,
    )
    .expect("schedule repeating GNU Lisp idle timer");

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let _tx_keepalive = tx.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(80));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('a'),
        ))
        .expect("send first keypress");
        thread::sleep(Duration::from_millis(80));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('b'),
        ))
        .expect("send second keypress");
    });

    assert_eq!(
        ev.read_char().expect("read first keypress"),
        Value::fixnum('a' as i64)
    );
    assert_eq!(
        ev.read_char().expect("read second keypress"),
        Value::fixnum('b' as i64)
    );
    assert_eq!(
        ev.eval_symbol("vm-repeating-idle-count")
            .expect("idle timer count should be bound"),
        Value::fixnum(2),
        "repeating idle timer must run once in each idle epoch"
    );
}

/// GNU `read_key_sequence_vs` clears the committed `this-command-keys` at
/// entry when CONTINUE-ECHO is nil (keyboard.c:11919-11923) so a fresh key
/// sequence starts from an empty `(this-command-keys-vector)`. `read-key`
/// (subr.el) depends on this: it reads a sequence with CONTINUE-ECHO nil and
/// arms an idle timer that throws the moment `(this-command-keys-vector)` is
/// non-empty (subr.el:3648-3665). If a command's PREVIOUS, invoking sequence
/// were still committed when the nested read begins, the probe would fire
/// immediately and return the wrong key.
///
/// This drives that exact path: it pre-seeds a STALE committed
/// `this-command-keys` (as if a `C-x r s` invocation had just been read),
/// arms an idle timer that snapshots `(this-command-keys-vector)` while the
/// nested `read_key_sequence` (continue-echo = nil) is waiting, and delivers
/// the real key only after a delay so the idle timer fires first. The
/// snapshot MUST be empty — proving the stale invoking sequence was cleared at
/// entry — and the read must still return the freshly delivered key.
#[test]
fn read_key_sequence_clears_stale_this_command_keys_at_entry_for_idle_probe() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let scratch = ev.buffers.create_buffer("*rks-entry-clear*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    assert!(ev.frames.select_frame(frame), "need a selected frame");

    // Pre-seed a stale, non-empty committed key sequence, exactly as the
    // command loop leaves it after reading the command's invoking keys
    // (e.g. `C-x r s`). Use plain character codes so the vector is concrete.
    ev.set_read_command_keys(vec![
        Value::fixnum('x' as i64),
        Value::fixnum('r' as i64),
        Value::fixnum('s' as i64),
    ]);
    assert_eq!(
        ev.read_command_keys().len(),
        3,
        "precondition: a stale invoking sequence is committed"
    );

    // Arm a `read-key`-style idle timer that snapshots
    // `(this-command-keys-vector)` while the nested read is waiting.
    ev.eval_str(
        r#"(progn
             (setq rks-idle-snapshot 'unset)
             (run-with-idle-timer
              0.01 nil
              (lambda ()
                (setq rks-idle-snapshot (this-command-keys-vector)))))"#,
    )
    .expect("arm idle snapshot timer");

    // Deliver the real key only after a delay so the idle timer fires first.
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let _tx_keepalive = tx.clone();
    thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(120));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('a'),
        ))
        .expect("send keypress");
    });

    let (keys, _binding) = ev
        .read_key_sequence_with_options(crate::keyboard::ReadKeySequenceOptions::new(
            Value::NIL,
            false, // continue-echo = nil -> clear stale this-command-keys
            false,
            false,
        ))
        .expect("nested read should return the freshly delivered key");

    assert_eq!(
        keys,
        vec![Value::fixnum('a' as i64)],
        "read must return the freshly delivered key, not the stale invoking one"
    );

    let snapshot = ev
        .eval_symbol("rks-idle-snapshot")
        .expect("idle snapshot bound");
    assert!(
        snapshot.is_vector(),
        "idle timer must have fired and captured a vector (got {snapshot:?})"
    );
    assert_eq!(
        crate::emacs_core::print::print_value_with_buffers(&snapshot, &ev.buffers),
        "[]",
        "while the nested read (continue-echo=nil) is waiting, \
         (this-command-keys-vector) must be EMPTY — the stale invoking \
         sequence was cleared at entry (GNU keyboard.c:11919-11923)"
    );
}

#[test]
fn callable_print_targets_stream_gnu_char_callbacks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-print-calls nil)
                 (fset 'vm-print-target
                       (lambda (ch)
                         (setq vm-print-calls (cons ch vm-print-calls))))
                 (list
                  (progn
                    (setq vm-print-calls nil)
                    (princ "ab" 'vm-print-target)
                    vm-print-calls)
                  (progn
                    (setq vm-print-calls nil)
                    (prin1 '(1 . 2) 'vm-print-target)
                    vm-print-calls)
                  (progn
                    (setq vm-print-calls nil)
                    (print 'foo 'vm-print-target)
                    vm-print-calls)))"#
        ),
        "OK ((98 97) (41 50 32 46 32 49 40) (10 111 111 102 10))"
    );
}

#[test]
fn marker_print_targets_insert_and_restore_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((orig (current-buffer))
                      (obuf (get-buffer-create "*vm-marker-print*")))
                 (save-current-buffer (set-buffer obuf)
                   (erase-buffer)
                   (insert "xy")
                   (goto-char 2))
                 (let ((m (save-current-buffer (set-buffer obuf) (point-marker))))
                   (list
                    (progn
                      (princ "ab" m)
                      (save-current-buffer (set-buffer obuf)
                        (list (buffer-string) (point) (marker-position m))))
                    (progn
                      (write-char 67 m)
                      (save-current-buffer (set-buffer obuf)
                        (list (buffer-string) (point) (marker-position m))))
                    (progn
                      (terpri m)
                      (save-current-buffer (set-buffer obuf)
                        (list (buffer-string) (point) (marker-position m))))
                    (eq (current-buffer) orig)
                    (point))))"#
        ),
        "OK ((\"xaby\" 4 4) (\"xabCy\" 5 5) (\"xabC\ny\" 6 6) t 1)"
    );
}

#[test]
fn basic_arithmetic() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(+ 1 2)"), "OK 3");
    assert_eq!(eval_one("(- 10 3)"), "OK 7");
    assert_eq!(eval_one("(* 4 5)"), "OK 20");
    assert_eq!(eval_one("(/ 10 3)"), "OK 3");
    assert_eq!(eval_one("(% 10 3)"), "OK 1");
    assert_eq!(eval_one("(1+ 5)"), "OK 6");
    assert_eq!(eval_one("(1- 5)"), "OK 4");
}

/// GNU `Fplus` returns its checked single non-marker operand directly
/// (`src/data.c:Fplus`), while markers are coerced to their live
/// position. Heap-number identity is observable with `eq`.
#[test]
fn unary_plus_matches_gnu_identity_and_marker_coercion() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            "(let ((b (1+ most-positive-fixnum)) (f 1.0)) \
               (list (eq (+ b) b) (eq (+ f) f) (+ (point-marker))))"
        ),
        "OK (t t 1)"
    );
}

/// Regression for audit §1.1 / §2.1-§2.2: arithmetic must promote to
/// bignum on overflow instead of silently wrapping. Mirrors GNU
/// `arith_driver` (`src/data.c:3215`) which uses `ckd_add` /
/// `ckd_mul` etc. to detect overflow and falls through to
/// `bignum_arith_driver`.
///
/// `most-positive-fixnum` is 2^61 - 1 = 2305843009213693951.
/// Adding 1 must yield 2305843009213693952 (== 2^61) as a bignum.
#[test]
fn arithmetic_promotes_to_bignum_on_overflow() {
    crate::test_utils::init_test_tracing();
    // bignump / fixnump come from subr.el — and mixing bare eval_one
    // with bootstrap_eval_one in the same #[test] pollutes the global
    // interner before the dump load asserts slot-by-slot agreement.
    // Use one bootstrap context for everything; repeatedly constructing
    // cached bootstrap evaluators makes this small regression test slow
    // enough to hit nextest's full-suite watchdog under high contention.
    let mut eval = runtime_startup_context();
    let mut bootstrap_eval = |src: &str| {
        let result = eval.eval_str(src);
        format_eval_result(&result)
    };
    //
    // (+ most-positive-fixnum 1) — used to wrap to most-negative-fixnum.
    assert_eq!(
        bootstrap_eval("(+ most-positive-fixnum 1)"),
        "OK 2305843009213693952"
    );
    // 1+ on the same value.
    assert_eq!(
        bootstrap_eval("(1+ most-positive-fixnum)"),
        "OK 2305843009213693952"
    );
    // (* most-positive-fixnum 2) — used to wrap.
    assert_eq!(
        bootstrap_eval("(* most-positive-fixnum 2)"),
        "OK 4611686018427387902"
    );
    // (- most-negative-fixnum 1).
    assert_eq!(
        bootstrap_eval("(- most-negative-fixnum 1)"),
        "OK -2305843009213693953"
    );
    // 1- on most-negative-fixnum.
    assert_eq!(
        bootstrap_eval("(1- most-negative-fixnum)"),
        "OK -2305843009213693953"
    );
    // Unary negate of most-negative-fixnum: -MIN_FIXNUM > MAX_FIXNUM.
    assert_eq!(
        bootstrap_eval("(- most-negative-fixnum)"),
        "OK 2305843009213693952"
    );
    // Round-trip: a bignum in + with a fixnum stays a bignum.
    assert_eq!(
        bootstrap_eval("(+ (1+ most-positive-fixnum) 1)"),
        "OK 2305843009213693953"
    );
    // bignump / integerp / fixnump on the result.
    assert_eq!(
        bootstrap_eval("(bignump (1+ most-positive-fixnum))"),
        "OK t"
    );
    assert_eq!(
        bootstrap_eval("(integerp (1+ most-positive-fixnum))"),
        "OK t"
    );
    assert_eq!(
        bootstrap_eval("(fixnump (1+ most-positive-fixnum))"),
        "OK nil"
    );
}

/// Regression for audit §2.4: `/` must not signal `overflow-error` on
/// `(/ most-negative-fixnum -1)` — that's a valid bignum result.
/// Mirrors GNU `Fquo` (`src/data.c:3315`) which dispatches through
/// `arith_driver` and `bignum_arith_driver` for the overflow case.
#[test]
fn division_promotes_to_bignum_on_min_div_neg_one() {
    crate::test_utils::init_test_tracing();
    // most-negative-fixnum = -2305843009213693952
    // -most-negative-fixnum = 2305843009213693952 = 1 + most-positive-fixnum
    assert_eq!(
        eval_one("(/ most-negative-fixnum -1)"),
        "OK 2305843009213693952"
    );
    // % and mod on this case (both give 0).
    assert_eq!(eval_one("(% most-negative-fixnum -1)"), "OK 0");
    assert_eq!(eval_one("(mod most-negative-fixnum -1)"), "OK 0");
    // / on a bignum dividend.
    assert_eq!(
        eval_one("(/ (* most-positive-fixnum 4) 2)"),
        "OK 4611686018427387902"
    );
    // % on a bignum dividend: 9223372036854775804 % 7 = 4.
    assert_eq!(eval_one("(% (* most-positive-fixnum 4) 7)"), "OK 4");
    // mod with a negative divisor on a bignum dividend:
    // r = 4, sign mismatch with -7 → r + (-7) = -3.
    assert_eq!(eval_one("(mod (* most-positive-fixnum 4) -7)"), "OK -3");
    // Division by zero still signals.
    assert_eq!(
        eval_one("(condition-case e (/ 1 0) (arith-error 'caught))"),
        "OK caught"
    );
    assert_eq!(
        eval_one("(condition-case e (% 1 0) (arith-error 'caught))"),
        "OK caught"
    );
    assert_eq!(
        eval_one("(condition-case e (mod 1 0) (arith-error 'caught))"),
        "OK caught"
    );
}

/// Regression for audit §2.7: bitwise ops must promote on overflow.
/// The headline case is `(ash 1 100)` — used to return 0 because
/// `1 << 100` is a no-op on i64. Mirrors GNU `Fash`
/// (`src/data.c:3519`) which delegates the slow path to `mpz_mul_2exp`.
#[test]
fn bitwise_promotes_to_bignum() {
    crate::test_utils::init_test_tracing();
    // (ash 1 100) — must be 2^100, not 0.
    assert_eq!(
        eval_one("(ash 1 100)"),
        "OK 1267650600228229401496703205376"
    );
    // (ash 1 62) — exceeds fixnum range (2^61 max), must be a bignum.
    assert_eq!(eval_one("(ash 1 62)"), "OK 4611686018427387904");
    // (ash 1 60) — fits in fixnum.
    assert_eq!(eval_one("(ash 1 60)"), "OK 1152921504606846976");
    // Right shift back from a bignum.
    assert_eq!(eval_one("(ash (ash 1 100) -100)"), "OK 1");
    // Right shift toward -infinity for negative bignum.
    assert_eq!(eval_one("(ash -1 -1)"), "OK -1");
    // logand/logior/logxor with bignum operands.
    assert_eq!(
        eval_one("(logand (ash 1 100) (ash 1 100))"),
        "OK 1267650600228229401496703205376"
    );
    assert_eq!(
        eval_one("(logior (ash 1 100) 1)"),
        "OK 1267650600228229401496703205377"
    );
    assert_eq!(eval_one("(logxor (ash 1 100) (ash 1 100))"), "OK 0");
    // lognot of fixnum and bignum.
    assert_eq!(eval_one("(lognot 0)"), "OK -1");
    assert_eq!(
        eval_one("(lognot (ash 1 100))"),
        "OK -1267650600228229401496703205377"
    );
}

/// Regression for audit §1.1, §2.6, §2.15-2.17. (expt 2 100), (abs
/// most-negative-fixnum), and floor/ceiling/round/truncate on
/// out-of-range floats must produce bignums or signal overflow-error
/// (for inf/NaN), not silently wrap or saturate to i64.
#[test]
fn expt_abs_and_rounding_promote_to_bignum() {
    crate::test_utils::init_test_tracing();
    // (expt 2 100) — used to wrap to 0.
    assert_eq!(
        eval_one("(expt 2 100)"),
        "OK 1267650600228229401496703205376"
    );
    // (expt 2 62) — exceeds fixnum but fits in i64.
    assert_eq!(eval_one("(expt 2 62)"), "OK 4611686018427387904");
    // Special cases that never overflow.
    assert_eq!(eval_one("(expt 1 1000000)"), "OK 1");
    assert_eq!(eval_one("(expt -1 1000000)"), "OK 1");
    assert_eq!(eval_one("(expt -1 1000001)"), "OK -1");
    assert_eq!(eval_one("(expt 0 5)"), "OK 0");
    assert_eq!(eval_one("(expt 0 0)"), "OK 1");
    // Negative exponent → float.
    assert_eq!(eval_one("(expt 2 -2)"), "OK 0.25");
    assert_eq!(eval_one("(float (expt 20 20))"), "OK 1.048576e+26");

    // (abs most-negative-fixnum) — used to signal overflow-error.
    assert_eq!(
        eval_one("(abs most-negative-fixnum)"),
        "OK 2305843009213693952"
    );
    // abs of a bignum.
    assert_eq!(
        eval_one("(abs (- (ash 1 100)))"),
        "OK 1267650600228229401496703205376"
    );

    // Float rounding on a value far outside i64.
    // 1e20 is about 2^66, outside fixnum range.
    assert_eq!(eval_one("(truncate 1e20)"), "OK 100000000000000000000");
    assert_eq!(eval_one("(floor 1e20)"), "OK 100000000000000000000");
    // Inf and NaN must signal overflow-error, not saturate.
    assert_eq!(
        eval_one("(condition-case e (truncate 1.0e+INF) (overflow-error 'caught))"),
        "OK caught"
    );
    assert_eq!(
        eval_one("(condition-case e (floor 0.0e+NaN) (overflow-error 'caught))"),
        "OK caught"
    );
}

/// Regression for audit §1.1 (comparisons sub-issue): numeric
/// comparisons must use exact arithmetic, not f64 coercion. Mirrors
/// GNU `arithcompare` (`src/data.c:2682`). Two distinct integers
/// outside ±2^53 (the f64 mantissa limit) used to compare equal under
/// f64 coercion.
#[test]
fn comparisons_are_exact_for_bignums() {
    crate::test_utils::init_test_tracing();
    // 2^60 + 1 vs 2^60 — under f64 coercion both round to the same
    // double; they must compare unequal as integers.
    assert_eq!(eval_one("(= (1+ (ash 1 60)) (ash 1 60))"), "OK nil");
    assert_eq!(eval_one("(< (ash 1 60) (1+ (ash 1 60)))"), "OK t");
    // Bignum vs bignum.
    assert_eq!(eval_one("(< (ash 1 100) (ash 1 101))"), "OK t");
    assert_eq!(eval_one("(> (ash 1 101) (ash 1 100))"), "OK t");
    assert_eq!(eval_one("(= (ash 1 100) (ash 1 100))"), "OK t");
    assert_eq!(eval_one("(/= (ash 1 100) (ash 1 101))"), "OK t");
    // Bignum vs fixnum.
    assert_eq!(eval_one("(< 1 (ash 1 100))"), "OK t");
    assert_eq!(eval_one("(> (ash 1 100) most-positive-fixnum)"), "OK t");
    assert_eq!(eval_one("(<= most-positive-fixnum (ash 1 100))"), "OK t");
    // Bignum vs float — exact even for bignums outside f64 range.
    assert_eq!(eval_one("(< 1.5 (ash 1 100))"), "OK t");
    assert_eq!(eval_one("(> (ash 1 100) 1e30)"), "OK t");
    assert_eq!(
        eval_one(
            "(list (= 0.0e+NaN 0.0e+NaN) (/= 0.0e+NaN 0.0e+NaN) (< 0.0e+NaN 1) (<= 0.0e+NaN 1) (> 0.0e+NaN 1) (>= 0.0e+NaN 1))"
        ),
        "OK (nil t nil nil nil nil)"
    );
    // Chained.
    assert_eq!(eval_one("(< 1 (ash 1 60) (ash 1 100) (ash 1 200))"), "OK t");
}

/// Regression for audit §1.1 (reader sub-issue): integer literals
/// outside fixnum range must read back as bignums, not silently
/// overflow to a wrapped fixnum or signal a parse error. Mirrors
/// GNU `string_to_number` (`src/lread.c`).
#[test]
fn reader_recognizes_bignum_literals() {
    crate::test_utils::init_test_tracing();
    // bignump comes from subr.el; mixing bare and bootstrap contexts
    // in one #[test] pollutes the global interner across the dump
    // load barrier, so bootstrap everything. Reuse the bootstrap context
    // for the same reason as `arithmetic_promotes_to_bignum_on_overflow`.
    let mut eval = runtime_startup_context();
    let mut bootstrap_eval = |src: &str| {
        let result = eval.eval_str(src);
        format_eval_result(&result)
    };
    //
    // Just over the fixnum boundary (2^61).
    assert_eq!(
        bootstrap_eval("4611686018427387904"),
        "OK 4611686018427387904"
    );
    assert_eq!(
        bootstrap_eval("-4611686018427387905"),
        "OK -4611686018427387905"
    );
    // Larger than i64 — has to come back as a bignum.
    assert_eq!(
        bootstrap_eval("12345678901234567890"),
        "OK 12345678901234567890"
    );
    assert_eq!(
        bootstrap_eval("-12345678901234567890"),
        "OK -12345678901234567890"
    );
    // 2^100 by literal.
    assert_eq!(
        bootstrap_eval("1267650600228229401496703205376"),
        "OK 1267650600228229401496703205376"
    );
    // Reader-produced bignum participates correctly in arithmetic.
    assert_eq!(
        bootstrap_eval("(+ 1267650600228229401496703205376 1)"),
        "OK 1267650600228229401496703205377"
    );
    // bignump on a literal.
    assert_eq!(
        bootstrap_eval("(bignump 1267650600228229401496703205376)"),
        "OK t"
    );
}

/// Regression for the symbol-redirect refactor §7.3 (Phase 7).
/// Mirrors GNU's `let_shadows_buffer_binding_p` invariant: a
/// `(let ((buffer-local-var ...)) ...)` form in buffer A must NOT
/// affect any other buffer's value of the same variable, and the
/// original A binding must be restored after the let unwinds.
///
/// This is the riskiest mechanism in the whole symbol-redirect
/// plan. The test exercises the existing NeoMacs `specbind` /
/// `unbind_to` dispatch to confirm GNU semantics hold today before
/// later phases rewire the hot path through the new
/// `Obarray::set_internal_localized` BLV machinery.
#[test]
fn let_buffer_local_does_not_corrupt_other_buffers() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_a = ev.buffers.create_buffer("A");
    let buf_b = ev.buffers.create_buffer("B");
    ev.buffers.set_current(buf_a);
    ev.eval_str("(make-variable-buffer-local 'phase7-x)")
        .expect("make-variable-buffer-local should succeed");
    // Seed each buffer with its own per-buffer value via setq.
    ev.eval_str("(setq phase7-x 1)").expect("setq A");
    ev.buffers.set_current(buf_b);
    ev.eval_str("(setq phase7-x 2)").expect("setq B");
    // Switch back to A and let-bind phase7-x to 999. Inside the
    // let, switching to B must read B's value (2), NOT 999.
    // We use save-current-buffer + set-buffer instead of
    // with-current-buffer because the latter is a macro that may
    // not be available in Context::new().
    ev.buffers.set_current(buf_a);
    let inside = ev.eval_str(
        "(let ((phase7-x 999))
           (save-current-buffer
             (set-buffer (get-buffer \"B\"))
             phase7-x))",
    );
    assert!(
        inside.is_ok(),
        "let+set-buffer should not error: {:?}",
        inside
    );
    let inside_val = inside.unwrap();
    assert_eq!(
        inside_val.as_int(),
        Some(2),
        "with-current-buffer B inside let should read B's local value (2), \
         got {:?}",
        inside_val
    );
    // After the let unwinds, A's binding must be restored to its
    // pre-let value (1).
    ev.buffers.set_current(buf_a);
    let after_a = ev.eval_str("phase7-x").unwrap();
    assert_eq!(
        after_a.as_int(),
        Some(1),
        "after let unwinds, buffer A's binding must be restored to 1, got {:?}",
        after_a
    );
    // And B's binding is unchanged.
    ev.buffers.set_current(buf_b);
    let after_b = ev.eval_str("phase7-x").unwrap();
    assert_eq!(
        after_b.as_int(),
        Some(2),
        "buffer B's binding must still be 2, got {:?}",
        after_b
    );
}

#[test]
fn dynamic_lambda_parameter_uses_and_restores_forwarded_buffer_slot() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (setq mode-name "Outer")
             (let ((during (funcall '(lambda (mode-name) mode-name) "Inner")))
               (list during mode-name
                     (boundp 'mode-name)
                     (local-variable-p 'mode-name))))"#,
    );
    assert_eq!(result[0], r#"OK ("Inner" "Outer" t t)"#);
}

#[test]
fn dynamic_bytecode_parameter_uses_and_restores_forwarded_buffer_slot() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (setq mode-name "Outer")
             (let* ((fn (byte-compile '(lambda (mode-name) mode-name)))
                    (during (funcall fn "Inner")))
               (list during mode-name
                     (boundp 'mode-name)
                     (local-variable-p 'mode-name))))"#,
    );
    assert_eq!(result[0], r#"OK ("Inner" "Outer" t t)"#);
}

/// Regression for the printer side of audit §1.1: bignums must
/// round-trip through prin1, number-to-string, format %d/%x/%o, and
/// string-to-number. Mirrors GNU Emacs's bignum print/parse symmetry.
#[test]
fn bignum_round_trips_through_print_and_parse() {
    crate::test_utils::init_test_tracing();
    // prin1 of a literal bignum.
    assert_eq!(
        eval_one("(prin1-to-string 1267650600228229401496703205376)"),
        "OK \"1267650600228229401496703205376\""
    );
    // number-to-string on a bignum.
    assert_eq!(
        eval_one("(number-to-string (ash 1 100))"),
        "OK \"1267650600228229401496703205376\""
    );
    // format %d on a bignum.
    assert_eq!(
        eval_one("(format \"%d\" (ash 1 100))"),
        "OK \"1267650600228229401496703205376\""
    );
    // format %x on a bignum.
    assert_eq!(
        eval_one("(format \"%x\" (ash 1 100))"),
        "OK \"10000000000000000000000000\""
    );
    // string-to-number reads a bignum literal.
    assert_eq!(
        eval_one("(string-to-number \"1267650600228229401496703205376\")"),
        "OK 1267650600228229401496703205376"
    );
    // Parse → arithmetic → print round-trip.
    assert_eq!(
        eval_one("(number-to-string (* (string-to-number \"1267650600228229401496703205376\") 2))"),
        "OK \"2535301200456458802993406410752\""
    );
}

/// Regression for audit Phase B: file primitives must dispatch
/// through `file-name-handler-alist`. Mirrors GNU's `Ffind_file_name_handler`
/// pattern (`src/fileio.c:371`) — file predicates first expand the
/// file name, then check the alist before doing native I/O. We install
/// a fake handler that records the `(operation . args)` it was invoked
/// with, returns the expanded file name for `expand-file-name`, and
/// returns a sentinel for the predicates.
#[test]
fn file_name_handler_dispatch_invokes_handler_for_matching_filenames() {
    crate::test_utils::init_test_tracing();
    // Use a raw lambda on the alist instead of a `defun`-defined
    // symbol — `Context::new()` is the bare-metal evaluator and
    // doesn't include the higher-level `defun` macro. The raw
    // lambda value is what `find-file-name-handler` returns and
    // what `funcall` invokes, mirroring the same dispatch path
    // a real handler symbol would take.
    let results = eval_all(
        r#"
        (setq my-handler-log nil)
        (setq file-name-handler-alist
              (cons (cons "\\`/fake:"
                          (lambda (op &rest args)
                            (setq my-handler-log
                                  (cons (cons op args) my-handler-log))
                            (if (eq op 'expand-file-name)
                                (car args)
                              'sentinel)))
                    nil))
        (file-exists-p "/fake:foo")
        (file-directory-p "/fake:bar")
        (file-readable-p "/fake:baz")
        (file-symlink-p "/fake:link")
        (expand-file-name "/fake:abs")
        (length my-handler-log)
        ;; GNU calls expand-file-name before each predicate-specific
        ;; handler. The log is built via `cons`, so reverse it before
        ;; comparing chronological operation names.
        (mapcar 'car (reverse my-handler-log))
        "#,
    );
    // Skip the two setq forms; assertions start at index 2.
    let answers: Vec<&String> = results.iter().skip(2).collect();
    assert_eq!(answers[0], "OK sentinel"); // file-exists-p
    assert_eq!(answers[1], "OK sentinel"); // file-directory-p
    assert_eq!(answers[2], "OK sentinel"); // file-readable-p
    assert_eq!(answers[3], "OK sentinel"); // file-symlink-p
    assert_eq!(answers[4], "OK \"/fake:abs\""); // expand-file-name returns a string
    assert_eq!(answers[5], "OK 9"); // four expand+predicate pairs, plus explicit expand
    assert_eq!(
        answers[6],
        "OK (expand-file-name file-exists-p expand-file-name file-directory-p expand-file-name file-readable-p expand-file-name file-symlink-p expand-file-name)"
    );

    // A non-matching filename must not invoke the handler — verifies
    // we don't dispatch indiscriminately. /tmp doesn't start with /fake:.
    let no_match = eval_all(
        r#"
        (setq my-handler-log nil)
        (setq file-name-handler-alist
              (cons (cons "\\`/fake:"
                          (lambda (op &rest args)
                            (setq my-handler-log (cons op my-handler-log))
                            'never-called))
                    nil))
        (file-exists-p "/tmp")
        my-handler-log
        "#,
    );
    // Result of file-exists-p depends on /tmp existing — we only
    // care that the handler did NOT log anything.
    assert!(no_match[2].starts_with("OK "));
    assert_eq!(no_match[3], "OK nil");
}

#[test]
fn substring_accepts_vectors_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(substring [10 20 30 40 50] 1 4)"),
        "OK [20 30 40]"
    );
    assert_eq!(eval_one("(substring [10 20 30 40 50] -3 -1)"), "OK [30 40]");
    assert_eq!(eval_one("(substring [10 20 30] 0)"), "OK [10 20 30]");
}

#[test]
fn substring_then_string_match_mirrors_gnu_bracket_class_closing() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(let* ((code "x = 42;")
                      (rest (substring code 2)))
                 (list rest
                       (string-match "\\`[-+*/=<>!&|(){}\\[\\];,.]" rest)))"#
        ),
        r#"OK ("= 42;" nil)"#
    );
}

#[test]
fn bootstrap_string_match_posix_upper_class_folds_to_alpha_under_case_fold() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(list
                 (string-match "[[:upper:]]+" "helloWORLDfoo")
                 (match-string 0 "helloWORLDfoo"))"#
        ),
        r#"OK (0 "helloWORLDfoo")"#
    );
}

#[test]
fn bootstrap_looking_at_case_fold_treats_unibyte_high_bytes_as_raw() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((magic (unibyte-string #xed #xab #xee #xdb 3 0))
                       (case-fold-search t))
                   (insert magic)
                   (goto-char (point-min))
                   (looking-at magic)))"#
        ),
        "OK t"
    );
}

#[test]
fn bootstrap_string_match_explicit_numbered_group_preserves_group_slot() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(let ((case-fold-search nil))
                 (list
                  (string-match "\\(?9:[A-Z]+\\)" "xxABCyy")
                  (match-string 9 "xxABCyy")))"#
        ),
        r#"OK (2 "ABC")"#
    );
}

#[test]
fn bootstrap_string_match_open_interval_quantifier_matches_gnu_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(list
                 (string-match "a\\{,2\\}b" "aab")
                 (match-string 0 "aab"))"#
        ),
        r#"OK (0 "aab")"#
    );
}

#[test]
fn string_match_descending_interval_signals_invalid_regexp_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU: (string-match "a\\{2,1\\}" "aa") => (invalid-regexp "Invalid content of \\{\\}")
    // A descending interval (lower > upper) must be rejected, not accepted.
    // The error-data string is "Invalid content of \{\}" (single backslashes),
    // which prin1 / print_value renders with the backslashes doubled.
    assert_eq!(
        eval_one(
            r#"(condition-case e (string-match "a\\{2,1\\}" "aa")
                 (invalid-regexp (car (cdr e))))"#
        ),
        r#"OK "Invalid content of \\{\\}""#
    );
    // Also {5,2} and {3,0}.
    assert_eq!(
        eval_one(
            r#"(condition-case e (string-match "a\\{5,2\\}" "aaaaa")
                 (invalid-regexp (car (cdr e))))"#
        ),
        r#"OK "Invalid content of \\{\\}""#
    );
    assert_eq!(
        eval_one(
            r#"(condition-case e (string-match "a\\{3,0\\}" "aaa")
                 (invalid-regexp (car (cdr e))))"#
        ),
        r#"OK "Invalid content of \\{\\}""#
    );
}

#[test]
fn string_match_stacked_trailing_quantifiers_fold_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU folds a redundant trailing quantifier onto the preceding one, so each
    // of these matches at position 0 of "aaa" (neo previously returned nil and
    // then signaled args-out-of-range in match-string).
    for pat in [r#"a**"#, r#"a*?*"#, r#"a*+"#, r#"a++"#, r#"a???"#] {
        assert_eq!(
            eval_one(&format!(r#"(string-match "{pat}" "aaa")"#)),
            "OK 0",
            "pattern {pat:?} should match at 0 like GNU"
        );
    }
    // a** folds to greedy a*, so it consumes the whole run: the match end is 3.
    // (match-string is a lisp subr unavailable in the bare context, so assert
    // the match-end via the C builtin match-end instead.)
    assert_eq!(
        eval_one(r#"(progn (string-match "a**" "aaa") (match-end 0))"#),
        "OK 3"
    );
}

#[test]
fn bootstrap_string_match_posix_char_class_sequence_matches_gnu_order() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(list
                 (string-match "[[:alpha:]]+" "hello123")
                 (match-string 0 "hello123")
                 (string-match "[[:digit:]]+" "hello123")
                 (match-string 0 "hello123")
                 (string-match "[[:alnum:]]+" "  abc123  ")
                 (match-string 0 "  abc123  ")
                 (string-match "[[:space:]]+" "hello   world")
                 (match-string 0 "hello   world")
                 (string-match "[[:upper:]]+" "helloWORLDfoo")
                 (match-string 0 "helloWORLDfoo")
                 (string-match "[[:lower:]]+" "HELLOworldFOO")
                 (match-string 0 "HELLOworldFOO")
                 (string-match "[[:punct:]]+" "hello!@#world")
                 (match-string 0 "hello!@#world")
                 (string-match "[^[:digit:]]+" "123abc456")
                 (match-string 0 "123abc456")
                 (string-match "[[:alpha:][:digit:]]+" "---abc123---")
                 (match-string 0 "---abc123---")
                 (progn (string-match "[[:blank:]]+" "a \t b")
                        (match-string 0 "a \t b")))"#
        ),
        r#"OK (0 "hello" 5 "123" 2 "abc123" 5 "   " 0 "helloWORLDfoo" 0 "HELLOworldFOO" 5 "!@#" 3 "abc" 3 "abc123" " 	 ")"#
    );
}

#[test]
fn void_function_symbol_signals_before_evaluating_arguments_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"
(let ((vm-side nil))
  (condition-case err
      (vm-undefined-function
       (progn
         (setq vm-side t)
         1))
    (error (list err vm-side))))
"#
        ),
        "OK ((void-function vm-undefined-function) nil)"
    );
}

#[test]
fn eval_of_generated_lambda_preserves_uninterned_symbol_identity() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"
(let* ((exp (make-symbol "exp"))
       (form (list 'let
                   '((lexical-binding t))
                   (list 'lambda
                         '(new)
                         (list 'let*
                               (list (list exp 'new)
                                     (list 'x exp))
                               'x))))
       (f (eval form t)))
  (funcall f 42))
"#
        ),
        "OK 42"
    );
}

#[test]
fn save_restriction_restores_labeled_restrictions_and_widen_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer_id = eval.buffers.create_buffer("eval-labeled-restriction");
    eval.buffers.set_current(buffer_id);
    let _ = eval.buffers.insert_into_buffer(buffer_id, "abcdef");
    let result = eval.eval_str(
        r#"(progn
             (internal--labeled-narrow-to-region 2 5 'tag)
             (list (point-min) (point-max)
                   (save-restriction
                     (internal--labeled-widen 'tag)
                     (list (point-min) (point-max)))
                   (point-min) (point-max)
                   (progn (widen) (list (point-min) (point-max)))
                   (progn (internal--labeled-widen 'tag)
                          (list (point-min) (point-max)))))"#,
    );
    assert_eq!(
        format_eval_result(&result),
        "OK (2 5 (1 7) 2 5 (2 5) (1 7))"
    );
}

#[test]
fn indirect_replace_while_widened_restores_original_restriction_once() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((base (get-buffer-create "indirect-replace-base"))
                     child)
                 (set-buffer base)
                 (erase-buffer)
                 (insert "abc")
                 (setq child (make-indirect-buffer
                              base "indirect-replace-child" t))
                 (set-buffer child)
                 (narrow-to-region 2 3)
                 (save-excursion
                   (save-restriction
                     (widen)
                     (goto-char 2)
                     (search-forward "b")
                     (replace-match "XX" t t)))
                 (list (point-min)
                       (point-max)
                       (buffer-substring-no-properties
                        (point-min) (point-max))))"#
        ),
        "OK (2 4 \"XX\")"
    );
}

#[test]
fn redisplay_restores_current_innermost_labeled_restriction_after_callback_mutation() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer_id = eval.buffers.create_buffer("redisplay-labeled");
    eval.buffers.set_current(buffer_id);
    let _ = eval.buffers.insert_into_buffer(buffer_id, "abcdef");
    let _ = eval.buffers.internal_labeled_narrow_to_emacs_byte_range(
        buffer_id,
        EmacsByteRange::from_usize(1, 5),
        Value::symbol("outer"),
    );
    let _ = eval.buffers.internal_labeled_narrow_to_emacs_byte_range(
        buffer_id,
        EmacsByteRange::from_usize(2, 4),
        Value::symbol("inner"),
    );

    let observed = Rc::new(RefCell::new(Vec::new()));
    let observed_in_callback = observed.clone();
    eval.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let buf = ev
            .buffers
            .get(buffer_id)
            .expect("buffer visible during redisplay");
        observed_in_callback.borrow_mut().push((
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        let _ = ev
            .buffers
            .internal_labeled_widen(buffer_id, &Value::symbol("inner"));
        let buf = ev
            .buffers
            .get(buffer_id)
            .expect("buffer after labeled widen");
        observed_in_callback.borrow_mut().push((
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
    }));

    eval.redisplay();

    assert_eq!(*observed.borrow(), vec![(0, 6), (1, 5)]);
    let buf = eval.buffers.get(buffer_id).expect("buffer after redisplay");
    assert_eq!(
        (
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get()
        ),
        (1, 5)
    );
}

#[test]
fn simple_defvar_declares_local_dynamic_scope_in_lexical_environment() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.lexenv = Value::list(vec![Value::T]);

    let result = ev.eval_str(
        r#"
        (progn
          (defvar vm-local-special)
          (let ((vm-local-special 10))
            (let ((f (lambda () vm-local-special)))
              (let ((vm-local-special 20))
                (funcall f)))))
    "#,
    );
    assert_eq!(format_eval_result(&result), "OK 20");
}

#[test]
fn put_get_preserves_closure_captured_uninterned_symbol_identity() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"
(let* ((exp (make-symbol "exp"))
       (form (list 'let
                   '((lexical-binding t))
                   (list 'lambda
                         '(new)
                         (list 'let*
                               (list (list exp 'new)
                                     (list 'x exp))
                               'x))))
       (f (eval form t)))
  (put 'vm-closure-prop 'vm-test-prop f)
  (garbage-collect)
  (funcall (get 'vm-closure-prop 'vm-test-prop) 42))
"#
        ),
        "OK 42"
    );
}

#[test]
fn recent_input_events_are_bounded() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    for i in 0..(RECENT_INPUT_EVENT_LIMIT + 1) {
        ev.record_input_event(Value::fixnum(i as i64));
    }
    let recent = ev.recent_input_events();
    assert_eq!(recent.len(), RECENT_INPUT_EVENT_LIMIT);
    assert_eq!(recent[0], Value::fixnum(1));
    assert_eq!(
        recent.last(),
        Some(&Value::fixnum(RECENT_INPUT_EVENT_LIMIT as i64))
    );
}

#[test]
fn recent_keys_include_cmds_reports_command_markers() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.record_input_event(Value::fixnum('x' as i64));
    ev.record_recent_command(Value::symbol("forward-char"));

    let plain = ev.eval_str("(recent-keys)").expect("plain recent-keys");
    assert_eq!(
        plain.as_vector_data().expect("plain recent-keys vector"),
        &vec![Value::fixnum('x' as i64)]
    );

    let with_commands = ev
        .eval_str("(recent-keys t)")
        .expect("recent-keys include commands");
    let items = with_commands
        .as_vector_data()
        .expect("recent-keys include commands vector");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], Value::fixnum('x' as i64));
    assert!(items[1].is_cons());
    assert!(items[1].cons_car().is_nil());
    assert_eq!(items[1].cons_cdr(), Value::symbol("forward-char"));
}

#[test]
fn eval_and_compile_defines_function() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let rendered: Vec<String> = ev
        .eval_str_each(
            r#"
        (defalias 'eval-and-compile (cons 'macro #'(lambda (&rest body)
          (list 'quote (eval (cons 'progn body))))))
        (eval-and-compile
          (defalias 'my-test-fn #'(lambda (x) (+ x 1))))
        (my-test-fn 41)
    "#,
        )
        .iter()
        .map(format_eval_result)
        .collect();
    tracing::debug!("eval-and-compile results: {:?}", rendered);
    // The function should be defined by eval-and-compile
    assert!(
        ev.obarray().symbol_function("my-test-fn").is_some(),
        "my-test-fn should be defined after eval-and-compile"
    );
    assert_eq!(rendered[2], "OK 42");
}

#[test]
fn eval_and_compile_with_backtick_name() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results: Vec<String> = ev
        .eval_str_each(r#"
        (defalias 'eval-and-compile (cons 'macro #'(lambda (&rest body)
          (list 'quote (eval (cons 'progn body))))))
        (let ((fsym (intern (format "%s--pcase-macroexpander" '\`))))
          (eval (list 'eval-and-compile
                      (list 'defalias (list 'quote fsym) (list 'function (list 'lambda '(x) '(+ x 1)))))))
    "#)
        .iter()
        .map(format_eval_result)
        .collect();
    tracing::debug!("backtick-name results: {:?}", results);
    let has_fn = ev
        .obarray()
        .symbol_function("`--pcase-macroexpander")
        .is_some();
    tracing::debug!("`--pcase-macroexpander defined: {}", has_fn);
    // Check what format produces for the backtick symbol
    let fmt_result = ev.eval_str(r#"(format "%s--pcase-macroexpander" '\`)"#);
    tracing::debug!("format result: {:?}", format_eval_result(&fmt_result));
}

#[test]
fn float_arithmetic() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(+ 1.0 2.0)"), "OK 3.0");
    assert_eq!(eval_one("(+ 1 2.0)"), "OK 3.0"); // int promoted to float
    assert_eq!(eval_one("(/ 10.0 3.0)"), "OK 3.3333333333333335");
}

#[test]
fn eq_float_corner_cases_match_oracle_shape() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(list (eq 1.0 1.0) (let ((x 1.0)) (eq x x)) (eq 0.0 -0.0) (eql 0.0 -0.0))"),
        "OK (nil t nil nil)"
    );
}

#[test]
fn intern_keyword_matches_reader_keyword_for_eq_and_memq() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((k (intern ":beginning"))
                      (keys (list k (intern ":end") (intern ":value"))))
                 (list (keywordp k)
                       (eq k :beginning)
                       (if (memq :beginning keys) t nil)
                       (eq (intern-soft ":beginning") :beginning)))"#
        ),
        "OK (t t t t)"
    );
}

#[test]
fn eval_keyword_checks_explicit_lexenv_before_self_value() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(list (eval :vm-eval-keyword) (eval ':vm-eval-keyword '((:vm-eval-keyword . 7))))"
        ),
        "OK (:vm-eval-keyword 7)"
    );
}

#[test]
fn intern_canonicalizes_ascii_multibyte_names_to_existing_symbol() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((m (string-to-multibyte "foo")))
                 (list (multibyte-string-p m)
                       (eq (intern m) 'foo)
                       (multibyte-string-p (symbol-name (intern m)))))"#
        ),
        "OK (t t nil)"
    );
}

#[test]
fn intern_reuses_ldefs_autoload_symbol_for_ascii_multibyte_name() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_ldefs_boot_autoloads(&["batch-byte-compile"]);
    let result = ev.eval_str(
        r#"(let ((m (string-to-multibyte "batch-byte-compile")))
             (let ((sym (intern m)))
               (list (eq sym 'batch-byte-compile)
                     (fboundp sym)
                     (multibyte-string-p (symbol-name sym)))))"#,
    );
    assert_eq!(format_eval_result(&result), "OK (t t nil)");
}

#[test]
fn setq_keeps_canonical_symbols_in_obarray() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((s 'vm-ghost))
                 (setq vm-ghost 1)
                 (list (if (intern-soft "vm-ghost") t nil)
                       (let (seen)
                         (mapatoms (lambda (x) (if (eq x s) (progn (setq seen t)))))
                         seen)
                       (symbol-value s)))"#
        ),
        "OK (t t 1)"
    );
}

#[test]
fn uninterned_nil_function_is_not_treated_as_canonical_nil() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(let ((s (make-symbol "nil")))
                 (fset s (lambda () 'ok))
                 (list (special-form-p s) (funcall s)))"#
        ),
        "OK (nil ok)"
    );
}

#[test]
fn comparisons() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(< 1 2)"), "OK t");
    assert_eq!(eval_one("(> 1 2)"), "OK nil");
    assert_eq!(eval_one("(= 3 3)"), "OK t");
    assert_eq!(eval_one("(<= 3 3)"), "OK t");
    assert_eq!(eval_one("(>= 5 3)"), "OK t");
    assert_eq!(eval_one("(/= 1 2)"), "OK t");
    assert_eq!(
        eval_one("(list (= 3) (< 3) (> 3) (<= 3) (>= 3))"),
        "OK (t t t t t)"
    );
    assert_eq!(
        eval_one(
            "(list (condition-case err (/= 1) (error (car err)))
                   (condition-case err (/= 1 2 3) (error (car err))))"
        ),
        "OK (wrong-number-of-arguments wrong-number-of-arguments)"
    );
}

#[test]
fn type_predicates() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(integerp 42)"), "OK t");
    assert_eq!(eval_one("(floatp 3.14)"), "OK t");
    assert_eq!(eval_one("(stringp \"hello\")"), "OK t");
    assert_eq!(eval_one("(symbolp 'foo)"), "OK t");
    assert_eq!(eval_one("(consp '(1 2))"), "OK t");
    assert_eq!(eval_one("(null nil)"), "OK t");
    assert_eq!(eval_one("(null t)"), "OK nil");
    assert_eq!(eval_one("(listp nil)"), "OK t");
}

#[test]
fn string_operations() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(r#"(concat "hello" " " "world")"#),
        r#"OK "hello world""#
    );
    assert_eq!(eval_one(r#"(substring "hello" 1 3)"#), r#"OK "el""#);
    assert_eq!(eval_one(r#"(length "hello")"#), "OK 5");
    assert_eq!(eval_one(r#"(upcase "hello")"#), r#"OK "HELLO""#);
    assert_eq!(eval_one(r#"(string-equal "abc" "abc")"#), "OK t");
}

#[test]
fn empty_strings_are_canonical_per_storage_kind_like_gnu() {
    crate::test_utils::init_test_tracing();

    // GNU alloc.c owns one permanent empty string for each storage kind.
    // All zero-length construction paths return the corresponding object,
    // while the unibyte and multibyte objects remain distinct.
    assert_eq!(
        eval_one(
            r#"(progn
                 ;; Seed the multibyte singleton, then prove the allocator's
                 ;; private cache is itself a permanent root.
                 (make-string 0 ?x t)
                 (garbage-collect)
                 (let ((unibyte (make-string 0 ?x nil))
                       (multibyte (make-string 0 ?x t)))
                   (list (eq "" "")
                         (eq "" unibyte)
                         (multibyte-string-p unibyte)
                         (multibyte-string-p multibyte)
                         (eq unibyte multibyte)
                         (eq unibyte (make-string 0 ?x nil))
                         (eq multibyte (make-string 0 ?x t)))))"#,
        ),
        "OK (t t nil t nil t t)"
    );
}

#[test]
fn and_or_cond() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(and 1 2 3)"), "OK 3");
    assert_eq!(eval_one("(and 1 nil 3)"), "OK nil");
    assert_eq!(eval_one("(or nil nil 3)"), "OK 3");
    assert_eq!(eval_one("(or nil nil nil)"), "OK nil");
    assert_eq!(eval_one("(cond (nil 1) (t 2))"), "OK 2");
}

#[test]
fn while_loop() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(let ((x 0)) (while (< x 5) (setq x (1+ x))) x)"),
        "OK 5"
    );
}

#[test]
fn defvar_only_sets_if_unbound() {
    crate::test_utils::init_test_tracing();
    let results = eval_all("(defvar x 42) x (defvar x 99) x");
    assert_eq!(results, vec!["OK x", "OK 42", "OK x", "OK 42"]);
}

#[test]
fn defining_outer_lexical_binding_dynamic_errors_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(eval '(let ((vm-same-scope-special nil))
                       (defvar vm-same-scope-special 1)
                       vm-same-scope-special)
                    t)"#
        ),
        "OK nil"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (eval '(let ((vm-late-special-defvar nil))
                            (eval '(defvar vm-late-special-defvar 1) t))
                         t)
                 (error err))"#
        ),
        "OK (error \"Defining as dynamic an already lexical var\" vm-late-special-defvar)"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (eval '(let ((vm-late-special-internal nil))
                            (eval '(internal--define-uninitialized-variable
                                     'vm-late-special-internal)
                                  t))
                         t)
                 (error err))"#
        ),
        "OK (error \"Defining as dynamic an already lexical var\" vm-late-special-internal)"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (eval '(let ((vm-late-special-defconst nil))
                            (eval '(defconst vm-late-special-defconst 1) t))
                         t)
                 (error err))"#
        ),
        "OK (error \"Defining as dynamic an already lexical var\" vm-late-special-defconst)"
    );
}

#[test]
fn defconst_updates_dynamic_binding_without_enforcing_constancy() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(let ((vm-defconst-local 1)) (defvar vm-defconst-local 2) vm-defconst-local)
         (let ((vm-defconst-local 1)) (defconst vm-defconst-local 3) vm-defconst-local)
         (progn (defconst vm-defconst-mutable 1) (setq vm-defconst-mutable 2) vm-defconst-mutable)",
    );
    assert_eq!(results, vec!["OK 1", "OK 3", "OK 2"]);
}

#[test]
fn bootstrap_does_not_prebind_lisp_derived_mode_tables() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();

    for name in [
        "completion-list-mode-abbrev-table",
        "completion-list-mode-syntax-table",
        "minibuffer-inactive-mode-abbrev-table",
        "minibuffer-inactive-mode-syntax-table",
        "minibuffer-mode-abbrev-table",
    ] {
        assert_eq!(
            ev.obarray.symbol_value(name),
            None,
            "{name} must remain void until Lisp define-derived-mode creates it"
        );
    }
}

#[test]
fn defvar_and_defconst_error_payloads_match_oracle_edges() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(condition-case err (defvar) (error err))
         (condition-case err (defvar 1) (error err))
         (condition-case err (defvar 'vm-dv) (error err))
         (condition-case err (defvar vm-dv 1 \"doc\" t) (error err))
         (condition-case err (defconst) (error err))
         (condition-case err (defconst vm-dc) (error err))
         (condition-case err (defconst 1 2) (error err))
         (condition-case err (defconst 'vm-dc 1) (error err))
         (condition-case err (defconst vm-dc 1 \"doc\" t) (error err))",
    );
    assert_eq!(results[0], "OK (wrong-number-of-arguments defvar 0)");
    assert_eq!(results[1], "OK (wrong-type-argument symbolp 1)");
    assert_eq!(results[2], "OK (wrong-type-argument symbolp 'vm-dv)");
    assert_eq!(results[3], "OK (error \"Too many arguments\")");
    assert_eq!(results[4], "OK (wrong-number-of-arguments defconst 0)");
    assert_eq!(results[5], "OK (wrong-number-of-arguments defconst 1)");
    assert_eq!(results[6], "OK (wrong-type-argument symbolp 1)");
    assert_eq!(results[7], "OK (wrong-type-argument symbolp 'vm-dc)");
    assert_eq!(results[8], "OK (error \"Too many arguments\")");
}

#[test]
fn setq_local_makes_binding_buffer_local() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one("(with-temp-buffer (set (make-local-variable 'vm-x) 7) vm-x)");
    assert_eq!(result, "OK 7");
}

#[test]
fn setq_local_constant_and_type_payloads_match_oracle() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(list
            (condition-case err (set (make-local-variable ':foo) 1) (error err))
            (condition-case err (set (make-local-variable 'nil) 1) (error err))
            (condition-case err (set (make-local-variable 't) 1) (error err))
            (condition-case err (set (make-local-variable 1) 2) (error err)))
         (let ((x 0))
           (condition-case err
               (set (make-local-variable 'nil) (setq x 1))
             (error (list err x))))
         (let ((x 0))
           (condition-case err
               (set (make-local-variable ':foo) (setq x 2))
             (error (list err x))))",
    );
    assert_eq!(
        results[0],
        "OK ((setting-constant :foo) (setting-constant nil) (setting-constant t) (wrong-type-argument symbolp 1))"
    );
    // make-local-variable signals before the RHS is evaluated
    assert_eq!(results[1], "OK ((setting-constant nil) 0)");
    assert_eq!(results[2], "OK ((setting-constant :foo) 0)");
}

#[test]
fn setq_local_follows_variable_alias_resolution() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        "(progn
           (defvaralias 'vm-setq-local-alias 'vm-setq-local-base)
           (with-temp-buffer
             (setq-local vm-setq-local-alias 5)
             (list
               (symbol-value 'vm-setq-local-alias)
               (symbol-value 'vm-setq-local-base)
               (local-variable-p 'vm-setq-local-alias)
               (local-variable-p 'vm-setq-local-base)
               (buffer-local-boundp 'vm-setq-local-alias (current-buffer))
               (buffer-local-boundp 'vm-setq-local-base (current-buffer)))))",
    );
    assert_eq!(result, "OK (5 5 t t t t)");
}

#[test]
fn setq_local_alias_to_constant_preserves_error_payload_and_rhs_skip() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(progn
           (defvaralias 'vm-setq-local-const 'nil)
           (let ((x 0))
             (condition-case err
                 (set (make-local-variable 'vm-setq-local-const) (setq x 1))
               (error (list err x)))))
         (progn
           (defvaralias 'vm-setq-local-const-k ':vm-setq-local-k)
           (let ((x 0))
             (condition-case err
                 (set (make-local-variable 'vm-setq-local-const-k) (setq x 2))
               (error (list err x)))))",
    );
    // make-local-variable signals before the RHS is evaluated
    assert_eq!(results[0], "OK ((setting-constant vm-setq-local-const) 0)");
    assert_eq!(
        results[1],
        "OK ((setting-constant vm-setq-local-const-k) 0)"
    );
}

#[test]
fn setq_local_alias_triggers_single_watcher_callback_on_resolved_target() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_subr(
        "(progn
           (setq vm-setq-local-watch-events nil)
           (fset 'vm-setq-local-watch-rec
                 (lambda (symbol newval operation where)
                   (setq vm-setq-local-watch-events
                         (cons (list symbol newval operation where)
                               vm-setq-local-watch-events))))
           (defvaralias 'vm-setq-local-watch 'vm-setq-local-watch-base)
           (add-variable-watcher 'vm-setq-local-watch-base 'vm-setq-local-watch-rec)
           (with-temp-buffer
             (set (make-local-variable 'vm-setq-local-watch) 7))
           (let ((where (nth 3 (car vm-setq-local-watch-events))))
             (list (length vm-setq-local-watch-events)
                   (car (car vm-setq-local-watch-events))
                   (nth 1 (car vm-setq-local-watch-events))
                   (nth 2 (car vm-setq-local-watch-events))
                   (bufferp where)
                   (buffer-live-p where))))",
    );
    assert_eq!(result, "OK (1 vm-setq-local-watch-base 7 set t nil)");
}

#[test]
fn defmacro_works() {
    crate::test_utils::init_test_tracing();
    let result = eval_all(
        "(defalias 'my-when (cons 'macro #'(lambda (cond &rest body)
           (list 'if cond (cons 'progn body)))))
         (my-when t 1 2 3)",
    );
    assert_eq!(result[1], "OK 3");
}

#[test]
fn defun_and_defmacro_allow_empty_body() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defalias 'vm-empty-f #'(lambda nil))
         (vm-empty-f)
         (defalias 'vm-empty-m (cons 'macro #'(lambda nil)))
         (vm-empty-m)",
    );
    assert_eq!(results[0], "OK vm-empty-f");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK vm-empty-m");
    assert_eq!(results[3], "OK nil");
}

#[test]
fn defun_and_defmacro_error_payloads_match_oracle_edges() {
    crate::test_utils::init_test_tracing();
    // defun and defmacro are no longer bare-evaluator special forms;
    // they are Elisp macros loaded from byte-run.el during bootstrap.
    // In a bare evaluator they are void functions.
    let results = eval_all(
        "(condition-case err (defun) (error err))
         (condition-case err (defmacro) (error err))",
    );
    assert_eq!(results[0], "OK (void-function defun)");
    assert_eq!(results[1], "OK (void-function defmacro)");
}

#[test]
fn optional_and_rest_params() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defalias 'f #'(lambda (a &optional b &rest c) (list a b c)))
         (f 1)
         (f 1 2)
         (f 1 2 3 4)",
    );
    assert_eq!(results[1], "OK (1 nil nil)");
    assert_eq!(results[2], "OK (1 2 nil)");
    assert_eq!(results[3], "OK (1 2 (3 4))");
}

#[test]
fn when_unless() {
    crate::test_utils::init_test_tracing();
    // when/unless are no longer bare-evaluator special forms; use if+progn.
    assert_eq!(eval_one("(if t (progn 1 2 3))"), "OK 3");
    assert_eq!(eval_one("(if nil (progn 1 2 3))"), "OK nil");
    assert_eq!(eval_one("(if nil nil (progn 1 2 3))"), "OK 3");
    assert_eq!(eval_one("(if t nil (progn 1 2 3))"), "OK nil");
}

#[test]
fn bound_and_true_p_runtime_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(bootstrap_eval_one("(fboundp 'bound-and-true-p)"), "OK t");
    assert_eq!(bootstrap_eval_one("(macrop 'bound-and-true-p)"), "OK t");
    // After the specbind refactor, let-bindings write to the obarray;
    // bound-and-true-p sees the value only when bound at the toplevel.
    assert_eq!(
        bootstrap_eval_one("(progn (setq vm-batp t) (bound-and-true-p vm-batp))"),
        "OK t"
    );
    assert_eq!(
        bootstrap_eval_one("(progn (setq vm-batp nil) (bound-and-true-p vm-batp))"),
        "OK nil"
    );
    assert_eq!(
        bootstrap_eval_one("(bound-and-true-p vm-batp-unbound)"),
        "OK nil"
    );
    assert_eq!(
        bootstrap_eval_one("(condition-case err (bound-and-true-p) (error err))"),
        "OK (wrong-number-of-arguments (1 . 1) 0)"
    );
    assert_eq!(
        bootstrap_eval_one("(condition-case err (bound-and-true-p a b) (error err))"),
        "OK (wrong-number-of-arguments (1 . 1) 2)"
    );
    assert_eq!(
        bootstrap_eval_one("(condition-case err (bound-and-true-p 1) (error err))"),
        "OK (wrong-type-argument symbolp 1)"
    );
}

#[test]
fn hash_table_ops() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(let ((ht (make-hash-table :test 'equal)))
           (puthash \"key\" 42 ht)
           (gethash \"key\" ht))",
    );
    assert_eq!(results[0], "OK 42");
}

#[test]
fn vector_ops() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(aref [10 20 30] 1)"), "OK 20");
    assert_eq!(eval_one("(length [1 2 3])"), "OK 3");
}

#[test]
fn vector_literals_are_self_evaluating_constants() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(aref [f1] 0)"), "OK f1");
    assert_eq!(eval_one("(let ((f1 'shadowed)) (aref [f1] 0))"), "OK f1");
    assert_eq!(eval_one("(aref [(+ 1 2)] 0)"), "OK (+ 1 2)");
    assert_eq!(eval_one("(let ((x 1)) (aref [x] 0))"), "OK x");
}

#[test]
fn sort_keyword_form_returns_stable_copy_by_default() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let* ((xs '((1 . a) (1 . b) (0 . c)))
                    (ys (sort xs :key #'car)))
               (list xs ys (eq xs ys)))"
        ),
        "OK (((1 . a) (1 . b) (0 . c)) ((0 . c) (1 . a) (1 . b)) nil)"
    );
}

#[test]
fn sort_legacy_form_remains_in_place() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let* ((xs '((1 . a) (0 . b)))
                    (ys (sort xs (lambda (a b) (< (car a) (car b))))))
               (list xs ys (eq xs ys)))"
        ),
        "OK (((0 . b) (1 . a)) ((0 . b) (1 . a)) t)"
    );
}

#[test]
fn sort_lisp_predicate_uses_gnu_one_way_lessp() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((n 0)
                   (i 0)
                   xs)
               (while (<= i 127)
                 (setq xs (cons (list i (% i 8) (% i 3)) xs))
                 (setq i (1+ i)))
               (sort xs (lambda (a b)
                          (setq n (1+ n))
                          (and (= (nth 1 a) (nth 1 b))
                               (< (nth 2 a) (nth 2 b)))))
               n)"
        ),
        "OK 127"
    );
}

#[test]
fn format_function() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(r#"(format "hello %s, %d" "world" 42)"#),
        r#"OK "hello world, 42""#
    );
}

#[test]
fn prog1() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(prog1 1 2 3)"), "OK 1");
}

#[test]
fn function_special_form() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defalias 'add1 #'(lambda (x) (+ x 1)))
         (funcall #'add1 5)",
    );
    assert_eq!(results[1], "OK 6");
}

#[test]
fn function_special_form_symbol_and_literal_payloads() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("#'car"), "OK car");
    assert_eq!(eval_one("#'definitely-missing"), "OK definitely-missing");
    assert_eq!(
        eval_one("(condition-case err #'1 (error (car err)))"),
        "OK 1"
    );
    assert_eq!(eval_one("(equal #''(lambda) ''(lambda))"), "OK t");
}

#[test]
fn lambda_captures_docstring_metadata() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let value = ev
        .eval_str("(lambda nil \"lambda-doc\" nil)")
        .expect("eval");
    assert_eq!(
        value
            .closure_docstring()
            .flatten()
            .and_then(|doc| doc.as_utf8_str()),
        Some("lambda-doc")
    );
}

#[test]
fn function_special_form_evaluates_dynamic_documentation_form() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let value = ev
        .eval_str("(function (lambda nil (:documentation (if t \"dyn-doc\" \"bad\")) nil))")
        .expect("eval");
    assert_eq!(
        value
            .closure_docstring()
            .flatten()
            .and_then(|doc| doc.as_utf8_str()),
        Some("dyn-doc")
    );
    let body = value
        .closure_body_value()
        .and_then(|body| crate::emacs_core::value::list_to_vec(&body))
        .expect("expected lambda body");
    assert_eq!(body, vec![Value::NIL]);
}

#[test]
fn function_special_form_value_path_evaluates_dynamic_documentation_form() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let value = ev
        .eval_str("(function (lambda nil (:documentation (if t \"dyn-doc\" \"bad\")) nil))")
        .expect("eval");
    assert_eq!(
        value
            .closure_docstring()
            .flatten()
            .and_then(|doc| doc.as_utf8_str()),
        Some("dyn-doc")
    );
    let body = value
        .closure_body_value()
        .and_then(|body| crate::emacs_core::value::list_to_vec(&body))
        .expect("expected lambda body");
    assert_eq!(body, vec![Value::NIL]);
}

#[test]
fn byte_code_literal_value_path_produces_bytecode() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let value = ev
        .eval_str(r#"#[(x) "\bT\207" [x] 1 (#$ . 83)]"#)
        .expect("eval");
    assert!(value.is_bytecode(), "expected bytecode object, got {value}");
}

#[test]
fn byte_code_literal_value_path_produces_interpreted_closure() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(r##"(funcall (read "#[(x) ((+ x 1)) nil]") 41)"##),
        "OK 42"
    );
}

#[test]
fn quoted_lambda_funcall_keeps_dynamic_documentation_form_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((f '(lambda nil (:documentation (if t \"dyn-doc\" \"bad\")) 7))) (funcall f))"
        ),
        "ERR (void-function (:documentation))"
    );
}

#[test]
fn function_lambda_funcall_strips_dynamic_documentation_form_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((f (function (lambda nil (:documentation (if t \"dyn-doc\" \"bad\")) 7)))) (funcall f))"
        ),
        "OK 7"
    );
}

#[test]
fn lambda_single_string_body_is_a_return_value_not_a_docstring() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let value = ev.eval_str("(lambda nil \"ok-1\")").expect("eval");
    assert_eq!(value.closure_docstring().flatten(), None);
    let body = value
        .closure_body_value()
        .and_then(|body| crate::emacs_core::value::list_to_vec(&body))
        .expect("expected lambda body");
    assert_eq!(body, vec![Value::string("ok-1")]);
    assert_eq!(eval_one("(funcall (lambda nil \"ok-1\"))"), "OK \"ok-1\"");
}

#[test]
fn defmacro_captures_docstring_metadata() {
    crate::test_utils::init_test_tracing();
    // defmacro is no longer a bare-evaluator special form; install a
    // macro with a docstring via defalias + cons 'macro + lambda.
    let mut ev = Context::new();
    ev.eval_str("(defalias 'vm-doc-macro (cons 'macro #'(lambda (x) \"macro-doc\" x)))")
        .expect("eval defalias macro");
    let macro_val = ev
        .obarray
        .symbol_function("vm-doc-macro")
        .expect("macro function cell");
    // The value is (macro . lambda), extract the lambda for docstring.
    let lambda_val = macro_val.cons_cdr();
    assert_eq!(
        lambda_val
            .closure_docstring()
            .flatten()
            .and_then(|doc| doc.as_utf8_str()),
        Some("macro-doc")
    );
}

#[test]
fn function_special_form_wrong_arity_signals() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(condition-case err (function) (error (car err)))"),
        "OK wrong-number-of-arguments"
    );
    assert_eq!(
        eval_one("(condition-case err (function 1 2) (error (car err)))"),
        "OK wrong-number-of-arguments"
    );
}

#[test]
fn special_form_arity_payloads_match_oracle_edges() {
    crate::test_utils::init_test_tracing();
    // when and unless are no longer special forms (now Elisp macros),
    // so they produce void-function errors in a bare evaluator.
    let results = eval_all(
        "(condition-case err (if) (error err))
         (condition-case err (if t) (error err))
         (condition-case err (when) (error err))
         (condition-case err (unless) (error err))
         (condition-case err (quote) (error err))
         (condition-case err (quote 1 2) (error err))
         (condition-case err (function) (error err))
         (condition-case err (function 1 2) (error err))
         (condition-case err (prog1) (error err))
         (condition-case err (catch) (error err))
         (condition-case err (throw) (error err))
         (condition-case err (condition-case) (error err))
         (condition-case err (let) (error err))
         (condition-case err (let*) (error err))
         (condition-case err (while) (error err))
         (condition-case err (unwind-protect) (error err))",
    );
    assert_eq!(results[0], "OK (wrong-number-of-arguments if 0)");
    assert_eq!(results[1], "OK (wrong-number-of-arguments if 1)");
    assert_eq!(results[2], "OK (void-function when)");
    assert_eq!(results[3], "OK (void-function unless)");
    assert_eq!(results[4], "OK (wrong-number-of-arguments quote 0)");
    assert_eq!(results[5], "OK (wrong-number-of-arguments quote 2)");
    assert_eq!(results[6], "OK (wrong-number-of-arguments function 0)");
    assert_eq!(results[7], "OK (wrong-number-of-arguments function 2)");
    assert_eq!(results[8], "OK (wrong-number-of-arguments prog1 0)");
    assert_eq!(results[9], "OK (wrong-number-of-arguments catch 0)");
    assert_eq!(results[10], "OK (wrong-number-of-arguments throw 0)");
    assert_eq!(
        results[11],
        "OK (wrong-number-of-arguments condition-case 0)"
    );
    assert_eq!(results[12], "OK (wrong-number-of-arguments let 0)");
    assert_eq!(results[13], "OK (wrong-number-of-arguments let* 0)");
    assert_eq!(results[14], "OK (wrong-number-of-arguments while 0)");
    assert_eq!(
        results[15],
        "OK (wrong-number-of-arguments unwind-protect 0)"
    );
}

#[test]
fn let_dotted_binding_list_reports_listp_tail_payload() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(condition-case err (let ((x 1) . 2) x) (error err))"),
        "OK (wrong-type-argument listp 2)"
    );
    assert_eq!(
        eval_one("(condition-case err (let* ((x 1) . 2) x) (error err))"),
        "OK (wrong-type-argument listp 2)"
    );
}

#[test]
fn let_and_let_star_binding_constants_signal_setting_constant() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(setq vm-let-a 0 vm-let-b 0)
         (condition-case err
             (let ((t (setq vm-let-a 1))
                   (x (setq vm-let-b 1)))
               x)
           (error (list :error (car err) (cdr err))))
         (list vm-let-a vm-let-b)
         (setq vm-let-a 0 vm-let-b 0)
         (condition-case err
             (let* ((t (setq vm-let-a 1))
                    (x (setq vm-let-b 1)))
               x)
           (error (list :error (car err) (cdr err))))
         (list vm-let-a vm-let-b)
         (condition-case err (let ((nil 1)) nil) (error (list :error (car err) (cdr err))))
         (condition-case err (let* ((nil 1)) nil) (error (list :error (car err) (cdr err))))
         (condition-case err (let (t) t) (error (list :error (car err) (cdr err))))
         (condition-case err (let* (t) t) (error (list :error (car err) (cdr err))))",
    );
    assert_eq!(results[1], "OK (:error setting-constant (t))");
    assert_eq!(results[2], "OK (1 1)");
    assert_eq!(results[4], "OK (:error setting-constant (t))");
    assert_eq!(results[5], "OK (1 0)");
    assert_eq!(results[6], "OK (:error setting-constant (nil))");
    assert_eq!(results[7], "OK (:error setting-constant (nil))");
    assert_eq!(results[8], "OK (:error setting-constant (t))");
    assert_eq!(results[9], "OK (:error setting-constant (t))");
}

/// GNU's `let`/`let*` refuse a constant only through `set_internal`'s
/// `SYMBOL_NOWRITE` arm (`src/data.c:1687-1697`), which explicitly permits
/// "setting keywords to their own value".  `dash`'s `-let` plist
/// destructuring emits exactly that shape — `(let* ((:text (pop src)) ...))`
/// where the popped value IS `:text` — so refusing it breaks every package
/// that destructures a plist with `(&as :key val . rest)`.
///
/// Expected values taken by running each form under GNU Emacs 31.0.90.
#[test]
fn let_binding_a_keyword_to_its_own_value_is_allowed_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(condition-case err (let ((:vm-kw-self :vm-kw-self)) 1)
           (error (list :error (car err) (cdr err))))
         (condition-case err (let* ((:vm-kw-self :vm-kw-self)) 2)
           (error (list :error (car err) (cdr err))))
         (condition-case err (let ((:vm-kw-self 5)) 3)
           (error (list :error (car err) (cdr err))))
         (condition-case err (let* ((:vm-kw-self 5)) 4)
           (error (list :error (car err) (cdr err))))
         :vm-kw-self
         (condition-case err (set :vm-kw-self :vm-kw-self)
           (error (list :error (car err) (cdr err))))
         (condition-case err (let ((t t)) 6)
           (error (list :error (car err) (cdr err))))
         (let* ((plist (list :text \"body\" :buffer 7)))
           (condition-case err
               (let* ((:text (car plist)) (text (nth 1 plist)) (rest (nthcdr 2 plist)))
                 (list text rest))
             (error (list :error (car err) (cdr err)))))",
    );
    assert_eq!(results[0], "OK 1");
    assert_eq!(results[1], "OK 2");
    assert_eq!(results[2], "OK (:error setting-constant (:vm-kw-self))");
    assert_eq!(results[3], "OK (:error setting-constant (:vm-kw-self))");
    // The permitted binding must not have disturbed the keyword's value.
    assert_eq!(results[4], "OK :vm-kw-self");
    assert_eq!(results[5], "OK :vm-kw-self");
    // `t` is a constant but not a keyword: GNU still refuses it.
    assert_eq!(results[6], "OK (:error setting-constant (t))");
    // The exact `-let` plist-destructuring shape from helm-org-rifle.
    assert_eq!(results[7], "OK (\"body\" (:buffer 7))");
}

#[test]
fn lambda_parameters_can_shadow_nil_and_t_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    // Task #36: GNU allows `t` / `nil` to appear as lambda parameter
    // names and the body reads/writes the shadowed cell. The
    // `setting-constant` guard only applies to top-level assignments,
    // not to lambda-parameter bindings. Verified against
    // GNU Emacs 31.0.50: these forms return `(7 9 (1 2 3) (4 5 6))`.
    let results = eval_all(
        "(list
            (funcall (lambda (t) t) 7)
            (funcall (lambda (nil) nil) 9)
            (mapcar (lambda (t) t) '(1 2 3))
            (mapcar (lambda (nil) nil) '(4 5 6)))",
    );
    assert_eq!(results[0], "OK (7 9 (1 2 3) (4 5 6))");
}

#[test]
fn setq_can_assign_shadowing_nil_and_t_parameters_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    // Task #36: with `t` / `nil` shadowed as lambda parameters,
    // setq inside the body is allowed (the specpdl entry from the
    // parameter binding is the "local" that
    // `has_local_binding_by_id` finds, which bypasses the
    // setting-constant guard). Verified against GNU Emacs 31.0.50:
    // these forms return `(9 11)`.
    let results = eval_all(
        "(list
            (funcall (lambda (t) (setq t 9) t) 7)
            (funcall (lambda (nil) (setq nil 11) nil) 8))",
    );
    assert_eq!(results[0], "OK (9 11)");
}

#[test]
fn random_accepts_string_seed_and_repeats_sequences_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(let ((seq1 (progn (random \"vm-random-seed\") (list (random 1000) (random 1000) (random 1000))))
               (seq2 (progn (random \"vm-random-seed\") (list (random 1000) (random 1000) (random 1000)))))
           (list (integerp (random \"vm-random-seed\"))
                 (equal seq1 seq2)
                 (random 1)))",
    );
    assert_eq!(results[0], "OK (t t 0)");
}

#[test]
fn setq_constants_signal_setting_constant_after_rhs_evaluation() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(setq vm-setq-side 0)
         (condition-case err
             (setq nil (setq vm-setq-side 1))
           (error (list (car err) (cdr err) vm-setq-side)))
         (setq vm-setq-side 0)
         (condition-case err
             (setq t (setq vm-setq-side 2))
           (error (list (car err) (cdr err) vm-setq-side)))
         (setq vm-setq-side 0)
         (condition-case err
             (setq :vm-key (setq vm-setq-side 3))
           (error (list (car err) (cdr err) vm-setq-side)))
         (condition-case err (setq 1 2) (error err))",
    );
    assert_eq!(results[1], "OK (setting-constant (nil) 1)");
    assert_eq!(results[3], "OK (setting-constant (t) 2)");
    assert_eq!(results[5], "OK (setting-constant (:vm-key) 3)");
    assert_eq!(results[6], "OK (wrong-type-argument symbolp 1)");
}

#[test]
fn set_ignores_lexical_bindings_and_updates_dynamic_cell() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let results = ev.eval_str_each(
        "(makunbound 'vm-lex-set)
         (let ((vm-lex-set 10))
           (list (set 'vm-lex-set 20) vm-lex-set (symbol-value 'vm-lex-set)))
         (makunbound 'vm-lex-set)",
    );
    assert_eq!(format_eval_result(&results[1]), "OK (20 10 20)");
}

#[test]
fn setq_follows_variable_alias_resolution() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defvaralias 'vm-setq-alias 'vm-setq-base)
         (setq vm-setq-alias 3)
         (list (symbol-value 'vm-setq-base) (symbol-value 'vm-setq-alias))",
    );
    assert_eq!(results[2], "OK (3 3)");
}

#[test]
fn setq_resolves_variable_aliases_only_after_exact_lexical_lookup() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_lexical(
        "(progn
           (defvaralias 'vm-setq-lex-alias 'vm-setq-lex-base)
           (internal-make-var-non-special 'vm-setq-lex-alias)
           (internal-make-var-non-special 'vm-setq-lex-base)
           (defvar vm-setq-lex-events nil)
           (defalias 'vm-setq-lex-watch
             (function
               (lambda (&rest args)
                 (setq vm-setq-lex-events
                       (cons args vm-setq-lex-events)))))
           (add-variable-watcher 'vm-setq-lex-base 'vm-setq-lex-watch)
           (list
             (progn
               (setq vm-setq-lex-base 'global
                     vm-setq-lex-events nil)
               (let ((vm-setq-lex-alias 'lex))
                 (setq vm-setq-lex-alias 'new)
                 (list vm-setq-lex-alias
                       (symbol-value 'vm-setq-lex-base)
                       vm-setq-lex-events)))
             (progn
               (setq vm-setq-lex-base 'global
                     vm-setq-lex-events nil)
               (let ((vm-setq-lex-base 'lex))
                 (setq vm-setq-lex-alias 'new)
                 (list vm-setq-lex-alias
                       vm-setq-lex-base
                       (symbol-value 'vm-setq-lex-base)
                       vm-setq-lex-events)))))",
    );
    assert_eq!(
        result,
        "OK ((new global nil) (new lex new ((vm-setq-lex-base new set nil))))"
    );
}

#[test]
fn special_form_aliases_dispatch_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defalias 'vm-special-if 'if)
         (fset 'vm-special-progn (symbol-function 'progn))
         (list (vm-special-if t 1 2)
               (vm-special-progn 1 2 3))",
    );
    assert_eq!(results[2], "OK (1 3)");
}

#[test]
fn special_form_alias_wrong_arity_mentions_surface_symbol() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defalias 'vm-special-if 'if)
         (condition-case err
             (vm-special-if t)
           (wrong-number-of-arguments err))",
    );
    assert_eq!(results[1], "OK (wrong-number-of-arguments vm-special-if 1)");
}

#[test]
fn makunbound_ignores_lexical_bindings_and_unbinds_runtime_cell() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let results = ev.eval_str_each(
        "(setq vm-lex-makunbound 30)
         (let ((vm-lex-makunbound 10))
           (list (makunbound 'vm-lex-makunbound)
                 vm-lex-makunbound
                 (condition-case err
                     (symbol-value 'vm-lex-makunbound)
                   (error (car err)))))
         (condition-case err
             (symbol-value 'vm-lex-makunbound)
           (error (car err)))",
    );
    assert_eq!(
        format_eval_result(&results[1]),
        "OK (vm-lex-makunbound 10 void-variable)"
    );
    assert_eq!(format_eval_result(&results[2]), "OK void-variable");
}

#[test]
fn makunbound_marks_dynamic_binding_void_without_falling_back_to_global() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defvar vm-mku-dyn 'global)
         (let ((vm-mku-dyn 'dyn))
           (list (makunbound 'vm-mku-dyn)
                 (condition-case err vm-mku-dyn (error (car err)))
                 (condition-case err (default-value 'vm-mku-dyn) (error (car err)))
                 (boundp 'vm-mku-dyn)))
         vm-mku-dyn
         (default-value 'vm-mku-dyn)",
    );
    assert_eq!(
        results[1],
        "OK (vm-mku-dyn void-variable void-variable nil)"
    );
    assert_eq!(results[2], "OK global");
    assert_eq!(results[3], "OK global");
}

#[test]
fn setq_alias_triggers_single_watcher_callback_on_resolved_target() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(setq vm-setq-watch-events nil)
         (defalias 'vm-setq-watch-rec #'(lambda (symbol newval operation where)
           (setq vm-setq-watch-events
                 (cons (list symbol newval operation where)
                       vm-setq-watch-events))))
         (defvaralias 'vm-setq-watch 'vm-setq-watch-base)
         (add-variable-watcher 'vm-setq-watch-base 'vm-setq-watch-rec)
         (setq vm-setq-watch 9)
         (length vm-setq-watch-events)",
    );
    assert_eq!(results[5], "OK 1");
}

#[test]
fn setq_keyword_self_assignment_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(condition-case err
             (setq :vm-setq-keyword :vm-setq-keyword)
           (error err))",
    );
    assert_eq!(results[0], "OK :vm-setq-keyword");
}

#[test]
fn buffer_local_value_follows_alias_and_keyword_semantics() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(progn
           (defvaralias 'vm-blv-alias 'vm-blv-base)
           (with-temp-buffer
             (set (make-local-variable 'vm-blv-alias) 3)
             (list (buffer-local-value 'vm-blv-alias (current-buffer))
                   (buffer-local-value 'vm-blv-base (current-buffer))
                   (local-variable-p 'vm-blv-alias)
                   (local-variable-p 'vm-blv-base))))
         (progn
           (defvaralias 'vm-blv-alias2 'vm-blv-base2)
           (with-temp-buffer
             (condition-case err
                 (buffer-local-value 'vm-blv-alias2 (current-buffer))
               (error err))))
         (list
           (with-temp-buffer (buffer-local-value nil (current-buffer)))
           (with-temp-buffer (buffer-local-value t (current-buffer)))
           (with-temp-buffer (buffer-local-value :vm-blv-k (current-buffer)))
           (condition-case err
               (with-temp-buffer (buffer-local-value 'vm-blv-miss (current-buffer)))
             (error err))
           (condition-case err
               (with-temp-buffer (buffer-local-value 1 (current-buffer)))
             (error err)))",
    );
    assert_eq!(results[0], "OK (3 3 t t)");
    assert_eq!(results[1], "OK (void-variable vm-blv-alias2)");
    assert_eq!(
        results[2],
        "OK (nil t :vm-blv-k (void-variable vm-blv-miss) (wrong-type-argument symbolp 1))"
    );
}

#[test]
fn buffer_local_value_reads_forwarded_slot_default_when_not_local() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (list (boundp 'line-spacing)
                   (default-value 'line-spacing)
                   (buffer-local-value 'line-spacing (current-buffer))
                   (local-variable-p 'line-spacing (current-buffer))
                   (local-variable-if-set-p 'line-spacing (current-buffer))))
           (progn
             (setq-default line-spacing 2)
             (with-temp-buffer
               (list (default-value 'line-spacing)
                     (buffer-local-value 'line-spacing (current-buffer))
                     (local-variable-p 'line-spacing (current-buffer)))))
           (with-temp-buffer
             (setq-local line-spacing 4)
             (list (default-value 'line-spacing)
                   (buffer-local-value 'line-spacing (current-buffer))
                   (local-variable-p 'line-spacing (current-buffer))))"#,
    );
    assert_eq!(results[0], "OK (t nil nil nil t)");
    assert_eq!(results[1], "OK (2 2 nil)");
    assert_eq!(results[2], "OK (2 4 t)");
}

#[test]
fn local_variable_if_set_p_follows_alias_and_contract_semantics() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(progn
           (defvaralias 'vm-lvis-alias 'vm-lvis-base)
           (make-variable-buffer-local 'vm-lvis-base)
           (list (local-variable-if-set-p 'vm-lvis-alias)
                 (local-variable-if-set-p 'vm-lvis-base)))
         (list
           (condition-case err (local-variable-if-set-p nil) (error err))
           (condition-case err (local-variable-if-set-p t) (error err))
           (condition-case err (local-variable-if-set-p :vm-k) (error err))
           (condition-case err (local-variable-if-set-p 1) (error err))
           (condition-case err (local-variable-if-set-p 'x nil) (error err))
           (condition-case err (local-variable-if-set-p 'x (current-buffer)) (error err))
           (condition-case err (local-variable-if-set-p 'x 1) (error err))
           (condition-case err (local-variable-if-set-p 'x (current-buffer) nil)
             (error err)))
         (local-variable-if-set-p 'fill-column)",
    );
    assert_eq!(results[0], "OK (t t)");
    assert_eq!(
        results[1],
        "OK (nil nil nil (wrong-type-argument symbolp 1) nil nil nil (wrong-number-of-arguments local-variable-if-set-p 3))"
    );
    assert_eq!(results[2], "OK t");
}

#[test]
fn variable_binding_locus_follows_buffer_local_and_alias_semantics() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(let ((locus (condition-case err
                          (progn (with-temp-buffer (set (make-local-variable 'x) 2) (variable-binding-locus 'x)))
                        (error err))))
           (list (condition-case err (variable-binding-locus 'x) (error err))
                 (condition-case err (progn (setq x 1) (variable-binding-locus 'x)) (error err))
                 (bufferp locus)
                 (buffer-live-p locus)
                 (condition-case err (variable-binding-locus nil) (error err))
                 (condition-case err (variable-binding-locus t) (error err))
                 (condition-case err (variable-binding-locus :vm-k) (error err))
                 (condition-case err (variable-binding-locus 1) (error err))
                 (condition-case err (variable-binding-locus 'x nil) (error err))))
         (progn
           (defvaralias 'vm-vbl-alias 'vm-vbl-base)
           (with-temp-buffer
             (set (make-local-variable 'vm-vbl-alias) 9)
             (list (bufferp (variable-binding-locus 'vm-vbl-alias))
                   (buffer-live-p (variable-binding-locus 'vm-vbl-alias))
                   (bufferp (variable-binding-locus 'vm-vbl-base))
                   (buffer-live-p (variable-binding-locus 'vm-vbl-base)))))",
    );
    assert_eq!(
        results[0],
        "OK (nil nil t nil nil nil nil (wrong-type-argument symbolp 1) (wrong-number-of-arguments variable-binding-locus 2))"
    );
    assert_eq!(results[1], "OK (t t t t)");
}

#[test]
fn uninterned_automatic_buffer_local_symbols_follow_gnu_identity_semantics() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(let ((sym (make-symbol "vm-auto-local")))
             (make-variable-buffer-local sym)
             (list (default-boundp sym)
                   (default-value sym)
                   (with-temp-buffer
                     (list (local-variable-p sym)
                           (local-variable-if-set-p sym)
                           (variable-binding-locus sym)
                           (progn
                             (set sym 9)
                             (list (local-variable-p sym)
                                   (local-variable-if-set-p sym)
                                   (eq (variable-binding-locus sym) (current-buffer))
                                   (symbol-value sym)))))
                   (with-temp-buffer
                     (list (local-variable-p sym)
                           (local-variable-if-set-p sym)
                           (boundp sym)))))"#,
    );
    assert_eq!(result, "OK (t nil (nil t nil (t t t 9)) (nil t t))");
}

#[test]
fn value_lt_matches_oracle_type_and_ordering_semantics() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(list
           (value< 1 2)
           (value< 2 1)
           (value< 1 1)
           (value< 'a 'b)
           (value< 'b 'a)
           (value< \"a\" \"b\")
           (condition-case err (value< 1 \"a\") (error err))
           (value< 1.0 2)
           (value< :a :b)
           (value< '(1 2) '(1 3))
           (value< '(1 2) '(1 2 0))
           (value< [1 2] [1 3])
           (condition-case err (value< [1] '(1)) (error err))
           (condition-case err (value< '(1 . 2) '(1 2)) (error err))
           (condition-case err (value< '(1 2) '(1 . 2)) (error err)))",
    );
    assert_eq!(
        results[0],
        "OK (t nil nil t nil t (type-mismatch 1 \"a\") t t t t t (type-mismatch [1] (1)) (type-mismatch 2 (2)) (type-mismatch (2) 2))"
    );
}

#[test]
fn variable_watchers_report_let_and_unlet_runtime_transitions() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(setq vm-watch-events nil)
         (setq vm-watch-target 9)
         (defalias 'vm-watch-rec #'(lambda (sym new op where)
           (setq vm-watch-events (cons (list op new) vm-watch-events))))
         (add-variable-watcher 'vm-watch-target 'vm-watch-rec)
         (let ((vm-watch-target 1)) 'done)
         vm-watch-events
         (setq vm-watch-events nil)
         (let* ((vm-watch-target 2)) 'done)
         vm-watch-events",
    );
    assert_eq!(results[5], "OK ((unlet 9) (let 1))");
    assert_eq!(results[8], "OK ((unlet 9) (let 2))");
}

#[test]
fn special_form_type_payloads_match_oracle_edges() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(condition-case err (setq x) (error err))
         (condition-case err (setq 1 2) (error err))
         (condition-case err (let ((1 2)) nil) (error err))
         (condition-case err (let* ((1 2)) nil) (error err))
         (condition-case err (cond 1) (error err))
         (condition-case err (condition-case 1 2 (error 3)) (error err))
         (condition-case err (condition-case err 2 3) (error err))
         (condition-case err (condition-case err 2 ()) (error err))",
    );
    assert_eq!(results[0], "OK (wrong-number-of-arguments setq 1)");
    assert_eq!(results[1], "OK (wrong-type-argument symbolp 1)");
    assert_eq!(results[2], "OK (wrong-type-argument symbolp 1)");
    assert_eq!(results[3], "OK (wrong-type-argument symbolp 1)");
    assert_eq!(results[4], "OK (wrong-type-argument listp 1)");
    assert_eq!(results[5], "OK (wrong-type-argument symbolp 1)");
    assert_eq!(results[6], "OK (error \"Invalid condition handler: 3\")");
    assert_eq!(results[7], "OK 2");
}

#[test]
fn mapcar_works() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(mapcar #'1+ '(1 2 3))"), "OK (2 3 4)");
}

#[test]
fn mapcar_list_mutation_matches_gnu_prefix_result() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((x (list 1 2 3)))
               (mapcar (lambda (a)
                         (if (= a 1) (setcdr x nil))
                         a)
                       x))"
        ),
        "OK (1)"
    );
    assert_eq!(
        eval_one(
            "(let ((x (list 1 2 3)))
               (mapcar (lambda (a)
                         (if (= a 1) (setcdr x 9))
                         a)
                       x))"
        ),
        "OK (1)"
    );
}

#[test]
fn mapcar_dotted_list_validates_before_callback_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((called nil))
               (condition-case err
                   (mapcar (lambda (x) (setq called t) x) '(1 . 2))
                 (error (list (car err) (nth 1 err) (nth 2 err) called))))"
        ),
        "OK (wrong-type-argument listp 2 nil)"
    );
}

#[test]
fn memory_use_counts_track_heap_allocations() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let* ((before (memory-use-counts))
                    (_cons (cons 1 2))
                    (_string (make-string 3 ?x))
                    (_vector (vector 1 2 3))
                    (after (memory-use-counts)))
               (list (> (nth 0 after) (nth 0 before))
                     (> (nth 2 after) (nth 2 before))
                     (> (nth 4 after) (nth 4 before))
                     (> (nth 6 after) (nth 6 before))))"
        ),
        "OK (t t t t)"
    );
}

#[test]
fn apply_works() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(apply #'+ '(1 2 3))"), "OK 6");
    assert_eq!(eval_one("(apply #'+ 1 2 '(3))"), "OK 6");
    assert_eq!(eval_one("(apply (list #'+ 1 2 3))"), "OK 6");
}

#[test]
fn apply_improper_tail_signals_wrong_type_argument() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(condition-case err
                 (apply 'list '(1 . 2))
               (error (list (car err) (nth 2 err))))"
        ),
        "OK (wrong-type-argument 2)"
    );
}

#[test]
fn apply_lambda_with_dotted_formals_signals_invalid_function() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(condition-case err
                 (apply (lambda (a b . rest) (list a b rest)) '(1 2 3 4))
               (error (car err)))"
        ),
        "OK invalid-function"
    );
    assert_eq!(
        eval_one_lexical(
            "(condition-case err
                 (apply (lambda (a b . rest) (list a b rest)) '(1 2 3 4))
               (error (car err)))"
        ),
        "OK invalid-function"
    );
}

#[test]
fn lambda_forms_validate_non_list_arglists_during_closure_construction() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(condition-case err (eval '(lambda _) nil) (error err))"),
        "OK (wrong-type-argument listp _)"
    );

    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.eval_str(
        r#"
        (fset 'vm-closure-hook
              (lambda (args body env docstring iform)
                (if (listp args)
                    (make-interpreted-closure args body env docstring iform)
                  (signal 'cl-assertion-failed (list '(listp args))))))
        (setq internal-make-interpreted-closure-function 'vm-closure-hook)
        "#,
    )
    .expect("install closure hook");

    let rendered = format_eval_result(&ev.eval_str(
        r#"(list
            (condition-case err (eval '(lambda _) t) (error err))
            (condition-case err (function (lambda _)) (error err)))"#,
    ));
    assert_eq!(
        rendered,
        "OK ((cl-assertion-failed (listp args)) (cl-assertion-failed (listp args)))"
    );
}

#[test]
fn funcall_and_apply_nil_signal_void_function() {
    crate::test_utils::init_test_tracing();
    let funcall_result = eval_one(
        "(condition-case err
             (funcall nil)
           (void-function (car err)))",
    );
    assert_eq!(funcall_result, "OK void-function");

    let apply_result = eval_one(
        "(condition-case err
             (apply nil nil)
           (void-function (car err)))",
    );
    assert_eq!(apply_result, "OK void-function");
}

#[test]
fn funcall_and_apply_non_callable_symbol_edges() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(condition-case err (funcall t) (error (car err)))"),
        "OK void-function"
    );
    assert_eq!(
        eval_one("(condition-case err (funcall :vm-matrix-keyword) (error (car err)))"),
        "OK void-function"
    );
    assert_eq!(
        eval_one("(condition-case err (funcall 'if) (error (car err)))"),
        "OK invalid-function"
    );
    assert_eq!(
        eval_one("(condition-case err (funcall 'if) (error err))"),
        "OK (invalid-function #<subr if>)"
    );
    assert_eq!(
        eval_one("(condition-case err (funcall (symbol-function 'if) t 1 2) (error (car err)))"),
        "OK invalid-function"
    );
    assert_eq!(
        eval_one("(condition-case err (apply t nil) (error (car err)))"),
        "OK void-function"
    );
    assert_eq!(
        eval_one("(condition-case err (apply :vm-matrix-keyword nil) (error (car err)))"),
        "OK void-function"
    );
    assert_eq!(
        eval_one("(condition-case err (apply 'if '(t 1 2)) (error (car err)))"),
        "OK invalid-function"
    );
    assert_eq!(
        eval_one("(condition-case err (apply 'if '(t 1 2)) (error err))"),
        "OK (invalid-function #<subr if>)"
    );
}

#[test]
fn funcall_throw_is_callable_and_preserves_throw_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(catch 'tag (funcall 'throw 'tag 42))"), "OK 42");
    assert_eq!(
        eval_one("(condition-case err (funcall 'throw 'tag 42) (error err))"),
        "OK (no-catch tag 42)"
    );
    assert_eq!(
        eval_one("(condition-case err (funcall 'throw) (error err))"),
        "OK (wrong-number-of-arguments #<subr throw> 0)"
    );
}

#[test]
fn throw_alias_wrong_arity_mentions_surface_symbol() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(progn
               (defalias 'vm-throw-alias 'throw)
               (condition-case err (vm-throw-alias) (error err)))"
        ),
        "OK (wrong-number-of-arguments vm-throw-alias 0)"
    );
}

#[test]
fn funcall_throw_uses_shared_condition_stack_without_catch_tag_mirror() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let tag = Value::symbol("vm-shared-throw");
    ev.push_condition_frame(ConditionFrame::Catch {
        tag,
        resume: ResumeTarget::InterpreterCatch,
    });

    let result = ev.funcall_general(Value::symbol("throw"), vec![tag, Value::fixnum(42)]);
    assert!(matches!(
        result,
        Err(Flow::Throw(ref thrown)) if thrown.tag == tag && thrown.value == Value::fixnum(42)
    ));
    assert_eq!(ev.condition_stack_depth_for_test(), 1);

    ev.pop_condition_frame();
    assert!(ev.top_level_eval_state_is_clean());
}

#[test]
fn funcall_named_symbol_propagates_inner_invalid_function_payload() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(progn
               (fset 'vm-invalid-wrap
                     (lambda ()
                       (funcall '(1 2 3))))
               (unwind-protect
                   (condition-case err
                       (funcall 'vm-invalid-wrap)
                     (invalid-function (nth 1 err)))
                 (fmakunbound 'vm-invalid-wrap)))"
        ),
        "OK (1 2 3)"
    );
}

#[test]
fn unwind_protect_cleanup_signal_overrides_body_result() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(condition-case err
                 (unwind-protect 'ok (car 1))
               (wrong-type-argument (car err)))"
        ),
        "OK wrong-type-argument"
    );
}

#[test]
fn unwind_protect_cleanup_signal_overrides_throw() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(condition-case err
                 (catch 'neomacs--cleanup-tag
                   (unwind-protect
                       (throw 'neomacs--cleanup-tag 'ok)
                     (car 1)))
               (wrong-type-argument (car err)))"
        ),
        "OK wrong-type-argument"
    );
}

#[test]
fn native_unwind_scope_runs_lower_cleanups_after_a_cleanup_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let original_depth = eval.specpdl.len();
    let lower_cleanup = eval
        .eval_str("'((setq native-lower-cleanup-ran t))")
        .expect("parse lower cleanup forms");
    let failing_cleanup = eval
        .eval_str("'((error \"native cleanup failed\"))")
        .expect("parse failing cleanup forms");

    let result = eval.with_unwind_scope(|eval| {
        eval.specpdl.push(SpecBinding::UnwindProtect {
            forms: lower_cleanup,
            lexenv: Value::NIL,
        });
        eval.specpdl.push(SpecBinding::UnwindProtect {
            forms: failing_cleanup,
            lexenv: Value::NIL,
        });
        Ok(Value::NIL)
    });

    assert!(matches!(result, Err(Flow::Signal(_))));
    assert_eq!(
        eval.eval_symbol("native-lower-cleanup-ran")
            .expect("lower cleanup should have run"),
        Value::T
    );
    assert_eq!(eval.specpdl.len(), original_depth);
}

#[test]
fn fmakunbound_masks_builtin_special_and_evaluator_callable_fallbacks() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(fmakunbound 'car)
         (fboundp 'car)
         (symbol-function 'car)
         (condition-case err (car '(1 2)) (void-function 'void-function))
         (fmakunbound 'if)
         (fboundp 'if)
         (symbol-function 'if)
         (condition-case err (if t 1 2) (void-function 'void-function))
         (fmakunbound 'throw)
         (fboundp 'throw)
         (symbol-function 'throw)
         (condition-case err (throw 'tag 1) (void-function 'void-function))",
    );
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK void-function");
    assert_eq!(results[5], "OK nil");
    assert_eq!(results[6], "OK nil");
    assert_eq!(results[7], "OK void-function");
    assert_eq!(results[9], "OK nil");
    assert_eq!(results[10], "OK nil");
    assert_eq!(results[11], "OK void-function");
}

#[test]
fn fset_can_override_special_form_name_for_direct_calls() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let ((orig (symbol-function 'if)))
           (unwind-protect
               (progn
                 (fset 'if (lambda (&rest _args) 'ov))
                 (if t 1 2))
             (fset 'if orig)))",
    );
    assert_eq!(result, "OK ov");
}

#[test]
fn fset_restoring_subr_object_keeps_callability() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((orig (symbol-function 'car)))
               (fset 'car orig)
               (car '(1 2)))"
        ),
        "OK 1"
    );

    assert_eq!(
        eval_one(
            "(let ((orig (symbol-function 'if)))
               (fset 'if orig)
               (if t 1 2))"
        ),
        "OK 1"
    );
}

#[test]
fn canonical_subr_survives_rebinding_and_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let sym_id = intern("car");
    let original = Value::subr(sym_id);

    crate::emacs_core::builtins::builtin_fset(
        &mut ev,
        vec![Value::symbol("car"), Value::fixnum(1)],
    )
    .expect("rebind public function cell");

    ev.gc_collect_exact();

    let after = Value::subr(sym_id);
    assert_eq!(after.bits(), original.bits());

    crate::emacs_core::builtins::builtin_fset(&mut ev, vec![Value::symbol("car"), original])
        .expect("restore original subr");
}

#[test]
fn dispatch_subr_id_uses_name_identity_not_symbol_slot_identity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let namesake = crate::emacs_core::intern::intern_uninterned("car");
    let args = vec![Value::list(vec![Value::fixnum(1), Value::fixnum(2)])];
    let result = ev
        .dispatch_subr_id(namesake, args)
        .expect("canonical subr should be found by shared name atom")
        .expect("subr call should succeed");
    assert_eq!(result, Value::fixnum(1));
}

#[test]
fn funcall_subr_object_ignores_symbol_function_rebinding() {
    crate::test_utils::init_test_tracing();
    // GNU Emacs tree-walking evaluator respects fset: after (fset 'car shadow),
    // calling (car ...) uses the shadow function. Only the bytecode VM
    // bypasses via dedicated opcodes (Bcar).
    // funcall with the original subr object still uses the original.
    assert_eq!(
        eval_one(
            "(let ((orig (symbol-function 'car))
                   (snap (symbol-function 'car)))
               (unwind-protect
                   (progn
                     (fset 'car (lambda (&rest _) 'shadow))
                     (list (funcall snap '(1 2)) (car '(1 2))))
                 (fset 'car orig)))"
        ),
        "OK (1 shadow)"
    );
}

#[test]
fn funcall_autoload_object_signals_wrong_type_argument_symbolp() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(condition-case err
                 (funcall '(autoload \"x\" nil nil nil) 3)
               (wrong-type-argument
                (list (car err)
                      (nth 1 err)
                      (and (consp (nth 2 err))
                           (eq (car (nth 2 err)) 'autoload)))))"
        ),
        "OK (wrong-type-argument symbolp t)"
    );
}

#[test]
fn apply_autoload_object_signals_wrong_type_argument_symbolp() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(condition-case err
                 (apply '(autoload \"x\" nil nil nil) '(3))
               (wrong-type-argument
                (list (car err)
                      (nth 1 err)
                      (and (consp (nth 2 err))
                           (eq (car (nth 2 err)) 'autoload)))))"
        ),
        "OK (wrong-type-argument symbolp t)"
    );
}

#[test]
fn fset_nil_reports_symbol_payload_for_void_function_calls() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(fset 'vm-fsetnil nil)
         (fboundp 'vm-fsetnil)
         (condition-case err (vm-fsetnil) (error err))
         (condition-case err (funcall 'vm-fsetnil) (error err))
         (condition-case err (apply 'vm-fsetnil nil) (error err))
         (fset 'length nil)
         (fboundp 'length)
         (condition-case err (length '(1 2)) (error err))",
    );

    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK (void-function vm-fsetnil)");
    assert_eq!(results[3], "OK (void-function vm-fsetnil)");
    assert_eq!(results[4], "OK (void-function vm-fsetnil)");
    assert_eq!(results[5], "OK nil");
    assert_eq!(results[6], "OK nil");
    assert_eq!(results[7], "OK (void-function length)");
}

#[test]
fn fset_noncallable_reports_symbol_payload_for_invalid_function_calls() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(fset 'vm-fsetint 1)
         (fboundp 'vm-fsetint)
         (symbol-function 'vm-fsetint)
         (condition-case err (vm-fsetint) (error err))
         (condition-case err (funcall 'vm-fsetint) (error err))
         (condition-case err (apply 'vm-fsetint nil) (error err))",
    );

    assert_eq!(results[0], "OK 1");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK 1");
    assert_eq!(results[3], "OK (invalid-function vm-fsetint)");
    assert_eq!(results[4], "OK (invalid-function vm-fsetint)");
    assert_eq!(results[5], "OK (invalid-function vm-fsetint)");
}

#[test]
fn fset_t_function_cell_controls_funcall_and_apply_behavior() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((orig (symbol-function 't)))
               (unwind-protect
                   (progn
                     (fset 't 'car)
                     (funcall t '(1 2)))
                 (fset 't orig)))"
        ),
        "OK 1"
    );

    assert_eq!(
        eval_one(
            "(let ((orig (symbol-function 't)))
               (unwind-protect
                   (progn
                     (fset 't 1)
                     (condition-case err (funcall t) (error err)))
                 (fset 't orig)))"
        ),
        "OK (invalid-function t)"
    );
}

#[test]
fn fset_keyword_function_cell_controls_funcall_and_apply_behavior() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((orig (symbol-function :k)))
               (unwind-protect
                   (progn
                     (fset :k 'car)
                     (funcall :k '(1 2)))
                 (fset :k orig)))"
        ),
        "OK 1"
    );

    assert_eq!(
        eval_one(
            "(let ((orig (symbol-function :k)))
               (unwind-protect
                   (progn
                     (fset :k 'car)
                     (apply :k '((1 2))))
                 (fset :k orig)))"
        ),
        "OK 1"
    );

    assert_eq!(
        eval_one(
            "(let ((orig (symbol-function :k)))
               (unwind-protect
                   (progn
                     (fset :k 1)
                     (condition-case err (funcall :k) (error err)))
                 (fset :k orig)))"
        ),
        "OK (invalid-function :k)"
    );
}

#[test]
fn fset_uninterned_symbol_function_cell_controls_funcall_and_apply_behavior() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((fun (make-symbol "vm-uninterned-funcall")))
                 (fset fun (lambda (x) (+ x 1)))
                 (list (functionp fun)
                       (funcall fun 41)
                       (apply fun '(41))))"#
        ),
        "OK (t 42 42)"
    );
}

#[test]
fn named_call_cache_invalidates_on_function_cell_mutation() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(condition-case err
             (funcall 'vm-cache-target)
           (error (car err)))
         (fset 'vm-cache-target (lambda () 9))
         (funcall 'vm-cache-target)
         (fset 'vm-cache-target (lambda () 11))
         (funcall 'vm-cache-target)",
    );
    assert_eq!(results[0], "OK void-function");
    assert_eq!(results[2], "OK 9");
    assert_eq!(results[4], "OK 11");
}

#[test]
fn compiler_function_overrides_cache_tracks_dynamic_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (fset 'neomacs--override-target (lambda () 'base))
                 (list (neomacs--override-target)
                       (let ((internal--compiler-function-overrides
                              (list (cons 'neomacs--override-target
                                          (lambda () 'override)))))
                         (list (neomacs--override-target)
                               (funcall 'neomacs--override-target)))
                       (neomacs--override-target)))"#
        ),
        "OK (base (override override) base)"
    );
}

#[test]
fn compiler_function_overrides_cache_tracks_default_assignment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (fset 'neomacs--default-override-target (lambda () 'base))
                 (set-default-toplevel-value
                  'internal--compiler-function-overrides
                  (list (cons 'neomacs--default-override-target
                              (lambda () 'override))))
                 (let ((during (list (neomacs--default-override-target)
                                     (funcall 'neomacs--default-override-target))))
                   (set-default-toplevel-value
                    'internal--compiler-function-overrides nil)
                   (list during (neomacs--default-override-target))))"#
        ),
        "OK ((override override) base)"
    );
}

#[test]
fn set_default_toplevel_alias_triggers_variable_watchers_twice() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(setq vm-set-default-top-watch-events nil)
           (fset 'vm-set-default-top-watch-rec
                 (lambda (symbol newval operation where)
                   (setq vm-set-default-top-watch-events
                         (cons (list symbol newval operation where)
                               vm-set-default-top-watch-events))))
           (defvaralias 'vm-set-default-top-watch 'vm-set-default-top-base)
           (add-variable-watcher 'vm-set-default-top-base 'vm-set-default-top-watch-rec)
           (set-default-toplevel-value 'vm-set-default-top-watch 7)
           (length vm-set-default-top-watch-events)"#,
    );
    assert_eq!(results[5], "OK 2");
}

#[test]
fn set_default_toplevel_updates_only_buffers_without_local_override() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let current = ctx.buffers.current_buffer_id().expect("current buffer");
    let inherited = ctx.buffers.create_buffer("*inherited*");
    let local = ctx.buffers.create_buffer("*local*");
    let local_value = Value::list(vec![Value::string("LOCAL")]);
    ctx.set_buffer_local_binding_by_id(
        local,
        crate::emacs_core::intern::intern("mode-line-format"),
        local_value,
    )
    .expect("local mode-line-format");
    let new_default = Value::list(vec![Value::string("DEFAULT")]);

    set_default_toplevel_value(
        &mut ctx,
        vec![Value::symbol("mode-line-format"), new_default],
    )
    .expect("set-default-toplevel-value");

    assert_eq!(
        super::super::data::default_value(&mut ctx, vec![Value::symbol("mode-line-format")],)
            .expect("default-value"),
        new_default
    );
    for buffer_id in [current, inherited] {
        assert_eq!(
            ctx.buffers
                .get(buffer_id)
                .and_then(|buffer| buffer.buffer_local_value("mode-line-format")),
            Some(new_default)
        );
    }
    assert_eq!(
        ctx.buffers
            .get(local)
            .and_then(|buffer| buffer.buffer_local_value("mode-line-format")),
        Some(local_value)
    );
}

#[test]
fn default_toplevel_alias_scan_uses_exact_symbol_identity_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (defvar vm-toplevel-alias-base 1)
                 (defvaralias 'vm-toplevel-alias 'vm-toplevel-alias-base)
                 (list
                  (let ((vm-toplevel-alias 2))
                    (default-toplevel-value 'vm-toplevel-alias))
                  (let ((vm-toplevel-alias 2))
                    (set-default-toplevel-value 'vm-toplevel-alias 9)
                    vm-toplevel-alias)
                  vm-toplevel-alias-base))"#
        ),
        "OK (2 9 1)"
    );
}

#[test]
fn set_default_toplevel_value_does_not_notify_when_updating_saved_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-set-default-toplevel-events 0)
                 (defvar vm-set-default-toplevel-target 1)
                 (fset 'vm-set-default-toplevel-watcher
                       (lambda (_symbol _new-value operation _where)
                         (if (eq operation 'set)
                             (setq vm-set-default-toplevel-events
                                   (1+ vm-set-default-toplevel-events)))))
                 (add-variable-watcher 'vm-set-default-toplevel-target
                                       'vm-set-default-toplevel-watcher)
                 (list
                  (let ((vm-set-default-toplevel-target 2))
                    (set-default-toplevel-value
                     'vm-set-default-toplevel-target 3)
                    vm-set-default-toplevel-events)
                  vm-set-default-toplevel-target
                  vm-set-default-toplevel-events))"#
        ),
        "OK (0 3 0)"
    );
}

#[test]
fn set_default_toplevel_saved_buffer_default_is_published_on_unwind() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let symbol = intern("truncate-lines");
    let specpdl_depth = ctx.specpdl.len();

    ctx.try_specbind(symbol, Value::T)
        .expect("bind inherited truncate-lines default");
    let after_bind = ctx.redisplay_generation();

    let restored_default = Value::symbol("vm-restored-truncate-lines");
    set_default_toplevel_value(&mut ctx, vec![Value::from_sym_id(symbol), restored_default])
        .expect("update saved toplevel default");
    assert_eq!(
        ctx.redisplay_generation(),
        after_bind,
        "changing only the saved binding must not invalidate visible state"
    );

    ctx.unbind_to(specpdl_depth);
    assert!(
        ctx.redisplay_generation() > after_bind,
        "restoring the saved display default must invalidate redisplay"
    );
    assert_eq!(
        super::super::data::default_value(&mut ctx, vec![Value::from_sym_id(symbol)],)
            .expect("restored default-value"),
        restored_default
    );
}

#[test]
fn inherited_buffer_default_let_reports_set_watch_operations_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-buffer-default-let-events nil)
                 (fset 'vm-buffer-default-let-watcher
                       (lambda (_symbol new-value operation where)
                         (setq vm-buffer-default-let-events
                               (cons (list new-value operation where)
                                     vm-buffer-default-let-events))))
                 (add-variable-watcher 'truncate-lines
                                       'vm-buffer-default-let-watcher)
                 (let ((truncate-lines t)) nil)
                 (nreverse vm-buffer-default-let-events))"#
        ),
        "OK ((t set nil) (nil set nil))"
    );
}

#[test]
fn local_let_watchers_report_buffer_and_skip_unlet_after_local_is_killed() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-local-let-events nil
                       vm-local-let-target 1)
                 (make-local-variable 'vm-local-let-target)
                 (fset 'vm-local-let-watcher
                       (lambda (_symbol _new-value operation where)
                         (if (memq operation '(let unlet))
                             (setq vm-local-let-events
                                   (cons (list operation (bufferp where))
                                         vm-local-let-events))
                           nil)))
                 (add-variable-watcher 'vm-local-let-target
                                       'vm-local-let-watcher)
                 (let ((vm-local-let-target 2))
                   (kill-local-variable 'vm-local-let-target))
                 (nreverse vm-local-let-events))"#
        ),
        "OK ((let t))"
    );
}

#[test]
fn inherited_buffer_default_let_propagates_watcher_signal_before_body() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-buffer-default-body-ran nil)
                 (fset 'vm-buffer-default-error-watcher
                       (lambda (_symbol _new-value _operation _where)
                         (signal 'error '("boom"))))
                 (add-variable-watcher 'truncate-lines
                                       'vm-buffer-default-error-watcher)
                 (condition-case error
                     (let ((truncate-lines t))
                       (setq vm-buffer-default-body-ran t))
                   (error
                    (list (car error) vm-buffer-default-body-ran))))"#
        ),
        "OK (error nil)"
    );
}

#[test]
fn forwarded_let_watchers_run_before_validation_and_see_requested_value() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-forwarded-let-events nil
                       vm-forwarded-let-body-ran nil
                       vm-forwarded-let-old-undo-limit undo-limit)
                 (fset 'vm-forwarded-let-watcher
                       (lambda (_symbol new-value operation _where)
                         (setq vm-forwarded-let-events
                               (cons
                                (list
                                 (if (equal new-value "x")
                                     'requested
                                   (eq new-value vm-forwarded-let-old-undo-limit))
                                 operation)
                                vm-forwarded-let-events))))
                 (add-variable-watcher 'undo-limit
                                       'vm-forwarded-let-watcher)
                 (condition-case _error
                     (let ((undo-limit "x"))
                       (setq vm-forwarded-let-body-ran t))
                   (error nil))
                 (list vm-forwarded-let-body-ran
                       (nreverse vm-forwarded-let-events)))"#
        ),
        "OK (nil ((requested let) (t set) (t unlet)))"
    );

    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-bool-let-events nil)
                 (fset 'vm-bool-let-watcher
                       (lambda (_symbol new-value operation _where)
                         (setq vm-bool-let-events
                               (cons (list new-value operation)
                                     vm-bool-let-events))))
                 (add-variable-watcher 'inhibit-redisplay
                                       'vm-bool-let-watcher)
                 (let ((inhibit-redisplay 9))
                   inhibit-redisplay)
                 (nreverse vm-bool-let-events))"#
        ),
        "OK ((9 let) (nil set) (nil unlet))"
    );
}

#[test]
fn macro_scope_propagates_lexical_binding_watcher_signal_before_body() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-macro-bind-body-ran nil)
                 (fset 'vm-macro-bind-error-watcher
                       (lambda (_symbol _new-value operation _where)
                         (if (eq operation 'let)
                             (signal 'error '("macro bind"))
                           nil)))
                 (add-variable-watcher 'lexical-binding
                                       'vm-macro-bind-error-watcher)
                 (fset 'vm-macro-bind-error
                       (cons 'macro
                             (lambda ()
                               (setq vm-macro-bind-body-ran t)
                               nil)))
                 (condition-case error
                     (vm-macro-bind-error)
                   (error
                    (list (car error) vm-macro-bind-body-ran))))"#
        ),
        "OK (error nil)"
    );
}

#[test]
fn inherited_buffer_default_let_propagates_restore_watcher_signal_and_finishes_unwind() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-buffer-default-outer 0
                       vm-buffer-default-body-ran nil
                       vm-buffer-default-restore-fired nil)
                 (fset 'vm-buffer-default-restore-error-watcher
                       (lambda (_symbol new-value _operation _where)
                         (if (and (null new-value)
                                  (null vm-buffer-default-restore-fired))
                             (progn
                               (setq vm-buffer-default-restore-fired t)
                               (signal 'error '("restore")))
                           nil)))
                 (add-variable-watcher
                  'truncate-lines
                  'vm-buffer-default-restore-error-watcher)
                 (condition-case error
                     (let ((vm-buffer-default-outer 1)
                           (truncate-lines t))
                       (setq vm-buffer-default-body-ran t)
                       'body)
                   (error
                    (list (car error)
                          vm-buffer-default-body-ran
                          vm-buffer-default-outer))))"#
        ),
        "OK (error t 0)"
    );
}

#[test]
fn lower_restore_watcher_signal_supersedes_inner_signal_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (defvar vm-unwind-outer 0)
                 (defvar vm-unwind-inner 0)
                 (fset 'vm-unwind-outer-watcher
                       (lambda (_symbol _new-value operation _where)
                         (if (eq operation 'unlet)
                             (signal 'error '("outer"))
                           nil)))
                 (fset 'vm-unwind-inner-watcher
                       (lambda (_symbol _new-value operation _where)
                         (if (eq operation 'unlet)
                             (signal 'error '("inner"))
                           nil)))
                 (add-variable-watcher 'vm-unwind-outer
                                       'vm-unwind-outer-watcher)
                 (add-variable-watcher 'vm-unwind-inner
                                       'vm-unwind-inner-watcher)
                 (condition-case error
                     (let ((vm-unwind-outer 1))
                       (let ((vm-unwind-inner 1))
                         'body))
                   (error error)))"#
        ),
        "OK (error \"outer\")"
    );
}

#[test]
fn lambda_and_condition_case_propagate_dynamic_cleanup_signals() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (defvar vm-lambda-cleanup-var 0)
                 (defvar vm-partial-lambda-cleanup-var 0)
                 (defvar vm-condition-cleanup-var 0)
                 (defvar vm-success-cleanup-var 0)
                 (fset 'vm-result-scope-cleanup-watcher
                       (lambda (_symbol _new-value operation _where)
                         (if (eq operation 'unlet)
                             (signal 'error '("restore"))
                           nil)))
                 (add-variable-watcher 'vm-lambda-cleanup-var
                                       'vm-result-scope-cleanup-watcher)
                 (add-variable-watcher 'vm-partial-lambda-cleanup-var
                                       'vm-result-scope-cleanup-watcher)
                 (add-variable-watcher 'vm-condition-cleanup-var
                                       'vm-result-scope-cleanup-watcher)
                 (add-variable-watcher 'vm-success-cleanup-var
                                       'vm-result-scope-cleanup-watcher)
                 (list
                  (condition-case caught
                      ((lambda (vm-lambda-cleanup-var) 'body) 1)
                    (error caught))
                  (condition-case caught
                      ((lambda (vm-partial-lambda-cleanup-var undo-limit)
                         'body)
                       1 "bad-integer")
                    (error caught))
                  (condition-case caught
                      (condition-case vm-condition-cleanup-var
                          (signal 'error '("body"))
                        (error 'handled))
                    (error caught))
                  (condition-case caught
                      (condition-case vm-success-cleanup-var
                          7
                        (:success 'handled))
                    (error caught))))"#
        ),
        "OK ((error \"restore\") (error \"restore\") (error \"restore\") (error \"restore\"))"
    );
}

#[test]
fn fallible_cleanup_restores_a_pending_quit_flag() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let quoted_error = Value::list(vec![Value::symbol("quote"), Value::symbol("error")]);
    let error_data = Value::list(vec![Value::string("cleanup")]);
    let quoted_data = Value::list(vec![Value::symbol("quote"), error_data]);
    let cleanup_form = Value::list(vec![Value::symbol("signal"), quoted_error, quoted_data]);
    ctx.specpdl.push(SpecBinding::UnwindProtect {
        forms: Value::list(vec![cleanup_form]),
        lexenv: ctx.lexenv,
    });
    ctx.set_quit_flag_value(Value::T);

    assert!(matches!(ctx.unbind_to_result(0), Err(Flow::Signal(_))));
    assert_eq!(ctx.quit_flag_value(), Value::T);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn thread_switch_round_trip_preserves_live_buffer_default_binding() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let symbol = intern("truncate-lines");
    let specpdl_depth = ctx.specpdl.len();

    ctx.try_specbind(symbol, Value::T)
        .expect("bind inherited truncate-lines default");
    assert_eq!(
        super::super::data::default_value_by_id(&ctx, symbol),
        Some(Value::T)
    );

    let thread_state = ctx
        .suspend_dynamic_bindings_for_thread_switch()
        .expect("suspend dynamic bindings");
    assert_eq!(
        super::super::data::default_value_by_id(&ctx, symbol),
        Some(Value::NIL),
        "suspending restores the thread's outer default"
    );

    ctx.resume_dynamic_bindings_for_thread_switch(thread_state)
        .expect("resume dynamic bindings");
    assert_eq!(
        super::super::data::default_value_by_id(&ctx, symbol),
        Some(Value::T),
        "resuming restores the thread's dynamically bound live default"
    );

    ctx.unbind_to(specpdl_depth);
    assert_eq!(
        super::super::data::default_value_by_id(&ctx, symbol),
        Some(Value::NIL)
    );
}

#[test]
fn thread_switch_rechecks_plain_binding_that_became_buffer_local() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let symbol = intern("vm-thread-switch-localized-during-let");
    ctx.obarray_mut()
        .set_symbol_value_id(symbol, Value::fixnum(10));
    let specpdl_depth = ctx.specpdl.len();

    ctx.try_specbind(symbol, Value::fixnum(20))
        .expect("bind plain symbol");
    super::super::custom::builtin_make_variable_buffer_local(
        &mut ctx,
        vec![Value::from_sym_id(symbol)],
    )
    .expect("make dynamically bound symbol buffer-local");
    super::super::builtins::symbols::builtin_set(
        &mut ctx,
        vec![Value::from_sym_id(symbol), Value::fixnum(30)],
    )
    .expect("create current-buffer local value");
    assert_eq!(
        super::super::data::default_value_by_id(&ctx, symbol),
        Some(Value::fixnum(20))
    );

    let thread_state = ctx
        .suspend_dynamic_bindings_for_thread_switch()
        .expect("suspend dynamic bindings");
    assert_eq!(
        super::super::data::default_value_by_id(&ctx, symbol),
        Some(Value::fixnum(10)),
        "suspending restores the outer default"
    );
    assert_eq!(
        ctx.visible_variable_value_or_nil_by_id(symbol),
        Value::fixnum(30),
        "thread switching must leave the current buffer's local value alone"
    );

    ctx.resume_dynamic_bindings_for_thread_switch(thread_state)
        .expect("resume dynamic bindings");
    assert_eq!(
        super::super::data::default_value_by_id(&ctx, symbol),
        Some(Value::fixnum(20))
    );
    assert_eq!(
        ctx.visible_variable_value_or_nil_by_id(symbol),
        Value::fixnum(30)
    );

    ctx.unbind_to(specpdl_depth);
}

#[test]
fn thread_switch_propagates_rejected_saved_default_without_losing_it() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let symbol = intern("undo-limit");
    let specpdl_depth = ctx.specpdl.len();
    let outer_default = super::super::data::default_value_by_id(&ctx, symbol)
        .expect("undo-limit has a forwarded default");

    ctx.try_specbind(symbol, Value::fixnum(5))
        .expect("bind valid undo-limit");
    let rejected = Value::string("not-an-integer");
    set_default_toplevel_value(&mut ctx, vec![Value::from_sym_id(symbol), rejected])
        .expect("updating the saved toplevel binding does not store it yet");

    assert!(matches!(
        ctx.suspend_dynamic_bindings_for_thread_switch(),
        Err(Flow::Signal(_))
    ));
    assert_eq!(
        super::super::data::default_value_by_id(&ctx, symbol),
        Some(Value::fixnum(5)),
        "a rejected thread-switch store must leave the live binding intact"
    );
    assert_eq!(
        default_toplevel_value(&mut ctx, vec![Value::from_sym_id(symbol)])
            .expect("saved binding remains readable"),
        rejected,
        "the failed exchange must not overwrite the requested saved value"
    );

    set_default_toplevel_value(&mut ctx, vec![Value::from_sym_id(symbol), outer_default])
        .expect("restore a valid saved value for cleanup");
    ctx.unbind_to_with_result(specpdl_depth, Ok(Value::NIL))
        .expect("clean up dynamic binding");
}

#[test]
fn failed_thread_switch_rolls_back_inner_binding_exchanges() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let outer = intern("undo-limit");
    let inner = intern("gc-cons-threshold");
    let specpdl_depth = ctx.specpdl.len();
    let outer_default = super::super::data::default_value_by_id(&ctx, outer)
        .expect("undo-limit has a forwarded default");
    let inner_default = super::super::data::default_value_by_id(&ctx, inner)
        .expect("gc-cons-threshold has a forwarded default");

    ctx.try_specbind(outer, Value::fixnum(5))
        .expect("bind valid outer forwarder");
    let rejected = Value::string("not-an-integer");
    set_default_toplevel_value(&mut ctx, vec![Value::from_sym_id(outer), rejected])
        .expect("install invalid saved outer value");
    let live_inner = Value::fixnum(1_234_567);
    ctx.try_specbind(inner, live_inner)
        .expect("bind valid inner forwarder");

    assert!(matches!(
        ctx.suspend_dynamic_bindings_for_thread_switch(),
        Err(Flow::Signal(_))
    ));
    assert_eq!(
        super::super::data::default_value_by_id(&ctx, inner),
        Some(live_inner),
        "the successfully exchanged inner binding must be rolled back"
    );
    assert_eq!(
        default_toplevel_value(&mut ctx, vec![Value::from_sym_id(inner)])
            .expect("saved inner binding remains readable"),
        inner_default,
        "rollback must also restore the inner specpdl cell"
    );

    set_default_toplevel_value(&mut ctx, vec![Value::from_sym_id(outer), outer_default])
        .expect("restore valid outer saved value for cleanup");
    ctx.unbind_to_with_result(specpdl_depth, Ok(Value::NIL))
        .expect("clean up nested dynamic bindings");
}

#[test]
fn funcall_builtin_wrong_arity_uses_subr_object_payload() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(condition-case err (car) (error (subrp (nth 1 err))))"),
        "OK nil"
    );
    assert_eq!(
        eval_one("(condition-case err (funcall 'car) (error (subrp (nth 1 err))))"),
        "OK t"
    );
}

#[test]
fn bytecode_bcall_symbol_function_cell_subr_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ctx = runtime_startup_context();
    let result = ctx.eval_str(
        r#"(progn
             (fset 'vm-bcall-subr-alias (symbol-function 'car))
             (defun vm-bcall-subr-ok (x)
               (vm-bcall-subr-alias x))
             (defun vm-bcall-subr-bad ()
               (vm-bcall-subr-alias))
             (byte-compile 'vm-bcall-subr-ok)
             (byte-compile 'vm-bcall-subr-bad)
             (list (subrp (symbol-function 'vm-bcall-subr-alias))
                   (vm-bcall-subr-ok '(a b))
                   (condition-case err
                       (vm-bcall-subr-bad)
                     (error
                      (list (car err) (subrp (nth 1 err)) (nth 2 err))))))"#,
    );
    assert_eq!(
        format_eval_result(&result),
        "OK (t a (wrong-number-of-arguments t 0))"
    );
}

#[test]
fn condition_case_catches_uncaught_throw_as_no_catch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(condition-case err (throw 'tag 42) (error (car err)))"),
        "OK no-catch"
    );
    // Test uncaught throw from a function call (not just special form).
    // Use a lambda that throws instead of exit-minibuffer (which is Elisp).
    assert_eq!(
        eval_one("(condition-case err (funcall (lambda () (throw 'exit nil))) (error (car err)))"),
        "OK no-catch"
    );
    assert_eq!(
        eval_one("(condition-case err (funcall (lambda () (throw 'exit nil))) (no-catch err))"),
        "OK (no-catch exit nil)"
    );
}

#[test]
fn byte_compiled_condition_case_catches_uncaught_throw_as_no_catch() {
    crate::test_utils::init_test_tracing();
    let mut ctx = runtime_startup_context();
    let result = ctx.eval_str(
        r#"(let ((fn (byte-compile
                     '(lambda ()
                        (condition-case err
                            (throw 'tag 42)
                          (error (car err)))))))
             (funcall fn))"#,
    );
    assert_eq!(format_eval_result(&result), "OK no-catch");
}

#[test]
fn real_timer_event_handler_catches_uncaught_throw_as_timer_error() {
    crate::test_utils::init_test_tracing();
    let mut ctx = runtime_startup_context();
    let result = ctx.eval_str(
        r#"(progn
             (setq timer-list nil
                   timer-idle-list nil)
             (let ((timer (vector nil 0 0 0 nil
                                  (lambda () (throw 'stale-timer-catch nil))
                                  nil nil 0 nil)))
               (setq timer-list (list timer))
               (condition-case err
                   (progn
                     (timer-event-handler timer)
                     'returned)
                 (error (list 'escaped err)))))"#,
    );
    assert_eq!(format_eval_result(&result), "OK returned");
}

#[test]
fn nested_condition_case_uses_current_shared_condition_slice() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(condition-case outer
               (condition-case inner
                   (signal 'error 1)
                 (void-variable 'inner-miss))
             (error (car outer)))"
        ),
        "OK error"
    );
}

#[test]
fn condition_control_symbol_domain_matches_gnu_debug_marker() {
    crate::test_utils::init_test_tracing();
    let debug = Value::symbol(ConditionControlSymbol::Debug.name());
    let error = Value::symbol("error");
    assert_eq!(
        ConditionControlSymbol::from_lisp_value(&debug),
        Some(ConditionControlSymbol::Debug)
    );
    assert_eq!(ConditionControlSymbol::from_lisp_value(&error), None);
    assert!(condition_value_contains_debug(&Value::list(vec![
        debug, error
    ])));
}

#[test]
fn condition_case_suppresses_debugger_without_debug_marker() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((debug-on-error t)
                   (called nil)
                   (debugger (lambda (&rest _args)
                               (setq called 'debugger))))
               (list (condition-case nil
                         (signal 'error 1)
                       (error 'handled))
                     called))"
        ),
        "OK (handled nil)"
    );
}

#[test]
fn active_condition_handler_detection_matches_condition_case_error_clause() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.push_condition_frame(ConditionFrame::ConditionCase {
        conditions: Value::symbol("error"),
        resume: ResumeTarget::InterpreterConditionCase {
            handler_index: 0,
            condition_stack_base: 0,
        },
    });

    let Flow::Signal(sig) = signal("error", vec![Value::string("handled later")]) else {
        panic!("signal should create Flow::Signal");
    };

    assert!(ev.has_active_condition_handler_for_signal(&sig));
}

#[test]
fn condition_case_debug_marker_calls_debugger_before_handler() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((debug-on-error t)
                   (called nil)
                   (debugger (lambda (&rest args)
                               (setq called args))))
               (list (condition-case nil
                         (signal 'error 1)
                       ((debug error) 'handled))
                     called))"
        ),
        "OK (handled (error (error . 1)))"
    );
}

#[test]
fn debug_on_signal_overrides_condition_case_debugger_suppression() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((debug-on-error t)
                   (debug-on-signal t)
                   (called nil)
                   (debugger (lambda (&rest _args)
                               (setq called 'debugger))))
               (list (condition-case nil
                         (signal 'error 1)
                       (error 'handled))
                     called))"
        ),
        "OK (handled debugger)"
    );
}

#[test]
fn debug_ignored_errors_blocks_debugger_even_with_debug_marker() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((debug-on-error t)
                   (debug-ignored-errors '(arith-error))
                   (called nil)
                   (debugger (lambda (&rest _args)
                               (setq called 'debugger))))
               (list (condition-case nil
                         (/ 1 0)
                       ((debug error) 'handled))
                     called))"
        ),
        "OK (handled nil)"
    );
}

#[test]
fn backward_compat_core_forms() {
    crate::test_utils::init_test_tracing();
    // Same tests as original elisp.rs
    let source = r#"
    (+ 1 2)
    (let ((x 1)) (setq x (+ x 2)) x)
    (let ((lst '(1 2))) (setcar lst 9) lst)
    (catch 'tag (throw 'tag 42))
    (condition-case e (/ 1 0) (arith-error 'div-zero))
    (let ((x 1))
      (let ((f (lambda () x)))
        (let ((x 2))
          (funcall f))))
    "#;

    let mut ev = Context::new();
    let rendered: Vec<String> = ev
        .eval_str_each(source)
        .iter()
        .map(format_eval_result)
        .collect();

    assert_eq!(
        rendered,
        vec!["OK 3", "OK 3", "OK (9 2)", "OK 42", "OK div-zero", "OK 2"]
    );
}

#[test]
fn excessive_recursion_detected() {
    crate::test_utils::init_test_tracing();
    let results = eval_all("(defalias 'inf #'(lambda () (inf)))\n(inf)");
    // Second form should trigger excessive nesting
    assert!(results[1].contains("excessive-lisp-nesting"));
}

#[test]
fn excessive_recursion_reports_overflow_depth_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let results = eval_all("(defalias 'inf #'(lambda () (inf)))\n(inf)");
    assert_eq!(results[1], "ERR (excessive-lisp-nesting (1601))");
}

#[test]
fn max_lisp_eval_depth_binding_updates_overflow_limit() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(let ((max-lisp-eval-depth 100)) (defalias 'inf #'(lambda () (inf))) (inf))"),
        "ERR (excessive-lisp-nesting (101))"
    );
}

#[test]
fn lambda_can_call_symbol_function_subr_as_first_class_value() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("((lambda (orig x y) (funcall orig (+ x 1) y)) (symbol-function '+) 4 7)"),
        "OK 12"
    );
    assert_eq!(
        eval_one(
            "(apply (lambda (orig x y) (funcall orig (+ x 1) y)) (symbol-function '+) '(4 7))"
        ),
        "OK 12"
    );
}

#[test]
fn non_lambda_cons_function_position_is_not_evaluated() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("((symbol-function '+) 4 7)"),
        "ERR (invalid-function ((symbol-function '+)))"
    );
}

#[test]
fn quoted_closure_list_is_not_callable() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(funcall '(closure nil (x) (+ x 1)) 2)"),
        "ERR (invalid-function ((closure nil (x) (+ x 1))))"
    );
}

#[test]
fn interpreted_closure_is_not_a_cons_cell() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((closure
                    (make-interpreted-closure
                     nil '((quote ok)) nil))
                   (quoted-lambda '(lambda () (quote ok))))
               (list
                (car-safe closure)
                (cdr-safe closure)
                (condition-case error-data
                    (car closure)
                  (error (car error-data)))
                (condition-case error-data
                    (cdr closure)
                  (error (car error-data)))
                (condition-case error-data
                    (nthcdr 1 closure)
                  (error (car error-data)))
                (car-safe quoted-lambda)
                (consp (cdr-safe quoted-lambda))))"
        ),
        "OK (nil nil wrong-type-argument wrong-type-argument wrong-type-argument lambda t)"
    );
}

#[test]
fn lexical_binding_closure() {
    crate::test_utils::init_test_tracing();
    // With lexical binding, closures capture the lexical environment
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let result = format_eval_result(&ev.eval_str(
        r#"
        (let ((x 1))
          (let ((f (lambda () x)))
            (let ((x 2))
              (funcall f))))
    "#,
    ));
    // In lexical binding, the closure captures x=1, not x=2
    assert_eq!(result, "OK 1");
}

#[test]
fn dynamic_binding_closure() {
    crate::test_utils::init_test_tracing();
    // Without lexical binding (default), closures see dynamic scope
    let mut ev = Context::new();
    let result = format_eval_result(&ev.eval_str(
        r#"
        (let ((x 1))
          (let ((f (lambda () x)))
            (let ((x 2))
              (funcall f))))
    "#,
    ));
    // In dynamic binding, the lambda sees x=2 (innermost dynamic binding)
    assert_eq!(result, "OK 2");
}

#[test]
fn dynamic_closure_clears_callers_lexical_environment() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);

    let result = format_eval_result(&ev.eval_str(
        r#"
        (let ((captured 'outer))
          (let ((f (make-interpreted-closure
                    '(x)
                    '((let ((captured x))
                        (lambda (y) (+ captured y))))
                    nil)))
            (let ((g (funcall f 10)))
              (funcall g 5))))
    "#,
    ));

    assert_eq!(result, "ERR (void-variable (captured))");
}

#[test]
fn lexical_binding_special_var_stays_dynamic() {
    crate::test_utils::init_test_tracing();
    // defvar makes a variable special — it stays dynamically scoped
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let results: Vec<String> = ev
        .eval_str_each(
            r#"
        (defvar my-special 10)
        (let ((my-special 20))
          (let ((f (lambda () my-special)))
            (let ((my-special 30))
              (funcall f))))
    "#,
        )
        .iter()
        .map(format_eval_result)
        .collect();
    // my-special is declared special, so even in lexical mode it's dynamic
    assert_eq!(results[1], "OK 30");
}

#[test]
fn defalias_works() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defalias 'my-add #'(lambda (a b) (+ a b)))
         (defalias 'my-plus 'my-add)
         (my-plus 3 4)",
    );
    assert_eq!(results[2], "OK 7");
}

#[test]
fn defalias_rejects_self_alias_cycle() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(condition-case err
             (defalias 'vm-da-self 'vm-da-self)
           (error err))",
    );
    assert_eq!(result, "OK (cyclic-function-indirection vm-da-self)");
}

#[test]
fn defalias_rejects_two_node_alias_cycle() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defalias 'vm-da-a 'vm-da-b)
         (condition-case err
             (defalias 'vm-da-b 'vm-da-a)
           (error err))",
    );
    assert_eq!(results[0], "OK vm-da-a");
    assert_eq!(results[1], "OK (cyclic-function-indirection vm-da-b)");
}

#[test]
fn defalias_nil_signals_setting_constant() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(condition-case err
             (defalias nil 'car)
           (error err))",
    );
    assert_eq!(result, "OK (setting-constant nil)");
}

#[test]
fn defalias_t_accepts_symbol_cell_updates() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defalias t 'car)
         (symbol-function t)",
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK car");
}

#[test]
fn defalias_enforces_argument_count() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(condition-case err (defalias) (error err))
         (condition-case err (defalias 'vm-da-too-few) (error err))
         (condition-case err (defalias 'vm-da-too-many 'car \"doc\" t) (error err))",
    );
    assert_eq!(results[0], "OK (wrong-number-of-arguments defalias 0)");
    assert_eq!(results[1], "OK (wrong-number-of-arguments defalias 1)");
    assert_eq!(results[2], "OK (wrong-number-of-arguments defalias 4)");
}

#[test]
fn defalias_honors_defalias_fset_function_hook() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(setq vm-da-hook-log nil)
         (put 'vm-da-hooked 'defalias-fset-function
              (lambda (sym def)
                (setq vm-da-hook-log (list sym def))
                (fset sym def)))
         (defalias 'vm-da-hooked 'car)
         vm-da-hook-log
         (symbol-function 'vm-da-hooked)",
    );
    assert_eq!(results[2], "OK vm-da-hooked");
    assert_eq!(results[3], "OK (vm-da-hooked car)");
    assert_eq!(results[4], "OK car");
}

#[test]
fn defalias_stores_function_documentation_property() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defalias 'vm-da-doc (lambda () 'ok) \"vm doc\")
         (get 'vm-da-doc 'function-documentation)",
    );
    assert_eq!(results[0], "OK vm-da-doc");
    assert_eq!(results[1], "OK \"vm doc\"");
}

#[test]
fn fset_inside_lambda_uses_argument_definition() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "((lambda (sym def)
                (fset sym def)
                (list sym def (symbol-function sym)))
              'vm-eval-hook-lambda
              'car)"
        ),
        "OK (vm-eval-hook-lambda car car)"
    );
}

#[test]
fn compiled_literal_reader_form_is_callable_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU emacs 31.0.50 verified: a bytecode object printed as the
    // reader literal `#[ARGS BYTECODE CONSTANTS DEPTH ...]` *is*
    // executable when funcall'd; the reader does the equivalent of
    // `make-byte-code` on it. Mirror that here.
    let result = eval_one(
        "(condition-case err
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\207\\\" [42] 1]\")))
           (error (car err)))",
    );
    assert_eq!(result, "OK 42");
}

#[test]
fn byte_code_object_function_position_is_callable_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU eval_sub sends non-symbol function positions through `function`.
    // That quotes byte-code objects through to normal function dispatch, so a
    // form whose car is a byte-code object is callable.
    let result = eval_one(
        "(eval (list (car (read-from-string \"#[nil \\\"\\\\300\\\\207\\\" [42] 1]\"))) t)",
    );
    assert_eq!(result, "OK 42");
}

#[test]
fn byte_code_function_prints_readable_gnu_literal() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let* ((fn (make-byte-code nil "\300\207" [42] 1))
                  (printed (prin1-to-string fn))
                  (read-back (car (read-from-string printed))))
             (list (substring printed 0 2)
                   (byte-code-function-p read-back)
                   (funcall read-back)))"#,
    );
    assert_eq!(result, "OK (\"#[\" t 42)");
}

#[test]
fn provide_require() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results: Vec<String> = ev
        .eval_str_each("(provide 'my-feature) (featurep 'my-feature)")
        .iter()
        .map(format_eval_result)
        .collect();
    assert_eq!(results[0], "OK my-feature");
    assert_eq!(results[1], "OK t");
}

#[test]
fn provide_preserves_uninterned_feature_identity() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((s (make-symbol "neo-x"))
                 (same-name (make-symbol "neo-x")))
             (provide s)
             (list (featurep s)
                   (and (memq s features) t)
                   (featurep same-name)
                   (memq same-name features)))"#,
    );
    assert_eq!(result, "OK (t t nil nil)");
}

#[test]
fn provide_cons_preserves_features_tail_identity() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((s (make-symbol "neo-x"))
                 (before features))
             (provide s)
             (list (eq (car features) s)
                   (eq (cdr features) before)
                   (featurep s)))"#,
    );
    assert_eq!(result, "OK (t t t)");
}

#[test]
fn provide_stores_subfeatures_list() {
    crate::test_utils::init_test_tracing();
    // GNU provide stores the SUBFEATURES list via (put FEATURE 'subfeatures LIST).
    // featurep with a subfeature arg checks membership in that list.
    let results = eval_all(
        r#"(provide 'test-sf-feat '(sub-a sub-b))
           (featurep 'test-sf-feat)
           (featurep 'test-sf-feat 'sub-a)
           (featurep 'test-sf-feat 'sub-b)
           (featurep 'test-sf-feat 'sub-c)
           (get 'test-sf-feat 'subfeatures)"#,
    );
    assert_eq!(results[0], "OK test-sf-feat");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK t", "sub-a should be in subfeatures");
    assert_eq!(results[3], "OK t", "sub-b should be in subfeatures");
    assert_eq!(results[4], "OK nil", "sub-c should NOT be in subfeatures");
    assert_eq!(results[5], "OK (sub-a sub-b)");
}

#[test]
fn provide_nil_subfeatures_preserves_existing_subfeatures() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(provide 'test-sf-nil '(sub-a sub-b))
           (provide 'test-sf-nil nil)
           (featurep 'test-sf-nil 'sub-a)
           (get 'test-sf-nil 'subfeatures)
           (condition-case err
               (provide 'test-sf-nil 1)
             (error (car err)))"#,
    );
    assert_eq!(results[0], "OK test-sf-nil");
    assert_eq!(results[1], "OK test-sf-nil");
    assert_eq!(results[2], "OK t");
    assert_eq!(results[3], "OK (sub-a sub-b)");
    assert_eq!(results[4], "OK wrong-type-argument");
}

#[test]
fn provide_runs_after_load_alist_callbacks() {
    crate::test_utils::init_test_tracing();
    // GNU Fprovide runs (mapc #'funcall (cdr (assq feature after-load-alist)))
    // after adding the feature to the features list.
    let results = eval_all(
        r#"(defvar test-eal-log nil)
           ;; Set up after-load-alist with a callback for the feature.
           ;; Each entry is (FEATURE-OR-REGEXP callback1 callback2 ...)
           (setq after-load-alist
                 (list (list 'test-eal-feat
                             (lambda () (setq test-eal-log
                                              (cons 'fired-1 test-eal-log)))
                             (lambda () (setq test-eal-log
                                              (cons 'fired-2 test-eal-log))))))
           ;; Provide should trigger the callbacks
           (provide 'test-eal-feat)
           test-eal-log"#,
    );
    // Both callbacks should have fired (in order: fired-1 pushed, then fired-2)
    assert_eq!(results[3], "OK (fired-2 fired-1)");
}

#[test]
fn provide_does_not_refire_after_load_callbacks_on_redundant_provide() {
    crate::test_utils::init_test_tracing();
    // When provide is called again for an already-provided feature,
    // the after-load-alist callbacks should still fire (GNU behavior:
    // Fprovide always runs the hooks regardless of whether the feature
    // was already present).
    let results = eval_all(
        r#"(defvar test-eal-count 0)
           (setq after-load-alist
                 (list (list 'test-refire-feat
                             (lambda () (setq test-eal-count
                                              (1+ test-eal-count))))))
           (provide 'test-refire-feat)
           test-eal-count
           (provide 'test-refire-feat)
           test-eal-count"#,
    );
    assert_eq!(results[3], "OK 1", "first provide should fire callback");
    assert_eq!(
        results[5], "OK 2",
        "second provide should also fire callback"
    );
}

#[test]
fn default_directory_is_bound_to_directory_path() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(stringp default-directory)
         (file-directory-p default-directory)
         (let ((len (length default-directory)))
           (and (> len 0)
                (eq (aref default-directory (1- len)) ?/)))",
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK t");
}

#[test]
fn unread_command_events_is_bound_to_nil_at_startup() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "unread-command-events
         (boundp 'unread-command-events)
         (let ((unread-command-events '(97))) unread-command-events)
         unread-command-events",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK (97)");
    assert_eq!(results[3], "OK nil");
}

#[test]
fn completion_in_region_mode_map_has_gnu_navigation_bindings_after_startup() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(mapcar
             (lambda (key)
               (lookup-key completion-in-region-mode-map (kbd key)))
             '("M-?" "TAB" "M-<up>" "M-<down>" "M-RET"))"#,
    );

    assert_eq!(
        result,
        "OK (completion-help-at-point completion-at-point minibuffer-previous-completion minibuffer-next-completion minibuffer-choose-completion)"
    );
}

#[test]
fn completion_list_mode_map_has_gnu_navigation_bindings_after_startup() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(list
             (eq (keymap-parent completion-list-mode-map) special-mode-map)
             (mapcar
              (lambda (key)
                (lookup-key completion-list-mode-map (kbd key)))
              '("RET" "<up>" "<down>" "TAB" "<backtab>"
                "M-<up>" "M-<down>" "M-RET" "z" "n" "p" "M-g M-c")))"#,
    );

    assert_eq!(
        result,
        "OK (t (choose-completion previous-line-completion next-line-completion next-completion previous-completion minibuffer-previous-completion minibuffer-next-completion minibuffer-choose-completion kill-current-buffer next-completion previous-completion switch-to-minibuffer))"
    );
}

#[test]
fn emacs_copyright_is_bound_at_startup() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "emacs-copyright
         (boundp 'emacs-copyright)
         (string-match \"Copyright (C) [0-9]+ Free Software Foundation\" emacs-copyright nil t)",
    );
    assert_eq!(
        results[0],
        "OK \"Copyright (C) 2026 Free Software Foundation, Inc.\""
    );
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK 0");
}

/// GNU's `Fsnarf_documentation` diagonal, asked of the whole obarray rather
/// than of a list of names.
///
/// GNU installs a `variable-documentation` from exactly three places, and
/// every one of them is downstream of the variable actually existing:
///
/// - `src/doc.c:606-613`, `Fsnarf_documentation`, where the `Fput` is the
///   *entire* branch under `(!NILP (Fboundp (sym)) || !NILP (Fmemq (sym,
///   delayed_init))) && strncmp (end, "\nSKIP", 5)`.  An unbound name's doc is
///   not recorded differently; it is not recorded at all.
/// - `src/eval.c:911`, `Finternal__define_uninitialized_variable` -- the
///   callee of Lisp `defvar`/`defconst`/`defcustom` -- and only `if (!NILP
///   (doc))`.
/// - `src/eval.c:723`/`:741`, `Fdefvaralias` and
///   `Finternal-delete-indirect-variable`.
///
/// There is no fourth, and in particular **there is no pre-seeding**: nothing
/// in GNU writes a `variable-documentation` for a name before that name has a
/// value.  `src/doc.c:433-434` makes the point twice over by reserving the
/// fixnum `0` as "no doc" -- `if (BASE_EQ (tem, make_fixnum (0))) tem = Qnil;`
/// -- a value `make-docfile` never emits, since the smallest real offset is
/// `end + 1 - buf`.
///
/// So the diagonal is a property of the image, not of any table, and that is
/// why this test is a `mapatoms` rather than a list of names: ledger 173's law
/// is that a predicate over rows that exist cannot see a row that was never
/// written, and a per-name pin over a doc table reports green the moment the
/// table is empty.  A `mapatoms` over 17k symbols has no empty state.
///
/// Measured under GNU Emacs 31.0.90 `-Q --batch`: 18815 symbols, 2747 with a
/// `variable-documentation`, **zero** unbound-yet-documented and **zero**
/// holding the reserved `0`.
///
/// Ledger 178.  Before it, this port answered `(35 35 70)` in the shipped
/// image and roughly nineteen hundred in a bare `Context`, because
/// `eval.rs` pre-seeded a `variable-documentation` for all 1972 names of the
/// `STARTUP_VARIABLE_DOC_*` tables -- 70 of them with GNU's own "no doc"
/// sentinel.
#[test]
fn no_unbound_symbol_carries_a_variable_documentation() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(list
          ;; unbound, yet the plist carries a `variable-documentation'
          (let ((n 0))
            (mapatoms
             (lambda (s)
               (if (get s 'variable-documentation)
                   (if (boundp s) nil (setq n (1+ n))))))
            n)
          ;; unbound, yet `documentation-property' answers
          (let ((n 0))
            (mapatoms
             (lambda (s)
               (if (documentation-property s 'variable-documentation t)
                   (if (boundp s) nil (setq n (1+ n))))))
            n)
          ;; carrying GNU's reserved `no doc' sentinel (src/doc.c:433-434)
          (let ((n 0))
            (mapatoms
             (lambda (s)
               (if (eq (get s 'variable-documentation) 0) (setq n (1+ n)))))
            n))",
    );
    assert_eq!(results[0], "OK (0 0 0)");
}

/// The same diagonal counted from the other side: how much
/// `variable-documentation` exists at all before any Lisp has run.
///
/// GNU's answer is "none".  Nothing installs a `variable-documentation` before
/// the variable exists -- `Fsnarf_documentation` runs from `loadup.el:476`,
/// after the C `DEFVAR`s and after the preloaded Lisp, and its `Fput` is
/// gated on `Fboundp` (`src/doc.c:606-613`); Lisp `defvar` installs one only
/// while defining the variable and only when the docstring is non-nil
/// (`src/eval.c:909-912`).  A bare `Context` is the moment before all of that,
/// so both counts are zero.
///
/// This test replaces two that asserted `OK (70 1902)` -- one counting the
/// plist entries, one counting the ones that resolved to a string.  Those were
/// the size of the `STARTUP_VARIABLE_DOC_*` seeding rather than a fact about
/// GNU, and ledger 178 removed the seeding: it put a doc on the FIRST arm
/// `documentation_property_plan` consults, ahead of the `Fboundp` gate, so 35
/// unbound names in the shipped image answered where GNU answers nil.  The 70
/// integer rows were seeded with `(fixnum 0)`, the value `src/doc.c:433-434`
/// reserves to mean "there is no doc".
///
/// The C variables this port declares keep their documentation: it comes from
/// `var_docs::gnu_table` through `Fsnarf_documentation`'s gate, which is a
/// lookup rather than a plist entry, so it is deliberately not counted here.
#[test]
fn no_variable_documentation_is_installed_before_any_lisp_runs() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(list
          (let ((n 0))
            (mapatoms
             (lambda (s)
               (if (integerp (get s 'variable-documentation)) (setq n (1+ n)))))
            n)
          (let ((n 0))
            (mapatoms
             (lambda (s)
               (if (stringp (get s 'variable-documentation)) (setq n (1+ n)))))
            n))",
    );
    assert_eq!(results[0], "OK (0 0)");
}

#[test]
fn mapatoms_roots_anonymous_callback_across_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((ob (make-vector 7 0)))
             (intern "mapatoms-root-a" ob)
             (intern "mapatoms-root-b" ob)
             (let ((count 0))
               (mapatoms (lambda (_sym)
                           (garbage-collect)
                           (setq count (1+ count)))
                         ob)
               count))"#,
    ));
    assert_eq!(result, "OK 2");
    assert!(ev.gc_count > 0, "callback-triggered GC should run");
}

#[test]
fn mapatoms_default_obarray_uses_dynamic_lisp_binding() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((obarray (make-vector 17 0))
                 (names nil))
             (set (intern "mapatoms-dynamic-alpha") 1)
             (intern "mapatoms-dynamic-beta")
             (intern "other")
             (mapatoms (lambda (sym)
                         (if (string-match "\\`mapatoms-dynamic-" (symbol-name sym))
                             (setq names (cons sym names)))))
             (sort names #'string-lessp))"#,
    );
    assert_eq!(result, "OK (mapatoms-dynamic-alpha mapatoms-dynamic-beta)");
}

#[test]
fn maphash_roots_reconstructed_keys_across_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((h (make-hash-table :test 'equal))
                 (sum 0))
             (puthash (list 'a 1) 'x h)
             (puthash (list 'b 2) 'y h)
             (maphash (lambda (k _v)
                        (garbage-collect)
                        (setq sum (+ sum (car (cdr k)))))
                      h)
             sum)"#,
    ));
    assert_eq!(result, "OK 3");
    assert!(ev.gc_count > 0, "callback-triggered GC should run");
}

#[test]
fn maphash_walks_live_hash_slots_during_mutation_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(let ((h (make-hash-table :test 'eq))
                 seen)
             (puthash 'a 1 h)
             (puthash 'b 2 h)
             (maphash (lambda (k _v)
                        (push k seen)
                        (when (eq k 'a)
                          (puthash 'c 3 h)
                          (remhash 'b h)))
                      h)
             (list (sort seen (lambda (a b)
                                (string< (symbol-name a) (symbol-name b))))
                   (hash-table-count h)
                   (gethash 'b h 'missing)
                   (gethash 'c h 'missing)))"#,
    );
    assert_eq!(result, "OK ((a c) 2 missing 3)");
}

#[test]
fn features_variable_controls_featurep_and_require() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(setq features '(vm-existing))
         (featurep 'vm-existing)
         (require 'vm-existing)",
    );
    assert_eq!(results[0], "OK (vm-existing)");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK vm-existing");
}

#[test]
fn require_accepts_nil_filename_as_feature_name() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("vm-require-nil.el"),
        "(provide 'vm-require-nil)\n",
    )
    .expect("write require fixture");

    let escaped = dir
        .path()
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let script = format!(
        "(progn (setq load-path (cons \"{}\" load-path)) 'ok)\n\
         (require 'vm-require-nil nil)\n\
         (featurep 'vm-require-nil)",
        escaped
    );
    let results = eval_all(&script);

    assert_eq!(results[1], "OK vm-require-nil");
    assert_eq!(results[2], "OK t");
}

#[test]
fn require_missing_file_uses_gnu_file_missing_condition_data() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(condition-case err
               (require 'vm-require-missing)
             (error err))
           (condition-case err
               (require 'vm-require-missing "vm-explicit-file")
             (error err))"#,
    );

    assert_eq!(
        results[0],
        r#"OK (file-missing "Cannot open load file" "No such file or directory" "vm-require-missing")"#
    );
    assert_eq!(
        results[1],
        r#"OK (file-missing "Cannot open load file" "No such file or directory" "vm-explicit-file")"#
    );
}

#[test]
fn provide_preserves_features_variable_entries() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(setq features '(vm-existing))
         (provide 'vm-new)
         features",
    );
    assert_eq!(results[0], "OK (vm-existing)");
    assert_eq!(results[1], "OK vm-new");
    assert_eq!(results[2], "OK (vm-new vm-existing)");
}

#[test]
fn require_recursive_cycle_with_early_provide_loads_until_feature_is_provided() {
    crate::test_utils::init_test_tracing();
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-require-recursive-{unique}"));
    fs::create_dir_all(&dir).expect("create fixture dir");
    fs::write(
        dir.join("vm-rec-a.el"),
        "(provide 'vm-rec-a)\n(require 'vm-rec-b)\n(setq vm-rec-a-saw-b vm-rec-b-value)\n",
    )
    .expect("write vm-rec-a");
    fs::write(
        dir.join("vm-rec-b.el"),
        "(require 'vm-rec-a)\n(setq vm-rec-b-value 42)\n(provide 'vm-rec-b)\n",
    )
    .expect("write vm-rec-b");

    let escaped = dir
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let script = format!(
        "(progn (setq load-path (cons \"{}\" load-path)) 'ok)\n\
         (require 'vm-rec-b)\n\
         (featurep 'vm-rec-a)\n\
         (featurep 'vm-rec-b)\n\
         vm-rec-a-saw-b",
        escaped
    );
    let results = eval_all(&script);
    assert_eq!(results[1], "OK vm-rec-b");
    assert_eq!(results[2], "OK t");
    assert_eq!(results[3], "OK t");
    assert_eq!(results[4], "OK 42");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn require_recursive_cycle_without_provide_hits_gnu_nesting_guard() {
    crate::test_utils::init_test_tracing();
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("neovm-require-recursive-error-{unique}"));
    fs::create_dir_all(&dir).expect("create fixture dir");
    fs::write(
        dir.join("vm-rec-a.el"),
        "(require 'vm-rec-b)\n(provide 'vm-rec-a)\n",
    )
    .expect("write vm-rec-a");
    fs::write(
        dir.join("vm-rec-b.el"),
        "(require 'vm-rec-a)\n(provide 'vm-rec-b)\n",
    )
    .expect("write vm-rec-b");

    let escaped = dir
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let result = eval_one(&format!(
        "(progn
           (setq load-path (cons \"{}\" load-path))
           (require 'vm-rec-a))",
        escaped
    ));
    assert_eq!(
        result,
        "ERR (error (\"Recursive `require' for feature `vm-rec-a'\"))"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dotimes_loop() {
    crate::test_utils::init_test_tracing();
    // dotimes is no longer a special form; use let+while equivalent
    let result = eval_one(
        "(let ((sum 0) (i 0))
           (while (< i 5)
             (setq sum (+ sum i))
             (setq i (1+ i)))
           sum)",
    );
    assert_eq!(result, "OK 10"); // 0+1+2+3+4 = 10
}

#[test]
fn dolist_loop() {
    crate::test_utils::init_test_tracing();
    // dolist is no longer a special form; use let+while equivalent
    let result = eval_one(
        "(let ((result nil) (--dl-- '(a b c)))
           (while --dl--
             (let ((x (car --dl--)))
               (setq result (cons x result)))
             (setq --dl-- (cdr --dl--)))
           result)",
    );
    assert_eq!(result, "OK (c b a)");
}

#[test]
fn ignore_errors_catches_signal() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one("(ignore-errors (/ 1 0) 42)");
    assert_eq!(result, "OK nil"); // error caught, returns nil
}

#[test]
fn math_functions() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(expt 2 10)"), "OK 1024");
    assert_eq!(eval_one("(sqrt 4.0)"), "OK 2.0");
}

#[test]
fn hook_system() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(defvar my-hook nil)
         (defun hook-fn () 42)
         (add-hook 'my-hook 'hook-fn)
         (list (run-hooks 'my-hook)
               my-hook
               (subrp (symbol-function 'add-hook))
               (subrp (symbol-function 'remove-hook))
               (subrp (symbol-function 'run-mode-hooks)))",
    );
    assert_eq!(results[3], "OK (nil (hook-fn) nil nil nil)");
}

#[test]
fn hook_system_runtime_value_shapes() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(setq hook-count 0)
         (defalias 'hook-inc #'(lambda () (setq hook-count (1+ hook-count))))
         (setq hook-probe-hook 'hook-inc)
         (condition-case err (run-hooks 'hook-probe-hook) (error err))
         hook-count
         (setq hook-count 0)
         (setq hook-probe-hook (cons 'hook-inc 1))
         (condition-case err (run-hooks 'hook-probe-hook) (error err))
         hook-count
         (setq hook-probe-hook t)
         (condition-case err (run-hooks 'hook-probe-hook) (error err))
         (setq hook-probe-hook 42)
         (condition-case err (run-hooks 'hook-probe-hook) (error err))
         (setq hook-probe-hook '(t hook-inc))
         (setq hook-count 0)
         (condition-case err (run-hooks 'hook-probe-hook) (error err))
         hook-count",
    );
    assert_eq!(results[3], "OK nil");
    assert_eq!(results[4], "OK 1");
    assert_eq!(results[7], "OK nil");
    assert_eq!(results[8], "OK 1");
    assert_eq!(results[10], "OK (void-function t)");
    assert_eq!(results[12], "OK (invalid-function 42)");
    assert_eq!(results[15], "OK nil");
    assert_eq!(results[16], "OK 2");
}

#[test]
fn local_hook_inheritance_marker_is_canonical_t_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (setq hook-count 0)
             (defalias 'hook-inc #'(lambda () (setq hook-count (1+ hook-count))))
             (set-default 'hook-probe-hook '(hook-inc))
             (make-local-variable 'hook-probe-hook)
             (setq hook-probe-hook (list (make-symbol "t") 'hook-inc))
             (list (condition-case nil
                       (progn (run-hooks 'hook-probe-hook) 'no-error)
                     (void-function 'void-function)
                     (error 'other-error))
                   hook-count))"#,
    );
    assert_eq!(result, "OK (void-function 0)");
}

#[test]
fn safe_run_hook_removes_failing_local_hook_and_continues_to_global_hook() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer = ev.buffers.create_buffer("*safe-hook*");
    ev.buffers.set_current(buffer);

    ev.eval_str(
        r#"(progn
             (setq safe-hook-log nil)
             (defalias 'safe-hook-bad
               #'(lambda ()
                   (setq safe-hook-log (cons 'bad safe-hook-log))
                   (signal 'error '("boom"))))
             (defalias 'safe-hook-good
               #'(lambda ()
                   (setq safe-hook-log (cons 'good safe-hook-log))))
             (setq safe-local-hook '(safe-hook-good))
             (make-local-variable 'safe-local-hook)
             (setq safe-local-hook '(safe-hook-bad t)))"#,
    )
    .expect("safe hook test setup");

    crate::emacs_core::hook_runtime::safe_run_named_hook(
        &mut ev,
        crate::emacs_core::intern::intern("safe-local-hook"),
        &[],
    )
    .expect("safe hook should swallow ordinary hook errors");

    let result = ev
        .eval_str("(list safe-hook-log safe-local-hook (default-value 'safe-local-hook))")
        .expect("inspect safe hook result");
    assert_eq!(format!("{}", result), "((good bad) (t) (safe-hook-good))");

    let messages_id = ev
        .buffers
        .find_buffer_by_name("*Messages*")
        .expect("safe hook errors should be reported through `message`");
    let messages = ev.buffers.get(messages_id).expect("live *Messages* buffer");
    assert_eq!(
        messages.buffer_string(),
        "Error in safe-local-hook (safe-hook-bad): (error \"boom\")\n"
    );
}

#[test]
fn run_hook_with_args_runtime_value_shapes() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(setq hook-log nil)
         (defalias 'hook-log-fn #'(lambda (&rest args) (setq hook-log (cons args hook-log))))
         (setq hook-probe-hook 'hook-log-fn)
         (condition-case err (run-hook-with-args 'hook-probe-hook 1 2) (error err))
         hook-log
         (setq hook-log nil)
         (setq hook-probe-hook (cons 'hook-log-fn 1))
         (condition-case err (run-hook-with-args 'hook-probe-hook 3) (error err))
         hook-log
         (setq hook-probe-hook t)
         (condition-case err (run-hook-with-args 'hook-probe-hook 4) (error err))
         (setq hook-probe-hook 42)
         (condition-case err (run-hook-with-args 'hook-probe-hook 5) (error err))
         (setq hook-log nil)
         (setq hook-probe-hook '(t hook-log-fn))
         (condition-case err (run-hook-with-args 'hook-probe-hook 6) (error err))
         hook-log",
    );
    assert_eq!(results[3], "OK nil");
    assert_eq!(results[4], "OK ((1 2))");
    assert_eq!(results[7], "OK nil");
    assert_eq!(results[8], "OK ((3))");
    assert_eq!(results[10], "OK (void-function t)");
    assert_eq!(results[12], "OK (invalid-function 42)");
    assert_eq!(results[15], "OK nil");
    assert_eq!(results[16], "OK ((6) (6))");
}

#[test]
fn run_hook_with_args_roots_callbacks_and_args_across_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"
(progn
  (setq hook-root-a nil)
  (setq hook-root-b nil)
  (setq hook-probe-hook
        (list
         (lambda (arg)
           (garbage-collect)
           (setq hook-root-a arg))
         (lambda (arg)
           (garbage-collect)
           (setq hook-root-b arg))))
  (let ((payload (cons 'x 'y)))
    (run-hook-with-args 'hook-probe-hook payload)
    (list hook-root-a hook-root-b payload)))
"#,
    ));
    assert_eq!(result, "OK ((x . y) (x . y) (x . y))");
    assert!(ev.gc_count > 0, "hook callback GC should run");
}

#[test]
fn run_hook_with_args_accepts_uninterned_symbol_after_same_eval_let_setup() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq test-hook nil)
                 (let ((fun (make-symbol "vm-hook-uninterned")))
                   (fset fun (lambda (x) (setq test-hook-result x)))
                   (setq test-hook (list fun)))
                 (run-hook-with-args 'test-hook 42)
                 test-hook-result)"#
        ),
        "OK 42"
    );
}

#[test]
fn run_hook_with_args_accepts_uninterned_symbol_after_same_eval_lexical_let_setup() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let result = format_eval_result(&ev.eval_str(
        r#"(progn
             (setq test-hook nil)
             (let ((fun (make-symbol "vm-hook-uninterned-lex")))
               (fset fun (lambda (x) (setq test-hook-result x)))
               (setq test-hook (list fun)))
             (run-hook-with-args 'test-hook 42)
             test-hook-result)"#,
    ));
    assert_eq!(result, "OK 42");
}

#[test]
fn run_hook_wrapped_stops_on_first_non_nil_wrapper_result() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let ((seen nil))
           (defalias 'hook-wrap-a #'(lambda () 'a))
           (defalias 'hook-wrap-b #'(lambda () 'b))
           (defalias 'hook-wrap-wrapper
             #'(lambda (fn)
                 (setq seen (cons fn seen))
                 (if (eq fn 'hook-wrap-a) 'stop nil)))
           (setq hook-wrap-probe '(hook-wrap-a hook-wrap-b))
           (list (run-hook-wrapped 'hook-wrap-probe 'hook-wrap-wrapper)
                 seen))",
    );
    assert_eq!(result, "OK (stop (hook-wrap-a))");
}

#[test]
fn get_buffer_create_runs_buffer_list_update_hook_when_enabled() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq hook-log nil)
           (setq buffer-list-update-hook
                 (list (lambda ()
                         (setq hook-log (cons 'ran hook-log)))))
           (get-buffer-create \"gbc-hook\")
           hook-log)",
    );
    assert_eq!(result, "OK (ran)");
}

#[test]
fn get_buffer_create_inhibit_buffer_hooks_suppresses_buffer_and_kill_hooks() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq hook-log nil)
           (setq buffer-list-update-hook
                 (list (lambda ()
                         (setq hook-log (cons 'buffer-list hook-log)))))
           (let ((buf (get-buffer-create \"gbc-inhibit\" t)))
             (save-current-buffer
               (set-buffer buf)
               (setq kill-buffer-query-functions
                     (list (lambda ()
                             (setq hook-log (cons 'query hook-log))
                             t)))
               (setq kill-buffer-hook
                     (list (lambda ()
                             (setq hook-log (cons 'kill hook-log))))))
             (kill-buffer buf)
             hook-log))",
    );
    assert_eq!(result, "OK nil");
}

#[test]
fn kill_buffer_runs_query_functions_and_hook_in_target_buffer_context() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq hook-log nil)
           (let ((buf (get-buffer-create \"kill-hook\"))
                 (other (get-buffer-create \"kill-other\")))
             (set-buffer buf)
             (setq kill-buffer-query-functions
                   (list (lambda ()
                           (setq hook-log
                                 (cons (list 'query (buffer-name)) hook-log))
                           t)))
             (setq kill-buffer-hook
                   (list (lambda ()
                           (setq hook-log
                                 (cons (list 'hook (buffer-name)) hook-log)))))
             (set-buffer other)
             (list (kill-buffer buf)
                   (get-buffer \"kill-hook\")
                   (nreverse hook-log)
                   (buffer-name))))",
    );
    assert_eq!(
        result,
        "OK (t nil ((query \"kill-hook\") (hook \"kill-hook\")) \"kill-other\")"
    );
}

#[test]
fn kill_buffer_hook_survives_a_major_mode_local_variable_reset() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq kill-hook-ran nil)
           (let ((buffer (get-buffer-create \" *kill-hook-permanent*\")))
             (set-buffer buffer)
             (make-local-variable 'kill-buffer-hook)
             (setq kill-buffer-hook
                   (list (lambda () (setq kill-hook-ran t))))
             (kill-all-local-variables)
             (kill-buffer buffer))
           kill-hook-ran)",
    );

    assert_eq!(result, "OK t");
}

#[test]
fn kill_buffer_query_abort_does_not_record_buffer_list_order_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let* ((a (get-buffer-create \"kb-order-a\"))
                (b (get-buffer-create \"kb-order-b\"))
                (before (mapcar #'buffer-name (buffer-list))))
           (setq kill-buffer-query-functions (list (lambda () nil)))
           (list (equal before '(\"*scratch*\" \"kb-order-a\" \"kb-order-b\"))
                 (kill-buffer b)
                 (equal before (mapcar #'buffer-name (buffer-list)))
                 (and (get-buffer \"kb-order-b\") t)))",
    );
    assert_eq!(result, "OK (t nil t t)");
}

#[test]
fn kill_selected_current_buffer_selects_its_window_replacement() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(let ((victim (get-buffer-create " *kill-selected-current*")))
  (unwind-protect
      (save-window-excursion
        (switch-to-buffer victim)
        (kill-buffer victim)
        (list
         (buffer-name (current-buffer))
         (buffer-name (window-buffer (selected-window)))
         (eq (current-buffer) (window-buffer (selected-window)))))
    (when (buffer-live-p victim)
      (kill-buffer victim))))"#,
    );
    assert_eq!(result, r#"OK ("*scratch*" "*scratch*" t)"#);
}

#[test]
fn kill_buffer_propagates_window_replacement_errors() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(progn
  (require 'cl-lib)
  (let ((victim (get-buffer-create " *kill-replacement-error*")))
    (unwind-protect
        (progn
          (switch-to-buffer victim)
          (list
           (condition-case err
               (cl-letf
                   (((symbol-function 'replace-buffer-in-windows)
                     (lambda (&rest _)
                       (error "replacement failed"))))
                 (kill-buffer victim))
             (error (list (car err) (cadr err))))
           (buffer-live-p victim)))
      (when (buffer-live-p victim)
        (kill-buffer victim)))))"#,
    );
    assert_eq!(result, r#"OK ((error "replacement failed") t)"#);
}

#[test]
fn killed_buffer_slots_and_local_defaults_match_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((file-buffer (get-buffer-create "dead-file"))
                 (local-buffer (get-buffer-create "dead-local")))
             (set-buffer file-buffer)
             (setq buffer-file-name "/tmp/dead.txt"
                   buffer-file-truename "/tmp/dead.txt")
             (set-buffer local-buffer)
             (setq fill-column 33)
             (set (make-local-variable 'dead-local-only) 44)
             (let ((before (list (local-variable-p 'fill-column local-buffer)
                                 (buffer-local-value 'fill-column local-buffer)
                                 (buffer-local-value 'dead-local-only local-buffer))))
               (kill-buffer file-buffer)
               (kill-buffer local-buffer)
               (list (buffer-live-p file-buffer)
                     (buffer-name file-buffer)
                     (buffer-last-name file-buffer)
                     (buffer-file-name file-buffer)
                     (buffer-base-buffer file-buffer)
                     (condition-case e
                         (set-buffer file-buffer)
                       (error (car e)))
                     before
                     (buffer-live-p local-buffer)
                     (buffer-local-value 'fill-column local-buffer)
                     (boundp 'dead-local-only))))"#,
    );
    assert_eq!(
        result,
        r#"OK (nil nil "dead-file" "/tmp/dead.txt" nil error (t 33 44) nil 70 nil)"#
    );
}

#[test]
fn killed_buffer_detaches_overlay_objects_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((buf (get-buffer-create "dead-overlay"))
                 ov)
             (set-buffer buf)
             (erase-buffer)
             (insert "abc")
             (setq ov (make-overlay 1 2))
             (set-buffer (get-buffer-create "*scratch*"))
             (kill-buffer buf)
             (list (overlay-start ov)
                   (overlay-end ov)
                   (overlay-buffer ov)
                   (delete-overlay ov)))"#,
    );
    assert_eq!(result, "OK (nil nil nil nil)");
}

#[test]
fn run_window_scroll_functions_uses_scrolled_window_buffer_context() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq hook-log nil)
           (let* ((buf1 (get-buffer-create \"scroll-a\"))
                  (buf2 (get-buffer-create \"scroll-b\")))
             (set-buffer buf1)
             (set-window-buffer (selected-window) buf1)
             (let ((w2 (split-window-internal (selected-window) nil nil nil)))
               (set-window-buffer w2 buf2)
               (set-buffer buf2)
               (setq window-scroll-functions
                     (list (lambda (_w _start)
                             (setq hook-log (buffer-name)))))
               (set-buffer buf1)
               (run-window-scroll-functions w2)
               (list hook-log (buffer-name)))))",
    );
    assert_eq!(result, "OK (\"scroll-b\" \"scroll-a\")");
}

#[test]
fn run_window_scroll_functions_reads_displayed_buffer_local_hook() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq hook-log nil)
           (let* ((buf1 (get-buffer-create \"scroll-local-a\"))
                  (buf2 (get-buffer-create \"scroll-local-b\")))
             (set-buffer buf1)
               (set-window-buffer (selected-window) buf1)
               (let ((w2 (split-window-internal (selected-window) nil nil nil)))
                 (set-window-buffer w2 buf2)
               (let ((orig (current-buffer)))
                 (set-buffer buf2)
                 (make-local-variable 'window-scroll-functions)
                 (setq window-scroll-functions
                       (list (lambda (w start)
                               (setq hook-log
                                     (list (buffer-name)
                                           (buffer-name (window-buffer w))
                                           start)))))
                 (set-buffer orig))
               (set-buffer buf1)
               (run-window-scroll-functions w2)
               (list hook-log (buffer-name)))))",
    );
    assert_eq!(
        result,
        "OK ((\"scroll-local-b\" \"scroll-local-b\" 1) \"scroll-local-a\")"
    );
}

#[test]
fn set_window_buffer_runs_window_scroll_functions_in_new_buffer_context() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq hook-log nil)
             (let* ((buf1 (get-buffer-create \"swb-scroll-a\"))
                    (buf2 (get-buffer-create \"swb-scroll-b\")))
               (set-buffer buf1)
               (set-window-buffer (selected-window) buf1)
             (let ((orig (current-buffer)))
               (set-buffer buf2)
               (make-local-variable 'window-scroll-functions)
               (setq window-scroll-functions
                     (list (lambda (w start)
                             (setq hook-log
                                   (list (buffer-name)
                                         (buffer-name (window-buffer w))
                                         start)))))
               (set-buffer orig))
             (set-window-buffer (selected-window) buf2)
             (list hook-log
                   (buffer-name)
                   (buffer-name (window-buffer (selected-window))))))",
    );
    assert_eq!(
        result,
        "OK ((\"swb-scroll-b\" \"swb-scroll-b\" 1) \"swb-scroll-a\" \"swb-scroll-b\")"
    );
}

#[test]
fn point_motion_hooks_follow_gnu_interval_boundary_order() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (erase-buffer)
           (insert \"abcd\")
           (setq hook-log nil)
           (setq inhibit-point-motion-hooks nil)
           (defalias 'pm-leave-before
             #'(lambda (old new)
                 (setq hook-log (append hook-log (list (list 'leave-before old new))))))
           (defalias 'pm-leave-after
             #'(lambda (old new)
                 (setq hook-log (append hook-log (list (list 'leave-after old new))))))
           (defalias 'pm-enter-before
             #'(lambda (old new)
                 (setq hook-log (append hook-log (list (list 'enter-before old new))))))
           (defalias 'pm-enter-after
             #'(lambda (old new)
                 (setq hook-log (append hook-log (list (list 'enter-after old new))))))
           (put-text-property 1 2 'point-left 'pm-leave-before)
           (put-text-property 2 3 'point-left 'pm-leave-after)
           (put-text-property 3 4 'point-entered 'pm-enter-before)
           (put-text-property 4 5 'point-entered 'pm-enter-after)
           (goto-char 2)
           (goto-char 4)
           hook-log)",
    );
    assert_eq!(
        result,
        "OK ((leave-before 2 4) (leave-after 2 4) (enter-before 2 4) (enter-after 2 4))"
    );
}

#[test]
fn overlay_intangible_adjusts_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((buf (get-buffer-create "ov-intangible"))
                 ov)
             (set-buffer buf)
             (erase-buffer)
             (insert "abcdef")
             (setq ov (make-overlay 3 5))
             (overlay-put ov 'intangible 'zone)
             (let ((inhibit-point-motion-hooks nil))
               (goto-char 2)
               (goto-char 4)
               (let ((forward (point)))
                 (goto-char 6)
                 (goto-char 4)
                 (let ((backward (point)))
                   (let ((inhibit-point-motion-hooks t))
                     (goto-char 4)
                     (list forward backward (point)))))))"#,
    );
    assert_eq!(result, "OK (5 3 4)");
}

#[test]
fn adjacent_overlay_intangible_values_remain_separate_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((buf (get-buffer-create "ov-intangible-adjacent"))
                 left right)
             (set-buffer buf)
             (erase-buffer)
             (insert "abcdef")
             (setq left (make-overlay 2 4))
             (setq right (make-overlay 4 6))
             (overlay-put left 'intangible 'left-zone)
             (overlay-put right 'intangible 'right-zone)
             (setq inhibit-point-motion-hooks nil)
             (goto-char 1)
             (goto-char 3)
             (let ((from-left (point)))
               (goto-char 7)
               (goto-char 5)
               (list from-left (point))))"#,
    );
    assert_eq!(result, "OK (4 4)");
}

#[test]
fn run_window_configuration_change_hook_uses_window_buffer_context() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
           (setq hook-log nil)
           (defalias 'wcch-log-current-buffer
             #'(lambda ()
                 (setq hook-log
                       (cons (intern (buffer-name)) hook-log))))
           (defalias 'wcch-log-global-buffer
             #'(lambda ()
                 (setq hook-log
                       (cons (intern (concat "global:" (buffer-name))) hook-log)))))"#,
    )
    .expect("hook setup");

    let buf1 = ev.buffers.create_buffer("wcch-a");
    let buf2 = ev.buffers.create_buffer("wcch-b");
    ev.switch_current_buffer(buf1).expect("switch to buf1");

    let selected_window = crate::emacs_core::window_cmds::builtin_selected_window(&mut ev, vec![])
        .expect("selected window");
    crate::emacs_core::window_cmds::builtin_set_window_buffer(
        &mut ev,
        vec![selected_window, Value::make_buffer(buf1)],
    )
    .expect("selected window buffer");
    let split_window = ev
        .eval_str("(split-window-internal (selected-window) nil nil nil)")
        .expect("split window");
    crate::emacs_core::window_cmds::builtin_set_window_buffer(
        &mut ev,
        vec![split_window, Value::make_buffer(buf2)],
    )
    .expect("split window buffer");

    ev.buffers
        .set_buffer_local_property(
            buf1,
            "window-configuration-change-hook",
            Value::list(vec![Value::symbol("wcch-log-current-buffer")]),
        )
        .expect("buf1 local hook");
    ev.buffers
        .set_buffer_local_property(
            buf2,
            "window-configuration-change-hook",
            Value::list(vec![Value::symbol("wcch-log-current-buffer")]),
        )
        .expect("buf2 local hook");
    crate::emacs_core::data::set_default(
        &mut ev,
        vec![
            Value::symbol("window-configuration-change-hook"),
            Value::list(vec![Value::symbol("wcch-log-global-buffer")]),
        ],
    )
    .expect("default hook");
    assert!(
        ev.buffers
            .get(buf1)
            .and_then(|buffer| buffer.buffer_local_value("window-configuration-change-hook"))
            .is_some()
    );
    assert!(
        ev.buffers
            .get(buf2)
            .and_then(|buffer| buffer.buffer_local_value("window-configuration-change-hook"))
            .is_some()
    );
    assert_eq!(
        ev.frames
            .selected_frame()
            .expect("selected frame")
            .window_list()
            .len(),
        2
    );

    ev.switch_current_buffer(buf1).expect("restore buf1");
    super::builtins::builtin_run_window_configuration_change_hook(&mut ev, vec![])
        .expect("run window-configuration-change-hook");

    let hook_log = ev.eval_symbol("hook-log").expect("hook log");
    let items = list_to_vec(&hook_log).expect("hook log list");
    let names: Vec<String> = items
        .iter()
        .map(|value| value.as_symbol_name().expect("symbol").to_string())
        .collect();
    assert!(names.iter().any(|name| name == "wcch-a"), "names={names:?}");
    assert!(names.iter().any(|name| name == "wcch-b"), "names={names:?}");
    assert!(
        names.iter().any(|name| name == "global:wcch-a"),
        "names={names:?}"
    );
    assert_eq!(
        ev.buffers
            .current_buffer()
            .expect("current buffer")
            .name_value(),
        Value::string("wcch-a")
    );
}

#[test]
fn run_window_configuration_change_hook_ignores_sides_inhibit_check() {
    // Regression for #191: `window--sides-inhibit-check' is let-bound to t only
    // to suppress `window--check' (the side-window consistency validator) and
    // routinely sits at t in real configs (Doom). GNU's
    // run_window_configuration_change_hook never consults it, so
    // `window-configuration-change-hook' must still fire. A wrong guard here
    // silenced the hook whenever that var was t -- winum never renumbered a
    // freshly-opened popup/compile window until an unrelated command forced it.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (setq wcch-fired 0)
             (setq window--sides-inhibit-check t)
             (defalias 'wcch-count #'(lambda () (setq wcch-fired (1+ wcch-fired))))
             (set-default 'window-configuration-change-hook '(wcch-count)))"#,
    )
    .expect("setup");
    super::builtins::builtin_run_window_configuration_change_hook(&mut ev, vec![])
        .expect("run window-configuration-change-hook");
    let fired = ev.eval_symbol("wcch-fired").expect("wcch-fired");
    assert_eq!(
        fired.as_fixnum(),
        Some(1),
        "window-configuration-change-hook must fire even when window--sides-inhibit-check is t"
    );
}

#[test]
fn redisplay_runs_window_change_functions_with_selected_frame_context() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq hook-log nil)
           (let* ((buf1 (get-buffer-create \"wcf-a\"))
                  (buf2 (get-buffer-create \"wcf-b\")))
             (set-window-buffer (selected-window) buf1)
             (let ((w2 (split-window-internal (selected-window) nil nil nil)))
               (set-window-buffer w2 buf2)
               (setq window-size-change-functions
                     (list (lambda (frame)
                             (setq hook-log
                                   (cons (list 'size (eq frame (selected-frame))
                                               (buffer-name))
                                         hook-log)))))
               (setq window-selection-change-functions
                     (list (lambda (frame)
                             (setq hook-log
                                   (cons (list 'selection (eq frame (selected-frame))
                                               (buffer-name))
                                         hook-log)))))
               (setq window-state-change-functions
                     (list (lambda (frame)
                             (setq hook-log
                                   (cons (list 'state (eq frame (selected-frame))
                                               (buffer-name))
                                         hook-log)))))
               (setq window-state-change-hook
                     (list (lambda ()
                             (setq hook-log (cons 'state-hook hook-log)))))
               (select-window w2)
               (redisplay)
               (nreverse hook-log))))",
    );
    assert_eq!(
        result,
        "OK ((size t \"wcf-b\") (selection t \"wcf-b\") (state t \"wcf-b\") state-hook)"
    );
}

#[test]
fn set_frame_window_state_change_forces_state_hooks_on_redisplay() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq hook-log nil)
           (setq window-state-change-functions
                 (list (lambda (_frame)
                         (setq hook-log (cons 'state hook-log)))))
           (setq window-state-change-hook
                 (list (lambda ()
                         (setq hook-log (cons 'state-hook hook-log)))))
           (set-frame-window-state-change nil t)
           (redisplay)
           (nreverse hook-log))",
    );
    assert_eq!(result, "OK (state state-hook)");
}

#[test]
fn delete_frame_runs_before_and_after_delete_hooks() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        "(progn
           (setq hook-log nil)
           ;; `make-frame' is lisp/frame.el:1019 and has no subr any more
           ;; (DIVERGENCES.md 154); on a text terminal its
           ;; `frame-creation-function' reaches this C DEFUN (src/frame.c:1736).
           (let ((f2 (make-terminal-frame nil)))
             (setq delete-frame-functions
                   (list (lambda (frame)
                           (setq hook-log
                                 (cons (list 'before (frame-live-p frame)) hook-log)))))
             (setq after-delete-frame-functions
                   (list (lambda (frame)
                           (setq hook-log
                                 (cons (list 'after (frame-live-p frame)) hook-log)))))
             (delete-frame f2)
             (nreverse hook-log)))",
    );
    assert_eq!(result, "OK ((before t) (after nil))");
}

#[test]
fn first_change_and_before_change_hooks_run_with_inhibit_bound() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq hook-log nil)
           (setq first-change-hook
                 (list (lambda ()
                         (setq hook-log
                               (cons (list 'first inhibit-modification-hooks) hook-log)))))
           (setq before-change-functions
                 (list (lambda (_beg _end)
                         (setq hook-log
                               (cons (list 'before inhibit-modification-hooks) hook-log)))))
           (insert \"x\")
           (nreverse hook-log))",
    );
    assert_eq!(result, "OK ((first t) (before t))");
}

#[test]
fn inhibit_modification_hooks_is_bound_to_nil_by_default() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(list (boundp 'inhibit-modification-hooks) inhibit-modification-hooks)");
    assert_eq!(result, "OK (t nil)");
}

#[test]
fn combine_after_change_calls_is_gnu_defvar_style_dynamic_variable() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(list (boundp 'combine-after-change-calls)
               combine-after-change-calls
               (let ((combine-after-change-calls t))
                 combine-after-change-calls))",
    );
    assert_eq!(result, "OK (t nil t)");
}

#[test]
fn after_change_functions_receive_character_old_len() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (erase-buffer)
           (insert \"é\")
           (setq hook-log nil)
           (setq after-change-functions
                 (list (lambda (_beg _end old-len)
                         (setq hook-log (list old-len inhibit-modification-hooks)))))
           (delete-region 1 2)
           hook-log)",
    );
    assert_eq!(result, "OK (1 t)");
}

#[test]
fn princ_to_buffer_runs_before_and_after_change_hooks_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (erase-buffer)
             (setq hook-log nil)
             (setq before-change-functions
                   (list (lambda (beg end)
                           (setq hook-log
                                 (cons (list 'before beg end) hook-log)))))
             (setq after-change-functions
                   (list (lambda (beg end old-len)
                           (setq hook-log
                                 (cons (list 'after beg end old-len) hook-log)))))
             (princ "X" (current-buffer))
             (list (buffer-string) (nreverse hook-log)))"#,
    );
    assert_eq!(result, r#"OK ("X" ((before 1 1) (after 1 2 0)))"#);
}

#[test]
fn subst_char_in_region_reports_gnu_first_to_last_changed_after_range() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (erase-buffer)
           (insert \"xaaxb\")
           (let ((events nil))
             (setq before-change-functions
                   (list (lambda (beg end)
                           (setq events (cons (list 'before beg end) events)))))
             (setq after-change-functions
                   (list (lambda (beg end old-len)
                           (setq events
                                 (cons (list 'after beg end old-len) events)))))
             (subst-char-in-region 1 6 ?a ?b)
             (list (buffer-string) (nreverse events))))",
    );
    assert_eq!(result, r#"OK ("xbbxb" ((before 2 6) (after 2 4 2)))"#);
}

#[test]
fn subst_char_in_region_uses_gnu_first_change_span_for_modified_ticks() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (erase-buffer)
           (insert \"zzzaa\")
           (set-buffer-modified-p nil)
           (let ((modified (buffer-modified-tick))
                 (chars-modified (buffer-chars-modified-tick)))
             (subst-char-in-region 1 6 ?a ?b)
             (list (buffer-string)
                   (- (buffer-modified-tick) modified)
                   (- (buffer-chars-modified-tick) chars-modified)
                   (buffer-modified-p))))",
    );
    assert_eq!(result, r#"OK ("zzzbb" 2 2 t)"#);
}

#[test]
fn subst_char_in_region_records_per_character_undo_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (erase-buffer)
           (insert \"zzzaa\")
           (setq buffer-undo-list nil)
           (subst-char-in-region 1 6 ?a ?b)
           (list (buffer-string) buffer-undo-list))",
    );
    assert_eq!(
        result,
        r#"OK ("zzzbb" ((5 . 6) ("a" . -5) (4 . 5) ("a" . 4)))"#
    );
}

#[test]
fn subst_char_in_region_restarts_after_before_change_hook_removes_match_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (erase-buffer)
           (insert \"xa\")
           (let ((n 0) (events nil))
             (setq before-change-functions
                   (list (lambda (beg end)
                           (setq events
                                 (cons (list :before beg end (buffer-string))
                                       events))
                           (if (= n 0)
                               (progn
                                 (setq n 1)
                                 (goto-char 2)
                                 (delete-region 2 3)
                                 (insert \"c\"))))))
             (setq after-change-functions
                   (list (lambda (beg end old-len)
                           (setq events
                                 (cons (list :after beg end old-len
                                             (buffer-string))
                                       events)))))
             (subst-char-in-region 1 (point-max) ?a ?b)
             (list (buffer-string) (nreverse events))))",
    );
    assert_eq!(result, r#"OK ("xc" ((:before 2 3 "xa")))"#);
}

#[test]
fn subst_char_in_region_restarts_after_before_change_hook_inserts_earlier_match_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (erase-buffer)
           (insert \"xa\")
           (let ((n 0) (events nil))
             (setq before-change-functions
                   (list (lambda (beg end)
                           (setq events
                                 (cons (list :before beg end (buffer-string))
                                       events))
                           (if (= n 0)
                               (progn
                                 (setq n 1)
                                 (goto-char 1)
                                 (insert \"a\"))))))
             (setq after-change-functions
                   (list (lambda (beg end old-len)
                           (setq events
                                 (cons (list :after beg end old-len
                                             (buffer-string))
                                       events)))))
             (subst-char-in-region 1 (point-max) ?a ?b)
             (list (buffer-string) (nreverse events))))",
    );
    assert_eq!(
        result,
        r#"OK ("bxa" ((:before 2 3 "xa") (:after 1 2 1 "bxa")))"#
    );
}

#[test]
fn text_property_modification_hooks_run_before_text_delete() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (erase-buffer)
             (insert "abcd")
             (let ((events nil))
               (put-text-property
                2 4 'modification-hooks
                (list (lambda (beg end)
                        (setq events
                              (cons (list 'mod beg end
                                          (substring-no-properties (buffer-string)))
                                    events)))))
               (delete-region 2 3)
               (list (substring-no-properties (buffer-string))
                     (nreverse events))))"#,
    );
    assert_eq!(result, r#"OK ("acd" ((mod 2 3 "abcd")))"#);
}

#[test]
fn text_property_insert_hooks_run_after_text_insert() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (erase-buffer)
             (insert "ab")
             (let ((events nil))
               (put-text-property
                1 2 'insert-behind-hooks
                (list (lambda (beg end)
                        (setq events
                              (cons (list 'behind beg end
                                          (substring-no-properties (buffer-string)))
                                    events)))))
               (put-text-property
                2 3 'insert-in-front-hooks
                (list (lambda (beg end)
                        (setq events
                              (cons (list 'front beg end
                                          (substring-no-properties (buffer-string)))
                                    events)))))
               (goto-char 2)
               (insert "X")
               (list (substring-no-properties (buffer-string))
                     (nreverse events))))"#,
    );
    assert_eq!(
        result,
        r#"OK ("aXb" ((behind 2 3 "aXb") (front 2 3 "aXb")))"#
    );
}

#[test]
fn overlay_modification_after_phase_replays_before_recorded_hook_list() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (erase-buffer)
             (insert "abc")
             (setq hook-log nil)
             (fset 'neo-original-overlay-hook
                   (lambda (ov after beg end &optional old-len)
                     (setq hook-log
                           (cons (list 'original after beg end old-len) hook-log))
                     (if after
                         nil
                       (overlay-put ov 'modification-hooks
                                    (list 'neo-replacement-overlay-hook)))))
             (fset 'neo-replacement-overlay-hook
                   (lambda (_ov after beg end &optional old-len)
                     (setq hook-log
                           (cons (list 'replacement after beg end old-len)
                                 hook-log))))
             (let ((ov (make-overlay 1 4)))
               (overlay-put ov 'modification-hooks
                            (list 'neo-original-overlay-hook))
               (delete-region 2 3)
               (nreverse hook-log)))"#,
    );
    assert_eq!(result, "OK ((original nil 2 3 nil) (original t 2 2 1))");
}

#[test]
fn overlay_modification_hooks_are_collected_after_before_change_functions() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (erase-buffer)
             (insert "abc")
             (setq hook-log nil)
             (fset 'neo-old-overlay-hook
                   (lambda (_ov after beg end &optional old-len)
                     (setq hook-log
                           (cons (list 'old after beg end old-len) hook-log))))
             (fset 'neo-new-overlay-hook
                   (lambda (_ov after beg end &optional old-len)
                     (setq hook-log
                           (cons (list 'new after beg end old-len) hook-log))))
             (let ((ov (make-overlay 1 4)))
               (overlay-put ov 'modification-hooks
                            (list 'neo-old-overlay-hook))
               (setq before-change-functions
                     (list (lambda (_beg _end)
                             (overlay-put ov 'modification-hooks
                                          (list 'neo-new-overlay-hook)))))
               (delete-region 2 3)
               (nreverse hook-log)))"#,
    );
    assert_eq!(result, "OK ((new nil 2 3 nil) (new t 2 2 1))");
}

#[test]
fn deleting_before_a_large_overlay_suffix_does_not_enumerate_every_overlay() {
    crate::test_utils::init_test_tracing();
    crate::buffer::overlay::reset_overlay_full_enumeration_visit_count();

    let result = eval_one(
        r#"(progn
             (insert (make-string 10050 ?x))
             (let ((index 0))
               (while (< index 4000)
               (let ((start (+ 100 (* index 2))))
                   (make-overlay start (1+ start)))
                 (setq index (1+ index))))
             (goto-char 1)
             (insert "abc")
             (delete-region 1 4)
             (length (overlays-at 100)))"#,
    );

    assert_eq!(result, "OK 1");
    assert_eq!(
        crate::buffer::overlay::overlay_full_enumeration_visit_count(),
        0,
        "a localized deletion must not walk all live overlays"
    );
}

#[test]
fn deletion_evaporates_only_collapsed_boundary_overlays_via_category() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (insert "abcdef")
             (put 'neo-evaporating-category 'evaporate t)
             (let ((collapsed (make-overlay 2 3))
                   (unrelated (make-overlay 5 5)))
               (overlay-put collapsed 'category 'neo-evaporating-category)
               (overlay-put unrelated 'category 'neo-evaporating-category)
               (delete-region 2 3)
               (list (overlay-buffer collapsed)
                     (overlay-start collapsed)
                     (null (overlay-buffer unrelated)))))"#,
    );
    assert_eq!(result, "OK (nil nil nil)");
}

#[test]
fn before_change_functions_reset_to_nil_on_error() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq before-change-functions
                 (list (lambda (_beg _end) (error \"boom\"))))
           (condition-case _ (insert \"x\") (error nil))
           before-change-functions)",
    );
    assert_eq!(result, "OK nil");
}

#[test]
fn symbol_operations() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(defvar x 42)
         (boundp 'x)
         (symbol-value 'x)
         (put 'x 'doc \"A variable\")
         (get 'x 'doc)",
    );
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK 42");
    assert_eq!(results[4], r#"OK "A variable""#);
}

// -- Buffer operations -------------------------------------------------

#[test]
fn buffer_create_and_switch() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"test-buf\")
         (set-buffer \"test-buf\")
         (buffer-name)
         (bufferp (current-buffer))",
    );
    assert!(results[0].starts_with("OK #<buffer"));
    assert!(results[1].starts_with("OK #<buffer"));
    assert_eq!(results[2], r#"OK "test-buf""#);
    assert_eq!(results[3], "OK t");
}

#[test]
fn buffer_insert_and_point() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"ed\")
         (set-buffer \"ed\")
         (insert \"hello\")
         (point)
         (goto-char 1)
         (point)
         (buffer-string)
         (point-min)
         (point-max)",
    );
    assert_eq!(results[3], "OK 6"); // after inserting "hello", point is 6 (1-based)
    assert_eq!(results[5], "OK 1"); // after goto-char 1
    assert_eq!(results[6], r#"OK "hello""#);
    assert_eq!(results[7], "OK 1"); // point-min
    assert_eq!(results[8], "OK 6"); // point-max
}

#[test]
fn buffer_delete_region() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"del\")
         (set-buffer \"del\")
         (insert \"abcdef\")
         (delete-region 2 5)
         (buffer-string)",
    );
    assert_eq!(results[4], r#"OK "aef""#);
}

#[test]
fn buffer_delete_and_extract_region_accepts_live_markers_after_insertions() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (insert \"abcdef\")
           (let ((start (copy-marker 2))
                 (end (copy-marker 5 t)))
             (goto-char 1)
             (insert \"X\")
             (list (delete-and-extract-region start end)
                   (buffer-string))))",
    );
    assert_eq!(results[0], r#"OK ("bcd" "Xaef")"#);
}

#[test]
fn buffer_delete_and_extract_region_preserves_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (set-buffer-multibyte nil)
           (insert-byte 255 1)
           (let ((s (delete-and-extract-region 1 2)))
             (list (multibyte-string-p s)
                   (string-bytes s)
                   (aref s 0)
                   (buffer-size))))",
    );
    assert_eq!(results[0], "OK (nil 1 255 0)");
}

#[test]
fn buffer_erase() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"era\")
        (set-buffer \"era\")
         (insert \"stuff\")
         (erase-buffer)
         (buffer-string)
         (buffer-size)",
    );
    assert_eq!(results[4], r#"OK """#);
    assert_eq!(results[5], "OK 0");
}

#[test]
fn buffer_mutation_read_only_shape_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(list
           (with-temp-buffer
             (insert \"abc\")
             (setq buffer-read-only t)
             (condition-case err
                 (delete-region 1 2)
               (error (list (car err) (bufferp (car (cdr err)))))))
           (with-temp-buffer
             (insert \"abc\")
             (setq buffer-read-only t)
             (condition-case err
                 (delete-and-extract-region 1 2)
               (error (list (car err) (bufferp (car (cdr err)))))))
           (with-temp-buffer
             (insert \"abc\")
             (setq buffer-read-only t)
             (condition-case err
                 (erase-buffer)
               (error (list (car err) (bufferp (car (cdr err))))))))",
    );
    assert_eq!(
        results[0],
        "OK ((buffer-read-only t) (buffer-read-only t) (buffer-read-only t))"
    );
}

#[test]
fn buffer_mutation_read_only_noop_cases_match_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(list
           (with-temp-buffer
             (setq buffer-read-only t)
             (delete-region 1 1))
           (with-temp-buffer
             (setq buffer-read-only t)
             (delete-and-extract-region 1 1))
           (with-temp-buffer
             (narrow-to-region 1 1)
             (setq buffer-read-only t)
             (erase-buffer)
             (list (point-min) (point-max) (buffer-string))))",
    );
    assert_eq!(results[0], r#"OK (nil "" (1 1 ""))"#);
}

#[test]
fn match_string_preserves_unibyte_raw_bytes_for_buffer_searches() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (set-buffer-multibyte nil)
           (insert-byte 255 1)
           (goto-char 1)
           (re-search-forward \".\")
           (let ((s (match-string 0)))
             (list (multibyte-string-p s)
                   (string-bytes s)
                   (aref s 0))))",
    );
    assert_eq!(results[0], "OK (nil 1 255)");
}

#[test]
fn buffer_narrowing() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"nar\")
         (set-buffer \"nar\")
         (insert \"hello world\")
         (narrow-to-region 7 12)
         (buffer-string)
         (widen)
         (buffer-string)",
    );
    assert_eq!(results[4], r#"OK "world""#);
    assert_eq!(results[6], r#"OK "hello world""#);
}

#[test]
fn buffer_narrowing_accepts_live_marker_bounds_after_insertions() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (insert \"abcdef\")
           (let ((start (copy-marker 2))
                 (end (copy-marker 5 t)))
             (goto-char 1)
             (insert \"X\")
             (narrow-to-region start end)
             (list (point-min) (point-max) (buffer-string))))",
    );
    assert_eq!(results[0], r#"OK (3 6 "bcd")"#);
}

#[test]
fn buffer_modified_p() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"mod\")
         (set-buffer \"mod\")
         (buffer-modified-p)
         (insert \"x\")
         (buffer-modified-p)
         (set-buffer-modified-p nil)
         (buffer-modified-p)",
    );
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[4], "OK t");
    assert_eq!(results[6], "OK nil");
}

#[test]
fn buffer_mark() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(get-buffer-create \"mk\")
         (set-buffer \"mk\")
         (insert \"hello\")
         (set-mark 3)
         (mark)",
    );
    assert_eq!(results[4], "OK 3");
}

#[test]
fn buffer_with_current_buffer() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"a\")
         (get-buffer-create \"b\")
         (set-buffer \"a\")
         (insert \"in-a\")
         (save-current-buffer
           (set-buffer \"b\")
           (insert \"in-b\")
           (buffer-string))
         (buffer-name)
         (buffer-string)",
    );
    // save-current-buffer+set-buffer should switch to b, insert, get string, then restore a
    assert_eq!(results[4], r#"OK "in-b""#);
    assert_eq!(results[5], r#"OK "a""#); // current buffer restored
    assert_eq!(results[6], r#"OK "in-a""#); // a's content unchanged
}

#[test]
fn buffer_save_excursion() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"se\")
         (set-buffer \"se\")
         (insert \"abcdef\")
         (goto-char 3)
         (save-excursion
           (goto-char 1)
           (insert \"X\"))
         (point)",
    );
    // save-excursion restores point to the marker, which shifted from 3 to 4
    // because "X" was inserted before it at position 1.
    assert_eq!(results[5], "OK 4");
}

#[test]
fn buffer_save_excursion_marker_survives_exact_gc() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"se-gc\")
         (set-buffer \"se-gc\")
         (erase-buffer)
         (insert \"abcdef\")
         (goto-char (point-max))
         (save-excursion
           (garbage-collect)
           (goto-char 3)
           (insert \"XXX\"))
         (list (point) (buffer-string))",
    );
    assert_eq!(results[6], "OK (10 \"abXXXcdef\")");
}

#[test]
fn buffer_save_excursion_tracks_marker_through_edits() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (insert \"0123456789\")
           (goto-char 6)
           (let ((before-point (point)))
             (save-excursion
               (goto-char 3)
               (insert \"XXX\")
               (goto-char 12)
               (delete-char 2))
             (list before-point (point) (buffer-string))))",
    );
    assert_eq!(results[0], "OK (6 9 \"01XXX234567\")");
}

#[test]
fn save_excursion_unlinks_point_markers_after_unwind() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("se-marker-chain");
    ev.buffers.set_current(buffer_id);
    let result = ev.eval_str(
        r#"(progn
             (insert "abcdef")
             (let ((i 0))
               (while (< i 10)
                 (save-excursion
                   (goto-char 1)
                   (insert "x"))
                 (setq i (1+ i)))))"#,
    );
    assert_eq!(format_eval_result(&result), "OK nil");

    let buffer = ev.buffers.get(buffer_id).expect("test buffer should exist");
    assert_eq!(buffer.marker_chain_len(), 0);
}

#[test]
fn insert_before_markers_advances_before_markers_at_point() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (insert \"ab\")
           (goto-char 1)
           (let ((m (copy-marker (point))))
             (insert-before-markers \"X\")
             (list (buffer-string) (marker-position m))))",
    );
    assert_eq!(results[0], r#"OK ("Xab" 2)"#);
}

#[test]
fn keymap_prompt_resolves_an_uninterned_symbol_function_keymap() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let ((map (make-symbol "menu-function")))
             (fset map (make-sparse-keymap "Submenu"))
             (list (keymapp map) (keymap-prompt map)))"#,
    );
    assert_eq!(results[0], r#"OK (t "Submenu")"#);
}

#[test]
fn insert_read_only_shape_and_noop_cases_match_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(list
           (with-temp-buffer
             (setq buffer-read-only t)
             (condition-case err
                 (insert \"x\")
               (error (list (car err) (bufferp (car (cdr err)))))))
           (with-temp-buffer
             (setq buffer-read-only t)
             (condition-case err
                 (insert-char ?x 1)
               (error (list (car err) (bufferp (car (cdr err)))))))
           (with-temp-buffer
             (setq buffer-read-only t)
             (condition-case err
                 (insert-and-inherit \"x\")
               (error (list (car err) (bufferp (car (cdr err)))))))
           (with-temp-buffer
             (setq buffer-read-only t)
             (condition-case err
                 (insert-before-markers-and-inherit \"x\")
               (error (list (car err) (bufferp (car (cdr err)))))))
           (with-temp-buffer
             (setq buffer-read-only t)
             (condition-case err
                 (insert-byte 120 1)
               (error (list (car err) (bufferp (car (cdr err)))))))
           (with-temp-buffer
             (setq buffer-read-only t)
             (list (insert)
                   (insert \"\")
                   (insert-char ?x 0)
                   (insert-byte 120 0)
                   (insert-and-inherit)
                   (insert-and-inherit \"\")
                   (insert-before-markers-and-inherit)
                   (insert-before-markers-and-inherit \"\")
                   (buffer-string))))",
    );
    assert_eq!(
        results[0],
        r#"OK ((buffer-read-only t) (buffer-read-only t) (buffer-read-only t) (buffer-read-only t) (buffer-read-only t) (nil nil nil nil nil nil nil nil ""))"#
    );
}

#[test]
fn delete_read_only_text_property_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(list
           (with-temp-buffer
             (insert "abc")
             (put-text-property 1 2 'read-only t)
             (goto-char 1)
             (condition-case err
                 (progn (delete-char 1) (list :ok (buffer-string)))
               (error (list (car err) (cdr err) (buffer-substring-no-properties (point-min) (point-max))))))
           (with-temp-buffer
             (insert "abc")
             (put-text-property 1 2 'read-only "locked")
             (goto-char 1)
             (condition-case err
                 (progn (delete-region 1 2) (list :ok (buffer-string)))
               (error (list (car err) (cdr err) (buffer-substring-no-properties (point-min) (point-max))))))
           (with-temp-buffer
             (insert "abc")
             (put-text-property 1 2 'read-only t)
             (goto-char 2)
             (condition-case err
                 (progn (delete-char -1) (list :ok (buffer-string)))
               (error (list (car err) (cdr err) (buffer-substring-no-properties (point-min) (point-max))))))
           (with-temp-buffer
             (insert "abcdef")
             (put-text-property 1 7 'read-only t)
             (goto-char 5)
             (condition-case err
                 (progn (delete-char -1) (list :ok (buffer-string)))
               (error (list (car err) (cdr err) (buffer-substring-no-properties (point-min) (point-max)))))))"#,
    );
    assert_eq!(
        results[0],
        r#"OK ((text-read-only nil "abc") (text-read-only ("locked") "abc") (text-read-only nil "abc") (text-read-only nil "abcdef"))"#
    );
}

#[test]
fn lexical_inhibit_read_only_binding_overrides_buffer_read_only() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.set_lexical_binding(true);
    let result = ev.eval_str(
        "(with-temp-buffer
           (setq buffer-read-only t)
           (let ((inhibit-read-only t))
             (insert \"ok\")
             (buffer-string)))",
    );
    assert_eq!(format_eval_result(&result), r#"OK "ok""#);
}

#[test]
fn bootstrap_display_warning_does_not_signal_buffer_read_only() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        "(condition-case err
             (progn
               (display-warning 'emacs \"hello from neomacs startup\")
               'ok)
           (error (list 'error (car err))))",
    );
    assert_eq!(result, "OK ok");
}

#[test]
fn insert_char_nil_count_defaults_to_one_with_inherit() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (insert \"ab\")
           (put-text-property 2 3 'face 'bold)
           (insert-char ?X nil t)
           (list (buffer-substring-no-properties (point-min) (point-max))
                 (get-text-property 3 'face)))",
    );
    assert_eq!(results[0], r#"OK ("abX" bold)"#);
}

#[test]
fn insert_inherit_variants_match_gnu_property_and_marker_semantics() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(list
           (with-temp-buffer
             (insert \"a\")
             (put-text-property 1 2 'face 'bold)
             (insert-and-inherit (propertize \"X\" 'face 'italic 'mouse-face 'highlight))
             (list (buffer-substring-no-properties (point-min) (point-max))
                   (get-text-property 2 'face)
                   (get-text-property 2 'mouse-face)))
           (with-temp-buffer
             (insert \"ab\")
             (put-text-property 1 2 'face 'bold)
             (goto-char 2)
             (let ((m (copy-marker (point))))
               (insert-before-markers-and-inherit
                (propertize \"X\" 'mouse-face 'highlight))
               (list (buffer-substring-no-properties (point-min) (point-max))
                     (marker-position m)
                     (get-text-property 2 'face)
                     (get-text-property 2 'mouse-face)))))",
    );
    assert_eq!(
        results[0],
        r#"OK (("aX" bold highlight) ("aXb" 3 bold highlight))"#
    );
}

#[test]
fn insert_buffer_substring_preserves_source_text_properties() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((src (get-buffer-create "*eval-sub-src*"))
                     (dst (get-buffer-create "*eval-sub-dst*")))
                 (save-current-buffer (set-buffer src)
                   (erase-buffer)
                   (insert "abcXYZ")
                   (put-text-property 2 5 'face 'bold))
                 (set-buffer dst)
                 (erase-buffer)
                 (insert-buffer-substring src 2 5)
                 (let ((sub (save-current-buffer (set-buffer src)
                              (buffer-substring 2 5)))
                       (copied (buffer-string)))
                   (list sub
                         (get-text-property 1 'face sub)
                         copied
                         (get-text-property 1 'face copied))))"#,
        ),
        r#"OK (#("bcX" 0 3 (face bold)) bold #("bcX" 0 3 (face bold)) bold)"#
    );
}

#[test]
fn compare_buffer_substrings_respects_case_fold_search() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((left (get-buffer-create "*eval-cmp-left*"))
                     (right (get-buffer-create "*eval-cmp-right*")))
                 (save-current-buffer (set-buffer left)
                   (erase-buffer)
                   (insert "Abc"))
                 (save-current-buffer (set-buffer right)
                   (erase-buffer)
                   (insert "aBc"))
                 (list
                  (let ((case-fold-search nil))
                    (compare-buffer-substrings left nil nil right nil nil))
                  (let ((case-fold-search t))
                    (compare-buffer-substrings left nil nil right nil nil))
                  (let ((case-fold-search t))
                    (compare-buffer-substrings left 1 2 right 1 3))))"#,
        ),
        "OK (-1 0 -2)"
    );
}

#[test]
fn field_builtins_match_gnu_property_boundary_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (list
                  (progn
                    (insert "abcdefg")
                    (put-text-property 2 5 'field 'left)
                    (put-text-property 5 8 'field 'right)
                    (put-text-property 2 5 'face 'bold)
                    (let ((s (field-string 3)))
                      (list
                       (list (field-beginning 3)
                             (field-end 3)
                             (field-string-no-properties 3))
                       (get-text-property 1 'face s)
                       (list (field-beginning 5)
                             (field-beginning 5 t)
                             (field-end 5)
                             (field-end 5 t))
                       (progn
                         (delete-field 3)
                         (list
                          (buffer-substring-no-properties (point-min) (point-max))
                          (get-text-property 2 'field))))))
                  (progn
                    (erase-buffer)
                    (insert "abcdefg")
                    (put-text-property 2 4 'field 'left)
                    (put-text-property 4 5 'field 'boundary)
                    (put-text-property 5 8 'field 'right)
                    (list (field-beginning 4)
                          (field-beginning 4 t)
                          (field-end 4)
                          (field-end 4 t)
                          (field-beginning 5)
                          (field-beginning 5 t)
                          (field-end 5)
                          (field-end 5 t)))))"#,
        ),
        r#"OK (((2 5 "bcd") bold (2 2 5 8) ("aefg" right)) (2 2 4 8 4 2 5 8))"#
    );
}

#[test]
fn constrain_to_field_matches_gnu_boundary_and_capture_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (list
                  (progn
                    (insert "abcdefg")
                    (put-text-property 2 5 'field 'left)
                    (put-text-property 5 8 'field 'right)
                    (put-text-property 3 4 'capture t)
                    (goto-char 7)
                    (list
                     (constrain-to-field 7 3)
                     (constrain-to-field 7 5)
                     (constrain-to-field 7 5 t)
                     (progn
                       (goto-char 7)
                       (list (constrain-to-field nil 3) (point)))
                     (constrain-to-field 7 3 nil nil 'capture)
                     (constrain-to-field 7 2 nil nil 'capture)))
                  (progn
                    (erase-buffer)
                    (insert "ab\ncd\nef")
                    (put-text-property 1 4 'field 'top)
                    (put-text-property 4 9 'field 'bottom)
                    (list
                     (constrain-to-field 6 2 nil t)
                     (constrain-to-field 6 2 nil nil)
                     (constrain-to-field 6 4 t nil)))))"#,
        ),
        r#"OK ((5 5 7 (5 5) 5 2) (4 4 6))"#
    );
}

#[test]
fn constrain_to_field_honors_dynamic_inhibit_field_text_motion_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (insert "aa" (propertize "bb" 'field 'f) "cc\nxx")
                 (goto-char 4)
                 (list
                  (line-beginning-position)
                  (line-end-position)
                  (let ((inhibit-field-text-motion t))
                    (list
                     (line-beginning-position)
                     (line-end-position)))
                  (let ((inhibit-field-text-motion t))
                    (goto-char 4)
                    (move-beginning-of-line nil)
                    (point))
                  (let ((inhibit-field-text-motion t))
                    (goto-char 4)
                    (move-end-of-line nil)
                    (point))))"#,
        ),
        r#"OK (3 5 (1 7) 1 7)"#
    );
}

#[test]
fn replace_region_contents_preserves_source_properties_and_rejects_self_buffer() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((src (get-buffer-create "*rrc-src*"))
                       (s (propertize "CD" 'face 'bold)))
                   (save-current-buffer (set-buffer src)
                     (erase-buffer)
                     (insert "1234")
                     (put-text-property 2 4 'face 'italic))
                   (list
                    (progn
                      (erase-buffer)
                      (insert "abZZef")
                      (replace-region-contents 3 5 s)
                      (list
                       (buffer-substring-no-properties 1 (point-max))
                       (get-text-property 3 'face)))
                    (progn
                      (erase-buffer)
                      (insert "abZZef")
                      (replace-region-contents 3 5 (vector src 2 4))
                      (list
                       (buffer-substring-no-properties 1 (point-max))
                       (get-text-property 3 'face)
                       (get-text-property 4 'face)))
                    (condition-case err
                        (replace-region-contents 3 5 (current-buffer))
                      (error (list (car err) (car (cdr err))))))))"#,
        ),
        r#"OK (("abCDef" bold) ("ab23ef" italic italic) (error "Cannot replace a buffer with itself"))"#
    );
}

#[test]
fn subst_char_in_region_read_only_shape_and_noop_cases_match_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(list
           (with-temp-buffer
             (insert \"abc\")
             (setq buffer-read-only t)
             (condition-case err
                 (subst-char-in-region 1 2 ?a ?b)
               (error (list (car err) (bufferp (car (cdr err)))))))
           (with-temp-buffer
             (insert \"abc\")
             (setq buffer-read-only t)
             (list (subst-char-in-region 1 1 ?a ?b)
                   (subst-char-in-region 1 4 ?z ?b)
                   (buffer-substring-no-properties (point-min) (point-max)))))",
    );
    assert_eq!(results[0], r#"OK ((buffer-read-only t) (nil nil "abc"))"#);
}

#[test]
fn buffer_undo_list_reflects_recorded_edits() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (setq buffer-undo-list nil)
           (insert \"Hello\")
           (let ((after-insert (not (null buffer-undo-list))))
             (undo-boundary)
             (insert \" World\")
             (undo-boundary)
             (delete-region 1 6)
             (undo-boundary)
             (list after-insert
                   (not (null buffer-undo-list))
                   buffer-undo-list)))",
    );
    assert_eq!(
        results[0],
        "OK (t t (nil (\"Hello\" . 1) 12 nil (6 . 12) nil (1 . 6) (t . 0)))"
    );
}

#[test]
fn let_bound_buffer_undo_list_suppresses_text_property_undo_records() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (setq buffer-undo-list nil)
           (insert \"abc\")
           (let ((before buffer-undo-list)
                 during after)
             (let ((buffer-undo-list t))
               (put-text-property 1 2 'fontified t)
               (setq during (list buffer-undo-list
                                  (get-text-property 1 'fontified))))
             (setq after (list (eq buffer-undo-list before)
                               buffer-undo-list))
             (list during after)))",
    );
    assert_eq!(results[0], "OK ((t t) (t ((1 . 4) (t . 0))))");
}

#[test]
fn char_primitives_respect_narrowing() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (insert \"Hello, 世界\")
           (narrow-to-region 3 8)
           (goto-char (point-min))
           (list (following-char)
                 (preceding-char)
                 (char-after (point-min))
                 (char-before (point-min))))",
    );
    assert_eq!(results[0], "OK (108 0 108 nil)");
}

#[test]
fn delete_char_respects_narrowing_boundaries() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (insert \"abc\")
           (narrow-to-region 1 2)
           (list (progn
                   (goto-char (point-max))
                   (condition-case err
                       (delete-char 1)
                     (error (car err))))
                 (progn
                   (goto-char (point-min))
                   (condition-case err
                       (delete-char -1)
                     (error (car err))))))",
    );
    assert_eq!(results[0], "OK (end-of-buffer beginning-of-buffer)");
}

#[test]
fn navigation_predicates_and_line_positions_respect_narrowing() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (insert \"wx\nab\ncd\")
           (narrow-to-region 4 6)
           (goto-char (point-min))
           (list (list (bobp) (eobp) (bolp) (eolp)
                       (line-beginning-position) (line-end-position))
                 (progn
                   (goto-char (point-max))
                   (list (bobp) (eobp) (bolp) (eolp)
                         (line-beginning-position) (line-end-position)))))",
    );
    assert_eq!(results[0], "OK ((t nil t nil 4 6) (nil t nil t 4 6))");
}

#[test]
fn line_position_optional_argument_matches_gnu_current_rules() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-buffer
           (insert \"a\nbb\nccc\")
           (goto-char 2)
           (list (line-beginning-position 2)
                 (line-end-position 2)
                 (line-beginning-position 3)
                 (line-end-position 3)))",
    );
    assert_eq!(results[0], "OK (3 5 6 9)");
}

#[test]
fn save_match_data_restores_after_success_and_error() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(set-match-data '(1 2))
         (save-match-data (set-match-data '(3 4)) (match-data))
         (match-data)
         (condition-case err
             (save-match-data
               (set-match-data '(5 6))
               (signal 'error '(\"boom\")))
           (error (car err)))
         (match-data)",
    );
    assert_eq!(results[1], "OK (3 4)");
    assert_eq!(results[2], "OK (1 2)");
    assert_eq!(results[3], "OK error");
    assert_eq!(results[4], "OK (1 2)");
}

#[test]
fn save_mark_and_excursion_restores_mark_and_mark_active() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(save-current-buffer
           (let ((b (get-buffer-create \"smx-eval\")))
             (set-buffer b)
             (erase-buffer)
             (insert \"abcdef\")
             (goto-char 2)
             (set-mark 5)
             (setq mark-active nil)
             (let ((before (list (point) (mark) mark-active)))
               (save-mark-and-excursion
                 (goto-char 4)
                 (set-mark 3)
                 (setq mark-active t))
               (list before (point) (mark) mark-active))))",
    );
    assert_eq!(results[0], "OK ((2 5 nil) 2 5 nil)");
}

#[test]
fn save_window_excursion_restores_selected_window_on_success_and_error() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(let ((w1 (selected-window))
               (w2 (split-window)))
           (prog1
               (list
                (save-window-excursion
                  (select-window w2)
                  (eq (selected-window) w2))
                (eq (selected-window) w1))
             (ignore-errors (delete-window w2))))
         (let ((w1 (selected-window))
               (w2 (split-window)))
           (prog1
               (list
                (condition-case err
                    (save-window-excursion
                      (select-window w2)
                      (error \"boom\"))
                  (error (car err)))
                (eq (selected-window) w1))
             (ignore-errors (delete-window w2))))",
    );
    assert_eq!(results[0], "OK (t t)");
    assert_eq!(results[1], "OK (error t)");
}

#[test]
fn save_window_excursion_restores_window_layout_after_split() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(let ((before (length (window-list))))
           (list
            (let ((wconfig (current-window-configuration)))
              (unwind-protect
                  (progn
                    (split-window-internal (selected-window) nil nil nil)
                    (length (window-list)))
                (set-window-configuration wconfig)))
            (length (window-list))
            before))",
    );
    assert_eq!(results[0], "OK (2 1 1)");
}

#[test]
fn set_window_configuration_reconciles_restored_batch_frame_geometry() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buffer_id);
    let frame_id = ev.frames.create_frame("F1", 80, 25, buffer_id);
    {
        let frame = ev.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        if let Some(minibuffer) = frame.minibuffer_leaf.as_mut() {
            let bounds = *minibuffer.bounds();
            minibuffer.set_bounds(crate::window::Rect::new(
                bounds.x,
                bounds.y,
                bounds.width,
                1.0,
            ));
        }
        frame.set_parameter(Value::symbol("menu-bar-lines"), Value::fixnum(1));
        frame.sync_menu_bar_height_from_parameters();
    }

    let result = ev
        .eval_str(
            // `window-edges' and `window-pixel-edges' are lisp/window.el:3839
            // and :3922 and have no subr any more (DIVERGENCES.md 154).  The
            // pixel edges are `(LEFT TOP LEFT+WIDTH TOP+HEIGHT)' over the C
            // primitives the Lisp reads, with a zero internal border.
            r#"(progn
                 ;; lisp/window.el:3839 `window-edges' with PIXELWISE non-nil,
                 ;; written out over the C primitives its body reads.  A batch
                 ;; frame's `frame-internal-border-width' is 0 and its character
                 ;; cell is one pixel, so this is also the non-pixelwise answer.
                 ;; `defun' is a `subr.el' macro, so a bare evaluator -- GNU
                 ;; before loadup.el -- has only `fset'.
                 (fset 'pw61-pixel-edges
                       (lambda (w)
                         (let ((left (+ (window-pixel-left w)
                                        (frame-internal-border-width)))
                               (top (+ (window-pixel-top w)
                                       (frame-internal-border-width))))
                           (list left top
                                 (+ left (window-pixel-width w))
                                 (+ top (window-pixel-height w))))))
                 (let ((before
                        (list (window-total-height (frame-root-window))
                              (pw61-pixel-edges (frame-root-window)))))
                   (let ((configuration (current-window-configuration)))
                     (unwind-protect
                         (split-window-internal (selected-window) nil nil nil)
                       (set-window-configuration configuration)))
                   (list before
                         (window-total-height (frame-root-window))
                         (pw61-pixel-edges (frame-root-window))
                         (pw61-pixel-edges (frame-root-window)))))"#,
        )
        .expect("set-window-configuration should restore the frame");

    assert_eq!(
        crate::emacs_core::format_eval_result(&Ok(result)),
        "OK ((24 (0 0 80 24)) 23 (0 1 80 24) (0 1 80 24))"
    );
}

#[test]
fn set_window_configuration_honors_dont_set_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let first_buffer = ev.buffers.create_buffer("*first-frame*");
    let second_buffer = ev.buffers.create_buffer("*second-frame*");
    ev.buffers.set_current(first_buffer);
    let first_frame = ev.frames.create_frame("F1", 80, 24, first_buffer);
    let second_frame = ev.frames.create_frame("F2", 80, 24, second_buffer);
    ev.frames.select_frame(first_frame);
    ev.obarray
        .set_symbol_value("test-first-frame", Value::make_frame(first_frame.0));
    ev.obarray
        .set_symbol_value("test-second-frame", Value::make_frame(second_frame.0));

    let result = ev
        .eval_str(
            r##"(let ((configuration
                         (current-window-configuration test-first-frame)))
                   (select-frame test-second-frame)
                   (set-window-configuration configuration)
                   (let ((selected-saved-frame
                          (eq (selected-frame) test-first-frame)))
                     (select-frame test-second-frame)
                     (set-window-configuration configuration t)
                     (list selected-saved-frame
                           (eq (selected-frame) test-second-frame))))"##,
        )
        .expect("set-window-configuration should honor DONT-SET-FRAME");

    assert_eq!(format!("{result}"), "(t t)");
}

#[test]
fn set_window_configuration_honors_dont_set_miniwindow() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let root_buffer = ev.buffers.create_buffer("*root*");
    ev.buffers.set_current(root_buffer);
    ev.frames.create_frame("F1", 80, 24, root_buffer);

    let result = ev
        .eval_str(
            r##"(let* ((miniwindow (minibuffer-window))
                        (saved-buffer (window-buffer miniwindow))
                        (replacement (get-buffer-create " *replacement-mini*"))
                        (configuration (current-window-configuration)))
                   (unwind-protect
                       (progn
                         (set-window-buffer miniwindow replacement)
                         (set-window-configuration configuration nil t)
                         (let ((kept-replacement
                                (eq (window-buffer miniwindow) replacement)))
                           (set-window-configuration configuration)
                           (list kept-replacement
                                 (eq (window-buffer miniwindow) saved-buffer))))
                     (kill-buffer replacement)))"##,
        )
        .expect("set-window-configuration should honor DONT-SET-MINIWINDOW");

    assert_eq!(format!("{result}"), "(t t)");
}

#[test]
fn save_window_excursion_restores_current_buffer_separate_from_selected_window() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let* ((current (get-buffer-create "neo-current"))
                  (shown (get-buffer-create "neo-shown")))
             (unwind-protect
                 (progn
                   (switch-to-buffer shown)
                   (set-buffer current)
                   (list
                    (buffer-name (current-buffer))
                    (buffer-name (window-buffer (selected-window)))
                    (save-window-excursion
                      (dolist (buffer (buffer-list))
                        (with-current-buffer buffer nil))
                      (list (buffer-name (current-buffer))
                            (buffer-name (window-buffer (selected-window)))))
                    (buffer-name (current-buffer))
                    (buffer-name (window-buffer (selected-window)))))
               (ignore-errors (kill-buffer current))
               (ignore-errors (kill-buffer shown))))"#,
    );
    assert_eq!(
        results[0],
        r#"OK ("neo-current" "neo-shown" ("neo-current" "neo-shown") "neo-current" "neo-shown")"#
    );
}

#[test]
fn save_window_excursion_with_help_window_restores_original_window_buffer() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let* ((orig (generate-new-buffer "*neo-help-orig*"))
                  (help (get-buffer-create "*neo-help-test*")))
             (unwind-protect
                 (progn
                   (switch-to-buffer orig)
                   (with-current-buffer orig
                     (erase-buffer)
                     (insert "alpha\nbeta\n"))
                   (list
                    (buffer-name (current-buffer))
                    (buffer-name (window-buffer (selected-window)))
                    (save-window-excursion
                      (save-excursion
                        (help--window-setup
                         help
                         (lambda ()
                           (with-current-buffer standard-output
                             (insert "help body")))))
                      (list
                       (buffer-name (current-buffer))
                       (buffer-name (window-buffer (selected-window)))))
                    (buffer-name (current-buffer))
                    (buffer-name (window-buffer (selected-window)))))
               (ignore-errors (kill-buffer help))
               (ignore-errors (kill-buffer orig))))"#,
    );
    assert_eq!(
        results[0],
        r#"OK ("*neo-help-orig*" "*neo-help-orig*" ("*neo-help-orig*" "*neo-help-orig*") "*neo-help-orig*" "*neo-help-orig*")"#
    );
}

#[test]
fn redisplay_does_not_copy_unrelated_current_buffer_point_into_selected_window() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let selected_buffer = ev.buffers.create_buffer("*redisplay-selected*");
    ev.buffers.set_current(selected_buffer);
    ev.buffers
        .get_mut(selected_buffer)
        .expect("selected buffer")
        .insert("selected buffer\n");
    ev.buffers
        .goto_buffer_emacs_byte_pos(selected_buffer, crate::buffer::EmacsBytePos::new(0))
        .expect("move selected buffer point");
    ev.frames.create_frame("F1", 960, 640, selected_buffer);
    let frame_id = ev.frames.selected_frame().expect("selected frame").id;
    let selected_window = ev.frames.get(frame_id).expect("frame").selected_window;

    let current_buffer = ev.buffers.create_buffer("*redisplay-current*");
    ev.buffers.set_current(current_buffer);
    ev.buffers
        .get_mut(current_buffer)
        .expect("current buffer")
        .insert("current buffer point should not affect the selected window\n");
    ev.buffers
        .goto_buffer_emacs_byte_pos(current_buffer, crate::buffer::EmacsBytePos::new(40))
        .expect("move current buffer point");

    ev.redisplay_fn = Some(Box::new(|_| {}));
    ev.redisplay();

    let selected_window_point = ev
        .frames
        .get(frame_id)
        .and_then(|frame| frame.find_window(selected_window))
        .and_then(|window| match window {
            crate::window::Window::Leaf { point, .. } => Some(*point),
            crate::window::Window::Internal { .. } => None,
        })
        .expect("selected window point");
    assert_eq!(selected_window_point, crate::buffer::LispCharPos1::ONE);
}

#[test]
fn save_window_excursion_defers_restore_redisplay_until_the_next_cycle() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buffer_id);
    ev.buffers
        .get_mut(buffer_id)
        .expect("scratch buffer")
        .insert("0123456789abcdefghijklmnopqrstuvwxyz");
    ev.frames.create_frame("F1", 960, 640, buffer_id);

    let redisplayed_points = Rc::new(RefCell::new(Vec::new()));
    let redisplayed_points_in_cb = Rc::clone(&redisplayed_points);
    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let point = crate::emacs_core::window_cmds::builtin_window_point(ev, vec![])
            .expect("window-point during redisplay");
        let Some(point) = point.as_fixnum() else {
            panic!("window-point should produce an integer during redisplay, got {point:?}");
        };
        redisplayed_points_in_cb.borrow_mut().push(point);
    }));

    ev.eval_str(
        r#"(let ((wconfig (current-window-configuration)))
           (unwind-protect
               (progn
                 (set-window-point (selected-window) 10)
                 (redisplay))
             (set-window-configuration wconfig)))"#,
    )
    .expect("save-window-excursion equivalent should evaluate");

    // GNU `Fset_window_configuration` calls `fset_redisplay`; it does not call
    // `redisplay` itself (`src/window.c:7786,8050`).  Therefore only the
    // explicit redisplay in the protected body has run so far.
    assert_eq!(*redisplayed_points.borrow(), vec![10]);

    // The restore is nevertheless dirty, and the next natural cycle publishes
    // it.  The configuration never recorded point in the buffer that was
    // current when it was saved, so the restore leaves the live point at 10
    // (`src/window.c:7692-7733,7978-7984`).
    ev.redisplay();
    assert_eq!(*redisplayed_points.borrow(), vec![10, 10]);
}

/// `current-window-configuration` does not record point in the buffer that was
/// current when it ran, so a round trip through it leaves point where the live
/// session put it -- `Fset_window_configuration` writes `old_point` back over
/// the saved-selected window (`src/window.c:7692-7733, 7978-7984`).  Verified
/// against GNU (emacs -Q --batch), which answers `(3 3)` here:
///
/// ```elisp
/// (let* ((w (selected-window)) (_ (goto-char 10))
///        (cfg (current-window-configuration)))
///   (goto-char 3)
///   (set-window-configuration cfg)
///   (list (window-point w) (point)))
/// ```
#[test]
fn current_window_configuration_does_not_save_point_in_the_current_buffer() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buffer_id);
    ev.buffers
        .get_mut(buffer_id)
        .expect("scratch buffer")
        .insert("0123456789abcdefghijklmnopqrstuvwxyz");
    ev.frames.create_frame("F1", 960, 640, buffer_id);

    let result = ev
        .eval_str(
            r#"(let* ((w (selected-window))
                (_ (goto-char 10))
                (cfg (current-window-configuration)))
           (goto-char 3)
           (set-window-configuration cfg)
           (list (window-point w) (point)))"#,
        )
        .expect("current-window-configuration round-trip should evaluate");
    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(3), Value::fixnum(3)])
    );
}

#[test]
fn set_window_configuration_commits_point_from_a_disconnected_nonselected_window() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        eval_one_with_frame(
            r##"(let ((original-buffer (current-buffer))
                       (extra-buffer
                        (get-buffer-create " *window-configuration-point*"))
                       (before nil))
                   (unwind-protect
                       (progn
                         (set-buffer extra-buffer)
                         (insert "abcdefghij")
                         (set-buffer original-buffer)
                         (let ((configuration (current-window-configuration))
                               (extra-window
                                (split-window-internal
                                 (selected-window) nil nil nil)))
                           (set-window-buffer extra-window extra-buffer)
                           (set-window-point extra-window 7)
                           (set-buffer extra-buffer)
                           (goto-char 2)
                           (setq before (point))
                           (set-buffer original-buffer)
                           (set-window-configuration configuration)
                           (set-buffer extra-buffer)
                           (list before (point) (window-live-p extra-window))))
                     (if (buffer-live-p extra-buffer)
                         (kill-buffer extra-buffer))))"##,
        ),
        "OK (2 7 nil)"
    );
}

#[test]
fn set_window_configuration_preserves_live_history_on_a_reused_window() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        eval_one_with_frame(
            r##"(let ((buffer-a (get-buffer-create "window-history-a"))
                       (buffer-b (get-buffer-create "window-history-b")))
                   (unwind-protect
                       (let ((window (selected-window)))
                         (set-window-buffer window buffer-a)
                         (set-window-prev-buffers window nil)
                         (let ((configuration (current-window-configuration)))
                           (set-window-buffer window buffer-b)
                           (set-window-configuration configuration)
                           (let ((history (window-prev-buffers window)))
                             (list (buffer-name (window-buffer window))
                                   (consp history)
                                   (if history
                                       (buffer-name (car (car history)))
                                     nil)))))
                     (if (buffer-live-p buffer-a) (kill-buffer buffer-a))
                     (if (buffer-live-p buffer-b) (kill-buffer buffer-b))))"##,
        ),
        "OK (\"window-history-a\" t \"window-history-b\")"
    );
}

#[test]
fn current_window_configuration_owns_independent_window_point_markers() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        eval_one_with_frame(
            r##"(let ((original-buffer (current-buffer))
                       (buffer (get-buffer-create " *configuration-marker*")))
                   (unwind-protect
                       (progn
                         (set-buffer buffer)
                         (insert "abcdefghij")
                         (set-buffer original-buffer)
                         (let ((window
                                (split-window-internal
                                 (selected-window) nil nil nil)))
                           (set-window-buffer window buffer)
                           (set-window-point window 3)
                           (let ((configuration
                                  (current-window-configuration)))
                             (set-window-point window 7)
                             (set-window-configuration configuration)
                             (let ((restored-point (window-point window)))
                               (set-buffer buffer)
                               (goto-char 1)
                               (insert "X")
                               (let ((shifted-point (window-point window)))
                                 (set-window-point window 8)
                                 (set-window-configuration configuration)
                                 (list restored-point shifted-point
                                       (window-point window)))))))
                     (if (buffer-live-p buffer) (kill-buffer buffer))))"##,
        ),
        "OK (3 4 4)"
    );
}

#[test]
fn set_window_configuration_keeps_a_reused_windows_live_buffer_when_saved_buffer_died() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        eval_one_with_frame(
            r##"(let ((buffer-a (get-buffer-create "window-killed-a"))
                       (buffer-b (get-buffer-create "window-killed-b"))
                       (window (selected-window)))
                   (unwind-protect
                       (progn
                         (set-window-buffer window buffer-a)
                         (let ((configuration
                                (current-window-configuration)))
                           (set-window-buffer window buffer-b)
                           (kill-buffer buffer-a)
                           (set-window-configuration configuration)
                           (list (buffer-name (window-buffer window))
                                 (eq window (selected-window))
                                 (window-live-p window))))
                     (if (buffer-live-p buffer-a) (kill-buffer buffer-a))
                     (if (buffer-live-p buffer-b) (kill-buffer buffer-b))))"##,
        ),
        "OK (\"window-killed-b\" t t)"
    );
}

#[test]
fn save_selected_window_restores_selected_window_on_success_and_error() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(let ((w1 (selected-window))
               (w2 (split-window)))
           (prog1
               (list
                (save-selected-window
                  (select-window w2)
                  (eq (selected-window) w2))
                (eq (selected-window) w1))
             (ignore-errors (delete-window w2))))
         (let ((w1 (selected-window))
               (w2 (split-window)))
           (prog1
               (list
                (condition-case err
                    (save-selected-window
                      (select-window w2)
                      (error \"boom\"))
                  (error (car err)))
                (eq (selected-window) w1))
             (ignore-errors (delete-window w2))))",
    );
    assert_eq!(results[0], "OK (t t)");
    assert_eq!(results[1], "OK (error t)");
}

#[test]
fn alist_get_comes_from_gnu_subr_runtime() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let ((foo '((a . 1) (b . 2))))
             (list
              (alist-get 'a foo)
              (alist-get 'z foo 'missing)
              (progn
                (setf (alist-get 'c foo) 3)
                foo)))"#,
    );
    assert_eq!(results[0], "OK (1 missing ((c . 3) (a . 1) (b . 2)))");
}

#[test]
fn with_local_quit_catches_quit_and_sets_quit_flag() {
    crate::test_utils::init_test_tracing();
    // GNU verified: subr.el's `with-local-quit` macro re-signals the
    // quit via `(eval '(ignore nil))` after setting `quit-flag`, so
    // a top-level evaluation of the form propagates the quit instead
    // of returning nil. Mirror GNU by wrapping the form in a
    // condition-case so we observe both: the propagated quit and the
    // quit-flag handling for the explicit inhibit-quit branch.
    let results = bootstrap_eval_all(
        "(setq quit-flag nil)
         (condition-case nil
             (with-local-quit
               (keyboard-quit)
               'after)
           (quit 'caught-quit))
         (setq quit-flag nil)
         (condition-case err
             (with-local-quit (error \"boom\"))
           (error (car err)))
         quit-flag
         (let ((inhibit-quit t)
               (quit-flag nil))
           (with-local-quit (keyboard-quit))
           (list inhibit-quit quit-flag))",
    );
    assert_eq!(results[1], "OK caught-quit");
    assert_eq!(results[3], "OK error");
    assert_eq!(results[4], "OK nil");
    assert_eq!(results[5], "OK (t t)");
}

#[test]
fn condition_case_does_not_catch_quit_with_error_clause() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(list
           (condition-case err
               (signal 'error \"boom\")
             (error 'got-error)
             (quit 'got-quit))
           (condition-case err
               (signal 'quit nil)
             (error 'got-error)
             (quit 'got-quit))
           (member 'error (get 'quit 'error-conditions))
           (get 'minibuffer-quit 'error-conditions))",
    );
    assert_eq!(
        results[0],
        "OK (got-error got-quit nil (minibuffer-quit quit))"
    );
}

#[test]
fn xdisp_prefix_variables_are_bound_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result =
        eval_one("(list (boundp 'wrap-prefix) wrap-prefix (boundp 'line-prefix) line-prefix)");
    assert_eq!(result, "OK (t nil t nil)");
}

#[test]
fn while_processes_quit_flag_without_loop_local_gc() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(condition-case err
             (while (progn (setq quit-flag t) t)
               nil)
           (quit 'quit))
         quit-flag
         (catch 'tag
           (let ((throw-on-input 'tag))
             (while (progn (setq quit-flag 'tag) t)
               nil)
             'missed))",
    );
    assert_eq!(results[0], "OK quit");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK t");
}

#[test]
fn throw_on_input_is_special_and_dynamically_bound() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(special-variable-p 'throw-on-input)
         (let ((throw-on-input 'tag))
           throw-on-input)
         throw-on-input",
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK tag");
    assert_eq!(results[2], "OK nil");
}

#[test]
fn window_system_is_special_and_dynamically_bound_like_gnu_defvar_kboard() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(eval
               '(let ((reader (lambda () window-system)))
                  (let ((window-system 'w32))
                    (list (special-variable-p 'window-system)
                          (funcall reader))))
               t)"
        ),
        "OK (t w32)"
    );
}

#[test]
fn fixnum_limit_constants_are_special_like_gnu_defvar_lisp() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(list (special-variable-p 'most-positive-fixnum)
               (special-variable-p 'most-negative-fixnum))
         (condition-case err
             (let ((most-positive-fixnum 1)) most-positive-fixnum)
           (setting-constant (car err)))
         (condition-case err
             (let* ((most-negative-fixnum -1)) most-negative-fixnum)
           (setting-constant (car err)))
         (condition-case err
             (let ((:neomacs-keyword-constant 1)) :neomacs-keyword-constant)
           (setting-constant (car err)))",
    );
    assert_eq!(results[0], "OK (t t)");
    assert_eq!(results[1], "OK setting-constant");
    assert_eq!(results[2], "OK setting-constant");
    assert_eq!(results[3], "OK setting-constant");
}

#[test]
fn post_self_insert_hook_is_special_and_dynamically_bound_like_gnu_cmds() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(special-variable-p 'post-self-insert-hook)
         (let ((post-self-insert-hook nil))
           (symbol-value 'post-self-insert-hook))",
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK nil");
}

#[test]
fn delayed_warning_defvars_are_special_and_dynamically_bound_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(list (special-variable-p 'delayed-warnings-list)
               (special-variable-p 'delayed-warnings-hook))
         (let ((delayed-warnings-list 'local)
               (delayed-warnings-hook 'hook-local))
           (list delayed-warnings-list delayed-warnings-hook))
         (list delayed-warnings-list delayed-warnings-hook)",
    );
    assert_eq!(results[0], "OK (t t)");
    assert_eq!(results[1], "OK (local hook-local)");
    assert_eq!(results[2], "OK (nil nil)");
}

#[test]
fn c_defvar_runtime_globals_are_special_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(mapcar (lambda (sym)
                   (list sym (boundp sym) (special-variable-p sym)))
                 '(system-type
                   system-configuration
                   system-configuration-options
                   system-configuration-features
                   emacs-version
                   system-name
                   operating-system-release
                   command-line-args
                   user-full-name
                   user-login-name
                   user-real-login-name
                   overriding-plist-environment
                   gc-cons-threshold
                   purify-flag))
         (let ((system-type 'windows-nt)
               (system-configuration \"oracle-config\")
               (gc-cons-threshold 1234567)
               (purify-flag t))
           (list (funcall (lambda () system-type))
                 (symbol-value 'system-configuration)
                 gc-cons-threshold
                 purify-flag))",
    );
    assert_eq!(
        results[0],
        "OK ((system-type t t) (system-configuration t t) (system-configuration-options t t) (system-configuration-features t t) (emacs-version t t) (system-name t t) (operating-system-release t t) (command-line-args t t) (user-full-name t t) (user-login-name t t) (user-real-login-name t t) (overriding-plist-environment t t) (gc-cons-threshold t t) (purify-flag t t))"
    );
    assert_eq!(results[1], "OK (windows-nt \"oracle-config\" 1234567 t)");
}

#[test]
fn global_mode_string_is_a_dynamic_c_variable_like_gnu_xdisp() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(eval
               '(let ((global-mode-string '(base)))
                  (funcall (lambda ()
                             (set 'global-mode-string
                                  (cons 'entry
                                        (symbol-value 'global-mode-string)))))
                  (list (special-variable-p 'global-mode-string)
                        global-mode-string))
               t)"
        ),
        "OK (t (entry base))"
    );
}

#[test]
fn c_defvar_lisp_hook_state_is_bound_and_special_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(mapcar (lambda (sym)
                   (list sym (boundp sym) (special-variable-p sym)))
                 '(kbd-macro-termination-hook
                   minibuffer-exit-hook
                   minibuffer-setup-hook
                   mouse-leave-buffer-hook
                   prefix-help-command
                   pre-command-hook
                   post-command-hook
                   window-size-change-functions))",
    );
    assert_eq!(
        results[0],
        "OK ((kbd-macro-termination-hook t t) (minibuffer-exit-hook t t) (minibuffer-setup-hook t t) (mouse-leave-buffer-hook t t) (prefix-help-command t t) (pre-command-hook t t) (post-command-hook t t) (window-size-change-functions t t))"
    );
}

/// GNU `src/minibuf.c:2553-2559` DEFVARs `minibuffer-setup-hook' and
/// `minibuffer-exit-hook' and initializes both to `Qnil'.  Every entry those
/// hooks carry in a running Emacs is put there by an `add-hook' call in
/// preloaded Lisp (simple.el, minibuffer.el, rfn-eshadow.el) while loadup
/// runs, so the C level owns the *variable* and Lisp owns the *contents*.
///
/// `minibuffer-regexp-mode' has no C definition at all: it is the
/// `define-minor-mode' at `lisp/minibuffer.el:5641', whose `defcustom' is
/// initialized by `custom-initialize-after-file-load'.  That initializer ends
/// in `custom-initialize-set' (`lisp/custom.el:68-82'), which does nothing at
/// all when the symbol *already* has a default top-level value -- so a C-level
/// seed does not merely duplicate the Lisp default, it suppresses the whole
/// `:set' path and with it the mode body that installs the hooks.
#[test]
fn minibuffer_hooks_and_regexp_mode_are_lisp_owned_at_the_c_level_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(list (delq nil
                    (mapcar (lambda (sym)
                              (and (default-value sym) sym))
                            '(after-insert-file-functions
                              delete-terminal-functions
                              display-monitors-changed-functions
                              kbd-macro-termination-hook
                              kill-emacs-hook
                              minibuffer-exit-hook
                              minibuffer-setup-hook
                              mouse-leave-buffer-hook
                              post-command-hook
                              post-select-region-hook
                              pre-command-hook
                              resume-tty-functions
                              suspend-tty-functions
                              write-region-annotate-functions)))
               (boundp 'minibuffer-regexp-mode))",
    );
    assert_eq!(results[0], "OK (nil nil)");
}

/// The preloaded value the two hooks reach once loadup has run every
/// `add-hook' in the preloaded Lisp, in GNU's order.  `add-hook' conses onto
/// the front, so the list reads newest-first: rfn-eshadow.el last,
/// minibuffer.el's two delayed global modes before it (nonselected registers
/// its `after-load-functions' closure after regexp's, so it runs first and
/// ends up behind it), minibuffer.el's top-level `add-hook' before those, and
/// simple.el's three at the tail.
#[test]
fn preloaded_minibuffer_hooks_match_gnu_add_hook_order() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(list (default-value 'minibuffer-setup-hook)
               (default-value 'minibuffer-exit-hook))",
    );
    assert_eq!(
        results[0],
        "OK ((rfn-eshadow-setup-minibuffer minibuffer--regexp-setup \
         minibuffer--nonselected-setup minibuffer-setup-on-screen-keyboard \
         minibuffer-error-initialize minibuffer-history-isearch-setup \
         minibuffer-history-initialize) \
         (minibuffer--regexp-exit minibuffer--nonselected-exit \
         minibuffer-exit-on-screen-keyboard minibuffer-restore-windows))"
    );
}

#[test]
fn system_type_matches_gnu_host_platform_symbol() {
    crate::test_utils::init_test_tracing();
    let results = eval_all("system-type");
    let expected = if cfg!(target_os = "windows") {
        "windows-nt"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "gnu/linux"
    } else if cfg!(target_os = "android") {
        "android"
    } else {
        std::env::consts::OS
    };
    assert_eq!(results[0], format!("OK {expected}"));
}

/// `monitors-changed` is followed by `toolkit-theme-changed` and nothing else.
///
/// `memq` returns the TAIL, so this pin is also a statement about what comes
/// after: GNU's `init_while_no_input_ignore_events` (`src/keyboard.c:13318-13322`)
/// ends its unguarded base list `... Qselection_request, Qmonitors_changed,
/// Qtoolkit_theme_changed`, and ledger 192 restored the `toolkit-theme-changed`
/// this port was missing.  The whole-list assertion lives in
/// `c_features_test.rs`; this one keeps the two neighbouring facts it was
/// written for.
#[test]
fn while_no_input_ignore_events_bootstraps_monitors_changed_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(memq 'monitors-changed while-no-input-ignore-events)
         (special-variable-p 'while-no-input-ignore-events)
         input-pending-p-filter-events",
    );
    assert_eq!(results[0], "OK (monitors-changed toolkit-theme-changed)");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK t");
}

#[test]
fn while_no_input_catches_pending_key_queued_during_body() {
    crate::test_utils::init_test_tracing();

    fn queue_key_for_while_no_input_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "queue helper should not receive arguments");
        ctx.command_loop.keyboard.pending_input_events.push_back(
            crate::keyboard::InputEvent::KeyPress {
                key: crate::keyboard::KeyEvent::char('k'),
                emacs_frame_id: 0,
            },
        );
        Ok(Value::NIL)
    }

    let mut ev = runtime_startup_context();
    ev.set_variable("noninteractive", Value::NIL);
    ev.register_subr(SubrSpec::new(
        "neo-queue-key-for-while-no-input-test",
        NativeFn::ContextVec(queue_key_for_while_no_input_test),
        SubrArity::new(0, Some(0)),
    ));

    let result = ev.eval_str(
        "(condition-case err
             (while-no-input
               (neo-queue-key-for-while-no-input-test)
               (eval '(ignore nil) t)
               'missed)
           (error err))",
    );

    assert_eq!(
        crate::emacs_core::error::format_eval_result(&result),
        "OK t"
    );
}

#[test]
fn while_no_input_catches_pending_key_across_load_boundary() {
    crate::test_utils::init_test_tracing();

    fn queue_key_for_while_no_input_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "queue helper should not receive arguments");
        ctx.command_loop.keyboard.pending_input_events.push_back(
            crate::keyboard::InputEvent::KeyPress {
                key: crate::keyboard::KeyEvent::char('k'),
                emacs_frame_id: 0,
            },
        );
        Ok(Value::NIL)
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let load_path = dir.path().join("while-no-input-load.el");
    std::fs::write(
        &load_path,
        "(neo-queue-key-for-while-no-input-test)\n\
         (eval '(ignore nil) t)\n\
         (setq neo-loaded-after-input t)\n",
    )
    .expect("write load fixture");

    let mut ev = runtime_startup_context();
    ev.set_variable("noninteractive", Value::NIL);
    ev.set_variable(
        "neo-while-no-input-load-file",
        Value::string(load_path.to_string_lossy().into_owned()),
    );
    ev.register_subr(SubrSpec::new(
        "neo-queue-key-for-while-no-input-test",
        NativeFn::ContextVec(queue_key_for_while_no_input_test),
        SubrArity::new(0, Some(0)),
    ));

    let result = ev.eval_str(
        "(progn
           (setq neo-loaded-after-input nil)
           (list
            (condition-case err
                (while-no-input
                  (load neo-while-no-input-load-file nil t)
                  'missed)
              (error err))
            neo-loaded-after-input))",
    );

    assert_eq!(
        crate::emacs_core::error::format_eval_result(&result),
        "OK (t nil)"
    );
}

#[test]
fn with_selected_window_restores_after_while_no_input_keyboard_interrupt() {
    crate::test_utils::init_test_tracing();

    fn queue_key_for_while_no_input_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "queue helper should not receive arguments");
        ctx.command_loop.keyboard.pending_input_events.push_back(
            crate::keyboard::InputEvent::KeyPress {
                key: crate::keyboard::KeyEvent::char_with_mods(
                    'n',
                    crate::keyboard::Modifiers::ctrl(),
                ),
                emacs_frame_id: 0,
            },
        );
        Ok(Value::NIL)
    }

    let mut ev = runtime_startup_context();
    ev.set_variable("noninteractive", Value::NIL);
    ev.register_subr(SubrSpec::new(
        "neo-queue-key-for-while-no-input-test",
        NativeFn::ContextVec(queue_key_for_while_no_input_test),
        SubrArity::new(0, Some(0)),
    ));

    let result = ev.eval_str(
        r#"(let* ((w1 (selected-window))
                  (w2 (split-window))
                  (body-value
                   (with-selected-window w2
                     (while-no-input
                       (neo-queue-key-for-while-no-input-test)
                       (eval '(ignore nil) t)
                       'missed))))
             (prog1
                 (list body-value
                       (eq (selected-window) w1)
                       (eq (current-buffer) (window-buffer w1))
                       (read-event nil nil 0))
               (ignore-errors (delete-window w2))))"#,
    );

    assert_eq!(
        crate::emacs_core::error::format_eval_result(&result),
        "OK (t t t nil)"
    );
}

#[test]
fn while_no_input_unwinds_inner_with_selected_window_on_keyboard_input() {
    crate::test_utils::init_test_tracing();

    fn queue_key_for_while_no_input_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "queue helper should not receive arguments");
        ctx.command_loop.keyboard.pending_input_events.push_back(
            crate::keyboard::InputEvent::KeyPress {
                key: crate::keyboard::KeyEvent::char_with_mods(
                    'n',
                    crate::keyboard::Modifiers::ctrl(),
                ),
                emacs_frame_id: 0,
            },
        );
        Ok(Value::NIL)
    }

    let mut ev = runtime_startup_context();
    ev.set_variable("noninteractive", Value::NIL);
    ev.register_subr(SubrSpec::new(
        "neo-queue-key-for-while-no-input-test",
        NativeFn::ContextVec(queue_key_for_while_no_input_test),
        SubrArity::new(0, Some(0)),
    ));

    let result = ev.eval_str(
        r#"(let* ((w1 (selected-window))
                  (w2 (split-window))
                  (body-value
                   (while-no-input
                     (with-selected-window w2
                       (neo-queue-key-for-while-no-input-test)
                       (eval '(ignore nil) t)
                       'missed))))
             (prog1
                 (list body-value
                       (eq (selected-window) w1)
                       (eq (current-buffer) (window-buffer w1))
                       (read-event nil nil 0))
               (ignore-errors (delete-window w2))))"#,
    );

    assert_eq!(
        crate::emacs_core::error::format_eval_result(&result),
        "OK (t t t nil)"
    );
}

#[test]
fn sit_for_requeues_delayed_input_after_with_selected_window_cleanup() {
    crate::test_utils::init_test_tracing();

    let mut ev = runtime_startup_context();
    ev.set_variable("noninteractive", Value::NIL);
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    // Keep one sender alive for the whole test: dropping the last sender
    // disconnects the channel, which the input machinery treats as
    // terminal-gone -> quit. The test models a terminal that stays connected
    // and delivers one delayed key — without this, any input poll that lands
    // after the spawned thread exits (timing-dependent; reliably triggered by
    // JIT compile pauses under NEOVM_JIT_THRESHOLD=1) sees Disconnected and
    // the whole form signals (quit).
    let _tx_keepalive = tx.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char_with_mods('n', crate::keyboard::Modifiers::ctrl()),
        ))
        .expect("send delayed C-n");
    });

    let result = ev.eval_str(
        r#"(let* ((w1 (selected-window))
                  (w2 (split-window))
                  (body-value
                   (with-selected-window w2
                     (sit-for 0.2 t))))
             (prog1
                 (list body-value
                       (eq (selected-window) w1)
                       (eq (current-buffer) (window-buffer w1))
                       (read-event nil nil 0))
               (ignore-errors (delete-window w2))))"#,
    );

    assert_eq!(
        crate::emacs_core::error::format_eval_result(&result),
        "OK (nil t t 14)"
    );
}

#[test]
fn safe_run_hook_preserves_selected_window_after_while_no_input_interrupt() {
    crate::test_utils::init_test_tracing();

    fn queue_key_for_while_no_input_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "queue helper should not receive arguments");
        ctx.command_loop.keyboard.pending_input_events.push_back(
            crate::keyboard::InputEvent::KeyPress {
                key: crate::keyboard::KeyEvent::char_with_mods(
                    'n',
                    crate::keyboard::Modifiers::ctrl(),
                ),
                emacs_frame_id: 0,
            },
        );
        Ok(Value::NIL)
    }

    let mut ev = runtime_startup_context();
    ev.set_variable("noninteractive", Value::NIL);
    ev.register_subr(SubrSpec::new(
        "neo-queue-key-for-while-no-input-test",
        NativeFn::ContextVec(queue_key_for_while_no_input_test),
        SubrArity::new(0, Some(0)),
    ));
    ev.eval_str(
        r#"(let ((w1 (selected-window))
                 (w2 (split-window)))
             (select-window w2)
             (setq neo-safe-hook-window w1)
             (setq neo-safe-hook-result nil)
             (defalias 'neo-safe-hook-fn
               #'(lambda ()
                   (setq neo-safe-hook-result
                         (with-selected-window neo-safe-hook-window
                           (while-no-input
                             (neo-queue-key-for-while-no-input-test)
                             (eval '(ignore nil) t)
                             'missed)))))
             (setq neo-safe-hook '(neo-safe-hook-fn)))"#,
    )
    .expect("install hook");

    crate::emacs_core::hook_runtime::safe_run_named_hook(
        &mut ev,
        crate::emacs_core::intern::intern("neo-safe-hook"),
        &[],
    )
    .expect("safe hook should finish");

    let result = ev.eval_str(
        r#"(prog1
               (list neo-safe-hook-result
                     (eq (selected-window) (next-window neo-safe-hook-window))
                     (eq (current-buffer) (window-buffer (selected-window)))
                     (read-event nil nil 0))
             (ignore-errors (delete-window (selected-window))))"#,
    );

    assert_eq!(
        crate::emacs_core::error::format_eval_result(&result),
        "OK (t t t nil)"
    );
}

#[test]
fn window_and_minibuffer_defvars_are_bound_and_special_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(list (boundp 'minibuffer-scroll-window)
               (special-variable-p 'minibuffer-scroll-window)
               (boundp 'other-window-scroll-buffer)
               (special-variable-p 'other-window-scroll-buffer)
               (boundp 'other-window-scroll-default)
               (special-variable-p 'other-window-scroll-default)
               (boundp 'scroll-minibuffer-conservatively)
               (special-variable-p 'scroll-minibuffer-conservatively)
               scroll-minibuffer-conservatively)",
    );
    assert_eq!(results[0], "OK (t t t t t t t t t)");
}

#[test]
fn input_pending_p_filters_default_ignored_events_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    let window_id = ev.frames.get(fid).expect("frame").window_list()[0];

    ev.command_loop.keyboard.pending_input_events.push_back(
        crate::keyboard::InputEvent::MonitorsChanged {
            monitors: vec![crate::emacs_core::builtins::NeomacsMonitorInfo {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale: 1.0,
                width_mm: 500,
                height_mm: 300,
                name: Some("DP-1".to_string()),
            }],
        },
    );
    ev.command_loop
        .keyboard
        .pending_input_events
        .push_back(crate::keyboard::InputEvent::SelectWindow { window_id });

    let filtered = crate::emacs_core::reader::builtin_input_pending_p(&mut ev, vec![])
        .expect("default input-pending-p should succeed");
    assert_eq!(filtered, Value::NIL);

    ev.obarray
        .set_symbol_value("input-pending-p-filter-events", Value::NIL);
    let unfiltered = crate::emacs_core::reader::builtin_input_pending_p(&mut ev, vec![])
        .expect("unfiltered input-pending-p should succeed");
    assert_eq!(unfiltered, Value::T);
}

#[test]
fn with_temp_message_accepts_min_arity_and_runs_body() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(with-temp-message nil 42)
         (with-temp-message \"tmp\" 7)
         (condition-case err
             (with-temp-message)
           (error (car err)))",
    );
    assert_eq!(results[0], "OK 42");
    assert_eq!(results[1], "OK 7");
    assert_eq!(results[2], "OK wrong-number-of-arguments");
}

#[test]
fn with_demoted_errors_runtime_semantics() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(fboundp 'with-demoted-errors)
         (macrop 'with-demoted-errors)
         (with-demoted-errors \"DM %S\" (+ 1 2))
         (condition-case err
             (with-demoted-errors \"DM %S\" (/ 1 0))
           (error (list :error (car err) (cdr err))))
         (condition-case err
             (with-demoted-errors 1 (/ 1 0))
           (error (list :error (car err) (cdr err))))
         (with-demoted-errors \"DM %S\")
         (func-arity (symbol-function 'with-demoted-errors))
         (condition-case err
             (with-demoted-errors)
           (error err))",
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK 3");
    assert_eq!(results[3], "OK nil");
    assert_eq!(results[4], "OK nil");
    assert_eq!(results[5], r#"OK "DM %S""#);
    // GNU reports the macro's public arity as (1 . many), but a direct
    // zero-argument macro call signals from the compiled macro entry path
    // as exactly one argument.
    assert_eq!(results[6], "OK (1 . many)");
    assert_eq!(results[7], "OK (wrong-number-of-arguments (1 . 1) 0)");
}

#[test]
fn func_arity_direct_lambda_and_macro_values_match_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(func-arity '(lambda (x &optional y &rest z) x))
           (func-arity '(lambda (&rest) x))
           (func-arity '(lambda (&optional &optional x) x))
           (condition-case err
               (func-arity '(lambda (x . y) x))
             (error err))
           (condition-case err
               (func-arity '(macro . 1))
             (error err))"#,
    );
    assert_eq!(results[0], "OK (1 . many)");
    assert_eq!(results[1], "OK (0 . many)");
    assert_eq!(results[2], "OK (0 . 1)");
    assert_eq!(results[3], "OK (invalid-function (lambda (x . y) x))");
    assert_eq!(results[4], "OK (invalid-function (macro . 1))");
}

#[test]
fn bootstrap_condition_case_unless_debug_calls_debugger_before_handler() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            "(progn
               (setq neovm-debugger-called nil)
               (let ((debug-on-error t)
                   (debugger (lambda (&rest args)
                               (setq neovm-debugger-called args))))
                 (list (condition-case-unless-debug nil
                           (signal 'error 1)
                         (error 'handled))
                       neovm-debugger-called)))"
        ),
        "OK (handled (error (error . 1)))"
    );
}

#[test]
fn bootstrap_with_demoted_errors_calls_debugger_when_debug_on_error_is_enabled() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            "(progn
               (setq neovm-debugger-called nil)
               (let ((debug-on-error t)
                   (debugger (lambda (&rest _args)
                               (setq neovm-debugger-called 'debugger))))
                 (list (with-demoted-errors \"DM %S\" (/ 1 0))
                       neovm-debugger-called)))"
        ),
        "OK (nil debugger)"
    );
}

#[test]
fn buffer_char_after_before() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"cb\")
         (set-buffer \"cb\")
         (insert \"abc\")
         (goto-char 2)
         (char-after)
         (char-before)",
    );
    assert_eq!(results[4], "OK 98"); // ?b = 98
    assert_eq!(results[5], "OK 97"); // ?a = 97
}

#[test]
fn buffer_list_and_kill() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        "(get-buffer-create \"kill-me\")
         (kill-buffer \"kill-me\")
         (get-buffer \"kill-me\")",
    );
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK nil");
}

#[test]
fn buffer_generate_new_buffer() {
    crate::test_utils::init_test_tracing();
    let results = eval_all_with_subr(
        "(buffer-name (generate-new-buffer \"test\"))
         (buffer-name (generate-new-buffer \"test\"))",
    );
    assert_eq!(results[0], r#"OK "test""#);
    assert_eq!(results[1], r#"OK "test<2>""#);
}

#[test]
fn fillarray_string_writeback_updates_symbol_binding() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(let ((s (copy-sequence \"abc\"))) (fillarray s ?x) s)");
    assert_eq!(result, r#"OK "xxx""#);
}

#[test]
fn fillarray_alias_string_writeback_updates_symbol_binding() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
            (defalias 'vm-fillarray-alias 'fillarray)
            (let ((s (copy-sequence \"abc\")))
              (vm-fillarray-alias s ?y)
              s))",
    );
    assert_eq!(result, r#"OK "yyy""#);
}

#[test]
fn fillarray_string_writeback_updates_alias_from_prog1_expression() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(let ((s (copy-sequence \"abc\"))) (fillarray (prog1 s) ?x) s)");
    assert_eq!(result, r#"OK "xxx""#);
}

#[test]
fn fillarray_string_writeback_updates_alias_from_list_car_expression() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(let ((s (copy-sequence \"abc\"))) (fillarray (car (list s)) ?y) s)");
    assert_eq!(result, r#"OK "yyy""#);
}

#[test]
fn fillarray_string_writeback_updates_vector_alias_element() {
    crate::test_utils::init_test_tracing();
    let result =
        eval_one("(let* ((s (copy-sequence \"abc\")) (v (vector s))) (fillarray s ?x) (aref v 0))");
    assert_eq!(result, r#"OK "xxx""#);
}

#[test]
fn fillarray_string_writeback_updates_cons_alias_element() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let* ((s (copy-sequence \"abc\")) (cell (cons s nil))) (fillarray s ?y) (car cell))",
    );
    assert_eq!(result, r#"OK "yyy""#);
}

#[test]
fn fillarray_string_writeback_preserves_eq_hash_key_lookup() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let* ((s (copy-sequence \"abc\")) (ht (make-hash-table :test 'eq)))
           (puthash s 'v ht)
           (fillarray s ?x)
           (gethash s ht))",
    );
    assert_eq!(result, "OK v");
}

#[test]
fn fillarray_string_writeback_preserves_eql_hash_key_lookup() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let* ((s (copy-sequence \"abc\")) (ht (make-hash-table :test 'eql)))
           (puthash s 'v ht)
           (fillarray s ?y)
           (gethash s ht))",
    );
    assert_eq!(result, "OK v");
}

#[test]
fn fillarray_string_writeback_equal_hash_key_lookup_stays_nil() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let* ((s (copy-sequence \"abc\")) (ht (make-hash-table :test 'equal)))
           (puthash s 'v ht)
           (fillarray s ?z)
           (gethash s ht))",
    );
    assert_eq!(result, "OK nil");
}

#[test]
fn aset_string_writeback_updates_symbol_binding() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(let ((s (copy-sequence \"abc\"))) (aset s 1 ?x) s)");
    assert_eq!(result, r#"OK "axc""#);
}

#[test]
fn aset_alias_string_writeback_updates_symbol_binding() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
            (defalias 'vm-aset-alias 'aset)
            (let ((s (copy-sequence \"abc\")))
              (vm-aset-alias s 1 ?y)
              s))",
    );
    assert_eq!(result, r#"OK "ayc""#);
}

#[test]
fn aset_string_writeback_updates_alias_from_prog1_expression() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(let ((s (copy-sequence \"abc\"))) (aset (prog1 s) 1 ?x) s)");
    assert_eq!(result, r#"OK "axc""#);
}

#[test]
fn aset_string_writeback_updates_alias_from_list_car_expression() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(let ((s (copy-sequence \"abc\"))) (aset (car (list s)) 1 ?y) s)");
    assert_eq!(result, r#"OK "ayc""#);
}

#[test]
fn aset_string_writeback_updates_vector_alias_element() {
    crate::test_utils::init_test_tracing();
    let result =
        eval_one("(let* ((s (copy-sequence \"abc\")) (v (vector s))) (aset s 1 ?x) (aref v 0))");
    assert_eq!(result, r#"OK "axc""#);
}

#[test]
fn aset_string_writeback_updates_cons_alias_element() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let* ((s (copy-sequence \"abc\")) (cell (cons s nil))) (aset s 1 ?y) (car cell))",
    );
    assert_eq!(result, r#"OK "ayc""#);
}

#[test]
fn aset_string_writeback_preserves_eq_hash_key_lookup() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let* ((s (copy-sequence \"abc\")) (ht (make-hash-table :test 'eq)))
           (puthash s 'v ht)
           (aset s 1 ?x)
           (gethash s ht))",
    );
    assert_eq!(result, "OK v");
}

#[test]
fn aset_string_writeback_preserves_eql_hash_key_lookup() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let* ((s (copy-sequence \"abc\")) (ht (make-hash-table :test 'eql)))
           (puthash s 'v ht)
           (aset s 1 ?y)
           (gethash s ht))",
    );
    assert_eq!(result, "OK v");
}

#[test]
fn aset_string_writeback_equal_hash_key_lookup_stays_nil() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(let* ((s (copy-sequence \"abc\")) (ht (make-hash-table :test 'equal)))
           (puthash s 'v ht)
           (aset s 1 ?z)
           (gethash s ht))",
    );
    assert_eq!(result, "OK nil");
}

// -----------------------------------------------------------------------
// GC integration tests
// -----------------------------------------------------------------------

#[test]
fn gc_collect_retains_reachable() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str_each("(setq x (cons 1 2))");
    let before = ev.tagged_heap.allocated_count();
    ev.gc_collect();
    let after = ev.tagged_heap.allocated_count();
    // The cons stored in variable `x` must survive.
    assert!(after >= 1, "reachable cons was collected");
    assert!(after <= before, "gc should not increase count");
    // Verify the value is still accessible.
    let results = ev.eval_str_each("(car x)");
    assert_eq!(format_eval_result(&results[0]), "OK 1");
}

#[test]
fn gc_collect_exact_retains_reachable() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str_each("(setq x (cons 11 22))");
    ev.gc_collect_exact();

    let results = ev.eval_str_each("(car x)");
    assert_eq!(format_eval_result(&results[0]), "OK 11");
}

#[test]
fn finalizer_runs_after_object_becomes_unreachable() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str_each("(setq finalizer-ran nil)");
    ev.eval_str_each("(progn (make-finalizer (lambda () (setq finalizer-ran t))) nil)");
    let results = ev.eval_str_each("finalizer-ran");
    assert_eq!(
        format_eval_result(&results[0]),
        "OK nil",
        "finalizer must not run before a GC dooms it"
    );

    ev.gc_collect_exact();

    let results = ev.eval_str_each("finalizer-ran");
    assert_eq!(format_eval_result(&results[0]), "OK t");
}

#[test]
fn finalizer_runs_exactly_once_across_multiple_gcs() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str_each("(setq finalizer-count 0)");
    ev.eval_str_each(
        "(progn (make-finalizer (lambda () (setq finalizer-count (1+ finalizer-count)))) nil)",
    );
    ev.gc_collect_exact();
    ev.gc_collect_exact();
    ev.gc_collect_exact();

    let results = ev.eval_str_each("finalizer-count");
    assert_eq!(format_eval_result(&results[0]), "OK 1");
}

#[test]
fn finalizer_does_not_run_while_object_reachable() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str_each("(setq finalizer-ran nil)");
    ev.eval_str_each("(setq finalizer-keeper (make-finalizer (lambda () (setq finalizer-ran t))))");
    ev.gc_collect_exact();
    ev.gc_collect_exact();
    let results = ev.eval_str_each("finalizer-ran");
    assert_eq!(
        format_eval_result(&results[0]),
        "OK nil",
        "a reachable finalizer must never run"
    );

    ev.eval_str_each("(setq finalizer-keeper nil)");
    ev.gc_collect_exact();
    let results = ev.eval_str_each("finalizer-ran");
    assert_eq!(format_eval_result(&results[0]), "OK t");
}

#[test]
fn finalizer_function_closure_data_kept_alive_until_run() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);

    ev.eval_str_each("(setq finalizer-data nil)");
    // Nothing but the doomed finalizer references the closure or the list it
    // captured; both must survive until the function has run.
    ev.eval_str_each(
        "(progn (let ((cell (list 1 2 3))) \
                   (make-finalizer (lambda () (setq finalizer-data cell)))) \
                nil)",
    );
    ev.gc_collect_exact();

    let results = ev.eval_str_each("finalizer-data");
    assert_eq!(format_eval_result(&results[0]), "OK (1 2 3)");
}

#[test]
fn finalizer_error_is_ignored_and_other_finalizers_run() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str_each("(setq finalizer-log nil)");
    ev.eval_str_each(
        "(progn (make-finalizer (lambda () (setq finalizer-log (cons 'a finalizer-log)))) \
                (make-finalizer (lambda () (error \"finalizer boom\"))) \
                (make-finalizer (lambda () (setq finalizer-log (cons 'b finalizer-log)))) \
                nil)",
    );
    ev.gc_collect_exact();

    let results = ev.eval_str_each(
        "(and (memq 'a finalizer-log) (memq 'b finalizer-log) (length finalizer-log))",
    );
    assert_eq!(format_eval_result(&results[0]), "OK 2");
}

#[test]
fn finalizer_created_inside_finalizer_runs_on_later_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str_each("(setq finalizer-inner-ran nil)");
    ev.eval_str_each(
        "(progn (make-finalizer \
                  (lambda () \
                    (make-finalizer (lambda () (setq finalizer-inner-ran t))) \
                    nil)) \
                nil)",
    );

    // First GC dooms + runs the outer finalizer, which creates (and drops)
    // the inner one; the inner must NOT run in the same batch.
    ev.gc_collect_exact();
    let results = ev.eval_str_each("finalizer-inner-ran");
    assert_eq!(format_eval_result(&results[0]), "OK nil");

    // A later GC dooms + runs the inner finalizer.
    ev.gc_collect_exact();
    let results = ev.eval_str_each("finalizer-inner-ran");
    assert_eq!(format_eval_result(&results[0]), "OK t");
}

#[test]
fn finalizer_type_of_and_prints_opaquely() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let results = ev.eval_str_each("(type-of (make-finalizer #'ignore))");
    assert_eq!(format_eval_result(&results[0]), "OK finalizer");
    let results = ev.eval_str_each("(cl-type-of (make-finalizer #'ignore))");
    assert_eq!(format_eval_result(&results[0]), "OK finalizer");
    let results = ev.eval_str_each("(prin1-to-string (make-finalizer #'ignore))");
    assert_eq!(format_eval_result(&results[0]), "OK \"#<finalizer>\"");
}

#[test]
fn gc_collect_exact_frees_stack_only_values() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let marker = 0u8;
    ev.tagged_heap.set_stack_bottom(&marker as *const u8);

    ev.gc_collect_exact();
    let baseline = ev.tagged_heap.allocated_count();
    let stack_only = Value::cons(Value::fixnum(31), Value::fixnum(32));
    let keep_visible = [stack_only];
    std::hint::black_box(&keep_visible);
    let after_alloc = ev.tagged_heap.allocated_count();
    assert_eq!(
        after_alloc,
        baseline + 1,
        "stack-only cons should have allocated exactly one object after the baseline collection: baseline={baseline}, after_alloc={after_alloc}"
    );

    ev.gc_collect_exact();

    let after_gc = ev.tagged_heap.allocated_count();
    assert_eq!(
        after_gc, baseline,
        "exact GC must ignore the configured conservative stack scan and free stack-only objects: baseline={baseline}, after_alloc={after_alloc}, after_gc={after_gc}"
    );
}

#[test]
fn gc_collect_exact_inside_extra_root_scope_retains_explicit_slice() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let rooted = Value::cons(Value::fixnum(11), Value::fixnum(22));
    let _unreachable = Value::cons(Value::fixnum(1), Value::fixnum(2));
    let before = ev.tagged_heap.allocated_count();

    let scope = ev.save_specpdl_roots();
    ev.push_specpdl_root(rooted);
    ev.gc_collect_exact();
    ev.restore_specpdl_roots(scope);

    let after = ev.tagged_heap.allocated_count();
    assert_eq!(rooted.cons_car(), Value::fixnum(11));
    assert!(
        after < before,
        "exact collection with explicit roots should free unrelated garbage: before={before}, after={after}"
    );
}

#[test]
fn specpdl_roots_are_traced_across_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let payload = Value::vector(vec![Value::fixnum(29)]);
    let scope = ev.save_specpdl_roots();
    ev.push_specpdl_root(payload);

    ev.gc_collect_exact();

    let rooted = match ev.specpdl.last() {
        Some(SpecBinding::GcRoot { value }) => *value,
        other => panic!("expected specpdl gc root entry, got {other:?}"),
    };
    assert_eq!(
        rooted.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(29)]
    );

    ev.restore_specpdl_roots(scope);
    assert!(ev.specpdl.is_empty());
}

#[test]
fn eval_str_each_roots_parsed_forms_on_specpdl() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let results = ev.eval_str_each("(setq x (cons 11 22)) (garbage-collect) (car x)");
    assert_eq!(format_eval_result(&results[2]), "OK 11");
}

#[test]
fn prog1_primary_survives_cleanup_garbage_collect() {
    assert_eq!(
        eval_one("(car (prog1 (cons 31 32) (garbage-collect)))"),
        "OK 31"
    );
}

#[test]
fn unwind_protect_primary_survives_cleanup_garbage_collect() {
    assert_eq!(
        eval_one("(car (unwind-protect (cons 41 42) (garbage-collect)))"),
        "OK 41"
    );
}

#[test]
fn let_init_values_survive_gc_stress_until_bindings_own_them() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;

    let result = ev.eval_str(
        "(let ((x (cons 51 52))
               (y (cons 61 62)))
           (list (car x) (car y)))",
    );
    assert_eq!(format_eval_result(&result), "OK (51 61)");
}

#[test]
fn specpdl_backtrace_frame_args_survive_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let payload = Value::vector(vec![Value::fixnum(17)]);
    let bt_count = ev.specpdl.len();
    ev.push_backtrace_frame(Value::symbol("runtime-backtrace-active-call"), &[payload]);

    ev.gc_collect_exact();

    // Find the backtrace frame and verify args survived GC.
    let rooted = ev
        .specpdl
        .iter()
        .rev()
        .find_map(|entry| {
            ev.backtrace_entry_values(entry)
                .and_then(|(_, args, _, _)| args.first().copied())
        })
        .expect("backtrace frame should remain present");
    assert_eq!(
        rooted.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(17)]
    );

    ev.unbind_to(bt_count);
}

#[test]
fn specpdl_gc_root_survives_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let payload = Value::vector(vec![Value::fixnum(13)]);
    let bt_count = ev.specpdl.len();
    ev.push_backtrace_frame(Value::symbol("active-call-root"), &[payload]);

    ev.gc_collect_exact();

    let rooted = ev
        .specpdl
        .iter()
        .rev()
        .find_map(|entry| {
            ev.backtrace_entry_values(entry)
                .and_then(|(_, args, _, _)| args.first().copied())
        })
        .expect("backtrace frame should remain present");
    assert_eq!(
        rooted.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(13)]
    );

    ev.unbind_to(bt_count);
}

#[test]
fn specpdl_gc_root_entries_are_traced_across_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let payload = Value::vector(vec![Value::fixnum(17)]);
    let scope = ev.save_specpdl_roots();
    ev.push_specpdl_root(payload);
    ev.gc_collect_exact();

    let rooted = match ev.specpdl.last() {
        Some(SpecBinding::GcRoot { value }) => *value,
        other => panic!("expected specpdl gc root entry, got {other:?}"),
    };
    assert_eq!(
        rooted.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(17)]
    );
    ev.restore_specpdl_roots(scope);
}

#[test]
fn vm_root_frames_are_traced_across_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let payload = Value::vector(vec![Value::fixnum(37)]);
    ev.push_vm_root_frame();
    ev.push_vm_frame_root(payload);

    ev.gc_collect_exact();

    let rooted = ev
        .vm_root_frames
        .last()
        .expect("vm root frame should remain present")
        .roots[0];
    assert_eq!(
        rooted.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(37)]
    );

    ev.pop_vm_root_frame();
}

#[test]
fn make_symbol_name_value_survives_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.gc_stress = true;

    let result = ev.eval_str(
        r#"(let* ((name (copy-sequence "vm-exact-symbol-name"))
                  (sym (make-symbol name))
                  (i 0))
             (while (< i 300)
               (setq i (1+ i))
               (vector (copy-sequence "replacement")))
             (list (eq (symbol-name sym) name)
                   (symbol-name sym)))"#,
    );

    assert_eq!(
        format_eval_result(&result),
        r#"OK (t "vm-exact-symbol-name")"#
    );
}

#[test]
fn custom_obarray_intern_preserves_exact_name_value() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let result = ev.eval_str(
        r#"(let* ((obarray (make-vector 17 0))
                  (name (copy-sequence "abc"))
                  (sym (intern name obarray)))
             (list (eq (symbol-name sym) name)
                   (symbol-name sym)
                   (progn (aset name 0 ?X) name)
                   (eq (symbol-name sym) name)
                   name
                   (symbol-name sym)
                   (eq sym (intern-soft name obarray))
                   (intern-soft "abc" obarray)
                   (intern-soft "Xbc" obarray)))"#,
    );

    assert_eq!(
        format_eval_result(&result),
        r#"OK (t "Xbc" "Xbc" t "Xbc" "Xbc" t nil Xbc)"#
    );
}

#[test]
fn extra_gc_roots_use_specpdl_when_no_runtime_frame_owns_them() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let payload = Value::vector(vec![Value::fixnum(43)]);

    let scope = ev.save_specpdl_roots();
    ev.push_specpdl_root(payload);
    assert!(matches!(
        ev.specpdl.last(),
        Some(SpecBinding::GcRoot { .. })
    ));
    ev.gc_collect_exact();
    let rooted = match ev.specpdl.last() {
        Some(SpecBinding::GcRoot { value }) => *value,
        other => panic!("expected specpdl gc root entry, got {other:?}"),
    };
    ev.restore_specpdl_roots(scope);

    assert_eq!(
        rooted.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(43)]
    );
    assert!(ev.specpdl.is_empty());
}

#[test]
fn push_specpdl_root_creates_gc_root_entry_and_restore_removes_it() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let payload = Value::vector(vec![Value::fixnum(44)]);

    let scope = ev.save_specpdl_roots();
    ev.push_specpdl_root(payload);
    assert!(matches!(
        ev.specpdl.last(),
        Some(SpecBinding::GcRoot { .. })
    ));
    ev.gc_collect_exact();
    let rooted = match ev.specpdl.last() {
        Some(SpecBinding::GcRoot { value }) => *value,
        other => panic!("expected specpdl gc root entry, got {other:?}"),
    };
    ev.restore_specpdl_roots(scope);

    assert_eq!(
        rooted.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(44)]
    );
    assert!(ev.specpdl.is_empty());
}

#[test]
fn lexical_binding_rooting_uses_specpdl() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let payload = Value::vector(vec![Value::fixnum(47)]);
    let sym = intern("specpdl-lexical-binding");

    ev.bind_lexical_value_rooted(sym, payload);

    assert_eq!(
        ev.lexenv_lookup_cached_in(ev.lexenv, sym)
            .expect("lexical binding should exist")
            .as_vector_data()
            .unwrap()
            .as_slice(),
        &[Value::fixnum(47)]
    );
    // bind_lexical_value_rooted uses a temporary specpdl root that is
    // popped after the cons cells are allocated, so specpdl should be empty.
    assert!(
        ev.specpdl.is_empty(),
        "temporary specpdl roots should be released once lexenv owns the binding"
    );
}

#[test]
fn lexical_binding_fallback_uses_specpdl_when_no_frame_is_available() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.gc_stress = true;
    let payload = Value::vector(vec![Value::fixnum(48)]);
    let sym = intern("specpdl-lexical-fallback");

    ev.bind_lexical_value_rooted(sym, payload);

    assert_eq!(
        ev.lexenv_lookup_cached_in(ev.lexenv, sym)
            .expect("lexical binding should exist")
            .as_vector_data()
            .unwrap()
            .as_slice(),
        &[Value::fixnum(48)]
    );
    assert!(
        ev.specpdl.is_empty(),
        "temporary specpdl roots should be released once lexenv owns the binding"
    );
}

#[test]
fn direct_closure_call_uses_specpdl_for_rooting() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;

    let callable = ev
        .eval_str(
            "(let ((captured (vector 71)))
               (lambda (x &optional y &rest rest)
                 (list (aref captured 0) x y rest)))",
        )
        .expect("closure should evaluate");

    let specpdl_before = ev.specpdl.len();

    let result = match ev.funcall_general_untraced(
        callable,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::fixnum(3),
            Value::fixnum(4),
        ],
    ) {
        Ok(value) => value,
        Err(Flow::Signal(sig)) => panic!(
            "direct closure call should succeed: {} {:?}",
            sig.symbol_name(),
            sig.data
        ),
        Err(other) => panic!("direct closure call should succeed: {other:?}"),
    };

    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        "(71 1 2 (3 4))"
    );
    assert_eq!(
        ev.specpdl.len(),
        specpdl_before,
        "closure call should clean up all specpdl entries"
    );
}

#[test]
fn direct_closure_call_rest_args_preserve_heap_values_under_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.gc_stress = true;

    let callable = ev
        .eval_str("(lambda (&rest rest) (car (cdr (cdr rest))))")
        .expect("lambda should evaluate");

    let result = ev
        .funcall_general_untraced(
            callable,
            vec![Value::fixnum(1), Value::fixnum(2), Value::string("29.1")],
        )
        .expect("rest-arg lambda call should succeed");

    assert_eq!(result, Value::string("29.1"));
}

/// End-to-end: a hot bytecode function actually tiers up to native code through
/// the real `funcall` dispatch seam, and a non-compilable body falls back to the
/// interpreter — all producing correct results.
#[cfg(feature = "jit")]
#[test]
fn jit_tierup_executes_through_funcall_seam() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();

    // Build a *hot* nullary bytecode function (forced hot so the next dispatch
    // tiers it up, instead of driving HOT_THRESHOLD real calls).
    let mk = |ops: Vec<Op>, consts: Vec<Value>| -> Value {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.ops = ops;
        f.constants = consts.into();
        f.max_stack = 16;
        f.jit_runtime().set_hot_for_test();
        Value::make_bytecode(f)
    };

    // Compilable leaf -> executes as NATIVE code through the seam.
    let konst = mk(vec![Op::Constant(0), Op::Return], vec![Value::make_int(42)]);
    assert_eq!(
        ev.funcall_general_untraced(konst, vec![]).unwrap(),
        Value::make_int(42)
    );

    // Fixnum arithmetic, native.
    let sum = mk(
        vec![Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
        vec![Value::make_int(40), Value::make_int(2)],
    );
    assert_eq!(
        ev.funcall_general_untraced(sum, vec![]).unwrap(),
        Value::make_int(42)
    );

    // Non-compilable body (Mul is unsupported) -> the seam falls back to the
    // interpreter, still correct: (* 6 7) = 42.
    let mul = mk(
        vec![Op::Constant(0), Op::Constant(1), Op::Mul, Op::Return],
        vec![Value::make_int(6), Value::make_int(7)],
    );
    assert_eq!(
        ev.funcall_general_untraced(mul, vec![]).unwrap(),
        Value::make_int(42)
    );

    // A function WITH required (lexical) args also tiers up: (lambda (a b) (+ a b)).
    let mut addf = ByteCodeFunction::new(LambdaParams {
        required: vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ],
        optional: Vec::new(),
        rest: None,
    });
    addf.lexical = true;
    addf.ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
    addf.max_stack = 16;
    addf.jit_runtime().set_hot_for_test();
    let addv = Value::make_bytecode(addf);
    assert_eq!(
        ev.funcall_general_untraced(addv, vec![Value::make_int(40), Value::make_int(2)])
            .unwrap(),
        Value::make_int(42)
    );
    // Wrong argument count -> native is skipped (arity mismatch); the interpreter
    // fallback signals wrong-number-of-arguments.
    assert!(
        ev.funcall_general_untraced(addv, vec![Value::make_int(1)])
            .is_err()
    );
}

/// End-to-end: a hot function that allocates (`cons`) runs as native code
/// through the funcall seam against a real `Context` heap, exercising the JIT's
/// runtime-shim call + GC rooting on the live dispatch path.
#[cfg(feature = "jit")]
#[test]
fn jit_cons_through_funcall_seam() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    // (lambda (a b) (cons a b)):
    //  0 StackRef(1)=a; 1 StackRef(1)=b; 2 Cons; 3 Return
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Cons, Op::Return];
    f.max_stack = 16;
    f.jit_runtime().set_hot_for_test();
    let fv = Value::make_bytecode(f);

    let r = ev
        .funcall_general_untraced(fv, vec![Value::make_int(1), Value::make_int(2)])
        .expect("native cons runs");
    assert!(r.is_cons());
    assert_eq!(r.cons_car(), Value::make_int(1));
    assert_eq!(r.cons_cdr(), Value::make_int(2));
}

/// End-to-end: a hot function that CALLS another function runs as native code
/// through the funcall seam — the call shim re-enters the runtime, the callee
/// executes, and the result flows back into native code. Also covers nested
/// JIT->JIT dispatch, deopt-before-call fallback, and signal propagation.
#[cfg(feature = "jit")]
#[test]
fn jit_call_through_funcall_seam() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    // Callee: (lambda (y) (* y 2)) as cold bytecode (a bare Context has no
    // elisp preloaded, so build it directly instead of via defun).
    let mut dbl = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    dbl.lexical = true;
    dbl.ops = vec![Op::StackRef(0), Op::Constant(0), Op::Mul, Op::Return];
    dbl.constants = vec![Value::make_int(2)].into();
    dbl.max_stack = 16;
    let dbl_sym = Value::symbol("jit-e2e-double");
    let ValueKind::Symbol(dbl_id) = dbl_sym.kind() else {
        panic!("symbol expected");
    };
    ev.obarray
        .set_symbol_function_id(dbl_id, Value::make_bytecode(dbl));

    // Hot caller: (lambda (x) (jit-e2e-double (1+ x))) — a guard BEFORE the
    // call (allowed by the poisoning analysis).
    let mk_caller = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::Constant(0), // 'jit-e2e-double
            Op::StackRef(1), // x
            Op::Add1,        // guard before the call
            Op::Call(1),
            Op::Return,
        ];
        f.constants = vec![Value::symbol("jit-e2e-double")].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };

    // Native path result must equal the pure-interpreter result.
    let hot = mk_caller(true);
    let cold = mk_caller(false);
    let native = ev
        .funcall_general_untraced(hot, vec![Value::make_int(5)])
        .expect("native call runs");
    let interp = ev
        .funcall_general_untraced(cold, vec![Value::make_int(5)])
        .expect("interp call runs");
    assert_eq!(native, Value::make_int(12));
    assert_eq!(native, interp);

    // Deopt-before-call: boundary input fails the 1+ guard BEFORE the call ran,
    // so the seam reruns the interpreter, which promotes to a bignum.
    let big_native = ev
        .funcall_general_untraced(hot, vec![Value::make_int(Value::MOST_POSITIVE_FIXNUM)])
        .expect("deopt falls back to the interpreter");
    let big_interp = ev
        .funcall_general_untraced(cold, vec![Value::make_int(Value::MOST_POSITIVE_FIXNUM)])
        .expect("interp bignum path");
    assert!(!big_native.is_fixnum(), "result must promote past fixnum");
    assert!(
        ev.eval_str("nil").is_ok(),
        "context stays healthy after deopt"
    );
    assert_eq!(
        crate::emacs_core::print::print_value(&big_native),
        crate::emacs_core::print::print_value(&big_interp),
        "deopt fallback must match the interpreter exactly"
    );

    // Nested JIT->JIT: a HOT bytecode callee makes the inner dispatch re-enter
    // the per-thread compiled cache from inside native code (the Rc-clone
    // execution path; a borrow-held call would panic the RefCell).
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::StackRef(0), Op::Constant(0), Op::Mul, Op::Return];
    callee.constants = vec![Value::make_int(3)].into();
    callee.max_stack = 16;
    callee.jit_runtime().set_hot_for_test();
    let callee_sym = Value::symbol("jit-e2e-triple-hot");
    let ValueKind::Symbol(callee_id) = callee_sym.kind() else {
        panic!("symbol expected");
    };
    ev.obarray
        .set_symbol_function_id(callee_id, Value::make_bytecode(callee));

    let mut nested = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    nested.lexical = true;
    nested.ops = vec![
        Op::Constant(0), // 'jit-e2e-triple-hot
        Op::StackRef(1), // x
        Op::Call(1),
        Op::Return,
    ];
    nested.constants = vec![callee_sym].into();
    nested.max_stack = 16;
    nested.jit_runtime().set_hot_for_test();
    let nested_v = Value::make_bytecode(nested);
    assert_eq!(
        ev.funcall_general_untraced(nested_v, vec![Value::make_int(14)])
            .expect("nested JIT->JIT call runs"),
        Value::make_int(42)
    );

    // Signal propagation: calling an unbound function from native code must
    // surface the same error the interpreter raises.
    let mut sig = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    sig.lexical = true;
    sig.ops = vec![Op::Constant(0), Op::Call(0), Op::Return];
    sig.constants = vec![Value::symbol("jit-e2e-no-such-function")].into();
    sig.max_stack = 16;
    sig.jit_runtime().set_hot_for_test();
    let sig_v = Value::make_bytecode(sig);
    assert!(
        ev.funcall_general_untraced(sig_v, vec![]).is_err(),
        "void-function must propagate out of native code"
    );
    assert!(
        ev.eval_str("(+ 1 2)").is_ok(),
        "context stays healthy after a propagated signal"
    );
}

/// End-to-end: handler opcodes (PushCatch/PushConditionCase/PopHandler +
/// in-frame Throw) compile and run natively through the funcall seam — catch,
/// rethrow, normal-path PopHandler, deopt inside a protected extent, and
/// specpdl unwinding on a caught throw all mirror the interpreter.
#[cfg(feature = "jit")]
#[test]
fn jit_handlers_through_funcall_seam() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    let mk = |required: usize, ops: Vec<Op>, consts: Vec<Value>, hot: bool| -> Value {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: (1..=required)
                .map(|i| crate::emacs_core::intern::SymId(i as u32))
                .collect(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = consts.into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };

    // 1. Same-function catch + conditional throw:
    //    (lambda (x) (catch 'tag (when x (throw 'tag 42)) 7)).
    //    The handler target (9) is also the normal join — both enter depth 2.
    let catch_fn = |hot: bool| {
        mk(
            1,
            vec![
                Op::Constant(0),  // 0: 'tag            [x 'tag]
                Op::PushCatch(9), // 1: frame, tgt=9    [x]
                Op::StackRef(0),  // 2: x               [x x]
                Op::GotoIfNil(7), // 3:                 [x]
                Op::Constant(0),  // 4: 'tag            [x 'tag]
                Op::Constant(1),  // 5: 42              [x 'tag 42]
                Op::Throw,        // 6: -> handler 9 with [x 42]
                Op::PopHandler,   // 7: normal path     [x]
                Op::Constant(2),  // 8: 7               [x 7]
                Op::Return,       // 9: join + handler: returns TOS
            ],
            vec![
                Value::symbol("jit-h-tag"),
                Value::make_int(42),
                Value::make_int(7),
            ],
            hot,
        )
    };
    let hot_catch = catch_fn(true);
    let cold_catch = catch_fn(false);
    for (arg, want) in [(Value::T, 42), (Value::NIL, 7)] {
        let native = ev
            .funcall_general_untraced(hot_catch, vec![arg])
            .expect("native catch/throw runs");
        let interp = ev
            .funcall_general_untraced(cold_catch, vec![arg])
            .expect("interpreted catch/throw runs");
        assert_eq!(native, Value::make_int(want));
        assert_eq!(native, interp);
        assert_eq!(ev.condition_stack.len(), 0, "handler frames balanced");
    }

    // 2. condition-case catching a signal raised by a CALLED function, plus the
    //    normal path (PopHandler) on the SAME compiled body:
    //    (lambda () (condition-case nil (jit-h-boom) (error 99))).
    let boom_sym = Value::symbol("jit-h-boom");
    let ValueKind::Symbol(boom_id) = boom_sym.kind() else {
        panic!("symbol expected");
    };
    // (car 5) signals wrong-type-argument in the interpreter (the callee is
    // cold bytecode).
    let boom_signal = mk(
        0,
        vec![Op::Constant(0), Op::Car, Op::Return],
        vec![Value::make_int(5)],
        false,
    );
    ev.obarray.set_symbol_function_id(boom_id, boom_signal);
    let cc_fn = |hot: bool| {
        mk(
            0,
            vec![
                Op::PushConditionCase(5), // 0: implicit 'error  []
                Op::Constant(0),          // 1: 'jit-h-boom      [f]
                Op::Call(0),              // 2: signal -> 5      [res]
                Op::PopHandler,           // 3: normal exit
                Op::Return,               // 4: callee result
                Op::Pop,                  // 5: handler: drop the error object
                Op::Constant(1),          // 6: 99
                Op::Return,               // 7
            ],
            vec![boom_sym, Value::make_int(99)],
            hot,
        )
    };
    let hot_cc = cc_fn(true);
    let cold_cc = cc_fn(false);
    let native = ev
        .funcall_general_untraced(hot_cc, vec![])
        .expect("native condition-case catches");
    let interp = ev
        .funcall_general_untraced(cold_cc, vec![])
        .expect("interpreted condition-case catches");
    assert_eq!(native, Value::make_int(99));
    assert_eq!(native, interp);
    assert_eq!(ev.condition_stack.len(), 0);
    // Redefine the callee to return normally: the same compiled body takes the
    // PopHandler path.
    let boom_ok = mk(
        0,
        vec![Op::Constant(0), Op::Return],
        vec![Value::make_int(31)],
        false,
    );
    ev.obarray.set_symbol_function_id(boom_id, boom_ok);
    assert_eq!(
        ev.funcall_general_untraced(hot_cc, vec![])
            .expect("native normal path runs"),
        Value::make_int(31)
    );
    assert_eq!(ev.condition_stack.len(), 0);

    // 3. Unmatched throw propagates out (no-catch), frames balanced:
    //    (lambda () (catch 'a (throw 'b 1))).
    let rethrow = mk(
        0,
        vec![
            Op::Constant(0),  // 'a
            Op::PushCatch(5), // frame, tgt=5
            Op::Constant(1),  // 'b
            Op::Constant(2),  // 1
            Op::Throw,        // no frame matches 'b -> propagate
            Op::Return,       // 5: handler (reachable only via the frame)
        ],
        vec![
            Value::symbol("jit-h-a"),
            Value::symbol("jit-h-b"),
            Value::make_int(1),
        ],
        true,
    );
    assert!(
        ev.funcall_general_untraced(rethrow, vec![]).is_err(),
        "unmatched throw must propagate as no-catch"
    );
    assert_eq!(ev.condition_stack.len(), 0, "frames unwound on propagation");
    assert!(ev.eval_str("(+ 1 2)").is_ok(), "context healthy after");

    // 4. Deopt INSIDE a protected extent (the non-poisoning Push* payoff): a
    //    guard after PushConditionCase compiles; a non-fixnum deopts, the frame
    //    unwind truncates the registered handler frame, and the interpreter
    //    rerun re-registers it and catches its own signal:
    //    (lambda (x) (condition-case nil (1+ x) (error 99))).
    let cc_arith = |hot: bool| {
        mk(
            1,
            vec![
                Op::PushConditionCase(5), // 0:              [x]
                Op::StackRef(0),          // 1:              [x x]
                Op::Add1,                 // 2: guard        [x x+1]
                Op::PopHandler,           // 3:
                Op::Return,               // 4: x+1
                Op::Pop,                  // 5: handler      [x]
                Op::Constant(0),          // 6: 99
                Op::Return,               // 7
            ],
            vec![Value::make_int(99)],
            hot,
        )
    };
    let hot_arith = cc_arith(true);
    let cold_arith = cc_arith(false);
    for arg in [Value::make_int(5), Value::string("boom")] {
        let native = ev
            .funcall_general_untraced(hot_arith, vec![arg])
            .expect("native/deopt path runs");
        let interp = ev
            .funcall_general_untraced(cold_arith, vec![arg])
            .expect("interpreter path runs");
        assert_eq!(native, interp, "deopt-in-extent must match the interpreter");
        assert_eq!(ev.condition_stack.len(), 0);
    }
    assert_eq!(
        ev.funcall_general_untraced(hot_arith, vec![Value::make_int(5)])
            .unwrap(),
        Value::make_int(6)
    );

    // 5. A caught throw unwinds dynamic bindings made inside the extent (the
    //    match shim's unbind_to + bind-stack truncation):
    //    (lambda () (catch 'tag (let ((jit-h-var 123)) (throw 'tag 55)))).
    ev.eval_str("(setq jit-h-var 9)").expect("global value set");
    let unwind = mk(
        0,
        vec![
            Op::Constant(0),  // 0: 'tag
            Op::PushCatch(7), // 1: frame, tgt=7
            Op::Constant(1),  // 2: 123
            Op::VarBind(2),   // 3: bind jit-h-var
            Op::Constant(0),  // 4: 'tag
            Op::Constant(3),  // 5: 55
            Op::Throw,        // 6: caught below; must unbind first
            Op::Return,       // 7: handler -> 55
        ],
        vec![
            Value::symbol("jit-h-tag"),
            Value::make_int(123),
            Value::symbol("jit-h-var"),
            Value::make_int(55),
        ],
        true,
    );
    assert_eq!(
        ev.funcall_general_untraced(unwind, vec![])
            .expect("native catch with varbind runs"),
        Value::make_int(55)
    );
    assert_eq!(
        ev.eval_str("jit-h-var").expect("global survives"),
        Value::make_int(9),
        "caught throw must unwind the dynamic binding"
    );
    assert_eq!(ev.condition_stack.len(), 0);
}

/// End-to-end: `Op::Switch` (pcase/cl-case jump tables) compiles and runs
/// natively through the funcall seam — table hits jump to their static
/// targets, misses fall through, matching the interpreter exactly.
#[cfg(feature = "jit")]
#[test]
fn jit_switch_through_funcall_seam() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::{HashTableTest, LambdaParams};

    let mut ev = Context::new();
    // Jump table {a -> 5, b -> 7} (raw instruction indices: no GNU byte-offset
    // map on natively built chunks, same as the interpreter's resolution).
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        for (name, target) in [("jit-sw-a", 5), ("jit-sw-b", 7)] {
            let key = Value::symbol(name).to_hash_key(&ht.test);
            ht.insert(key, Value::symbol(name), Value::fixnum(target));
        }
    });
    // (lambda (x) (pcase x ('jit-sw-a 1) ('jit-sw-b 2) (_ 0)))
    let mk = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::StackRef(0), // 0: [x x]
            Op::Constant(0), // 1: [x x table]
            Op::Switch,      // 2: [x]
            Op::Constant(3), // 3: miss -> 0
            Op::Return,      // 4
            Op::Constant(1), // 5: 'jit-sw-a -> 1
            Op::Return,      // 6
            Op::Constant(2), // 7: 'jit-sw-b -> 2
            Op::Return,      // 8
        ];
        f.constants = vec![
            table,
            Value::make_int(1),
            Value::make_int(2),
            Value::make_int(0),
        ]
        .into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    let hot = mk(true);
    let cold = mk(false);
    for (arg, want) in [
        (Value::symbol("jit-sw-a"), 1),
        (Value::symbol("jit-sw-b"), 2),
        (Value::symbol("jit-sw-other"), 0),
        (Value::make_int(33), 0),
    ] {
        let native = ev
            .funcall_general_untraced(hot, vec![arg])
            .expect("native switch runs");
        let interp = ev
            .funcall_general_untraced(cold, vec![arg])
            .expect("interpreted switch runs");
        assert_eq!(native, Value::make_int(want));
        assert_eq!(native, interp, "switch must match the interpreter");
    }
}

/// End-to-end: the named-builtin escape hatch (`CallBuiltin`/`CallBuiltinSym`)
/// and `Aset` compile and run natively, matching the interpreter — including
/// the override-aware path (a redefined builtin's function cell is honored by
/// CallBuiltin and deliberately bypassed by CallBuiltinSym, GNU parity).
#[cfg(feature = "jit")]
#[test]
fn jit_named_builtins_through_funcall_seam() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    let mk = |ops: Vec<Op>, consts: Vec<Value>, hot: bool| -> Value {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = consts.into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };

    // (lambda (x) (length x)) via the constants-pool escape hatch.
    let cb = |hot| {
        mk(
            vec![Op::StackRef(0), Op::CallBuiltin(0, 1), Op::Return],
            vec![Value::symbol("length")],
            hot,
        )
    };
    // Same via the symbol-encoded variant.
    let cbs = |hot| {
        mk(
            vec![
                Op::StackRef(0),
                Op::CallBuiltinSym(crate::emacs_core::intern::intern("length"), 1),
                Op::Return,
            ],
            vec![],
            hot,
        )
    };
    for f in [&cb as &dyn Fn(bool) -> Value, &cbs] {
        let hot = f(true);
        let cold = f(false);
        for arg in [Value::string("hello"), Value::NIL] {
            let native = ev
                .funcall_general_untraced(hot, vec![arg])
                .expect("native named builtin runs");
            let interp = ev
                .funcall_general_untraced(cold, vec![arg])
                .expect("interpreted named builtin runs");
            assert_eq!(native, interp, "named builtin must match the interpreter");
        }
        // Signal parity: (length 5) is a wrong-type-argument both ways.
        assert!(
            ev.funcall_general_untraced(hot, vec![Value::make_int(5)])
                .is_err()
        );
    }

    // Aset differential: (lambda (v) (aset v 0 7) v).
    let aset = |hot| {
        mk(
            vec![
                Op::StackRef(0), // v
                Op::StackRef(1), // v (for the return)
                Op::Constant(0), // 0
                Op::Constant(1), // 7
                Op::Aset,        // -> 7
                Op::Pop,
                Op::Return, // v
            ],
            vec![Value::make_int(0), Value::make_int(7)],
            hot,
        )
    };
    let native_vec = Value::vector(vec![Value::make_int(1)]);
    let interp_vec = Value::vector(vec![Value::make_int(1)]);
    let r1 = ev
        .funcall_general_untraced(aset(true), vec![native_vec])
        .expect("native aset runs");
    let r2 = ev
        .funcall_general_untraced(aset(false), vec![interp_vec])
        .expect("interpreted aset runs");
    use crate::emacs_core::print::print_value;
    assert_eq!(print_value(&r1), "[7]");
    assert_eq!(print_value(&r1), print_value(&r2));
}

/// End-to-end: `Op::SaveWindowExcursion` compiles — the body list evaluates
/// under a window-configuration save/restore, matching the interpreter.
#[cfg(feature = "jit")]
#[test]
fn jit_save_window_excursion_through_funcall_seam() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    // (lambda () (save-window-excursion (+ 20 22))) — body list as a constant.
    let body = ev.eval_str("'((+ 20 22))").expect("body list parses");
    let mk = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::Constant(0), Op::SaveWindowExcursion, Op::Return];
        f.constants = vec![body].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    let native = ev
        .funcall_general_untraced(mk(true), vec![])
        .expect("native save-window-excursion runs");
    let interp = ev
        .funcall_general_untraced(mk(false), vec![])
        .expect("interpreted save-window-excursion runs");
    assert_eq!(native, Value::make_int(42));
    assert_eq!(native, interp);

    // Signal parity: a body that signals propagates identically.
    let bad_body = ev.eval_str("'((car 5))").expect("body parses");
    let mk_bad = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::Constant(0), Op::SaveWindowExcursion, Op::Return];
        f.constants = vec![bad_body].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    assert!(ev.funcall_general_untraced(mk_bad(true), vec![]).is_err());
    assert!(ev.funcall_general_untraced(mk_bad(false), vec![]).is_err());
}

/// End-to-end: direct-call speculation — a hot caller whose callee slot is a
/// constant symbol bound to bytecode calls it through the epoch-validated
/// spec shim. fset MUST take effect immediately (GNU default semantics):
/// across calls, after unrelated epoch bumps (re-arm path), and for
/// non-bytecode replacements (permanent slow path).
#[cfg(feature = "jit")]
#[test]
fn jit_direct_call_speculation_tracks_redefinition() {
    crate::test_utils::init_test_tracing();
    // Compiles a deliberately call-only forwarder to exercise direct-call
    // speculation; production would decline it as unprofitable, so disable the
    // profitability gate for this machinery test.
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    let mk_times = |k: i64| -> Value {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        // Two blocks so the JIT inliner (single-block callees only) leaves this a
        // SPEC call — `(if x (* x k) (* x k))`, same value, two basic blocks.
        f.ops = vec![
            Op::StackRef(0),
            Op::GotoIfNil(6),
            Op::StackRef(0),
            Op::Constant(0),
            Op::Mul,
            Op::Return,
            Op::StackRef(0),
            Op::Constant(0),
            Op::Mul,
            Op::Return,
        ];
        f.constants = vec![Value::make_int(k)].into();
        f.max_stack = 16;
        Value::make_bytecode(f)
    };
    let g_sym = Value::symbol("jit-spec-g");
    let ValueKind::Symbol(g_id) = g_sym.kind() else {
        panic!("symbol expected");
    };
    ev.obarray.set_symbol_function_id(g_id, mk_times(2));

    // Hot caller (lambda (x) (jit-spec-g x)) — the exact speculation shape:
    // Constant(sym) StackRef Call(1).
    let mk_caller = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
        f.constants = vec![g_sym].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    let hot = mk_caller(true);
    let cold = mk_caller(false);
    let five = vec![Value::make_int(5)];

    // 1) Speculated direct call (compile-time binding = doubler) — and prove
    //    the spec shim engaged (the generic path would also compute 10; the
    //    engagement counter only exists in debug builds).
    #[cfg(debug_assertions)]
    let spec_before =
        crate::emacs_core::jit::compile::SPEC_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        ev.funcall_general_untraced(hot, five.clone()).unwrap(),
        Value::make_int(10)
    );
    #[cfg(debug_assertions)]
    assert!(
        crate::emacs_core::jit::compile::SPEC_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed)
            > spec_before,
        "the speculated call site must route through the spec shim"
    );
    // 2) Redefine: MUST take effect on the next call (epoch moved, binding
    //    differs -> strict symbol path).
    ev.obarray.set_symbol_function_id(g_id, mk_times(3));
    assert_eq!(
        ev.funcall_general_untraced(hot, five.clone()).unwrap(),
        Value::make_int(15)
    );
    assert_eq!(
        ev.funcall_general_untraced(cold, five.clone()).unwrap(),
        Value::make_int(15),
        "interpreter agrees"
    );
    // 3) Unrelated epoch bump (different symbol): caller still correct.
    let other = Value::symbol("jit-spec-unrelated");
    let ValueKind::Symbol(other_id) = other.kind() else {
        panic!("symbol expected");
    };
    ev.obarray.set_symbol_function_id(other_id, mk_times(7));
    assert_eq!(
        ev.funcall_general_untraced(hot, five.clone()).unwrap(),
        Value::make_int(15)
    );
    // 4) Replace with a non-bytecode callable (interpreted lambda): the spec
    //    site stays disarmed and the symbol path resolves it.
    let lam = ev
        .eval_str("(lambda (x) (* x 10))")
        .expect("lambda evaluates");
    ev.obarray.set_symbol_function_id(g_id, lam);
    assert_eq!(
        ev.funcall_general_untraced(hot, five.clone()).unwrap(),
        Value::make_int(50)
    );
    // 5) fmakunbound: the call must now signal void-function.
    ev.obarray.fmakunbound_id(g_id);
    assert!(ev.funcall_general_untraced(hot, five).is_err());
}

/// The strictest case: the callee redefines ITSELF mid-caller — the second
/// speculated site in the same native frame must see the new binding (the
/// interpreter resolves per call; the spec shim's per-call epoch check must
/// match it exactly).
#[cfg(feature = "jit")]
#[test]
fn jit_direct_call_speculation_mid_execution_redefinition() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    let h_sym = Value::symbol("jit-spec-h");
    let ValueKind::Symbol(h_id) = h_sym.kind() else {
        panic!("symbol expected");
    };
    // h2: (lambda () 2)
    let mut h2 = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    h2.lexical = true;
    h2.ops = vec![Op::Constant(0), Op::Return];
    h2.constants = vec![Value::make_int(2)].into();
    h2.max_stack = 16;
    let h2_val = Value::make_bytecode(h2);
    // h1: (lambda () (fset 'jit-spec-h h2) 1) — redefines itself, returns 1.
    let mut h1 = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    h1.lexical = true;
    h1.ops = vec![
        Op::Constant(0), // 'fset
        Op::Constant(1), // 'jit-spec-h
        Op::Constant(2), // h2
        Op::Call(2),
        Op::Pop,
        Op::Constant(3), // 1
        Op::Return,
    ];
    h1.constants = vec![Value::symbol("fset"), h_sym, h2_val, Value::make_int(1)].into();
    h1.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(h_id, Value::make_bytecode(h1));

    // Hot caller: (lambda () (list (jit-spec-h) (jit-spec-h))) — both call
    // sites speculate on h1 at compile time.
    let mk_caller = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::Constant(0),
            Op::Call(0),
            Op::Constant(0),
            Op::Call(0),
            Op::List(2),
            Op::Return,
        ];
        f.constants = vec![h_sym].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    use crate::emacs_core::print::print_value;
    // First invocation: site 1 hits h1 (which fsets h -> h2); site 2 MUST
    // already see h2.
    let r = ev
        .funcall_general_untraced(mk_caller(true), vec![])
        .expect("native caller runs");
    assert_eq!(print_value(&r), "(1 2)");
    // Second invocation: both sites see h2.
    let hot2 = mk_caller(true);
    let r = ev
        .funcall_general_untraced(hot2, vec![])
        .expect("native caller runs");
    assert_eq!(print_value(&r), "(2 2)");
    // Interpreter differential on a fresh pair: reinstall h1 and compare.
    // (Rebuild h1 since the previous one's constants still hold h2.)
    let mut h1b = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    h1b.lexical = true;
    h1b.ops = vec![
        Op::Constant(0),
        Op::Constant(1),
        Op::Constant(2),
        Op::Call(2),
        Op::Pop,
        Op::Constant(3),
        Op::Return,
    ];
    h1b.constants = vec![Value::symbol("fset"), h_sym, h2_val, Value::make_int(1)].into();
    h1b.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(h_id, Value::make_bytecode(h1b));
    let r = ev
        .funcall_general_untraced(mk_caller(false), vec![])
        .expect("interpreted caller runs");
    assert_eq!(print_value(&r), "(1 2)", "interpreter agrees on strictness");
}

/// Build `(lambda (a1..aK) (NAME a1..aK))` as hand-rolled bytecode — the exact
/// `Constant(sym) StackRef* Call(k)` shape `find_spec_sites` speculates on.
/// Callers disable the profitability gate (call-only body) before tiering.
#[cfg(feature = "jit")]
fn jit_subr_spec_caller(name: &str, nargs: usize, hot: bool) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs as u32)
            .map(crate::emacs_core::intern::SymId)
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    let mut ops = vec![Op::Constant(0)];
    // After pushing the callee, every remaining arg sits `nargs` below the top.
    for _ in 0..nargs {
        ops.push(Op::StackRef(nargs as u16));
    }
    ops.push(Op::Call(nargs as u16));
    ops.push(Op::Return);
    f.ops = ops;
    f.constants = vec![Value::symbol(name)].into();
    f.max_stack = 16;
    if hot {
        f.jit_runtime().set_hot_for_test();
    }
    Value::make_bytecode(f)
}

/// Debug-build snapshot of the three subr-spec counters (entries/fast/generic).
#[cfg(all(feature = "jit", debug_assertions))]
fn jit_subr_spec_counters() -> (u64, u64, u64) {
    use crate::emacs_core::jit::compile;
    use std::sync::atomic::Ordering;
    (
        compile::SUBR_SPEC_COUNT.load(Ordering::Relaxed),
        compile::SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed),
        compile::SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed),
    )
}

/// Gap 1 engagement + parity: a hot `(recordp x)` compiles with a PREDICATE
/// spec site (armed tag test), fires the fast path, and matches the
/// interpreter on records, char-tables (a veclike that is NOT a record),
/// symbols, fixnums, and nil.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_pred_recordp_engages_and_matches() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    // Root each cross-eval Rust local AT CREATION (see the swp-p test note).
    let hot = jit_subr_spec_caller("recordp", 1, true);
    ev.push_specpdl_root(hot);
    let cold = jit_subr_spec_caller("recordp", 1, false);
    ev.push_specpdl_root(cold);
    let record = ev.eval_str("(record 'foo 1 2)").expect("record");
    ev.push_specpdl_root(record);
    let chartable = ev.eval_str("(make-char-table 'test)").expect("char-table");
    ev.push_specpdl_root(chartable);
    let probe_string = Value::string("s");
    ev.push_specpdl_root(probe_string);
    let cases = [
        record,
        chartable,
        Value::symbol("x"),
        Value::make_int(3),
        Value::NIL,
        probe_string,
    ];
    #[cfg(debug_assertions)]
    let (entries0, fast0, _) = jit_subr_spec_counters();
    for arg in cases {
        let native = ev
            .funcall_general_untraced(hot, vec![arg])
            .expect("native recordp caller");
        let interp = ev
            .funcall_general_untraced(cold, vec![arg])
            .expect("interpreted recordp caller");
        assert_eq!(native.bits(), interp.bits(), "recordp parity hot vs cold");
    }
    let native = ev
        .funcall_general_untraced(hot, vec![record])
        .expect("native");
    assert!(native.is_truthy(), "recordp on a record is t");
    let native = ev
        .funcall_general_untraced(hot, vec![chartable])
        .expect("native");
    assert!(native.is_nil(), "recordp on a char-table is nil");
    #[cfg(debug_assertions)]
    {
        let (entries1, fast1, _) = jit_subr_spec_counters();
        assert!(
            entries1 > entries0,
            "the recordp site must route through a subr spec shim"
        );
        assert!(fast1 > fast0, "the armed predicate fast path must fire");
    }
}

/// `vectorp` must stay a GENERAL site (correct through the real builtin, no
/// tag test): bool-vectors and sentinel char-tables are genuine
/// `VecLikeType::Vector` objects an inline tag test would misclassify as `t`.
/// Runs THROUGH the JIT and expects the builtin's answers.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_vectorp_stays_general() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let hot = jit_subr_spec_caller("vectorp", 1, true);
    let boolvec = ev
        .eval_str("(make-bool-vector 3 t)")
        .expect("make-bool-vector");
    let chartable = ev.eval_str("(make-char-table 'test)").expect("char-table");
    let vector = ev.eval_str("[1 2 3]").expect("vector");
    #[cfg(debug_assertions)]
    let (_, fast0, _) = jit_subr_spec_counters();
    let native = ev
        .funcall_general_untraced(hot, vec![boolvec])
        .expect("native vectorp caller");
    assert!(
        native.is_nil(),
        "(vectorp (make-bool-vector 3 t)) is nil THROUGH the JIT — vectorp stayed General"
    );
    let native = ev
        .funcall_general_untraced(hot, vec![chartable])
        .expect("native");
    assert!(native.is_nil(), "(vectorp (make-char-table)) is nil");
    let native = ev
        .funcall_general_untraced(hot, vec![vector])
        .expect("native");
    assert!(native.is_truthy(), "(vectorp [1 2 3]) is t");
    #[cfg(debug_assertions)]
    {
        let (_, fast1, _) = jit_subr_spec_counters();
        assert!(
            fast1 > fast0,
            "vectorp engages as a GENERAL subr spec site (direct dispatch of the real builtin)"
        );
    }
}

/// `symbol-with-pos-p` predicate site: exact under BOTH
/// `symbols-with-pos-enabled` states (the builtin is flag-independent — it
/// tests the representation, not the bare-symbol view).
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_symbol_with_pos_p_both_flag_states() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    // Heap values held in Rust locals across evals must be rooted AT
    // CREATION: under NEOVM_GC_STRESS every allocation-bearing safe point
    // collects, so the very next eval can free an unrooted local.
    let hot = jit_subr_spec_caller("symbol-with-pos-p", 1, true);
    ev.push_specpdl_root(hot);
    let cold = jit_subr_spec_caller("symbol-with-pos-p", 1, false);
    ev.push_specpdl_root(cold);
    let swp = ev
        .eval_str("(position-symbol 'foo 5)")
        .expect("position-symbol");
    ev.push_specpdl_root(swp);
    for flag in ["t", "nil"] {
        ev.eval_str(&format!("(setq symbols-with-pos-enabled {flag})"))
            .expect("set flag");
        for arg in [swp, Value::symbol("foo"), Value::make_int(1)] {
            let native = ev
                .funcall_general_untraced(hot, vec![arg])
                .expect("native swp-p caller");
            let interp = ev
                .funcall_general_untraced(cold, vec![arg])
                .expect("interpreted swp-p caller");
            assert_eq!(
                native.bits(),
                interp.bits(),
                "symbol-with-pos-p parity (flag={flag})"
            );
        }
        let native = ev.funcall_general_untraced(hot, vec![swp]).expect("native");
        assert!(
            native.is_truthy(),
            "swp-p on a symbol-with-pos (flag={flag})"
        );
    }
}

/// `equal-including-properties` site: bitwise-equal args hit the shim fast
/// path (`t`, FAST counter); everything else bounces to the generic block and
/// runs the REAL builtin — equal-but-distinct strings (t), property mismatch
/// (nil), and distinct same-bit NaN boxes (t, GNU bit-pattern float equality).
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_eq_incl_props_hit_and_miss() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let hot = jit_subr_spec_caller("equal-including-properties", 2, true);
    let cold = jit_subr_spec_caller("equal-including-properties", 2, false);
    let s = Value::string("hello");
    // Same object: the bitwise fast path answers t.
    #[cfg(debug_assertions)]
    let (_, fast0, _) = jit_subr_spec_counters();
    let native = ev
        .funcall_general_untraced(hot, vec![s, s])
        .expect("native eq-incl-props");
    assert!(native.is_truthy(), "same-object strings are t");
    #[cfg(debug_assertions)]
    {
        let (_, fast1, _) = jit_subr_spec_counters();
        assert!(fast1 > fast0, "bitwise-equal args take the shim fast path");
    }
    // Distinct but equal strings, no properties: miss -> generic -> t.
    let cases: Vec<(Value, Value)> = {
        let with_props = Value::string("abcd");
        crate::emacs_core::value::set_string_text_properties_for_value(
            with_props,
            vec![crate::emacs_core::value::StringTextPropertyRun {
                start: 1,
                end: 3,
                plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
            }],
        );
        vec![
            (Value::string("hello"), Value::string("hello")), // t (deep equal)
            (with_props, Value::string("abcd")),              // nil (props differ)
            (Value::string("ab"), Value::string("ac")),       // nil
            // Distinct NaN boxes, same bit pattern: t (GNU float equality).
            (Value::make_float(f64::NAN), Value::make_float(f64::NAN)),
            (Value::make_int(7), Value::make_int(7)), // t (bitwise hit)
        ]
    };
    for (a, b) in cases {
        let native = ev
            .funcall_general_untraced(hot, vec![a, b])
            .expect("native eq-incl-props caller");
        let interp = ev
            .funcall_general_untraced(cold, vec![a, b])
            .expect("interpreted eq-incl-props caller");
        assert_eq!(
            native.bits(),
            interp.bits(),
            "equal-including-properties parity hot vs cold"
        );
    }
}

/// GENERAL-kind engagement on a side-effecting builtin: `(put sym prop val)`
/// dispatches directly when armed and the effect + result match the
/// interpreter.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_put_general_engages_and_matches() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let hot = jit_subr_spec_caller("put", 3, true);
    #[cfg(debug_assertions)]
    let (entries0, fast0, _) = jit_subr_spec_counters();
    let r = ev
        .funcall_general_untraced(
            hot,
            vec![
                Value::symbol("jit-subr-spec-put-sym"),
                Value::symbol("prop"),
                Value::make_int(41),
            ],
        )
        .expect("native put caller");
    assert_eq!(r, Value::make_int(41), "put returns the value");
    assert_eq!(
        format_eval_result(&ev.eval_str("(get 'jit-subr-spec-put-sym 'prop)")),
        "OK 41",
        "the armed direct dispatch performed put's side effect"
    );
    #[cfg(debug_assertions)]
    {
        let (entries1, fast1, _) = jit_subr_spec_counters();
        assert!(entries1 > entries0, "put site routes through the subr shim");
        assert!(fast1 > fast0, "put site dispatches directly when armed");
    }
}

/// Redefinition semantics (GNU default parity): fset over a speculated subr
/// takes effect on the very next call (disarmed site -> generic block resolves
/// the new binding); restoring the ORIGINAL subr object re-arms the site
/// (fast path resumes — counters prove re-arm, not permanent slow); an
/// UNRELATED epoch bump also re-arms.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_tracks_redefinition_and_rearms() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    // Root each cross-eval Rust local AT CREATION (see the swp-p test note).
    let hot = jit_subr_spec_caller("recordp", 1, true);
    ev.push_specpdl_root(hot);
    let record = ev.eval_str("(record 'foo 1)").expect("record");
    ev.push_specpdl_root(record);
    let original = ev
        .eval_str("(symbol-function 'recordp)")
        .expect("original recordp");
    ev.push_specpdl_root(original);

    // 1) Armed: t.
    let r = ev
        .funcall_general_untraced(hot, vec![record])
        .expect("native");
    assert!(r.is_truthy());

    // 2) Redefine recordp -> a lambda returning 'redefined. MUST take effect
    //    on the next call (site disarms, generic path resolves the closure).
    ev.eval_str("(fset 'recordp (lambda (x) 'redefined))")
        .expect("fset recordp");
    let r = ev
        .funcall_general_untraced(hot, vec![record])
        .expect("native after fset");
    assert_eq!(
        r,
        Value::symbol("redefined"),
        "fset takes effect immediately on the speculated site"
    );

    // 3) Restore the ORIGINAL subr object: bits == expected again, so the
    //    next call re-validates and RE-ARMS (fast path resumes).
    let ValueKind::Symbol(recordp_id) = Value::symbol("recordp").kind() else {
        panic!("symbol expected");
    };
    ev.obarray.set_symbol_function_id(recordp_id, original);
    #[cfg(debug_assertions)]
    let (_, fast0, gen0) = jit_subr_spec_counters();
    let r = ev
        .funcall_general_untraced(hot, vec![record])
        .expect("native after restore");
    assert!(r.is_truthy(), "restored recordp answers t again");
    #[cfg(debug_assertions)]
    {
        let (_, fast1, gen1) = jit_subr_spec_counters();
        assert!(
            fast1 > fast0,
            "restoring the same subr object re-arms the site"
        );
        assert_eq!(gen1, gen0, "the re-armed call does not bounce generic");
    }

    // 4) UNRELATED epoch bump: still fast (re-validate + re-arm, not slow).
    ev.eval_str("(fset 'jit-subr-spec-unrelated (lambda () 1))")
        .expect("unrelated fset");
    #[cfg(debug_assertions)]
    let (_, fast2, gen2) = jit_subr_spec_counters();
    let r = ev
        .funcall_general_untraced(hot, vec![record])
        .expect("native after unrelated bump");
    assert!(r.is_truthy());
    #[cfg(debug_assertions)]
    {
        let (_, fast3, gen3) = jit_subr_spec_counters();
        assert!(
            fast3 > fast2,
            "unrelated epoch bump re-arms via re-validation"
        );
        assert_eq!(gen3, gen2, "no generic bounce after an unrelated bump");
    }
}

/// Arity conservatism: `(put 'a 'b)` (n < min_args) must NOT be speculated —
/// the generic path signals wrong-number-of-arguments byte-identically hot vs
/// cold. And a `Many`-variant builtin (`+`) gets NO subr site at all.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_arity_and_variadic_stay_generic() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    use crate::emacs_core::error::map_flow;
    let mut ev = Context::new();

    // n=2 < put's min_args=3: no site; identical signal hot vs cold.
    let hot = jit_subr_spec_caller("put", 2, true);
    let cold = jit_subr_spec_caller("put", 2, false);
    #[cfg(debug_assertions)]
    let (entries0, _, _) = jit_subr_spec_counters();
    let args = || vec![Value::symbol("a"), Value::symbol("b")];
    let native_err = ev
        .funcall_general_untraced(hot, args())
        .expect_err("put with 2 args signals");
    let interp_err = ev
        .funcall_general_untraced(cold, args())
        .expect_err("interpreter agrees");
    assert_eq!(
        format!("{:?}", map_flow(native_err)),
        format!("{:?}", map_flow(interp_err)),
        "wrong-number-of-arguments is byte-identical hot vs cold"
    );

    // A Many-variant builtin (+) is never speculated.
    let plus_hot = jit_subr_spec_caller("+", 2, true);
    let r = ev
        .funcall_general_untraced(plus_hot, vec![Value::make_int(1), Value::make_int(2)])
        .expect("native + caller");
    assert_eq!(r, Value::make_int(3));
    #[cfg(debug_assertions)]
    {
        let (entries1, _, _) = jit_subr_spec_counters();
        assert_eq!(
            entries1, entries0,
            "neither the under-arity put nor the Many-variant + creates a subr spec site"
        );
    }
}

/// In-place entry-rewrite soundness: registering a fixed one-slot subr under the SAME
/// name rewrites the static registry entry while the immediate subr bits stay
/// unchanged, and bumps function_epoch. The site re-validates, RE-ARMS (bits
/// still match), and the ARMED path must call the NEW function — proving the
/// shim reads the entry FRESH per call instead of caching the fn pointer.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_inplace_rewrite_calls_fresh_entry() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    fn f1(_: &mut Context, _: Value) -> crate::emacs_core::error::EvalResult {
        Ok(Value::make_int(1))
    }
    fn f2(_: &mut Context, _: Value) -> crate::emacs_core::error::EvalResult {
        Ok(Value::make_int(2))
    }
    let mut ev = Context::new();
    ev.register_subr(SubrSpec::fixed1(
        "jit-subr-spec-rewrite",
        f1,
        FixedMin1::One,
    ));
    let hot = jit_subr_spec_caller("jit-subr-spec-rewrite", 1, true);
    #[cfg(debug_assertions)]
    let (_, fast0, _) = jit_subr_spec_counters();
    let r = ev
        .funcall_general_untraced(hot, vec![Value::NIL])
        .expect("native rewrite caller");
    assert_eq!(r, Value::make_int(1), "armed dispatch runs f1");
    // Re-register: entry rewritten in place (bits identical), epoch bumped.
    ev.register_subr(SubrSpec::fixed1(
        "jit-subr-spec-rewrite",
        f2,
        FixedMin1::One,
    ));
    let r = ev
        .funcall_general_untraced(hot, vec![Value::NIL])
        .expect("native after in-place rewrite");
    assert_eq!(
        r,
        Value::make_int(2),
        "the ARMED path calls the NEW function (fresh entry read)"
    );
    #[cfg(debug_assertions)]
    {
        let (_, fast1, _) = jit_subr_spec_counters();
        assert!(
            fast1 >= fast0 + 2,
            "both calls (pre- and post-rewrite) took the armed fast path"
        );
    }
}

/// R2 phase 2 (Op::Call Many allowlist) engagement + parity: `re-search-forward`
/// is an allowlisted `SubrFn::Many` builtin — round-1 EXCLUDED as non-fixed-arity
/// — that now routes through the `SubrGeneral` subr shim. A hot `(re-search-forward
/// re)` engages a spec site, fires the armed fast path, performs the identical
/// search side effect (point move + match-state), and matches the interpreter on
/// the RESULT and on `(point)`/`(match-beginning 0)`/`(match-end 0)`.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_many_re_search_forward_engages_and_matches() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"hello world\")")
        .expect("buffer content");
    // Root each cross-eval Rust local AT CREATION (see the swp-p test note).
    let hot = jit_subr_spec_caller("re-search-forward", 1, true);
    ev.push_specpdl_root(hot);
    let cold = jit_subr_spec_caller("re-search-forward", 1, false);
    ev.push_specpdl_root(cold);
    #[cfg(debug_assertions)]
    let (entries0, fast0, _) = jit_subr_spec_counters();

    fn probe(ev: &mut Context) -> String {
        format_eval_result(&ev.eval_str("(list (point) (match-beginning 0) (match-end 0))"))
    }
    ev.eval_str("(goto-char (point-min))")
        .expect("point to bob");
    let native = ev
        .funcall_general_untraced(hot, vec![Value::string("world")])
        .expect("native re-search-forward");
    let native_state = probe(&mut ev);
    ev.eval_str("(goto-char (point-min))")
        .expect("point to bob");
    let interp = ev
        .funcall_general_untraced(cold, vec![Value::string("world")])
        .expect("interp re-search-forward");
    let interp_state = probe(&mut ev);
    assert_eq!(
        native.bits(),
        interp.bits(),
        "re-search-forward result parity hot vs cold"
    );
    assert_eq!(
        native_state, interp_state,
        "point + match-state parity after the armed Many dispatch"
    );
    #[cfg(debug_assertions)]
    {
        let (entries1, fast1, _) = jit_subr_spec_counters();
        assert!(
            entries1 > entries0,
            "the re-search-forward Many site routes through the subr spec shim"
        );
        assert!(fast1 > fast0, "the armed Many fast path must fire");
    }
}

/// Residual-coverage audit (task A PART 2): `get-text-property` — the READ
/// sibling of the already-allowlisted `put-text-property`, and the hottest
/// UNCOVERED `Op::Call` builtin in the font-lock SUBR-MIX (11.0%). A hot
/// `(get-text-property POS PROP)` must engage a `Many` subr spec site, fire the
/// armed fast path, and return the interpreter's EXACT value at a propertized
/// position, a different property, and an unpropertized position.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_many_get_text_property_engages_and_matches() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str(
        "(progn (insert \"hello world\") \
                (put-text-property 1 6 'face 'bold) \
                (put-text-property 1 6 'depth 42))",
    )
    .expect("buffer + text properties");
    #[cfg(debug_assertions)]
    let (entries0, fast0, _) = jit_subr_spec_counters();
    let hot = jit_subr_spec_caller("get-text-property", 2, true);
    let cold = jit_subr_spec_caller("get-text-property", 2, false);
    for (pos, prop, label) in [
        (2i64, "face", "inside->symbol"),
        (2, "depth", "inside->fixnum"),
        (2, "absent", "inside->nil"),
        (9, "face", "outside->nil"),
    ] {
        let args = || vec![Value::make_int(pos), Value::symbol(prop)];
        let native = ev
            .funcall_general_untraced(hot, args())
            .unwrap_or_else(|e| panic!("native {label}: {e:?}"));
        let interp = ev
            .funcall_general_untraced(cold, args())
            .unwrap_or_else(|e| panic!("interp {label}: {e:?}"));
        assert_eq!(
            native.bits(),
            interp.bits(),
            "get-text-property {label} parity hot vs cold"
        );
    }
    // Anchor an absolute value so the test can't pass on a mutual bug.
    assert_eq!(
        format_eval_result(&ev.eval_str("(get-text-property 2 'face)")),
        "OK bold"
    );
    #[cfg(debug_assertions)]
    if std::env::var("NEOVM_JIT_FORCE_DEOPT").as_deref() != Ok("1")
        && std::env::var("NEOVM_JIT_FORCE_SLOW_SPEC").as_deref() != Ok("1")
    {
        let (entries1, fast1, _) = jit_subr_spec_counters();
        assert!(
            entries1 > entries0,
            "the get-text-property Many site routes through the subr spec shim"
        );
        assert!(fast1 > fast0, "the armed Many fast path must fire");
    }
}

/// R2 phase 2 MUST-NAIL — SIDE-EFFECT goldens: the armed native dispatch of an
/// allowlisted `SubrFn::Many` builtin performs byte-identical state mutations to
/// the interpreter. Covers `looking-at` / `re-search-forward` match-state,
/// `set-match-data` round-trip, and `parse-partial-sexp`'s full state list.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_many_side_effect_goldens_match_interp() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"foo (bar baz) qux\")")
        .expect("buffer");

    let match_state = |ev: &mut Context| {
        format_eval_result(&ev.eval_str("(list (match-beginning 0) (match-end 0) (match-data))"))
    };

    // looking-at at bob: result + match-state parity.
    let la_hot = jit_subr_spec_caller("looking-at", 1, true);
    let la_cold = jit_subr_spec_caller("looking-at", 1, false);
    ev.eval_str("(goto-char (point-min))").unwrap();
    let a = ev
        .funcall_general_untraced(la_hot, vec![Value::string("foo ")])
        .expect("looking-at armed");
    let a_md = match_state(&mut ev);
    ev.eval_str("(goto-char (point-min))").unwrap();
    let i = ev
        .funcall_general_untraced(la_cold, vec![Value::string("foo ")])
        .expect("looking-at interp");
    let i_md = match_state(&mut ev);
    assert_eq!(a.bits(), i.bits(), "looking-at result parity");
    assert_eq!(a_md, i_md, "looking-at match-state golden armed==interp");

    // re-search-forward: point-move + match-state parity.
    let rsf_hot = jit_subr_spec_caller("re-search-forward", 1, true);
    let rsf_cold = jit_subr_spec_caller("re-search-forward", 1, false);
    let rsf_state = |ev: &mut Context| {
        format_eval_result(&ev.eval_str("(list (point) (match-beginning 0) (match-end 0))"))
    };
    ev.eval_str("(goto-char (point-min))").unwrap();
    let a = ev
        .funcall_general_untraced(rsf_hot, vec![Value::string("bar")])
        .expect("rsf armed");
    let a_state = rsf_state(&mut ev);
    ev.eval_str("(goto-char (point-min))").unwrap();
    let i = ev
        .funcall_general_untraced(rsf_cold, vec![Value::string("bar")])
        .expect("rsf interp");
    let i_state = rsf_state(&mut ev);
    assert_eq!(a.bits(), i.bits(), "re-search-forward result parity");
    assert_eq!(
        a_state, i_state,
        "re-search-forward point+match golden armed==interp"
    );

    // set-match-data round-trip: set '(3 6), read back via (match-data).
    let smd_hot = jit_subr_spec_caller("set-match-data", 1, true);
    let smd_cold = jit_subr_spec_caller("set-match-data", 1, false);
    let payload = ev.eval_str("(list 3 6)").unwrap();
    ev.funcall_general_untraced(smd_hot, vec![payload])
        .expect("set-match-data armed");
    let a_rt = format_eval_result(&ev.eval_str("(match-data)"));
    let payload = ev.eval_str("(list 3 6)").unwrap();
    ev.funcall_general_untraced(smd_cold, vec![payload])
        .expect("set-match-data interp");
    let i_rt = format_eval_result(&ev.eval_str("(match-data)"));
    assert_eq!(a_rt, i_rt, "set-match-data round-trip golden armed==interp");

    // parse-partial-sexp: full state list parity (single hot vs cold call).
    let pp_hot = jit_subr_spec_caller("parse-partial-sexp", 2, true);
    let pp_cold = jit_subr_spec_caller("parse-partial-sexp", 2, false);
    let a = ev
        .funcall_general_untraced(pp_hot, vec![Value::make_int(1), Value::make_int(12)])
        .expect("pp armed");
    let i = ev
        .funcall_general_untraced(pp_cold, vec![Value::make_int(1), Value::make_int(12)])
        .expect("pp interp");
    assert_eq!(
        crate::emacs_core::print::print_value(&a),
        crate::emacs_core::print::print_value(&i),
        "parse-partial-sexp state list golden armed==interp"
    );
}

/// R2 phase 2 MUST-NAIL — SIGNAL parity: an armed Many site that must signal
/// produces the byte-identical `Flow` (error symbol + payload) as the
/// interpreter. `re-search-forward` no-match -> `search-failed`; `scan-sexps`
/// over unbalanced parens -> `scan-error`; `put-text-property` into read-only
/// text -> `text-read-only`.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_many_signal_parity_armed_vs_interp() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    use crate::emacs_core::error::map_flow;
    let mut ev = Context::new();
    ev.eval_str("(insert \"hello (world\")").expect("buffer"); // unbalanced paren

    // re-search-forward, no match, no NOERROR -> search-failed.
    let rsf_hot = jit_subr_spec_caller("re-search-forward", 1, true);
    let rsf_cold = jit_subr_spec_caller("re-search-forward", 1, false);
    ev.eval_str("(goto-char (point-min))").unwrap();
    let e_a = ev
        .funcall_general_untraced(rsf_hot, vec![Value::string("ZZZ-nomatch")])
        .expect_err("rsf armed signals");
    ev.eval_str("(goto-char (point-min))").unwrap();
    let e_i = ev
        .funcall_general_untraced(rsf_cold, vec![Value::string("ZZZ-nomatch")])
        .expect_err("rsf interp signals");
    assert_eq!(
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_a)
        )),
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_i)
        )),
        "re-search-forward search-failed Flow parity armed==interp"
    );

    // scan-sexps over the unbalanced "(world" -> scan-error.
    let ss_hot = jit_subr_spec_caller("scan-sexps", 2, true);
    let ss_cold = jit_subr_spec_caller("scan-sexps", 2, false);
    let e_a = ev
        .funcall_general_untraced(ss_hot, vec![Value::make_int(7), Value::make_int(1)])
        .expect_err("scan-sexps armed signals");
    let e_i = ev
        .funcall_general_untraced(ss_cold, vec![Value::make_int(7), Value::make_int(1)])
        .expect_err("scan-sexps interp signals");
    assert_eq!(
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_a)
        )),
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_i)
        )),
        "scan-sexps scan-error Flow parity armed==interp"
    );

    // put-text-property into read-only text -> text-read-only.
    ev.eval_str("(put-text-property 1 6 'read-only t)")
        .expect("mark read-only");
    let ptp_hot = jit_subr_spec_caller("put-text-property", 4, true);
    let ptp_cold = jit_subr_spec_caller("put-text-property", 4, false);
    let ro_args = || {
        vec![
            Value::make_int(1),
            Value::make_int(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ]
    };
    let e_a = ev
        .funcall_general_untraced(ptp_hot, ro_args())
        .expect_err("ptp armed signals");
    let e_i = ev
        .funcall_general_untraced(ptp_cold, ro_args())
        .expect_err("ptp interp signals");
    assert_eq!(
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_a)
        )),
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_i)
        )),
        "put-text-property text-read-only Flow parity armed==interp"
    );
}

/// R2 phase 2 MUST-NAIL — put-text-property is the ONLY allowlisted Many target
/// whose arity is BODY-enforced (declared as `0..unbounded`; real 4..5
/// checked in textprop.rs). So the spec site is created for ANY nargs and the
/// armed path reaches the body's own arity check — which must signal
/// byte-identically to the interpreter for BOTH under-arity `Op::Call(2)` and
/// over-arity `Op::Call(6)`. Also asserts the valid `Op::Call(4)` side effect.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_many_put_text_property_arity_and_effect() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    use crate::emacs_core::error::map_flow;
    let mut ev = Context::new();
    ev.eval_str("(insert \"hello world\")").expect("buffer");

    // Valid Op::Call(4): side-effect + result parity.
    let ptp_hot = jit_subr_spec_caller("put-text-property", 4, true);
    let ptp_cold = jit_subr_spec_caller("put-text-property", 4, false);
    let ok_args = || {
        vec![
            Value::make_int(1),
            Value::make_int(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ]
    };
    let a = ev
        .funcall_general_untraced(ptp_hot, ok_args())
        .expect("ptp(4) armed");
    let a_get = format_eval_result(&ev.eval_str("(get-text-property 1 'face)"));
    ev.eval_str("(set-text-properties 1 12 nil)").ok();
    let i = ev
        .funcall_general_untraced(ptp_cold, ok_args())
        .expect("ptp(4) interp");
    let i_get = format_eval_result(&ev.eval_str("(get-text-property 1 'face)"));
    assert_eq!(a.bits(), i.bits(), "put-text-property(4) result parity");
    assert_eq!(
        a_get, i_get,
        "put-text-property(4) side-effect golden armed==interp"
    );

    // Op::Call(2): below the body-enforced min (4) -> wrong-number-of-arguments.
    let ptp2_hot = jit_subr_spec_caller("put-text-property", 2, true);
    let ptp2_cold = jit_subr_spec_caller("put-text-property", 2, false);
    let two = || vec![Value::make_int(1), Value::make_int(2)];
    let e_a = ev
        .funcall_general_untraced(ptp2_hot, two())
        .expect_err("ptp(2) armed signals");
    let e_i = ev
        .funcall_general_untraced(ptp2_cold, two())
        .expect_err("ptp(2) interp signals");
    assert_eq!(
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_a)
        )),
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_i)
        )),
        "put-text-property(2) under-arity signal parity armed==interp"
    );

    // Op::Call(6): above the body-enforced max (5) -> wrong-number-of-arguments.
    let ptp6_hot = jit_subr_spec_caller("put-text-property", 6, true);
    let ptp6_cold = jit_subr_spec_caller("put-text-property", 6, false);
    let six = || {
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::symbol("face"),
            Value::symbol("bold"),
            Value::NIL,
            Value::NIL,
        ]
    };
    let e_a = ev
        .funcall_general_untraced(ptp6_hot, six())
        .expect_err("ptp(6) armed signals");
    let e_i = ev
        .funcall_general_untraced(ptp6_cold, six())
        .expect_err("ptp(6) interp signals");
    assert_eq!(
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_a)
        )),
        format_eval_result(&Err::<Value, crate::emacs_core::error::EvalError>(
            map_flow(e_i)
        )),
        "put-text-property(6) over-arity signal parity armed==interp"
    );
}

/// R2 phase 2 MUST-NAIL — advice/redefinition deopt: the allowlisted Many sites
/// are CELL-DISPATCHED (unlike name-canonical CBSym), so `fset` / `defalias`
/// over a speculated Many builtin must take effect on the very next call — the
/// armed site disarms (epoch move / bits mismatch) and its generic fallback
/// resolves the new binding. Verified for re-search-forward, looking-at, and
/// put-text-property.
#[cfg(feature = "jit")]
#[test]
fn jit_subr_spec_many_redefinition_deopts_like_interp() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"hello world\")").ok();

    // fset re-search-forward -> lambda: armed site deopts, new def runs.
    let rsf = jit_subr_spec_caller("re-search-forward", 1, true);
    ev.eval_str("(goto-char (point-min))").unwrap();
    let _ = ev
        .funcall_general_untraced(rsf, vec![Value::string("world")])
        .expect("rsf armed");
    ev.eval_str("(fset 're-search-forward (lambda (&rest _) 'RSF-REDEF))")
        .expect("fset rsf");
    let r = ev
        .funcall_general_untraced(rsf, vec![Value::string("world")])
        .expect("rsf after fset");
    assert_eq!(
        r,
        Value::symbol("RSF-REDEF"),
        "fset re-search-forward takes effect: site deopts to the new def"
    );

    // defalias looking-at -> lambda.
    let la = jit_subr_spec_caller("looking-at", 1, true);
    ev.eval_str("(goto-char (point-min))").unwrap();
    let _ = ev
        .funcall_general_untraced(la, vec![Value::string("hello")])
        .expect("la armed");
    ev.eval_str("(defalias 'looking-at (lambda (&rest _) 'LA-REDEF))")
        .expect("defalias la");
    let r = ev
        .funcall_general_untraced(la, vec![Value::string("hello")])
        .expect("la after defalias");
    assert_eq!(
        r,
        Value::symbol("LA-REDEF"),
        "defalias looking-at takes effect: site deopts to the new def"
    );

    // fset put-text-property -> lambda.
    let ptp = jit_subr_spec_caller("put-text-property", 4, true);
    let ptp_args = || {
        vec![
            Value::make_int(1),
            Value::make_int(3),
            Value::symbol("face"),
            Value::symbol("bold"),
        ]
    };
    let _ = ev
        .funcall_general_untraced(ptp, ptp_args())
        .expect("ptp armed");
    ev.eval_str("(fset 'put-text-property (lambda (&rest _) 'PTP-REDEF))")
        .expect("fset ptp");
    let r = ev
        .funcall_general_untraced(ptp, ptp_args())
        .expect("ptp after fset");
    assert_eq!(
        r,
        Value::symbol("PTP-REDEF"),
        "fset put-text-property takes effect: site deopts to the new def"
    );
}

/// Build `(lambda (a1..aK) (CBSYM-NAME a1..aK))` as hand-rolled bytecode: the K
/// args are pushed then consumed by a single `Op::CallBuiltinSym(name, K)` (the
/// exact op R2 classifies). Callers disable the profitability gate (COMMIT 2)
/// or rely on the COMMIT 3 re-weight.
#[cfg(feature = "jit")]
fn jit_cbsym_spec_caller(name: &str, nargs: usize, hot: bool) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::intern::{SymId, intern};
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs as u32).map(SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    let mut ops = Vec::new();
    // Push a1..aK: each `StackRef(nargs-1)` walks the arg window as the pushes
    // shift it (no callee constant — CallBuiltinSym carries the SymId).
    for _ in 0..nargs {
        ops.push(Op::StackRef((nargs - 1) as u16));
    }
    ops.push(Op::CallBuiltinSym(intern(name), nargs as u8));
    ops.push(Op::Return);
    f.ops = ops;
    f.constants = Vec::new().into();
    f.max_stack = 16;
    if hot {
        f.jit_runtime().set_hot_for_test();
    }
    Value::make_bytecode(f)
}

/// Debug-build snapshot of the R2 CBSym-spec counters (entries/fast/generic).
#[cfg(all(feature = "jit", debug_assertions))]
fn jit_cbsym_spec_counters() -> (u64, u64, u64) {
    use crate::emacs_core::jit::compile;
    use std::sync::atomic::Ordering;
    (
        compile::CBSYM_SPEC_COUNT.load(Ordering::Relaxed),
        compile::CBSYM_SPEC_FAST_COUNT.load(Ordering::Relaxed),
        compile::CBSYM_SPEC_GENERIC_COUNT.load(Ordering::Relaxed),
    )
}

/// True when the differential harness deliberately routes the CBSym shims OFF
/// their armed FAST path — `NEOVM_JIT_FORCE_CBSYM_GENERIC` forces the
/// NEED_GENERIC bounce, and `NEOVM_JIT_FORCE_DEOPT` deopts the whole compiled
/// body to the interpreter. Under either, the fast-path engagement counters do
/// NOT move, so the *counter* assertions must be skipped (RESULT parity still
/// holds and is still asserted — the whole point of the harness). Keeps the
/// FORCE_DEOPT failure SET equal to base.
#[cfg(all(feature = "jit", debug_assertions))]
fn jit_cbsym_fastpath_suppressed_by_harness() -> bool {
    std::env::var("NEOVM_JIT_FORCE_CBSYM_GENERIC").as_deref() == Ok("1")
        || std::env::var("NEOVM_JIT_FORCE_DEOPT").as_deref() == Ok("1")
}

/// R2 COMMIT 2 engagement + parity: hot Tier-B CallBuiltinSym sites (`length`
/// pure, `current-column` state read, `goto-char` idempotent state mutation)
/// route through `neovm_jit_cbsym_spec`, fire the fast path, and match the
/// interpreter byte-for-byte.
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_spec_tierb_engages_and_matches() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"hello world\")")
        .expect("buffer setup");
    #[cfg(debug_assertions)]
    let (count0, fast0, _) = jit_cbsym_spec_counters();
    let list = ev.eval_str("'(a b c d)").expect("list");
    let cases: [(&str, usize, Vec<Value>); 3] = [
        ("length", 1, vec![list]),
        ("current-column", 0, vec![]),
        ("goto-char", 1, vec![Value::make_int(3)]),
    ];
    for (name, nargs, args) in cases {
        let hot = jit_cbsym_spec_caller(name, nargs, true);
        let cold = jit_cbsym_spec_caller(name, nargs, false);
        let native = ev
            .funcall_general_untraced(hot, args.clone())
            .unwrap_or_else(|e| panic!("native {name}: {e:?}"));
        let interp = ev
            .funcall_general_untraced(cold, args)
            .unwrap_or_else(|e| panic!("interp {name}: {e:?}"));
        assert_eq!(
            native.bits(),
            interp.bits(),
            "{name}: Tier-B CBSym parity hot vs cold"
        );
    }
    #[cfg(debug_assertions)]
    if !jit_cbsym_fastpath_suppressed_by_harness() {
        let (count1, fast1, _) = jit_cbsym_spec_counters();
        assert!(
            count1 > count0,
            "a Tier-B CBSym site must route through neovm_jit_cbsym_spec"
        );
        assert!(
            fast1 > fast0,
            "the Tier-B fast path must fire (not silently bounce to generic)"
        );
    }
}

/// Residual-coverage audit (task A PART 2): `end-of-line` — a dedicated-opcode
/// CBSym motion builtin (2.37% in the font-lock SUBR-MIX) added to Tier-B
/// alongside its loop-siblings forward-line / forward-char / current-column. Its
/// point-moving side effect (not a pure read) must be byte-identical hot vs cold:
/// same return value AND same resulting `(point)`.
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_spec_tierb_end_of_line_engages_and_matches() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"line one\\nsecond line\\nthird\")")
        .expect("multi-line buffer");
    #[cfg(debug_assertions)]
    let (count0, fast0, _) = jit_cbsym_spec_counters();
    // `end-of-line` is emitted as CallBuiltinSym(end-of-line, 1) (GNU op 127);
    // classify by name, exercise the 1-arg (N=nil) form.
    let probe = |ev: &mut Context, hot: bool| -> (usize, String) {
        ev.eval_str("(goto-char 4)").expect("point mid line 1");
        let caller = jit_cbsym_spec_caller("end-of-line", 1, hot);
        let r = ev
            .funcall_general_untraced(caller, vec![Value::NIL])
            .expect("end-of-line runs");
        (r.bits(), format_eval_result(&ev.eval_str("(point)")))
    };
    let (nat_bits, nat_point) = probe(&mut ev, true);
    let (int_bits, int_point) = probe(&mut ev, false);
    assert_eq!(nat_bits, int_bits, "end-of-line return parity hot vs cold");
    assert_eq!(nat_point, int_point, "end-of-line point-move parity");
    // "line one" is 8 chars (pos 1..8), newline at 9 -> end-of-line lands on 9.
    assert_eq!(nat_point, "OK 9", "end-of-line moved point to line-1 EOL");
    #[cfg(debug_assertions)]
    if !jit_cbsym_fastpath_suppressed_by_harness() {
        let (count1, fast1, _) = jit_cbsym_spec_counters();
        assert!(
            count1 > count0,
            "the end-of-line Tier-B site must route through neovm_jit_cbsym_spec"
        );
        assert!(fast1 > fast0, "the Tier-B fast path must fire");
    }
}

/// R2 COMMIT 3 end-to-end: a buffer-op loop (goto-char / current-column / widen,
/// 3 intrinsifiable CBSym ops vs 2 arith) that `body_is_jit_profitable` USED to
/// reject (3 > 2 -> NotProfitable, so the intrinsic could never engage) now
/// TIERS with the profitability gate ON (no override) — the Tier-B fast path
/// fires and the result matches the interpreter.
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_buffer_loop_tiers_with_profit_gate_on() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    // Gate ON (production default) — do NOT disable it; the re-weight must carry.
    crate::emacs_core::jit::compile::force_profit_gate_for_test(true);
    let mk = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::StackRef(0),                                                       // 0  [n n]
            Op::Constant(0),                                                       // 1  [n n 0]
            Op::Gtr,           // 2  [n c]   arith
            Op::GotoIfNil(15), // 3  [n]
            Op::Constant(1),   // 4  [n 1]
            Op::CallBuiltinSym(crate::emacs_core::intern::intern("goto-char"), 1), // 5 [n pos]
            Op::Pop,           // 6  [n]
            Op::CallBuiltinSym(crate::emacs_core::intern::intern("current-column"), 0), // 7 [n col]
            Op::Pop,           // 8  [n]
            Op::CallBuiltinSym(crate::emacs_core::intern::intern("widen"), 0), // 9 [n nil]
            Op::Pop,           // 10 [n]
            Op::StackRef(0),   // 11 [n n]
            Op::Sub1,          // 12 [n n-1] arith
            Op::StackSet(1),   // 13 [n-1]
            Op::Goto(0),       // 14 backedge
            Op::StackRef(0),   // 15 [n n]   exit
            Op::Return,        // 16
        ];
        f.constants = vec![Value::make_int(0), Value::make_int(1)].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    let mut ev = Context::new();
    ev.eval_str("(insert \"abcdef\")").expect("buffer setup");
    #[cfg(debug_assertions)]
    let (_, fast0, _) = jit_cbsym_spec_counters();
    let native = ev
        .funcall_general_untraced(mk(true), vec![Value::make_int(4)])
        .expect("hot buffer loop runs");
    let interp = ev
        .funcall_general_untraced(mk(false), vec![Value::make_int(4)])
        .expect("interp buffer loop runs");
    assert_eq!(
        native.bits(),
        interp.bits(),
        "buffer-op loop parity hot vs cold"
    );
    assert_eq!(native, Value::make_int(0), "loop counts down to 0");
    #[cfg(debug_assertions)]
    if !jit_cbsym_fastpath_suppressed_by_harness() {
        let (_, fast1, _) = jit_cbsym_spec_counters();
        assert!(
            fast1 > fast0,
            "the buffer loop TIERED (gate ON) and the Tier-B fast path fired — \
             the CBSym re-weight let a formerly-NotProfitable body compile"
        );
    }
}

/// Must-nail #7 (Tier-B half): an in-place rewrite of a Tier-B primitive's
/// STATIC entry to a non-Builtin kind makes the compiled CBSym site bounce
/// (STATUS_NEED_GENERIC), and the general fallback reproduces the SAME signal as
/// the interpreter (invalid-function). Proves the shim re-reads the FRESH entry
/// and defers every non-clean case to the general path.
#[cfg(all(feature = "jit", debug_assertions))]
#[test]
fn jit_cbsym_spec_inplace_rewrite_to_nonbuiltin_bounces_like_interp() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    use crate::emacs_core::eval::{
        SubrEntry, lookup_global_subr_entry, register_global_subr_entry,
    };
    use crate::emacs_core::intern::intern;
    use crate::tagged::header::SubrDispatchKind;
    let mut ev = Context::new();
    let widen = intern("widen"); // Tier-B, 0-arg, not used between rewrite/restore
    let orig = lookup_global_subr_entry(widen).expect("widen is a builtin");
    let hot = jit_cbsym_spec_caller("widen", 0, true);
    let cold = jit_cbsym_spec_caller("widen", 0, false);
    // Armed: fast path returns nil.
    let r = ev
        .funcall_general_untraced(hot, Vec::<Value>::new())
        .expect("armed widen");
    assert!(r.is_nil(), "(widen) returns nil on the armed fast path");
    let (_, _, gen0) = jit_cbsym_spec_counters();
    // In-place rewrite the STATIC entry to a non-Builtin kind (VALUE bits stable).
    register_global_subr_entry(
        widen,
        SubrEntry {
            dispatch_kind: SubrDispatchKind::SpecialForm,
            ..orig
        },
    );
    let native = ev.funcall_general_untraced(hot, Vec::<Value>::new());
    let interp = ev.funcall_general_untraced(cold, Vec::<Value>::new());
    // Restore BEFORE asserting so a failure can't leave `widen` broken.
    register_global_subr_entry(widen, orig);
    let native_err = native.expect_err("compiled widen now signals invalid-function");
    let interp_err = interp.expect_err("interp widen now signals invalid-function");
    assert_eq!(
        format!("{native_err:?}"),
        format!("{interp_err:?}"),
        "compiled NEED_GENERIC bounce == interpreter signal"
    );
    if !jit_cbsym_fastpath_suppressed_by_harness() {
        // (FORCE_DEOPT deopts the body to the interpreter, so the shim — and its
        // GENERIC counter — never runs; the invalid-function parity above still
        // holds and is asserted unconditionally.)
        let (_, _, gen1) = jit_cbsym_spec_counters();
        assert!(
            gen1 > gen0,
            "the rewritten (non-Builtin) site bounced to STATUS_NEED_GENERIC"
        );
    }
    // Sanity: restored entry works again.
    let r = ev
        .funcall_general_untraced(hot, Vec::<Value>::new())
        .expect("widen restored");
    assert!(r.is_nil());
}

/// Must-nail #4: `(backtrace-frames)` inside an after-change hook fired by a
/// COMPILED Tier-B `insert` shows the frame's function as `#<subr insert>` (a
/// SUBR value — `funcall_general` pushes `subr_from_sym_id`, NOT the symbol) ==
/// the interpreter.
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_spec_insert_backtrace_shows_subr_frame_like_interp() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    // A hook that captures the printed form of the first `insert` runtime
    // backtrace frame (mapbacktrace calls (fn EVALD FUNC ARGS FLAGS); FUNC is
    // the frame's callable). `backtrace-frames` proper is stubbed in neomacs;
    // `mapbacktrace` walks the REAL specpdl backtrace frames.
    ev.eval_str(
        r#"(setq after-change-functions
             (list (lambda (_b _e _l)
                     (setq cbsym-cap 'no-subr-frame)
                     (mapbacktrace
                       (lambda (_evald func _args _flags)
                         (if (and (eq cbsym-cap 'no-subr-frame)
                                  (subrp func)
                                  (equal (subr-name func) "insert"))
                             (setq cbsym-cap (prin1-to-string func))))))))"#,
    )
    .expect("install hook");
    let run = |ev: &mut Context, hot: bool| -> String {
        ev.eval_str("(setq cbsym-cap 'unset)").unwrap();
        let f = jit_cbsym_spec_caller("insert", 1, hot);
        ev.funcall_general_untraced(f, vec![Value::string("X")])
            .expect("insert runs");
        let cap = ev.eval_str("cbsym-cap").unwrap();
        crate::emacs_core::print::print_value(&cap)
    };
    let cap_hot = run(&mut ev, true);
    let cap_cold = run(&mut ev, false);
    assert_eq!(
        cap_hot, cap_cold,
        "the insert backtrace frame is identical compiled vs interp"
    );
    assert!(
        cap_hot.contains("#<subr insert>"),
        "the insert frame is a SUBR (#<subr insert>), not a symbol; got {cap_hot}"
    );
}

/// Must-nail #5: deep recursion whose body runs a compiled Tier-B CBSym op hits
/// `max-lisp-eval-depth` at the SAME recursion depth as the interpreter — the
/// open-coded CBSym op adds NO lisp-eval-depth level (the shim has no
/// `with_bytecode_call_depth`, unlike an `Op::Call`).
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_spec_adds_no_eval_depth_level() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(defvar cbsym-depth 0)").unwrap();
    // f: (setq cbsym-depth (1+ cbsym-depth)) (widen) (f) — recurse until the
    // depth guard signals; `cbsym-depth` is left at the deepest level reached.
    let f_sym = Value::symbol("cbsym-depth-f");
    let ValueKind::Symbol(f_id) = f_sym.kind() else {
        panic!("symbol");
    };
    let mk = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::VarRef(0), // 0  [d]        (const0 = cbsym-depth)
            Op::Add1,      // 1  [d+1]
            Op::VarSet(0), // 2  []         cbsym-depth = d+1
            Op::CallBuiltinSym(crate::emacs_core::intern::intern("widen"), 0), // 3 [nil]  Tier-B
            Op::Pop,       // 4  []
            Op::Constant(1), // 5  [f]        (const1 = f symbol)
            Op::Call(0),   // 6  recurse (adds ONE eval-depth level)
            Op::Return,    // 7
        ];
        f.constants = vec![Value::symbol("cbsym-depth"), f_sym].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    // Lower the limit so the recursion terminates quickly and identically.
    ev.eval_str("(setq max-lisp-eval-depth 150)").unwrap();
    let depth_after = |ev: &mut Context, hot: bool| -> i64 {
        ev.eval_str("(setq cbsym-depth 0)").unwrap();
        ev.obarray.set_symbol_function_id(f_id, mk(hot));
        // Recurses until max-lisp-eval-depth signals; catch it.
        let _ = ev.funcall_general_untraced(f_sym, Vec::<Value>::new());
        ev.eval_str("cbsym-depth").unwrap().as_fixnum().unwrap()
    };
    let hot_depth = depth_after(&mut ev, true);
    let cold_depth = depth_after(&mut ev, false);
    assert!(
        hot_depth > 1,
        "the recursion actually ran (depth {hot_depth})"
    );
    assert_eq!(
        hot_depth, cold_depth,
        "a compiled Tier-B CBSym op adds no eval-depth level: \
         compiled reached depth {hot_depth}, interp {cold_depth}"
    );
}

/// R2 COMMIT 5 engagement + parity: hot Tier-A CallBuiltinSym reads
/// (point/point-min/point-max/bolp/eolp/bobp/eobp/following-char/preceding-char/
/// char-after) route through `neovm_jit_cbsym_read` (GC-free), fire the fast
/// path, and match the interpreter byte-for-byte.
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_read_tiera_engages_and_matches() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"abc\\ndef\")").unwrap();
    ev.eval_str("(goto-char 2)").unwrap();
    #[cfg(debug_assertions)]
    let (_, fast0, _) = jit_cbsym_spec_counters();
    for name in [
        "point",
        "point-min",
        "point-max",
        "bolp",
        "eolp",
        "bobp",
        "eobp",
        "following-char",
        "preceding-char",
        "char-after",
    ] {
        let hot = jit_cbsym_spec_caller(name, 0, true);
        let cold = jit_cbsym_spec_caller(name, 0, false);
        let native = ev
            .funcall_general_untraced(hot, Vec::<Value>::new())
            .unwrap_or_else(|e| panic!("native {name}: {e:?}"));
        let interp = ev
            .funcall_general_untraced(cold, Vec::<Value>::new())
            .unwrap_or_else(|e| panic!("interp {name}: {e:?}"));
        assert_eq!(
            native.bits(),
            interp.bits(),
            "{name}: Tier-A read parity hot vs cold"
        );
    }
    #[cfg(debug_assertions)]
    if !jit_cbsym_fastpath_suppressed_by_harness() {
        let (_, fast1, _) = jit_cbsym_spec_counters();
        assert!(fast1 > fast0, "the Tier-A read fast path must fire");
    }
}

/// Must-nail #1: match-beginning/match-end after a BUFFER regexp search return
/// CHAR positions (the shim delegates to the body's byte→char conversion — a
/// register read of the byte offset would be WRONG) == interp, plus the edges:
/// negative group → args-out-of-range, non-int → wrong-type-argument, group
/// beyond count → nil.
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_read_match_beginning_end_char_positions_and_edges() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"foo bar baz\")").unwrap();
    ev.eval_str("(goto-char (point-min))").unwrap();
    ev.eval_str("(search-forward \"bar\")").unwrap(); // sets match-data, group 0 = "bar"
    let mb_hot = jit_cbsym_spec_caller("match-beginning", 1, true);
    let mb_cold = jit_cbsym_spec_caller("match-beginning", 1, false);
    let me_hot = jit_cbsym_spec_caller("match-end", 1, true);
    let me_cold = jit_cbsym_spec_caller("match-end", 1, false);
    let call = |ev: &mut Context, f: Value, g: i64| {
        ev.funcall_general_untraced(f, vec![Value::make_int(g)])
    };
    // group 0: CHAR positions, == interp (both compiled/interp and the live form).
    let nb = call(&mut ev, mb_hot, 0).unwrap();
    assert_eq!(
        nb,
        call(&mut ev, mb_cold, 0).unwrap(),
        "match-beginning 0 hot==cold"
    );
    assert_eq!(
        nb,
        ev.eval_str("(match-beginning 0)").unwrap(),
        "match-beginning 0 == live interp CHAR pos"
    );
    let ne = call(&mut ev, me_hot, 0).unwrap();
    assert_eq!(
        ne,
        call(&mut ev, me_cold, 0).unwrap(),
        "match-end 0 hot==cold"
    );
    assert_eq!(
        ne,
        ev.eval_str("(match-end 0)").unwrap(),
        "match-end 0 == live interp CHAR pos"
    );
    // group beyond the match count -> nil.
    assert_eq!(
        call(&mut ev, mb_hot, 5).unwrap(),
        Value::NIL,
        "group beyond count -> nil"
    );
    // negative group -> args-out-of-range == interp.
    let neg_hot = format!("{:?}", call(&mut ev, mb_hot, -1).unwrap_err());
    let neg_cold = format!("{:?}", call(&mut ev, mb_cold, -1).unwrap_err());
    assert_eq!(
        neg_hot, neg_cold,
        "negative group -> args-out-of-range == interp\nHOT:  {neg_hot}\nCOLD: {neg_cold}"
    );
    // non-int group -> wrong-type-argument == interp.
    let ni_hot = ev.funcall_general_untraced(mb_hot, vec![Value::symbol("x")]);
    let ni_cold = ev.funcall_general_untraced(mb_cold, vec![Value::symbol("x")]);
    assert_eq!(
        format!("{:?}", ni_hot.unwrap_err()),
        format!("{:?}", ni_cold.unwrap_err()),
        "non-int group -> wrong-type-argument == interp"
    );
}

/// Must-nail #2: following-char/preceding-char at ZV/BOB → 0; char-after at ZV →
/// nil (in the SAME buffer/point — proves the two are not conflated).
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_read_char_accessors_at_boundaries() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"xy\")").unwrap();
    ev.eval_str("(goto-char (point-max))").unwrap(); // at ZV
    let fc = jit_cbsym_spec_caller("following-char", 0, true);
    let ca = jit_cbsym_spec_caller("char-after", 0, true);
    assert_eq!(
        ev.funcall_general_untraced(fc, Vec::<Value>::new())
            .unwrap(),
        Value::make_int(0),
        "following-char at ZV -> 0"
    );
    assert_eq!(
        ev.funcall_general_untraced(ca, Vec::<Value>::new())
            .unwrap(),
        Value::NIL,
        "char-after at ZV -> nil (NOT conflated with following-char's 0)"
    );
    ev.eval_str("(goto-char (point-min))").unwrap(); // at BOB
    let pc = jit_cbsym_spec_caller("preceding-char", 0, true);
    let ca2 = jit_cbsym_spec_caller("char-after", 0, true);
    assert_eq!(
        ev.funcall_general_untraced(pc, Vec::<Value>::new())
            .unwrap(),
        Value::make_int(0),
        "preceding-char at BOB -> 0"
    );
    assert_eq!(
        ev.funcall_general_untraced(ca2, Vec::<Value>::new())
            .unwrap(),
        Value::make_int('x' as i64),
        "char-after at BOB -> 'x' (a real char, proves ZV-nil is position-specific)"
    );
}

/// Must-nail #6: bolp with point at BEGV of a buffer narrowed to BEGV > 0 → t
/// (the body's first case, `point == accessible-region start`).
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_read_bolp_at_begv_in_narrowed_buffer() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"line1\\nline2\\nline3\")").unwrap();
    ev.eval_str("(narrow-to-region 3 10)").unwrap(); // BEGV = 3 (mid-line)
    ev.eval_str("(goto-char (point-min))").unwrap(); // point = BEGV = 3
    let bolp = jit_cbsym_spec_caller("bolp", 0, true);
    let bolp_cold = jit_cbsym_spec_caller("bolp", 0, false);
    let native = ev
        .funcall_general_untraced(bolp, Vec::<Value>::new())
        .unwrap();
    let interp = ev
        .funcall_general_untraced(bolp_cold, Vec::<Value>::new())
        .unwrap();
    assert_eq!(native, interp, "bolp at BEGV parity hot vs cold");
    assert!(
        native.is_truthy(),
        "bolp with point == BEGV (narrowed, BEGV>0) -> t"
    );
}

/// Must-nail #3: current-buffer through a compiled CBSym on a buffer whose value
/// was never materialized bounces (STATUS_NEED_GENERIC — the shim NEVER calls
/// make_buffer, which would allocate under the unrooted residual stack); result
/// == interp. A second call (now materialized) takes the GC-free fast path.
#[cfg(all(feature = "jit", debug_assertions))]
#[test]
fn jit_cbsym_read_current_buffer_bounces_when_unmaterialized() {
    crate::test_utils::init_test_tracing();
    // This test's premise is that the current buffer's tagged value has NOT
    // been materialized before the first compiled call. Under NEOVM_GC_STRESS
    // the compilations above force collections whose buffer marking
    // materializes it, so the bounce it asserts never happens. The premise
    // (not the behavior) is stress-incompatible; skip, like the other
    // stress-gated tests.
    if std::env::var("NEOVM_GC_STRESS").as_deref() == Ok("1") {
        return;
    }
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    // Do NOT call current-buffer / make_buffer before the compiled call, so the
    // current buffer's tagged value is (likely) not yet in the buffer registry.
    let hot = jit_cbsym_spec_caller("current-buffer", 0, true);
    let cold = jit_cbsym_spec_caller("current-buffer", 0, false);
    let (_, fast0, gen0) = jit_cbsym_spec_counters();
    let native = ev
        .funcall_general_untraced(hot, Vec::<Value>::new())
        .unwrap();
    let (_, fast1, gen1) = jit_cbsym_spec_counters();
    let interp = ev
        .funcall_general_untraced(cold, Vec::<Value>::new())
        .unwrap();
    assert_eq!(native, interp, "current-buffer compiled == interp");
    if !jit_cbsym_fastpath_suppressed_by_harness() {
        // The FIRST compiled call materialized the buffer via the general
        // fallback (a NEED_GENERIC bounce), NOT the fast path.
        assert!(
            gen1 > gen0,
            "the first current-buffer (unmaterialized) bounced to the general path"
        );
        assert_eq!(
            fast1, fast0,
            "the shim did NOT take the fast (allocating-risk) path"
        );
        // Now materialized: a second compiled call takes the GC-free fast path.
        let hot2 = jit_cbsym_spec_caller("current-buffer", 0, true);
        let (_, fast2, _) = jit_cbsym_spec_counters();
        let native2 = ev
            .funcall_general_untraced(hot2, Vec::<Value>::new())
            .unwrap();
        let (_, fast3, _) = jit_cbsym_spec_counters();
        assert_eq!(native2, interp, "second current-buffer still == interp");
        assert!(
            fast3 > fast2,
            "the materialized buffer now takes the fast path"
        );
    }
}

/// Must-nail #7 (Tier-A advice half): overriding a Tier-A primitive's function
/// CELL is a no-op for the compiled CallBuiltinSym call — CBSym name-dispatches
/// the STATIC subr table (`lookup_global_subr_entry`), bypassing the cell /
/// advice, exactly like the interpreter's Bpoint arm.
#[cfg(feature = "jit")]
#[test]
fn jit_cbsym_read_ignores_function_cell_override() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"abcd\")").unwrap();
    ev.eval_str("(goto-char 3)").unwrap();
    let real_point = ev.eval_str("(point)").unwrap(); // 3 (interp CBSym is also cell-immune)
    // Override the function CELL of `point` (what advice-add ultimately mutates).
    ev.eval_str("(fset 'point (lambda () 999))").unwrap();
    // A normal funcall through the cell now yields 999...
    assert_eq!(
        ev.eval_str("(funcall (symbol-function 'point))").unwrap(),
        Value::make_int(999),
        "the cell override IS visible to a cell dispatch"
    );
    // ...but the compiled CallBuiltinSym `point` ignores it (static-table dispatch).
    let hot = jit_cbsym_spec_caller("point", 0, true);
    let cold = jit_cbsym_spec_caller("point", 0, false);
    let native = ev
        .funcall_general_untraced(hot, Vec::<Value>::new())
        .unwrap();
    let interp = ev
        .funcall_general_untraced(cold, Vec::<Value>::new())
        .unwrap();
    assert_eq!(
        native, interp,
        "compiled == interp CBSym point (both cell-immune)"
    );
    assert_eq!(
        native, real_point,
        "compiled CBSym point returns the REAL point, ignoring the cell override"
    );
}

/// End-to-end: the classic hot shapes the poisoning analysis used to BAIL on
/// now compile — recursive fib (arithmetic after recursive calls) and a
/// while-loop mixing a call with arithmetic (guards at the loop join). Both
/// must match the interpreter exactly, including the deopt path.
#[cfg(feature = "jit")]
#[test]
fn jit_fib_and_loops_compile_under_precise_deopt() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    // fib: (lambda (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) —
    // self-recursive through its constant symbol (also exercises direct-call
    // speculation), with Sub/Add guards AFTER the recursive calls.
    let fib_sym = Value::symbol("jit-pd-fib");
    let ValueKind::Symbol(fib_id) = fib_sym.kind() else {
        panic!("symbol expected");
    };
    let mk_fib = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::StackRef(0),  // 0  [n n]
            Op::Constant(0),  // 1  [n n 2]
            Op::Lss,          // 2  [n c]
            Op::GotoIfNil(6), // 3  [n]
            Op::StackRef(0),  // 4  [n n]
            Op::Return,       // 5
            Op::Constant(1),  // 6  [n f]
            Op::StackRef(1),  // 7  [n f n]
            Op::Constant(2),  // 8  [n f n 1]
            Op::Sub,          // 9  [n f n-1]
            Op::Call(1),      // 10 [n r1]
            Op::Constant(1),  // 11 [n r1 f]
            Op::StackRef(2),  // 12 [n r1 f n]
            Op::Constant(0),  // 13 [n r1 f n 2]
            Op::Sub,          // 14 [n r1 f n-2]   guard AFTER a call
            Op::Call(1),      // 15 [n r1 r2]
            Op::Add,          // 16 [n r]          guard AFTER both calls
            Op::Return,       // 17
        ];
        f.constants = vec![Value::make_int(2), fib_sym, Value::make_int(1)].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    // Interpreter oracle first (cold installed), then the hot version.
    ev.obarray.set_symbol_function_id(fib_id, mk_fib(false));
    let interp = ev
        .funcall_general_untraced(mk_fib(false), vec![Value::make_int(12)])
        .expect("interpreted fib runs");
    assert_eq!(interp, Value::make_int(144));
    let hot = mk_fib(true);
    ev.obarray.set_symbol_function_id(fib_id, hot);
    let native = ev
        .funcall_general_untraced(hot, vec![Value::make_int(12)])
        .expect("native fib runs");
    assert_eq!(native, Value::make_int(144), "fib compiles + runs natively");
    // Non-fixnum argument: the Lss guard deopts and the resumed interpreter
    // signals wrong-type-argument, exactly like the oracle.
    assert!(
        ev.funcall_general_untraced(hot, vec![Value::string("x")])
            .is_err()
    );

    // while-loop with a call + arithmetic at the join:
    // (lambda (n) (while (> n 0) (setq n (1- (identity-callee n)))) n).
    let id_sym = Value::symbol("jit-pd-id");
    let ValueKind::Symbol(id_id) = id_sym.kind() else {
        panic!("symbol expected");
    };
    let mut ident = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    ident.lexical = true;
    ident.ops = vec![Op::StackRef(0), Op::Return];
    ident.max_stack = 16;
    ev.obarray
        .set_symbol_function_id(id_id, Value::make_bytecode(ident));
    let mk_loop = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::StackRef(0),   // 0  [n n]      <- loop head (backedge target)
            Op::Constant(0),   // 1  [n n 0]
            Op::Gtr,           // 2  [n c]      guard at the loop JOIN
            Op::GotoIfNil(10), // 3  [n]
            Op::Constant(1),   // 4  [n f]
            Op::StackRef(1),   // 5  [n f n]
            Op::Call(1),       // 6  [n r]
            Op::Sub1,          // 7  [n r-1]    guard AFTER the call
            Op::StackSet(1),   // 8  [r-1]
            Op::Goto(0),       // 9  backedge
            Op::StackRef(0),   // 10 [n n]
            Op::Return,        // 11
        ];
        f.constants = vec![Value::make_int(0), id_sym].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    let native = ev
        .funcall_general_untraced(mk_loop(true), vec![Value::make_int(300)])
        .expect("native loop runs");
    let interp = ev
        .funcall_general_untraced(mk_loop(false), vec![Value::make_int(300)])
        .expect("interpreted loop runs");
    assert_eq!(native, Value::make_int(0));
    assert_eq!(native, interp, "loop with call+guards matches interpreter");
}

/// Native-to-native correctness across the pure/marshaled split: a speculated
/// callee with `&optional` takes the pure pass-through path when all args are
/// supplied (nargs == arity) and the arg-marshaling path (nil-padding) when
/// fewer are — both must match the interpreter exactly.
#[cfg(feature = "jit")]
#[test]
fn jit_native_to_native_optional_callee_pure_and_marshaled() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;

    let mut ev = Context::new();
    // Callee (lambda (a &optional b) (if b (+ a b) a)) as hand-built bytecode.
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: vec![crate::emacs_core::intern::SymId(2)],
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![
        Op::StackRef(0),  // 0: b           [a b b]
        Op::GotoIfNil(6), // 1:             [a b]
        Op::StackRef(1),  // 2: a           [a b a]
        Op::StackRef(1),  // 3: b           [a b a b]
        Op::Add,          // 4:             [a b r]
        Op::Return,       // 5
        Op::StackRef(1),  // 6: a           [a b a]
        Op::Return,       // 7
    ];
    callee.max_stack = 16;
    let c_sym = Value::symbol("jit-n2n-opt");
    let ValueKind::Symbol(c_id) = c_sym.kind() else {
        panic!("symbol expected");
    };
    ev.obarray
        .set_symbol_function_id(c_id, Value::make_bytecode(callee));

    // Caller with 2 args -> callee nargs==2==arity -> PURE native pass-through.
    let mk2 = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![
                crate::emacs_core::intern::SymId(1),
                crate::emacs_core::intern::SymId(2),
            ],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::Constant(0), // 'jit-n2n-opt
            Op::StackRef(2), // a
            Op::StackRef(2), // b
            Op::Call(2),
            Op::Return,
        ];
        f.constants = vec![c_sym].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };
    // Caller with 1 arg -> callee nargs==1 < arity==2 -> MARSHALED (nil-pad b).
    let mk1 = |hot: bool| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
        f.constants = vec![c_sym].into();
        f.max_stack = 16;
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        Value::make_bytecode(f)
    };

    // Pure path: (callee 5 7) = 12.
    let native = ev
        .funcall_general_untraced(mk2(true), vec![Value::make_int(5), Value::make_int(7)])
        .unwrap();
    let interp = ev
        .funcall_general_untraced(mk2(false), vec![Value::make_int(5), Value::make_int(7)])
        .unwrap();
    assert_eq!(native, Value::make_int(12));
    assert_eq!(
        native, interp,
        "pure native pass-through matches interpreter"
    );

    // Marshaled path: (callee 5) -> b nil -> 5.
    let native = ev
        .funcall_general_untraced(mk1(true), vec![Value::make_int(5)])
        .unwrap();
    let interp = ev
        .funcall_general_untraced(mk1(false), vec![Value::make_int(5)])
        .unwrap();
    assert_eq!(native, Value::make_int(5));
    assert_eq!(
        native, interp,
        "marshaled (nil-pad) path matches interpreter"
    );
}

/// V3 fast path: a speculated call to a compiled bytecode callee runs the
/// cached leaf DIRECTLY (engagement counter proves it), and a redefinition
/// clears the cached leaf so the new binding takes effect — same observable
/// semantics as before, now via the fast path.
#[cfg(feature = "jit")]
#[cfg(debug_assertions)]
#[test]
fn jit_v3_fast_path_engages_and_tracks_redefinition() {
    crate::test_utils::init_test_tracing();
    // Compiles a deliberately call-only forwarder to exercise the V3 fast-path
    // speculation; production would decline it as unprofitable, so disable the
    // profitability gate for this machinery test.
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    use std::sync::atomic::Ordering;

    let mut ev = Context::new();
    let mk_times = |k: i64| -> Value {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        // Two blocks so the JIT inliner (single-block callees only) leaves this a
        // SPEC call — `(if x (* x k) (* x k))`, same value, two basic blocks.
        f.ops = vec![
            Op::StackRef(0),
            Op::GotoIfNil(6),
            Op::StackRef(0),
            Op::Constant(0),
            Op::Mul,
            Op::Return,
            Op::StackRef(0),
            Op::Constant(0),
            Op::Mul,
            Op::Return,
        ];
        f.constants = vec![Value::make_int(k)].into();
        f.max_stack = 16;
        Value::make_bytecode(f)
    };
    let g_sym = Value::symbol("jit-v3-g");
    let ValueKind::Symbol(g_id) = g_sym.kind() else {
        panic!("symbol expected");
    };
    ev.obarray.set_symbol_function_id(g_id, mk_times(2));

    let mk_caller = || {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
        f.constants = vec![g_sym].into();
        f.max_stack = 16;
        f.jit_runtime().set_hot_for_test();
        Value::make_bytecode(f)
    };
    let hot = mk_caller();
    let five = vec![Value::make_int(5)];

    let fast_before = crate::emacs_core::jit::compile::SPEC_FAST_CALL_COUNT.load(Ordering::Relaxed);
    assert_eq!(
        ev.funcall_general_untraced(hot, five.clone()).unwrap(),
        Value::make_int(10)
    );
    // A second call re-uses the cached leaf slot (no recompile, no hash lookup).
    assert_eq!(
        ev.funcall_general_untraced(hot, five.clone()).unwrap(),
        Value::make_int(10)
    );
    let fast_after = crate::emacs_core::jit::compile::SPEC_FAST_CALL_COUNT.load(Ordering::Relaxed);
    assert!(
        fast_after >= fast_before + 2,
        "both speculated calls must take the V3 fast path (cached direct native call)"
    );

    // Redefine to different bytecode: epoch moves, revalidation fails, the
    // cached leaf is cleared, the strict path resolves the new binding.
    ev.obarray.set_symbol_function_id(g_id, mk_times(3));
    assert_eq!(
        ev.funcall_general_untraced(hot, five.clone()).unwrap(),
        Value::make_int(15)
    );
    // Redefine to a NON-bytecode callable: the fast path must abandon (the
    // strict path resolves the interpreted lambda).
    let lam = ev.eval_str("(lambda (x) (* x 100))").unwrap();
    ev.obarray.set_symbol_function_id(g_id, lam);
    assert_eq!(
        ev.funcall_general_untraced(hot, five).unwrap(),
        Value::make_int(500)
    );
}

/// Build the canonical recursive-fib benchmark shape (self-recursive through
/// `sym_name`, guards after the recursive calls — only compilable since
/// precise-PC deopt). `tier`: Hot forces native, Cold pins the interpreter.
#[cfg(feature = "jit")]
fn jit_bench_fib_value(sym_name: &str, tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let fib_sym = Value::symbol(sym_name);
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Lss,
        Op::GotoIfNil(6),
        Op::StackRef(0),
        Op::Return,
        Op::Constant(1),
        Op::StackRef(1),
        Op::Constant(2),
        Op::Sub,
        Op::Call(1),
        Op::Constant(1),
        Op::StackRef(2),
        Op::Constant(0),
        Op::Sub,
        Op::Call(1),
        Op::Add,
        Op::Return,
    ];
    f.constants = vec![Value::make_int(2), fib_sym, Value::make_int(1)].into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

/// Body-dominated control benchmark: a countdown accumulator loop (pure
/// arithmetic + backedges, no calls) — isolates the native-body win from call
/// overhead.
#[cfg(feature = "jit")]
fn jit_bench_loop_value(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(0),
        Op::StackRef(1),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(13),
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Add,
        Op::StackSet(1),
        Op::StackRef(1),
        Op::Sub1,
        Op::StackSet(2),
        Op::Goto(1),
        Op::StackRef(0),
        Op::Return,
    ];
    f.constants = vec![Value::make_int(0)].into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

/// A >256-op loop body in the 352-op font-lock matcher's SHAPE (arith chunks
/// interleaved with never-taken conditional branches inside one hot loop).
/// This is the missing Phase-0 fixture for the mid-end campaign's acceptance
/// test written into `RuntimeState::MAX_TIER_OPS`'s doc: "raise it only when
/// a measurement shows the tier's output beating the interpreter on a
/// >256-op body". Every other jit_bench body is <=~26 ops.
///
/// Body: `(lambda (n) (push 0 as acc) (while (> n 0) {85 chunks} (setq n
/// (1- n))) acc)` where chunk i is `acc += cycle(1,2,3,4)` (3 ops) and every
/// 5th chunk adds a `(> acc -1)` conditional branch (4 ops, never taken).
/// Result: n * 170.
#[cfg(feature = "jit")]
fn jit_bench_big_body_value(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;

    let mut ops = Vec::new();
    ops.push(Op::Constant(0)); // acc = 0; stack [n, acc]
    let header = ops.len();
    // while (> n 0)
    ops.push(Op::StackRef(1)); // n
    ops.push(Op::Constant(0)); // 0
    ops.push(Op::Gtr);
    let exit_patch = ops.len();
    ops.push(Op::GotoIfNil(u32::MAX)); // patched to EXIT below
    let mut add_cycle = 0usize;
    for chunk in 0..85 {
        if chunk % 5 == 4 {
            // Never-taken branch: (when (= acc -1) <fallthrough>) shaped as
            // acc > -1, always true, so GotoIfNil falls through either way.
            ops.push(Op::StackRef(0)); // acc
            ops.push(Op::Constant(5)); // -1
            ops.push(Op::Gtr);
            let next = (ops.len() + 1) as u32;
            ops.push(Op::GotoIfNil(next));
        } else {
            ops.push(Op::StackRef(0)); // acc
            ops.push(Op::Constant((1 + (add_cycle % 4)) as u16)); // 1..4
            ops.push(Op::Add); // acc + k
            ops.push(Op::StackSet(1)); // acc = ...
            add_cycle += 1;
        }
    }
    // n -= 1; loop
    ops.push(Op::StackRef(1));
    ops.push(Op::Sub1);
    ops.push(Op::StackSet(2));
    ops.push(Op::Goto(header as u32));
    let exit = ops.len();
    ops[exit_patch] = Op::GotoIfNil(exit as u32);
    ops.push(Op::StackRef(0)); // acc
    ops.push(Op::Return);
    assert!(
        ops.len() > 256,
        "fixture must exceed MAX_TIER_OPS ({})",
        ops.len()
    );

    f.ops = ops;
    f.constants = vec![
        Value::make_int(0),
        Value::make_int(1),
        Value::make_int(2),
        Value::make_int(3),
        Value::make_int(4),
        Value::make_int(-1),
    ]
    .into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

/// Run with the cap lifted so the Hot copy actually tiers:
/// `NEOVM_JIT_MAX_OPS=0 taskset -c 2 cargo nextest run --cargo-profile profiling \
///  -p neovm-core -E 'test(jit_bench_big_body)' --run-ignored all --no-capture`
/// (the env var is set in-test too, but the OnceLock caches the FIRST read,
/// so a prior in-process dispatch would pin the default — nextest's
/// process-per-test makes the in-test set reliable).
#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_big_body_matcher_shape() {
    crate::test_utils::init_test_tracing();
    // SAFETY: single-threaded test process; set before any JIT dispatch.
    // MAX_OPS=0 lifts the 256-op cap; SIZE_UNIT=0 drops the size factor
    // (set_hot_for_test presets heat to HOT_THRESHOLD, but a 281-op body's
    // scaled threshold is 4x that).
    unsafe { std::env::set_var("NEOVM_JIT_MAX_OPS", "0") };
    unsafe { std::env::set_var("NEOVM_JIT_SIZE_UNIT", "0") };
    let mut ev = Context::new();
    let native = jit_bench_big_body_value(BenchTier::Hot);
    let cold = jit_bench_big_body_value(BenchTier::Cold);
    let n = 50_000i64;
    // 68 add chunks cycling +1+2+3+4 => 17 * 10 per iteration.
    let want = Value::make_int(n * 170);
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let (ok, deopt, deopt_at, signal) = crate::emacs_core::jit::cache::native_run_counters();
    assert!(
        ok > 0,
        "hot copy never ran natively (ok={ok} deopt={deopt} deopt_at={deopt_at} \
         signal={signal}) — is NEOVM_JIT_MAX_OPS=0 in effect?"
    );
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    panic!(
        "BENCH big-body(281 ops, 50K iters): native {nat:?} interp {int:?} -> {:.2}x",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// Which execution tier a benchmark copy is pinned to (same-process A/B).
#[cfg(feature = "jit")]
#[derive(Clone, Copy)]
enum BenchTier {
    Hot,
    Cold,
}

#[cfg(feature = "jit")]
impl BenchTier {
    fn apply(self, rt: &crate::emacs_core::jit::Runtime) {
        match self {
            BenchTier::Hot => rt.set_hot_for_test(),
            BenchTier::Cold => rt.set_cold_for_test(),
        }
    }
}

/// Min wall-clock of `iters` calls to `f(arg)` (warming once first), asserting
/// each result equals `want`. Min cancels transient scheduler/thermal noise.
#[cfg(feature = "jit")]
fn jit_bench_min(
    ev: &mut Context,
    f: Value,
    arg: i64,
    want: Value,
    iters: u32,
) -> std::time::Duration {
    assert_eq!(
        ev.funcall_general_untraced(f, vec![Value::make_int(arg)])
            .unwrap(),
        want
    );
    let mut best = std::time::Duration::MAX;
    for _ in 0..iters {
        let t = std::time::Instant::now();
        let r = ev
            .funcall_general_untraced(f, vec![Value::make_int(arg)])
            .unwrap();
        best = best.min(t.elapsed());
        assert_eq!(r, want);
    }
    best
}

/// `cargo nextest run --release --run-ignored ignored-only jit_bench` — both
/// tiers measured in ONE process (a hot copy and a force-cold copy recursing
/// through DISTINCT symbols), min-of-N each, so the native/interpreter ratio
/// is free of cross-process CPU-frequency variance.
#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_fib() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let native = jit_bench_fib_value("jit-bench-fib-n", BenchTier::Hot);
    let cold = jit_bench_fib_value("jit-bench-fib-c", BenchTier::Cold);
    let ValueKind::Symbol(nid) = Value::symbol("jit-bench-fib-n").kind() else {
        panic!()
    };
    let ValueKind::Symbol(cid) = Value::symbol("jit-bench-fib-c").kind() else {
        panic!()
    };
    ev.obarray.set_symbol_function_id(nid, native);
    ev.obarray.set_symbol_function_id(cid, cold);
    let want = Value::make_int(196418);
    let nat = jit_bench_min(&mut ev, native, 27, want, 9);
    let int = jit_bench_min(&mut ev, cold, 27, want, 9);
    panic!(
        "BENCH fib(27): native {nat:?} interp {int:?} -> {:.2}x",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_loop() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let native = jit_bench_loop_value(BenchTier::Hot);
    let cold = jit_bench_loop_value(BenchTier::Cold);
    let n = 5_000_000i64;
    let want = Value::make_int(n * (n + 1) / 2);
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    panic!(
        "BENCH loop(5M): native {nat:?} interp {int:?} -> {:.2}x",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// GATE-RELAXATION BENCH: a CALL-DOMINATED body (4 calls + 2 arith per iter, so
/// `calls > arith` -> `body_is_jit_profitable` DECLINES it) that the JIT normally
/// keeps interpreted. Forces the gate OFF so it tiers to native, then A/Bs native
/// vs interp: ratio > 1 means tiering a call-heavy body is now NET-POSITIVE, so
/// the gate could relax (the workstream's end goal). Pair with `NEOVM_JIT_LEVER1`
/// on/off to see how much lever 1 moved it (the pre-lever-1 baseline was ~+5.6%
/// SLOWER when forced open). Callee is a hot trivial `(lambda (x) x)` so the calls
/// hit the native-to-native spec fast path.
#[cfg(feature = "jit")]
fn jit_bench_call_bound_caller(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    // (lambda (n) (while (> n 0) (cbleaf n)(cbleaf n)(cbleaf n)(cbleaf n)
    //                            (setq n (1- n))) n)  ; slot0 = n
    f.ops = vec![
        Op::StackRef(0),   // 0  n
        Op::Constant(0),   // 1  0
        Op::Gtr,           // 2  n>0
        Op::GotoIfNil(24), // 3  -> exit
        Op::Constant(1),   // 4  'cbleaf
        Op::StackRef(1),   // 5  n
        Op::Call(1),       // 6  (cbleaf n)
        Op::Pop,           // 7
        Op::Constant(1),   // 8
        Op::StackRef(1),   // 9
        Op::Call(1),       // 10
        Op::Pop,           // 11
        Op::Constant(1),   // 12
        Op::StackRef(1),   // 13
        Op::Call(1),       // 14
        Op::Pop,           // 15
        Op::Constant(1),   // 16
        Op::StackRef(1),   // 17
        Op::Call(1),       // 18
        Op::Pop,           // 19
        Op::StackRef(0),   // 20 n
        Op::Sub1,          // 21 n-1
        Op::StackSet(1),   // 22 slot0 = n-1
        Op::Goto(0),       // 23 loop
        Op::StackRef(0),   // 24 exit: n (=0)
        Op::Return,        // 25
    ];
    f.constants = vec![Value::make_int(0), Value::symbol("jit-bench-cbleaf")].into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_call_bound_loop() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // Hot trivial callee (lambda (x) x) so the calls hit the native fast path.
    let mut leaf = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    leaf.lexical = true;
    // (lambda (x) (if x x x)) — multi-block so the caller CANNOT inline it (which
    // would DCE the discarded calls and defeat the call-bound measurement). Returns
    // x for the non-nil fixnum args this bench passes.
    leaf.ops = vec![
        Op::StackRef(0),  // 0  x
        Op::GotoIfNil(4), // 1  (never taken for fixnum x)
        Op::StackRef(0),  // 2  x
        Op::Return,       // 3
        Op::StackRef(0),  // 4  x (else)
        Op::Return,       // 5
    ];
    leaf.max_stack = 8;
    leaf.jit_runtime().set_hot_for_test();
    let ValueKind::Symbol(cbleaf_id) = Value::symbol("jit-bench-cbleaf").kind() else {
        panic!("symbol")
    };
    ev.obarray
        .set_symbol_function_id(cbleaf_id, Value::make_bytecode(leaf));
    // Force the profitability gate OFF so the call-dominated caller actually tiers
    // (else NotProfitable keeps BOTH copies interpreted -> interp-vs-interp ~1x).
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let native = jit_bench_call_bound_caller(BenchTier::Hot);
    let cold = jit_bench_call_bound_caller(BenchTier::Cold);
    let n = 200_000i64;
    let want = Value::make_int(0);
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    panic!(
        "BENCH call-bound-loop(n={n}, 4 calls/iter): native {nat:?} interp {int:?} -> {:.3}x (>1 = tiering net-positive, gate could relax)",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// GATE-RELAXATION BENCH, BUILTIN variant — the font-lock case the user-fn bench
/// above could NOT answer. Same call-dominated shape but the 4 calls/iter are
/// `CallBuiltinSym` to a cheap NON-intrinsified builtin (`abs`), so they take the
/// generic builtin-dispatch shim (NOT the native-to-native user-fn fast path).
/// `abs` is trivial, so this isolates the builtin CALL DISPATCH cost (native vs
/// interp) — real font-lock builtins (re-search-forward) do buffer work that's
/// identical either way, so dispatch is the only thing tiering changes. ratio > 1
/// => even builtin-heavy bodies are net-positive tiered (gate can relax broadly);
/// ratio < 1 => builtin dispatch is the gate's remaining valid concern.
#[cfg(feature = "jit")]
fn jit_bench_builtin_bound_caller(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::intern::intern;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    let abs = intern("abs");
    // (lambda (n) (while (> n 0) (abs n)(abs n)(abs n)(abs n) (setq n (1- n))) n)
    f.ops = vec![
        Op::StackRef(0),            // 0  n
        Op::Constant(0),            // 1  0
        Op::Gtr,                    // 2  n>0
        Op::GotoIfNil(20),          // 3  -> exit
        Op::StackRef(0),            // 4  n
        Op::CallBuiltinSym(abs, 1), // 5  (abs n)
        Op::Pop,                    // 6
        Op::StackRef(0),            // 7
        Op::CallBuiltinSym(abs, 1), // 8
        Op::Pop,                    // 9
        Op::StackRef(0),            // 10
        Op::CallBuiltinSym(abs, 1), // 11
        Op::Pop,                    // 12
        Op::StackRef(0),            // 13
        Op::CallBuiltinSym(abs, 1), // 14
        Op::Pop,                    // 15
        Op::StackRef(0),            // 16 n
        Op::Sub1,                   // 17 n-1
        Op::StackSet(1),            // 18 slot0 = n-1
        Op::Goto(0),                // 19 loop
        Op::StackRef(0),            // 20 exit: n (=0)
        Op::Return,                 // 21
    ];
    f.constants = vec![Value::make_int(0)].into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_builtin_bound_loop() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let native = jit_bench_builtin_bound_caller(BenchTier::Hot);
    let cold = jit_bench_builtin_bound_caller(BenchTier::Cold);
    let n = 200_000i64;
    let want = Value::make_int(0);
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    panic!(
        "BENCH builtin-bound-loop(n={n}, 4 CallBuiltinSym abs/iter): native {nat:?} interp {int:?} -> {:.3}x (>1 = tiering net-positive)",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// R2-D DECISIVE BENCH (the AOT sweet spot): a COMPUTE-heavy AOT-candidate body
/// called FEWER than `HOT_THRESHOLD` (10k) times — so the JIT NEVER tiers it up
/// (it stays interpreted), but AOT serves it NATIVE FROM CALL 1. This is the case
/// the trivial-accessor batch bench could NOT show: AOT native code == the JIT's
/// MIR codegen, so AOT can only inherit a win where the JIT HAS one — i.e. on
/// compute, not on trivial/call-dominated bodies. Measures AOT-native-from-call-1
/// vs interpreter on the SAME pure-arith loop body.
///
/// Methodology mirrors `jit_bench_loop` (warm once, min-of-N, BENCH-panic), but
/// the "native" side is served through the REAL AOT path (emit → link → inject the
/// unit → `NEOVM_AOT=force` → `try_run_compiled` serves it AOT-backed at heat=0),
/// and the "interp" side is a force-COLD copy that never tiers (heat pinned cold).
/// The body does ~`n` internal arithmetic iterations per call; we use a moderate
/// `n` and report the single-call min (the per-call native-vs-interp ratio is the
/// no-warmup throughput the prewarm delivers from call 1).
#[cfg(all(feature = "jit", target_os = "linux"))]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn aot_bench_compute_loop() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::jit::aot;
    use crate::emacs_core::value::LambdaParams;

    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    // The SAME pure-arith countdown-accumulator loop body as `jit_bench_loop_value`
    // (Constant/StackRef/Gtr/GotoIfNil/Add/Sub1/StackSet/Goto/Return) — AOT-runnable.
    // Copied VERBATIM from that helper so both sides run identical semantics.
    let ops = vec![
        Op::Constant(0),
        Op::StackRef(1),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(13),
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Add,
        Op::StackSet(1),
        Op::StackRef(1),
        Op::Sub1,
        Op::StackSet(2),
        Op::Goto(1),
        Op::StackRef(0),
        Op::Return,
    ];
    let constants = vec![Value::make_int(0)];
    let arity = 1usize;

    // Emit + link the body's AOT `.so`, dlopen, inject by content hash (the pure
    // arith body has NO shim imports, so its `.so` dlopens in the unit-test binary).
    let (obj, content_hash) = aot::compile_leaf_to_object(&ops, &constants, arity, None)
        .expect("compile ok")
        .expect("pure-arith body is AOT-runnable");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join("aot_bench_compute.so");
    aot::link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(crate::emacs_core::jit::compile::LoadedUnit::new(lib));
    aot::test_support::set_forced_enabled(true);
    aot::test_support::inject_unit(content_hash, unit);

    // The AOT-served copy: a FRESH bytecode fn (its own compiled_id), cold heat —
    // try_run_compiled will consult AOT FIRST (forced) and serve it native from
    // call 1 (no JIT warmup). Interp copy: force-COLD, never tiers (< HOT_THRESHOLD).
    let mut aot_fn = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    aot_fn.lexical = true;
    aot_fn.ops = ops.clone();
    aot_fn.constants = constants.clone().into();
    aot_fn.max_stack = 16;
    let aot_val = Value::make_bytecode(aot_fn.clone());

    let cold = jit_bench_loop_value(BenchTier::Cold);

    // Confirm the AOT copy is actually served AOT-backed (not JIT'd) at heat=0.
    let ctx = &mut ev as *mut Context;
    let n = 2_000_000i64;
    let want = Value::make_int(n * (n + 1) / 2);
    let first = crate::emacs_core::jit::cache::try_run_compiled(
        ctx,
        &aot_fn,
        aot_val,
        &[Value::make_int(n)],
    )
    .expect("aot run ok");
    assert_eq!(first, Some(want.bits()), "AOT compute result");
    assert_eq!(
        crate::emacs_core::jit::cache::cached_leaf_is_aot_for_func(&aot_fn),
        Some(true),
        "the compute body must be served AOT-backed (native from call 1), not JIT'd"
    );

    // min-of-9 single-call wall-clock, each side. AOT = native-from-call-1;
    // cold = interpreted (never reaches HOT_THRESHOLD).
    let aot_min = {
        let mut best = std::time::Duration::MAX;
        for _ in 0..9 {
            let t = std::time::Instant::now();
            let r = crate::emacs_core::jit::cache::try_run_compiled(
                ctx,
                &aot_fn,
                aot_val,
                &[Value::make_int(n)],
            )
            .expect("aot run");
            best = best.min(t.elapsed());
            assert_eq!(r, Some(want.bits()));
        }
        best
    };
    let int_min = jit_bench_min(&mut ev, cold, n, want, 9);

    aot::test_support::reset();
    crate::emacs_core::jit::cache::clear();
    panic!(
        "BENCH aot-compute-loop(n={n}): aot-native {aot_min:?} interp {int_min:?} -> {:.2}x (native-from-call-1, no JIT warmup)",
        int_min.as_secs_f64() / aot_min.as_secs_f64()
    );
}

/// PART 1 GO/NO-GO bench for the CALL-HEAVY spec-bearing profitability re-weight
/// (`body_is_jit_profitable`). Today a font-lock/syntax sweep body is
/// `calls >> arith` -> `NotProfitable` -> it NEVER tiers, so the #1 `Op::Call`
/// `Many`-spec sites (re-search-forward / looking-at / parse-partial-sexp) that
/// it carries NEVER engage. The re-weight would let it tier; the question this
/// bench answers is whether that is net-POSITIVE.
///
/// A/B in ONE process (cancels CPU-frequency variance): a Hot copy of a REAL
/// byte-compiled font-lock sweep, compiled with the profitability gate FORCED
/// OFF so it actually tiers to native WITH the #1 `Many`-spec sites armed, vs a
/// force-Cold copy pinned to the Tier-0 interpreter. The body is a `while
/// re-search-forward` loop that at each match runs `looking-at` +
/// `parse-partial-sexp` over the current line — call-heavy, little arithmetic,
/// carrying exactly the `Op::Call` `Many`-spec population the re-weight unlocks.
/// `has_op_call_spec_sites` asserts the spec sites are actually present, so the
/// Hot side is native+spec, not a silent fallback. The reported ratio (>1 =
/// native+spec faster) is the ship/defer signal.
#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_call_heavy_fontlock_reweight() {
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::jit::compile;
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();

    // A sizeable, varied elisp-ish buffer, made PERMANENTLY current (no
    // with-current-buffer, so it stays current for the funcall'd scans). ~24 KB
    // of real defun/defvar/comment shapes so the search + line-parse do
    // realistic work (not a degenerate empty/tiny buffer).
    let setup = r#"(progn
      (set-buffer (get-buffer-create "*neo-bench-fl*"))
      (fundamental-mode)
      (erase-buffer)
      (dotimes (_ 80)
        (insert "(defun sample-alpha (a b) \"docstring here\" (let ((x (+ a b)) (y (- a b))) (if (> x y) (list x y 'ok) (cons y x))))\n")
        (insert "(defvar sample-beta 12345 \"a special variable\")\n")
        (insert ";; a comment line with (parens) and foo-bar-baz symbols\n")
        (insert "(defun sample-gamma (items acc) (while items (setq acc (cons (car items) acc)) (setq items (cdr items))) acc)\n"))
      (goto-char (point-min))
      (buffer-size))"#;
    let sz = ev.eval_str(setup).expect("bench buffer setup");

    // A REALISTIC font-lock/syntax sweep: search each `(defXXX`, look at the
    // following char class, and parse-partial-sexp the current line (bounded,
    // realistic "local syntactic context" check). Call-heavy, ~1 arith/iter.
    ev.eval_str(
        r#"(defun neo-bench-fontlock-scan ()
             (goto-char (point-min))
             (let ((count 0))
               (while (re-search-forward "(def[a-z]+" nil t)
                 (looking-at "[ \t]*[a-z]")
                 (parse-partial-sexp (line-beginning-position) (point))
                 (setq count (1+ count)))
               count))"#,
    )
    .expect("defun scan");
    ev.eval_str("(byte-compile 'neo-bench-fontlock-scan)")
        .expect("byte-compile scan");

    // Extract the byte-compiled body (the REAL compiler output, not hand-rolled).
    let fn_val = ev
        .eval_str("(symbol-function 'neo-bench-fontlock-scan)")
        .expect("symbol-function");
    let bc = fn_val
        .get_bytecode_data()
        .expect("scan must be byte-compiled to a bytecode object");
    let arity = bc.params.required.len();

    // Shape report: op histogram (documents that this really is call-heavy).
    let (mut n_call, mut n_cbsym, mut n_arith) = (0usize, 0usize, 0usize);
    for op in bc.executable_ops() {
        match op {
            Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) => n_call += 1,
            Op::CallBuiltinSym(..) => n_cbsym += 1,
            Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Rem
            | Op::Add1
            | Op::Sub1
            | Op::Negate
            | Op::Max
            | Op::Min
            | Op::Eqlsign
            | Op::Lss
            | Op::Gtr
            | Op::Leq
            | Op::Geq => n_arith += 1,
            _ => {}
        }
    }

    // CONFIRM the #1 Op::Call Many-spec (or round-1 fixed) sites are actually
    // present — so the Hot side really is native+spec, not a silent fallback.
    let has_spec =
        compile::has_op_call_spec_sites(bc.executable_ops(), &bc.constants, arity, &ev.obarray);
    assert!(
        has_spec,
        "the byte-compiled sweep must carry >=1 Op::Call spec site (re-search-forward / looking-at / parse-partial-sexp); \
         ops-call={n_call} cbsym={n_cbsym} arith={n_arith}"
    );

    // A/B copies. Gate FORCED OFF so the call-heavy body actually compiles;
    // otherwise `NotProfitable` would keep BOTH on the interpreter and the bench
    // would measure interp-vs-interp (~1x — which is exactly the state the
    // re-weight would change).
    compile::force_profit_gate_for_test(false);
    let hot = bc.clone();
    hot.jit_runtime().set_hot_for_test();
    let hot_val = Value::make_bytecode(hot);
    // Clones SHARE tiering state (jit::Runtime); give the cold copy its own so
    // the pin does not reach the hot copy.
    let mut cold = bc.clone();
    cold.runtime = Some(crate::emacs_core::jit::Runtime::new());
    cold.jit_runtime().set_cold_for_test();
    let cold_val = Value::make_bytecode(cold);

    // Parity: native+spec sweep must equal the interpreter sweep.
    let want = ev
        .funcall_general_untraced(cold_val, vec![])
        .expect("cold scan runs");
    // Native-engagement PROOF: snapshot the armed-subr-spec fast counter around
    // the Hot warm-up. If it does not move, the Hot copy silently ran the
    // interpreter (compile bailed) and the whole bench would be interp-vs-interp
    // — the one confound that would fake a ~1x result. (debug-assertions only,
    // which is how release+jit is built here.)
    #[cfg(debug_assertions)]
    let fast_before = compile::SUBR_SPEC_FAST_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    let hot_first = ev
        .funcall_general_untraced(hot_val, vec![])
        .expect("hot scan runs");
    assert_eq!(
        hot_first.bits(),
        want.bits(),
        "native+spec sweep result == interpreter sweep result"
    );
    #[cfg(debug_assertions)]
    {
        let fast_after = compile::SUBR_SPEC_FAST_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            fast_after > fast_before,
            "the Hot copy must run NATIVE with the Many-spec fast path armed \
             (fast-count delta={}) — else this measures interp-vs-interp",
            fast_after - fast_before
        );
    }

    // Min-of-N wall clock (min cancels transient scheduler/thermal noise).
    let bench0 = |ev: &mut Context, f: Value, want: Value, iters: u32| -> std::time::Duration {
        assert_eq!(
            ev.funcall_general_untraced(f, vec![]).unwrap().bits(),
            want.bits()
        );
        let mut best = std::time::Duration::MAX;
        for _ in 0..iters {
            let t = std::time::Instant::now();
            let r = ev.funcall_general_untraced(f, vec![]).unwrap();
            best = best.min(t.elapsed());
            assert_eq!(r.bits(), want.bits());
        }
        best
    };
    let iters = 9u32;
    let nat = bench0(&mut ev, hot_val, want, iters);
    let int = bench0(&mut ev, cold_val, want, iters);
    panic!(
        "BENCH call-heavy-fontlock(buf={sz:?} matches={want:?} | ops={} call={n_call} cbsym={n_cbsym} arith={n_arith}): \
         native+spec {nat:?} interp {int:?} -> {:.3}x",
        bc.executable_ops().len(),
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// PART 1 UPPER-BOUND companion to `jit_bench_call_heavy_fontlock_reweight`: the
/// DISPATCH-DOMINATED extreme. A tight loop whose body is nothing but the
/// LIGHTEST allowlisted `Many`-spec builtin that the byte-compiler actually
/// emits as a generic `Op::Call` (`looking-at` on a 1-char regexp over the empty
/// scratch buffer — a near-immediate non-match). NOTE the dedicated-opcode
/// "cheap" buffer builtins (point/goto-char/widen/current-column/char-syntax/…)
/// are all `Op::CallBuiltinSym`, already covered by round-2 — so the `Op::Call`
/// `Many` population the re-weight actually unlocks is inherently the *heavier*
/// builtins; `looking-at` is about as light as that population gets. If EVEN this
/// shows no meaningful win, the re-weight cannot help the realistic shape either.
#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_spec_call_dispatch_upper_bound() {
    use crate::emacs_core::jit::compile;
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();

    // Body = a tight countdown loop calling a light Many-spec builtin 4x/iter
    // (results discarded), so dispatch is ~all of the per-iteration cost.
    ev.eval_str(
        r#"(defun neo-bench-spec-microloop (n)
             (let ((i 0))
               (while (< i n)
                 (looking-at "a") (looking-at "a") (looking-at "a") (looking-at "a")
                 (setq i (1+ i)))
               i))"#,
    )
    .expect("defun microloop");
    ev.eval_str("(byte-compile 'neo-bench-spec-microloop)")
        .expect("byte-compile microloop");
    let fn_val = ev
        .eval_str("(symbol-function 'neo-bench-spec-microloop)")
        .expect("symbol-function");
    let bc = fn_val
        .get_bytecode_data()
        .expect("microloop must be byte-compiled");
    let arity = bc.params.required.len();
    assert!(
        compile::has_op_call_spec_sites(bc.executable_ops(), &bc.constants, arity, &ev.obarray),
        "microloop must carry the looking-at Op::Call Many-spec site"
    );

    compile::force_profit_gate_for_test(false);
    let hot = bc.clone();
    hot.jit_runtime().set_hot_for_test();
    let hot_val = Value::make_bytecode(hot);
    // Clones SHARE tiering state (jit::Runtime); give the cold copy its own so
    // the pin does not reach the hot copy.
    let mut cold = bc.clone();
    cold.runtime = Some(crate::emacs_core::jit::Runtime::new());
    cold.jit_runtime().set_cold_for_test();
    let cold_val = Value::make_bytecode(cold);

    let n = 200_000i64;
    let want = Value::make_int(n);
    let arg = || vec![Value::make_int(n)];
    let cold_r = ev.funcall_general_untraced(cold_val, arg()).expect("cold");
    #[cfg(debug_assertions)]
    let fast_before = compile::SUBR_SPEC_FAST_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    let hot_r = ev.funcall_general_untraced(hot_val, arg()).expect("hot");
    assert_eq!(cold_r.bits(), want.bits());
    assert_eq!(hot_r.bits(), want.bits(), "native+spec result == interp");
    #[cfg(debug_assertions)]
    {
        let fast_after = compile::SUBR_SPEC_FAST_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            fast_after > fast_before,
            "Hot copy must run native+spec (fast delta={})",
            fast_after - fast_before
        );
    }

    let bench = |ev: &mut Context, f: Value, want: Value, iters: u32| -> std::time::Duration {
        let mut best = std::time::Duration::MAX;
        for _ in 0..iters {
            let t = std::time::Instant::now();
            let r = ev
                .funcall_general_untraced(f, vec![Value::make_int(n)])
                .unwrap();
            best = best.min(t.elapsed());
            assert_eq!(r.bits(), want.bits());
        }
        best
    };
    let iters = 9u32;
    let nat = bench(&mut ev, hot_val, want, iters);
    let int = bench(&mut ev, cold_val, want, iters);
    panic!(
        "BENCH spec-call-upper-bound(n={n}, 4x looking-at/iter): native+spec {nat:?} interp {int:?} -> {:.3}x",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// R2-E E2: the ONE-MORE-REAL-FN demo — a RECOGNIZABLE pure-fixnum algorithm
/// (Collatz step-count) served via the AOT path NATIVE FROM CALL 1 vs the
/// interpreter. The body is the REAL byte-compiled `rb-collatz-steps` (verified
/// off-line via the byte-compiler that its hot loop is ZERO CallBuiltin(Sym) —
/// only dedicated arith ops: Gtr/Rem/Eqlsign/Div/Mul/Add1, the unboxable fixnum
/// set the JIT/AOT actually speeds up). This is the NARROW pure-fixnum-compute
/// sweet spot: the win is real on a recognizable algorithm, but most real elisp
/// is shim-bound (aref/aset/logand/named-builtins → CallBuiltinSym → ~1x). We
/// show "here's WHERE it helps", not "AOT helps everywhere".
///
/// Workload: sum collatz-step-counts over a sweep of starting values, each call
/// < HOT_THRESHOLD so the interp copy never tiers (stays interpreted), while AOT
/// serves it native from call 1. AOT served via compile_leaf_to_object (no shims
/// in this body, so its `.so` dlopens in the unit-test binary) + inject + force.
#[cfg(all(feature = "jit", target_os = "linux"))]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn aot_bench_real_algorithm() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::jit::aot;
    use crate::emacs_core::value::LambdaParams;

    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    // rb-collatz-steps's REAL byte-compiled ops (extracted from the byte-compiler,
    // VERIFIED zero CallBuiltin(Sym) — pure dedicated arith). (defun rb-collatz-steps
    // (n) (let ((steps 0)) (while (> n 1) (if (= (% n 2) 0) (setq n (/ n 2))
    // (setq n (+ (* 3 n) 1))) (setq steps (1+ steps))) steps))
    let ops = vec![
        Op::Constant(0),   // 0  steps=0  [n steps]
        Op::StackRef(1),   // 1  [n steps n]   <- loop head
        Op::Constant(1),   // 2  [.. n 1]
        Op::Gtr,           // 3  [.. (> n 1)]
        Op::GotoIfNil(23), // 4  exit
        Op::StackRef(1),   // 5  [.. n]
        Op::Constant(2),   // 6  [.. n 2]
        Op::Rem,           // 7  [.. (% n 2)]
        Op::Constant(0),   // 8  [.. r 0]
        Op::Eqlsign,       // 9  [.. (= r 0)]
        Op::GotoIfNil(16), // 10 odd branch
        Op::StackRef(1),   // 11 [.. n]
        Op::Constant(2),   // 12 [.. n 2]
        Op::Div,           // 13 [.. (/ n 2)]
        Op::StackSet(2),   // 14 n=/   [n steps]
        Op::Goto(21),      // 15
        Op::StackRef(1),   // 16 [.. n]   <- odd
        Op::Constant(3),   // 17 [.. n 3]
        Op::Mul,           // 18 [.. (* 3 n)]  (constants[3]=3)
        Op::Add1,          // 19 [.. (1+ (* 3 n))]
        Op::StackSet(2),   // 20 n=3n+1
        Op::Add1,          // 21 steps=1+steps  [n steps']  <- join
        Op::Goto(1),       // 22 backedge
        Op::Return,        // 23 return steps
    ];
    let constants = vec![
        Value::make_int(0),
        Value::make_int(1),
        Value::make_int(2),
        Value::make_int(3),
    ];
    let arity = 1usize;

    // Correctness anchor: collatz(27)=111, collatz(97)=118 (independent reference).
    let collatz_ref = |mut n: i64| -> i64 {
        let mut s = 0;
        while n > 1 {
            n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
            s += 1;
        }
        s
    };
    assert_eq!(collatz_ref(27), 111);
    assert_eq!(collatz_ref(97), 118);

    // Emit + serve via AOT (pure body, no shim imports → dlopens in this binary).
    let (obj, content_hash) = aot::compile_leaf_to_object(&ops, &constants, arity, None)
        .expect("compile ok")
        .expect("pure-fixnum collatz body is AOT-runnable");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join("aot_bench_collatz.so");
    aot::link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(crate::emacs_core::jit::compile::LoadedUnit::new(lib));
    aot::test_support::set_forced_enabled(true);
    aot::test_support::inject_unit(content_hash, unit);

    // The AOT-served copy: a fresh bytecode fn, cold heat — try_run_compiled
    // consults AOT FIRST (forced) and serves it native from call 1.
    let mut aot_fn = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    aot_fn.lexical = true;
    aot_fn.ops = ops.clone();
    aot_fn.constants = constants.clone().into();
    aot_fn.max_stack = 16;
    let aot_val = Value::make_bytecode(aot_fn.clone());

    // Interp copy: force-COLD via the shared BenchTier mechanism, never tiers
    // (each call < HOT_THRESHOLD), invoked through the normal interpreter path.
    let mut cold_f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    cold_f.lexical = true;
    cold_f.ops = ops.clone();
    cold_f.constants = constants.clone().into();
    cold_f.max_stack = 16;
    BenchTier::Cold.apply(cold_f.jit_runtime());
    let cold_val = Value::make_bytecode(cold_f);

    let ctx = &mut ev as *mut Context;

    // Confirm AOT-served at heat=0 + correct result (collatz(27)=111).
    let n0 = 27i64;
    let r0 = crate::emacs_core::jit::cache::try_run_compiled(
        ctx,
        &aot_fn,
        aot_val,
        &[Value::make_int(n0)],
    )
    .expect("aot run ok");
    assert_eq!(
        r0,
        Some(Value::make_int(collatz_ref(n0)).bits()),
        "AOT collatz(27) result must be 111"
    );
    assert_eq!(
        crate::emacs_core::jit::cache::cached_leaf_is_aot_for_func(&aot_fn),
        Some(true),
        "collatz must be served AOT-backed (native from call 1), not JIT'd"
    );
    // Cross-check the interp copy agrees (and is genuinely interpreted, not tiered).
    assert_eq!(
        ev.funcall_general_untraced(cold_val, vec![Value::make_int(97)])
            .unwrap(),
        Value::make_int(collatz_ref(97)),
        "interp collatz(97) result must be 118"
    );
    let ctx = &mut ev as *mut Context;

    // Realistic pattern: sum collatz-step-counts over starting values 2..=sweep,
    // each a separate call (< HOT_THRESHOLD so the cold copy stays interpreted).
    // min-of-5 over the whole sweep. AOT = native from call 1; cold = interpreted.
    //
    // Two regimes on the SAME real algorithm, to separate the two costs honestly:
    //  (A) many SHORT calls (sweep 2..8000, ~100 inner iters each) — realistic call
    //      pattern; per-call dispatch (try_run/funcall entry) dilutes the body win.
    //  (B) FEW LONG calls (long-orbit fixnum-safe seeds, ~500 inner iters each) —
    //      isolates the inner-loop compute win, the regime where AOT pays off.
    // Both keep call-count < HOT_THRESHOLD so the interp copy never tiers.
    let aot_fn = &aot_fn; // borrow for closures
    let aot_val2 = aot_val;
    let mut regime =
        |calls: &[i64], reps: usize| -> (std::time::Duration, std::time::Duration, i64) {
            let want: i64 = calls.iter().map(|&n| collatz_ref(n)).sum::<i64>() * reps as i64;
            let aot_min = {
                let mut best = std::time::Duration::MAX;
                for _ in 0..9 {
                    let t = std::time::Instant::now();
                    let mut acc = 0i64;
                    for _ in 0..reps {
                        for &n in calls {
                            let r = crate::emacs_core::jit::cache::try_run_compiled(
                                ctx,
                                aot_fn,
                                aot_val2,
                                &[Value::make_int(n)],
                            )
                            .expect("aot run")
                            .expect("aot served");
                            acc += Value::from_bits(r).as_fixnum().expect("fixnum");
                        }
                    }
                    best = best.min(t.elapsed());
                    assert_eq!(acc, want, "AOT collatz regime sum");
                }
                best
            };
            let int_min = {
                let mut best = std::time::Duration::MAX;
                for _ in 0..9 {
                    let t = std::time::Instant::now();
                    let mut acc = 0i64;
                    for _ in 0..reps {
                        for &n in calls {
                            let r = ev
                                .funcall_general_untraced(cold_val, vec![Value::make_int(n)])
                                .unwrap();
                            acc += r.as_fixnum().expect("fixnum");
                        }
                    }
                    best = best.min(t.elapsed());
                    assert_eq!(acc, want, "interp collatz regime sum");
                }
                best
            };
            (aot_min, int_min, want)
        };

    // (A) realistic many-short-calls: 7999 calls × ~100 inner iters.
    let short_calls: Vec<i64> = (2..=8000).collect();
    let (a_aot, a_int, _) = regime(&short_calls, 1);
    let a_ratio = a_int.as_secs_f64() / a_aot.as_secs_f64();

    // (B) inner-loop-bound: long-orbit fixnum-safe seeds (524/685 steps), repeated.
    // 8 seeds × 800 reps = 6400 calls < HOT_THRESHOLD, but ~500 inner iters/call.
    let long_seeds: Vec<i64> = vec![703, 6171, 77031, 837799, 8400511, 6171, 837799, 8400511];
    let (b_aot, b_int, _) = regime(&long_seeds, 800);
    let b_ratio = b_int.as_secs_f64() / b_aot.as_secs_f64();

    aot::test_support::reset();
    crate::emacs_core::jit::cache::clear();
    panic!(
        "BENCH aot-real-collatz [pure-fixnum, ZERO CallBuiltin(Sym) — verified]: \
         (A) realistic many-short-calls(7999×~100it): aot {a_aot:?} interp {a_int:?} -> {a_ratio:.2}x \
         | (B) inner-loop-bound long-orbits(6400×~500it): aot {b_aot:?} interp {b_int:?} -> {b_ratio:.2}x. \
         NARROW sweet spot: the compute win ({b_ratio:.2}x) is REAL on this recognizable algorithm when \
         per-call work dominates; short calls are dispatch-bound ({a_ratio:.2}x); most real elisp is \
         shim-bound (~1x). Showing WHERE AOT helps, not that it helps everywhere."
    );
}

/// Profiling aid (NOT a pass/fail test): dump the dynamic, execution-weighted
/// opcode histogram the interpreter runs for a workload — which opcodes actually
/// dominate execution, the input the deferred tier-0 IC/quickening work needs to
/// size itself. Reuses the loop workload run force-COLD (interpreted) so every op
/// flows through the dispatch loop's `vm_profile::bump`. Like the benches it
/// reports via panic! so the dump surfaces under nextest's capture. Run with:
///   cargo nextest run -p neovm-core --features jit,vm-profile --release \
///     --run-ignored ignored-only --no-capture vm_op_mix_loop
#[cfg(all(feature = "jit", feature = "vm-profile"))]
#[test]
#[ignore = "profiling aid; run explicitly with --features vm-profile --no-capture"]
fn vm_op_mix_loop() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let cold = jit_bench_loop_value(BenchTier::Cold);
    let n = 1_000_000i64;
    let want = Value::make_int(n * (n + 1) / 2);
    crate::emacs_core::bytecode::vm::vm_profile::reset();
    assert_eq!(
        ev.funcall_general_untraced(cold, vec![Value::make_int(n)])
            .unwrap(),
        want
    );
    crate::emacs_core::bytecode::vm::vm_profile::dump("loop(1M) interpreted");
    panic!("OP-MIX dumped above (profiling aid, not a failure)");
}

/// Op-mix profiling aid on a REAL byte-compiled elisp workload (vs the hand-built
/// arithmetic loop): a recursive list build + recursive sum, driven 500x. Three
/// knobs make it count real-elisp ops: (1) runtime_startup_context (defun/dotimes
/// are Lisp MACROS from byte-run.el — a bare Context::new lacks them); (2)
/// byte-compile + assert byte-code-function-p (a plain defun is a tree-walked
/// closure that never reaches run_loop, so vm_profile would see 0 ops); (3)
/// NEOVM_JIT=0 so the hot body stays in the VM (else it tiers to native mid-run
/// and silently drops ops). The mix (Call/Car/Cdr/Cons/branch-weighted) is
/// genuinely different from the StackRef/StackSet-dominated hand-built loop —
/// representative of real list-processing elisp. Run:
///   cargo nextest run -p neovm-core --features vm-profile --release \
///     --run-ignored ignored-only --no-capture vm_op_mix_real_elisp
#[cfg(feature = "vm-profile")]
#[test]
#[ignore = "profiling aid; run explicitly with --features vm-profile --no-capture"]
fn vm_op_mix_real_elisp() {
    crate::test_utils::init_test_tracing();
    // SAFETY: nextest runs each test in its own process; this write happens before
    // the VM reads the JIT gate. Pins the body in run_loop so vm_profile counts
    // every op (a tiered-to-native body bumps nothing).
    unsafe { std::env::set_var("NEOVM_JIT", "0") };
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(
        "(progn \
           (defun rl-build (n acc) (if (= n 0) acc (rl-build (1- n) (cons n acc)))) \
           (defun rl-sum (lst) (if (null lst) 0 (+ (car lst) (rl-sum (cdr lst))))) \
           (defun rl-work (n) (rl-sum (rl-build n nil))) \
           (byte-compile 'rl-build) (byte-compile 'rl-sum) (byte-compile 'rl-work) t)",
    )
    .expect("setup defuns + byte-compile");
    assert_eq!(
        format_eval_result(&ev.eval_str("(byte-code-function-p (symbol-function 'rl-work))")),
        "OK t",
        "rl-work must be byte-compiled (else it tree-walks and counts no VM ops)"
    );
    crate::emacs_core::bytecode::vm::vm_profile::reset();
    let run = ev.eval_str("(let ((s 0)) (dotimes (_ 500) (setq s (rl-work 200))) s)");
    assert_eq!(format_eval_result(&run), "OK 20100");
    crate::emacs_core::bytecode::vm::vm_profile::dump("real-elisp list-processing");
    panic!("OP-MIX dumped above (profiling aid, not a failure)");
}

/// Builtin-call profiling aid on the heaviest real elisp we can run in-process:
/// the byte compiler itself (bytecomp/cconv/macroexp working over a nontrivial
/// defun). Dumps the per-builtin call ranking (SUBR-MIX section) recorded at
/// `subr_entry_from_value` — the evidence the JIT builtin-intrinsics work is
/// gated on: WHICH builtins dominate real workloads. Re-defuns each round so
/// byte-compile does full work every iteration. Run:
///   cargo nextest run -p neovm-core --features vm-profile --release \
///     --run-ignored ignored-only --no-capture vm_subr_mix_byte_compile
#[cfg(feature = "vm-profile")]
#[test]
#[ignore = "profiling aid; run explicitly with --features vm-profile --no-capture"]
fn vm_subr_mix_byte_compile() {
    crate::test_utils::init_test_tracing();
    // SAFETY: nextest runs each test in its own process; this write happens
    // before the VM reads the JIT gate (keeps the workload interpreted so the
    // recorded mix reflects call counts, not tiering artifacts).
    unsafe { std::env::set_var("NEOVM_JIT", "0") };
    let mut ev = crate::test_utils::runtime_startup_context();
    let mut body = String::new();
    for i in 0..30 {
        body.push_str(&format!(
            "(setq acc (cons (list {i} (format \"s%d\" n) (assq 'k tbl)) acc)) \
             (when (> (length acc) 40) (setq acc (nthcdr 2 acc))) \
             (setq s (concat s (substring (symbol-name 'sym{i}) 0 2))) ",
        ));
    }
    let defun = format!(
        "(progn (defun sm-work (n) \
           (let ((acc nil) (s \"\") (tbl '((k . 1) (j . 2)))) {body} (list acc s))) t)"
    );
    ev.eval_str(&defun).expect("defun sm-work");
    crate::emacs_core::bytecode::vm::vm_profile::reset();
    for _ in 0..3 {
        ev.eval_str(&defun).expect("re-defun sm-work");
        ev.eval_str("(progn (byte-compile 'sm-work) t)")
            .expect("byte-compile sm-work");
    }
    crate::emacs_core::bytecode::vm::vm_profile::dump("byte-compile x3");
    panic!("SUBR-MIX dumped above (profiling aid, not a failure)");
}

/// Round-2 profiling aid (task 02): the SUBR-MIX of a font-lock / interactive
/// editing workload — the population round 2 must intrinsify (round 1 profiled
/// the byte-compiler, a different mix). Two sub-workloads over the same 256 KiB
/// real-elisp buffer (`lisp/subr.el`, the regex fontlock-bench haystack), both
/// with `NEOVM_JIT=0` so every call stays in the interpreter and is counted:
///
///   B1. REAL `font-lock-fontify-region` chunk-by-chunk (jit-lock style), IF
///       font-lock loads in the runtime-startup context. This is the highest
///       fidelity: it runs font-lock's actual keyword + syntactic machinery.
///   B2. A byte-compiled editing + keyword-matcher loop (always runs). It
///       exercises BOTH JIT lowerings on purpose: buffer/point motion
///       (forward-line/point/bolp/eolp/following-char/current-column/…) lowers
///       to `Op::CallBuiltinSym`; re-search-forward + match-beginning/-end +
///       get/put-text-property arrive as generic `Op::Call`. The dump's entry
///       split (Op::Call vs CBSym) validates the round-2 instrumentation.
///
/// Run:
///   cargo nextest run -p neovm-core --features vm-profile --release \
///     --run-ignored ignored-only --no-capture vm_subr_mix_fontlock
#[cfg(feature = "vm-profile")]
#[test]
#[ignore = "profiling aid; run explicitly with --features vm-profile --no-capture"]
fn vm_subr_mix_fontlock() {
    use crate::emacs_core::bytecode::vm::vm_profile;
    crate::test_utils::init_test_tracing();
    // SAFETY: nextest runs each test in its own process; this write happens
    // before the VM reads the JIT gate. Pins bodies in run_loop so vm_profile
    // counts every call + entry (a tiered-to-native body bumps nothing).
    unsafe { std::env::set_var("NEOVM_JIT", "0") };
    let mut ev = crate::test_utils::runtime_startup_context();

    // 256 KiB of real elisp, cut at a char boundary (same as the regex benches).
    let path = concat!(env!("CARGO_WORKSPACE_DIR"), "/lisp/subr.el");
    let text = std::fs::read_to_string(path).expect("read lisp/subr.el haystack");
    let mut cut = text.len().min(256 * 1024);
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut escaped = String::with_capacity(cut + 1024);
    for ch in text[..cut].chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(ch),
        }
    }
    let kib = cut as f64 / 1024.0;
    ev.eval_str(&format!(
        "(progn (insert \"{escaped}\") (goto-char (point-min)) nil)"
    ))
    .expect("insert haystack");

    // --- B1: real font-lock-fontify-region, chunk-by-chunk (best effort). ---
    let setup = ev.eval_str(
        r#"(condition-case e
             (progn
               (require 'font-lock)
               (setq-local font-lock-defaults
                 '(("\\_<\\(defun\\|defvar\\|defmacro\\|defcustom\\|defconst\\|lambda\\|let\\*?\\|when\\|unless\\|if\\|cond\\|while\\|setq\\|dolist\\|dotimes\\|save-excursion\\)\\_>"
                    (0 font-lock-keyword-face))))
               (font-lock-set-defaults)
               t)
           (error (list 'font-lock-unavailable (error-message-string e))))"#,
    );
    if format_eval_result(&setup) == "OK t" {
        vm_profile::reset();
        let r = ev.eval_str(
            r#"(let ((pos (point-min)) (end (point-max)) (chunk 1500) (n 0))
                 (while (< pos end)
                   (let ((to (min end (+ pos chunk))))
                     (font-lock-fontify-region pos to)
                     (setq pos to n (1+ n))))
                 n)"#,
        );
        if format_eval_result(&r).starts_with("OK") {
            vm_profile::dump(&format!("B1 fontlock-real chunked(1500) over {kib:.0}KiB"));
        } else {
            // Best effort: real font-lock's internals are fragile in the bare
            // runtime-startup context (observed: invalid-function inside the
            // syntactic/keyword machinery). Not a harness failure — B2 + the
            // batch workload (task 02 (a), full lisp) cover real font-lock.
            eprintln!(
                "NOTE[B1]: chunked font-lock-fontify-region failed ({}); \
                 skipping B1 dump, continuing to B2.",
                format_eval_result(&r)
            );
        }
    } else {
        eprintln!(
            "NOTE[B1]: real font-lock unavailable in runtime_startup_context ({}); \
             relying on B2 + the batch workload (task 02 (a)) for real font-lock.",
            format_eval_result(&setup)
        );
    }

    // --- B2: byte-compiled editing + keyword-matcher loop (always runs). ---
    ev.eval_str(
        r#"(progn
             (defun t2-fl-emul (chunk)
               (let ((pos (point-min)) (end (point-max)) (hits 0))
                 (while (< pos end)
                   (let ((to (min end (+ pos chunk))))
                     (goto-char pos)
                     (while (re-search-forward
                             "(\\(def[a-z]+\\|let\\*?\\|if\\|when\\|unless\\|cond\\|while\\|setq\\|dolist\\|dotimes\\|lambda\\|save-excursion\\)\\_>" to t)
                       (let ((b (match-beginning 1)) (e (match-end 1)))
                         (put-text-property b e 'face 'font-lock-keyword-face)
                         (setq hits (1+ hits))))
                     (goto-char pos)
                     (while (re-search-forward "\\_<\\([a-z][a-z0-9-]+\\)\\_>" to t)
                       (let ((b (match-beginning 1)) (e (match-end 1)))
                         (when (eq (get-text-property b 'face) 'font-lock-keyword-face)
                           (setq hits (1+ hits)))
                         (put-text-property b e 'face 'font-lock-variable-name-face)))
                     (setq pos to)))
                 hits))
             (defun t2-edit (n)
               (goto-char (point-min))
               (let ((acc 0))
                 (dotimes (_ n)
                   (forward-line 1)
                   (end-of-line)
                   (when (bolp) (setq acc (1+ acc)))
                   (when (eolp) (setq acc (1+ acc)))
                   (setq acc (+ acc (following-char) (preceding-char)
                                (current-column) (point)))
                   (goto-char (max (point-min) (- (point) 3)))
                   (forward-char 1))
                 acc))
             (byte-compile 't2-fl-emul)
             (byte-compile 't2-edit)
             t)"#,
    )
    .expect("define + byte-compile emulation");
    assert_eq!(
        format_eval_result(&ev.eval_str("(and (byte-code-function-p (symbol-function 't2-fl-emul)) (byte-code-function-p (symbol-function 't2-edit)))")),
        "OK t",
        "emulation must be byte-compiled (else it tree-walks and misses run_loop entry hooks)"
    );
    vm_profile::reset();
    let r = ev.eval_str(
        "(progn (set-text-properties (point-min) (point-max) nil) \
                (t2-fl-emul 1500) (t2-edit 6000) t)",
    );
    assert_eq!(format_eval_result(&r), "OK t", "B2 emulation run");
    vm_profile::dump(&format!(
        "B2 editing+fontlock-matcher emulation (byte-compiled) over {kib:.0}KiB"
    ));
    panic!("SUBR-MIX dumped above (profiling aid, not a failure)");
}

/// Profiling aid (NOT a pass/fail test): measure the synchronous compile stall
/// the JIT pays at each first hot call, over a corpus of byte-compiled defuns
/// (the runtime_startup_context + byte-compile recipe from vm_op_mix_real_elisp
/// — a plain defun tree-walks and never reaches the JIT). Each function is
/// forced hot and called once — the call that compiles — then the per-thread
/// `jit::stats` aggregate is dumped bench-style via panic!. Run:
///   cargo nextest run -p neovm-core --release \
///     --run-ignored ignored-only --no-capture -E 'test(jit_compile_time_profile)'
#[cfg(feature = "jit")]
#[test]
#[ignore = "profiling aid; run explicitly in release with --no-capture"]
fn jit_compile_time_profile() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(
        "(progn \
           (defun jcp-sq (x) (* x x)) \
           (defun jcp-poly (x y) (+ (* x x) (* 2 x y) (* y y) (- x y) 1)) \
           (defun jcp-clamp (x lo hi) (cond ((< x lo) lo) ((> x hi) hi) (t x))) \
           (defun jcp-sum-to (n) (let ((s 0) (i 0)) (while (< i n) (setq s (+ s i)) (setq i (1+ i))) s)) \
           (defun jcp-fib (n) (if (< n 2) n (+ (jcp-fib (- n 1)) (jcp-fib (- n 2))))) \
           (defun jcp-build (n acc) (if (= n 0) acc (jcp-build (1- n) (cons n acc)))) \
           (defun jcp-sum (lst) (if (null lst) 0 (+ (car lst) (jcp-sum (cdr lst))))) \
           (defun jcp-work (n) (jcp-sum (jcp-build n nil))) \
           (defun jcp-vecsum (v) (let ((s 0) (i 0) (n (length v))) (while (< i n) (setq s (+ s (aref v i))) (setq i (1+ i))) s)) \
           (defun jcp-mix (a b c) \
             (let* ((d (+ (* a a) (* b b) (* c c))) \
                    (e (- (* 3 d) (+ a b c))) \
                    (g (+ (* d e) (- e a) (* 2 b))) \
                    (h (- (* g g) (* d e) (+ g d e)))) \
               (+ (* h 5) (- g h) (* d 7) e))) \
           (dolist (f '(jcp-sq jcp-poly jcp-clamp jcp-sum-to jcp-fib jcp-build \
                        jcp-sum jcp-work jcp-vecsum jcp-mix)) \
             (byte-compile f)) \
           t)",
    )
    .expect("setup defuns + byte-compile");
    crate::emacs_core::jit::stats::reset_compile_stats();
    for (name, call) in [
        ("jcp-sq", "(jcp-sq 9)"),
        ("jcp-poly", "(jcp-poly 3 4)"),
        ("jcp-clamp", "(jcp-clamp 12 0 10)"),
        ("jcp-sum-to", "(jcp-sum-to 100)"),
        ("jcp-fib", "(jcp-fib 12)"),
        ("jcp-build", "(jcp-build 20 nil)"),
        ("jcp-sum", "(jcp-sum '(1 2 3 4 5))"),
        ("jcp-work", "(jcp-work 50)"),
        ("jcp-vecsum", "(jcp-vecsum [1 2 3 4 5 6 7 8])"),
        ("jcp-mix", "(jcp-mix 2 3 4)"),
    ] {
        let f = ev
            .eval_str(&format!("(symbol-function '{name})"))
            .expect(name);
        let bc = f
            .get_bytecode_data()
            .unwrap_or_else(|| panic!("{name} must be byte-compiled (else it never tiers up)"));
        bc.jit_runtime().set_hot_for_test();
        ev.eval_str(call).expect(call);
    }
    let snap = crate::emacs_core::jit::stats::compile_stats_snapshot();
    assert!(
        snap.total_compiles > 0,
        "forcing the corpus hot must reach the JIT at least once"
    );
    panic!(
        "COMPILE-STATS {} (profiling aid, not a failure)",
        crate::emacs_core::jit::stats::format_summary(&snap)
    );
}

/// Build the Task-4 dispatch-bench caller: the `jit_bench_loop_value` skeleton
/// with the `acc += n` body replaced by `acc = (CALLEE acc)` — an `Op::Call`
/// per iteration on the designator in `constants[1]`. All three bench arms use
/// THIS EXACT op sequence; only that constant differs (closure symbol / closure
/// value / builtin symbol), so a time delta isolates callee-resolution cost.
fn vm_bench_call_loop_caller(callee_designator: Value) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(0),   // acc = 0                  -> [n, acc]
        Op::StackRef(1),   // loop: push n             -> [n, acc, n]
        Op::Constant(0),   //                          -> [n, acc, n, 0]
        Op::Gtr,           // n > 0                    -> [n, acc, t/nil]
        Op::GotoIfNil(13), //                          -> [n, acc]
        Op::Constant(1),   // push callee designator   -> [n, acc, f]
        Op::StackRef(1),   // push acc                 -> [n, acc, f, acc]
        Op::Call(1),       // r = (f acc)              -> [n, acc, r]
        Op::StackSet(1),   // acc = r                  -> [n, acc]
        Op::StackRef(1),   // push n                   -> [n, acc, n]
        Op::Sub1,          //                          -> [n, acc, n-1]
        Op::StackSet(2),   // n = n-1                  -> [n, acc]
        Op::Goto(1),
        Op::StackRef(0), // exit:                    -> [n, acc, acc]
        Op::Return,
    ];
    f.constants = vec![Value::make_int(0), callee_designator].into();
    f.max_stack = 16;
    Value::make_bytecode(f)
}

/// Warm once, then min wall-clock of `iters` calls of `f(n)` (mirrors
/// `jit_bench_min` without the jit feature gate — this is an interpreter
/// dispatch bench).
fn vm_bench_min_call(
    ev: &mut Context,
    f: Value,
    n: i64,
    want: Value,
    iters: u32,
) -> std::time::Duration {
    assert_eq!(
        ev.funcall_general_untraced(f, vec![Value::make_int(n)])
            .unwrap(),
        want
    );
    let mut best = std::time::Duration::MAX;
    for _ in 0..iters {
        let t = std::time::Instant::now();
        let r = ev
            .funcall_general_untraced(f, vec![Value::make_int(n)])
            .unwrap();
        best = best.min(t.elapsed());
        assert_eq!(r, want);
    }
    best
}

/// Task-4 Step-2 GATE bench: per-call cost of the three `Op::Call` dispatch
/// shapes the session profile splits — closure-via-SYMBOL (re-resolves the
/// function cell every call: the population a per-site resolved-target cache
/// would serve), closure-via-VALUE (no resolution: the cache's best-case
/// ceiling, since a hit still pays its guard), and builtin-via-symbol (the
/// already-array-indexed comparison point). Identical loop/op shape in all
/// arms; the callee closure is `(lambda (x) (1+ x))` hand-built. The
/// symbol-minus-value delta is the UPPER BOUND on what any resolution cache
/// can recover per call. Run:
///   cargo nextest run -p neovm-core --release \
///     --run-ignored ignored-only --no-capture -E 'test(vm_bench_call_dispatch)'
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn vm_bench_call_dispatch_closure_sym_vs_val() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    crate::test_utils::init_test_tracing();
    // SAFETY: nextest runs each test in its own process; set before the VM
    // reads the JIT gate so every arm stays on the Tier-0 interpreter (the
    // dispatch path this bench measures).
    unsafe { std::env::set_var("NEOVM_JIT", "0") };
    let mut ev = Context::new();

    // Callee closure: (lambda (x) (1+ x)) — at entry the stack is [x].
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::Add1, Op::Return];
    callee.max_stack = 4;
    let callee_val = Value::make_bytecode(callee);

    let ValueKind::Symbol(callee_sym) = Value::symbol("vm-bench-callee").kind() else {
        panic!()
    };
    ev.obarray.set_symbol_function_id(callee_sym, callee_val);

    let caller_sym = vm_bench_call_loop_caller(Value::symbol("vm-bench-callee"));
    let caller_val = vm_bench_call_loop_caller(callee_val);
    let caller_builtin = vm_bench_call_loop_caller(Value::symbol("1+"));

    let n = 2_000_000i64;
    let want = Value::make_int(n);
    let t_sym = vm_bench_min_call(&mut ev, caller_sym, n, want, 7);
    let t_val = vm_bench_min_call(&mut ev, caller_val, n, want, 7);
    let t_builtin = vm_bench_min_call(&mut ev, caller_builtin, n, want, 7);
    let per_call = |d: std::time::Duration| d.as_secs_f64() * 1e9 / n as f64;
    panic!(
        "BENCH call-dispatch({n} calls): closure-sym {t_sym:?} ({:.1} ns/call) | \
         closure-val {t_val:?} ({:.1} ns/call) | builtin-sym(1+) {t_builtin:?} ({:.1} ns/call) | \
         sym-vs-val delta {:.1} ns/call = {:.2}% of the sym arm (resolution-cache ceiling)",
        per_call(t_sym),
        per_call(t_val),
        per_call(t_builtin),
        per_call(t_sym) - per_call(t_val),
        100.0 * (per_call(t_sym) - per_call(t_val)) / per_call(t_sym).max(1e-9),
    );
}

/// Build the Task-4 VarRef-bench caller: the same loop skeleton with the body
/// `acc = acc + (varref SYM)` — one `Op::VarRef` per iteration on the symbol
/// in `constants[1]` (whose value must be the fixnum 1, so the loop result is
/// `n`). Arms differ ONLY in which symbol `constants[1]` names.
fn vm_bench_varref_loop_caller(var_sym: Value) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(0),   // acc = 0                  -> [n, acc]
        Op::StackRef(1),   // loop: push n             -> [n, acc, n]
        Op::Constant(0),   //                          -> [n, acc, n, 0]
        Op::Gtr,           // n > 0                    -> [n, acc, t/nil]
        Op::GotoIfNil(14), //                          -> [n, acc]
        Op::StackRef(0),   // push acc                 -> [n, acc, acc]
        Op::VarRef(1),     // push (symbol-value SYM)  -> [n, acc, acc, v]
        Op::Add,           //                          -> [n, acc, acc+v]
        Op::StackSet(1),   // acc = acc+v              -> [n, acc]
        Op::StackRef(1),   // push n                   -> [n, acc, n]
        Op::Sub1,          //                          -> [n, acc, n-1]
        Op::StackSet(2),   // n = n-1                  -> [n, acc]
        Op::Goto(1),
        Op::StackRef(0), // exit:                    -> [n, acc, acc]
        Op::Return,
    ];
    f.constants = vec![Value::make_int(0), var_sym].into();
    f.max_stack = 16;
    Value::make_bytecode(f)
}

/// Task-4 Step-2 GATE bench (BLV side): per-read cost of `Op::VarRef` on a
/// SYMBOL_LOCALIZED buffer-local (the session's 58% class — every read runs
/// `swap_in_blv`, an unconditional assq walk of the buffer's whole
/// `local_var_alist`) vs a Plainval global (the direct-value fast path). The
/// localized symbol is created FIRST so it sits DEEPEST in the ~31-entry
/// alist, matching a font-lock/syntax-ppss-style mode buffer where the hot
/// syntax-ppss locals predate dozens of later `make-local-variable`s. The
/// plain arm is the floor a same-buffer swap cache could approach. Run:
///   cargo nextest run -p neovm-core --release \
///     --run-ignored ignored-only --no-capture -E 'test(vm_bench_varref)'
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn vm_bench_varref_localized_vs_plain() {
    crate::test_utils::init_test_tracing();
    // SAFETY: nextest per-process isolation; set before the VM reads the gate.
    unsafe { std::env::set_var("NEOVM_JIT", "0") };
    let mut ev = Context::new();

    ev.eval_str("(set-buffer (get-buffer-create \"vmb\"))")
        .expect("create bench buffer");
    // Measured BLV first -> deepest alist position after the pads prepend.
    ev.eval_str("(set (make-local-variable 'vmb-blv) 1)")
        .expect("make vmb-blv buffer-local");
    for i in 0..30 {
        ev.eval_str(&format!("(set (make-local-variable 'vmb-pad-{i}) {i})"))
            .expect("pad local");
    }
    ev.eval_str("(set 'vmb-plain 1)").expect("plain global");

    let caller_blv = vm_bench_varref_loop_caller(Value::symbol("vmb-blv"));
    let caller_plain = vm_bench_varref_loop_caller(Value::symbol("vmb-plain"));

    let n = 2_000_000i64;
    let want = Value::make_int(n);
    let t_blv = vm_bench_min_call(&mut ev, caller_blv, n, want, 7);
    let t_plain = vm_bench_min_call(&mut ev, caller_plain, n, want, 7);
    let per_call = |d: std::time::Duration| d.as_secs_f64() * 1e9 / n as f64;
    panic!(
        "BENCH varref({n} reads, 31-local alist, blv deepest): localized {t_blv:?} \
         ({:.1} ns/read) | plain {t_plain:?} ({:.1} ns/read) | delta {:.1} ns/read = \
         {:.2}% of the localized arm (swap-cache ceiling)",
        per_call(t_blv),
        per_call(t_plain),
        per_call(t_blv) - per_call(t_plain),
        100.0 * (per_call(t_blv) - per_call(t_plain)) / per_call(t_blv).max(1e-9),
    );
}

/// Build a zero-arg bytecode function whose body is exactly `Op::VarRef` on
/// `sym` — the Task-4 BLV swap-cache tests read through THIS so every assert
/// exercises the real interpreter fast path (`fast_path_var_ref` →
/// `find_symbol_value_in_buffer`'s same-buffer arm), not the tree-walker.
fn varref_reader_fn(sym: Value) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::VarRef(1), Op::Return];
    f.constants = vec![Value::NIL, sym].into();
    f.max_stack = 4;
    Value::make_bytecode(f)
}

/// Read `reader` (a [`varref_reader_fn`] value) several times and return the
/// last result — repeated reads both WARM the BLV swap cache and prove the
/// cached value is served consistently.
fn read_var_warm(ev: &mut Context, reader: Value) -> Value {
    let mut last = Value::NIL;
    for _ in 0..3 {
        last = ev.funcall_general_untraced(reader, vec![]).unwrap();
    }
    last
}

/// Task-4 BLV swap-cache invalidation tests: the same-buffer fast path in
/// `find_symbol_value_in_buffer` must never serve a stale `valcell` across
/// setq / kill-local-variable / kill-all-local-variables / set-default /
/// make-local-variable / let-binding / buffer switches / the raw
/// buffer-helper alist edits. Expected values pinned against GNU Emacs 31
/// (`emacs --batch` probes, Task-4 report §5).
#[test]
fn blv_swap_cache_setq_updates_cached_read() {
    let mut ev = Context::new();
    ev.eval_str("(set-default 'blvt-a 1)").unwrap();
    ev.eval_str("(set-buffer (get-buffer-create \"blvt-a\"))")
        .unwrap();
    ev.eval_str("(set (make-local-variable 'blvt-a) 5)")
        .unwrap();
    let reader = varref_reader_fn(Value::symbol("blvt-a"));
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(5));
    ev.eval_str("(setq blvt-a 6)").unwrap();
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(6));
}

#[test]
fn blv_swap_cache_kill_local_variable_reverts_to_default() {
    let mut ev = Context::new();
    ev.eval_str("(set-default 'blvt-b 1)").unwrap();
    ev.eval_str("(set-buffer (get-buffer-create \"blvt-b\"))")
        .unwrap();
    ev.eval_str("(set (make-local-variable 'blvt-b) 5)")
        .unwrap();
    let reader = varref_reader_fn(Value::symbol("blvt-b"));
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(5));
    // GNU: after kill-local-variable the read reverts to the default (1).
    ev.eval_str("(kill-local-variable 'blvt-b)").unwrap();
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(1));
}

#[test]
fn blv_swap_cache_make_local_then_set_default_keeps_snapshot() {
    let mut ev = Context::new();
    ev.eval_str("(set-default 'blvt-c 1)").unwrap();
    ev.eval_str("(set-buffer (get-buffer-create \"blvt-c\"))")
        .unwrap();
    // make-local WITHOUT set: GNU snapshots the default at make-local time.
    ev.eval_str("(make-local-variable 'blvt-c)").unwrap();
    let reader = varref_reader_fn(Value::symbol("blvt-c"));
    // Warm the cache on the local binding (currently equal to the default).
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(1));
    // GNU pin: local read stays 1, default-value becomes 9.
    ev.eval_str("(set-default 'blvt-c 9)").unwrap();
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(1));
    assert_eq!(
        ev.eval_str("(default-value 'blvt-c)").unwrap(),
        Value::make_int(9)
    );
}

#[test]
fn blv_swap_cache_kill_all_local_variables_reverts() {
    let mut ev = Context::new();
    ev.eval_str("(set-default 'blvt-d 1)").unwrap();
    ev.eval_str("(set-buffer (get-buffer-create \"blvt-d\"))")
        .unwrap();
    ev.eval_str("(set (make-local-variable 'blvt-d) 5)")
        .unwrap();
    let reader = varref_reader_fn(Value::symbol("blvt-d"));
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(5));
    ev.eval_str("(kill-all-local-variables)").unwrap();
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(1));
}

#[test]
fn blv_swap_cache_buffer_switch_swaps_values() {
    let mut ev = Context::new();
    ev.eval_str("(set-default 'blvt-e 1)").unwrap();
    ev.eval_str("(set-buffer (get-buffer-create \"blvt-e1\"))")
        .unwrap();
    ev.eval_str("(set (make-local-variable 'blvt-e) 5)")
        .unwrap();
    let reader = varref_reader_fn(Value::symbol("blvt-e"));
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(5));
    ev.eval_str("(set-buffer (get-buffer-create \"blvt-e2\"))")
        .unwrap();
    ev.eval_str("(set (make-local-variable 'blvt-e) 7)")
        .unwrap();
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(7));
    ev.eval_str("(set-buffer \"blvt-e1\")").unwrap();
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(5));
    ev.eval_str("(set-buffer \"blvt-e2\")").unwrap();
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(7));
}

#[test]
fn blv_swap_cache_let_binding_shadows_and_restores() {
    let mut ev = Context::new();
    ev.eval_str("(set-default 'blvt-f 1)").unwrap();
    ev.eval_str("(set-buffer (get-buffer-create \"blvt-f\"))")
        .unwrap();
    ev.eval_str("(set (make-local-variable 'blvt-f) 5)")
        .unwrap();
    let reader = varref_reader_fn(Value::symbol("blvt-f"));
    let ValueKind::Symbol(reader_sym) = Value::symbol("blvt-f-reader").kind() else {
        panic!()
    };
    ev.obarray.set_symbol_function_id(reader_sym, reader);
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(5));
    // GNU pin: inside the let the bytecode read sees 7; after, 5 again.
    assert_eq!(
        ev.eval_str("(let ((blvt-f 7)) (blvt-f-reader))").unwrap(),
        Value::make_int(7)
    );
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(5));
}

/// The raw `Buffer::set_buffer_local_by_sym_id` /
/// `set_buffer_local_void_by_sym_id` helpers edit `local_var_alist` WITHOUT
/// updating the BLV cache (the eval.rs specpdl restore-into-buffer shape) —
/// exactly the paths the structural-epoch bump exists for.
#[test]
fn blv_swap_cache_raw_buffer_helper_edits_are_seen() {
    let mut ev = Context::new();
    ev.eval_str("(set-default 'blvt-g 1)").unwrap();
    ev.eval_str("(set-buffer (get-buffer-create \"blvt-g\"))")
        .unwrap();
    // Localized redirect with NO local binding in this buffer yet: reads warm
    // the cache on defcell.
    ev.eval_str("(make-variable-buffer-local 'blvt-g)").unwrap();
    let reader = varref_reader_fn(Value::symbol("blvt-g"));
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(1));
    let ValueKind::Symbol(sym) = Value::symbol("blvt-g").kind() else {
        panic!()
    };
    // Raw helper PREPENDS a binding behind the cache's back.
    let buf_id = ev.buffers.current_buffer_id().unwrap();
    ev.buffers
        .get_mut(buf_id)
        .unwrap()
        .set_buffer_local_by_sym_id(sym, Value::make_int(8));
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(8));
    // Raw helper REMOVES it again behind the cache's back.
    ev.buffers
        .get_mut(buf_id)
        .unwrap()
        .set_buffer_local_void_by_sym_id(sym);
    assert_eq!(read_var_warm(&mut ev, reader), Value::make_int(1));
}

/// CallBuiltinSym-dominated benchmark: a loop calling the primitive `length`
/// via `Op::CallBuiltinSym` each iteration (the byte-compiler's inlined-
/// primitive call opcode, opcodes 0140-0177). Isolates the JIT's named-builtin
/// dispatch cost (callbuiltinsym_for_jit -> dispatch_vm_builtin), a SEPARATE
/// path from the general Op::Call subr dispatch.
#[cfg(feature = "jit")]
fn jit_bench_cbsym_value(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(0),   // 0  [n n]  <- head
        Op::Constant(0),   // 1  [n n 0]
        Op::Gtr,           // 2  [n c]
        Op::GotoIfNil(11), // 3  [n]
        Op::Constant(1),   // 4  '(a b c)  [n list]
        Op::CallBuiltinSym(crate::emacs_core::intern::intern("length"), 1), // 5 [n len]
        Op::Pop,           // 6  [n]
        Op::StackRef(0),   // 7  [n n]
        Op::Sub1,          // 8  [n n-1]
        Op::StackSet(1),   // 9  [n-1]
        Op::Goto(0),       // 10 backedge
        Op::StackRef(0),   // 11 [n n]
        Op::Return,        // 12
    ];
    f.constants = vec![
        Value::make_int(0),
        Value::list_from_slice(&[Value::symbol("a"), Value::symbol("b"), Value::symbol("c")]),
    ]
    .into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_cbsym() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let native = jit_bench_cbsym_value(BenchTier::Hot);
    let cold = jit_bench_cbsym_value(BenchTier::Cold);
    let n = 2_000_000i64;
    let want = Value::make_int(0);
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    // NOTE (R2): `length` is now a Tier-B CBSym intrinsic, so `native` here
    // exercises `neovm_jit_cbsym_spec` (dispatch skip). Run with
    // NEOVM_JIT_FORCE_CBSYM_GENERIC=1 to A/B the intrinsic against the general
    // `neovm_jit_named_builtin` lowering in the same binary.
    panic!(
        "BENCH cbsym-loop(2M length-CallBuiltinSym): native {nat:?} interp {int:?} -> {:.2}x",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// Tier-B CallBuiltinSym intrinsic benchmark: a loop calling the BUFFER
/// primitive `goto-char` (position 1, idempotent) via `Op::CallBuiltinSym` each
/// iteration — the hot interactive population R2 targets. `native` routes
/// through `neovm_jit_cbsym_spec` (dispatch skip); NEOVM_JIT_FORCE_CBSYM_GENERIC=1
/// forces the general `neovm_jit_named_builtin` path for an in-binary A/B.
#[cfg(feature = "jit")]
fn jit_bench_cbsym_goto_value(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(0),   // 0  [n n]  <- head
        Op::Constant(0),   // 1  [n n 0]
        Op::Gtr,           // 2  [n c]
        Op::GotoIfNil(11), // 3  [n]
        Op::Constant(1),   // 4  [n 1]  (goto-char position)
        Op::CallBuiltinSym(crate::emacs_core::intern::intern("goto-char"), 1), // 5 [n pos]
        Op::Pop,           // 6  [n]
        Op::StackRef(0),   // 7  [n n]
        Op::Sub1,          // 8  [n n-1]
        Op::StackSet(1),   // 9  [n-1]
        Op::Goto(0),       // 10 backedge
        Op::StackRef(0),   // 11 [n n]
        Op::Return,        // 12
    ];
    f.constants = vec![Value::make_int(0), Value::make_int(1)].into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_cbsym_goto() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str("(insert \"hello world\")")
        .expect("buffer content");
    let native = jit_bench_cbsym_goto_value(BenchTier::Hot);
    let cold = jit_bench_cbsym_goto_value(BenchTier::Cold);
    let n = 2_000_000i64;
    let want = Value::make_int(0);
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    panic!(
        "BENCH cbsym-goto-loop(2M goto-char-CallBuiltinSym Tier-B): native {nat:?} interp {int:?} -> {:.2}x",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// Subr-call-dominated benchmark: a loop that calls the primitive `length` via
/// the general `Op::Call` path each iteration (NOT the inlined `Op::Length`
/// opcode) — isolates the cost of the JIT's subr-call dispatch, the 75.4% of
/// real-elisp calls that go to primitives. The decrement (`1-`) is inlined, so
/// the per-iteration delta over a pure loop is the subr call.
#[cfg(feature = "jit")]
fn jit_bench_subr_value(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(0),   // 0  [n n]      <- loop head
        Op::Constant(0),   // 1  [n n 0]
        Op::Gtr,           // 2  [n c]
        Op::GotoIfNil(12), // 3  [n]
        Op::Constant(1),   // 4  'length    [n length]
        Op::Constant(2),   // 5  '(a b c)   [n length list]
        Op::Call(1),       // 6  [n len]    <- subr call via general dispatch
        Op::Pop,           // 7  [n]
        Op::StackRef(0),   // 8  [n n]
        Op::Sub1,          // 9  [n n-1]    inlined decrement
        Op::StackSet(1),   // 10 [n-1]
        Op::Goto(0),       // 11 backedge
        Op::StackRef(0),   // 12 [n n]
        Op::Return,        // 13
    ];
    f.constants = vec![
        Value::make_int(0),
        Value::symbol("length"),
        Value::list_from_slice(&[Value::symbol("a"), Value::symbol("b"), Value::symbol("c")]),
    ]
    .into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_subr() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let native = jit_bench_subr_value(BenchTier::Hot);
    let cold = jit_bench_subr_value(BenchTier::Cold);
    let n = 2_000_000i64;
    let want = Value::make_int(0);
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    panic!(
        "BENCH subr-loop(2M length-calls): native {nat:?} interp {int:?} -> {:.2}x",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// R2 phase 2 Many-spec benchmark: a loop DOMINATED by `looking-at` (an
/// allowlisted `SubrFn::Many` builtin) at a fixed point — the font-lock inner-loop
/// shape. Three Many calls vs two inlined arith ops per iteration, so
/// `calls > arith`: the profitability gate rejects this body by default (COMMIT B
/// is exactly the question of whether to re-tier it). The caller forces the gate
/// off so the Hot copy compiles; the armed Op::Call site dispatches the exact
/// 1-arg slice straight to the Many subr (skipping symbol resolution + generic
/// dispatch), vs the interpreter's full protocol. `looking-at` updates the match
/// state in place and returns an immediate (t/nil) — no per-call lisp allocation,
/// so the loop isolates dispatch cost cleanly.
#[cfg(feature = "jit")]
fn jit_bench_many_value(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(0),   // 0  [n n]        <- loop head
        Op::Constant(0),   // 1  [n n 0]
        Op::Gtr,           // 2  [n c]
        Op::GotoIfNil(20), // 3  [n]          -> END
        Op::Constant(1),   // 4  looking-at
        Op::Constant(2),   // 5  regex
        Op::Call(1),       // 6  [n t/nil]    <- Many subr via general dispatch
        Op::Pop,           // 7  [n]
        Op::Constant(1),   // 8
        Op::Constant(2),   // 9
        Op::Call(1),       // 10
        Op::Pop,           // 11
        Op::Constant(1),   // 12
        Op::Constant(2),   // 13
        Op::Call(1),       // 14
        Op::Pop,           // 15
        Op::StackRef(0),   // 16 [n n]
        Op::Sub1,          // 17 [n n-1]      inlined decrement
        Op::StackSet(1),   // 18 [n-1]
        Op::Goto(0),       // 19 backedge
        Op::StackRef(0),   // 20 [n n]        END
        Op::Return,        // 21
    ];
    f.constants = vec![
        Value::make_int(0),
        Value::symbol("looking-at"),
        Value::string("(defun\\|[a-z]+"),
    ]
    .into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_many() {
    crate::test_utils::init_test_tracing();
    // Call-DOMINATED body (3 Many calls vs 2 arith) — NotProfitable by default,
    // so force the gate off to tier the Hot copy. This is the exact shape COMMIT
    // B weighs: does forced-tier + Many-spec beat the interpreter? Cold stays
    // interpreted (the tier-0 baseline production runs without the re-weight).
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.eval_str("(insert \"(defun f (a b) (+ a b))\")")
        .expect("buffer content for looking-at");
    ev.eval_str("(goto-char (point-min))")
        .expect("point to bob");
    let native = jit_bench_many_value(BenchTier::Hot);
    let cold = jit_bench_many_value(BenchTier::Cold);
    let n = std::env::var("NEOVM_BENCH_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000_000i64);
    let want = Value::make_int(0);
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    panic!(
        "BENCH many-loop({n}*3 looking-at Many-spec): native {nat:?} interp {int:?} -> {:.2}x",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// Predicate-call benchmark: a loop calling the primitive `recordp` via the
/// general `Op::Call` path each iteration — the byte-compile workload's shape
/// (recordp / symbol-with-pos-p / keywordp arrive as generic `Op::Call`s, not
/// inlined predicate opcodes). Isolates the Gap-1 predicate fast-path shim: a
/// speculated `recordp` site collapses to quit-poll + epoch check + tag test,
/// vs the full generic protocol (bc_buf arg push, backtrace frame, subr entry
/// resolution + dispatch). Same loop skeleton as [`jit_bench_subr_value`], so
/// the two are directly comparable.
#[cfg(feature = "jit")]
fn jit_bench_pred_value(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(0),   // 0  [n n]      <- loop head
        Op::Constant(0),   // 1  [n n 0]
        Op::Gtr,           // 2  [n c]
        Op::GotoIfNil(12), // 3  [n]
        Op::Constant(1),   // 4  'recordp   [n recordp]
        Op::Constant(2),   // 5  '(a b c)   [n recordp list]
        Op::Call(1),       // 6  [n nil]    <- predicate call via general dispatch
        Op::Pop,           // 7  [n]
        Op::StackRef(0),   // 8  [n n]
        Op::Sub1,          // 9  [n n-1]    inlined decrement
        Op::StackSet(1),   // 10 [n-1]
        Op::Goto(0),       // 11 backedge
        Op::StackRef(0),   // 12 [n n]
        Op::Return,        // 13
    ];
    f.constants = vec![
        Value::make_int(0),
        Value::symbol("recordp"),
        Value::list_from_slice(&[Value::symbol("a"), Value::symbol("b"), Value::symbol("c")]),
    ]
    .into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_pred() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let native = jit_bench_pred_value(BenchTier::Hot);
    let cold = jit_bench_pred_value(BenchTier::Cold);
    let n = 2_000_000i64;
    let want = Value::make_int(0);
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    panic!(
        "BENCH pred-loop(2M recordp-calls): native {nat:?} interp {int:?} -> {:.2}x",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

/// Allocation-throughput benchmark: a loop that conses one `(n . n)` cell per
/// iteration and immediately discards it (garbage), churning the cons allocator
/// and the GC without growing the live heap. Isolates allocation + collection
/// cost — the workload that slab/generational GC would target. Both tiers poll
/// the GC at back-edges (`neovm_jit_backedge` mirrors the interpreter's
/// `bytecode_branch_maybe_gc_and_quit`), so the native and interpreter runs
/// collect on the same schedule; the ratio is GC-cost-fair.
#[cfg(feature = "jit")]
fn jit_bench_cons_value(tier: BenchTier) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    // (lambda (n) (while (> n 0) (cons n n) (setq n (1- n))) n)
    f.ops = vec![
        Op::StackRef(0),   // 0  [n n]      <- loop head
        Op::Constant(0),   // 1  [n n 0]
        Op::Gtr,           // 2  [n c]
        Op::GotoIfNil(12), // 3  [n]
        Op::StackRef(0),   // 4  [n n]
        Op::StackRef(1),   // 5  [n n n]
        Op::Cons,          // 6  [n cons]   <- allocate (n . n)
        Op::Pop,           // 7  [n]        <- discard => garbage
        Op::StackRef(0),   // 8  [n n]
        Op::Sub1,          // 9  [n n-1]
        Op::StackSet(1),   // 10 [n-1]
        Op::Goto(0),       // 11 backedge
        Op::StackRef(0),   // 12 [n n]
        Op::Return,        // 13
    ];
    f.constants = vec![Value::make_int(0)].into();
    f.max_stack = 16;
    tier.apply(f.jit_runtime());
    Value::make_bytecode(f)
}

#[cfg(feature = "jit")]
#[test]
#[ignore = "macro benchmark; run explicitly in release"]
fn jit_bench_cons() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let native = jit_bench_cons_value(BenchTier::Hot);
    let cold = jit_bench_cons_value(BenchTier::Cold);
    // Root BOTH function objects via obarray for the whole test. This is a
    // heavy-allocation benchmark: the native phase triggers ~40 GCs/run, so a
    // bare Rust local is NOT a GC root (exact GC never scans the Rust stack) and
    // the not-currently-executing copy would be swept mid-test and its memory
    // reused — the cold phase would then execute a freed ByteCodeObj. Interning
    // on symbols keeps both copies reachable, exactly like `jit_bench_fib`.
    let ValueKind::Symbol(nid) = Value::symbol("jit-bench-cons-n").kind() else {
        panic!()
    };
    let ValueKind::Symbol(cid) = Value::symbol("jit-bench-cons-c").kind() else {
        panic!()
    };
    ev.obarray.set_symbol_function_id(nid, native);
    ev.obarray.set_symbol_function_id(cid, cold);
    let n = 2_000_000i64;
    let want = Value::make_int(0);
    // GC frequency over ONE native run: gc_collections() counts completed cycles
    // (monotonic). Each ~16-byte cons against the 800 KB gc-cons-threshold means
    // ~40 cycles for 2M conses. (allocated_count is NOT usable here — sweep
    // resets it to the live count, so its delta is net live growth, not volume.)
    let gc0 = ev.tagged_heap.gc_collections();
    let nat = jit_bench_min(&mut ev, native, n, want, 9);
    let gc = ev.tagged_heap.gc_collections() - gc0;
    let int = jit_bench_min(&mut ev, cold, n, want, 9);
    let per_cons_ns = nat.as_secs_f64() * 1e9 / n as f64;
    let gc_per_run = gc as f64 / 10.0; // jit_bench_min does 1 warmup + 9 timed calls
    panic!(
        "BENCH cons-churn(2M): native {nat:?} interp {int:?} -> {:.2}x | \
         {per_cons_ns:.1} ns/cons native | ~{gc_per_run:.0} GC cycles/run",
        int.as_secs_f64() / nat.as_secs_f64()
    );
}

#[test]
fn direct_context_apply_accepts_uninterned_symbol_function_designators() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fun = intern_uninterned("vm-apply-uninterned");
    let callable = ev
        .eval_str("(lambda (x) (+ x 1))")
        .expect("lambda should evaluate");
    ev.obarray.set_symbol_function_id(fun, callable);

    let result = ev
        .apply(Value::from_sym_id(fun), vec![Value::fixnum(41)])
        .expect("Context::apply should funcall uninterned symbol");

    assert_eq!(result, Value::fixnum(42));
}

#[test]
fn macro_expansion_scope_uses_lexenv_dynvars() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;
    ev.bind_lexical_value_rooted(intern("macro-scope-a"), Value::fixnum(1));
    ev.bind_lexical_value_rooted(intern("macro-scope-b"), Value::fixnum(2));
    ev.lexenv = Value::cons(Value::from_sym_id(intern("macro-scope-special")), ev.lexenv);
    let specpdl_count = ev.specpdl.len();
    let dyn_sym = intern("macro-scope-dyn");
    ev.try_specbind(dyn_sym, Value::fixnum(9))
        .expect("macro scope dynamic binding");

    let state = ev
        .begin_macro_expansion_scope()
        .expect("enter macro expansion scope");

    // lexical-binding is specbound as the last entry, with the GcRoot
    // for macroexp--dynvars at the penultimate position.
    assert!(matches!(ev.specpdl.last(), Some(SpecBinding::Let { .. })));
    assert!(matches!(
        ev.specpdl.get(specpdl_count + 1),
        Some(SpecBinding::GcRoot { .. })
    ));

    let dynvars = ev
        .obarray
        .symbol_value_id(macroexp_dynvars_symbol())
        .copied()
        .expect("macroexp--dynvars should be bound inside macro expansion scope");
    let dynvars = list_to_vec(&dynvars).expect("macroexp--dynvars should stay a proper list");
    assert!(dynvars.contains(&Value::T), "{dynvars:?}");
    assert!(
        dynvars.contains(&Value::from_sym_id(intern("macro-scope-special"))),
        "{dynvars:?}"
    );
    assert!(
        !dynvars.contains(&Value::from_sym_id(intern("macro-scope-a"))),
        "{dynvars:?}"
    );
    assert!(
        !dynvars.contains(&Value::from_sym_id(intern("macro-scope-b"))),
        "{dynvars:?}"
    );
    assert!(
        !dynvars.contains(&Value::from_sym_id(intern("macro-scope-dyn"))),
        "{dynvars:?}"
    );

    ev.finish_macro_expansion_scope(state, Ok(Value::NIL))
        .expect("macro expansion scope cleanup");

    assert!(
        ev.specpdl.len() == specpdl_count + 1,
        "macro expansion scope should release only its temporary specpdl roots"
    );
    ev.unbind_to(specpdl_count);
    assert!(ev.specpdl.is_empty());
}

#[test]
fn gc_collect_frees_unreachable() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // Create orphaned conses that aren't bound to any variable.
    let _ = ev.eval_str("(progn (cons 1 2) (cons 3 4) (cons 5 6) nil)");
    let before = ev.tagged_heap.allocated_count();
    ev.gc_collect();
    let after = ev.tagged_heap.allocated_count();
    // The orphaned conses should have been freed.
    assert!(
        after < before,
        "gc did not free unreachable objects: before={before}, after={after}"
    );
}

#[test]
fn gc_collect_handles_cycles() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // Create a circular list: (setq x (cons 1 nil)) (setcdr x x)
    let _ = ev.eval_str("(progn (setq x (cons 1 nil)) (setcdr x x) t)");
    // GC should handle cycles without infinite loop.
    ev.gc_collect();
    // x is still reachable.
    let results = ev.eval_str_each("(car x)");
    assert_eq!(format_eval_result(&results[0]), "OK 1");

    // Now remove the root and collect — the cycle should be freed.
    ev.eval_str_each("(setq x nil)");
    let before = ev.tagged_heap.allocated_count();
    ev.gc_collect();
    let after = ev.tagged_heap.allocated_count();
    assert!(
        after < before,
        "cyclic cons not freed: before={before}, after={after}"
    );
}

#[test]
fn gc_safe_point_collects_when_threshold_reached() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(5);
    // Allocate enough conses to exceed threshold.
    ev.eval_str_each("(progn (cons 1 2) (cons 3 4) (cons 5 6) (cons 7 8) (cons 9 10) nil)");
    assert!(
        ev.gc_count > 0 || ev.gc_pending || ev.tagged_heap.should_collect(),
        "incremental GC should be pending, active, or already finished"
    );
    // With incremental GC, safe point may need multiple calls to finish.
    while ev.gc_count == 0 {
        ev.gc_safe_point();
    }
    assert!(ev.gc_count > 0);
}

#[test]
fn gc_threshold_adapts_after_collection() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // Create 3 conses that are reachable via variables.
    ev.eval_str_each("(progn (setq a (cons 1 2)) (setq b (cons 3 4)) (setq c (cons 5 6)))");
    ev.gc_collect();
    // GNU uses a byte threshold driven by `gc-cons-threshold` and
    // `gc-cons-percentage`, not a raw object-count heuristic.
    let alive = ev.tagged_heap.allocated_count();
    assert!(alive >= 3);
    let threshold = ev.tagged_heap.gc_threshold();
    assert!(
        threshold >= 800_000,
        "threshold should track GNU's default byte budget, got {threshold}"
    );
}

#[test]
fn gc_threshold_grows_with_live_heap() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // Retain ~200k conses (~3.2 MB of cons cells alone) so the live heap far
    // exceeds the default 800 KB byte budget, then collect so `live_bytes` is
    // recomputed exactly at the sweep and the threshold re-synced from it.
    ev.eval_str_each("(setq gc-live-growth-anchor (make-list 200000 0))");
    ev.gc_collect();
    let retained_bytes = 200_000 * 2 * std::mem::size_of::<usize>();
    assert!(
        ev.tagged_heap.live_bytes() >= retained_bytes,
        "live heap should include the retained structure, got {}",
        ev.tagged_heap.live_bytes()
    );
    // The live-proportional trigger requires allocating at least half the live
    // heap before the next cycle (the elisp defaults stay floors below it).
    assert!(
        ev.tagged_heap.gc_threshold() >= retained_bytes / 2,
        "threshold should grow with the live heap, got {}",
        ev.tagged_heap.gc_threshold()
    );
}

#[test]
fn gc_startup_ceiling_bounds_explicit_huge_cons_threshold() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str_each(
        "(progn
           (setq neomacs--startup-gc-ceiling-active t)
           (setq gc-cons-percentage nil)
           (setq gc-cons-threshold 268435456))",
    );
    assert_eq!(
        ev.tagged_heap.gc_threshold(),
        GC_STARTUP_THRESHOLD_CEILING_BYTES
    );
}

#[test]
fn gc_explicit_huge_cons_threshold_stays_the_floor_after_startup() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // Once startup is complete, an explicit `gc-cons-threshold` far above the
    // live-proportional term wins exactly. Clearing the host's startup flag
    // releases the ceiling; no later threshold mutation or collection is
    // needed.
    ev.eval_str_each(
        "(progn
           (setq neomacs--startup-gc-ceiling-active t)
           (setq gc-cons-percentage nil)
           (setq gc-cons-threshold 268435456)
           (setq neomacs--startup-gc-ceiling-active nil))",
    );
    assert_eq!(ev.tagged_heap.gc_threshold(), 268_435_456);
}

#[test]
fn gc_runtime_setting_mutation_reloads_threshold_immediately() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str_each(
        "(progn
           (setq gc-cons-percentage nil)
           (setq gc-cons-threshold 1234567))",
    );
    assert_eq!(ev.tagged_heap.gc_threshold(), 1_234_567);

    ev.eval_str_each("(setq gc-cons-threshold 2345678)");
    assert_eq!(ev.tagged_heap.gc_threshold(), 2_345_678);
}

#[test]
fn gc_collect_uses_exact_root_tracing() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    ev.eval_str_each("(setq mode-root (cons 7 8))");
    ev.gc_collect();

    let results = ev.eval_str_each("(car mode-root)");
    assert_eq!(format_eval_result(&results[0]), "OK 7");
}

#[test]
fn gc_safe_point_uses_exact_root_tracing() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(5);

    ev.eval_str_each(
        "(progn
           (setq mode-safe-root (cons 7 8))
           (cons 1 2)
           (cons 3 4)
           (cons 5 6)
           (cons 9 10)
           nil)",
    );

    while ev.gc_count == 0 {
        ev.gc_safe_point();
    }

    let results = ev.eval_str_each("(car mode-safe-root)");
    assert_eq!(format_eval_result(&results[0]), "OK 7");
}

#[test]
fn gc_safe_point_runs_concurrent_cycles_without_a_dump() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // A rooted structure that must survive every cycle.
    ev.eval_str_each("(setq gc-concurrent-root (cons 7 8))");
    // Small threshold so safe points trigger collections immediately.
    ev.tagged_heap.set_gc_threshold(1024);
    // Bootstrap: the first completed safe-point collection is stop-the-world.
    while ev.gc_count == 0 {
        ev.eval_str_each("(make-list 64 0)");
        ev.gc_safe_point();
    }
    assert!(
        ev.tagged_heap.should_run_concurrent(),
        "a dump-less heap must enable the concurrent collector after bootstrap"
    );

    // Churn through more safe points until another cycle completes. With
    // `should_run_concurrent` true and no forced GC, the only completing
    // path is concurrent mark -> STW termination -> deferred sweep slices,
    // so `gc_count` advancing proves a full concurrent cycle ran.
    let bootstrap_count = ev.gc_count;
    let mut saw_mark_overlap = false;
    for _ in 0..50_000 {
        ev.eval_str_each("(make-list 64 0)");
        ev.gc_safe_point();
        // Opportunistic: a mark observed in flight between safe points.
        saw_mark_overlap |= ev.tagged_heap.concurrent_mark_running();
        if ev.gc_count > bootstrap_count {
            break;
        }
    }
    assert!(
        ev.gc_count > bootstrap_count,
        "a concurrent cycle must terminate and complete its deferred sweep \
         (gc_count={}, bootstrap_count={bootstrap_count}, overlap_seen={saw_mark_overlap})",
        ev.gc_count,
    );
    // The rooted structure survived the concurrent cycles.
    let results = ev.eval_str_each("(car gc-concurrent-root)");
    assert_eq!(format_eval_result(&results[0]), "OK 7");
}

/// Handshake instrumentation (root-scan floor probe): a full concurrent cycle
/// driven through the evaluator must populate `HandshakeStats` end to end —
/// both handshake counters, the whole-pause start total, the per-GROUP
/// context-root breakdowns on BOTH handshakes (with nonzero counts for the
/// groups this heap actually has), and the context-side size probes.
#[test]
fn gc_concurrent_handshake_stats_populate_per_group() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str_each("(setq gc-handshake-root (cons 1 2))");
    ev.tagged_heap.set_gc_threshold(1024);
    // Bootstrap STW cycle first; then churn until a concurrent cycle
    // completes (same driver as `gc_safe_point_runs_concurrent_cycles_
    // without_a_dump`).
    while ev.gc_count == 0 {
        ev.eval_str_each("(make-list 64 0)");
        ev.gc_safe_point();
    }
    assert!(ev.tagged_heap.should_run_concurrent());
    let bootstrap_count = ev.gc_count;
    for _ in 0..50_000 {
        ev.eval_str_each("(make-list 64 0)");
        ev.gc_safe_point();
        if ev.gc_count > bootstrap_count {
            break;
        }
    }
    assert!(
        ev.gc_count > bootstrap_count,
        "no concurrent cycle completed"
    );

    let hs = ev.tagged_heap.handshake_stats();
    assert!(hs.start_count >= 1, "start handshake never recorded");
    assert!(hs.term_count >= 1, "termination handshake never recorded");
    assert!(
        hs.last_start_total_us > 0,
        "whole start pause must be timed"
    );
    assert!(hs.max_start_total_us >= hs.last_start_total_us);
    assert!(hs.max_term_roots_total_us >= hs.last_term_roots_total_us);
    for (which, breakdown) in [
        ("start", &hs.last_start_roots),
        ("termination", &hs.last_term_ctxroots),
    ] {
        assert!(
            !breakdown.groups.is_empty(),
            "{which}: per-group breakdown empty"
        );
        // `misc` visits lexenv/quit-flag/inhibit-quit unconditionally.
        assert!(
            breakdown.group_count("misc") >= 3,
            "{which}: misc group must visit the unconditional singletons \
             (count={})",
            breakdown.group_count("misc"),
        );
        // At least one live buffer's marker chain head is installed.
        assert!(
            breakdown.group_count("marker_heads") >= 1,
            "{which}: marker_heads count = live buffers must be >= 1"
        );
    }
    // Context-side size probes.
    assert!(hs.probe_obarray_slots > 0, "obarray slots probe empty");
    assert!(hs.probe_obarray_chunks > 0, "obarray chunks probe empty");
    assert!(hs.probe_buffer_count >= 1, "buffer count probe empty");
    assert!(
        hs.probe_vector_snapshot_len > 0,
        "a bootstrapped dump-less heap owns vectors; Tier B snapshot empty"
    );
    assert!(hs.probe_cons_blocks > 0, "cons block probe empty");
}

#[test]
fn gc_safe_point_exact_inside_extra_root_scope_retains_explicit_slice() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(2);
    let rooted = Value::cons(Value::fixnum(21), Value::fixnum(22));
    let _unreachable = Value::cons(Value::fixnum(1), Value::fixnum(2));
    let before = ev.tagged_heap.allocated_count();

    while ev.gc_count == 0 {
        let scope = ev.save_specpdl_roots();
        ev.push_specpdl_root(rooted);
        ev.gc_safe_point_exact();
        ev.restore_specpdl_roots(scope);
    }

    let after = ev.tagged_heap.allocated_count();
    assert_eq!(rooted.cons_car(), Value::fixnum(21));
    assert!(
        after < before,
        "exact safe point with explicit roots should free unrelated garbage: before={before}, after={after}"
    );
}

#[test]
fn gc_safe_point_exact_frees_stack_only_values() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(1);
    let marker = 0u8;
    ev.tagged_heap.set_stack_bottom(&marker as *const u8);

    ev.gc_collect_exact();
    let baseline = ev.tagged_heap.allocated_count();
    let gc_count_before = ev.gc_count;
    let stack_only = Value::cons(Value::fixnum(41), Value::fixnum(42));
    let keep_visible = [stack_only];
    std::hint::black_box(&keep_visible);
    let after_alloc = ev.tagged_heap.allocated_count();
    assert_eq!(
        after_alloc,
        baseline + 1,
        "stack-only cons should have allocated exactly one object after the baseline collection: baseline={baseline}, after_alloc={after_alloc}"
    );

    while ev.gc_count == gc_count_before {
        ev.gc_safe_point_exact();
    }

    let after_gc = ev.tagged_heap.allocated_count();
    assert_eq!(
        after_gc, baseline,
        "exact GC safe points must ignore the configured conservative stack scan and free stack-only objects: baseline={baseline}, after_alloc={after_alloc}, after_gc={after_gc}"
    );
}

/// The load-bearing claim behind DIVERGENCES.md 163's audit.
///
/// ~680 `Value::as_lisp_string` call sites hand out a `&'static LispString`
/// into a mark-sweep heap, and the single largest group of them — 102 of the
/// 235 that bind the borrow to a name — reads a SUBR'S OWN ARGUMENT. Every one
/// of those is sound for one reason and one reason only: `apply_internal`
/// pushes a backtrace frame carrying the arguments before dispatching, and
/// `Context::trace_roots` visits it, so the argument is rooted for the whole
/// call however much Lisp the subr runs.
///
/// GNU roots the same thing by name, in the same place: `mark_specpdl`'s
/// `SPECPDL_BACKTRACE` arm marks `backtrace_function` and every
/// `backtrace_args` slot (`src/eval.c`), which is what makes `Faset`-style C
/// primitives free to hold `SDATA (arg)` across a `Fsignal`.
///
/// Nothing pinned that. This does, together with its control below: if the
/// backtrace frame ever stops being a root, this test goes red at the exact
/// invariant instead of leaving 102 borrow sites silently live.
#[test]
fn a_subr_argument_string_survives_a_collection_inside_the_subr() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(1);
    ev.gc_collect_exact();

    // A fresh string reachable from nothing but the argument list — exactly
    // what a subr sees when Lisp calls it with a computed string.
    let arg = Value::heap_string(crate::heap_types::LispString::from_utf8("subr-argument"));
    let bt_count = ev.specpdl.len();
    ev.push_backtrace_frame(Value::symbol("neovm-probe-subr"), &[arg]);

    // The subr body borrows its argument...
    let borrowed = arg.as_lisp_string().expect("the argument is a string");
    assert_eq!(borrowed.as_bytes(), b"subr-argument");

    // ...and then runs Lisp, i.e. crosses a safepoint. Force the collection
    // that a safepoint may perform.
    let gc_before = ev.gc_count;
    while ev.gc_count == gc_before {
        ev.gc_safe_point_exact();
    }

    // `as_lisp_string` panics on a reclaimed object now (its `data` is GNU's
    // free marker), so a lost root aborts here rather than returning bytes
    // from freed storage.
    assert_eq!(
        arg.as_lisp_string()
            .expect("the argument is still a string")
            .as_bytes(),
        b"subr-argument",
        "a subr's string argument must survive a collection that lands inside \
         the subr: the backtrace frame is its root",
    );
    ev.unbind_to(bt_count);
}

/// The control for the test above, and the reason it does not pass for the
/// wrong reason: the SAME string and the SAME safepoint, minus only the
/// backtrace frame. Without that root the collector takes it, which is what
/// makes the paired assertion evidence about rooting rather than about
/// whether the collector runs at all.
#[test]
fn a_string_with_no_backtrace_frame_is_reclaimed_at_the_same_safepoint() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(1);
    ev.gc_collect_exact();

    // Keep one rooted string so the arena page survives and the doomed
    // string's slot storage is still mapped when we inspect it.
    let anchor = Value::heap_string(crate::heap_types::LispString::from_utf8("anchor"));
    let anchor_scope = ev.save_specpdl_roots();
    ev.push_specpdl_root(anchor);

    let unrooted = Value::heap_string(crate::heap_types::LispString::from_utf8("subr-argument"));
    let unrooted_ptr = unrooted.as_string_ptr().expect("string");

    let gc_before = ev.gc_count;
    while ev.gc_count == gc_before {
        ev.gc_safe_point_exact();
    }

    assert!(
        unsafe { (*unrooted_ptr).data.is_reclaimed() },
        "the collector must take a string held only in a Rust local: this \
         collector is precise and `set_stack_bottom` is a no-op \
         (tagged/CONCURRENT_GC.md)",
    );
    ev.restore_specpdl_roots(anchor_scope);
}

/// The tripwire itself. Before DIVERGENCES.md 163 this read freed bytes and
/// returned them, so a use-after-free surfaced (if at all) many frames later
/// in the printer or the symbol resolver — 161's whole diagnosis problem.
#[test]
#[should_panic(expected = "use-after-free")]
fn borrowing_a_reclaimed_string_aborts_at_the_scene() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(1);
    ev.gc_collect_exact();

    let anchor = Value::heap_string(crate::heap_types::LispString::from_utf8("anchor"));
    let anchor_scope = ev.save_specpdl_roots();
    ev.push_specpdl_root(anchor);

    let doomed = Value::heap_string(crate::heap_types::LispString::from_utf8("doomed"));
    let gc_before = ev.gc_count;
    while ev.gc_count == gc_before {
        ev.gc_safe_point_exact();
    }
    ev.restore_specpdl_roots(anchor_scope);

    let _ = doomed.as_lisp_string();
}

/// Dropping an evaluator must retract the thread-local allocation slot it
/// installed. A stale slot points at freed storage, and `with_tagged_heap`
/// treats "non-null" as "usable": the next allocation is a use-after-free.
#[test]
fn dropping_the_evaluator_uninstalls_its_thread_local_heap() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();
    assert!(
        crate::tagged::gc::tagged_heap_is_installed(),
        "constructing an evaluator installs its heap for allocation"
    );
    drop(ev);
    assert!(
        !crate::tagged::gc::tagged_heap_is_installed(),
        "dropping the evaluator must retract the pointer to its freed heap"
    );
}

/// A later evaluator's installation wins: retraction is by pointer identity,
/// so dropping an evaluator that is no longer the installed one is a no-op.
#[test]
fn dropping_a_displaced_evaluator_leaves_the_live_installation() {
    crate::test_utils::init_test_tracing();
    let first = Context::new();
    let _second = Context::new();
    drop(first);
    assert!(
        crate::tagged::gc::tagged_heap_is_installed(),
        "the still-live evaluator's heap must stay installed"
    );
    // Allocating and reading it back proves the installed heap is the live one.
    let v = Value::cons(Value::fixnum(1), Value::fixnum(2));
    assert_eq!(v.cons_car(), Value::fixnum(1));
}

#[test]
fn gc_stress_collects_after_allocation_not_at_unchanged_safe_points() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.gc_stress = true;
    ev.gc_collect_exact();
    ev.gc_count = 0;

    assert_eq!(ev.tagged_heap.bytes_since_gc(), 0);
    ev.gc_safe_point_exact();
    assert_eq!(
        ev.gc_count, 0,
        "gc_stress should not collect again before any new allocation"
    );

    let rooted = Value::cons(Value::fixnum(21), Value::fixnum(22));
    assert!(ev.tagged_heap.bytes_since_gc() > 0);
    let roots = ev.save_specpdl_roots();
    ev.push_specpdl_root(rooted);
    ev.gc_safe_point_exact();
    ev.restore_specpdl_roots(roots);
    assert_eq!(ev.gc_count, 1);
    assert_eq!(rooted.cons_car(), Value::fixnum(21));

    ev.gc_safe_point_exact();
    assert_eq!(
        ev.gc_count, 1,
        "a second unchanged safe point should not repeat the collection"
    );
}

#[test]
fn eval_sub_exact_gc_retains_cons_form() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(1);

    let form = Value::list(vec![
        Value::symbol("car"),
        Value::list(vec![
            Value::symbol("quote"),
            Value::cons(Value::fixnum(9), Value::fixnum(10)),
        ]),
    ]);
    let result = ev
        .eval_sub(form)
        .map_err(crate::emacs_core::error::map_flow);

    assert_eq!(format_eval_result(&result), "OK 9");
    assert!(ev.gc_count > 0, "exact eval_sub path should trigger GC");
}

#[test]
fn apply_exact_gc_retains_rooted_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.tagged_heap.set_gc_threshold(1);

    let arg = Value::cons(Value::fixnum(12), Value::fixnum(13));
    let result = ev
        .apply(Value::symbol("car"), vec![arg])
        .map_err(crate::emacs_core::error::map_flow);

    assert_eq!(format_eval_result(&result), "OK 12");
    assert!(ev.gc_count > 0, "exact apply path should trigger GC");
}

#[test]
fn gc_collect_runs_post_gc_hook() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        "(progn
           (setq gc-hook-log nil)
           (setq post-gc-hook
                 (list (lambda ()
                         (setq gc-hook-log (cons 'ran gc-hook-log)))))
           (garbage-collect)
           gc-hook-log)",
    );
    assert_eq!(result, "OK (ran)");
}

#[test]
fn gc_safe_point_runs_post_gc_hook_when_incremental_collection_finishes() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str_each(
        "(progn
           (setq gc-hook-log nil)
           (setq post-gc-hook
                 (list (lambda ()
                         (setq gc-hook-log (cons 'ran gc-hook-log))))))",
    );
    ev.tagged_heap.set_gc_threshold(5);
    ev.eval_str_each("(progn (cons 1 2) (cons 3 4) (cons 5 6) (cons 7 8) (cons 9 10) nil)");
    while ev.gc_count == 0 {
        ev.gc_safe_point();
    }
    assert!(ev.gc_count > 0);
    let hook_log = ev.obarray().symbol_value("gc-hook-log").copied();
    assert!(hook_log.is_some());
    let entries = list_to_vec(&hook_log.unwrap()).expect("gc-hook-log list");
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|entry| *entry == Value::symbol("ran")));
}

// -----------------------------------------------------------------------
// GC stress tests — force collection between every top-level form
// -----------------------------------------------------------------------

fn eval_stress(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    let forms = crate::emacs_core::value_reader::read_all(src, &test_ob()).expect("parse");
    ev.gc_stress = true;
    // Force very low threshold so gc_safe_point triggers on every call
    ev.tagged_heap.set_gc_threshold(1);
    // Root all parsed forms before the eval loop. The Vec<Value>
    // lives on the malloc heap and is invisible to conservative
    // stack scanning; without rooting, the forced low-threshold
    // GC reclaims the cons cells while we are still iterating.
    let roots = ev.save_specpdl_roots();
    for form in &forms {
        ev.push_specpdl_root(*form);
    }
    let mut results = Vec::new();
    for form in &forms {
        let r = ev.eval_form(*form);
        results.push(format_eval_result(&r));
        ev.gc_safe_point();
    }
    ev.restore_specpdl_roots(roots);
    results
}

#[test]
fn gc_stress_arithmetic() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress("(+ 1 2) (* 3 4) (- 10 5)");
    assert_eq!(r, vec!["OK 3", "OK 12", "OK 5"]);
}

#[test]
fn gc_stress_cons_operations() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress(
        "(setq x (cons 1 (cons 2 (cons 3 nil))))
         (car x)
         (car (cdr x))
         (length x)",
    );
    assert_eq!(r, vec!["OK (1 2 3)", "OK 1", "OK 2", "OK 3"]);
}

#[test]
fn gc_stress_vector_operations() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress(
        "(setq v (vector 10 20 30))
         (aref v 0)
         (aset v 1 99)
         (aref v 1)",
    );
    assert_eq!(r, vec!["OK [10 20 30]", "OK 10", "OK 99", "OK 99"]);
}

#[test]
fn gc_stress_hash_table() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress(
        "(setq ht (make-hash-table :test 'equal))
         (puthash \"a\" 1 ht)
         (puthash \"b\" 2 ht)
         (gethash \"a\" ht)
         (gethash \"b\" ht)
         (hash-table-count ht)",
    );
    assert_eq!(r[3], "OK 1");
    assert_eq!(r[4], "OK 2");
    assert_eq!(r[5], "OK 2");
}

#[test]
fn gc_stress_closures() {
    crate::test_utils::init_test_tracing();
    // Test lambdas and funcall survive GC (dynamic binding).
    // Lexical capture across separate top-level forms is a
    // pre-existing limitation unrelated to GC.
    let r = eval_stress(
        "(defalias 'my-add #'(lambda (a b) (+ a b)))
         (setq f (lambda (x) (my-add x 10)))
         (funcall f 5)
         (funcall f 20)",
    );
    assert_eq!(r[2], "OK 15");
    assert_eq!(r[3], "OK 30");
}

#[test]
fn gc_stress_lambda_argument_closure_survives_binding_installation() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((payload (list 1 2 3)))
             ((lambda (orig)
                (funcall orig))
              (lambda () payload)))"#,
    ));
    assert_eq!(result, "OK (1 2 3)");
}

#[test]
fn gc_stress_direct_lambda_head_roots_fresh_closure_during_arg_eval() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"((lambda (f value)
              (funcall f value))
            (lambda (x) x)
            (prog1 (list 1 2 3)
              (list 4 5 6)
              (list 7 8 9)))"#,
    ));
    assert_eq!(result, "OK (1 2 3)");
}

#[test]
fn gc_stress_builtin_apply_roots_closure_function_argument() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((payload (list 7 8 9)))
             (let ((f (lambda () payload)))
               (apply f nil)))"#,
    ));
    assert_eq!(result, "OK (7 8 9)");
}

#[test]
fn gc_stress_macro_expansion_result_stays_rooted_for_eval() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.gc_stress = true;
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"(progn
             (defalias 'vm-gc-expand-put
               (cons 'macro
                     #'(lambda ()
                         (list 'put ''vm-gc-expand-target ''custom-version "29.1"))))
             (vm-gc-expand-put)
             (get 'vm-gc-expand-target 'custom-version))"#,
    ));
    assert_eq!(result, "OK \"29.1\"");
}

#[test]
fn gc_stress_closure_call_restores_outer_lexenv_after_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((warnings nil))
             (let ((warn (lambda (form)
                           (setq warnings (cons form warnings)))))
               (funcall warn 'a)
               warnings))"#,
    ));
    assert_eq!(result, "OK (a)");
}

#[test]
fn gc_stress_let_star_lexical_binding_roots_evaluated_values() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((build (lambda () (list 4 5 6))))
             (let* ((x (funcall build))
                    (y x))
               y))"#,
    ));
    assert_eq!(result, "OK (4 5 6)");
}

#[test]
fn gc_stress_prog1_roots_first_value() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress("(prog1 (list 1 2 3) (list 4 5 6) (list 7 8 9))");
    assert_eq!(r[0], "OK (1 2 3)");
}

#[test]
fn gc_stress_apply_env_expander_closure_capturing_uninterned_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.lexenv = Value::list(vec![Value::T]);
    ev.gc_stress = true;
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"
        (let ((newenv nil)
              (magic (make-symbol "vm-magic")))
          (let ((var (make-symbol "vm-var")))
            (setq newenv
                  (cons
                   (cons 'vm-head
                         (lambda (&rest args)
                           (if (eq (car args) magic)
                               (list magic var)
                             (cons 'funcall (cons var args)))))
                   newenv))
            (let* ((form '(vm-head 1 2 3))
                   (head (car form))
                   (env-expander (assq head newenv)))
              (apply (cdr env-expander) (cdr form)))))
        "#,
    ));
    assert_eq!(result, "OK (funcall vm-var 1 2 3)");
}

#[test]
fn interpreted_closure_while_can_advance_lexical_loop_variable() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let result = format_eval_result(&ev.eval_str(
        r#"
        (funcall
         (let ((items '(a b c)))
           (lambda ()
             (let ((l items)
                   (count 0))
               (while l
                 (setq l (cdr l))
                 (setq count (1+ count)))
               count))))
        "#,
    ));
    assert_eq!(result, "OK 3");
}

#[test]
fn interpreted_closure_trim_cache_survives_exact_gc() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);

    ev.eval_str(
        r#"
        (setq vm-interpreted-closure-count 0)
        (fset 'cconv-make-interpreted-closure
              (lambda (args body env docstring iform)
                (setq vm-interpreted-closure-count
                      (1+ vm-interpreted-closure-count))
                (make-interpreted-closure args body env docstring iform)))
        (setq internal-make-interpreted-closure-function
              'cconv-make-interpreted-closure)
        "#,
    )
    .expect("eval forms");

    let filter_fn = ev
        .obarray()
        .symbol_function("cconv-make-interpreted-closure")
        .expect("cconv interpreted closure filter");
    ev.set_interpreted_closure_filter_fn(Some(filter_fn));

    let first = format_eval_result(&ev.eval_str("(funcall (let ((x 1)) (lambda () x)))"));
    assert_eq!(first, "OK 1");

    ev.gc_collect_exact();

    let count = format_eval_result(&ev.eval_str("vm-interpreted-closure-count"));
    assert_eq!(count, "OK 1");
}

#[test]
fn raw_quoted_lambda_uses_nil_lexenv_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);

    let rendered = format_eval_result(&ev.eval_str(
        r#"(let ((x 1))
             (list (funcall '(lambda () x))
                   (funcall '(lambda () x))))"#,
    ));
    assert_eq!(rendered, "ERR (void-variable (x))");
}

#[test]
fn gc_stress_aref_on_closure_survives_closure_vector_conversion() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((payload (list 1 2 3)))
             (let ((closure (lambda () payload)))
               (if (aref closure 2) t)))"#,
    ));
    assert_eq!(result, "OK t");
}

#[test]
fn gc_stress_cdr_rejects_closure_without_losing_capture() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    ev.gc_stress = true;
    ev.tagged_heap.set_gc_threshold(1);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((payload (list 1 2 3)))
             (let ((closure (lambda () payload)))
               (list
                (condition-case error-data
                    (cdr closure)
                  (error (car error-data)))
                (funcall closure))))"#,
    ));
    assert_eq!(result, "OK (wrong-type-argument (1 2 3))");
}

#[test]
fn gc_stress_recursive_function() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress(
        "(defalias 'my-length #'(lambda (lst)
           (if (null lst) 0
             (1+ (my-length (cdr lst))))))
         (my-length '(a b c d e))
         (my-length nil)",
    );
    assert_eq!(r[1], "OK 5");
    assert_eq!(r[2], "OK 0");
}

#[test]
fn gc_stress_setcar_setcdr() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress(
        "(setq x (cons 1 2))
         (setcar x 10)
         (setcdr x 20)
         x",
    );
    assert_eq!(r[3], "OK (10 . 20)");
}

#[test]
fn gc_stress_let_bindings() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress(
        "(let ((a (cons 1 2))
               (b (cons 3 4)))
           (cons (car a) (car b)))",
    );
    assert_eq!(r[0], "OK (1 . 3)");
}

#[test]
fn gc_stress_mapcar() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress("(mapcar '1+ '(1 2 3 4 5))");
    assert_eq!(r[0], "OK (2 3 4 5 6)");
}

#[test]
fn gc_stress_string_operations() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress(
        r#"(setq s (concat "hello" " " "world"))
           (length s)
           (substring s 0 5)"#,
    );
    assert_eq!(r[0], r#"OK "hello world""#);
    assert_eq!(r[1], "OK 11");
    assert_eq!(r[2], r#"OK "hello""#);
}

#[test]
fn gc_stress_nreverse() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress(
        "(setq x (list 1 2 3 4 5))
         (setq y (nreverse x))
         y",
    );
    assert_eq!(r[2], "OK (5 4 3 2 1)");
}

#[test]
fn gc_stress_plist() {
    crate::test_utils::init_test_tracing();
    let r = eval_stress(
        "(setq pl '(a 1 b 2 c 3))
         (plist-get pl 'b)
         (setq pl (plist-put pl 'b 99))
         (plist-get pl 'b)",
    );
    assert_eq!(r[1], "OK 2");
    assert_eq!(r[3], "OK 99");
}

#[test]
fn gc_stress_circular_list_survives() {
    crate::test_utils::init_test_tracing();
    // Create circular list inside a single progn to avoid formatting
    // the circular cons (which would hang the Display impl).
    let r = eval_stress(
        "(progn
           (setq x (cons 42 nil))
           (setcdr x x)
           (car x))",
    );
    assert_eq!(r[0], "OK 42");
}

#[test]
fn gc_stress_many_allocations() {
    crate::test_utils::init_test_tracing();
    // Allocate many short-lived conses; only final result should survive
    // dotimes is no longer a special form; use let+while equivalent
    let r = eval_stress(
        "(let ((result nil) (i 0))
           (while (< i 100)
             (setq result (cons i result))
             (setq i (1+ i)))
           (length result))",
    );
    assert_eq!(r[0], "OK 100");
}

// -----------------------------------------------------------------------
// Lexical closure mutation visibility tests
// -----------------------------------------------------------------------

#[test]
fn lexical_closure_mutation_visible() {
    crate::test_utils::init_test_tracing();
    // Closures must share the same lexical frame — mutations through
    // one closure must be visible to the outer scope.
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((x 0))
             (let ((f (lambda () (setq x (1+ x)))))
               (funcall f)
               (funcall f)
               x))"#,
    ));
    assert_eq!(result, "OK 2");
}

#[test]
fn lexical_closure_shared_state() {
    crate::test_utils::init_test_tracing();
    // Two closures sharing the same binding (inc + get).
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((x 0))
             (let ((inc (lambda () (setq x (1+ x))))
                   (get (lambda () x)))
               (funcall inc)
               (funcall inc)
               (funcall inc)
               (funcall get)))"#,
    ));
    assert_eq!(result, "OK 3");
}

#[test]
fn lexical_closure_make_counter() {
    crate::test_utils::init_test_tracing();
    // Classic make-counter pattern with independent counters.
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let result = format_eval_result(&ev.eval_str(
        r#"(progn
             (defalias 'make-counter #'(lambda ()
               (let ((n 0))
                 (lambda () (setq n (1+ n))))))
             (let ((c1 (make-counter))
                   (c2 (make-counter)))
               (funcall c1)
               (funcall c1)
               (funcall c1)
               (let ((r1 (funcall c1))
                     (r2 (funcall c2)))
                 (list r1 r2))))"#,
    ));
    // c1 called 4 times → 4; c2 called once → 1; independent counters
    assert_eq!(result, "OK (4 1)");
}

#[test]
fn lexical_closure_outer_mutation_visible() {
    crate::test_utils::init_test_tracing();
    // Outer setq visible to closure.
    let mut ev = Context::new();
    ev.set_lexical_binding(true);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((x 10))
             (let ((f (lambda () x)))
               (setq x 42)
               (funcall f)))"#,
    ));
    assert_eq!(result, "OK 42");
}

#[test]
fn closure_inside_mapcar_lambda_captures_outer_param() {
    crate::test_utils::init_test_tracing();
    // Reproduces the pcase-compile-patterns pattern:
    // (mapcar (lambda (case)
    //           (list case
    //                 (lambda (vars) case)))
    //         '(a b c))
    // Each inner lambda should capture `case` from the outer lambda.
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.set_lexical_binding(true);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((closures
                 (mapcar (lambda (case)
                           (lambda () case))
                         '(a b c))))
             (list (funcall (car closures))
                   (funcall (car (cdr closures)))
                   (funcall (car (cdr (cdr closures))))))"#,
    ));
    assert_eq!(result, "OK (a b c)");
}

#[test]
fn closure_inside_backquote_mapcar_captures_outer_param() {
    crate::test_utils::init_test_tracing();
    // More closely matches pcase-compile-patterns:
    // The inner lambda is created inside a backquote, after a function call.
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.set_lexical_binding(true);
    let result = format_eval_result(&ev.eval_str(
        r#"(let ((closures
                 (mapcar (lambda (case)
                           (list (car case)
                                 (lambda (vars)
                                   (list case vars))))
                         '((a 1) (b 2) (c 3)))))
             (let ((fn2 (car (cdr (car closures)))))
               (funcall fn2 42)))"#,
    ));
    assert_eq!(result, "OK ((a 1) 42)");
}

#[test]
fn closure_inside_real_backquote_with_fn_call_captures_outer_param() {
    crate::test_utils::init_test_tracing();
    // Replicates the exact pcase-compile-patterns pattern:
    // (mapcar (lambda (case)
    //           `(,(some-fn val (car case))
    //             ,(lambda (vars) (list case vars))))
    //         cases)
    // The inner lambda is inside a REAL backquote (macro), after a function call.
    // This requires loading backquote.el.
    let mut eval = crate::test_utils::runtime_startup_context();
    load_minimal_gnu_backquote_runtime(&mut eval);

    let result = format_eval_result(&eval.eval_str(
        r#"(progn
             (defalias 'my-match #'(lambda (val upat) (list val upat)))
             (let ((closures
                    (mapcar (lambda (case)
                              `(,(my-match 'x (car case))
                                ,(lambda (vars) (list case vars))))
                            '((a 1) (b 2)))))
               (let ((fn1 (car (cdr (car closures)))))
                 (funcall fn1 'matched))))"#,
    ));
    assert_eq!(result, "OK ((a 1) matched)");
}

#[test]
fn real_backquote_computed_symbols_match_runtime_macro_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);

    let result = format_eval_result(&eval.eval_str(
        r#"(let ((prefix "neovm-bqc-test")
                 (suffixes '("x" "y" "z")))
             (let ((forms
                    (let ((i 0))
                      (mapcar (lambda (s)
                                (setq i (1+ i))
                                `(list ',(intern (concat prefix "-" s)) ,i))
                              suffixes))))
               (mapcar #'eval forms)))"#,
    ));
    assert_eq!(
        result,
        "OK ((neovm-bqc-test-x 1) (neovm-bqc-test-y 2) (neovm-bqc-test-z 3))"
    );
}

#[test]
fn real_backquote_macroexpand_preserves_debug_head_before_splice() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_all_with_subr(
            "(progn
               (fset 'neovm--debug-head
                     (cons 'macro
                           (lambda (condition)
                             `((debug ,@(if (listp condition)
                                            condition
                                          (list condition)))))))
               (macroexpand '(neovm--debug-head error)))"
        )[0],
        "OK ((debug error))"
    );
}

#[test]
fn loaded_subr_condition_case_unless_debug_calls_debugger_before_handler() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(progn
           (setq neovm-debugger-called nil)
           (let ((debug-on-error t)
               (debugger (lambda (&rest args)
                           (setq neovm-debugger-called args))))
             (list (condition-case-unless-debug nil
                       (signal 'error 1)
                     (error 'handled))
                   neovm-debugger-called)))"#
        )),
        "OK (handled (error (error . 1)))"
    );
}

#[test]
fn loaded_subr_condition_case_unless_debug_macroexpand_includes_debug_marker() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(equal
            (macroexpand '(condition-case-unless-debug nil
                            (signal 'error 1)
                            (error 42)))
            '(condition-case nil
               (signal 'error 1)
               ((debug error) 42)))"#
        )),
        "OK t"
    );
}

#[test]
fn macroexpand_environment_shadows_alias_targets_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_all(
            "(let* ((alias-target (make-symbol \"ma-target\"))
                    (alias-head (make-symbol \"ma-head\")))
               (fset alias-target (cons 'macro (lambda (x) (list 'global x))))
               (fset alias-head alias-target)
               (macroexpand (list alias-head 42)
                            (list (cons alias-target
                                        (lambda (x) (list 'env x))))))"
        )[0],
        "OK (env 42)"
    );
}

#[test]
fn lexical_condition_case_debug_marker_calls_debugger_before_handler() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_lexical_binding(true);

    assert_eq!(
        format_eval_result(&eval.eval_str(
            r#"(progn
           (setq neovm-debugger-called nil)
           (let ((debug-on-error t)
               (debugger (lambda (&rest args)
                           (setq neovm-debugger-called args))))
             (list (condition-case nil
                       (signal 'error 1)
                     ((debug error) 'handled))
                   neovm-debugger-called)))"#
        )),
        "OK (handled (error (error . 1)))"
    );
}

#[test]
fn real_backquote_nested_eval_chain_matches_gnu_error_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_backquote_runtime(&mut eval);

    let result = format_eval_result(&eval.eval_str(
        r#"(let ((x 10))
             (let ((template `(let ((y ,,x)) `(+ ,y ,,x))))
               (list template
                     (condition-case e (eval template) (error (cons 'ERR e)))
                     (condition-case e (eval (eval template)) (error (cons 'ERR e))))))"#,
    ));
    assert_eq!(result, r#"ERR (void-function (\,))"#);
}

#[test]
fn condition_case_lexical_handler_binding_restores_outer_let() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_lexical_binding(true);

    let result = format_eval_result(&eval.eval_str(
        r#"(let ((outer 'original))
             (list
              (condition-case outer
                  (/ 1 0)
                (arith-error
                 (setq outer (list 'caught (car outer)))
                 outer))
              outer))"#,
    ));
    assert_eq!(result, "OK ((caught arith-error) original)");
}

#[test]
fn gc_stress_lexical_closure_mutation() {
    crate::test_utils::init_test_tracing();
    // GC stress variant of closure mutation.
    let r = eval_stress(
        "(let ((x 0))
           (let ((f (lambda () (setq x (1+ x)))))
             (funcall f)
             (funcall f)
             (funcall f)
             x))",
    );
    assert_eq!(r[0], "OK 3");
}

#[test]
fn evaluator_face_table_has_standard_faces() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();
    let ft = ev.face_table();

    // Standard faces must exist
    assert!(ft.get("default").is_some(), "missing default face");
    assert!(ft.get("bold").is_some(), "missing bold face");
    assert!(ft.get("italic").is_some(), "missing italic face");
    assert!(ft.get("mode-line").is_some(), "missing mode-line face");
    assert!(
        ft.get("minibuffer-prompt").is_some(),
        "missing minibuffer-prompt face"
    );

    // GNU keeps `bold' foreground unspecified; it supplies only weight.
    let bold = ft.resolve("bold");
    assert!(
        bold.foreground.is_none(),
        "bold foreground should remain unspecified"
    );
    assert!(
        bold.weight.map_or(false, |w| w.is_bold()),
        "bold face should have bold weight",
    );
}

#[test]
fn advice_around_compiler_macro_pattern() {
    crate::test_utils::init_test_tracing();
    // Reproduce the cl-macs pattern: macroexp--compiler-macro calls a
    // compiler-macro handler. condition-case-unless-debug should catch
    // wrong-number-of-arguments errors.
    let results = eval_all(
        r#"
        ;; Simulate a compiler-macro handler that needs 2 args
        (defalias 'my-cmacro-handler #'(lambda (form arg)
          (list 'optimized form arg)))

        ;; But it gets called with wrong arity via apply
        (condition-case err
            (apply 'my-cmacro-handler '((my-fn 1 2) 1 2))
          (wrong-number-of-arguments
           (list 'caught-wna err)))
        "#,
    );
    for (i, r) in results.iter().enumerate() {
        eprintln!("cmacro[{i}]: {r}");
    }
}

#[test]
fn oclosure_define_basic() {
    crate::test_utils::init_test_tracing();
    // Test basic oclosure-define usage - the pattern that fails in loadup
    let results = eval_all(
        r#"
        ;; oclosure-define should create a type
        (condition-case err
            (oclosure-define my-test-ocl "A test oclosure type.")
          (error (list 'error err)))
        ;; Check if it worked
        (condition-case err
            (oclosure-define my-test-ocl2 "Another test." (slot1))
          (error (list 'error err)))
        "#,
    );
    for (i, r) in results.iter().enumerate() {
        eprintln!("oclosure-define[{i}]: {r}");
    }
}

#[test]
fn oclosure_define_macroexpand() {
    crate::test_utils::init_test_tracing();
    // Trace what oclosure-define expands to
    let results = eval_all(
        r#"
        ;; Check if oclosure-define is a macro
        (fboundp 'oclosure-define)
        (macroexpand-1 '(oclosure-define my-test-ocl "Test type."))
        (macroexpand-1 '(oclosure-define my-test-ocl2 "Test2." (slot1)))
        "#,
    );
    for (i, r) in results.iter().enumerate() {
        eprintln!("macroexpand-ocl[{i}]: {r}");
    }
}

#[test]
fn cl_defstruct_keyword_handling() {
    crate::test_utils::init_test_tracing();
    // Test cl-defstruct with :copier/:constructor keywords
    // These fail with (invalid-function :copier) in loadup
    let results = eval_all(
        r#"
        ;; Check if cl-defstruct is a macro
        (fboundp 'cl-defstruct)
        (condition-case err
            (macroexpand '(cl-defstruct (my-test-struct (:copier nil)) field1 field2))
          (error (list 'macroexpand-error err)))
        (condition-case err
            (cl-defstruct (my-test-struct (:copier nil)) field1 field2)
          (error (list 'error err)))
        "#,
    );
    for (i, r) in results.iter().enumerate() {
        eprintln!("cl-defstruct[{i}]: {r}");
    }
}

#[test]
fn cl_deftype_basic() {
    crate::test_utils::init_test_tracing();
    // Test cl-deftype which fails in ring.el with (void-variable ring)
    let results = eval_all(
        r#"
        (condition-case err
            (cl-deftype my-ring-test nil '(satisfies ring-p))
          (error (list 'error err)))
        "#,
    );
    for (i, r) in results.iter().enumerate() {
        eprintln!("cl-deftype[{i}]: {r}");
    }
}

#[test]
fn bootstrap_window_system_modes_match_gnu_defaults() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    assert_eq!(
        eval.obarray().symbol_value("menu-bar-mode"),
        Some(&Value::T),
        "GNU initializes menu-bar-mode to t in frame.c"
    );
    assert_eq!(
        eval.obarray().symbol_value("tool-bar-mode"),
        Some(&Value::T),
        "GNU initializes tool-bar-mode to t for window-system builds"
    );
}

/// GNU `keyboard.c:adjust_point_for_property` — after a command moves point
/// into an `invisible' text run, the command loop relocates point to a
/// boundary so the cursor never rests inside hidden text.  Without this,
/// motion commands (evil `e`) that land inside org's invisible link-target
/// text leave the cursor parked where the display collapses the run to a
/// single column, so it appears frozen.
///
/// Verified against GNU Emacs 31.0.50: in buffer "abcXXXXdef" with chars 4..7
/// invisible, `(goto-char 6)` followed by the command loop leaves point at 4.
#[test]
fn adjust_point_for_property_relocates_out_of_invisible_run() {
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().unwrap();
        buf.insert("abcXXXXdef");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    ev.eval_str("(put-text-property 4 8 'invisible t)").unwrap();
    ev.eval_str("(setq buffer-invisibility-spec t)").unwrap();
    ev.eval_str("(goto-char 6)").unwrap();
    assert_eq!(
        ev.eval_str("(point)").unwrap().as_fixnum(),
        Some(6),
        "precondition: point is inside the invisible run"
    );

    ev.adjust_point_for_property(1, false).unwrap();

    assert_eq!(
        ev.eval_str("(point)").unwrap().as_fixnum(),
        Some(4),
        "point relocated to the invisible run boundary (matches GNU 31.0.50)"
    );
}

/// The adjustment must be a no-op when point is not inside invisible text.
#[test]
fn adjust_point_for_property_noop_in_visible_text() {
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().unwrap();
        buf.insert("abcXXXXdef");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    ev.eval_str("(put-text-property 4 8 'invisible t)").unwrap();
    ev.eval_str("(setq buffer-invisibility-spec t)").unwrap();
    ev.eval_str("(goto-char 2)").unwrap();

    ev.adjust_point_for_property(1, false).unwrap();

    assert_eq!(
        ev.eval_str("(point)").unwrap().as_fixnum(),
        Some(2),
        "point in visible text is left untouched"
    );
}

/// GNU's `adjust_point_for_property` also relocates point out of a
/// `display`-intangible run (a replacing display string), not just invisible
/// text. Ground truth from GNU 31.0.50 (via execute-kbd-macro through the
/// command loop): `display "=>"` on [4,8), a command lands point at 6, and
/// moving FORWARD (last_pt=1) relocates point to the run END (8).
#[test]
fn adjust_point_for_property_relocates_out_of_display_intangible_run_forward() {
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().unwrap();
        buf.insert("abcXXXXdef");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    ev.eval_str("(put-text-property 4 8 'display \"=>\")")
        .unwrap();
    ev.eval_str("(goto-char 6)").unwrap();
    assert_eq!(
        ev.eval_str("(point)").unwrap().as_fixnum(),
        Some(6),
        "precondition: point is inside the display-intangible run"
    );

    ev.adjust_point_for_property(1, false).unwrap();

    assert_eq!(
        ev.eval_str("(point)").unwrap().as_fixnum(),
        Some(8),
        "forward into display-intangible run relocates to its end (GNU 31.0.50)"
    );
}

/// Backward motion (last_pt=10) into the same `display`-intangible run
/// relocates point to the run START (4). GNU 31.0.50 ground truth.
#[test]
fn adjust_point_for_property_relocates_out_of_display_intangible_run_backward() {
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().unwrap();
        buf.insert("abcXXXXdef");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    ev.eval_str("(put-text-property 4 8 'display \"=>\")")
        .unwrap();
    ev.eval_str("(goto-char 6)").unwrap();

    ev.adjust_point_for_property(10, false).unwrap();

    assert_eq!(
        ev.eval_str("(point)").unwrap().as_fixnum(),
        Some(4),
        "backward into display-intangible run relocates to its start (GNU 31.0.50)"
    );
}

#[test]
fn set_current_message_mirrors_into_echo_area_buffer() {
    // GNU set_message_1: the echo message is written into the ` *Echo Area 0*`
    // buffer so redisplay can render it as ordinary buffer text. Slice 8 step 1
    // stages this additively; the layout still renders from current_message.
    let mut ev = Context::new();
    // The echo-area buffers are materialized by the message-display setup, not by
    // set_current_message (GNU set_message_1 assumes they exist).
    ev.ensure_echo_area_buffers();
    let message = crate::emacs_core::builtins::plain_str_to_lisp_string("hello echo", true);
    ev.set_current_message(Some(message));

    let id = ev
        .buffers
        .find_buffer_by_name(" *Echo Area 0*")
        .expect("echo-area buffer should exist");
    assert_eq!(
        ev.buffers
            .get(id)
            .expect("echo buffer live")
            .full_text_string(),
        "hello echo",
        "echo buffer mirrors the message text"
    );

    ev.set_current_message(None);
    assert_eq!(
        ev.buffers
            .get(id)
            .expect("echo buffer live")
            .full_text_string(),
        "",
        "echo buffer is cleared when the message is cleared"
    );
}

#[test]
fn set_current_message_handles_back_to_back_messages_of_differing_multibyteness() {
    // Regression: the echo-area mirror must CLEAR the echo buffer before toggling
    // its multibyteness (GNU `with_echo_area_buffer` order). Showing a multibyte
    // message and then a unibyte one — exactly what byte-compile does, emitting
    // curly-quote (multibyte) warnings followed by ASCII progress — used to panic
    // "buffer text edit position underflow": the flag was flipped while the buffer
    // still held the previous message, so the full-range delete in
    // `replace_buffer_contents_lisp_string` ran against content encoded in the
    // other multibyteness and miscomputed its marker/position adjustment.
    let mut ev = Context::new();
    ev.ensure_echo_area_buffers();

    let multibyte = crate::emacs_core::builtins::plain_str_to_lisp_string("中文", true);
    ev.set_current_message(Some(multibyte));

    // Unibyte message while the echo buffer still holds the multibyte one.
    let unibyte = crate::emacs_core::builtins::plain_str_to_lisp_string("x", false);
    ev.set_current_message(Some(unibyte));

    let id = ev
        .buffers
        .find_buffer_by_name(" *Echo Area 0*")
        .expect("echo-area buffer should exist");
    assert_eq!(
        ev.buffers
            .get(id)
            .expect("echo buffer live")
            .full_text_string(),
        "x",
        "echo buffer holds the latest (unibyte) message after a multibyte one"
    );

    // And the other direction: unibyte buffer, then a multibyte message.
    let multibyte2 = crate::emacs_core::builtins::plain_str_to_lisp_string("日本語", true);
    ev.set_current_message(Some(multibyte2));
    assert_eq!(
        ev.buffers
            .get(id)
            .expect("echo buffer live")
            .full_text_string(),
        "日本語",
        "echo buffer holds the latest (multibyte) message after a unibyte one"
    );
}

/// Helper: a runtime-startup context wired with a live scratch buffer,
/// selected frame and window so `command_loop_1` has somewhere to run.
fn command_loop_test_context() -> Context {
    let mut ev = runtime_startup_context();
    let scratch = ev.buffers.create_buffer("*command-loop-finding*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    assert!(
        ev.frames.select_frame(frame),
        "command loop test should have a selected frame"
    );
    ev
}

/// GNU `command_loop_2' catches an unhandled command signal with `cmd_error',
/// which delegates to the current buffer's `command-error-function'.  This is
/// load-bearing in an active minibuffer: `minibuffer-error-initialize' installs
/// a buffer-local handler that renders the diagnostic as an EOB overlay instead
/// of replacing the prompt in the echo area.
#[test]
fn command_loop_error_uses_buffer_local_command_error_function() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*command-error-function*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    assert!(ev.frames.select_frame(frame), "test needs a selected frame");
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    install_global_map_for_test(&mut ev, global_map);

    fn stop_command_loop_after_error_probe(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "stop helper should not receive arguments");
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-command-loop-after-error-probe",
        NativeFn::ContextVec(stop_command_loop_after_error_probe),
        SubrArity::new(0, Some(0)),
    ));
    ev.eval_str(
        r#"(progn
             (setq neo-command-error-observation nil)
             (set (make-local-variable 'command-error-function)
                  (lambda (data context caller)
                    (setq neo-command-error-observation
                          (list data context caller))))
             (fset 'neo-signaling-command
                   (lambda () (interactive) (signal 'error '("boom"))))
             (fset 'neo-stop-after-error-command
                   (lambda ()
                     (interactive)
                     (neo-stop-command-loop-after-error-probe)))
             (fset 'command-execute
                   (lambda (cmd &optional _record _keys _special)
                     (funcall cmd))))"#,
    )
    .expect("install command-error dispatch probe");

    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::symbol("f9")],
        Value::symbol("neo-signaling-command"),
    )
    .expect("define signaling command");
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum('q' as i64)],
        Value::symbol("neo-stop-after-error-command"),
    )
    .expect("define stop command");

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::symbol("f9"));
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('q' as i64));
    ev.command_loop.running = true;

    ev.recursive_edit_inner()
        .expect("command loop should recover and run the stop command");

    assert_eq!(
        ev.eval_symbol("neo-command-error-observation")
            .expect("command error observation"),
        Value::list(vec![
            Value::list(vec![Value::symbol("error"), Value::string("boom")]),
            Value::string(""),
            Value::NIL,
        ]),
        "GNU cmd_error must dispatch through the current buffer's handler"
    );
}

/// One completed command is the public observability seam: input waiting has
/// already ended, and the command loop owns every phase through finalization.
/// A user can therefore correlate the start/complete records without mistaking
/// time spent entering a multi-key sequence for command execution time.
#[tracing_test::traced_test]
#[test]
fn command_loop_emits_structured_timing_for_one_user_command() {
    let mut ev = command_loop_test_context();

    fn stop_observed_command(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "stop helper should not receive arguments");
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-observed-command-loop",
        NativeFn::ContextVec(stop_observed_command),
        SubrArity::new(0, Some(0)),
    ));
    ev.eval_str(
        r#"(progn
             (fset 'neo-observed-command
                   (lambda ()
                     (interactive)
                     (neo-stop-observed-command-loop)))
             (keymap-set global-map "q" 'neo-observed-command))"#,
    )
    .expect("install observed command");
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('q' as i64));
    ev.command_loop.running = true;

    ev.recursive_edit_inner()
        .expect("observed command should stop the command loop cleanly");

    assert!(logs_contain("user_command_start"));
    assert!(logs_contain("user_command_complete"));
    assert!(logs_contain("keys=q"));
    assert!(logs_contain("command=neo-observed-command"));
    assert!(logs_contain("depth=1"));
    assert!(logs_contain("outcome=completed"));
    assert!(logs_contain("total_wall_us="));
    assert!(logs_contain("command_execute_wall_us="));
}

/// GNU's command-input auto-save gate invokes `do-auto-save` after more than
/// `max (auto-save-interval, 20)` non-macro input events.  This exercises the
/// complete command-loop boundary rather than calling `do-auto-save`
/// directly, and observes the public `auto-save-hook` contract.
#[test]
fn command_loop_input_interval_triggers_auto_save_hook() {
    crate::test_utils::init_test_tracing();
    let mut ev = command_loop_test_context();

    fn stop_command_loop_after_auto_save_probe(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "stop helper should not receive arguments");
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-command-loop-after-auto-save-probe",
        NativeFn::ContextVec(stop_command_loop_after_auto_save_probe),
        SubrArity::new(0, Some(0)),
    ));
    ev.eval_str(
        r#"(progn
             (setq neo-auto-save-hook-count 0)
             (setq auto-save-interval 1)
             (setq auto-save-hook
                   (list (lambda ()
                           (setq neo-auto-save-hook-count
                                 (1+ neo-auto-save-hook-count)))))
             (fset 'neo-stop-auto-save-probe-command
                   (lambda ()
                     (interactive)
                     (neo-stop-command-loop-after-auto-save-probe)))
             (keymap-set global-map "q" 'neo-stop-auto-save-probe-command))"#,
    )
    .expect("install auto-save command-loop probe");

    for _ in 0..21 {
        ev.command_loop
            .keyboard
            .kboard
            .unread_events
            .push_back(Value::fixnum('a' as i64));
    }
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('q' as i64));
    ev.command_loop.running = true;

    ev.recursive_edit_inner()
        .expect("command loop should exit through the stop command");

    assert_eq!(
        ev.eval_symbol("neo-auto-save-hook-count")
            .expect("auto-save hook count"),
        Value::fixnum(1),
        "the input-event threshold must trigger exactly one auto-save pass"
    );
}

/// GNU also invokes `do-auto-save` when command input has modified state and
/// Emacs subsequently remains idle for `auto-save-timeout` seconds.  The
/// timeout is independent of `auto-save-interval`; disabling the event-count
/// trigger must not disable the idle trigger.
#[test]
fn command_loop_idle_timeout_triggers_auto_save_hook() {
    crate::test_utils::init_test_tracing();
    let mut ev = command_loop_test_context();

    fn stop_command_loop_after_idle_auto_save_probe(
        ctx: &mut Context,
        args: Vec<Value>,
    ) -> EvalResult {
        assert!(args.is_empty(), "stop helper should not receive arguments");
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-command-loop-after-idle-auto-save-probe",
        NativeFn::ContextVec(stop_command_loop_after_idle_auto_save_probe),
        SubrArity::new(0, Some(0)),
    ));
    ev.eval_str(
        r#"(progn
             ;; Keep the probe buffer out of the auto-save file writer; this
             ;; test observes the hook boundary, not filesystem mechanics.
             (rename-buffer " command-loop-idle-auto-save")
             (setq neo-idle-auto-save-hook-count 0)
             (setq auto-save-interval 0)
             (setq auto-save-timeout 1)
             (setq auto-save-hook
                   (list (lambda ()
                           (setq neo-idle-auto-save-hook-count
                                 (1+ neo-idle-auto-save-hook-count)))))
             (fset 'neo-stop-idle-auto-save-probe-command
                   (lambda ()
                     (interactive)
                     (neo-stop-command-loop-after-idle-auto-save-probe)))
             (keymap-set global-map "q"
                         'neo-stop-idle-auto-save-probe-command))"#,
    )
    .expect("install idle auto-save command-loop probe");

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let _tx_keepalive = tx.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('a'),
        ))
        .expect("send editing keypress");
        // Bound the test even while the idle auto-save implementation is
        // absent: this real command stops the loop after the GNU deadline.
        thread::sleep(Duration::from_millis(1_950));
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char('q'),
        ))
        .expect("send stop keypress");
    });
    ev.command_loop.running = true;

    ev.recursive_edit_inner()
        .expect("command loop should exit through the stop command");

    assert_eq!(
        ev.eval_symbol("neo-idle-auto-save-hook-count")
            .expect("idle auto-save hook count"),
        Value::fixnum(1),
        "one second of post-command idleness must trigger one auto-save pass"
    );
}

/// Finding 1 — pressing a truly-unbound key must run the per-command
/// finalize tail like GNU `command_loop_1` (keyboard.c:1506-1648): it
/// sets `this-command`/`real-this-command` to nil, runs
/// `pre-command-hook`, invokes the `undefined` command
/// (`call0 (Qundefined)` at keyboard.c:1514 — dings and echoes
/// "<key> is undefined"), then runs `post-command-hook`.
///
/// Before the fix neomacs short-circuited the nil-binding case with a
/// bare `continue`, so the message never appeared and per-command hooks
/// were skipped. This test drives one unbound `<f9>` keypress through
/// the loop and asserts both effects.
#[test]
fn unbound_key_runs_undefined_command_and_per_command_hooks() {
    crate::test_utils::init_test_tracing();
    let mut ev = command_loop_test_context();

    // A Rust subr the post-command-hook can call to (a) count and (b)
    // stop the loop after the first iteration, so the test terminates.
    fn stop_command_loop_for_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "stop helper should not receive arguments");
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-command-loop-for-test",
        NativeFn::ContextVec(stop_command_loop_for_test),
        SubrArity::new(0, Some(0)),
    ));

    // The command loop runs `post-command-hook` once in its entry
    // prologue before reading any key (GNU keyboard.c does the same).
    // Gate the counters on `(eq last-command-event 'f9)` so they only
    // fire for the iteration that actually processed the unbound key,
    // not for that prologue run.
    //
    // We queue a second, *bound* key (`a` -> a stop command) after the
    // unbound `<f9>`. That guarantees the loop terminates whether or not
    // the unbound key runs the per-command hooks: in the buggy state the
    // `<f9>` iteration short-circuits (counters stay 0) and the `a`
    // command stops the loop; in the fixed state the `<f9>` iteration
    // runs the hooks (counters become 1) and then `a` stops the loop.
    ev.eval_str(
        r#"(progn
             (setq neo-pre-count 0)
             (setq neo-post-count 0)
             (fset 'neo-stop-key-command
                   (lambda () (interactive) (neo-stop-command-loop-for-test)))
             (keymap-set global-map "a" 'neo-stop-key-command)
             (add-hook 'pre-command-hook
                       (lambda ()
                         (when (eq last-command-event 'f9)
                           (setq neo-pre-count (1+ neo-pre-count)))))
             (add-hook 'post-command-hook
                       (lambda ()
                         (when (eq last-command-event 'f9)
                           (setq neo-post-count (1+ neo-post-count))
                           ;; The `undefined' command logs "<key> is
                           ;; undefined" to *Messages* (GNU `message' always
                           ;; calls `message_dolog'). Capture the last line of
                           ;; *Messages* during this very iteration; reading
                           ;; the next key (`a') would clear the echo area.
                           (setq neo-undefined-message
                                 (with-current-buffer (messages-buffer)
                                   (save-excursion
                                     (goto-char (point-max))
                                     (forward-line (if (bolp) -1 0))
                                     (buffer-substring-no-properties
                                      (line-beginning-position)
                                      (line-end-position)))))))))"#,
    )
    .expect("install per-command hooks");

    // `<f9>` is unbound in the default global map.
    assert!(
        ev.eval_str("(key-binding (kbd \"<f9>\"))")
            .expect("key lookup")
            .is_nil(),
        "<f9> must be unbound for this test"
    );

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::symbol("f9"));
    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::fixnum('a' as i64));
    ev.command_loop.running = true;

    ev.recursive_edit_inner()
        .expect("command loop should exit through the stop command");

    assert_eq!(
        ev.eval_symbol("neo-pre-count").expect("pre count"),
        Value::fixnum(1),
        "pre-command-hook must run for an unbound key (GNU keyboard.c:1509)"
    );
    assert_eq!(
        ev.eval_symbol("neo-post-count").expect("post count"),
        Value::fixnum(1),
        "post-command-hook must run for an unbound key (GNU keyboard.c:1563)"
    );

    let message = ev
        .eval_symbol("neo-undefined-message")
        .expect("captured message variable");
    let message = message
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .expect("the `undefined' command should have echoed a string message");
    assert!(
        message.contains("is undefined"),
        "unbound key should echo \"... is undefined\" (GNU subr.el `undefined'), got: {message:?}"
    );
}

/// GNU `yes-or-no-p` dynamically binds `real-this-command` around its recursive
/// minibuffer (`src/fns.c:Fyes_or_no_p`).  The answer's `exit-minibuffer`
/// command therefore cannot replace the caller that the outer command loop
/// later commits to `real-last-command`.
#[test]
fn yes_or_no_p_preserves_callers_real_this_command() {
    crate::test_utils::init_test_tracing();
    let mut ev = command_loop_test_context();

    fn stop_command_loop_for_yes_or_no_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "stop helper should not receive arguments");
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-command-loop-for-yes-or-no-test",
        NativeFn::ContextVec(stop_command_loop_for_yes_or_no_test),
        SubrArity::new(0, Some(0)),
    ));

    ev.eval_str(
        r#"(progn
             (setq neo-real-command-after-query nil)
             (fset 'neo-query-command
                   (lambda ()
                     (interactive)
                     (yes-or-no-p "Proceed?")
                     (setq neo-real-command-after-query real-this-command)
                     (neo-stop-command-loop-for-yes-or-no-test)))
             (keymap-set global-map "a" 'neo-query-command))"#,
    )
    .expect("install yes-or-no-p caller-preservation probe");

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let _tx_keepalive = tx;
    for event in ['a', 'y', 'e', 's', '\r'] {
        ev.command_loop
            .keyboard
            .kboard
            .unread_events
            .push_back(Value::fixnum(event as i64));
    }
    ev.command_loop.running = true;
    ev.recursive_edit_inner()
        .expect("query command should stop the command loop");

    assert_eq!(
        ev.eval_symbol("neo-real-command-after-query")
            .expect("captured real-this-command"),
        Value::symbol("neo-query-command"),
        "yes-or-no-p must restore its caller after the recursive minibuffer"
    );
}

#[test]
fn command_loop_dispatches_select_window_before_following_key() {
    crate::test_utils::init_test_tracing();
    let mut ev = command_loop_test_context();

    fn stop_command_loop_for_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "stop helper should not receive arguments");
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-command-loop-for-select-window-test",
        NativeFn::ContextVec(stop_command_loop_for_test),
        SubrArity::new(0, Some(0)),
    ));

    let target_window = ev
        .eval_str(
            r#"(let* ((w1 (selected-window))
                      (buf (get-buffer-create "select-window-command-loop-target"))
                      (w2 (split-window-internal w1 nil nil nil)))
                 (set-window-buffer w2 buf)
                 (setq neo-select-window-target w2)
                 w2)"#,
        )
        .expect("create second window");
    let target_window_id = target_window
        .as_window_id()
        .expect("target value should be a window");

    ev.eval_str(
        r#"(progn
             (setq neo-stop-selected-window nil)
             (fset 'neo-handle-select-window-for-test
                   (lambda (event)
                     (interactive "e")
                     (select-window
                      (posn-window (event-start event)))))
             (fset 'neo-stop-key-command
                   (lambda ()
                     (interactive)
                     (setq neo-stop-selected-window (selected-window))
                     (neo-stop-command-loop-for-select-window-test)))
             (keymap-set global-map "<select-window>"
                         'neo-handle-select-window-for-test)
             (keymap-set global-map "a" 'neo-stop-key-command))"#,
    )
    .expect("install test commands");

    ev.assign(
        "unread-command-events",
        Value::list(vec![
            Value::list(vec![
                Value::symbol("select-window"),
                Value::list(vec![Value::make_window(target_window_id)]),
            ]),
            Value::fixnum('a' as i64),
        ]),
    );
    ev.command_loop.running = true;

    ev.recursive_edit_inner()
        .expect("command loop should exit through the stop command");

    assert_eq!(
        ev.eval_symbol("neo-stop-selected-window")
            .expect("stop selected window"),
        Value::make_window(target_window_id),
        "normal command-loop reads must return leading select-window events \
         instead of delaying them past the following key"
    );
}

/// Finding 2 — `deactivate-mark` handling must run AFTER
/// `post-command-hook`, not before. GNU `command_loop_1` runs
/// `safe_run_hooks (Qpost_command_hook)` at keyboard.c:1563 and only
/// then evaluates the deactivate-mark / select-active-regions block at
/// keyboard.c:1597-1648 (`call0 (Qdeactivate_mark)` at 1611).
///
/// So a command that sets `deactivate-mark` must still observe an active
/// region from inside `post-command-hook`. Before the fix neomacs
/// deactivated the mark first, so the hook saw `(region-active-p) => nil`.
#[test]
fn deactivate_mark_runs_after_post_command_hook() {
    crate::test_utils::init_test_tracing();
    let mut ev = command_loop_test_context();

    fn stop_command_loop_for_test(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        assert!(args.is_empty(), "stop helper should not receive arguments");
        ctx.command_loop.running = false;
        Ok(Value::NIL)
    }
    ev.register_subr(SubrSpec::new(
        "neo-stop-command-loop-for-test",
        NativeFn::ContextVec(stop_command_loop_for_test),
        SubrArity::new(0, Some(0)),
    ));

    // Put some text in the buffer and activate the mark, then bind a key
    // to a command that requests deactivation. The post-command-hook
    // records whether the region is still active when it runs.
    ev.eval_str(
        r#"(progn
             (insert "hello world")
             (set-mark (point-min))
             (goto-char (point-max))
             (setq transient-mark-mode t)
             (activate-mark)
             (setq neo-region-active-in-post-hook 'unset)
             (setq neo-command-ran nil)
             (fset 'neo-deactivating-command
                   (lambda () (interactive)
                     (setq neo-command-ran t)
                     (setq deactivate-mark t)))
             (add-hook 'post-command-hook
                       (lambda ()
                         ;; The command loop runs post-command-hook once in
                         ;; its entry prologue before the first command; only
                         ;; record/stop after our command has actually run.
                         (when (and neo-command-ran
                                    (eq neo-region-active-in-post-hook 'unset))
                           (setq neo-region-active-in-post-hook
                                 (and (region-active-p) t))
                           (neo-stop-command-loop-for-test))))
             (keymap-set global-map "<f9>" 'neo-deactivating-command))"#,
    )
    .expect("set up deactivate-mark command and probe hook");

    assert!(
        ev.eval_str("(region-active-p)")
            .expect("region check")
            .is_truthy(),
        "region must be active before the command runs"
    );

    ev.command_loop
        .keyboard
        .kboard
        .unread_events
        .push_back(Value::symbol("f9"));
    ev.command_loop.running = true;

    ev.recursive_edit_inner()
        .expect("command loop should exit through the stop hook");

    assert_eq!(
        ev.eval_symbol("neo-command-ran").expect("command-ran flag"),
        Value::T,
        "the bound <f9> command must actually have run"
    );

    assert_eq!(
        ev.eval_symbol("neo-region-active-in-post-hook")
            .expect("probe variable"),
        Value::T,
        "post-command-hook must observe an active region; GNU deactivates \
         the mark only AFTER the hook (keyboard.c:1563 then 1597-1611)"
    );
    // And the mark is actually deactivated by the end of the iteration.
    assert!(
        ev.eval_str("(region-active-p)")
            .expect("region check after")
            .is_nil(),
        "deactivate-mark should still have run by the end of the iteration"
    );
}

/// Microbenchmark: baseline JIT (native) vs the Tier-0 bytecode VM running the
/// *identical* bytecode, in one binary, through the real `funcall` seam.
///
/// The body is a nullary arithmetic LOOP that sums 0..N for a compile-time N
/// (one funcall, many ops). We A/B it with the codebase's own forced-tier
/// helpers: `set_cold_for_test()` pins the function to the interpreter
/// (`Plan::Interpret`), `set_hot_for_test()` tiers it to native
/// (`Plan::Compiled`). Both functions hold byte-identical `ops`/`constants`.
///
/// What it measures: end-to-end `funcall_general_untraced` cost on each tier —
/// i.e. the loop body PLUS the shared call-seam overhead (dispatch match,
/// arg marshaling, GC-root push/pop). That seam cost is paid on BOTH sides, so
/// the ratio understates the pure body speedup but is a fair apples-to-apples
/// "what does tiering this call up actually buy" number.
#[cfg(feature = "jit")]
#[test]
fn bench_jit_vs_vm_loop() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    use std::time::Instant;

    // sum := 0; i := 0; while (i < N) { sum := sum + i; i := i + 1 }; return sum
    //
    // Two stack slots survive across the loop: slot0 = sum, slot1 = i.
    // constants[0] = 0 (the two initial pushes), constants[1] = N.
    //
    //  op  bytecode          stack after (top on right)        note
    //   0  Constant(0)       [0]                               push sum=0
    //   1  Constant(0)       [sum, 0]                          push i=0
    //   2  StackRef(0)       [sum, i, i]                       loop top: push i
    //   3  Constant(1)       [sum, i, i, N]                    push N
    //   4  Lss               [sum, i, (i<N)]                   i < N
    //   5  GotoIfNil(14)     [sum, i]                          exit if !(i<N); pops cond
    //   6  StackRef(1)       [sum, i, sum]                     push sum (1 below i)
    //   7  StackRef(1)       [sum, i, sum, i]                  push i   (1 below sum)
    //   8  Add               [sum, i, sum+i]                   sum + i
    //   9  StackSet(2)       [sum+i, i]                        store into slot0 (sum'), pop
    //  10  StackRef(0)       [sum', i, i]                      push i
    //  11  Add1              [sum', i, i+1]                    i + 1
    //  12  StackSet(1)       [sum', i+1]                       store into slot1 (i'), pop
    //  13  Goto(2)           [sum', i']                        back to loop top
    //  14  StackRef(1)       [sum, i, sum]                     exit: push sum (1 below i)
    //  15  Return            -> sum
    const N: i64 = 1000;
    let ops = vec![
        Op::Constant(0),   // 0
        Op::Constant(0),   // 1
        Op::StackRef(0),   // 2  loop top
        Op::Constant(1),   // 3
        Op::Lss,           // 4
        Op::GotoIfNil(14), // 5
        Op::StackRef(1),   // 6
        Op::StackRef(1),   // 7
        Op::Add,           // 8
        Op::StackSet(2),   // 9
        Op::StackRef(0),   // 10
        Op::Add1,          // 11
        Op::StackSet(1),   // 12
        Op::Goto(2),       // 13
        Op::StackRef(1),   // 14  loop end
        Op::Return,        // 15
    ];
    let constants = vec![Value::make_int(0), Value::make_int(N)];
    let expected = Value::make_int(N * (N - 1) / 2); // sum 0..N-1 = 499500

    let build = |hot: bool| -> Value {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.ops = ops.clone();
        f.constants = constants.clone().into();
        f.max_stack = 64;
        if hot {
            f.jit_runtime().set_hot_for_test();
        } else {
            // Pin to Tier-0 forever so the timed VM loop never tiers up.
            f.jit_runtime().set_cold_for_test();
        }
        Value::make_bytecode(f)
    };

    let mut ev = Context::new();

    // --- Correctness gate 1: VM result matches the closed form. ---
    let vm_fn = build(false);
    let vm_result = ev.funcall_general_untraced(vm_fn, vec![]).unwrap();
    assert_eq!(
        vm_result,
        expected,
        "VM bytecode result must equal N*(N-1)/2 = {}",
        N * (N - 1) / 2
    );

    // --- Correctness gate 2: the body is JIT-compilable (no compile-time bail). ---
    {
        let mut probe = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        probe.ops = ops.clone();
        probe.constants = constants.clone().into();
        probe.max_stack = 64;
        crate::emacs_core::jit::compile::compile_bytecode_function(&probe)
            .expect("loop body must compile to native (no unsupported op / CFG bail)");
    }

    // --- Correctness gate 3: native (hot) result matches the VM result. A
    // runtime deopt would silently fall back to the VM and still return the
    // right value, but it would also return the right value via native; the
    // real anti-deopt guard is the per-iter ratio + gate 2 above. We assert the
    // value here so a miscompile can never masquerade as a fast result. ---
    let jit_fn = build(true);
    let jit_result = ev.funcall_general_untraced(jit_fn.clone(), vec![]).unwrap();
    assert_eq!(
        jit_result, expected,
        "JIT-native result must equal the VM result"
    );

    // Reuse the SAME hot function value across all timed iterations so the
    // per-thread compiled-code cache (keyed by compiled_id) is hit every time —
    // no per-iteration recompile.
    const M: u64 = 200_000;

    // Warm up each path once more (caches, branch predictor).
    let warm_cold = build(false);
    let _ = ev.funcall_general_untraced(warm_cold, vec![]).unwrap();
    let _ = ev.funcall_general_untraced(jit_fn.clone(), vec![]).unwrap();

    // --- Time the VM path. A fresh cold function per outer iter would re-pay
    // make_bytecode; instead build ONE cold function and reuse it (it never
    // tiers up because force_interpret is pinned). ---
    let vm_timed = build(false);
    let t0 = Instant::now();
    let mut vm_acc = 0i64;
    for _ in 0..M {
        let r = ev
            .funcall_general_untraced(vm_timed.clone(), vec![])
            .unwrap();
        vm_acc = vm_acc.wrapping_add(r.xfixnum());
    }
    let vm_elapsed = t0.elapsed();

    // --- Time the JIT path: reuse the single hot function value. ---
    let t1 = Instant::now();
    let mut jit_acc = 0i64;
    for _ in 0..M {
        let r = ev.funcall_general_untraced(jit_fn.clone(), vec![]).unwrap();
        jit_acc = jit_acc.wrapping_add(r.xfixnum());
    }
    let jit_elapsed = t1.elapsed();

    // Both accumulators must be M * expected — proves every timed call returned
    // the correct sum on BOTH paths (so the JIT stayed correct/native, the VM
    // never tiered up).
    let want_acc = (M as i64).wrapping_mul(expected.xfixnum());
    assert_eq!(vm_acc, want_acc, "VM timed loop produced wrong sums");
    assert_eq!(jit_acc, want_acc, "JIT timed loop produced wrong sums");

    let vm_ns = vm_elapsed.as_nanos() as f64 / M as f64;
    let jit_ns = jit_elapsed.as_nanos() as f64 / M as f64;
    let ratio = vm_ns / jit_ns;

    eprintln!("=== bench_jit_vs_vm_loop (N={N}, M={M}) ===");
    eprintln!("VM  : total {:?}  ->  {:.1} ns/call", vm_elapsed, vm_ns);
    eprintln!("JIT : total {:?}  ->  {:.1} ns/call", jit_elapsed, jit_ns);
    eprintln!("speedup (VM/JIT): {:.2}x", ratio);
}

/// Threshold-economics companion to [`bench_jit_vs_vm_loop`]: how does the
/// tier-up threshold change END-TO-END wall time for functions of different
/// total call volume? Runs the same shape of fixnum sum loop (N=100 per call)
/// through the REAL heat counter — no forced tiers: the first
/// `hot_threshold()` calls interpret, then the seam compiles once and later
/// calls run native.
///
/// One process per threshold (the OnceLock caches the env at first read):
///
///   NEOVM_JIT_THRESHOLD=1000 cargo nextest run -p neovm-core \
///     -E 'test(jit_bench_threshold_economics)' --run-ignored all
///
/// Interpreting the numbers: a debug build inflates Cranelift compile time
/// relative to release, and compilation is the ONLY cost a lower threshold
/// adds (every other term favors tiering up earlier). A threshold that wins
/// under debug therefore wins in release a fortiori.
#[cfg(feature = "jit")]
#[test]
#[ignore = "manual perf measurement; A/B via NEOVM_JIT_THRESHOLD, one process per value"]
fn jit_bench_threshold_economics() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    use std::time::Instant;

    const N: i64 = 100;
    let ops = vec![
        Op::Constant(0),   // 0
        Op::Constant(0),   // 1
        Op::StackRef(0),   // 2  loop top
        Op::Constant(1),   // 3
        Op::Lss,           // 4
        Op::GotoIfNil(14), // 5
        Op::StackRef(1),   // 6
        Op::StackRef(1),   // 7
        Op::Add,           // 8
        Op::StackSet(2),   // 9
        Op::StackRef(0),   // 10
        Op::Add1,          // 11
        Op::StackSet(1),   // 12
        Op::Goto(2),       // 13
        Op::StackRef(1),   // 14  loop end
        Op::Return,        // 15
    ];
    let constants = vec![Value::make_int(0), Value::make_int(N)];
    let expected = Value::make_int(N * (N - 1) / 2);

    // Fresh function per scenario: an untouched heat counter, so tier-up
    // happens exactly where the threshold puts it.
    let build = || -> Value {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.ops = ops.clone();
        f.constants = constants.clone().into();
        f.max_stack = 64;
        Value::make_bytecode(f)
    };

    let mut ev = Context::new();
    let threshold = crate::emacs_core::jit::hot_threshold();

    // Call volumes bracketing the decision: "hot" amortizes any compile cost,
    // "medium" is the population a 10k threshold strands in the interpreter,
    // "barely-warm" is the compile-cost risk case for a low threshold.
    for (label, calls) in [
        ("hot", 20_000u32),
        ("medium", 3_000u32),
        ("barely-warm", 1_200u32),
    ] {
        let f = build();
        let t = Instant::now();
        for _ in 0..calls {
            let r = ev.funcall_general_untraced(f, vec![]).unwrap();
            assert_eq!(r, expected, "sum loop must stay correct on every tier");
        }
        let elapsed = t.elapsed();
        eprintln!(
            "[threshold-econ] threshold={threshold} workload={label} calls={calls} total={elapsed:?} ({:.0} ns/call)",
            elapsed.as_nanos() as f64 / calls as f64
        );
    }
}

#[test]
fn string_equal_unibyte_high_byte_vs_multibyte_char() {
    crate::test_utils::init_test_tracing();
    // GNU: these are all nil because the raw unibyte byte (1 byte) and the
    // multibyte char of the same number (2 bytes) differ in internal-form bytes.
    assert_eq!(
        eval_one(r#"(string-equal "é" (unibyte-string 233))"#),
        "OK nil"
    );
    assert_eq!(
        eval_one(r#"(string-equal (unibyte-string 200) (char-to-string 200))"#),
        "OK nil"
    );
    assert_eq!(
        eval_one(r#"(string-equal (unibyte-string 233 234) "éê")"#),
        "OK nil"
    );
    // Sanity: identical multibyte strings and pure-ASCII unibyte/multibyte
    // still compare equal, matching GNU.
    assert_eq!(eval_one(r#"(string-equal "éê" "éê")"#), "OK t");
    assert_eq!(
        eval_one(r#"(string-equal (string-to-unibyte "abc") (string-to-multibyte "abc"))"#),
        "OK t"
    );
}

// =======================================================================
// Special-form bodies with an improper tail are validated UP FRONT.
//
// GNU eval.c:2624 runs `list_length (args_left)` for every SUBRP `fun`
// (including UNEVALLED special forms) before dispatch, so an improper
// top-level argument list signals `(wrong-type-argument listp BAD-CDR)`
// *before* any body form is evaluated. Neo used to walk lazily, evaluating
// the first element first (yielding void-variable, or no error at all).
// =======================================================================

#[test]
fn if_improper_body_signals_listp_before_eval() {
    // GNU: (eval '(if t a . b) t) => (wrong-type-argument listp b)
    // (NOT void-variable a; the body is validated before `a` is evaluated).
    assert_eq!(
        eval_one("(condition-case e (eval '(if t a . b) t) (error e))"),
        "OK (wrong-type-argument listp b)"
    );
}

#[test]
fn if_improper_body_validated_even_for_else_branch() {
    // GNU: (eval '(if nil x . b) t) => (wrong-type-argument listp b)
    // (cond is nil so `x` is never reached, but the tail is still checked).
    assert_eq!(
        eval_one("(condition-case e (eval '(if nil x . b) t) (error e))"),
        "OK (wrong-type-argument listp b)"
    );
}

#[test]
fn macro_call_improper_body_signals_listp_with_no_error_before() {
    // The bug report's `(eval '(when t . b) t)` => (wrong-type-argument listp b)
    // case (neo previously returned nil with NO error) is the macroexpand path:
    // `when` is a macro, so the improper tail is caught when `apply` collects
    // the expander's args. `when` lives in subr.el (unavailable on a bare
    // Context), so model it with a `when`-shaped cons-cell macro.
    let src = "(defalias 'my-when (cons 'macro #'(lambda (cond &rest body)
                 (list 'if cond (cons 'progn body)))))
               (condition-case e (eval '(my-when t . b) t) (error e))";
    let results = eval_all(src);
    assert_eq!(results[1], "OK (wrong-type-argument listp b)");
}

#[test]
fn progn_improper_body_signals_listp_before_eval() {
    // GNU: (eval '(progn a . b) t) => (wrong-type-argument listp b)
    // (NOT void-variable a).
    assert_eq!(
        eval_one("(condition-case e (eval '(progn a . b) t) (error e))"),
        "OK (wrong-type-argument listp b)"
    );
    // Even with a literal first form, the tail check fires first.
    assert_eq!(
        eval_one("(condition-case e (eval '(progn 1 . b) t) (error e))"),
        "OK (wrong-type-argument listp b)"
    );
}

#[test]
fn lambda_apply_improper_arglist_signals_listp() {
    // GNU: (eval '((lambda (a &rest b) b) x . y) t)
    //        => (wrong-type-argument listp y)
    // (NOT void-variable x; the argument list is validated up front).
    assert_eq!(
        eval_one("(condition-case e (eval '((lambda (a &rest b) b) x . y) t) (error e))"),
        "OK (wrong-type-argument listp y)"
    );
}

#[test]
fn proper_special_form_bodies_unchanged() {
    // Regression guard: normal proper bodies must keep evaluating identically.
    // (`when` lives in subr.el and is unavailable on a bare Context, so the
    // proper-`when` cases live in the macro-path tests further down.)
    assert_eq!(eval_one("(eval '(progn 1 2 3) t)"), "OK 3");
    assert_eq!(eval_one("(eval '(if t 1 2) t)"), "OK 1");
    assert_eq!(eval_one("(eval '(if nil 1 2 3) t)"), "OK 3");
    assert_eq!(eval_one("(eval '(progn) t)"), "OK nil");
    assert_eq!(eval_one("(eval '(and 1 2 3) t)"), "OK 3");
    assert_eq!(eval_one("(eval '(or nil 7) t)"), "OK 7");
    assert_eq!(
        eval_one("(eval '((lambda (a &rest b) b) 1 2 3) t)"),
        "OK (2 3)"
    );
    // Other special forms also validate the top-level tail up front (GNU
    // eval.c:2624 `list_length`), matching GNU exactly.
    assert_eq!(
        eval_one("(condition-case e (eval (cons 'quote 5) t) (error e))"),
        "OK (wrong-type-argument listp 5)"
    );
    assert_eq!(
        eval_one("(condition-case e (eval '(setq x 1 . y) t) (error e))"),
        "OK (wrong-type-argument listp y)"
    );
    assert_eq!(
        eval_one("(condition-case e (eval '(let ((a 1)) a . b) t) (error e))"),
        "OK (wrong-type-argument listp b)"
    );
}

// =======================================================================
// `macroexpand` of a macro call with an improper argument tail reports
// only the bad cdr, not the whole improper tail.
//
// GNU `apply1 (expander, XCDR (form))` -> `Fapply` -> `list_length`
// (fns.c:115) ends in `CHECK_LIST_END (list, list)`, so the irritant is
// the final non-nil cdr, not the entire `(a . b)` tail.
// =======================================================================

// `m` is defined as a cons-cell macro via `defalias` (the bare-evaluator
// idiom used by `defmacro_works`); a plain `defmacro` is unavailable on a
// bare `Context`. The expander quotes its &rest args, so a successful
// expansion of `(m ...)` yields `(quote (...))`.
const MACRO_M_DEF: &str = "(defalias 'm (cons 'macro #'(lambda (&rest b) (list 'quote b))))";

#[test]
fn macroexpand_improper_tail_reports_only_bad_cdr() {
    // GNU: (macroexpand '(m a . b)) => (wrong-type-argument listp b)
    // (NOT (wrong-type-argument listp (a . b))).
    let src = format!("{MACRO_M_DEF}\n(condition-case e (macroexpand '(m a . b)) (error e))");
    let results = eval_all(&src);
    assert_eq!(results[1], "OK (wrong-type-argument listp b)");
}

#[test]
fn macroexpand_improper_tail_deeper_bad_cdr() {
    // GNU: (macroexpand '(m a c . b)) => (wrong-type-argument listp b)
    let src = format!("{MACRO_M_DEF}\n(condition-case e (macroexpand '(m a c . b)) (error e))");
    let results = eval_all(&src);
    assert_eq!(results[1], "OK (wrong-type-argument listp b)");
}

#[test]
fn macroexpand_proper_args_unchanged() {
    // Regression guard: a proper arg list still expands normally.
    // GNU verified: (macroexpand '(m a c d)) => '(a c d).
    let src = format!("{MACRO_M_DEF}\n(macroexpand '(m a c d))");
    let results = eval_all(&src);
    assert_eq!(results[1], "OK '(a c d)");
}

// =======================================================================
// Concurrent-GC termination-drain KIND profile (profiling aid)
// =======================================================================

/// Median of an unsorted sample (0 for an empty one).
#[allow(dead_code)]
fn drain_probe_median(samples: &mut [u64]) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// Shared driver for the `gc_drain_kinds_profile_*` probes: churn a mixed
/// Lisp working set (strings/lists/vectors/records/closures/hash-tables)
/// through a bootstrapped evaluator in SMALL eval chunks, and after each chunk
/// poll `SweepStats.termination_count` to capture the per-cycle deferred
/// total, per-kind breakdown, fold cost, and termination drain (`mark_us`).
/// The retained rolling window (1 in 8 iterations, 256 slots) keeps a live
/// mixed working set while the rest churns to drive cycles. Note what feeds
/// `deferred`: the GC thread parks a non-cons per DISCOVERED EDGE — from the
/// rooted cons walk (live graph only) but also from the obarray scan and the
/// Tier B vector-backing scan, and the latter conservatively scans EVERY
/// owned vector alive at cycle start, garbage included, so churned vectors'
/// slot contents inflate the buffer with duplicates.
///
/// `pdump=true` measures the REAL dump-partitioned configuration: the
/// bootstrap-cache pdump `runtime_startup_context` loads maps the whole
/// bootstrap state, so dump conses are skipped and young objects reach the GC
/// via the remembered set. `pdump=false` (`NEOVM_DISABLE_PDUMP=1`) measures
/// the dump-less heap, where the concurrent collector re-walks the entire
/// live graph every cycle after the STW bootstrap.
#[allow(dead_code)]
fn gc_drain_kinds_profile(pdump: bool, chunks: usize) {
    crate::test_utils::init_test_tracing();
    // Print the per-cycle `concurrent_termination ... kinds[...]` trace lines
    // alongside the in-test capture (ground truth under --no-capture).
    unsafe { std::env::set_var("NEOVM_GC_TRACE", "1") };
    if !pdump {
        unsafe { std::env::set_var("NEOVM_DISABLE_PDUMP", "1") };
    }
    let mut ev = runtime_startup_context();
    if pdump && !ev.tagged_heap.dump_partition_active() {
        // Cold bootstrap cache: that first call ran the live bootstrap
        // (dump-less) and WROTE the cache; load the freshly written pdump so
        // the measured heap really has the mapped partition.
        ev = runtime_startup_context();
        assert!(
            ev.tagged_heap.dump_partition_active(),
            "pdump probe config requires a mapped bootstrap image",
        );
    }
    ev.set_lexical_binding(true);
    eprintln!(
        "DRAIN-KINDS PROBE start: pdump={pdump} partition_active={} live={}B",
        ev.tagged_heap.dump_partition_active(),
        ev.tagged_heap.live_bytes(),
    );
    ev.eval_str(
        "(progn \
           (defvar drain-probe--keep (make-vector 256 nil)) \
           (defvar drain-probe--i 0) \
           (defun drain-probe--step (n) \
             (let ((k 0)) \
               (while (< k n) \
                 (let* ((s (make-string 64 ?s)) \
                        (l (make-list 32 k)) \
                        (v (make-vector 24 s)) \
                        (r (record 'drain-probe s l v)) \
                        (h (make-hash-table :test 'eq :size 8)) \
                        (c (lambda (q) (cons q s)))) \
                   (puthash 0 r h) \
                   (puthash 1 c h) \
                   (when (= 0 (% k 8)) \
                     (aset drain-probe--keep (% drain-probe--i 256) \
                           (list s l v r h c)) \
                     (setq drain-probe--i (1+ drain-probe--i)))) \
                 (setq k (1+ k)))) \
             nil) \
           t)",
    )
    .expect("probe setup");

    let mut captures: Vec<crate::tagged::gc::SweepStats> = Vec::new();
    // Handshake decomposition snapshot per captured cycle (same poll instant;
    // the start-side fields belong to the just-terminated cycle unless the
    // NEXT cycle already started within the same chunk — an attribution skew
    // that washes out of the medians).
    let mut hs_captures: Vec<crate::tagged::gc::HandshakeStats> = Vec::new();
    let mut missed = 0usize;
    let mut seen = ev.tagged_heap.sweep_stats().termination_count;
    let start_collections = ev.tagged_heap.gc_collections();
    for _ in 0..chunks {
        ev.eval_str("(drain-probe--step 200)").expect("churn step");
        let stats = ev.tagged_heap.sweep_stats();
        if stats.termination_count > seen {
            // More than one termination inside a single chunk loses all but
            // the last cycle's per-cycle stats; count the loss honestly.
            missed += stats.termination_count - seen - 1;
            seen = stats.termination_count;
            captures.push(stats);
            hs_captures.push(ev.tagged_heap.handshake_stats());
        }
    }

    let mut lines = String::new();
    for (i, s) in captures.iter().enumerate() {
        lines.push_str(&format!(
            "cycle#{i} deferred={} satb={} fold={}us drain={}us str_claimed={} \
             f_claimed={} sub_dropped={} v_claimed={} bc_claimed={} kinds[{}]\n",
            s.last_termination_deferred,
            s.last_termination_satb,
            s.last_termination_fold_us,
            s.mark_us,
            s.last_concurrent_str_claimed,
            s.last_concurrent_float_claimed,
            s.last_concurrent_subr_dropped,
            s.last_concurrent_vec_claimed,
            s.last_concurrent_bc_claimed,
            s.last_termination_kinds,
        ));
    }
    let med = |f: fn(&crate::tagged::gc::SweepStats) -> u64| {
        let mut v: Vec<u64> = captures.iter().map(f).collect();
        drain_probe_median(&mut v)
    };
    let summary = format!(
        "config={} cycles_captured={} missed={} gc_collections={} \
         churn_chunks={chunks} live={}B\n\
         medians: deferred={} satb={} fold_us={} drain_us={} str_claimed={} \
         f_claimed={} sub_dropped={} v_claimed={} bc_claimed={}\n\
         kind medians: str={} vec={} rec={} clo={} bc={} ht={} ct={} f={} cons={} sub={} other={}\n\
         kind maxima (lifetime): {}\n",
        if pdump {
            "pdump(mapped-dump)"
        } else {
            "plain(dump-less)"
        },
        captures.len(),
        missed,
        ev.tagged_heap.gc_collections() - start_collections,
        ev.tagged_heap.live_bytes(),
        med(|s| s.last_termination_deferred as u64),
        med(|s| s.last_termination_satb as u64),
        med(|s| s.last_termination_fold_us),
        med(|s| s.mark_us),
        med(|s| s.last_concurrent_str_claimed as u64),
        med(|s| s.last_concurrent_float_claimed as u64),
        med(|s| s.last_concurrent_subr_dropped as u64),
        med(|s| s.last_concurrent_vec_claimed as u64),
        med(|s| s.last_concurrent_bc_claimed as u64),
        med(|s| s.last_termination_kinds.string as u64),
        med(|s| s.last_termination_kinds.vector as u64),
        med(|s| s.last_termination_kinds.record as u64),
        med(|s| s.last_termination_kinds.closure as u64),
        med(|s| s.last_termination_kinds.bytecode as u64),
        med(|s| s.last_termination_kinds.hash_table as u64),
        med(|s| s.last_termination_kinds.char_table as u64),
        med(|s| s.last_termination_kinds.float as u64),
        med(|s| s.last_termination_kinds.cons as u64),
        med(|s| s.last_termination_kinds.subr as u64),
        med(|s| s.last_termination_kinds.other as u64),
        ev.tagged_heap.sweep_stats().max_termination_kinds,
    );

    // --- HANDSHAKE decomposition (root-scan floor probe): per-phase and
    // per-group medians/maxima across the captured cycles, for BOTH STW
    // handshakes, plus the O() size probes. ---
    let hmed = |f: &dyn Fn(&crate::tagged::gc::HandshakeStats) -> u64| {
        let mut v: Vec<u64> = hs_captures.iter().map(f).collect();
        drain_probe_median(&mut v)
    };
    let hmax = |f: &dyn Fn(&crate::tagged::gc::HandshakeStats) -> u64| {
        hs_captures.iter().map(f).max().unwrap_or(0)
    };
    // Per-group aggregation: name -> (us samples, count samples), separately
    // for the start and termination context-root breakdowns.
    let group_table = |select: &dyn Fn(
        &crate::tagged::gc::HandshakeStats,
    ) -> &crate::tagged::gc::RootSeedBreakdown| {
        let mut agg: std::collections::BTreeMap<&'static str, (Vec<u64>, Vec<u64>)> =
            std::collections::BTreeMap::new();
        for hs in &hs_captures {
            for &(name, us, count) in &select(hs).groups {
                let entry = agg.entry(name).or_default();
                entry.0.push(us);
                entry.1.push(count as u64);
            }
        }
        let mut rows: Vec<(u64, u64, u64, &'static str)> = agg
            .into_iter()
            .map(|(name, (mut us, mut counts))| {
                let med_us = drain_probe_median(&mut us);
                let max_us = us.iter().copied().max().unwrap_or(0);
                let med_count = drain_probe_median(&mut counts);
                (med_us, max_us, med_count, name)
            })
            .collect();
        rows.sort_by(|a, b| b.cmp(a)); // by median us desc
        let mut out = String::new();
        for (med_us, max_us, med_count, name) in rows {
            out.push_str(&format!(
                "    {name}: med={med_us}us max={max_us}us med_count={med_count}\n"
            ));
        }
        out
    };
    let handshake_summary = format!(
        "HANDSHAKE start: total med={}us max={} | clear med={}[cons={} noncons={} \
         mapped={}] runtime med={}({}) \
         remembered med={}({}) obsnap med={} ctxroots med={} conssnap med={} \
         vecsnap med={} floatsnap med={} vecbasesnap med={} bcsnap med={} jobasm med={}\n\
         start groups (med us / max us / med count):\n{}\
         HANDSHAKE termination: roots-lump med={}us max={} | join med={} fold med={} \
         runtime med={}({}) remembered med={}({}) ctxroots med={} newsyms med={}({}) \
         drain med={} finalizer med={} weak med={} unchain med={}\n\
         termination groups (med us / max us / med count):\n{}\
         probes (median): jit={}/{} rem={} bc={} spec={} obslots={} obchunks={} \
         vecs={} consblk={} bufs={}\n",
        hmed(&|h| h.last_start_total_us),
        hmax(&|h| h.last_start_total_us),
        hmed(&|h| h.last_start_clear_us),
        hmed(&|h| h.last_start_clear_cons_us),
        hmed(&|h| h.last_start_clear_noncons_us),
        hmed(&|h| h.last_start_clear_mapped_us),
        hmed(&|h| h.last_start_runtime_us),
        hmed(&|h| h.last_start_runtime_roots as u64),
        hmed(&|h| h.last_start_remembered_us),
        hmed(&|h| h.last_start_remembered_roots as u64),
        hmed(&|h| h.last_start_obsnap_us),
        hmed(&|h| h.last_start_roots.total_us),
        hmed(&|h| h.last_start_conssnap_us),
        hmed(&|h| h.last_start_vecsnap_us),
        hmed(&|h| h.last_start_floatsnap_us),
        hmed(&|h| h.last_start_vecbasesnap_us),
        hmed(&|h| h.last_start_bcsnap_us),
        hmed(&|h| h.last_start_jobasm_us),
        group_table(&|h| &h.last_start_roots),
        hmed(&|h| h.last_term_roots_total_us),
        hmax(&|h| h.last_term_roots_total_us),
        hmed(&|h| h.last_term_join_us),
        med(|s| s.last_termination_fold_us),
        hmed(&|h| h.last_term_runtime_us),
        hmed(&|h| h.last_term_runtime_roots as u64),
        hmed(&|h| h.last_term_remembered_us),
        hmed(&|h| h.last_term_remembered_roots as u64),
        hmed(&|h| h.last_term_ctxroots.total_us),
        hmed(&|h| h.last_term_newsyms_us),
        hmed(&|h| h.last_term_newsyms_roots as u64),
        med(|s| s.mark_us),
        hmed(&|h| h.last_term_finalizer_us),
        hmed(&|h| h.last_term_weak_us),
        hmed(&|h| h.last_term_unchain_us),
        group_table(&|h| &h.last_term_ctxroots),
        hmed(&|h| h.probe_jit_compiled_entries as u64),
        hmed(&|h| h.probe_jit_reloc_slots as u64),
        hmed(&|h| h.probe_mapped_remembered as u64),
        hmed(&|h| h.probe_bc_buf_depth as u64),
        hmed(&|h| h.probe_specpdl_depth as u64),
        hmed(&|h| h.probe_obarray_slots as u64),
        hmed(&|h| h.probe_obarray_chunks as u64),
        hmed(&|h| h.probe_vector_snapshot_len as u64),
        hmed(&|h| h.probe_cons_blocks as u64),
        hmed(&|h| h.probe_buffer_count as u64),
    );
    // Like the other profiling aids, report via panic! so the dump surfaces
    // under nextest's capture (NOT a failure).
    panic!(
        "DRAIN-KINDS PROFILE (profiling aid, not a failure)\n{lines}{summary}{handshake_summary}"
    );
}

/// Task 01 SUBR RECOGNIZE-AND-DROP in the REAL bootstrap context (mapped
/// pdump when the cache is warm; the mandated `runtime_startup_context`
/// driver either way), under the armed partition/tricolor verifiers: the
/// startup image resolves ~1.6-1.7k builtin subrs as leaked statics that a
/// concurrent cycle used to park every time. After the drop: (a) the drop
/// counter is hot on churn-driven cycles, (b) every builtin stays callable
/// (subr values still work), and (c) the verifiers pass — the oracle that
/// no MAPPED subr was mis-recognized as leaked (its side-table mark would
/// be missing and the partition verifier would panic).
#[test]
fn gc_concurrent_leaked_subr_drop_under_pdump_verifiers() {
    crate::test_utils::init_test_tracing();
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    let mut ev = runtime_startup_context();
    if !ev.tagged_heap.dump_partition_active() {
        // Cold bootstrap cache: the first call ran the live bootstrap and
        // wrote it; reload so the measured heap has the mapped partition
        // (mirrors the drain profiler's cold-cache handling).
        ev = runtime_startup_context();
    }
    ev.set_lexical_binding(true);

    // Churn until two concurrent terminations complete (bounded).
    let seen0 = ev.tagged_heap.sweep_stats().termination_count;
    let mut guard = 0usize;
    while ev.tagged_heap.sweep_stats().termination_count < seen0 + 2 {
        ev.eval_str("(let ((l nil)) (dotimes (i 2000) (push (format \"s%d\" i) l)) (length l))")
            .expect("churn step");
        guard += 1;
        assert!(
            guard < 4000,
            "no concurrent termination observed under churn",
        );
    }

    let stats = ev.tagged_heap.sweep_stats();
    assert!(
        stats.last_concurrent_subr_dropped > 0,
        "builtin subrs (function cells scanned every cycle) must be dropped \
         on the GC thread (sub_dropped={} kinds[{}])",
        stats.last_concurrent_subr_dropped,
        stats.last_termination_kinds,
    );

    // Subr values still work: builtins dispatch fine after cycles of drops.
    assert_eq!(
        ev.eval_str("(car (list 1 2 3))").expect("car"),
        Value::fixnum(1),
    );
    assert_eq!(
        ev.eval_str("(funcall #'+ 20 22)").expect("funcall +"),
        Value::fixnum(42),
    );
}

/// TSan GATE for task #23 — the concurrent obarray-scan presence-byte race.
///
/// The concurrent GC obarray scan (`ObarrayScanSnapshot::scan`, GC thread) walks
/// the `[LispSymbol; 4096]` chunks over slots `[0, n_slots)`. `n_slots` is the
/// chunk-ROUNDED capacity, not the interned count, so the EMPTY tail slots of the
/// last chunk are IN RANGE. Meanwhile the mutator (this thread) concurrently:
///   (a) flips `function_unbound` in place via `set_symbol_function_id` /
///       `fmakunbound_id` on already-interned symbols (race A), and
///   (b) fresh-fills those empty tail slots None->Some via `intern`
///       (races B/C/D/E: the fill's arm writes vs the scan's val/function/plist
///       loads).
/// Before the fix, the scan read slot PRESENCE off the `Option<LispSymbol>` niche
/// — which Rust packed into the `function_unbound` byte — so the presence read
/// data-raced both writers.
///
/// This is a TSAN GATE, NOT a functional test. The race is BENIGN on x86
/// (presence is monotonic None->Some, so even a torn/stale presence read still
/// resolves correctly), so the assertions below passing does NOT prove the fix —
/// they only guard against a functional regression. The real acceptance
/// criterion is a ThreadSanitizer run:
///   crates/neovm-core/scripts/run-gc-tsan.sh gc_concurrent_obarray_scan_vs_defalias_churn
/// which RACES on pre-fix `main` and is CLEAN post-fix (0 races on obarray-slot
/// memory). The test name is on `run-gc-tsan.sh`'s `SURFACE_RE`
/// (`^emacs_core::eval::tests::gc_concurrent`).
#[test]
fn gc_concurrent_obarray_scan_vs_defalias_churn() {
    crate::test_utils::init_test_tracing();
    // This is a TSan DATA-RACE gate; it runs in PLAIN mode (run-gc-tsan.sh builds
    // without stress instrumentation). Under NEOVM_GC_STRESS the runtime bootstrap
    // (`runtime_startup_context` below) forces a full GC at every allocation-bearing
    // safe point (eval.rs `gc_stress_enabled`), so startup alone is orders of
    // magnitude slower and blows the test timeout -- the same pre-existing reason
    // the other full-bootstrap tests time out under stress. Stress mode adds no
    // race signal for this gate, so skip it rather than time out.
    if std::env::var("NEOVM_GC_STRESS").as_deref() == Ok("1") {
        return;
    }
    // Real pdump-partitioned config, same guard as the leaked-subr repro: the
    // concurrent mark engages and its obarray scan runs on the GC thread.
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_PARTITION", "1") };
    let mut ev = runtime_startup_context();
    if !ev.tagged_heap.dump_partition_active() {
        // Cold bootstrap cache: reload so the measured heap has the partition.
        ev = runtime_startup_context();
    }
    ev.set_lexical_binding(true);

    // The scan needs >=2 chunks so the last chunk has EMPTY tail slots that are
    // in-range for the snapshot (fresh interns land there -> races B/C/D/E).
    // `current_slot_len()` is always a multiple of the 4096-slot chunk; the real
    // startup obarray already spans several chunks, but top up defensively so the
    // precondition holds on any build.
    let mut topup = 0u64;
    while ev.obarray.current_slot_len() < 2 * 4096 {
        ev.obarray.intern(&format!("t23-topup-{topup}"));
        topup += 1;
    }
    assert!(ev.obarray.current_slot_len() >= 2 * 4096);

    // A pool of already-interned symbols whose function cells we flip IN PLACE
    // (race A). Materialize their obarray slots up front so the churn below is a
    // pure in-place flip, not a fresh fill.
    let churn_ids: Vec<SymId> = (0..64)
        .map(|i| {
            let id = crate::emacs_core::intern::intern(&format!("t23-churn-{i}"));
            ev.obarray.set_symbol_function_id(id, Value::NIL);
            id
        })
        .collect();

    // Overlap strategy: the GC obarray scan runs at the START of each concurrent
    // mark, so the mutator must be actively writing the obarray *during* mark
    // start. We therefore run a fixed, MUTATION-DENSE burst — a full flip batch
    // (race A) plus tail-slot fills (races B/C/D/E) EVERY iteration — with a
    // per-iteration heap bump to keep concurrent marks (hence their
    // start-of-cycle scans) starting. Over the burst the mutator spends almost
    // all its time writing obarray slots, so the many concurrent scans that run
    // reliably overlap a mutation of the same slot. The churn is cheap (tens of
    // ms per iteration — startup dominates wall time); the burst is sized for
    // many scan overlaps.
    let seen0 = ev.tagged_heap.sweep_stats().termination_count;
    let mut fresh = 0u64;
    // Cap fresh interns so the obarray can't grow without bound; the fset churn
    // (race A) keeps running regardless.
    const MAX_FRESH: u64 = 60_000;
    const BURST: u64 = 256;
    for _ in 0..BURST {
        // (a) in-place function-cell / `function_unbound` flips on existing
        //     interned symbols — the write the pre-fix presence read raced.
        for (k, &id) in churn_ids.iter().enumerate() {
            if k % 2 == 0 {
                ev.obarray
                    .set_symbol_function_id(id, Value::fixnum(k as i64));
            } else {
                ev.obarray.fmakunbound_id(id);
            }
        }
        // (b) fresh-fill empty tail slots None->Some (races B/C/D/E): each new
        //     name mints the next dense SymId, filling an in-range empty tail slot
        //     of the snapshotted obarray.
        if fresh < MAX_FRESH {
            for _ in 0..16 {
                ev.obarray.intern(&format!("t23-fill-{fresh}"));
                fresh += 1;
            }
        }
        // Heap bump to arm the pacer so concurrent marks keep starting (same
        // recipe as the leaked-subr repro).
        ev.eval_str("(let ((l nil)) (dotimes (i 2000) (push (format \"s%d\" i) l)) (length l))")
            .expect("churn alloc");
    }

    // Concurrency actually engaged: the GC obarray scan demonstrably ran on the
    // GC thread while we churned (else there was no race surface to exercise).
    // Mirrors the leaked-subr repro's ">=2 concurrent terminations" bar.
    let terminations = ev.tagged_heap.sweep_stats().termination_count - seen0;
    assert!(
        terminations >= 2,
        "concurrent GC did not engage under churn (terminations={terminations}); \
         the obarray-scan race surface was never exercised",
    );

    // Functional correctness — a REGRESSION guard, not proof of the fix. Every
    // churned symbol resolves to its deterministic last write, a fresh symbol
    // dispatches, and the whole obarray still walks.
    for (k, &id) in churn_ids.iter().enumerate() {
        if k % 2 == 0 {
            assert_eq!(
                ev.obarray.symbol_function_id(id),
                Some(Value::fixnum(k as i64)),
                "churn symbol {k} lost its function cell across concurrent GC",
            );
        } else {
            assert!(
                ev.obarray.is_function_unbound_id(id),
                "fmakunbound'd churn symbol {k} not unbound after concurrent GC",
            );
        }
    }
    assert_eq!(
        ev.eval_str("(car (list 1 2 3))").expect("car"),
        Value::fixnum(1),
    );
    assert_eq!(ev.eval_str("(+ 20 22)").expect("plus"), Value::fixnum(42));
    // The freshly-filled tail symbols are present and resolvable.
    assert!(
        ev.obarray.all_symbols().len() >= churn_ids.len(),
        "obarray scan/walk lost interned symbols",
    );
}

/// Which kinds dominate the concurrent GC's STW termination drain — the
/// measurement that decides the concurrent-tracing extension order
/// (strings-first vs records/closures vs "hash tables dominate, stop").
/// Real dump-partitioned config: the bootstrap state is a mapped pdump (the
/// bootstrap cache), dump conses are skipped by the GC thread. Run in release:
///   cargo nextest run -p neovm-core --release --run-ignored ignored-only \
///     --no-capture -E 'test(gc_drain_kinds_profile_pdump)'
#[test]
#[ignore = "profiling aid; run explicitly in release with --no-capture"]
fn gc_drain_kinds_profile_pdump() {
    gc_drain_kinds_profile(true, 400);
}

/// Plain dump-less config: the concurrent collector engages after the STW
/// bootstrap (post adaptive-pacer cadence — fewer, bigger cycles). Run:
///   cargo nextest run -p neovm-core --release --run-ignored ignored-only \
///     --no-capture -E 'test(gc_drain_kinds_profile_plain)'
#[test]
#[ignore = "profiling aid; run explicitly in release with --no-capture"]
fn gc_drain_kinds_profile_plain() {
    gc_drain_kinds_profile(false, 400);
}

/// Shared driver for the `alloc_class_profile_*` probes (size-class arena
/// design input): capture the NON-CONS allocation-rate and size-class
/// distribution (per kind, total-bytes histogram, peak
/// `non_cons_object_addrs` population) over three real phases —
///   1. startup (pdump load / live bootstrap = the expanded-cache replay),
///   2. the drain-kinds mixed churn workload (same recipe as
///      `gc_drain_kinds_profile`),
///   3. a byte-compile workload (the `vm_subr_mix_byte_compile` recipe).
///
/// Counters live in `crate::tagged::gc::alloc_probe` (test-only statics fed
/// by `link_object`/`link_veclike`).
#[allow(dead_code)]
fn alloc_class_profile(pdump: bool) {
    use crate::tagged::gc::alloc_probe;
    crate::test_utils::init_test_tracing();
    if !pdump {
        unsafe { std::env::set_var("NEOVM_DISABLE_PDUMP", "1") };
    }

    // -- Phase 1: startup allocation profile --
    alloc_probe::reset();
    let mut ev = runtime_startup_context();
    if pdump && !ev.tagged_heap.dump_partition_active() {
        // Cold bootstrap cache: first call ran the live bootstrap and wrote
        // the cache; reload so the measured config really has the partition.
        alloc_probe::reset();
        ev = runtime_startup_context();
        assert!(ev.tagged_heap.dump_partition_active());
    }
    let startup_report = alloc_probe::report();
    ev.set_lexical_binding(true);

    // -- Phase 2: mixed churn (gc_drain_kinds_profile recipe) --
    ev.eval_str(
        "(progn \
           (defvar drain-probe--keep (make-vector 256 nil)) \
           (defvar drain-probe--i 0) \
           (defun drain-probe--step (n) \
             (let ((k 0)) \
               (while (< k n) \
                 (let* ((s (make-string 64 ?s)) \
                        (l (make-list 32 k)) \
                        (v (make-vector 24 s)) \
                        (r (record 'drain-probe s l v)) \
                        (h (make-hash-table :test 'eq :size 8)) \
                        (c (lambda (q) (cons q s)))) \
                   (puthash 0 r h) \
                   (puthash 1 c h) \
                   (when (= 0 (% k 8)) \
                     (aset drain-probe--keep (% drain-probe--i 256) \
                           (list s l v r h c)) \
                     (setq drain-probe--i (1+ drain-probe--i)))) \
                 (setq k (1+ k)))) \
             nil) \
           t)",
    )
    .expect("probe setup");
    alloc_probe::reset();
    let churn_t0 = std::time::Instant::now();
    for _ in 0..100 {
        ev.eval_str("(drain-probe--step 200)").expect("churn step");
    }
    let churn_secs = churn_t0.elapsed().as_secs_f64();
    let churn_report = alloc_probe::report();

    // -- Phase 3: byte-compile workload (vm_subr_mix_byte_compile recipe) --
    let mut body = String::new();
    for i in 0..30 {
        body.push_str(&format!(
            "(setq acc (cons (list {i} (format \"s%d\" n) (assq 'k tbl)) acc)) \
             (when (> (length acc) 40) (setq acc (nthcdr 2 acc))) \
             (setq s (concat s (substring (symbol-name 'sym{i}) 0 2))) ",
        ));
    }
    let defun = format!(
        "(progn (defun sm-work (n) \
           (let ((acc nil) (s \"\") (tbl '((k . 1) (j . 2)))) {body} (list acc s))) t)"
    );
    ev.eval_str(&defun).expect("defun sm-work");
    alloc_probe::reset();
    let bc_t0 = std::time::Instant::now();
    for _ in 0..3 {
        ev.eval_str(&defun).expect("re-defun sm-work");
        ev.eval_str("(progn (byte-compile 'sm-work) t)")
            .expect("byte-compile sm-work");
    }
    let bc_secs = bc_t0.elapsed().as_secs_f64();
    let bc_report = alloc_probe::report();

    panic!(
        "ALLOC CLASS PROFILE (profiling aid, not a failure) config={}\n\
         === phase 1: startup (bootstrap load) ===\n{startup_report}\
         === phase 2: mixed churn (drain-kinds recipe, 100x200 iters, {churn_secs:.2}s) ===\n{churn_report}\
         === phase 3: byte-compile x3 ({bc_secs:.2}s) ===\n{bc_report}",
        if pdump {
            "pdump(mapped-dump)"
        } else {
            "plain(dump-less)"
        },
    );
}

/// Non-cons allocation-class profile, dump-partitioned config. Run:
///   cargo nextest run -p neovm-core --release --run-ignored ignored-only \
///     --no-capture -E 'test(alloc_class_profile_pdump)'
#[test]
#[ignore = "profiling aid; run explicitly in release with --no-capture"]
fn alloc_class_profile_pdump() {
    alloc_class_profile(true);
}

/// Non-cons allocation-class profile, dump-less config (the live bootstrap =
/// full expanded-cache replay). Run:
///   cargo nextest run -p neovm-core --release --run-ignored ignored-only \
///     --no-capture -E 'test(alloc_class_profile_plain)'
#[test]
#[ignore = "profiling aid; run explicitly in release with --no-capture"]
fn alloc_class_profile_plain() {
    alloc_class_profile(false);
}

/// One phase of `alloc_probe_bytecode_hash_isolation`: (re)define the phase's
/// defun, warm it once (so the engine's tiering settles before counting),
/// then run 20 chunks under fresh `alloc_probe` counters.
#[allow(dead_code)]
fn bc_isolation_phase(ev: &mut Context, label: &str, defun: &str, call: &str, out: &mut String) {
    use crate::tagged::gc::alloc_probe;
    ev.eval_str(defun).expect("phase defun");
    ev.eval_str(call).expect("phase warm");
    alloc_probe::reset();
    for _ in 0..20 {
        ev.eval_str(call).expect("phase step");
    }
    out.push_str(&format!(
        "=== {label} (20x200 iters) ===\n{}\n",
        alloc_probe::report()
    ));
}

/// Which construct in the drain-kinds churn recipe allocates `ByteCodeObj`?
/// (Follow-up to `alloc_class_profile_*`, where ~2 ByteCode allocations per
/// churn iteration appeared, 360B fixed each — suspicious next to the 2
/// `puthash` calls.) Phases isolate puthash (hoisted table), per-iteration
/// make-hash-table + puthash, the per-iteration lambda, the non-hash rest,
/// and the full recipe; then Rust backtraces are captured for the first
/// ByteCode-kind allocations of a short full-recipe run (needs debug info —
/// run WITHOUT --release for symbolized traces).
///
/// VERDICT (2026-07, adjudicated with this probe): hash ops are innocent —
/// puthash/make-hash-table allocate ZERO ByteCodeObj (phases A/B). The 2
/// ByteCode per churn iteration come from the per-iteration INTERPRETED
/// `(lambda (q) (cons q s))` (phase C: 2 ByteCode + 2 String + 1 Lambda per
/// evaluation): under lexical binding with a non-empty lexenv, `sf_lambda` →
/// `internal-make-interpreted-closure-function` (GNU loadup.el wiring) →
/// byte-compiled `cconv-make-interpreted-closure`, whose implementation
/// allocates two free-var-capturing closures per call via `make-closure` —
/// `(lambda (fv) (assq fv env))` in cconv-make-interpreted-closure and
/// `(lambda (var) (car (memq var dynvars)))` in `cconv-fv` (cconv.el). This
/// is GNU-parity behavior of upstream cconv.el, not engine waste; compiled
/// callers pay one `make-closure` per lambda and skip cconv entirely. Run:
///   cargo nextest run -p neovm-core --run-ignored ignored-only \
///     --no-capture -E 'test(alloc_probe_bytecode_hash_isolation)'
#[test]
#[ignore = "profiling aid; run explicitly with --no-capture"]
fn alloc_probe_bytecode_hash_isolation() {
    use crate::tagged::gc::alloc_probe;
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    ev.set_lexical_binding(true);
    let mut out = String::new();

    bc_isolation_phase(
        &mut ev,
        "A: puthash-only (hoisted eq table, int keys/values)",
        "(progn (defvar bcp--h (make-hash-table :test 'eq :size 8)) \
           (defun bcp-a (n) (let ((k 0)) (while (< k n) \
             (puthash 0 1 bcp--h) (puthash 1 2 bcp--h) (setq k (1+ k)))) nil) t)",
        "(bcp-a 200)",
        &mut out,
    );
    bc_isolation_phase(
        &mut ev,
        "B: make-hash-table + puthash (per-iteration table, int keys/values)",
        "(progn (defun bcp-b (n) (let ((k 0)) (while (< k n) \
             (let ((h (make-hash-table :test 'eq :size 8))) \
               (puthash 0 1 h) (puthash 1 2 h)) (setq k (1+ k)))) nil) t)",
        "(bcp-b 200)",
        &mut out,
    );
    bc_isolation_phase(
        &mut ev,
        "C: lambda-only (per-iteration closure, no hash table)",
        "(progn (defun bcp-c (n) (let ((k 0) (s \"x\")) (while (< k n) \
             (let ((c (lambda (q) (cons q s)))) (ignore c)) (setq k (1+ k)))) nil) t)",
        "(bcp-c 200)",
        &mut out,
    );
    bc_isolation_phase(
        &mut ev,
        "D: rest (string/list/vector/record, no hash table, no lambda)",
        "(progn (defun bcp-d (n) (let ((k 0)) (while (< k n) \
             (let* ((s (make-string 64 ?s)) (l (make-list 32 k)) \
                    (v (make-vector 24 s)) (r (record 'drain-probe s l v))) \
               (ignore r)) (setq k (1+ k)))) nil) t)",
        "(bcp-d 200)",
        &mut out,
    );
    bc_isolation_phase(
        &mut ev,
        "E: full drain-kinds body",
        "(progn (defun bcp-e (n) (let ((k 0)) (while (< k n) \
             (let* ((s (make-string 64 ?s)) (l (make-list 32 k)) \
                    (v (make-vector 24 s)) (r (record 'drain-probe s l v)) \
                    (h (make-hash-table :test 'eq :size 8)) \
                    (c (lambda (q) (cons q s)))) \
               (puthash 0 r h) (puthash 1 c h)) (setq k (1+ k)))) nil) t)",
        "(bcp-e 200)",
        &mut out,
    );

    // Call-chain evidence: capture backtraces for the first ByteCode-kind
    // allocations of a short full-recipe run.
    alloc_probe::arm_bytecode_backtraces(6);
    ev.eval_str("(bcp-e 3)").expect("backtrace step");
    let traces = alloc_probe::bytecode_backtraces();
    out.push_str(&format!(
        "=== ByteCode alloc backtraces captured during (bcp-e 3): {} ===\n",
        traces.len()
    ));
    for (i, t) in traces.iter().enumerate() {
        out.push_str(&format!("--- trace #{i} ---\n{t}\n"));
    }

    // Like the other profiling aids, report via panic! so the dump surfaces
    // under nextest's capture (NOT a failure).
    panic!("BYTECODE-ALLOC ISOLATION PROBE (profiling aid, not a failure)\n{out}");
}

/// Adversarial-review fix: setting a display-affecting variable (truncate-lines,
/// bidi-*, buffer-display-table, …) must advance the global display-var change
/// counter — the incremental fast paths key on it to escalate to a full rebuild,
/// since these change layout with no buffer/face/overlay tick.
#[test]
fn display_affecting_var_set_bumps_display_var_change_count() {
    let mut eval = Context::new();
    let before = eval.display_var_change_count;
    eval.mark_redisplay_dirty_if_display_var(intern("truncate-lines"));
    let after = eval.display_var_change_count;
    assert!(
        after > before,
        "a display-affecting var must bump display_var_change_count ({before} -> {after})"
    );
    eval.mark_redisplay_dirty_if_display_var(intern("neomacs--definitely-not-a-display-var"));
    assert_eq!(
        eval.display_var_change_count, after,
        "an ordinary variable must not bump the display-var counter"
    );
}

/// PS-T1 1a: `with_gc_inhibited` must rebalance `gc_inhibit_depth` when the
/// inhibited closure panics — the manual increment/decrement pair leaked the
/// increment on unwind, disabling safe-point GC for the rest of the session
/// once a caller catches the panic. `GcInhibitGuard` restores it in `Drop`.
#[test]
fn gc_inhibit_depth_rebalances_when_inhibited_closure_panics() {
    let mut eval = Context::new();
    assert_eq!(eval.gc_inhibit_depth, 0);

    // Normal path: nesting is visible inside, fully unwound after.
    let observed = eval.with_gc_inhibited(|outer| {
        let outer_depth = outer.gc_inhibit_depth;
        let inner_depth = outer.with_gc_inhibited(|inner| inner.gc_inhibit_depth);
        (outer_depth, inner_depth)
    });
    assert_eq!(observed, (1, 2));
    assert_eq!(eval.gc_inhibit_depth, 0);

    // Panic path: the closure unwinds out of the inhibited scope.
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eval.with_gc_inhibited(|_| panic!("boom inside gc-inhibited scope"));
    }));
    assert!(panicked.is_err());
    assert_eq!(
        eval.gc_inhibit_depth, 0,
        "a panicking inhibited closure must not leave GC inhibited"
    );
}

/// PS-T1 1b: `UnwindCleanupGuard` must rebalance `unwind_cleanup_depth` on
/// unwind, or a panicking `unwind-protect` cleanup body would permanently
/// suppress `throw-on-input` polling once panics become catchable.
#[test]
fn unwind_cleanup_depth_rebalances_when_guard_scope_panics() {
    let mut eval = Context::new();
    assert_eq!(eval.unwind_cleanup_depth, 0);

    // Normal path: nested guards count up and fully unwind.
    {
        let mut outer = UnwindCleanupGuard::enter(&mut eval);
        assert_eq!(outer.context().unwind_cleanup_depth, 1);
        {
            let mut inner = UnwindCleanupGuard::enter(outer.context());
            assert_eq!(inner.context().unwind_cleanup_depth, 2);
        }
        assert_eq!(outer.context().unwind_cleanup_depth, 1);
    }
    assert_eq!(eval.unwind_cleanup_depth, 0);

    // Panic path: the guard's scope unwinds.
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = UnwindCleanupGuard::enter(&mut eval);
        panic!("boom inside cleanup scope");
    }));
    assert!(panicked.is_err());
    assert_eq!(
        eval.unwind_cleanup_depth, 0,
        "a panicking cleanup scope must not leave throw-on-input suppressed"
    );
}

#[test]
fn eval_task_drain_runs_queued_closures_in_order() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut ctx = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded::<crate::emacs_core::eval::EvalThreadTask>();
    ctx.init_eval_task_system(rx);

    let counter = Arc::new(AtomicUsize::new(0));
    for _ in 0..3 {
        let c = counter.clone();
        tx.send(Box::new(move |_ctx: &mut Context| {
            // Runs synchronously on the Lisp thread when drained.
            c.fetch_add(1, Ordering::Relaxed);
        }))
        .unwrap();
    }
    // Nothing runs until the safe-point drain.
    assert_eq!(counter.load(Ordering::Relaxed), 0);
    ctx.drain_eval_tasks();
    assert_eq!(counter.load(Ordering::Relaxed), 3);
    // Draining again with no queued tasks is a harmless no-op.
    ctx.drain_eval_tasks();
    assert_eq!(counter.load(Ordering::Relaxed), 3);
}

#[test]
fn render_uncaught_signal_backtrace_lists_live_frames_innermost_first() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.push_backtrace_frame(Value::symbol("outer-fn"), &[]);
    ev.push_backtrace_frame(Value::symbol("inner-fn"), &[Value::fixnum(42)]);
    let bt = ev.render_uncaught_signal_backtrace(64);
    assert!(bt.contains("(inner-fn 42)"), "inner frame with arg: {bt}");
    assert!(bt.contains("(outer-fn)"), "outer frame, no args: {bt}");
    // The most recently pushed (innermost) frame must come first.
    assert!(
        bt.find("inner-fn").unwrap() < bt.find("outer-fn").unwrap(),
        "innermost-first order: {bt}"
    );
    // `max_frames` truncates with an ellipsis marker.
    let truncated = ev.render_uncaught_signal_backtrace(1);
    assert!(truncated.contains("..."), "truncation marker: {truncated}");
}

#[test]
fn bytecode_backtraces_reference_the_live_caller_stack_for_every_arity() {
    let mut ev = Context::new();

    for nargs in 0..=3 {
        let args_start = ev.bc_buf.len();
        ev.bc_buf
            .extend((0..nargs).map(|arg| Value::fixnum(arg as i64 + 10)));
        let backtrace_base = ev.specpdl.len();
        let backtrace =
            ev.push_backtrace_frame_from_bc_stack(Value::symbol("callee"), args_start, nargs);
        assert_eq!(backtrace.base_for_test(), backtrace_base);
        assert_eq!(
            backtrace.word_for_test(),
            backtrace_base,
            "the ordinary bytecode-call token must be the raw specpdl base so its hot pop needs no decode"
        );

        let args = match ev.specpdl.last() {
            Some(SpecBinding::Backtrace { args, .. }) => args,
            other => panic!("expected bytecode backtrace frame, got {other:?}"),
        };
        assert!(
            matches!(
                args.view(),
                BacktraceArgsView::EvaluatedBcStack(span)
                    if span.start() == args_start && span.len() == nargs
            ),
            "GNU Bcall keeps every arity as a pointer into the live caller stack: {args:?}"
        );
        assert_eq!(
            ev.backtrace_args_values(args),
            (0..nargs)
                .map(|arg| Value::fixnum(arg as i64 + 10))
                .collect::<LispArgVec>()
        );

        assert!(matches!(
            ev.pop_fast_bytecode_backtrace_frame(backtrace),
            crate::emacs_core::eval::FastBytecodePop::Popped
        ));
        ev.bc_buf.truncate(args_start);
    }
}

#[test]
fn generic_backtraces_keep_one_and_two_arguments_inline() {
    let mut ev = Context::new();
    let owned_base = ev.backtrace_args_stack.len();

    ev.push_backtrace_frame(Value::symbol("one"), &[Value::fixnum(11)]);
    assert!(matches!(
        ev.specpdl.last(),
        Some(SpecBinding::Backtrace1 {
            arg: value,
            debug_on_exit: false,
            ..
        }) if *value == Value::fixnum(11)
    ));
    assert_eq!(ev.backtrace_args_stack.len(), owned_base);
    ev.unbind_to(0);

    ev.push_backtrace_frame(
        Value::symbol("two"),
        &[Value::fixnum(21), Value::fixnum(22)],
    );
    assert!(matches!(
        ev.specpdl.last(),
        Some(SpecBinding::Backtrace2 {
            arg0,
            arg1,
            ..
        }) if *arg0 == Value::fixnum(21) && *arg1 == Value::fixnum(22)
    ));
    assert_eq!(ev.backtrace_args_stack.len(), owned_base);
    ev.unbind_to(0);

    let frame_base = ev.specpdl.len();
    ev.push_backtrace_frame(
        Value::symbol("three"),
        &[Value::fixnum(31), Value::fixnum(32), Value::fixnum(33)],
    );
    assert_eq!(ev.backtrace_args_stack.len(), owned_base + 1);
    let result = ev.unbind_to_with_result(frame_base, Ok(Value::fixnum(34)));
    assert_eq!(
        result.expect("trivial pop preserves success"),
        Value::fixnum(34)
    );
    assert_eq!(ev.specpdl.len(), frame_base);
    assert_eq!(
        ev.backtrace_args_stack.len(),
        owned_base,
        "the trivial pointer-decrement pop must still release owned variadic arguments"
    );
}

#[test]
#[should_panic(expected = "fast bytecode pop requires its frame to remain the specpdl top")]
fn bytecode_backtrace_token_rejects_an_unbalanced_fast_pop() {
    let mut ev = Context::new();
    let backtrace = ev.push_backtrace_frame_from_bc_stack(Value::symbol("callee"), 0, 0);
    ev.push_specpdl_root(Value::T);
    let _ = ev.pop_fast_bytecode_backtrace_frame(backtrace);
}

#[test]
fn bytecode_backtrace_span_is_checked_and_round_trips_both_indices() {
    let span = BytecodeBacktraceSpan::try_new(
        BytecodeBacktraceSpan::START_MAX,
        BytecodeBacktraceSpan::LEN_MASK,
    )
    .expect("values that fit in the packed representation");
    assert_eq!(span.start(), BytecodeBacktraceSpan::START_MAX);
    assert_eq!(span.len(), BytecodeBacktraceSpan::LEN_MASK);

    assert!(BytecodeBacktraceSpan::try_new(BytecodeBacktraceSpan::START_MAX + 1, 0).is_none());
    assert!(BytecodeBacktraceSpan::try_new(0, BytecodeBacktraceSpan::LEN_MASK + 1).is_none());
}

#[test]
fn compact_saved_binding_options_round_trip_none_and_live_values() {
    let value = Value::fixnum(42);
    assert_eq!(SavedBindingValue::from_option(None).get(), None);
    assert!(
        SavedBindingValue::from_option(Some(value))
            .get()
            .is_some_and(|saved| saved.bits() == value.bits())
    );

    let buffer_id = crate::buffer::BufferId(7);
    assert_eq!(SavedBufferId::from_option(None).get(), None);
    assert_eq!(
        SavedBufferId::from_option(Some(buffer_id)).get(),
        Some(buffer_id)
    );
}

/// GNU's `union specbinding` is 32 bytes on the supported 64-bit Unix build.
/// A bytecode call pushes one of these entries and Breturn immediately pops
/// it, so matching GNU's four-word stride is part of the hot call protocol.
#[test]
fn specpdl_entry_stays_compact_for_hot_backtrace_pushes() {
    let entry_size = std::mem::size_of::<SpecBinding>();

    assert_eq!(std::mem::size_of::<BacktraceArgs>(), 8);
    assert_eq!(std::mem::size_of::<BytecodeBacktraceFrame>(), 8);
    assert_eq!(std::mem::size_of::<SavedBindingValue>(), 8);
    assert_eq!(std::mem::size_of::<SavedBufferId>(), 8);
    assert_eq!(
        entry_size, 32,
        "SpecBinding is {entry_size} bytes; GNU's hot specpdl stride is 32 bytes"
    );
}

// ---------------------------------------------------------------------------
// Redisplay skip: a change that lands DURING the paint must still be painted
// ---------------------------------------------------------------------------

#[test]
fn a_buffer_change_during_redisplay_is_not_recorded_as_already_displayed() {
    // Found driving vterm in the GUI: after a burst of terminal output the
    // buffer held the complete text while the window kept showing the previous
    // frame, and every later redisplay logged "skipped: visible state
    // unchanged" -- for as long as nothing else forced a repaint.
    //
    // The skip compares the CURRENT visible state against the last one, so the
    // recorded value must be what was actually PAINTED. Re-reading the state
    // after the paint records changes the paint never showed -- a process
    // filter running inside the layout pass is exactly that case -- and every
    // later comparison then matches, so the change is never drawn.
    //
    // GNU cannot lose an update this way: `redisplay_internal` compares each
    // window's `last_modified` against `BUF_MODIFF (b)`, and `last_modified` is
    // assigned from the buffer state the window was laid out FROM
    // (src/xdisp.c:18269), never from the state after the fact.
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*redisplay-race*");
    assert!(ev.buffers.switch_current(buffer_id));
    let frame_id = ev.frames.create_frame("F1", 80, 25, buffer_id);
    ev.frames.select_frame(frame_id);

    // A "process filter" that appends to the displayed buffer while the frame
    // is being laid out.
    let painted = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let painted_in_cb = painted.clone();
    ev.redisplay_fn = Some(Box::new(move |ev: &mut Context| {
        let current = ev.buffers.current_buffer_id().expect("current buffer");
        painted_in_cb
            .borrow_mut()
            .push(ev.buffers.get(current).expect("buffer").buffer_string());
        // Output arrives mid-paint: it is NOT part of the frame just drawn.
        ev.buffers.insert_lisp_string_into_buffer(
            current,
            &crate::heap_types::LispString::from_utf8("late output\n"),
        );
    }));

    ev.redisplay();
    let after_first = painted.borrow().len();
    assert_eq!(after_first, 1, "first redisplay should paint");

    // The text inserted during the paint was never on screen, so the next
    // redisplay must run rather than conclude nothing changed.
    ev.redisplay();
    assert_eq!(
        painted.borrow().len(),
        2,
        "a change that landed during the paint must still be painted; \
         it was recorded as already displayed and the frame went stale"
    );
    assert!(
        painted.borrow()[1].contains("late output"),
        "the second paint should see the text inserted during the first: {:?}",
        painted.borrow()[1]
    );
}

#[test]
fn command_loop_exit_classifies_thrown_value_by_type_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    // GNU `recursive_edit_1' (keyboard.c:749-758) dispatches on the thrown
    // value's type, in this order: EQ t, then STRINGP, then FUNCTIONP.
    // Collapsing that into a truthiness test is what made every minibuffer
    // abort surface as a plain `quit'.
    assert_eq!(
        eval.classify_command_loop_exit(Value::T).unwrap(),
        CommandLoopExit::Quit,
        "`abort-recursive-edit' throws t and must stay a plain quit"
    );
    assert_eq!(
        eval.classify_command_loop_exit(Value::NIL).unwrap(),
        CommandLoopExit::Normal,
        "`exit-recursive-edit' throws nil and returns normally"
    );

    let message = Value::string("Cross-window minibuffer abort");
    assert_eq!(
        eval.classify_command_loop_exit(message).unwrap(),
        CommandLoopExit::Error(message),
        "read_minibuf (minibuf.c:646) throws a string to be re-signaled as `error'"
    );

    let thunk = eval
        .eval_str("(lambda () (signal 'minibuffer-quit nil))")
        .expect("thunk should evaluate");
    assert_eq!(
        eval.classify_command_loop_exit(thunk).unwrap(),
        CommandLoopExit::Call(thunk),
        "a thrown function must be called, not collapsed into a plain quit"
    );
}

/// GNU's `Fkill_emacs` (`src/emacs.c:2954-3088`) is declared
/// `attributes: noreturn` and its body ends in `exit (exit_code)`, with only
/// `safe_run_hooks (Qkill_emacs_hook)` and `shut_down_emacs` in between.  It
/// never touches the specpdl, so **every `unwind-protect` cleanup form still
/// on the stack is abandoned**: GNU's exit is an `exit(2)`, not an unwind.
///
/// This port cannot exit from inside an FFI call the way GNU does -- the
/// evaluator has to hand control back to `main` -- so `kill-emacs` returns
/// `Flow::Shutdown` and the Rust stack unwinds.  The specpdl drain that rides
/// along with that unwind must not evaluate the Lisp cleanup forms, or the
/// port keeps running Lisp after the point where GNU has already exited.
#[test]
fn kill_emacs_abandons_unwind_protect_cleanup_forms_like_gnus_noreturn_exit() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let flow = eval
        .eval_str(
            "(progn (setq l203-cleanup-ran nil)
                    (unwind-protect (kill-emacs 7)
                      (setq l203-cleanup-ran t)))",
        )
        .expect_err("kill-emacs is control flow and never returns a value");

    assert!(
        matches!(flow, crate::emacs_core::error::EvalError::Shutdown(request) if request.exit_code == 7 && !request.restart),
        "kill-emacs must propagate its own shutdown request, got {flow:?}"
    );
    assert_eq!(
        eval.obarray().symbol_value("l203-cleanup-ran"),
        Some(&Value::NIL),
        "GNU's Fkill_emacs is `noreturn`: the cleanup form is abandoned, not run"
    );
    assert_eq!(
        eval.shutdown_request(),
        Some(crate::emacs_core::eval::ShutdownRequest {
            exit_code: 7,
            restart: false,
        })
    );
}

/// The same contract nested, and with the cleanups reading back in order.
///
/// `lisp/startup.el:784-818` (GNU `:774-808`) wraps the whole of `command-line` in exactly this
/// shape -- `(unwind-protect (command-line) ... (run-hooks 'emacs-startup-hook
/// 'term-setup-hook) ...)` -- and `command-line` ends every batch session with
/// `(if noninteractive (kill-emacs t))` at `:1757` (GNU `:1739`).  A port that runs that
/// cleanup runs `emacs-startup-hook` in `--batch`, which GNU never does.
#[test]
fn kill_emacs_abandons_nested_unwind_protect_cleanups() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let flow = eval
        .eval_str(
            "(progn (setq l203-order nil)
                    (unwind-protect
                        (unwind-protect
                            (progn (setq l203-order (cons 'body l203-order))
                                   (kill-emacs 0))
                          (setq l203-order (cons 'inner l203-order)))
                      (setq l203-order (cons 'outer l203-order))))",
        )
        .expect_err("kill-emacs is control flow and never returns a value");

    assert!(
        matches!(flow, crate::emacs_core::error::EvalError::Shutdown(_)),
        "got {flow:?}"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("l203-order")),
        "OK (body)",
        "neither cleanup may run: GNU has already exited by then"
    );
}

/// A `let` binding whose scope the shutdown unwinds through is still restored.
///
/// GNU abandons those too, but nothing Lisp-visible can observe the difference
/// -- `kill-emacs-hook` has already run by the time `Fkill_emacs` exits and no
/// Lisp runs after it.  Restoring them keeps this port's own dynamic-binding
/// bookkeeping consistent for the shutdown path that still has to walk back
/// out to `main`, so the narrowing is to Lisp *evaluation*, not to unwinding.
#[test]
fn kill_emacs_still_restores_dynamic_bindings_while_abandoning_cleanup_forms() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    eval.eval_str("(progn (setq l203-var 'outer-value) (setq l203-witness nil))")
        .expect("seed the dynamic variable and the cleanup witness");
    let flow = eval
        .eval_str(
            "(let ((l203-var 'inner-value))
               (unwind-protect (kill-emacs 3)
                 (setq l203-witness l203-var)))",
        )
        .expect_err("kill-emacs is control flow and never returns a value");

    assert!(
        matches!(flow, crate::emacs_core::error::EvalError::Shutdown(request) if request.exit_code == 3),
        "got {flow:?}"
    );
    // The control half: this one is green before the fix as well as after, and
    // it is here so a fix that abandoned the whole unwind instead of only the
    // Lisp evaluation would be caught.
    assert_eq!(
        format_eval_result(&eval.eval_str("l203-var")),
        "OK outer-value",
        "the `let` binding is still unwound: the narrowing is to Lisp evaluation, not to unwinding"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("l203-witness")),
        "OK nil",
        "the cleanup form never ran, so it never observed the inner binding"
    );
}

/// `kill-emacs-hook` still gets a working `unwind-protect`.
///
/// GNU runs the hook from *inside* `Fkill_emacs` (`safe_run_hooks
/// (Qkill_emacs_hook)`, src/emacs.c:3015-3021) and only then calls
/// `shut_down_emacs` and `exit`, so a cleanup form registered by the hook
/// itself is on the specpdl BELOW the point of no return and runs normally.
/// This port matches that by running the hook before it records the
/// [`ShutdownRequest`](crate::emacs_core::eval::ShutdownRequest) -- so the
/// narrowing in `LispExecution` starts exactly where GNU's `exit` does, not
/// one call earlier.
#[test]
fn kill_emacs_hook_still_runs_its_own_unwind_protect_cleanups() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    eval.eval_str(
        "(progn
           (setq l203-hook-order nil)
           (setq kill-emacs-hook
                 (list (lambda ()
                         (unwind-protect
                             (setq l203-hook-order
                                   (cons 'hook-body l203-hook-order))
                           (setq l203-hook-order
                                 (cons 'hook-cleanup l203-hook-order)))))))",
    )
    .expect("install the kill-emacs-hook");

    let flow = eval
        .eval_str(
            "(unwind-protect (kill-emacs 0)
               (setq l203-hook-order (cons 'outer-cleanup l203-hook-order)))",
        )
        .expect_err("kill-emacs is control flow and never returns a value");

    assert!(
        matches!(
            flow,
            crate::emacs_core::error::EvalError::Shutdown(request) if request.exit_code == 0
        ),
        "got {flow:?}"
    );
    assert_eq!(
        format_eval_result(&eval.eval_str("(reverse l203-hook-order)")),
        "OK (hook-body hook-cleanup)",
        "the hook's own cleanup runs (GNU is still inside Fkill_emacs); \
         the caller's does not (GNU has exited)"
    );
}

/// Build a hot nullary `(+ 40 2)` bytecode function, fset it (so it stays
/// reachable across a forced GC independent of stack scanning), run it once
/// through the seam so it tiers up and lands in the compiled-leaf cache.
#[cfg(feature = "jit")]
fn jit_cached_leaf_fixture(ev: &mut Context, name: &str) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::LambdaParams;
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.ops = vec![Op::Constant(0), Op::Constant(1), Op::Add, Op::Return];
    f.constants = vec![Value::make_int(40), Value::make_int(2)].into();
    f.max_stack = 16;
    f.jit_runtime().set_hot_for_test();
    let fv = Value::make_bytecode(f);
    crate::emacs_core::builtins::builtin_fset(ev, vec![Value::symbol(name), fv])
        .expect("fset fixture");
    assert_eq!(
        ev.funcall_general_untraced(fv, vec![]).unwrap(),
        Value::make_int(42)
    );
    assert!(
        crate::emacs_core::jit::cache::compiled_cache_probe().0 >= 1,
        "the hot leaf must be cached after its native run"
    );
    fv
}

/// The FIRST GC of a thread must not wipe the compiled-leaf cache. Regression
/// pin for the JIT-campaign entry crash (rr-proven): `sync_cache_to_current_heap`
/// treated its first observation of the heap identity (`None -> Some`) as a heap
/// CHANGE and `clear()`ed every leaf compiled so far — while one of them was
/// executing (its shim call had triggered the GC), freeing its reloc/spec boxes
/// under its own machine code. No heap was replaced, so nothing may be dropped.
#[cfg(feature = "jit")]
#[test]
fn jit_first_gc_keeps_compiled_leaves() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fv = jit_cached_leaf_fixture(&mut ev, "jit-first-gc-probe-fn");
    let (before, _) = crate::emacs_core::jit::cache::compiled_cache_probe();

    ev.gc_collect_exact();

    let (after, _) = crate::emacs_core::jit::cache::compiled_cache_probe();
    assert_eq!(
        after, before,
        "a GC without a heap replacement must not clear the compiled-leaf cache"
    );
    // The retained leaf still serves the call, and nothing is recompiled.
    assert_eq!(
        ev.funcall_general_untraced(fv, vec![]).unwrap(),
        Value::make_int(42)
    );
    assert_eq!(
        crate::emacs_core::jit::cache::compiled_cache_probe().0,
        before
    );
}

/// The genuine-change path is unchanged: replacing the thread's tagged heap
/// (what an in-process image reload does) drops every cached leaf on the next
/// GC-root walk, and roots nothing from them.
#[cfg(feature = "jit")]
#[test]
fn jit_heap_swap_clears_compiled_leaves() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let _fv = jit_cached_leaf_fixture(&mut ev, "jit-heap-swap-probe-fn");
    assert!(crate::emacs_core::jit::cache::compiled_cache_probe().0 >= 1);

    let mut other = crate::tagged::gc::TaggedHeap::new();
    crate::tagged::gc::set_tagged_heap(&mut other);
    let mut roots = Vec::new();
    crate::emacs_core::jit::cache::collect_jit_reloc_gc_roots(&mut roots);
    assert_eq!(
        crate::emacs_core::jit::cache::compiled_cache_probe().0,
        0,
        "a heap identity change must clear the compiled-leaf cache"
    );
    assert!(
        roots.is_empty(),
        "no reloc root may be traced from a cleared cache"
    );
    // Restore the evaluator's own heap before it is dropped.
    crate::tagged::gc::set_tagged_heap(&mut ev.tagged_heap);
}
/// A `make-closure` PROTOTYPE shaped like `byte-compile-make-closure`'s: the
/// first `placeholders` constant slots hold the symbols `V0..`, patched per
/// instance by `make-closure`; `tail` follows.
#[cfg(feature = "jit")]
fn jit_closure_prototype(
    ops: Vec<crate::emacs_core::bytecode::opcode::Op>,
    placeholders: usize,
    tail: Vec<Value>,
) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::value::LambdaParams;
    let mut consts: Vec<Value> = (0..placeholders)
        .map(|i| Value::symbol(&format!("V{i}")))
        .collect();
    consts.extend(tail);
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = consts.into();
    f.max_stack = 16;
    Value::make_bytecode(f)
}

#[cfg(feature = "jit")]
fn jit_make_closure(proto: Value, vars: &[Value]) -> Value {
    let mut args = vec![proto];
    args.extend_from_slice(vars);
    crate::emacs_core::builtins::symbols::builtin_make_closure(args).expect("make-closure")
}

#[cfg(feature = "jit")]
fn jit_compiled_id(v: Value) -> Option<u64> {
    v.get_bytecode_data()
        .expect("bytecode")
        .jit_runtime()
        .compiled_id()
}

/// `make-closure` instances of one prototype share ONE tiering state and ONE
/// native leaf, and each instance still reads ITS OWN captured value: the
/// patched prefix is loaded through the executing callee, never baked from
/// the instance that happened to trigger the compile.
#[cfg(feature = "jit")]
#[test]
fn jit_make_closure_instances_share_one_leaf_and_read_their_own_captures() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::opcode::Op;
    let mut ev = Context::new();
    let proto = jit_closure_prototype(vec![Op::Constant(0), Op::Return], 1, vec![]);
    let a = jit_make_closure(proto, &[Value::make_int(11)]);
    let b = jit_make_closure(proto, &[Value::make_int(22)]);
    for (name, v) in [
        ("jit-share-proto", proto),
        ("jit-share-a", a),
        ("jit-share-b", b),
    ] {
        crate::emacs_core::builtins::builtin_fset(&mut ev, vec![Value::symbol(name), v])
            .expect("fset");
    }
    let rt_a = a.get_bytecode_data().expect("bytecode").jit_runtime();
    assert_eq!(
        rt_a.patched_prefix(),
        1,
        "make-closure records its patch width"
    );
    // Heat is shared: warming ONE instance warms the source.
    rt_a.set_hot_for_test();
    assert!(
        b.get_bytecode_data()
            .expect("bytecode")
            .jit_runtime()
            .is_hot()
    );

    let before = crate::emacs_core::jit::cache::compiled_cache_probe().0;
    assert_eq!(
        ev.funcall_general_untraced(a, vec![]).unwrap(),
        Value::make_int(11)
    );
    let after_a = crate::emacs_core::jit::cache::compiled_cache_probe().0;
    assert_eq!(
        after_a,
        before + 1,
        "the first instance compiles the shared leaf"
    );
    assert_eq!(
        ev.funcall_general_untraced(b, vec![]).unwrap(),
        Value::make_int(22),
        "the second instance must read its own capture, not the compile-time instance's"
    );
    assert_eq!(
        crate::emacs_core::jit::cache::compiled_cache_probe().0,
        after_a,
        "the second instance reuses the leaf — no per-instance compile"
    );
    let id = jit_compiled_id(a).expect("compiled id assigned");
    assert_eq!(jit_compiled_id(b), Some(id), "one compiled id per source");
    assert_eq!(jit_compiled_id(proto), Some(id));
    assert!(
        crate::emacs_core::jit::cache::is_compiled_for_test(id),
        "the shared entry is a real native leaf, not an interpreter fallback"
    );
}

/// A leaf compiled BEFORE the source was ever `make-closure`d (prefix 0, the
/// `V0` placeholder baked as a symbol) must be evicted when an instance widens
/// the prefix; the instance then runs a leaf that loads the slot live.
#[cfg(feature = "jit")]
#[test]
fn jit_make_closure_widening_evicts_leaf_compiled_under_narrower_prefix() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::opcode::Op;
    let mut ev = Context::new();
    let proto = jit_closure_prototype(vec![Op::Constant(0), Op::Return], 1, vec![]);
    crate::emacs_core::builtins::builtin_fset(
        &mut ev,
        vec![Value::symbol("jit-widen-proto"), proto],
    )
    .expect("fset");
    proto
        .get_bytecode_data()
        .expect("bytecode")
        .jit_runtime()
        .set_hot_for_test();
    // The prototype itself, called as a plain function, returns the placeholder.
    assert_eq!(
        ev.funcall_general_untraced(proto, vec![]).unwrap(),
        Value::symbol("V0")
    );
    assert!(
        jit_compiled_id(proto).is_some(),
        "the prototype compiled (prefix 0)"
    );

    let inst = jit_make_closure(proto, &[Value::make_int(7)]);
    crate::emacs_core::builtins::builtin_fset(&mut ev, vec![Value::symbol("jit-widen-inst"), inst])
        .expect("fset");
    assert_eq!(
        inst.get_bytecode_data()
            .expect("bytecode")
            .jit_runtime()
            .patched_prefix(),
        1
    );
    assert_eq!(
        ev.funcall_general_untraced(inst, vec![]).unwrap(),
        Value::make_int(7),
        "the prefix-0 leaf (baked V0) must have been evicted"
    );
    assert!(
        crate::emacs_core::jit::cache::is_compiled_for_test(jit_compiled_id(inst).unwrap()),
        "the instance runs a (re)compiled native leaf"
    );
    // And the prototype keeps working through the widened (live-loading) leaf.
    assert_eq!(
        ev.funcall_general_untraced(proto, vec![]).unwrap(),
        Value::symbol("V0")
    );
}

/// A captured CALLEE (the classic `(funcall captured-fn)` shape, `Constant(0)
/// Call(0)`) is per-instance: the shared leaf must not speculate on — or bake —
/// whatever the compile-time instance had captured.
#[cfg(feature = "jit")]
#[test]
fn jit_make_closure_captured_callee_is_not_speculated_across_instances() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::opcode::Op;
    let mut ev = Context::new();
    // Call-dominated body: lift the profitability gate so it tiers at all.
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mk_const = |n: i64| {
        jit_closure_prototype(
            vec![Op::Constant(0), Op::Return],
            0,
            vec![Value::make_int(n)],
        )
    };
    let f1 = mk_const(100);
    let f2 = mk_const(200);
    crate::emacs_core::builtins::builtin_fset(&mut ev, vec![Value::symbol("jit-cap-f1"), f1])
        .expect("fset");
    crate::emacs_core::builtins::builtin_fset(&mut ev, vec![Value::symbol("jit-cap-f2"), f2])
        .expect("fset");
    let proto = jit_closure_prototype(vec![Op::Constant(0), Op::Call(0), Op::Return], 1, vec![]);
    // Capture SYMBOLS (the shape direct-call speculation keys on).
    let a = jit_make_closure(proto, &[Value::symbol("jit-cap-f1")]);
    let b = jit_make_closure(proto, &[Value::symbol("jit-cap-f2")]);
    crate::emacs_core::builtins::builtin_fset(&mut ev, vec![Value::symbol("jit-cap-a"), a])
        .expect("fset");
    crate::emacs_core::builtins::builtin_fset(&mut ev, vec![Value::symbol("jit-cap-b"), b])
        .expect("fset");
    a.get_bytecode_data()
        .expect("bytecode")
        .jit_runtime()
        .set_hot_for_test();
    assert_eq!(
        ev.funcall_general_untraced(a, vec![]).unwrap(),
        Value::make_int(100)
    );
    assert!(
        crate::emacs_core::jit::cache::is_compiled_for_test(jit_compiled_id(a).unwrap()),
        "the call-bearing body compiled natively (profit gate lifted)"
    );
    assert_eq!(
        ev.funcall_general_untraced(b, vec![]).unwrap(),
        Value::make_int(200),
        "instance b must call ITS captured callee through the shared leaf"
    );
    assert_eq!(
        ev.funcall_general_untraced(a, vec![]).unwrap(),
        Value::make_int(100)
    );
    crate::emacs_core::jit::compile::force_profit_gate_for_test(true);
}

/// `gc-cons-threshold` is a forwarded int; a plain `setq` of it must be honored
/// at the next GC decision exactly as GNU reads the C variable live at
/// `garbage_collect` time. Regression pin: the settings cache used to be
/// refreshed only by one setter path, so `--batch` scripts raising the
/// threshold still collected every ~1.4 MB (GNU: never, for this workload).
#[test]
fn gc_cons_threshold_setq_is_honored_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    ev.eval_str("(setq gc-cons-threshold 400000000)")
        .expect("setq");
    // GNU applies a changed threshold at the next GC; make that GC happen now.
    ev.eval_str("(garbage-collect)").expect("gc");
    let before = ev
        .eval_str("gcs-done")
        .expect("gcs-done")
        .as_fixnum()
        .expect("fixnum");
    // ~16 MB of conses: far above the default 800 KB threshold, far below the
    // raised one. GNU runs zero collections here.
    ev.eval_str("(dotimes (_ 1000000) (cons 1 2))")
        .expect("cons loop");
    let after = ev
        .eval_str("gcs-done")
        .expect("gcs-done")
        .as_fixnum()
        .expect("fixnum");
    assert_eq!(
        after - before,
        0,
        "a raised gc-cons-threshold must suppress collections (GNU parity)"
    );
    // And lowering it again takes effect (the raise was not a one-way pin).
    ev.eval_str("(setq gc-cons-threshold 800000)")
        .expect("setq");
    ev.eval_str("(dotimes (_ 1000000) (cons 1 2))")
        .expect("cons loop");
    let lowered = ev
        .eval_str("gcs-done")
        .expect("gcs-done")
        .as_fixnum()
        .expect("fixnum");
    assert!(lowered > after, "the default threshold collects again");
}
#[test]
fn lisp_navigation_intent_boundary_records_typed_window_and_frame_scope() {
    let mut ev = Context::new();
    let buffer = ev
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame = ev
        .frame_manager_mut()
        .create_frame("navigation-intent", 80, 24, buffer);
    let window = ev
        .frame_manager()
        .get(frame)
        .expect("frame")
        .selected_window;

    ev.eval_str("(neomacs--record-window-navigation-intent 'backward)")
        .expect("record selected-window intent");
    ev.eval_str("(neomacs--record-frame-navigation-intent 'forward)")
        .expect("record selected-frame intent");

    assert_eq!(
        ev.frame_manager()
            .pending_window_navigation_intent(window)
            .map(|intent| intent.direction()),
        Some(neomacs_display_protocol::TransitionDirection::Backward)
    );
    assert_eq!(
        ev.frame_manager()
            .pending_frame_navigation_intent(frame)
            .map(|intent| intent.direction()),
        Some(neomacs_display_protocol::TransitionDirection::Forward)
    );
}

/// `make-closure` instances share their prototype's deferred GNU decode: the
/// decoded IR depends only on the byte string, the constant-pool length and
/// the stack contract, all copied unchanged by the clone. Regression pin for
/// the per-instance re-decode (1,366 decodes = 8% of the type-sim window).
#[test]
fn make_closure_instances_share_the_deferred_gnu_decode() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    // A closure factory: every call instantiates the SAME prototype. Running
    // instance `a` once decodes the prototype's byte string (if deferred).
    ev.eval_str(
        r#"(progn
             (setq lexical-binding t)
             (setq neomacs--decode-share-f
                   (byte-compile '(lambda (k) (lambda (x) (+ x k)))))
             (setq neomacs--decode-share-a (funcall neomacs--decode-share-f 1))
             (setq neomacs--decode-share-b (funcall neomacs--decode-share-f 10))
             (setq neomacs--decode-share-c (funcall neomacs--decode-share-f 100))
             (funcall neomacs--decode-share-a 1))"#,
    )
    .expect("setup");
    let decodes_after_a = crate::emacs_core::bytecode::chunk::lazy_gnu_decode_count_for_test();
    let r = ev
        .eval_str("(list (funcall neomacs--decode-share-b 1) (funcall neomacs--decode-share-c 1))")
        .expect("calls");
    assert_eq!(r, Value::list(vec![Value::fixnum(11), Value::fixnum(101)]));
    assert_eq!(
        crate::emacs_core::bytecode::chunk::lazy_gnu_decode_count_for_test(),
        decodes_after_a,
        "instances b and c must reuse the prototype's decode, not re-decode"
    );
}

/// The echo-area buffer's multibyteness is GNU's, and only one variable can
/// make it unibyte.
///
/// `set_message_1` (src/xdisp.c:13588-13601):
/// ```c
///   if (!message_enable_multibyte
///       && unibyte_display_via_language_environment
///       && !NILP (BVAR (current_buffer, enable_multibyte_characters)))
///     Fset_buffer_multibyte (Qnil);
///   else if (NILP (BVAR (current_buffer, enable_multibyte_characters)))
///     Fset_buffer_multibyte (Qt);
/// ```
/// with `message_enable_multibyte = STRING_MULTIBYTE (string)` set by
/// `set_message` (src/xdisp.c:13568). GNU's own comment says why: "We always set
/// it to be multibyte, except when unibyte-display-via-language-environment is
/// non-nil ... because in that case unibyte characters should not be displayed
/// as octal escapes."
///
/// This port took the message string's own multibyteness as the buffer's, an
/// invented rule that ignores the variable entirely. Measured against GNU Emacs
/// 31.0.90 under a pty at 80x24 (`scripts/l217-echo-multibyte-probe.el'):
/// `enable-multibyte-characters` of ` *Echo Area 0*` is `t` in GNU and `nil`
/// here after every unibyte message -- which is every pure-ASCII message, since
/// ASCII string literals are unibyte. The mini-window is laid out from this
/// buffer, so its multibyteness decides how a raw byte reaches the screen.
#[test]
fn echo_area_buffer_multibyteness_follows_unibyte_display_via_language_environment() {
    let mut ev = Context::new();
    ev.ensure_echo_area_buffers();
    let id = ev
        .buffers
        .find_buffer_by_name(" *Echo Area 0*")
        .expect("echo-area buffer should exist");

    // Default: `unibyte-display-via-language-environment' is nil, so a UNIBYTE
    // message must still leave the echo buffer multibyte.
    let unibyte = crate::emacs_core::builtins::plain_str_to_lisp_string("plain ascii", false);
    ev.set_current_message(Some(unibyte));
    assert!(
        ev.buffers
            .get(id)
            .expect("echo buffer live")
            .get_multibyte(),
        "GNU forces the echo buffer multibyte for a unibyte message when \
         unibyte-display-via-language-environment is nil (src/xdisp.c:13595-13598)"
    );

    // A multibyte message keeps it multibyte.
    let multibyte = crate::emacs_core::builtins::plain_str_to_lisp_string("café", true);
    ev.set_current_message(Some(multibyte));
    assert!(
        ev.buffers
            .get(id)
            .expect("echo buffer live")
            .get_multibyte(),
        "a multibyte message leaves the echo buffer multibyte"
    );

    // Only this variable can make it unibyte, and only for a unibyte message.
    ev.obarray.set_symbol_value(
        "unibyte-display-via-language-environment",
        crate::emacs_core::value::Value::T,
    );
    let unibyte = crate::emacs_core::builtins::plain_str_to_lisp_string("plain ascii", false);
    ev.set_current_message(Some(unibyte));
    assert!(
        !ev.buffers
            .get(id)
            .expect("echo buffer live")
            .get_multibyte(),
        "with unibyte-display-via-language-environment non-nil GNU turns the \
         echo buffer unibyte for a unibyte message (src/xdisp.c:13592-13594)"
    );

    // ... and a multibyte message turns it back.
    let multibyte = crate::emacs_core::builtins::plain_str_to_lisp_string("café", true);
    ev.set_current_message(Some(multibyte));
    assert!(
        ev.buffers
            .get(id)
            .expect("echo buffer live")
            .get_multibyte(),
        "a multibyte message forces the echo buffer multibyte regardless of \
         unibyte-display-via-language-environment"
    );
}

/// `current-message` is what the echo-area buffer holds, so it is multibyte
/// whenever that buffer is.
///
/// GNU `current_message` (src/xdisp.c:13420-13433) does not keep the message
/// anywhere: it enters `with_echo_area_buffer` and reads the buffer back with
/// `make_buffer_string (BEG, Z, true)` (`current_message_1`,
/// src/xdisp.c:13437-13446). Since `set_message_1` has just forced that buffer
/// multibyte (src/xdisp.c:13595-13598), the round trip returns a MULTIBYTE
/// string -- including for an ASCII message, ASCII string literals being
/// unibyte.
///
/// Measured against GNU Emacs 31.0.90 under a pty at 80x24
/// (`scripts/l217-echo-multibyte-probe.el'), `(multibyte-string-p
/// (current-message))` after `(message "%s" "...")`:
///   GNU t, this port nil -- for a pure-ASCII message, a unibyte ASCII one, and
///   a unibyte one carrying raw bytes >= 128.
/// This port kept the message in a field and mirrored a COPY into the buffer,
/// so the field never saw the conversion the buffer got.
#[test]
fn current_message_is_the_text_the_echo_buffer_holds_like_gnu() {
    let mut ev = Context::new();
    ev.ensure_echo_area_buffers();

    for (label, text) in [
        ("pure ascii", "plain ascii message"),
        ("raw high bytes", "A\u{c8}\u{c9}B"),
    ] {
        let unibyte = crate::emacs_core::builtins::plain_str_to_lisp_string(text, false);
        ev.set_current_message(Some(unibyte));
        let value = ev
            .current_message_value()
            .expect("current-message should be set");
        let message = value
            .as_lisp_string()
            .expect("current-message should be a string");
        assert!(
            message.is_multibyte(),
            "current-message must come back multibyte for a {label} message, \
             because GNU reads it out of the multibyte echo buffer \
             (src/xdisp.c:13437-13446)"
        );
    }
}

/// `specbind` takes GNU's plain-value fast path (push + one store) for a
/// symbol whose value cell is `Plainval`; the binding must still be
/// visible inside and restored exactly afterwards — including "unbound".
#[test]
fn let_of_a_plain_symbol_restores_the_old_value_and_unboundness() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let bound = format_eval_result(&ev.eval_str(
        "(progn (setq specbind-plain-v 1) (list (let ((specbind-plain-v 2)) specbind-plain-v) specbind-plain-v))",
    ));
    assert_eq!(bound, "OK (2 1)");
    let unbound = format_eval_result(&ev.eval_str(
        "(progn (let ((specbind-unbound-v 5)) specbind-unbound-v) (boundp 'specbind-unbound-v))",
    ));
    assert_eq!(unbound, "OK nil");
}

/// A variable alias is not a plain cell: the binding must land on the
/// alias target (GNU `specbind` follows `SYMBOL_VARALIAS` before binding).
#[test]
fn let_of_a_variable_alias_binds_its_target() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = format_eval_result(&ev.eval_str(
        "(progn (defvar specbind-alias-target 1) (defvaralias 'specbind-alias 'specbind-alias-target) \
         (list (let ((specbind-alias 2)) (list specbind-alias specbind-alias-target)) specbind-alias specbind-alias-target))",
    ));
    assert_eq!(result, "OK ((2 2) 1 1)");
}

/// The fast path must not skip variable watchers: GNU runs them for `let`
/// (operation `let`) and for the unbind (operation `unlet`).
#[test]
fn let_of_a_plain_symbol_runs_variable_watchers() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = format_eval_result(&ev.eval_str(
        "(progn (setq specbind-watched-v 1) (setq specbind-watch-log nil) \
         (add-variable-watcher 'specbind-watched-v (lambda (_s n op _w) (setq specbind-watch-log (cons (list n op) specbind-watch-log)))) \
         (let ((specbind-watched-v 2)) nil) specbind-watch-log)",
    ));
    assert_eq!(result, "OK ((1 unlet) (2 let))");
}

/// The plain-let unwind restores every binding of a multi-variable `let`
/// in one pass, values and order exactly as GNU `unbind_to`.
#[test]
fn nested_plain_lets_unwind_to_their_old_values() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = format_eval_result(&ev.eval_str(
        "(progn (setq unwind-a 1 unwind-b 10) \
         (list (let ((unwind-a 2) (unwind-b 20)) (let ((unwind-a 3)) (list unwind-a unwind-b))) unwind-a unwind-b))",
    ));
    assert_eq!(result, "OK ((3 20) 1 10)");
}

/// A specpdl suffix that mixes a plain `let` with an `unwind-protect`
/// entry takes the general path; both must still run in order.
#[test]
fn let_inside_unwind_protect_restores_before_the_cleanup_runs() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = format_eval_result(&ev.eval_str(
        "(progn (setq unwind-p 1) (setq unwind-seen nil) \
         (unwind-protect (let ((unwind-p 2)) unwind-p) (setq unwind-seen unwind-p)) \
         (list unwind-p unwind-seen))",
    ));
    assert_eq!(result, "OK (1 1)");
}

/// Reading a C-defined global takes GNU's one-load `do_symval_forwarding`
/// leaf.  The values, the void/boundp answers and the write-back must be
/// identical to the full walk for every descriptor variant: an integer
/// forwarder, a boolean one, an object one, and a buffer-local slot (which
/// stays on the buffer path).
#[test]
fn forwarded_globals_read_the_same_through_the_leaf_and_the_full_walk() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let probe = r#"(list (integerp gc-cons-threshold)
                         (progn (setq gc-cons-threshold 654321) gc-cons-threshold)
                         (default-value 'gc-cons-threshold)
                         (symbol-value 'gc-cons-threshold)
                         (boundp 'gc-cons-threshold)
                         (progn (setq case-fold-search t) (list case-fold-search
                                                                (symbol-value 'case-fold-search)
                                                                (default-value 'case-fold-search)))
                         (progn (setq tab-width 13) (list tab-width
                                                          (symbol-value 'tab-width))))"#;
    assert_eq!(
        format_eval_result(&ev.eval_str(probe)),
        "OK (t 654321 654321 654321 t (t t t) (13 13))"
    );
}

/// An alias to a forwarded global still indirects (GNU `find_symbol_value`
/// follows `SYMBOL_VARALIAS` before dispatching on the redirect), and a
/// buffer-local binding of a forwarded slot still shadows the default.
#[test]
fn forwarded_globals_keep_alias_and_buffer_local_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    assert_eq!(
        format_eval_result(&ev.eval_str(
            "(progn (defvaralias 'fwd-leaf-alias 'gc-cons-threshold)
                    (setq gc-cons-threshold 4242)
                    (list fwd-leaf-alias (symbol-value 'fwd-leaf-alias)))"
        )),
        "OK (4242 4242)"
    );
    // Primitives only: a bare `Context` has no `setq-default`/`with-temp-buffer`
    // macros (they live in subr.el).
    assert_eq!(
        format_eval_result(&ev.eval_str(
            "(progn (set-default 'tab-width 8)
                    (set-buffer (get-buffer-create \"fwd-leaf-probe\"))
                    (make-local-variable 'tab-width)
                    (set 'tab-width 3)
                    (list tab-width (symbol-value 'tab-width) (default-value 'tab-width)))"
        )),
        "OK (3 3 8)"
    );
}

/// `throw-on-input` is cached on the evaluator so the quit poll does not ask
/// the obarray on every safe point.  A cache is only correct while it agrees
/// with the cell it mirrors, so check it after each way Lisp can write the
/// variable: `setq`, entering a `let`, leaving it, and `set`.
#[test]
fn cached_throw_on_input_tracks_every_write_path() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let agrees = |ev: &Context, where_: &str| {
        let obarray = ev
            .obarray()
            .symbol_value("throw-on-input")
            .copied()
            .unwrap_or(Value::NIL);
        assert!(
            crate::emacs_core::value::eq_value(&ev.cached_throw_on_input_for_test(), &obarray),
            "{where_}: cached {:?} != obarray {:?}",
            ev.cached_throw_on_input_for_test(),
            obarray
        );
    };
    agrees(&ev, "fresh context");

    ev.eval_str("(setq throw-on-input 'from-setq)")
        .expect("setq");
    agrees(&ev, "after setq");

    // The value must be visible INSIDE the binding, and restored after it.
    let inside = format_eval_result(&ev.eval_str(
        "(let ((throw-on-input 'from-let)) (list throw-on-input (symbol-value 'throw-on-input)))",
    ));
    assert_eq!(inside, "OK (from-let from-let)");
    agrees(&ev, "after the let unwound");
    assert_eq!(
        format_eval_result(&ev.eval_str("throw-on-input")),
        "OK from-setq",
        "the let restored the outer value"
    );

    ev.eval_str("(set 'throw-on-input nil)").expect("set");
    agrees(&ev, "after set to nil");
}
