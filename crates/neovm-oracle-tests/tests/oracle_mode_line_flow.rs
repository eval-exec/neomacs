//! Active GNU mode-line non-local exits and state restoration.
//! Batch GNU bypasses the formatter, so expectations require active Xvfb GNU.

#[path = "../src/common.rs"]
mod common;

use std::path::Path;
use std::process::Command;

use common::oracle_sandbox::{OracleSandbox, project_root};
use common::return_if_neovm_enable_oracle_proptest_not_set;

fn run_active_gnu(form: &str) -> String {
    let requested = std::env::var("NEOVM_FORCE_ORACLE_PATH").unwrap_or_else(|_| "emacs".into());
    let reference = neomacs_parity_reference::attest(
        Path::new(&requested),
        neomacs_parity_reference::AttestationDepth::Fingerprint,
    )
    .expect("active mode-line oracle must use the pinned GNU reference");
    tracing::debug!(reference = %reference.stamp(), "active mode-line oracle attested");
    let artifacts = OracleSandbox::create_fixture_tempdir().expect("oracle artifact directory");
    let display = neomacs_infra::display::start_xvfb(artifacts.path())
        .expect("active GNU mode-line oracle requires Xvfb");
    let result_path = artifacts.path().join("mode-line-result.txt");
    let sandbox = OracleSandbox::new(form, &[], &project_root().join("lisp"))
        .expect("mode-line oracle sandbox");
    let mut command = Command::new(reference.executable());
    sandbox.configure(&mut command);
    command
        .envs(display.env().iter().map(|(key, value)| (key, value)))
        .env_remove("WAYLAND_DISPLAY")
        .env("EMACSNATIVELOADPATH", "/dev/null")
        .env("NO_AT_BRIDGE", "1")
        .env("NEOVM_ACTIVE_MODE_LINE_RESULT", &result_path)
        .args([
            "-Q",
            "--eval",
            r#"(progn
                 (condition-case err
                     (let ((form (with-temp-buffer
                                   (insert-file-contents
                                    (getenv "NEOVM_ORACLE_FORM_FILE"))
                                   (read (current-buffer)))))
                       (with-temp-file (getenv "NEOVM_ACTIVE_MODE_LINE_RESULT")
                         (princ "OK " (current-buffer))
                         (prin1 (eval form t) (current-buffer))))
                   (error (kill-emacs 1)))
                 (kill-emacs 0))"#,
        ]);
    let output = command.output().expect("run active GNU mode-line oracle");
    assert!(
        output.status.success(),
        "active GNU mode-line oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(result_path).expect("active GNU mode-line oracle result")
}

fn assert_active_mode_line_parity(form: &str, expected: expect_test::Expect) {
    let mode = std::env::var("NEOVM_ORACLE_MODE")
        .unwrap_or_else(|_| "snapshot".into())
        .to_ascii_lowercase();
    match mode.as_str() {
        "snapshot" | "snap" | "expected" => {
            let neovm = common::run_neovm_eval(form).expect("Neomacs mode-line oracle result");
            expected.assert_eq(&format!("{neovm:?}"));
        }
        "refresh" | "bless" | "update" => {
            let gnu = run_active_gnu(form);
            expected.assert_eq(&format!("{gnu:?}"));
        }
        "verify" | "live" => {
            let gnu = run_active_gnu(form);
            if mode == "verify" {
                expected.assert_eq(&format!("{gnu:?}"));
            }
            let neovm = common::run_neovm_eval(form).expect("Neomacs mode-line oracle result");
            assert_eq!(neovm, gnu, "active mode-line parity for {form}");
        }
        other => panic!("unknown NEOVM_ORACLE_MODE={other:?}"),
    }
}

#[test]
fn format_mode_line_eval_throw_reaches_outer_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (catch 'fx2-mode-line
               (format-mode-line '("before" (:eval (throw 'fx2-mode-line "escaped")) "after") 0)
               "swallowed"))"#,
        expect_test::expect![[r#""OK \"escaped\"""#]],
    );
}

#[test]
fn format_mode_line_eval_signal_is_logged_inside_outer_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (condition-case err
                 (format-mode-line '("before" (:eval (signal 'error '("mode-line signal"))) "after") 0)
               (error (list 'outer-handler err))))"#,
        expect_test::expect![[r#""OK \"beforeafter\"""#]],
    );
}

#[test]
fn format_mode_line_eval_inner_condition_case_handles_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (format-mode-line
              '("before" (:eval (condition-case err
                                   (signal 'error '("mode-line signal"))
                                 (error "caught"))) "after") 0))"#,
        expect_test::expect![[r#""OK \"beforecaughtafter\"""#]],
    );
}

#[test]
fn format_mode_line_eval_explicit_quit_signal_is_logged() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (condition-case err
                 (format-mode-line '("before" (:eval (signal 'quit nil)) "after") 0)
               (quit (list 'outer-quit err))))"#,
        expect_test::expect![[r#""OK \"beforeafter\"""#]],
    );
}

#[test]
fn format_mode_line_eval_nested_propertize_throw_with_percent_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (catch 'fx2-mode-line
               (format-mode-line
                '("%%:" (:propertize
                         ("pre" (:eval (throw 'fx2-mode-line '(nested 17))) "%b")
                         face bold) ":%%") 0)
               "swallowed"))"#,
        expect_test::expect![[r#""OK (nested 17)""#]],
    );
}

#[test]
fn format_mode_line_eval_signal_preserves_percent_construct_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil)
                 (buffer (get-buffer-create "fx2-percent")))
             (unwind-protect
                 (progn
                   (with-current-buffer buffer
                     (erase-buffer) (insert "abc\ndef\n") (goto-char 5))
                   (set-window-buffer (selected-window) buffer)
                   (format-mode-line
                    '("%%:%b:" (:eval (signal 'error '("mode-line signal"))) ":%l:%c:%%")
                    0 (selected-window) buffer))
               (kill-buffer buffer)))"#,
        expect_test::expect![[r#""OK \"%:fx2-percent::2:0:%\"""#]],
    );
}

#[test]
fn format_mode_line_eval_pending_quit_is_deferred_until_after_formatting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil) (inhibit-quit nil) (result nil))
             (condition-case err
                 (progn
                   (setq result
                         (format-mode-line
                          '("before" (:eval (progn (setq quit-flag t) "pending")) "after") 0))
                   (setq quit-flag nil)
                   (list 'finished result))
               (quit (list 'outer-quit result err))))"#,
        expect_test::expect![[r#""OK (outer-quit \"beforependingafter\" (quit))""#]],
    );
}

fn format_state_fixture(exit: &str) -> String {
    r#"(let* ((noninteractive nil)
               (original-buffer (current-buffer))
               (w1 (selected-window))
               (original-window-buffer (window-buffer w1))
               (a (get-buffer-create "fx2-a"))
               (b (get-buffer-create "fx2-b"))
               (c (get-buffer-create "fx2-c"))
               (w2 (split-window-below))
               result)
          (unwind-protect
              (progn
                (dolist (buffer (list a b c))
                  (with-current-buffer buffer
                    (erase-buffer) (insert "abcdefghij") (goto-char 2)))
                (set-window-buffer w1 a)
                (set-window-buffer w2 b)
                (select-window w1)
                (goto-char 3)
                (string-match "ab" "abcdef")
                (setq result
                      (catch 'fx2-mode-line
                        (condition-case err
                            (format-mode-line
                             (list "%%:%b:"
                                   (list :eval
                                         (list 'progn
                                               (list 'select-window w2)
                                               '(goto-char 7)
                                               (list 'set-buffer c)
                                               '(goto-char 9)
                                               '(string-match "yz" "xyz")
                                               __EXIT__))
                                   ":%b:%%")
                             0 w2 b)
                          (error (list 'outer-error err)))))
                (list result (buffer-name) (point)
                      (eq (selected-window) w1)
                      (match-data t)
                      (with-current-buffer a (point))
                      (with-current-buffer b (point))
                      (with-current-buffer c (point))))
            (select-window w1)
            (set-window-buffer w1 original-window-buffer)
            (delete-window w2)
            (set-buffer original-buffer)
            (dolist (buffer (list a b c)) (kill-buffer buffer))))"#
        .replace("__EXIT__", exit)
}

#[test]
fn format_mode_line_eval_throw_restores_buffer_point_and_window_with_changed_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        &format_state_fixture("'(throw 'fx2-mode-line '(escaped state))"),
        expect_test::expect![[r#""OK ((escaped state) \"fx2-a\" 3 t (1 3) 3 7 9)""#]],
    );
}

#[test]
fn format_mode_line_eval_signal_restores_buffer_point_and_window_with_changed_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        &format_state_fixture("'(signal 'error '(\"mode-line signal\"))"),
        expect_test::expect![[r#""OK (\"%:fx2-b::fx2-c:%\" \"fx2-a\" 3 t (1 3) 3 7 9)""#]],
    );
}

fn format_point_anchor_fixture(edit: &str) -> String {
    r#"(let* ((noninteractive nil)
               (original-buffer (current-buffer))
               (w1 (selected-window))
               (original-window-buffer (window-buffer w1))
               (a (get-buffer-create "fx2-anchor-a"))
               (b (get-buffer-create "fx2-anchor-b"))
               (w2 (split-window-below))
               result)
          (unwind-protect
              (progn
                (dolist (buffer (list a b))
                  (with-current-buffer buffer
                    (erase-buffer) (insert "abcdefghij") (goto-char 2)))
                (set-window-buffer w1 a)
                (set-window-buffer w2 b)
                (select-window w1)
                (goto-char 3)
                (setq result
                      (catch 'fx2-mode-line
                        (format-mode-line
                         (list :eval
                               (list 'progn
                                     (list 'set-buffer a)
                                     '(goto-char 1)
                                     __EDIT__
                                     '(throw 'fx2-mode-line 'escaped)))
                         0 w2 b)))
                (list result (buffer-name) (point)
                      (eq (selected-window) w1)
                      (buffer-string)))
            (select-window w1)
            (set-window-buffer w1 original-window-buffer)
            (delete-window w2)
            (set-buffer original-buffer)
            (dolist (buffer (list a b)) (kill-buffer buffer))))"#
        .replace("__EDIT__", edit)
}

#[test]
fn format_mode_line_eval_throw_restores_point_shifted_by_preceding_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        &format_point_anchor_fixture("'(insert \"XY\")"),
        expect_test::expect![[r#""OK (escaped \"fx2-anchor-a\" 5 t \"XYabcdefghij\")""#]],
    );
}

#[test]
fn format_mode_line_eval_throw_restores_point_shifted_by_preceding_deletion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        &format_point_anchor_fixture("'(delete-region 1 3)"),
        expect_test::expect![[r#""OK (escaped \"fx2-anchor-a\" 1 t \"cdefghij\")""#]],
    );
}

#[test]
fn format_mode_line_eval_signal_does_not_reach_outer_handler_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (setq fx2-mode-line-handlers nil)
             (list
              (handler-bind-1
               (lambda ()
                 (format-mode-line
                  '("before" (:eval (signal 'error '("mode-line signal"))) "after") 0))
               'error
               (lambda (err)
                 (setq fx2-mode-line-handlers (cons 'outer fx2-mode-line-handlers))))
              fx2-mode-line-handlers))"#,
        expect_test::expect![[r#""OK (\"beforeafter\" nil)""#]],
    );
}

#[test]
fn format_mode_line_eval_inner_handler_bind_precedes_safe_signal_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (setq fx2-mode-line-handlers nil)
             (list
              (handler-bind-1
               (lambda ()
                 (format-mode-line
                  '("before"
                    (:eval
                     (handler-bind-1
                      (lambda () (signal 'error '("mode-line signal")))
                      'error
                      (lambda (err)
                        (setq fx2-mode-line-handlers
                              (cons 'inner fx2-mode-line-handlers)))))
                    "after") 0))
               'error
               (lambda (err)
                 (setq fx2-mode-line-handlers (cons 'outer fx2-mode-line-handlers))))
              fx2-mode-line-handlers))"#,
        expect_test::expect![[r#""OK (\"beforeafter\" (inner))""#]],
    );
}

#[test]
fn format_mode_line_eval_safe_handler_suppresses_debug_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let* ((noninteractive nil)
                  (debug-on-error t)
                  (debug-on-signal nil)
                  (debug-ignored-errors nil)
                  (debugger-calls nil)
                  (debugger (lambda (&rest args)
                              (setq debugger-calls (cons 'debugger debugger-calls)))))
             (list
              (condition-case err
                  (format-mode-line
                   '("before" (:eval (signal 'error '("mode-line signal"))) "after") 0)
                ((debug error) (list 'outer-handler err)))
              debugger-calls))"#,
        expect_test::expect![[r#""OK (\"beforeafter\" nil)""#]],
    );
}

#[test]
fn format_mode_line_binding_watchers_signal_outside_safe_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (put 'fx2-derived-quit 'error-conditions '(fx2-derived-quit quit))
             (put 'fx2-non-error 'error-conditions '(fx2-non-error))
             (mapcar
              (lambda (condition)
                (let ((watcher (lambda (symbol value operation where)
                                 (when (eq operation 'let)
                                   (signal condition nil)))))
                  (unwind-protect
                      (progn
                        (add-variable-watcher 'inhibit-quit watcher)
                        (condition-case err
                            (format-mode-line '("before" (:eval "body") "after") 0)
                          (t (car err))))
                    (remove-variable-watcher 'inhibit-quit watcher))))
              '(quit fx2-derived-quit fx2-non-error error)))"#,
        expect_test::expect![[r#""OK (quit fx2-derived-quit fx2-non-error error)""#]],
    );
}

#[test]
fn format_mode_line_unbinding_watchers_signal_outside_safe_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_active_mode_line_parity(
        r#"(let ((noninteractive nil))
             (put 'fx2-derived-quit 'error-conditions '(fx2-derived-quit quit))
             (put 'fx2-non-error 'error-conditions '(fx2-non-error))
             (mapcar
              (lambda (condition)
                (let ((watcher (lambda (symbol value operation where)
                                 (when (eq operation 'unlet)
                                   (signal condition nil)))))
                  (unwind-protect
                      (progn
                        (add-variable-watcher 'inhibit-quit watcher)
                        (condition-case err
                            (format-mode-line '("before" (:eval "body") "after") 0)
                          (t (car err))))
                    (remove-variable-watcher 'inhibit-quit watcher))))
              '(quit fx2-derived-quit fx2-non-error error)))"#,
        expect_test::expect![[r#""OK (quit fx2-derived-quit fx2-non-error error)""#]],
    );
}
