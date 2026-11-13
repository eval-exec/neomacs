//! VM runtime tests requiring core's canonical upper-layer registration.
//!
//! Eval's `new_vm_runtime_harness` delegates to full `Context::new`.
//! Core's `new_context` constructs that same surface with the canonical
//! hook installation proof. Existing frame and obarray accessors preserve
//! the fixture operations without exposing new private state.

use crate::emacs_core::eval::Context;
use neovm_display::window::DisplayFrameManagerExt;
use neovm_value::display_vocab::WindowSystemKind;
use neovm_value::error::EvalError;
use neovm_value::value::Value;

fn find_bin(name: &str) -> String {
    let path = std::env::var_os("PATH").expect("PATH should be set");
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    panic!("binary {name} not found on PATH");
}

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

fn vm_eval_str(src: &str) -> String {
    with_vm_eval(src, false, |result| {
        crate::emacs_core::format_eval_result(&result)
    })
}

fn vm_eval_lexical_str(src: &str) -> String {
    with_vm_eval(src, true, |result| {
        crate::emacs_core::format_eval_result(&result)
    })
}

fn vm_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::new_context();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn vm_call_interactively_uses_shared_runtime_planning() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (fset 'vm-ci-shared-target '(lambda (x) (interactive (list 7)) x))
               (fset 'vm-ci-shared-alias 'vm-ci-shared-target)
               (list
                 (call-interactively 'vm-ci-shared-alias)
                 (interactive)
                 (condition-case err
                     (call-interactively 'vm-ci-shared-target nil '(1 2))
                   (wrong-type-argument (car err)))))"
        ),
        "OK (7 nil wrong-type-argument)"
    );
}

#[test]
fn vm_call_interactively_restores_command_identity_before_invocation() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (setq this-command 'outer-this
                       this-original-command 'outer-original
                       real-this-command 'outer-real
                       last-command 'outer-last)
                 (fset 'vm-ci-command-state
                       '(lambda (argument)
                          (interactive
                           (progn
                             (setq this-command 'inner-this
                                   this-original-command 'inner-original
                                   real-this-command 'inner-real
                                   last-command 'inner-last)
                             (list 7)))
                          (list argument
                                this-command
                                this-original-command
                                real-this-command
                                last-command)))
                 (list (call-interactively 'vm-ci-command-state)
                       this-command
                       this-original-command
                       real-this-command
                       last-command))"#
        ),
        "OK ((7 outer-this outer-original outer-real outer-last) outer-this outer-original outer-real outer-last)"
    );
}

#[test]
fn vm_call_interactively_resolves_noninteractive_autoload_before_commandp() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("vm-ci-late-command.el");
    std::fs::write(
        &fixture,
        "(defalias 'vm-ci-late-command
           #'(lambda () (interactive) 'loaded-command))\n",
    )
    .expect("write call-interactively autoload fixture");

    let mut eval = crate::test_utils::new_context();
    eval.obarray_mut().set_symbol_value(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );

    let result = eval
        .eval_str(
            "(progn
               (autoload 'vm-ci-late-command \"vm-ci-late-command\" nil nil)
               (call-interactively 'vm-ci-late-command))",
        )
        .expect("call-interactively should resolve the non-interactive autoload");

    assert_eq!(result, Value::symbol("loaded-command"));
}

#[test]
fn vm_call_interactively_propagates_noninteractive_autoload_failure_before_commandp() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (autoload 'vm-ci-missing-plain
                         \"vm-ci-missing-plain-library\"
                         nil nil)
               (condition-case error
                   (call-interactively 'vm-ci-missing-plain)
                 (error (car error))))"
        ),
        "OK file-missing"
    );
}

#[test]
fn vm_call_interactively_uses_one_resolved_interactive_form() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let ((original (symbol-function 'interactive-form))
                   (calls 0))
               (unwind-protect
                   (progn
                     (fset 'interactive-form
                           (lambda (command)
                             (setq calls (1+ calls))
                             (funcall original command)))
                     (fset 'vm-ci-single-form
                           (lambda (value) (interactive (list 9)) value))
                     (list (call-interactively 'vm-ci-single-form) calls))
                 (fset 'interactive-form original)
                 (fmakunbound 'vm-ci-single-form)))"
        ),
        "OK (9 1)"
    );
}

#[test]
fn vm_call_interactively_accepts_symbol_interactive_form_property() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(progn
               (fset 'vm-ci-property-command (lambda (value) value))
               (put 'vm-ci-property-command
                    'interactive-form
                    '(interactive (list 9)))
               (unwind-protect
                   (call-interactively 'vm-ci-property-command)
                 (fmakunbound 'vm-ci-property-command)))"
        ),
        "OK 9"
    );
}

#[test]
fn vm_call_interactively_honors_function_cell_mutation_during_interactive_spec_eval() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (fset 'vm-ci-mutate
                       '(lambda ()
                          (interactive
                           (progn
                             (fset 'vm-ci-mutate (lambda () 42))
                             (list)))
                          7))
                 (unwind-protect
                     (call-interactively 'vm-ci-mutate)
                   (fmakunbound 'vm-ci-mutate)))"#
        ),
        "OK 42"
    );
}

#[test]
fn vm_call_interactively_executes_raw_lambda_commands_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            "(let ((current-prefix-arg 3))
               (call-interactively '(lambda (n) (interactive \"p\") n)))"
        ),
        "OK 3"
    );
}

#[test]
fn vm_call_interactively_form_spec_uses_interpreted_closure_environment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            r#"(let ((command
                       (let ((captured 37))
                         (lambda (value)
                           (interactive (list captured))
                           value))))
                 (call-interactively command))"#
        ),
        "OK 37"
    );
}

#[test]
fn vm_call_interactively_quoted_lambda_form_spec_uses_dynamic_environment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_lexical_str(
            r#"(let ((ambient-only 99))
                 (call-interactively
                  '(lambda (value)
                     (interactive
                      (list
                       (condition-case nil
                           ambient-only
                         (void-variable 'dynamic))))
                     value)))"#
        ),
        "OK dynamic"
    );
}

#[test]
fn vm_call_interactively_handles_optional_coding_without_prefix_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((current-prefix-arg nil)
                     (unread-command-events '(97)))
                 (list
                  (call-interactively '(lambda (coding) (interactive "ZCoding: ") coding))
                  unread-command-events))"#
        ),
        "OK (nil (97))"
    );
}

#[test]
fn vm_call_interactively_handles_k_k_capital_and_u_specs_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (progn
                   (use-global-map (make-sparse-keymap))
                   (fset 'mouse-drag-region (lambda (&rest _args) nil))
                   (fset 'mouse-set-point (lambda (&rest _args) nil))
                   (define-key (current-global-map) [down-mouse-1] #'mouse-drag-region)
                   (define-key (current-global-map) [mouse-1] #'mouse-set-point)
                   nil)
                 (let ((unread-command-events (list '(down-mouse-1) '(mouse-1))))
                   (call-interactively
                    '(lambda (keys up) (interactive "k
U") (list keys up))))
                 (let ((unread-command-events (list '(down-mouse-1) '(mouse-1))))
                   (call-interactively
                    '(lambda (keys up) (interactive "K
U") (list keys up)))))"#
        ),
        "OK (nil ([(down-mouse-1)] [(mouse-1)]) ([(down-mouse-1)] [(mouse-1)]))"
    );
}

#[test]
fn vm_call_interactively_handles_prompt_driven_batch_specs_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list
                 (let ((unread-command-events (list 97)))
                   (call-interactively '(lambda (x) (interactive "cChar: ") x)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "aFunction: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "bBuffer: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "BAny buffer: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "CCommand: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "DDirectory: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "fFind file: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "FFind file: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "GFind file: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "sString: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "MInherited: ") x))
                   (error (car err)))
                 ;; `n' is not here: it dispatches through the `read-number'
                 ;; FUNCTION CELL (src/callint.c:645), and `read-number' is
                 ;; lisp/subr.el:3725, so a bare evaluator has nothing to call
                 ;; -- exactly as GNU before subr.el.  Its rows are in
                 ;; `vm_call_interactively_number_specs_need_subr_el'
                 ;; (DIVERGENCES.md 152).
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "SSymbol: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "zCoding: ") x))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (x) (interactive "vVariable: ") x))
                   (error (car err))))"#,
            |eval| {
                // GNU defines `read-file-name' in minibuffer.el, outside this
                // harness; its batch behaviour is `completing-read' -> EOF.
                let defined = eval.eval_str(
                    "(fset 'read-file-name #'(lambda (prompt &optional dir default-filename mustmatch initial predicate) \
                       (completing-read prompt nil predicate mustmatch initial 'file-name-history default-filename)))",
                );
                assert!(defined.is_ok(), "define read-file-name shim: {defined:?}");
            },
        ),
        "OK (97 end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file)"
    );
}

#[test]
fn vm_call_interactively_handles_optional_coding_prompt_cases_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (let ((unread-command-events (list 97)))
                   (list
                    (call-interactively '(lambda (c) (interactive "ZCoding: ") c))
                    unread-command-events))
                 (let ((current-prefix-arg '(4)))
                   (condition-case err
                       (call-interactively '(lambda (c) (interactive "ZCoding: ") c))
                     (error (car err)))))"#
        ),
        "OK ((nil (97)) nil)"
    );
}

#[test]
fn vm_call_interactively_handles_r_capital_spec_via_use_region_p_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list
                 (progn
                   (fset 'use-region-p (lambda () nil))
                   (call-interactively
                    '(lambda (beg end) (interactive "R") (list beg end))))
                 (progn
                   (fset 'use-region-p (lambda () t))
                   (call-interactively
                    '(lambda (beg end) (interactive "R") (list beg end)))))"#,
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
            },
        ),
        "OK ((nil nil) (2 3))"
    );
}

#[test]
fn vm_call_interactively_handles_expression_prompt_specs_on_shared_runtime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (condition-case err
                     (call-interactively '(lambda (expr) (interactive "xExpr: ") expr))
                   (error (car err)))
                 (condition-case err
                     (call-interactively '(lambda (value) (interactive "XExpr: ") value))
                   (error (car err))))"#
        ),
        "OK (end-of-file end-of-file)"
    );
}

#[test]
fn vm_call_process_builtins_use_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    let echo = find_bin("echo");
    let form = format!(
        r#"(let ((src (get-buffer-create "vm-cp-src"))
                 (dst (get-buffer-create "vm-cp-dst")))
             (set-buffer src)
             (erase-buffer)
             (set-buffer dst)
             (erase-buffer)
             (set-buffer src)
             (list
               (call-process "{echo}" nil t nil "hello")
               (buffer-string)
               (call-process "{echo}" nil "vm-cp-dst" nil "other")
               (progn (set-buffer dst) (buffer-string))
               (progn
                 (set-buffer src)
                 (call-process "{echo}" nil nil nil "drop"))
               (buffer-string)))"#
    );
    let result = vm_eval_str(&form);

    assert_eq!(
        result,
        r#"OK (0 "hello
" 0 "other
" 0 "hello
")"#
    );
}

#[test]
fn vm_call_process_region_builtins_use_shared_buffer_state() {
    crate::test_utils::init_test_tracing();
    let cat = find_bin("cat");
    assert_eq!(
        vm_eval_str(&format!(
            r#"(let ((b1 (get-buffer-create "vm-cpr-1"))
                     (b2 (get-buffer-create "vm-cpr-2"))
                     (b3 (get-buffer-create "vm-cpr-3"))
                     (b4 (get-buffer-create "vm-cpr-4")))
                 (list
                   (progn
                     (set-buffer b1)
                     (erase-buffer)
                     (insert "abc")
                     (list (call-process-region "xyz" nil "{cat}" nil t nil)
                           (buffer-string)))
                   (progn
                     (set-buffer b2)
                     (erase-buffer)
                     (insert "abcdef")
                     (goto-char 3)
                     (let ((m (copy-marker (point))))
                       (list (call-process-region m (point-max) "{cat}" nil t nil)
                             (buffer-string))))
                   (progn
                     (set-buffer b3)
                     (erase-buffer)
                     (insert "abcde")
                     (narrow-to-region 2 4)
                     (list (call-process-region nil nil "{cat}" nil t nil)
                           (buffer-string)))
                   (progn
                     (set-buffer b4)
                     (erase-buffer)
                     (insert "abc")
                     (list (call-process-region (point-max) (point-min) "{cat}" t t nil)
                           (buffer-string)))))"#
        )),
        r#"OK ((0 "abcxyz") (0 "abcdefcdef") (0 "bcabcde") (0 "abc"))"#
    );
}

#[test]
fn vm_gui_display_capability_builtins_use_live_window_system_state() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(progn
             (setq initial-window-system nil)
             (setq window-system nil)
             (let ((f (selected-frame)))
               (list
                (xw-display-color-p f)
                ;; `display-color-cells' is lisp/frame.el:2966 and has no
                ;; Rust subr (DIVERGENCES.md 157).  This VM runtime is
                ;; deliberately minimal, so ask the C primitive the `memq'
                ;; arm of its body calls for a `neo' frame:
                ;; `x-display-color-cells' (src/xfns.c:5714).
                (x-display-color-cells f))))"#,
        |eval| {
            let scratch = eval
                .buffer_manager()
                .current_buffer_id()
                .expect("current buffer");
            eval.frame_manager_mut()
                .create_frame("vm-gui", 960, 640, scratch);
            // GNU's Lisp `window-system` frame parameter never changes a
            // frame's output method (src/frame.c:415-436); a backend-less GUI
            // fixture installs the kind through the typed constructor.
            let selected = eval
                .frame_manager()
                .selected_frame()
                .expect("selected frame")
                .id;
            eval.frame_manager_mut()
                .get_mut(selected)
                .expect("selected frame")
                .install_backend_less_gui_kind(WindowSystemKind::Neo);
        },
    );
    assert_eq!(result, r#"OK (t 16777216)"#);
}

#[test]
fn vm_kill_buffer_uses_shared_manager_and_frame_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((a (get-buffer-create "vm-kill-a"))
                     (b (get-buffer-create "vm-kill-b")))
                 (set-buffer a)
                 (list
                  (kill-buffer nil)
                  (buffer-live-p a)
                  (let ((current (current-buffer)))
                    (list (null (eq current a))
                          (buffer-live-p current)))
                  (condition-case err
                      (kill-buffer "vm-kill-missing")
                    (error (car err)))))"#
        ),
        r#"OK (t nil (t t) error)"#
    );
}

#[test]
fn vm_indentation_builtins_use_buffer_local_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list (current-indentation)
                     (current-column)
                     (progn
                       (goto-char 1)
                       (move-to-column 3))
                     (current-column))"#,
            |eval| {
                let current = eval
                    .edit
                    .buffers
                    .current_buffer_id()
                    .expect("scratch buffer");
                let buffer = eval.edit.buffers.get_mut(current).expect("scratch buffer");
                buffer.set_buffer_local("tab-width", Value::fixnum(4));
                buffer.insert("\tb");
                buffer.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(3));
            },
        ),
        "OK (4 5 4 4)"
    );
}

#[test]
fn vm_indent_to_uses_dynamic_indentation_bindings() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((tab-width 4) (indent-tabs-mode t))
                 (list (indent-to 6 1)
                       (current-column)
                       (append (buffer-string) nil)))"#
        ),
        "OK (6 6 (9 32 32))"
    );
}

#[test]
fn vm_buffer_manager_query_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (get-buffer-create "*Messages*")
                 (get-buffer-create "*vm-alt*")
                 (get-buffer-create " hidden")
                 (list
                  (mapcar #'buffer-name (buffer-list))
                  (buffer-name (other-buffer "*vm-alt*"))
                  (buffer-name (other-buffer "*vm-alt*" t))
                  (generate-new-buffer-name "*vm-alt*" "*vm-alt*<2>")))"#
        ),
        r#"OK (("*scratch*" "*Messages*" "*vm-alt*" " hidden") "*Messages*" "*scratch*" "*vm-alt*<2>")"#
    );
}

#[test]
fn vm_vertical_motion_does_not_count_missing_previous_line_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "abc\ndef\n")
                 (let (first second-bol second-mid)
                   (goto-char 3)
                   (setq first (list (vertical-motion -1)
                                     (point)
                                     (line-number-at-pos)
                                     (current-column)))
                   (goto-char 5)
                   (setq second-bol (list (vertical-motion -1)
                                          (point)
                                          (line-number-at-pos)
                                          (current-column)))
                   (goto-char 6)
                   (setq second-mid (list (vertical-motion -1)
                                          (point)
                                          (line-number-at-pos)
                                          (current-column)))
                   (list first second-bol second-mid)))"#
        ),
        "OK ((0 1 1 0) (-1 1 1 0) (-1 1 1 0))"
    );
}
