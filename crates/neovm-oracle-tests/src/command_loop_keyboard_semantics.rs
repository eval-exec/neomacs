//! Oracle parity tests for command-loop keyboard variable semantics.
//!
//! These tests verify parity between GNU Emacs and Neomacs for:
//!
//! 1. `this-command` / `real-this-command` / `this-original-command` are nil
//!    at the start of each command-loop iteration, matching GNU
//!    `keyboard.c:1416-1419`.
//!
//! 2. `echo-keystrokes-help` default value (GNU `keyboard.c` initialization).
//!
//! 3. Minibuffer lifecycle: `minibuffer-mode` / `minibuffer-inactive-mode`
//!    are called around minibuffer entry/exit.
//!
//! 4. Idle timer interaction with `this-single-command-keys` and
//!    `this-command` during `read_key_sequence`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// this-command initial state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_this_command_initially_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(null this-command)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_real_this_command_initially_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(null real-this-command)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_this_original_command_initially_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(null this-original-command)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_last_command_initially_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(null last-command)", expect);
    assert_ok_eq("t", &o, &n);
}

// ---------------------------------------------------------------------------
// echo-keystrokes-help default
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_echo_keystrokes_help_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(symbol-value 'echo-keystrokes-help)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_echo_keystrokes_default_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect("(default-value 'echo-keystrokes)", expect);
}

// ---------------------------------------------------------------------------
// this-command set by command-execute
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_this_command_after_call_interactively() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (call-interactively (setq this-command 'ignore))
      this-command)"#;
    let expect = expect_test::expect![[r#""OK ignore""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_this_command_set_by_setq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (setq this-command 'some-command)
      this-command)"#;
    let expect = expect_test::expect![[r#""OK some-command""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("some-command", &o, &n);
}

// ---------------------------------------------------------------------------
// Minibuffer lifecycle variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_minibuffer_depth_initial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(minibuffer-depth)", expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_prop_minibufferp_initially_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(minibufferp)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_active_minibuffer_window_initial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(active-minibuffer-window)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_minibuffer_exit_hook_bound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(boundp 'minibuffer-exit-hook)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_minibuffer_exit_hook_is_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(listp (symbol-value 'minibuffer-exit-hook))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

// ---------------------------------------------------------------------------
// command-execute / this-command lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_command_execute_sets_this_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (let ((this-command nil))
        (command-execute 'ignore)
        this-command))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_this_command_keys_initially_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (let ((keys (this-command-keys)))
        (length keys)))"#;
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_unread_select_window_event_is_one_key_sequence_event() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((event (list 'select-window (list (selected-window))))
                  (unread-command-events (list event))
                  (keys (read-key-sequence-vector nil nil nil t)))
             (list (length keys)
                   (eq (car (aref keys 0)) 'select-window)
                   (windowp (car (cadr (aref keys 0))))
                   unread-command-events))"#,
        expect,
    );
}

#[test]
fn oracle_execute_kbd_macro_select_window_affects_following_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((w1 (selected-window))
                  (buf (get-buffer-create "oracle-select-window-target"))
                  (w2 (split-window w1)))
             (set-window-buffer w2 buf)
             (setq oracle-selected-window-seen nil)
             (fset 'oracle-record-selected-window
                   (lambda ()
                     (interactive)
                     (setq oracle-selected-window-seen (selected-window))))
             (keymap-set global-map "a" 'oracle-record-selected-window)
             (execute-kbd-macro (vector (list 'select-window (list w2)) ?a))
             (list (eq oracle-selected-window-seen w2)
                   (eq (selected-window) w2)))"#,
        expect,
    );
}

#[test]
fn oracle_execute_kbd_macro_publishes_raw_event_before_menu_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((ignore 32)) \" \" 32 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
             (setq oracle-menu-filter-observations nil)
             (fset 'oracle-capture-last-input-event
                   (lambda (binding)
                     (push (list binding last-input-event)
                           oracle-menu-filter-observations)
                     nil))
             (let ((map (make-sparse-keymap)))
               (define-key
                map " "
               '(menu-item "" ignore
                            :filter oracle-capture-last-input-event))
               (with-temp-buffer
                 (let ((minor-mode-map-alist (list (cons t map)))
                       (recent-before (append (recent-keys) nil)))
                   (execute-kbd-macro " ")
                   (list (nreverse oracle-menu-filter-observations)
                         (buffer-string)
                         last-input-event
                         (equal recent-before
                                (append (recent-keys) nil)))))))"#,
        expect,
    );
}

#[test]
fn oracle_insert_special_event_file_notify_uses_special_event_map_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (file-notify-handle-event (42 stopped \"/tmp/x\") nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
             (require 'filenotify)
             (setq oracle-file-notify-hit nil
                   oracle-file-notify-read nil
                   oracle-file-notify-error nil)
             (fset 'oracle-file-notify-callback
                   (lambda (event)
                     (setq oracle-file-notify-hit event)))
             (let ((binding (lookup-key special-event-map [file-notify])))
               (condition-case err
                   (progn
                     (insert-special-event
                      (make-file-notify
                       :-event '(42 stopped "/tmp/x")
                       :-callback #'oracle-file-notify-callback))
                     (setq oracle-file-notify-read
                           (read-event nil nil 0.01)))
                 (error (setq oracle-file-notify-error err)))
               (list binding
                     oracle-file-notify-hit
                     oracle-file-notify-read
                     oracle-file-notify-error
                     unread-command-events)))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// this-single-command-keys in batch context
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_this_single_command_keys_vector_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(vectorp (this-single-command-keys))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_this_command_keys_type_in_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect("(this-command-keys)", expect);
}

#[test]
fn oracle_prop_this_single_command_keys_empty_in_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(length (this-single-command-keys))";
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("0", &o, &n);
}

// ---------------------------------------------------------------------------
// Timer creation and cancellation parity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_run_with_idle_timer_returns_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (let ((timer (run-with-idle-timer 10 nil 'ignore)))
        (and (timerp timer) (prog1 t (cancel-timer timer)))))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_cancel_timer_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (let ((timer (run-with-idle-timer 10 nil 'ignore)))
        (cancel-timer timer)
        (not (memq timer timer-idle-list))))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_timerp_on_idle_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (let ((timer (run-with-idle-timer 10 nil 'ignore)))
        (prog1 (timerp timer) (cancel-timer timer))))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_run_with_timer_returns_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (let ((timer (run-with-timer 10 nil 'ignore)))
        (and (timerp timer) (prog1 t (cancel-timer timer)))))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_unbounded_read_event_services_with_timeout_without_keyboard_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (timed-out t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((timer-fired nil))
             (list
              (with-timeout (0.02 (setq timer-fired t) 'timed-out)
                (read-event))
              timer-fired))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// command-keys and key-description parity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_key_description_single_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(key-description [32])"#;
    let expect = expect_test::expect![[r#""OK \"SPC\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("\"SPC\"", &o, &n);
}

#[test]
fn oracle_prop_key_description_prefix_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(key-description [32 104])"#;
    let expect = expect_test::expect![[r#""OK \"SPC h\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("\"SPC h\"", &o, &n);
}

#[test]
fn oracle_prop_single_key_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(single-key-description 32)"#;
    let expect = expect_test::expect![[r#""OK \"SPC\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("\"SPC\"", &o, &n);
}

#[test]
fn oracle_prop_single_key_description_with_ctrl() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(single-key-description ?\\C-x)"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 1 27)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// this-command / last-command transition semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_last_command_after_command_execute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (setq this-command 'cmd-a)
      (command-execute 'ignore)
      (setq last-command this-command)
      last-command)"#;
    let expect = expect_test::expect![[r#""OK cmd-a""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_real_last_command_var_exists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(boundp 'real-last-command)";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}
