//! VM shared-runtime tests re-homed from neovm-eval's VM test module.
//!
//! Eval's `new_vm_runtime_harness` delegates to full `Context::new`.
//! Core's `new_context` constructs that same surface with the canonical
//! hook installation proof. No eval unit-runtime hooks are used here.

use crate::emacs_core::eval::Context;
use neovm_eval::test_support::authority::loads_in_progress_push_for_test;
use neovm_value::error::EvalError;
use neovm_value::intern::resolve_sym;
use neovm_value::value::Value;

fn with_vm_eval_in_context<R>(
    mut eval: Context,
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    eval.set_lexical_binding(lexical);
    let result = eval.eval_str(src);
    f(result, &eval)
}

fn with_vm_eval<R>(src: &str, lexical: bool, f: impl FnOnce(Result<Value, EvalError>) -> R) -> R {
    with_vm_eval_state(src, lexical, |result, _| f(result))
}

fn with_vm_eval_state<R>(
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    with_vm_eval_in_context(crate::test_utils::new_context(), src, lexical, f)
}

fn with_vm_eval_full_context_state<R>(
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    with_vm_eval_in_context(crate::test_utils::new_context(), src, lexical, f)
}

fn vm_eval_str(src: &str) -> String {
    with_vm_eval(src, false, |result| {
        crate::emacs_core::format_eval_result(&result)
    })
}

fn vm_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::new_context();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

fn vm_eval_interactive_str(src: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    eval.set_variable("noninteractive", Value::NIL);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

fn vm_eval_interactive_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::new_context();
    eval.set_variable("noninteractive", Value::NIL);
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn vm_reader_and_minibuffer_builtins_use_shared_runtime_entry() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((unread-command-events (list 97)))
                   (list (input-pending-p)
                         (read-char)
                         unread-command-events
                         (recent-keys)))
                 (let ((unread-command-events (list 97)))
                   (read-key-sequence "key: "))
                 (let ((unread-command-events (list 'foo)))
                   (read-key-sequence-vector "key: "))
                 (progn
                   (set-input-mode t nil nil 7)
                   (current-input-mode))
                 (progn
                   (set-input-interrupt-mode nil)
                   (current-input-mode))
                 (progn
                   (discard-input)
                   (input-pending-p))
                 (set-input-meta-mode t)
                 (set-output-flow-control t)
                 (set-quit-char 7)
                 (waiting-for-user-input-p)
                 (condition-case err (read-from-minibuffer 1) (error (car err)))
                 (condition-case err (read-string 1) (error (car err)))
                 (condition-case err (completing-read 1 '("a")) (error (car err)))
                 (condition-case err (read-buffer 1) (error (car err)))
                 (condition-case err (read-command 1) (error (car err)))
                 (condition-case err (read-variable 1) (error (car err)))
                 (condition-case err (yes-or-no-p 1) (error (car err))))"#
        ),
        "OK ((t 97 nil [97]) \"a\" [foo] (t nil t 7) (nil nil t 7) nil nil nil nil nil wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument)"
    );
}

#[test]
fn vm_keyboard_c_builtins_use_shared_unread_and_batch_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((unread-command-events (list 'foo 97)))
                   (condition-case err
                       (list (read-char) unread-command-events (recent-keys))
                     (error (list (car err) unread-command-events (recent-keys)))))
                 (let ((unread-command-events (list 'foo 97)))
                   (list (read-event) unread-command-events (recent-keys)))
                 (let ((unread-command-events (list 'foo 97)))
                   (list (read-char-exclusive) unread-command-events (recent-keys)))
                 (let ((unread-command-events nil))
                   (list (read-event)
                         (read-char-exclusive)
                         (read-key-sequence "k: ")
                         (read-key-sequence-vector "k: "))))"#
        ),
        "OK ((error (foo) [foo]) (foo (97) [foo foo]) (97 nil [foo foo foo 97]) (nil nil \"\" []))"
    );
}

#[test]
fn vm_runtime_harness_exposes_public_builtin_surface() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_in_context(
        crate::test_utils::new_context(),
        r#"(progn
             (setq vm-runtime-harness-base nil)
             (fset 'vm-runtime-harness-fn 'car)
             (defvaralias 'vm-runtime-harness-alias 'vm-runtime-harness-base)
             (setq vm-runtime-harness-alias 7)
             (list
              (windowp (selected-window))
              (funcall 'vm-runtime-harness-fn '(1 . 2))
              vm-runtime-harness-base
              (func-arity 'car)))"#,
        false,
        |result, _| {
            assert_eq!(
                crate::emacs_core::format_eval_result(&result),
                "OK (t 1 7 (1 . 1))"
            );
        },
    );
}

#[test]
fn vm_kmacro_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    neovm_eval::test_support::authority::run_vm_kmacro_builtins_use_shared_runtime_state_for_test(
        crate::test_utils::new_context,
    );
}

#[test]
fn vm_execute_kbd_macro_real_key_events_use_command_loop_dispatch() {
    crate::test_utils::init_test_tracing();
    let result = with_vm_eval_full_context_state(
        "(progn
           (setq vm-kmacro-command-loop-count 0)
           (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
           (let ((g (make-sparse-keymap)))
             (use-global-map g)
             (define-key g \"a\"
               (lambda ()
                 (interactive)
                 (setq vm-kmacro-command-loop-count
                       (1+ vm-kmacro-command-loop-count))))
             (execute-kbd-macro \"a\")
             vm-kmacro-command-loop-count))",
        false,
        |result, _| crate::emacs_core::format_eval_result(&result),
    );
    assert_eq!(result, "OK 1");
}

#[test]
fn vm_execute_kbd_macro_named_symbol_uses_function_indirection_chain() {
    crate::test_utils::init_test_tracing();
    let result = with_vm_eval_full_context_state(
        "(progn
           (setq vm-kmacro-named-symbol-count 0)
           (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
           (let ((g (make-sparse-keymap)))
             (use-global-map g)
             (define-key g \"a\"
               (lambda ()
                 (interactive)
                 (setq vm-kmacro-named-symbol-count
                       (1+ vm-kmacro-named-symbol-count)))))
           (fset 'vm-kmacro-target \"a\")
           (fset 'vm-kmacro-alias 'vm-kmacro-target)
           (execute-kbd-macro 'vm-kmacro-alias)
           vm-kmacro-named-symbol-count)",
        false,
        |result, _| crate::emacs_core::format_eval_result(&result),
    );
    assert_eq!(result, "OK 1");
}

#[test]
fn vm_execute_kbd_macro_zero_count_uses_loopfunc() {
    crate::test_utils::init_test_tracing();
    let result = with_vm_eval_full_context_state(
        "(progn
           (setq vm-kmacro-loop-count 0)
           (setq vm-kmacro-loopfunc-count 0)
           (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
           (fset 'vm-kmacro-loopfunc
             (lambda ()
               (setq vm-kmacro-loopfunc-count (1+ vm-kmacro-loopfunc-count))
               (< vm-kmacro-loopfunc-count 3)))
           (let ((g (make-sparse-keymap)))
             (use-global-map g)
             (define-key g \"a\"
               (lambda ()
                 (interactive)
                 (setq vm-kmacro-loop-count (1+ vm-kmacro-loop-count))))
             (execute-kbd-macro \"a\" 0 'vm-kmacro-loopfunc)
             (list vm-kmacro-loop-count vm-kmacro-loopfunc-count)))",
        false,
        |result, _| crate::emacs_core::format_eval_result(&result),
    );
    assert_eq!(result, "OK (2 3)");
}

#[test]
fn vm_execute_kbd_macro_runs_termination_hook_after_error() {
    crate::test_utils::init_test_tracing();
    let result = with_vm_eval_full_context_state(
        "(progn
           (setq vm-kmacro-term-ok nil)
           (setq real-this-command 'vm-outer-real)
           (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
           (fset 'vm-kmacro-term-hook
                 (lambda ()
                   (setq vm-kmacro-term-ok
                         (and (null executing-kbd-macro)
                              (= executing-kbd-macro-index 0)
                              (eq real-this-command 'vm-outer-real)))))
           (setq kbd-macro-termination-hook '(vm-kmacro-term-hook))
           (let ((g (make-sparse-keymap)))
             (use-global-map g)
             (define-key g \"a\" (lambda () (interactive) (error \"boom\"))))
           (condition-case nil
               (execute-kbd-macro \"a\")
             (error nil))
           (list vm-kmacro-term-ok real-this-command))",
        false,
        |result, _| crate::emacs_core::format_eval_result(&result),
    );
    assert_eq!(result, "OK (t vm-outer-real)");
}

#[test]
fn vm_eval_bridge_preserves_frames_across_eval_dependent_builtins() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(frame-parameter (selected-frame) 'width)"),
        "OK 80"
    );
}

#[test]
fn vm_window_and_frame_selection_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((f (selected-frame))
                      (w (selected-window)))
                 (list (framep f)
                       (windowp w)
                       (eq (selected-frame) f)
                       (eq (frame-selected-window f) w)
                       (eq (frame-first-window f) w)
                       (eq (frame-root-window f) w)
                       (eq (window-frame w) f)
                       (bufferp (window-buffer w))
                       (window-live-p w)
                       (window-valid-p w)
                       (frame-live-p f)
                       (frame-visible-p f)))"#
        ),
        "OK (t t t t t t t t t t t t)"
    );
}

#[test]
fn vm_frame_query_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list (frame-char-height)
                     (frame-char-width)
                     (frame-native-height)
                     (frame-native-width)
                     (frame-text-cols)
                     (frame-text-lines)
                     (frame-text-width)
                     (frame-text-height)
                     (frame-total-cols)
                     (frame-total-lines)
                     (frame-position))"#
        ),
        "OK (1 1 25 80 80 25 80 25 80 25 (0 . 0))"
    );
    assert_eq!(
        vm_eval_str(
            r#"(condition-case err
                   (frame-char-height 999999)
                 (error err))"#
        ),
        "OK (wrong-type-argument framep 999999)"
    );
}

#[test]
fn vm_frame_identity_and_display_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((mouse (mouse-position))
                     (pixel (mouse-pixel-position)))
                 (list (frame-id)
                       (eq (frame-root-frame) (selected-frame))
                       (eq (next-frame) (selected-frame))
                       (eq (previous-frame) (selected-frame))
                       (eq (old-selected-frame) (selected-frame))
                       (eq (car mouse) (selected-frame))
                       (cdr mouse)
                       (eq (car pixel) (selected-frame))
                       (cdr pixel)
                       (window-system)
                       (tool-bar-height)
                       (tab-bar-height)))"#
        ),
        "OK (1 t t t t t (nil) t (nil) nil 0 0)"
    );
    assert_eq!(
        vm_eval_str(
            r#"(list (condition-case err (frame-id "x") (error err))
                     (condition-case err (window-system "x") (error err))
                     (condition-case err (tool-bar-height "x") (error err))
                     (condition-case err (next-frame "x") (error err)))"#
        ),
        "OK ((wrong-type-argument frame-live-p \"x\") (wrong-type-argument framep \"x\") (wrong-type-argument framep \"x\") (wrong-type-argument frame-live-p \"x\"))"
    );
}

#[test]
fn vm_terminal_and_display_entrypoints_use_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((frame (selected-frame))
                      (before (length (frame-list)))
                      (created (x-create-frame '((name . "vm-x-frame")
                                                 (title . "vm-x-title")))))
                 (list
                  (null (redraw-frame frame))
                  (null (tty-type frame))
                 (condition-case err (tty-type (selected-window)) (error (car err)))
                 (condition-case err (suspend-tty frame) (error (car err)))
                 (condition-case err (resume-tty frame) (error (car err)))
                  (condition-case err (x-get-resource "Xft.dpi" "Xft.Dpi") (error (car err)))
                  (condition-case err (x-list-fonts "*") (error (car err)))
                  (condition-case err (x-server-vendor frame) (error (car err)))
                  (framep created)
                  (= (length (frame-list)) (1+ before))
                  (equal (frame-parameter created 'name) "vm-x-frame")))"#,
        ),
        "OK (t t wrong-type-argument error error error error error neo t t)"
    );
}

#[test]
fn vm_xdisp_window_visibility_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-xdisp")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert "hello\nworld\n")
                 (goto-char 1)
                 (list (format-mode-line "%b")
                       (window-text-pixel-size w)
                       (pos-visible-in-window-p 1 w)
                       (coordinates-in-window-p '(0 . 0) w)
                       (condition-case err
                           (format-mode-line "%b" nil "x")
                         (error err))
                       (condition-case err
                           (window-text-pixel-size 999999)
                         (error err))
                       (condition-case err
                           (pos-visible-in-window-p 'left w)
                         (error err))
                       (condition-case err
                           (coordinates-in-window-p 'x w)
                         (error err))))"#
        ),
        r#"OK ("" (5 . 2) nil (0 . 0) (wrong-type-argument windowp "x") (wrong-type-argument window-live-p 999999) (wrong-type-argument integer-or-marker-p left) (wrong-type-argument consp x))"#
    );
}

#[test]
fn vm_frame_parameter_and_resize_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (modify-frame-parameters
                  f
                  '((name . "vm-frame")
                    (title . "vm-title")
                    (visibility . nil)
                    (vm-param . 7)))
                 (set-frame-width f 90)
                 (set-frame-height f 30)
                 (set-frame-size f 100 35)
                 (list (frame-parameter f 'name)
                       (frame-parameter f 'title)
                       (frame-parameter f 'visibility)
                       (frame-parameter f 'vm-param)
                       (cdr (assq 'vm-param (frame-parameters f)))
                       (frame-parameter f 'width)
                       (frame-parameter f 'height)
                       (frame-position f)
                       (set-frame-position f 3 4)))"#
        ),
        "OK (\"vm-frame\" \"vm-title\" t 7 7 80 25 (0 . 0) t)"
    );
}

#[test]
fn vm_frame_selected_window_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w1 (selected-window))
                      (w2 ((lambda (old size side normal) (set-window-new-pixel old (- (if (memq side '(t left right)) (window-pixel-width old) (window-pixel-height old)) size)) (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
                 (prog1
                     (list (eq (frame-old-selected-window) nil)
                           (eq (set-frame-selected-window nil w2) w2)
                           (eq (selected-window) w2)
                           (eq (set-frame-selected-window nil w1 t) w1)
                           (eq (selected-window) w1))
                   (select-window w1)
                   ;; `delete-window' is lisp/window.el:4318 and has no subr any
                   ;; more (DIVERGENCES.md 154); its body ends in this C
                   ;; primitive (src/window.c:5684).
                   (delete-window-internal w2)))"#
        ),
        "OK (t t t t t)"
    );
}

#[test]
fn vm_window_scroll_and_history_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((w (selected-window)))
                 (list (window-hscroll w)
                       (set-window-hscroll w 3)
                       (window-hscroll w)
                       (set-window-hscroll w -1)
                       (window-hscroll w)
                       (set-window-hscroll w ?a)
                       (window-hscroll w)
                       (window-margins w)
                       (set-window-margins w 1 2)
                       (window-margins w)
                       (set-window-margins w 1 2)
                       (set-window-margins w nil nil)
                       (window-margins w)
                       (set-window-margins w 3)
                       (window-margins w)
                       (set-window-margins w 3)
                       (window-vscroll w)
                       (set-window-vscroll w 1)
                       (window-vscroll w)
                       (window-fringes w)
                       (set-window-fringes w 1 2)
                       (window-scroll-bars w)
                       (set-window-scroll-bars w 'left)
                       (window-scroll-bars w)
                       (set-window-prev-buffers w nil)
                       (window-prev-buffers w)
                       (set-window-next-buffers w nil)
                       (window-next-buffers w)))"#
        ),
        "OK (0 3 3 0 0 97 97 (nil) t (1 . 2) nil t (nil) t (3) nil 0 0 0 (0 0 nil nil) nil (nil 0 t nil 0 t nil) nil (nil 0 t nil 0 t nil) nil nil nil nil)"
    );
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w1 (selected-window))
                      (w2 ((lambda (old size side normal) (set-window-new-pixel old (- (if (memq side '(t left right)) (window-pixel-width old) (window-pixel-height old)) size)) (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
                 (list (window-use-time w1)
                       (window-use-time w2)
                       (window-bump-use-time w2)
                       (window-use-time w1)
                       (window-use-time w2)
                       (window-bump-use-time w1)))"#
        ),
        "OK (1 0 1 2 1 nil)"
    );
}

#[test]
fn vm_call_interactively_handles_simple_string_specs_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(let ((current-prefix-arg '(4))
                     (evt (list 'mouse-1 (list (list (selected-window) (point) '(0 . 0) 0)))))
                 (call-interactively
                  '(lambda (raw num pt mk beg end evt up ignored)
                     (interactive "P
p
d
m
r
e
U
i")
                     (list raw num pt mk beg end (car evt) up ignored))
                  nil
                  (vector evt)))"#,
            |eval| {
                let current = eval
                    .edit
                    .buffers
                    .current_buffer_id()
                    .expect("current buffer");
                let _ = eval.edit.buffers.replace_buffer_contents(current, "abcd");
                let _ = eval
                    .edit
                    .buffers
                    .goto_buffer_emacs_byte_pos(current, neovm_edit::buffer::EmacsBytePos::new(2));
                let _ = eval.edit.buffers.set_buffer_mark_emacs_byte_pos(
                    current,
                    neovm_edit::buffer::EmacsBytePos::new(1),
                );
            }
        ),
        "OK ((4) 4 3 2 2 3 mouse-1 nil nil)"
    );
}

#[test]
fn vm_yes_or_no_p_uses_shared_runtime_batch_path() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((unread-command-events '(121)))
                 (list
                  (condition-case err
                      (yes-or-no-p "Confirm? ")
                    (error (car err)))
                  unread-command-events))"#
        ),
        "OK (end-of-file (121))"
    );
}

#[test]
fn vm_runtime_control_tail_uses_localized_shared_paths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(listp (garbage-collect))"), "OK t");

    let mut eval = crate::test_utils::new_context();
    let kill_result = eval.eval_str("(kill-emacs 7)");
    match kill_result {
        Err(EvalError::Shutdown(request)) => assert_eq!(request.exit_code, 7),
        other => panic!("kill-emacs should unwind, got {other:?}"),
    }

    assert_eq!(
        eval.shutdown_request(),
        Some(crate::emacs_core::eval::ShutdownRequest {
            exit_code: 7,
            restart: false,
        })
    );
}

#[test]
fn vm_kill_emacs_runs_hooks_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = eval.eval_str(
        "(progn
           (setq vm-kill-hook-log nil)
           (setq kill-emacs-hook
                 (list (lambda () (setq vm-kill-hook-log 'ran))))
           (kill-emacs 3)
           vm-kill-hook-log)",
    );
    match result {
        Err(EvalError::Shutdown(request)) => assert_eq!(request.exit_code, 3),
        other => panic!("kill-emacs should unwind after running hooks, got {other:?}"),
    }
    assert_eq!(
        eval.obarray().symbol_value_copied("vm-kill-hook-log"),
        Some(Value::symbol("ran"))
    );
    assert_eq!(
        eval.shutdown_request(),
        Some(crate::emacs_core::eval::ShutdownRequest {
            exit_code: 3,
            restart: false,
        })
    );
}

#[test]
fn vm_window_metadata_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w (selected-window))
                      (m (minibuffer-window))
                      (dt '(1 2 3)))
                 (list (window-dedicated-p w)
                       (set-window-dedicated-p w t)
                       (window-dedicated-p w)
                       (set-window-dedicated-p w nil)
                       (window-dedicated-p w)
                       (null (window-parameters w))
                       (set-window-parameter w 'foo 'bar)
                       (window-parameter w 'foo)
                       (equal (window-parameters w) '((foo . bar)))
                       (set-window-parameter w 'foo nil)
                       (equal (window-parameters w) '((foo)))
                       (null (window-display-table w))
                       (let ((rv (set-window-display-table w dt))) (equal rv dt))
                       (equal (window-display-table w) dt)
                       (null (set-window-display-table w nil))
                       (null (window-display-table w))
                       (window-cursor-type w)
                       (set-window-cursor-type w 'bar)
                       (window-cursor-type w)
                       (set-window-cursor-type w t)
                       (window-cursor-type w)
                       (set-window-cursor-type m nil)
                       (window-cursor-type m)))"#
        ),
        "OK (nil t t nil nil t bar bar t nil t t t t t t t bar bar t t nil nil)"
    );
}

#[test]
fn vm_window_resize_and_metric_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((w (selected-window))
                      (m (minibuffer-window))
                      (f (selected-frame)))
                  (list
                   (window-minibuffer-p w)
                   (window-minibuffer-p m)
                   (window-resize-apply f)
                   (window-resize-apply-total f)
                   (set-window-new-normal w 0.5)
                   (window-new-normal w)
                   (set-window-new-pixel w 20)
                   (window-new-pixel w)
                   (set-window-new-total w 10)
                   (window-new-total w)
                   (window-bottom-divider-width w)
                   (window-lines-pixel-dimensions w)
                   (window-old-body-pixel-height w)
                   (window-old-body-pixel-width w)
                   (window-old-pixel-height w)
                   (window-old-pixel-width w)
                   (window-right-divider-width w)
                   (window-scroll-bar-height w)
                   (window-scroll-bar-width w)
                   (window-tab-line-height w)
                   (frame-ancestor-p f f)
                   (frame-bottom-divider-width f)
                   (frame-child-frame-border-width f)
                   (frame-focus f)
                   (frame-fringe-width f)
                   (frame-internal-border-width f)
                   (frame-parent f)
                   (frame-pointer-visible-p f)
                   (frame-right-divider-width f)
                   (frame-scale-factor f)
                   (frame-scroll-bar-height f)
                   (frame-scroll-bar-width f)
                   (redirect-frame-focus f f)))"#
        ),
        // GNU 31.1 (canonical oracle, batch -Q) prints exactly this list. In
        // particular `window-resize-apply' is nil on a frame whose root has no
        // pending new size: Fwindow_resize_apply refuses unless the root's
        // new_pixel equals its current pixel size (src/window.c:4986-4989).
        "OK (nil t nil t 0.5 0.5 20 20 10 10 0 nil 0 0 0 0 0 0 0 0 nil 0 0 nil 0 0 nil t 0 1.0 0 0 nil)"
    );
}

#[test]
fn vm_remaining_frame_stub_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (list
                  (hash-table-p (frame--face-hash-table))
                  (frame--set-was-invisible f t)
                  (frame-after-make-frame f nil)
                  (frame-font-cache f)
                  (frame-or-buffer-changed-p)
                  (frame-or-buffer-changed-p nil)
                  (condition-case err (frame-or-buffer-changed-p 'vm-missing-var) (error (car err)))
                  (frame-window-state-change f)
                  (frame--z-order-lessp f f)))"#
        ),
        "OK (t t nil nil t nil void-variable nil nil)"
    );
}

#[test]
fn vm_window_configuration_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((w1 (selected-window))
                      (w2 ((lambda (old size side normal) (set-window-new-pixel old (- (if (memq side '(t left right)) (window-pixel-width old) (window-pixel-height old)) size)) (split-window-internal old size side normal)) w1 (/ (window-pixel-width w1) 2) 'right nil))
                      (b1 (get-buffer-create "vm-wcfg-1"))
                      (b2 (get-buffer-create "vm-wcfg-2")))
                 (set-window-buffer w1 b1)
                 (set-window-buffer w2 b2)
                 (select-window w2)
                 (let ((cfg (current-window-configuration)))
                   (delete-window-internal w2)
                   (set-window-configuration cfg)
                   (list (window-configuration-p cfg)
                         (framep (window-configuration-frame cfg))
                         (window-configuration-equal-p cfg cfg)
                         (length (window-list))
                         (eq (selected-window) w2)
                         (eq (current-buffer) b2)
                         (eq (window-buffer w1) b1)
                         (eq (window-buffer w2) b2))))"#
        ),
        "OK (t t t 2 t t t t)"
    );
}

#[test]
fn vm_key_lookup_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((g (make-sparse-keymap))
                      (l (make-sparse-keymap))
                      (m (make-sparse-keymap)))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key g "a" 'ignore)
                 (define-key g [remap self-insert-command] 'delete-char)
                 (define-key l "c" 'self-insert-command)
                 (define-key m "b" 'forward-char)
                 (setq minor-mode-map-alist (list (cons 'vm-demo-mode m)))
                 (setq vm-demo-mode t)
                 (list (key-binding "a")
                       (key-binding "c")
                       (key-binding "c" t t)
                       (lookup-key (current-local-map) "c")
                       (minor-mode-key-binding "b")
                       (condition-case err
                           (key-binding "a" t nil 0)
                         (error (car err)))))"#
        ),
        "OK (ignore delete-char self-insert-command self-insert-command ((vm-demo-mode . forward-char)) args-out-of-range)"
    );
}

#[test]
fn vm_command_remapping_uses_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((g (make-sparse-keymap))
                     (m (make-sparse-keymap)))
                 (use-global-map g)
                 (define-key g [remap ignore] 'forward-char)
                 (define-key m [remap ignore] 'delete-char)
                 (setq minor-mode-map-alist (list (cons 'vm-remap-mode m)))
                 (setq vm-remap-mode t)
                 (list (command-remapping 'ignore)
                       (command-remapping 'ignore nil '(keymap (remap keymap (ignore . self-insert-command))))
                       (condition-case err
                           (command-remapping 'ignore 0)
                         (error (car err)))) )"#
        ),
        "OK (delete-char self-insert-command args-out-of-range)"
    );
}

#[test]
fn vm_minibuffer_reader_frontends_use_shared_runtime_batch_eof_path() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((unread-command-events (list 97)))
                   (condition-case err
                       (read-from-minibuffer "Prompt: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 98)))
                   (condition-case err
                       (read-string "Prompt: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 99)))
                   (condition-case err
                       (completing-read "Prompt: " '("alpha"))
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 100)))
                   (condition-case err
                       (read-buffer "Buffer: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 101)))
                   (condition-case err
                       (read-command "Command: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 102)))
                   (condition-case err
                       (read-variable "Variable: ")
                     (end-of-file (list (car err) unread-command-events))))
                 (let ((unread-command-events (list 103)))
                   (condition-case err
                       (yes-or-no-p "Confirm?")
                     (end-of-file (list (car err) unread-command-events)))))"#
        ),
        "OK ((end-of-file (97)) (end-of-file (98)) (end-of-file (99)) (end-of-file (100)) (end-of-file (101)) (end-of-file (102)) (end-of-file (103)))"
    );
}

#[test]
fn vm_read_buffer_delegates_to_read_buffer_function() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((target (get-buffer-create "rbf-target")))
                 (unwind-protect
                     (let ((read-buffer-function
                            (lambda (&rest args) args)))
                       (read-buffer "Buffer: " target t))
                   (kill-buffer target)))"#
        ),
        r#"OK ("Buffer: " "rbf-target" t)"#
    );
}

#[test]
fn vm_read_buffer_override_receives_predicate_and_completion_case_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((completion-ignore-case nil)
                     (read-buffer-completion-ignore-case t)
                     (read-buffer-function
                      (lambda (&rest args)
                        (list args completion-ignore-case))))
                 (list (read-buffer "Buffer: " nil nil 'identity)
                       completion-ignore-case))"#
        ),
        r#"OK ((("Buffer: " nil nil identity) t) nil)"#
    );
}

#[test]
fn vm_completing_read_calls_completing_read_function() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((completing-read-function
                      (lambda (&rest args)
                        (cons 'called args))))
                 (completing-read "Prompt: " '("alpha") nil t nil 'hist "alpha" t))"#
        ),
        r#"OK (called "Prompt: " ("alpha") nil t nil hist "alpha" t)"#
    );
}

#[test]
fn vm_process_contact_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_str(
        r#"(let ((network (make-network-process :name "vm-contact-network" :server t :service 0))
                 (pipe (make-pipe-process :name "vm-contact-pipe")))
             (unwind-protect
                 (list
                  (let ((port (process-contact network :service))
                        (local (process-contact network :local)))
                    (list
                     (stringp (process-contact network :name))
                     (eq (process-contact network :server) t)
                     (integerp port)
                     (and (vectorp local)
                          (= (length local) 5)
                          (= (aref local 0) 127)
                          (= (aref local 4) port))
                     (null (process-contact network :remote))
                     (null (process-contact network :coding))
                     (null (process-contact network :foo))))
                  (list
                   (stringp (process-contact pipe :name))
                   (null (process-contact pipe :server))
                   (null (process-contact pipe :service))
                   (null (process-contact pipe :local))
                   (null (process-contact pipe :remote))
                   (null (process-contact pipe :coding))
                   (null (process-contact pipe :foo))))
               (condition-case nil (delete-process network) (error nil))
               (condition-case nil (delete-process pipe) (error nil))))"#,
    );
    assert_eq!(result, "OK ((t t t t t t t) (t t t t t t t))");
}

#[test]
fn vm_process_attributes_builtin_uses_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((attrs (process-attributes (emacs-pid))))
                   (and (listp attrs)
                        (null (assq 'pid attrs))
                        (let ((pair (assq 'comm attrs)))
                          (and (consp pair) (stringp (cdr pair))))))
                 (null (process-attributes -1))
                 (eq (car (condition-case err (process-attributes 'x) (error err)))
                     'wrong-type-argument)
                 (null (process-attributes 999999999)))"#
        ),
        "OK (t t t t)"
    );
}

#[test]
fn vm_process_network_and_signal_builtins_use_direct_runtime_paths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let* ((interfaces (network-interface-list))
                      (first (car interfaces))
                      (ifname (car first))
                      (info (network-interface-info ifname)))
                 (list
                 (listp interfaces)
                  (stringp ifname)
                  (vectorp (cdr first))
                  (equal (format-network-address [127 0 0 1 80]) "127.0.0.1:80")
                  (equal (format-network-address [127 0 0 1 80] t) "127.0.0.1")
                  (equal (format-network-address [0 0 0 0 0 0 0 1 80]) "[0:0:0:0:0:0:0:1]:80")
                  (equal (format-network-address [0 0 0 0 0 0 0 1 80] t) "0:0:0:0:0:0:0:1")
                  (and (listp info) (= (length info) 5))
                  (consp (network-lookup-address-info "localhost"))
                  (consp (member "HUP" (signal-names)))
                  (listp (list-system-processes))
                  (integerp (num-processors))
                  (> (num-processors) 0)
                  (eq (car (condition-case err
                               (format-network-address)
                             (error err)))
                      'wrong-number-of-arguments)
                  (condition-case err
                      (network-interface-list nil 'bogus)
                    (error (car err)))) )"#
        ),
        "OK (t t t t t t t t t t t t t t error)"
    );
}

#[test]
fn vm_fileio_builtins_use_shared_default_directory_state() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!("neovm-vm-fileio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("subdir")).expect("create subdir");
    std::fs::write(base.join("alpha.txt"), b"alpha").expect("write alpha");
    let base_str = format!("{}/", base.to_string_lossy());
    let alpha = base.join("alpha.txt").to_string_lossy().to_string();

    let result = vm_eval_with_init_str(
        r#"(list
             (expand-file-name "alpha.txt")
             (file-name-as-directory "dir")
             (directory-file-name "dir/")
             (file-name-concat "dir" "child")
             (file-exists-p "alpha.txt")
             (file-readable-p "alpha.txt")
             (file-writable-p "alpha.txt")
             (file-accessible-directory-p "subdir")
             (file-directory-p "subdir")
             (file-regular-p "alpha.txt")
             (file-newer-than-file-p "alpha.txt" "missing.txt")
             (progn (access-file "alpha.txt" "open") 'ok)
             (length (file-system-info ".")))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval
                .edit
                .buffers
                .current_buffer_id()
                .expect("current buffer");
            eval.edit
                .buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(
        result,
        format!(r#"OK ("{alpha}" "dir/" "dir" "dir/child" t t t t t t t ok 3)"#)
    );
}

#[test]
fn vm_fileio_mutation_builtins_use_shared_default_directory_state() {
    crate::test_utils::init_test_tracing();
    let base =
        std::env::temp_dir().join(format!("neovm-vm-fileio-mutation-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    std::fs::write(base.join("alpha.txt"), b"alpha").expect("write alpha");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(list
             (progn (make-directory-internal "made") (file-directory-p "made"))
             (progn (copy-file "alpha.txt" "beta.txt" t) (file-exists-p "beta.txt"))
             (progn (rename-file "beta.txt" "gamma.txt" t) (file-exists-p "gamma.txt"))
             (progn (add-name-to-file "gamma.txt" "delta.txt" t) (file-exists-p "delta.txt"))
             (directory-files "." nil "\\.txt$")
             (progn (delete-file-internal "delta.txt") (file-exists-p "delta.txt"))
             (progn (delete-file-internal "gamma.txt") (file-exists-p "gamma.txt"))
             (progn (delete-directory-internal "made") (file-directory-p "made")))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval
                .edit
                .buffers
                .current_buffer_id()
                .expect("current buffer");
            eval.edit
                .buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(
        result,
        r#"OK (t t t t ("alpha.txt" "delta.txt" "gamma.txt") nil nil nil)"#
    );
}

#[test]
fn vm_insert_file_contents_and_write_region_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!(
        "neovm-vm-fileio-insert-write-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    std::fs::write(base.join("alpha.txt"), b"abcdef").expect("write alpha");
    std::fs::write(base.join("out.txt"), b"abcde").expect("write out");
    let base_str = format!("{}/", base.to_string_lossy());
    let alpha = base.join("alpha.txt").to_string_lossy().to_string();
    let out = base.join("out.txt").to_string_lossy().to_string();
    let visit = base.join("visit.txt").to_string_lossy().to_string();

    let mut eval = crate::test_utils::new_context();
    eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
    let current = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.edit
        .buffers
        .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
        .expect("buffer local default-directory should set");

    let insert_result = eval
        .eval_str(r#"(insert-file-contents "alpha.txt" t)"#)
        .expect("insert-file-contents should execute");

    let insert_parts = neovm_value::value::list_to_vec(&insert_result).expect("insert return list");
    assert_eq!(insert_parts[0].as_utf8_str(), Some(alpha.as_str()));
    assert_eq!(insert_parts[1], Value::fixnum(6));
    let buf = eval.edit.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "abcdef");
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some(alpha.as_str())
    );
    assert!(!buf.is_modified());

    eval.eval_str(&format!(
        r#"(write-region "XY" nil "out.txt" 2 "{}")"#,
        visit
    ))
    .expect("write-region should execute");

    assert_eq!(std::fs::read_to_string(&out).expect("read out"), "abXYe");
    let buf = eval.edit.buffers.current_buffer().expect("current buffer");
    assert_eq!(
        buf.file_name_runtime_string_owned().as_deref(),
        Some(visit.as_str())
    );
    assert!(!buf.is_modified());

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn vm_file_name_helper_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (file-name-directory "/tmp/example.txt")
                 (file-name-nondirectory "/tmp/example.txt")
                 (directory-file-name "/tmp/example/")
                 (file-name-concat "/tmp" "example" "child")
                 (file-name-absolute-p "/tmp/example.txt")
                 (file-name-absolute-p "example.txt")
                 (directory-name-p "/tmp/example/")
                 (directory-name-p "/tmp/example"))"#
        ),
        r#"OK ("/tmp/" "example.txt" "/tmp/example" "/tmp/example/child" t nil t nil)"#
    );
}

#[test]
fn vm_dired_builtins_use_shared_default_directory_state() {
    crate::test_utils::init_test_tracing();
    let base =
        std::env::temp_dir().join(format!("neovm-vm-dired-default-dir-{}", std::process::id()));
    let fixture = base.join("fixtures");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&fixture).expect("create fixture dir");
    std::fs::create_dir(fixture.join("adir")).expect("create adir");
    std::fs::create_dir(fixture.join("subdir")).expect("create subdir");
    std::fs::write(fixture.join("alpha.txt"), b"").expect("write alpha");
    std::fs::write(fixture.join("beta.el"), b"").expect("write beta");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(list
             (mapcar #'car (directory-files-and-attributes "fixtures" nil "\\.el$"))
             (file-name-all-completions "sub" "fixtures/")
             (file-name-completion "a" "fixtures/" 'file-directory-p)
             (file-name-completion "a" "fixtures/"
                                   (lambda (path) (file-directory-p path)))
             (let ((attrs (file-attributes "fixtures/alpha.txt")))
               (nth 7 attrs))
             (find-file-name-handler "fixtures/alpha.txt" 'insert-file-contents))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval
                .edit
                .buffers
                .current_buffer_id()
                .expect("current buffer");
            eval.edit
                .buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(
        result,
        r#"OK (("beta.el") ("subdir/") "adir/" "adir/" 0 nil)"#
    );
}

#[test]
fn vm_file_name_completion_callable_predicate_uses_shared_runtime_callback() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!(
        "neovm-vm-file-name-completion-callable-{}",
        std::process::id()
    ));
    let fixture = base.join("fixtures");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&fixture).expect("create fixture dir");
    std::fs::create_dir(fixture.join("adir")).expect("create adir");
    std::fs::write(fixture.join("alpha.txt"), b"").expect("write alpha");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(let ((seen 0))
             (list (file-name-completion
                    "a"
                    "fixtures/"
                    (lambda (path)
                      (setq seen (1+ seen))
                      (file-directory-p path)))
                   seen))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval
                .edit
                .buffers
                .current_buffer_id()
                .expect("current buffer");
            eval.edit
                .buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(result, r#"OK ("adir/" 2)"#);
}

#[test]
fn vm_file_metadata_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!("neovm-vm-file-metadata-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    let alpha = base.join("alpha.txt");
    let beta = base.join("beta.txt");
    std::fs::write(&alpha, b"alpha").expect("write alpha");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&beta, b"beta").expect("write beta");
    let base_str = format!("{}/", base.to_string_lossy());
    let alpha_abs = alpha.to_string_lossy().to_string();

    let result = vm_eval_with_init_str(
        r#"(list
             (integerp (file-modes "alpha.txt"))
             (progn (set-file-modes "alpha.txt" #o600) (file-modes "alpha.txt"))
             (progn (set-file-times "alpha.txt" 0)
                    (file-newer-than-file-p "beta.txt" "alpha.txt"))
             (verify-visited-file-modtime)
             (set-visited-file-modtime nil))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval
                .edit
                .buffers
                .current_buffer_id()
                .expect("current buffer");
            eval.edit
                .buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
            eval.edit
                .buffers
                .set_buffer_file_name(current, Value::string(alpha_abs.clone()))
                .expect("buffer file name should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(result, r#"OK (t 384 t t nil)"#);
}

#[test]
fn vm_file_metadata_tail_and_coding_scan_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!(
        "neovm-vm-file-metadata-tail-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    std::fs::write(base.join("alpha.txt"), b"alpha").expect("write alpha");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(let ((orig (default-file-modes)))
             (prog1
                 (progn
                   (erase-buffer)
                   (set-buffer-multibyte t)
                   (insert "abc")
                   (list
                    (progn (set-default-file-modes #o700) (default-file-modes))
                    (file-acl "alpha.txt")
                    (equal (file-selinux-context "alpha.txt") '(nil nil nil nil))
                    (find-coding-systems-region-internal 1 4)))
               (set-default-file-modes orig)))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval
                .edit
                .buffers
                .current_buffer_id()
                .expect("current buffer");
            eval.edit
                .buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(result, r#"OK (448 nil t t)"#);
}

#[test]
fn vm_file_setters_and_display_stubs_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    let base = std::env::temp_dir().join(format!("neovm-vm-file-setters-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base");
    let base_str = format!("{}/", base.to_string_lossy());

    let result = vm_eval_with_init_str(
        r#"(list
             (set-file-acl "alpha.txt" "user::rw-")
             (set-file-selinux-context "alpha.txt" '(nil nil nil nil))
             (send-string-to-terminal "" (selected-frame))
             (progn
               (internal-show-cursor nil nil)
               (internal-show-cursor-p))
             (progn
               (internal-show-cursor (selected-window) t)
               (internal-show-cursor-p (selected-window))))"#,
        |eval| {
            eval.set_variable("default-directory", Value::string("/tmp/neovm-global/"));
            let current = eval
                .edit
                .buffers
                .current_buffer_id()
                .expect("current buffer");
            eval.edit
                .buffers
                .set_buffer_local_property(current, "default-directory", Value::string(&base_str))
                .expect("buffer local default-directory should set");
        },
    );

    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(result, r#"OK (nil nil nil nil t)"#);
}

#[test]
fn vm_font_builtins_accept_live_frame_designators_on_shared_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (list (list-fonts (font-spec) f)
                       (find-font (font-spec) f)
                       (font-family-list f)))"#
        ),
        "OK (nil nil nil)"
    );
}

#[test]
fn vm_font_face_and_color_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let ((f (selected-frame)))
                  (list
                   (fontp (font-spec :family "Mono"))
                   (let ((font (font-spec :family "Mono")))
                     (font-put font :weight 'bold)
                     (font-get font :weight))
                   (stringp (font-xlfd-name (font-spec :family "Mono")))
                   (vectorp (internal-lisp-face-p 'default))
                   (equal (internal-lisp-face-attribute-values :underline) '(t nil))
                   (internal-lisp-face-equal-p 'default 'default)
                   (null (internal-lisp-face-empty-p 'default))
                   (face-attribute-relative-p :height 1.1)
                   (merge-face-attribute :weight 'unspecified 'bold)
                   (equal (color-values-from-color-spec "#111122223333")
                          '(4369 8738 13107))
                   (color-gray-p "#111111")
                   (color-supported-p "red")
                   (> (color-distance "black" "white") 0)
                   (null (face-font 'default))
                   (null (internal-face-x-get-resource "font" "Font"))
                   (null (internal-set-font-selection-order
                          '(:width :height :weight :slant)))
                   (equal (internal-set-alternative-font-family-alist '(("Foo" "Bar")))
                          '((Foo Bar)))
                   (equal (internal-set-alternative-font-registry-alist '((1 2)))
                          '((1 2)))))"##
        ),
        r#"OK (t bold t t t t t t bold t t t t t t t t t)"#
    );
}

#[test]
fn vm_font_face_frame_sensitive_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let* ((f (selected-frame))
                       (face 'vm-runtime-face))
                  (list
                    (vectorp (internal-make-lisp-face face f))
                    (eq (internal-copy-lisp-face 'default face f f) face)
                    (eq (internal-set-lisp-face-attribute face :foreground "red" f) face)
                    (equal (internal-get-lisp-face-attribute face :foreground f) "red")
                    (progn
                      (internal-set-lisp-face-attribute face :foreground "blue" t)
                      (internal-merge-in-global-face face f)
                      (equal (internal-get-lisp-face-attribute face :foreground f) "blue"))))"##
        ),
        r#"OK (t t t t t)"#
    );
}

#[test]
fn vm_default_face_color_updates_frame_parameters_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let ((f (selected-frame)))
                  (internal-set-lisp-face-attribute 'default :foreground "#112233" f)
                  (internal-set-lisp-face-attribute 'default :background "#445566" f)
                  (list (frame-parameter f 'foreground-color)
                        (frame-parameter f 'background-color)
                        (internal-get-lisp-face-attribute
                         'default :foreground f)
                        (internal-get-lisp-face-attribute
                         'default :background f)))"##,
        ),
        r##"OK ("#112233" "#445566" "#112233" "#445566")"##
    );
}

#[test]
fn vm_special_face_colors_update_frame_parameters_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let* ((f (selected-frame))
                        (original (symbol-function 'modify-frame-parameters))
                        (function-cell-calls 0))
                  (fset 'modify-frame-parameters
                        (lambda (&rest args)
                          (setq function-cell-calls
                                (1+ function-cell-calls))
                          (apply original args)))
                  (unwind-protect
                      (progn
                        (internal-set-lisp-face-attribute
                         'cursor :background "#112233" f)
                        (internal-set-lisp-face-attribute
                         'border :background "#445566" f)
                        (internal-set-lisp-face-attribute
                         'mouse :background "#778899" f)
                        (list (frame-parameter f 'cursor-color)
                              (frame-parameter f 'border-color)
                              (frame-parameter f 'mouse-color)
                              function-cell-calls))
                    (fset 'modify-frame-parameters original)))"##
        ),
        r##"OK ("#112233" "#445566" "#778899" 0)"##
    );
}

#[test]
fn vm_font_stub_tail_uses_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(list
                 (null (clear-face-cache))
                 (vectorp (face-attributes-as-vector nil))
                 (progn
                   (erase-buffer)
                   (insert "a")
                   (null (font-at 1 (selected-window))))
                 (condition-case nil
                     (font-face-attributes nil)
                   (error t))
                 (condition-case nil
                     (font-get-glyphs nil 0 1)
                   (wrong-type-argument t))
                 (null (font-get-system-font))
                 (null (font-get-system-normal-font))
                 (null (font-has-char-p (font-spec :family "Mono") ?a))
                 (null (font-info "Mono"))
                 (condition-case nil
                     (font-variation-glyphs nil ?a)
                   (wrong-type-argument t))
                 (null (fontset-font nil ?a))
                 (condition-case nil
                     (fontset-info nil)
                   (error t))
                 ;; ledger 190: fontset-list-all is GNU's #ifdef ENABLE_CHECKING
                 ;; debug entry point (src/fontset.c:2254) and no ordinary build
                 ;; declares it; this port's returned fontset-list's answer.
                 (null (fboundp 'fontset-list-all)))"##
        ),
        r#"OK (t t t t t t t t t t t t t)"#
    );
}

#[test]
fn vm_native_stub_clusters_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    // GNU compiles src/inotify.c only under HAVE_INOTIFY (src/Makefile.in:
    // "inotify.o if HAVE_INOTIFY"), and its file-notify backend is kqueue on
    // Darwin (src/kqueue.c:338,475,505).  NeoVM declares the same split when
    // it registers these subrs (file_notify/subrs.rs), so the inotify cluster
    // is three live calls on Linux and three absences elsewhere.
    let file_notify_probe = if cfg!(target_os = "linux") {
        r#"(let ((w (inotify-add-watch "/tmp" nil nil)))
                   (list (consp w)
                         (inotify-valid-p w)
                         (inotify-rm-watch w)))"#
    } else {
        r#"(list (null (fboundp 'inotify-add-watch))
                   (null (fboundp 'inotify-rm-watch))
                   (null (fboundp 'inotify-valid-p)))"#
    };
    assert_eq!(
        vm_eval_str(&format!(
            r##"(list
                 (null (fboundp 'debug-timer-check))
                 {file_notify_probe}
                 (null (fboundp 'inotify-watch-list))
                 (null (fboundp 'inotify-allocated-p))
                 (eq (condition-case err
                         (dbus-make-inhibitor-lock "session" "app")
                       (void-function (car err)))
                     'void-function)
                 (null (fboundp 'dbus-close-inhibitor-lock))
                 (null (fboundp 'dbus-registered-inhibitor-locks))
                 (if (featurep 'lcms2) (lcms2-available-p)
                   (null (fboundp 'lcms2-available-p)))
                 (if (featurep 'lcms2) (fboundp 'lcms-cie-de2000)
                   (null (fboundp 'lcms-cie-de2000)))
                 (if (featurep 'lcms2) (fboundp 'lcms-xyz->jch)
                   (null (fboundp 'lcms-xyz->jch)))
                 (if (featurep 'lcms2) (fboundp 'lcms-jch->xyz)
                   (null (fboundp 'lcms-jch->xyz)))
                 (if (featurep 'lcms2) (fboundp 'lcms-jch->jab)
                   (null (fboundp 'lcms-jch->jab)))
                 (if (featurep 'lcms2) (fboundp 'lcms-jab->jch)
                   (null (fboundp 'lcms-jab->jch)))
                 (if (featurep 'lcms2) (fboundp 'lcms-cam02-ucs)
                   (null (fboundp 'lcms-cam02-ucs)))
                 (if (featurep 'lcms2) (fboundp 'lcms-temp->white-point)
                   (null (fboundp 'lcms-temp->white-point)))
                 (null (neomacs-frame-geometry))
                 (null (neomacs-frame-edges))
                 (equal (neomacs-mouse-absolute-pixel-position) '(0 . 0))
                 (null (neomacs-set-mouse-absolute-pixel-position 0 0))
                 (null (neomacs-display-monitor-attributes-list))
                 (null (neomacs-clipboard-set "x"))
                 (equal (neomacs-clipboard-get) "x")
                 (null (neomacs-primary-selection-set "x"))
                 (equal (neomacs-primary-selection-get) "x")
                 (equal (neomacs-core-backend) "rust")
                 (if (memq 'gnutls (gnutls-available-p)) t)
                 (if (memq 'gnutls3 (gnutls-available-p)) t)
                 (neomacs-tls-available-p)
                 (null (featurep 'tls))
                 ;; ledger 190: open-tls-stream is a defun in
                 ;; lisp/obsolete/tls.el:186, which this port ships; the Rust
                 ;; subr under that name was deleted.  neomacs-open-tls-stream,
                 ;; asserted above, is the same implementation under this
                 ;; port's own namespace.
                 (null (fboundp 'open-tls-stream))
                 (if (assq 'AES-256-CBC (gnutls-ciphers)) t)
                 (if (assq 'SHA256 (gnutls-digests)) t)
                 (if (assq 'SHA256 (gnutls-macs)) t)
                 (gnutls-errorp nil)
                 (equal (gnutls-error-string 0) "Success.")
                 (null (gnutls-error-fatalp 1))
                 (null (gnutls-peer-status-warning-describe nil))
                 (let ((p (make-process :name "vm-gnutls-stub"
                                        :command (list "cat"))))
                   (unwind-protect
                       (list
                        (null (gnutls-asynchronous-parameters p nil))
                        (condition-case err (gnutls-boot t nil nil)
                          (wrong-type-argument (car err)))
                        (condition-case err (gnutls-bye t nil)
                          (wrong-type-argument (car err)))
                        (null (gnutls-deinit p))
                        (= (gnutls-get-initstage p) 0)
                        (null (gnutls-peer-status p)))
                     (condition-case nil (delete-process p) (error nil))))
                 (condition-case nil (gnutls-format-certificate "x")
                   (error t))
                 (condition-case nil (gnutls-hash-digest 'sha256 "x")
                   (error t))
                 (condition-case nil (gnutls-hash-mac 'sha256 "k" "x")
                   (error t))
                 (condition-case nil (gnutls-symmetric-decrypt nil nil nil nil)
                   (error t))
                 (condition-case nil (gnutls-symmetric-encrypt nil nil nil nil)
                   (error t)))"##
        )),
        r#"OK (t (t t t) t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t t (t wrong-type-argument wrong-type-argument t t t) t t t t t)"#
    );
}

#[test]
fn vm_format_mode_line_noninteractive_matches_gnu_batch_noop() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(r#"(list (stringp (format-mode-line "%b")) (length (format-mode-line "%b")))"#),
        r#"OK (t 0)"#
    );
}

#[test]
fn vm_format_mode_line_default_string_preserves_gnu_element_properties() {
    crate::test_utils::init_test_tracing();
    // Exact default-STRING result from GNU 31.1 CONT30 gnu-oracle-r3,
    // case "literal". Keep this VM-dispatch check property-bearing.
    assert_eq!(
        vm_eval_interactive_str(
            r#"(equal-including-properties
                 (format-mode-line "LITERAL")
                 '#("LITERAL" 0 7 (mode-line-elt-no 1)))"#
        ),
        "OK t"
    );
}

// The decoding fixtures below select GNU's integer FACE=0/NOPROP target
// when their contract is formatted text. Default STRING now correctly carries
// element properties; the strict test above and the native attribution/oracle
// families cover those without stripping any property from an output. Tests
// whose contract concerns text properties retain their original FACE target.
#[test]
fn vm_format_mode_line_uses_shared_state_and_falls_back_for_eval_forms() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-fmt-mode-line")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert "abc")
                 (setq mode-name "Neo")
                 (list (format-mode-line '("%b " mode-name) 0)
                       (format-mode-line '(:eval mode-name) 0)))"#
        ),
        r#"OK ("vm-fmt-mode-line Neo" "Neo")"#
    );
}

#[test]
fn vm_format_mode_line_symbol_conditional_uses_only_selected_branch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let ((mode-line-flag t))
                 (list
                  (format-mode-line
                   '(mode-line-flag
                     "then"
                     (:eval (error "boom"))) 0)
                  (progn
                    (setq mode-line-flag nil)
                    (format-mode-line
                     '(mode-line-flag
                       (:eval (error "boom"))
                       "else") 0))))"#
        ),
        r#"OK ("then" "else")"#
    );
}

#[test]
fn vm_format_mode_line_string_valued_symbols_render_literally() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-fmt-mode-line-literal")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (setq mode-name "%b")
                 (format-mode-line '("%b " mode-name) 0))"#
        ),
        r#"OK "vm-fmt-mode-line-literal %b""#
    );
}

#[test]
fn vm_format_mode_line_fixnum_elements_pad_and_truncate_tail() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "xy")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (format-mode-line '((5 "%b") "!" (-1 "%b")) 0))"#
        ),
        r#"OK "xy   !x""#
    );
}

#[test]
fn vm_format_mode_line_percent_specs_keep_gnu_field_width_and_dash_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "xy")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (format-mode-line "%5b|%-|%2*" 0))"#
        ),
        r#"OK "xy   |--|- ""#
    );
}

#[test]
fn vm_format_mode_line_respects_risky_local_variable_for_eval_forms() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let ((unsafe-mode-line '(:eval (error "boom")))
                      (trusted-mode-line '(:eval "ok")))
                 (put 'trusted-mode-line 'risky-local-variable t)
                 (list (format-mode-line 'unsafe-mode-line 0)
                       (format-mode-line 'trusted-mode-line 0)))"#
        ),
        r#"OK ("" "ok")"#
    );
}

#[test]
fn vm_format_mode_line_propertize_preserves_text_properties() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((s (format-mode-line '(:propertize "abc" face bold help-echo "h")))
                      (props (text-properties-at 1 s)))
                 (list (substring-no-properties s)
                       (plist-get props 'face)
                       (plist-get props 'help-echo)))"#
        ),
        r#"OK ("abc" bold "h")"#
    );
}

#[test]
fn vm_format_mode_line_percent_specs_preserve_source_string_text_properties() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-prop-buffer")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (let* ((fmt (propertize "%b!" 'face 'bold 'help-echo "h"))
                        (s (format-mode-line fmt))
                        (props0 (text-properties-at 0 s))
                        (props1 (text-properties-at (1- (length s)) s)))
                   (list (substring-no-properties s)
                         (plist-get props0 'face)
                         (plist-get props0 'help-echo)
                         (plist-get props1 'face)
                         (plist-get props1 'help-echo))))"#
        ),
        r#"OK ("vm-prop-buffer!" bold "h" bold "h")"#
    );
}

#[test]
fn vm_format_mode_line_status_specs_match_gnu_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-status-buffer")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert "abc")
                 (setq buffer-read-only t)
                 (let ((status (format-mode-line "%*|%+|%&" 0)))
                   (setq buffer-read-only nil)
                   (set-buffer-modified-p nil)
                   (narrow-to-region 2 3)
                   (list status (format-mode-line "%n" 0))))"#
        ),
        r#"OK ("%|*|*" " Narrow")"#
    );
}

#[test]
fn vm_format_mode_line_face_argument_merges_explicit_faces_and_can_drop_props() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((with-face (format-mode-line
                                '((:propertize "a" face italic) "b")
                                'bold))
                      (with-face-props-0 (text-properties-at 0 with-face))
                      (with-face-props-1 (text-properties-at 1 with-face))
                      (no-props (format-mode-line
                                 '(:propertize "abc" face bold help-echo "h")
                                 0)))
                 (list (substring-no-properties with-face)
                       (plist-get with-face-props-0 'face)
                       (plist-get with-face-props-1 'face)
                       (substring-no-properties no-props)
                       (text-properties-at 0 no-props)))"#
        ),
        r#"OK ("ab" (italic bold) bold "abc" nil)"#
    );
}

#[test]
fn vm_format_mode_line_fixnum_padding_does_not_inherit_inner_properties() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((s (format-mode-line '(5 (:propertize "x" face bold))))
                      (props0 (text-properties-at 0 s))
                      (props1 (text-properties-at 1 s)))
                 (list (substring-no-properties s)
                       (plist-get props0 'face)
                       (plist-get props1 'face)))"#
        ),
        r#"OK ("x    " bold nil)"#
    );
}

#[test]
fn vm_format_mode_line_recursive_depth_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_with_init_str(r#"(format-mode-line "%[|%]" 0)"#, |eval| {
            eval.command_loop_mut().recursive_depth = 4;
        }),
        r#"OK "[[[|]]]""#
    );
    assert_eq!(
        vm_eval_interactive_with_init_str(r#"(format-mode-line "%[|%]" 0)"#, |eval| {
            eval.command_loop_mut().recursive_depth = 7;
        }),
        r#"OK "[[[... | ...]]]""#
    );
}

#[test]
fn vm_format_mode_line_column_and_mode_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-col-mode")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert "abcdef")
                 (goto-char 4)
                 (format-mode-line "%c|%C|%m" 0))"#
        ),
        r#"OK "3|4|Fundamental""#
    );
}

#[test]
fn vm_format_mode_line_coding_and_remote_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-coding-remote")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (set (make-local-variable 'buffer-file-coding-system) 'utf-8-unix)
                 (format-mode-line "%z|%Z|%@" 0))"#
        ),
        // kbd/term prefix is "=–": the bare VM harness runs no
        // set-locale-environment, so the per-terminal codings keep their
        // create_terminal defaults (terminal.c:309-318) — keyboard
        // no-conversion (`=`), terminal undecided (`-`) — while the
        // buffer's utf-8-unix supplies the trailing U.  GNU answers the
        // same triple under a C-locale TTY with this buffer.
        r#"OK "=-U|=-U:|-""#
    );
}

#[test]
fn vm_format_mode_line_position_o_and_q_specs() {
    crate::test_utils::init_test_tracing();
    // With content and window covering the full buffer → "All"
    assert_eq!(
        vm_eval_interactive_str(
            r#"(let* ((w (selected-window))
                      (b (get-buffer-create "vm-pos-oq")))
                 (set-window-buffer w b)
                 (set-buffer b)
                 (erase-buffer)
                 (insert (make-string 100 ?x))
                 (set-window-start w (point-min) t)
                 (format-mode-line "%o|%q" 0 w))"#
        ),
        // window_start=begv, window_end=zv → full buffer visible → All
        r#"OK "All|All   ""#
    );
}

#[test]
fn vm_xdisp_query_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (invisible-p 1)
                 (invisible-p -1)
                 (line-pixel-height)
                 (lookup-image-map 'map 10 20)
                 (current-bidi-paragraph-direction)
                 (bidi-resolved-levels 0)
                 (line-number-display-width t)
                 (long-line-optimizations-p)
                 (condition-case err (move-point-visually 1) (error err))
                 (condition-case err (move-to-window-line 0) (error err)))"#
        ),
        r#"OK (nil t 1 nil left-to-right nil 0 nil (args-out-of-range 1 1) (error "move-to-window-line called from unrelated buffer"))"#
    );
}

#[test]
fn vm_terminal_query_builtins_accept_live_frame_designators() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (list
                  (terminal-name f)
                  (terminal-live-p f)
                  (terminal-live-p (frame-terminal f))
                  (terminal-live-p (car (terminal-list)))
                  (set-terminal-parameter f 'vm-terminal-query-test 7)
                  (terminal-parameter f 'vm-terminal-query-test)
                  (cdr (assq 'vm-terminal-query-test (terminal-parameters f)))
                  (tty-top-frame f)
                  (tty-display-color-p f)
                  (tty-display-color-cells f)
                  (tty-no-underline f)
                  (controlling-tty-p f)))"#
        ),
        r#"OK ("initial_terminal" t t t nil 7 7 nil nil 0 nil nil)"#
    );
}

#[test]
fn vm_x_display_query_builtins_reject_non_window_system_frame_designators() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (setq window-system 'x)
                 (list
                  (condition-case err (x-display-pixel-width f) (error err))
                  (condition-case err (x-display-pixel-height f) (error err))
                  (condition-case err (x-server-version f) (error err))
                  (condition-case err (x-server-max-request-size f) (error err))
                  (condition-case err (x-display-grayscale-p f) (error err))
                  (condition-case err (x-display-backing-store f) (error err))
                  (condition-case err (x-display-color-cells f) (error err))
                  (condition-case err (x-display-mm-height f) (error err))
                  (condition-case err (x-display-mm-width f) (error err))
                  (condition-case err (x-display-monitor-attributes-list f) (error err))
                  (condition-case err (x-display-planes f) (error err))
                  (condition-case err (x-display-save-under f) (error err))
                  (condition-case err (x-display-screens f) (error err))
                  (condition-case err (x-display-visual-class f) (error err))
                  (condition-case err (x-server-input-extension-version f) (error err))))"#
        ),
        r#"OK ((error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used"))"#
    );
}

#[test]
fn vm_x_display_stub_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (display-supports-face-attributes-p '(:weight bold) 999999)
                 (x-display-list)
                 (x-parse-geometry "80x24+10+20")
                 (x-selection-exists-p 'PRIMARY 'STRING)
                 (x-selection-owner-p 'PRIMARY 1)
                 (x-uses-old-gtk-dialog)
                 (x-disown-selection-internal 'PRIMARY)
                 (condition-case err (x-register-dnd-atom 'ATOM) (error err))
                 (condition-case err (x-focus-frame (selected-frame)) (error err))
                 (condition-case err (x-get-atom-name 'WM_CLASS) (error err))
                 (condition-case err (x-window-property "WM_NAME") (error err))
                 (condition-case err (x-window-property-attributes "WM_NAME") (error err))
                 (condition-case err (x-show-tip "hi") (error err))
                 (condition-case err (x-translate-coordinates nil 0 0) (error err))
                 (condition-case err (x-synchronize nil) (error err))
                 (condition-case err (x-get-selection-internal 'PRIMARY 'STRING) (error err))
                 (condition-case err (x-own-selection-internal 'PRIMARY "v") (error err)))"#
        ),
        r#"OK (nil nil ((height . 24) (width . 80) (top . 20) (left . 10)) nil nil nil nil (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "X windows are not in use or not initialized") (error "X windows are not in use or not initialized") (error "X selection unavailable for this frame") (error "X selection unavailable for this frame"))"#
    );
}

#[test]
fn vm_x_connection_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (setq initial-window-system 'x)
                 (list
                  (condition-case err (x-open-connection nil) (error err))
                  (condition-case err (x-close-connection f) (error err))
                  (condition-case err (x-close-connection "x") (error err))
                  (condition-case err (x-open-connection "x") (error err))))"#
        ),
        r#"OK ((wrong-type-argument stringp nil) (error "Window system frame should be used") (error "Display x can’t be opened") nil)"#
    );
}

#[test]
fn vm_x_frame_property_and_tty_stub_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (setq initial-window-system 'x)
                 (list
                  (x-frame-edges f)
                  (x-frame-geometry f)
                  (condition-case err (x-frame-list-z-order f) (error err))
                  (condition-case err (x-frame-restack f f) (error err))
                  (condition-case err (x-export-frames f) (error err))
                  (condition-case err (x-get-modifier-masks f) (error err))
                  (x-family-fonts nil f)
                  (x-mouse-absolute-pixel-position)
                  (x-set-mouse-absolute-pixel-position 1 2)
                  (x-internal-focus-input-context f)
                  (condition-case err (x-wm-set-size-hint f) (error err))
                  (condition-case err (x-get-atom-name 'WM_CLASS f) (error err))
                  (condition-case err (x-window-property "WM_NAME" f) (error err))
                  (condition-case err (x-window-property-attributes "WM_NAME" f) (error err))
                  (tty--output-buffer-size f)
                  (tty--set-output-buffer-size f 7)
                  (tty-display-pixel-height f)
                  (tty-display-pixel-width f)
                  (tty-frame-at 0 0)
                  (tty-frame-edges f nil)
                  (tty-frame-geometry f)
                  (tty-frame-list-z-order f)
                  (condition-case err (tty-frame-restack f f t) (error err))
                  (tty-suppress-bold-inverse-default-colors f)))"#
        ),
        r#"OK (nil nil (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") nil nil nil nil (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") (error "Window system frame should be used") 0 nil 25 80 (#<frame F1 0x100000000> 0 0) nil nil (#<frame F1 0x100000000>) (error "tty-frame-restack is not implemented") nil)"#
    );
}

#[test]
fn vm_remaining_display_stub_tail_uses_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((f (selected-frame)))
                 (setq initial-window-system 'x)
                 (list
                 (condition-case err (x-display-set-last-user-time nil f) (error err))
                  (x-load-color-file "/definitely/not/found")
                  (display--line-is-continued-p)
                  (display--update-for-mouse-movement f 1 2)
                  (x-begin-drag 'drag)
                  (x-double-buffered-p)
                  (x-double-buffered-p f)
                  (x-menu-bar-open-internal)
                  (x-menu-bar-open-internal f)
                  ;; ledger 190: neither name occurs anywhere in GNU's src/
                  ;; or lisp/; both were nil stubs and are deleted.
                  (fboundp 'x-scroll-bar-foreground)
                  (fboundp 'x-scroll-bar-background)))"#
        ),
        r#"OK ((error "Window system frame should be used") nil nil nil nil nil nil nil nil nil nil)"#
    );
}

#[test]
fn vm_image_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((spec (list 'image :type 'png :file "test.png")))
                 (list
                  (condition-case err (image-size spec) (error err))
                  (condition-case err (image-mask-p spec) (error err))
                  (image-flush spec t)
                  (image-cache-size)
                  (image-metadata 1)
                  (condition-case err (image-metadata spec) (error err))
                  (imagep spec)
                  (imagep 1)
                  (image-transforms-p)
                  (condition-case err (image-transforms-p 1) (error err))))"#
        ),
        r#"OK ((error "Window system frame should be used") (error "Window system frame should be used") nil 0 nil (error "Window system frame should be used") t nil nil (wrong-type-argument frame-live-p 1))"#
    );
}

#[test]
fn vm_set_buffer_multibyte_uses_shared_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (set-buffer-multibyte nil)
                 (insert-byte 200 1)
                 (let ((unibyte (append (buffer-string) nil)))
                   (erase-buffer)
                   (list unibyte
                         (set-buffer-multibyte 'foo)
                         (progn
                           (insert-byte 200 1)
                           (append (buffer-string) nil)))))"#
        ),
        r#"OK ((200) foo (4194248))"#
    );
}

#[test]
fn vm_vertical_motion_wraps_long_screen_lines_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (delete-other-windows-internal)
                 (insert (make-string 300 ?x))
                 (goto-char 1)
                 (let ((forward (list (window-body-width)
                                      (vertical-motion 2)
                                      (point)
                                      (current-column))))
                   (vertical-motion -1)
                   (list forward
                         (point)
                         (current-column))))"#
        ),
        "OK ((80 2 159 158) 80 79)"
    );
}

#[test]
fn vm_vertical_motion_large_downward_motion_lands_at_eob_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (delete-other-windows-internal)
                 (insert (make-string 200 ?x))
                 (goto-char 1)
                 (list (window-body-width)
                       (buffer-size)
                       (vertical-motion (buffer-size))
                       (point)))"#
        ),
        "OK (80 200 2 201)"
    );
}

#[test]
fn vm_vertical_motion_zero_moves_to_current_screen_line_start() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 ;; `delete-other-windows' is lisp/window.el:4453 and has no
                 ;; subr any more (DIVERGENCES.md 154); its body ends in this C
                 ;; primitive (src/window.c:3463).
                 (delete-other-windows-internal)
                 (insert (make-string 200 ?x))
                 (goto-char 81)
                 (list (current-column)
                       (vertical-motion 0)
                       (point)
                       (current-column)))"#
        ),
        "OK (80 0 80 79)"
    );
}

#[test]
fn vm_split_window_inherits_selected_buffer_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((b (get-buffer-create " *probe-wrap-narrow*")))
                 (unwind-protect
                     (progn
                       (delete-other-windows-internal)
                       ;; `switch-to-buffer' is lisp/window.el:9558 and has no
                       ;; subr any more (DIVERGENCES.md 154); these are the C
                       ;; primitives its body calls.
                       (set-window-buffer (selected-window) b)
                       (select-window (selected-window))
                       (set-buffer b)
                       (erase-buffer)
                       (insert (make-string 200 ?x))
                       (let ((w2 ((lambda (old size side normal) (set-window-new-pixel old (- (if (memq side '(t left right)) (window-pixel-width old) (window-pixel-height old)) size)) (split-window-internal old size side normal)) nil 40 'right nil)))
                         (list (window-point w2)
                               (progn
                                 (select-window w2)
                                 (point)))))
                   (if (buffer-live-p b)
                       (kill-buffer b))
                   (delete-other-windows-internal)))"#
        ),
        "OK (201 201)"
    );
}

#[test]
fn vm_compiled_load_uses_shared_runtime_and_restores_load_file_name() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-bytecode-shared-load.el");
    std::fs::write(&fixture, "(setq vm-bytecode-load-seen load-file-name)\n")
        .expect("write load fixture");

    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );

    let result = eval
        .eval_str(
            "(list
           (load \"vm-bytecode-shared-load\" nil nil nil nil)
           vm-bytecode-load-seen
           load-file-name)",
        )
        .expect("load should resolve path and execute through shared runtime");

    assert_eq!(
        result,
        Value::list(vec![
            Value::T,
            Value::string(fixture.to_string_lossy()),
            Value::NIL,
        ])
    );
    assert!(
        eval.loads_in_progress().is_empty(),
        "compiled load should unwind shared loads-in-progress state"
    );
}

#[test]
fn vm_compiled_load_allows_gnu_normal_recursive_load_depth() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-bytecode-load.el");
    std::fs::write(&fixture, "(setq vm-bytecode-load-ran t)\n").expect("write load fixture");
    let fixture = fixture.canonicalize().expect("canonical load fixture");
    let fixture_lisp =
        neovm_value::heap_types::LispString::from_utf8(fixture.to_string_lossy().as_ref());

    let mut eval = crate::test_utils::new_context();
    loads_in_progress_push_for_test(&mut eval, fixture_lisp.clone());

    let result = eval
        .eval_str(&format!(
            "(progn
           (setq vm-bytecode-load-ran nil)
           (load {:?} nil nil t)
           vm-bytecode-load-ran)",
            fixture.to_string_lossy()
        ))
        .expect("load should allow GNU's normal recursive depth");

    assert_eq!(
        result,
        Value::T,
        "compiled load should proceed until GNU's recursive-load limit is exceeded"
    );
    assert_eq!(
        eval.loads_in_progress(),
        vec![fixture_lisp],
        "compiled load should restore the caller's loads_in_progress stack"
    );
}

#[test]
fn vm_compiled_load_signals_after_gnu_recursive_load_limit() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-bytecode-recursive-limit.el");
    std::fs::write(&fixture, "(setq vm-bytecode-load-ran t)\n").expect("write load fixture");
    let fixture = fixture.canonicalize().expect("canonical load fixture");
    let fixture_lisp =
        neovm_value::heap_types::LispString::from_utf8(fixture.to_string_lossy().as_ref());

    let mut eval = crate::test_utils::new_context();
    // The Vec stores loads oldest-first; GNU's `Vloads_in_progress` cons
    // list holds them newest-first, so the signal payload must reverse it.
    // The two distinct outer entries make the order observable.
    let older = neovm_value::heap_types::LispString::from_utf8("/nonexistent/older.el");
    let newest = neovm_value::heap_types::LispString::from_utf8("/nonexistent/newest.el");
    for entry in [
        older.clone(),
        fixture_lisp.clone(),
        fixture_lisp.clone(),
        fixture_lisp.clone(),
        fixture_lisp.clone(),
        newest.clone(),
    ] {
        loads_in_progress_push_for_test(&mut eval, entry);
    }

    let err = eval
        .eval_str(&format!(
            r#"(load {:?} nil nil t)"#,
            fixture.to_string_lossy()
        ))
        .expect_err("load should signal once GNU's recursive-load limit is exceeded");

    match err {
        EvalError::Signal { symbol, data, .. } => {
            assert_eq!(resolve_sym(symbol), "error");
            // GNU `signal_error ("Recursive load", Fcons (found, loads))`
            // (src/lread.c:1314) splices the proper list: the first data
            // element after the message is the found filename itself, not a
            // nested list (src/eval.c:2105-2111).
            assert_eq!(data[0].as_utf8_str(), Some("Recursive load"));
            assert_eq!(data[1], Value::string(fixture.to_string_lossy()));
            // Newest-first cons order: the newest outer load, the four
            // copies of `found`, then the oldest entry.
            assert_eq!(data[2], Value::string("/nonexistent/newest.el"));
            for entry in &data[3..7] {
                assert_eq!(*entry, Value::string(fixture.to_string_lossy()));
            }
            assert_eq!(data[7], Value::string("/nonexistent/older.el"));
            assert_eq!(data.len(), 8);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn vm_compiled_load_tracks_recursive_state_by_found_filename_text() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let inner = dir.path().join("inner");
    std::fs::create_dir_all(&inner).expect("create inner dir");
    let fixture = inner.join("vm-bytecode-found.el");
    std::fs::write(&fixture, "(setq vm-bytecode-found-ran t)\n").expect("write load fixture");
    let canonical = fixture.canonicalize().expect("canonical load fixture");
    let canonical_lisp =
        neovm_value::heap_types::LispString::from_utf8(canonical.to_string_lossy().as_ref());
    let alternate = dir
        .path()
        .join("inner")
        .join("..")
        .join("inner")
        .join("vm-bytecode-found.el");

    let mut eval = crate::test_utils::new_context();
    loads_in_progress_push_for_test(&mut eval, canonical_lisp.clone());

    let result = eval
        .eval_str(&format!(
            "(progn
               (setq vm-bytecode-found-ran nil)
               (load {:?} nil nil t)
               vm-bytecode-found-ran)",
            alternate.to_string_lossy()
        ))
        .expect("textually distinct found filename should not trip recursive load");

    assert_eq!(result, Value::T);
    assert_eq!(
        eval.loads_in_progress(),
        vec![canonical_lisp],
        "load should restore the caller's in-progress stack after using a distinct found filename",
    );
}
