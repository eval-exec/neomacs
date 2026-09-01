use super::super::eval::Context;
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use super::*;
use crate::test_utils::runtime_startup_eval_all;
use std::fs;
use std::path::PathBuf;

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

fn eval_first_form_after_marker(eval: &mut Context, source: &str, marker: &str) {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("missing GNU simple.el marker: {marker}"));
    let (form, _) = crate::emacs_core::value_reader::read_one(&source[start..], 0, &test_ob())
        .unwrap_or_else(|err| panic!("parse GNU simple.el from {marker} failed: {:?}", err))
        .unwrap_or_else(|| panic!("no GNU simple.el form found after marker: {marker}"));
    eval.eval_form(form)
        .unwrap_or_else(|err| panic!("evaluate GNU simple.el form {marker} failed: {:?}", err));
}

/// Install minimal `defun`/`defmacro`/`when`/`unless` shims so a bare
/// evaluator can evaluate forms extracted from GNU `.el` source files.
/// Also installs a minimal `with-temp-buffer` macro because many GNU
/// elisp helpers (`back-to-indentation`, `indent-region`, etc.) used
/// by these tests are wrapped in `(with-temp-buffer ...)` blocks.
fn install_bare_elisp_shims(ev: &mut Context) {
    let shims = r#"
;; `subr.el' creates these five with `defalias'; GNU has no C version of any
;; of them (lisp/subr.el:71 and :2277-2280), so a bare evaluator standing in
;; for a loaded one has to be given the same aliases subr.el gives it.
;; DIVERGENCES.md 148.
(defalias 'not #'null)
(defalias 'string= #'string-equal)
(defalias 'string< #'string-lessp)
(defalias 'string> #'string-greaterp)
(defalias 'move-marker #'set-marker)
(defalias 'defun (cons 'macro #'(lambda (name arglist &rest body)
  (list 'defalias (list 'quote name) (cons 'function (list (cons 'lambda (cons arglist body))))))))
(defalias 'defmacro (cons 'macro #'(lambda (name arglist &rest body)
  (list 'defalias (list 'quote name)
        (list 'cons ''macro (cons 'function (list (cons 'lambda (cons arglist body)))))))))
(defalias 'when (cons 'macro #'(lambda (cond &rest body)
  (list 'if cond (cons 'progn body)))))
(defalias 'unless (cons 'macro #'(lambda (cond &rest body)
  (cons 'if (cons cond (cons nil body))))))
(defalias 'with-temp-buffer (cons 'macro #'(lambda (&rest body)
  ;; Minimal shim — uses get-buffer-create with a fixed unique name.
  ;; Sufficient for the bare-shim indent tests, which don't nest
  ;; with-temp-buffer calls.
  (list 'let
        (list (list 'vm-temp-buf
                    (list 'get-buffer-create " *vm-shim-temp*" t)))
        (list 'unwind-protect
              (list 'save-current-buffer
                    (list 'set-buffer 'vm-temp-buf)
                    (list 'erase-buffer)
                    (cons 'progn body))
              (list 'kill-buffer 'vm-temp-buf))))))
"#;
    ev.eval_str(shims).expect("install bare elisp shims");
}

fn gnu_simple_indent_eval() -> Context {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source = fs::read_to_string(&simple_path).expect("read GNU simple.el");

    let mut ev = Context::new();
    install_bare_elisp_shims(&mut ev);
    ev.set_lexical_binding(true);
    eval_first_form_after_marker(&mut ev, &simple_source, "(defun back-to-indentation ()");
    ev
}

fn gnu_indent_el_eval() -> Context {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let indent_path = project_root.join("lisp/indent.el");
    let indent_source = fs::read_to_string(&indent_path).expect("read GNU indent.el");
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source = fs::read_to_string(&simple_path).expect("read GNU simple.el");
    let subr_path = project_root.join("lisp/subr.el");
    let subr_source = fs::read_to_string(&subr_path).expect("read GNU subr.el");
    let syntax_path = project_root.join("lisp/emacs-lisp/syntax.el");
    let syntax_source = fs::read_to_string(&syntax_path).expect("read GNU syntax.el");

    let mut ev = Context::new();
    install_bare_elisp_shims(&mut ev);
    ev.set_lexical_binding(true);
    ev.eval_str(
        r#"
        (setq fill-prefix nil)
        (setq abbrev-mode nil)
        (defvar tab-always-indent t)
        (defvar tab-first-completion nil)
        (fset 'use-region-p (lambda () nil))
        (fset 'make-progress-reporter (lambda (&rest _args) nil))
        (fset 'progress-reporter-update (lambda (&rest _args) nil))
        (fset 'progress-reporter-done (lambda (&rest _args) nil))
        "#,
    )
    .expect("eval progress reporter stubs");
    eval_first_form_after_marker(
        &mut ev,
        &syntax_source,
        "(defvar syntax-propertize-function nil",
    );
    eval_first_form_after_marker(&mut ev, &syntax_source, "(defun syntax-propertize (pos)");
    eval_first_form_after_marker(&mut ev, &indent_source, "(defvar indent-line-function ");
    eval_first_form_after_marker(
        &mut ev,
        &indent_source,
        "(defvar indent-line-ignored-functions ",
    );
    eval_first_form_after_marker(
        &mut ev,
        &indent_source,
        "(defun indent-according-to-mode (&optional inhibit-widen)",
    );
    eval_first_form_after_marker(
        &mut ev,
        &indent_source,
        "(defun indent--default-inside-comment ()",
    );
    eval_first_form_after_marker(
        &mut ev,
        &simple_source,
        "(defun delete-horizontal-space (&optional backward-only)",
    );
    eval_first_form_after_marker(
        &mut ev,
        &simple_source,
        "(defun delete-space--internal (chars backward-only)",
    );
    eval_first_form_after_marker(&mut ev, &subr_source, "(defun cadr (x)");
    eval_first_form_after_marker(&mut ev, &subr_source, "(defun last (list &optional n)");
    eval_first_form_after_marker(&mut ev, &indent_source, "(defun indent-line-to (column)");
    eval_first_form_after_marker(
        &mut ev,
        &indent_source,
        "(defun indent--funcall-widened (func)",
    );
    eval_first_form_after_marker(&mut ev, &indent_source, "(defun insert-tab (&optional arg)");
    eval_first_form_after_marker(
        &mut ev,
        &indent_source,
        "(defun indent-next-tab-stop (column &optional prev)",
    );
    eval_first_form_after_marker(&mut ev, &indent_source, "(defun tab-to-tab-stop ()");
    eval_first_form_after_marker(
        &mut ev,
        &indent_source,
        "(defun indent-region-line-by-line (start end)",
    );
    eval_first_form_after_marker(
        &mut ev,
        &indent_source,
        "(defvar indent-region-function #'indent-region-line-by-line",
    );
    eval_first_form_after_marker(
        &mut ev,
        &indent_source,
        "(defun indent-region (start end &optional column)",
    );
    eval_first_form_after_marker(&mut ev, &indent_source, "(defun indent-relative (&optional");
    eval_first_form_after_marker(
        &mut ev,
        &indent_source,
        "(defun indent-for-tab-command (&optional arg)",
    );
    ev
}

fn eval_all(ev: &mut Context, src: &str) -> Vec<String> {
    ev.eval_str_each(src)
        .iter()
        .map(super::super::format_eval_result)
        .collect()
}

#[test]
fn eval_column_and_indentation_subset() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let col = ev
        .eval_str(
            r#"(with-temp-buffer (insert "abc") (goto-char (+ (point-min) 2)) (current-column))"#,
        )
        .expect("eval current-column");
    assert_eq!(col, Value::fixnum(2));

    let indent = ev
        .eval_str(
            r#"(with-temp-buffer (insert "  abc") (goto-char (point-max)) (current-indentation))"#,
        )
        .expect("eval current-indentation");
    assert_eq!(indent, Value::fixnum(2));

    let move_result = ev.eval_str(
        r#"(with-temp-buffer (insert "a\tb") (goto-char (point-min)) (move-to-column 5) (list (point) (current-column)))"#,
    )
    .expect("eval move-to-column");
    let items = list_to_vec(&move_result).expect("list result");
    assert_eq!(items, vec![Value::fixnum(3), Value::fixnum(8)]);
}

#[test]
fn current_column_and_move_to_column_treat_invisible_text_as_zero_width() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let value = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "aaaa\nbbbb\ncccc\n")
                 (put-text-property 5 10 'invisible t)
                 (list
                  (mapcar (lambda (p)
                            (goto-char p)
                            (current-column))
                          (number-sequence 5 11))
                  (progn
                    (goto-char 6)
                    (move-to-column 2)
                    (list (point) (current-column) (char-after)))))"#,
        )
        .expect("invisible column scan");
    assert_eq!(
        super::super::print::print_value(&value),
        "((4 0 0 0 0 0 0) (10 0 10))"
    );
}

#[test]
fn vertical_motion_skips_ellipsis_bearing_invisible_runs() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let value = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "head\nhidden1\nhidden2\ntail\n")
                 (setq buffer-invisibility-spec '((fold . t)))
                 (let ((overlay (make-overlay 5 21)))
                   (overlay-put overlay 'invisible 'fold))
                 (list
                  (progn (goto-char 1) (list (vertical-motion 1) (point)))
                  (progn (goto-char 1) (list (vertical-motion 2) (point)))))"#,
        )
        .expect("ellipsis-bearing invisible vertical motion");
    assert_eq!(super::super::print::print_value(&value), "((1 22) (2 27))");
}

#[test]
fn current_column_and_indentation_handle_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    ev.buffers
        .set_buffer_multibyte_flag(buffer_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    ev.buffers.insert_lisp_string_into_buffer(
        buffer_id,
        &crate::heap_types::LispString::from_unibyte(vec![b' ', b'\t', 0xFF, b'a']),
    );

    let current_column = current_column(&mut ev, vec![]).expect("current-column");
    assert_eq!(current_column, Value::fixnum(13));

    let current_indentation = current_indentation(&mut ev, vec![]).expect("current-indentation");
    assert_eq!(current_indentation, Value::fixnum(8));
}

#[test]
fn move_to_column_handles_unibyte_raw_byte_display_width() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    ev.buffers
        .set_buffer_multibyte_flag(buffer_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    ev.buffers.insert_lisp_string_into_buffer(
        buffer_id,
        &crate::heap_types::LispString::from_unibyte(vec![b' ', b'\t', 0xFF, b'a']),
    );
    ev.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0));

    let reached = move_to_column(&mut ev, vec![Value::fixnum(9)]).expect("move-to-column");
    assert_eq!(reached, Value::fixnum(12));

    let buffer = ev.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.point_emacs_byte_pos().get(), 3);
    let current_column = current_column(&mut ev, vec![]).expect("current-column");
    assert_eq!(current_column, Value::fixnum(12));
}

#[test]
fn eval_move_to_column_wholenump_validation() {
    crate::test_utils::init_test_tracing();
    let mut ev = super::super::eval::Context::new();
    let err = move_to_column(&mut ev, vec![Value::string("x")]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("wholenump"), Value::string("x")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn eval_move_to_column_force_subset() {
    crate::test_utils::init_test_tracing();

    let mut ev = Context::new();
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    ev.buffers
        .insert_into_buffer(buffer_id, "abc")
        .expect("insert text");
    ev.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0));

    let first = move_to_column(&mut ev, vec![Value::fixnum(10), Value::T]).expect("force eol");
    assert_eq!(first, Value::fixnum(10));
    let buffer = ev.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.point_emacs_byte_pos().get(), 6);
    assert_eq!(buffer.buffer_string(), "abc\t  ");

    ev.buffers
        .delete_buffer_emacs_byte_range(
            buffer_id,
            crate::buffer::EmacsByteRange::from_usize(
                0,
                ev.buffers
                    .get(buffer_id)
                    .unwrap()
                    .total_emacs_byte_len()
                    .get(),
            ),
        )
        .expect("clear buffer");
    ev.buffers
        .insert_into_buffer(buffer_id, "a\tb")
        .expect("insert tab text");
    ev.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0));
    let second = move_to_column(&mut ev, vec![Value::fixnum(5), Value::T]).expect("split tab");
    assert_eq!(second, Value::fixnum(5));
    let buffer = ev.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.point_emacs_byte_pos().get(), 5);
    assert_eq!(buffer.buffer_string(), "a    \tb");

    ev.buffers
        .delete_buffer_emacs_byte_range(
            buffer_id,
            crate::buffer::EmacsByteRange::from_usize(
                0,
                ev.buffers
                    .get(buffer_id)
                    .unwrap()
                    .total_emacs_byte_len()
                    .get(),
            ),
        )
        .expect("clear buffer");
    ev.buffers
        .get_mut(buffer_id)
        .expect("buffer")
        .set_buffer_local("indent-tabs-mode", Value::NIL);
    ev.buffers
        .insert_into_buffer(buffer_id, "a\tb\n")
        .expect("insert tab line");
    ev.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0));
    let third =
        move_to_column(&mut ev, vec![Value::fixnum(4), Value::T]).expect("split tab with spaces");
    assert_eq!(third, Value::fixnum(4));
    let buffer = ev.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.point_emacs_byte_pos().get(), 4);
    assert_eq!(buffer.buffer_string(), "a       b\n");
}

#[test]
fn gnu_back_to_indentation_matches_simple_el() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_indent_eval();
    let results = eval_all(
        &mut ev,
        r#"(subrp (symbol-function 'back-to-indentation))
           (with-temp-buffer
             (insert "  abc")
             (goto-char (point-max))
             (back-to-indentation)
             (point))
           (with-temp-buffer
             (insert "   ")
             (goto-char (point-max))
             (back-to-indentation)
             (point))
           (with-temp-buffer
             (insert (string 9 97 98 99))
             (goto-char (point-max))
             (back-to-indentation)
             (point))
           (with-temp-buffer
             (insert (string 10 32 32 97 98 99))
             (goto-char (point-max))
             (back-to-indentation)
             (point))"#,
    );

    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK 3");
    assert_eq!(results[2], "OK 4");
    assert_eq!(results[3], "OK 2");
    assert_eq!(results[4], "OK 4");
}

#[test]
fn gnu_indent_region_matches_indent_el() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_indent_el_eval();
    let first = ev
        .eval_str(
            r#"(with-temp-buffer
          (insert (string 97 10 32 32 98 10 10 9 99))
          (indent-region (point-min) (point-max) 2)
          (append (buffer-string) nil))"#,
        )
        .expect("eval indent-region column");
    assert_eq!(
        list_to_vec(&first).expect("first byte list"),
        vec![
            Value::fixnum(32),
            Value::fixnum(32),
            Value::fixnum(97),
            Value::fixnum(10),
            Value::fixnum(32),
            Value::fixnum(32),
            Value::fixnum(98),
            Value::fixnum(10),
            Value::fixnum(10),
            Value::fixnum(32),
            Value::fixnum(32),
            Value::fixnum(99),
        ]
    );

    let second = ev
        .eval_str(
            r#"(with-temp-buffer
          (insert (string 97 10 32 32 98))
          (indent-region (point-min) (point-max))
          (append (buffer-string) nil))"#,
        )
        .expect("eval indent-region nil column");
    assert_eq!(
        list_to_vec(&second).expect("second byte list"),
        vec![Value::fixnum(97), Value::fixnum(10), Value::fixnum(98)]
    );

    let third = ev
        .eval_str(
            r#"(with-temp-buffer
          (insert (string 97 10 98))
          (indent-region (point-max) (point-min) 1)
          (append (buffer-string) nil))"#,
        )
        .expect("eval indent-region swapped bounds");
    assert_eq!(
        list_to_vec(&third).expect("third byte list"),
        vec![Value::fixnum(97), Value::fixnum(10), Value::fixnum(98)]
    );

    let fourth = ev
        .eval_str(
            r#"(with-temp-buffer
          (insert "a")
          (indent-region (point-min) (point-max) "x"))"#,
        )
        .expect("eval indent-region non-numeric column");
    assert_eq!(fourth, Value::T);
}

#[test]
fn gnu_indent_according_to_mode_matches_indent_el() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_indent_el_eval();
    let first = ev
        .eval_str(
            r#"(with-temp-buffer
          (insert (string 32 32 97))
          (goto-char (point-max))
          (indent-according-to-mode)
          (append (buffer-string) nil))"#,
        )
        .expect("eval indent-according-to-mode");
    assert_eq!(
        list_to_vec(&first).expect("first byte list"),
        vec![Value::fixnum(97)]
    );

    let second = ev
        .eval_str(
            r#"(with-temp-buffer
          (insert (string 32 32 97))
          (goto-char (point-max))
          (indent-according-to-mode)
          (point))"#,
        )
        .expect("eval indent-according-to-mode point");
    assert_eq!(second, Value::fixnum(2));
}

#[test]
fn bootstrap_self_insert_command_uses_last_command_event() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (let ((last-command-event 10))
               (list (self-insert-command 1)
                     (point)
                     (append (buffer-string) nil))))"#,
    );
    assert_eq!(results[0], "OK (nil 2 (10))");
}

#[test]
fn bootstrap_newline_inserts_lf_in_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "ab")
             (goto-char 2)
             (list (newline)
                   (point)
                   (append (buffer-string) nil)))"#,
    );
    assert_eq!(results[0], "OK (nil 3 (97 10 98))");
}

#[test]
fn bootstrap_newline_marker_round_trip_in_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "ab")
             (goto-char 2)
             (let ((pos (point-marker)))
               (newline)
               (goto-char pos)
               (list (point)
                     (marker-position pos)
                     (append (buffer-string) nil))))"#,
    );
    assert_eq!(results[0], "OK (2 2 (97 10 98))");
}

#[test]
fn bootstrap_newline_copy_marker_sequence_matches_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "a b")
             (goto-char 3)
             (let ((pos (point-marker)))
               (newline)
               (save-excursion
                 (goto-char pos)
                 (setq pos (copy-marker pos t))
                 (list (point)
                       (marker-position pos)
                       (marker-insertion-type pos)
                       (append (buffer-string) nil)))))"#,
    );
    assert_eq!(results[0], "OK (3 3 t (97 32 10 98))");
}

#[test]
fn bootstrap_reindent_delete_horizontal_space_step_matches_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "a b")
             (goto-char 3)
             (let ((pos (point-marker)))
               (newline)
               (save-excursion
                 (goto-char pos)
                 (setq pos (copy-marker pos t))
                 (indent-according-to-mode)
                 (goto-char pos)
                 (delete-horizontal-space t))
               (list (point)
                     (append (buffer-string) nil))))"#,
    );
    assert_eq!(results[0], "OK (3 (97 10 98))");
}

#[test]
fn reindent_then_newline_and_indent_normalizes_split_whitespace() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "a b")
             (goto-char 3)
             (list (reindent-then-newline-and-indent)
                   (point)
                   (append (buffer-string) nil)))"#,
    );
    assert_eq!(results[0], "OK (nil 3 (97 10 98))");
}

#[test]
fn wrong_arg_count_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    // current-indentation takes no args
    assert!(current_indentation(&mut eval, vec![Value::fixnum(1)]).is_err());
    // indent-to requires at least 1 arg
    assert!(indent_to(&mut eval, vec![]).is_err());
    // indent-to accepts at most 2 args
    assert!(
        indent_to(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]
        )
        .is_err()
    );
    // current-column takes no args
    assert!(current_column(&mut eval, vec![Value::fixnum(1)]).is_err());
}

#[test]
fn indent_to_rejects_non_integer() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    assert!(indent_to(&mut eval, vec![Value::string("foo")]).is_err());
}

#[test]
fn init_indent_vars_sets_defaults() {
    crate::test_utils::init_test_tracing();
    let mut obarray = super::super::symbol::Obarray::new();
    init_indent_vars(&mut obarray);

    assert_eq!(obarray.symbol_value("tab-width").unwrap().as_int(), Some(8));
    // `indent-tabs-mode' is a GNU `DEFVAR_BOOL' (`src/indent.c:2575') that
    // `bindings.el:1048' then makes buffer-local, so it is declared by
    // `defvar_bool::GNU_BOOL_VARIABLES' rather than here.
    assert_eq!(
        obarray.symbol_value("standard-indent").unwrap().as_int(),
        Some(4)
    );
    assert!(obarray.symbol_value("tab-stop-list").unwrap().is_nil());

    // All should be special (dynamically bound)
    assert!(obarray.is_special("tab-width"));
    assert!(obarray.is_special("standard-indent"));
    assert!(obarray.is_special("tab-stop-list"));
}

#[test]
fn indent_for_tab_command_inserts_tab() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_indent_el_eval();
    let value = ev
        .eval_str(
            r#"(with-temp-buffer
             (insert "x")
             (goto-char 1)
             (indent-for-tab-command)
             (buffer-string))"#,
        )
        .expect("eval");
    assert_eq!(value.as_utf8_str(), Some("\tx"));
}

#[test]
fn eval_indent_to_inserts_padding_and_returns_column() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let first = ev
        .eval_str(
            r#"(with-temp-buffer
             (insert "abcdef")
             (goto-char (point-max))
             (list (current-column)
                   (indent-to 2)
                   (current-column)))"#,
        )
        .expect("first indent-to");
    assert_eq!(super::super::print::print_value(&first), "(6 6 6)");

    let second = ev
        .eval_str(
            r#"(with-temp-buffer
             (list (current-column)
                   (indent-to 2 5)
                   (current-column)))"#,
        )
        .expect("second indent-to");
    assert_eq!(super::super::print::print_value(&second), "(0 5 5)");
}

#[test]
fn eval_indent_to_inherits_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let value = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abc:tag:")
                 (put-text-property 1 4 'p 1)
                 (goto-char 4)
                 (let ((indent-tabs-mode nil))
                   (indent-to 10))
                 (buffer-string))"#,
        )
        .expect("eval");
    assert_eq!(
        super::super::print::print_value(&value),
        r#"#("abc       :tag:" 0 10 (p 1))"#
    );
}

#[test]
fn eval_move_to_column_force_inherits_text_properties_in_both_insertion_branches() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let value = ev
        .eval_str(
            r#"(list
                 (with-temp-buffer
                   (insert (propertize "x" :time "STAMP"))
                   (move-to-column 5 t)
                   (buffer-string))
                 (with-temp-buffer
                   (let ((indent-tabs-mode nil))
                     (insert (propertize "x" :time "STAMP") "\tz")
                     (goto-char (point-min))
                     (move-to-column 5 t)
                     (buffer-string))))"#,
        )
        .expect("move-to-column force property inheritance");
    assert_eq!(
        super::super::print::print_value(&value),
        r#"(#("x    " 0 5 (:time "STAMP")) #("x       z" 0 8 (:time "STAMP")))"#
    );
}

#[test]
fn eval_move_to_column_force_honors_category_rear_nonsticky() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let value = ev
        .eval_str(
            r#"(with-temp-buffer
                  (put 'padding-boundary 'rear-nonsticky t)
                  (insert (propertize "x" 'category 'padding-boundary 'probe t))
                  (let ((indent-tabs-mode nil))
                    (move-to-column 3 t))
                  (list (buffer-substring-no-properties (point-min) (point-max))
                        (get-text-property 1 'probe)
                        (get-text-property 2 'probe)
                        (get-text-property 2 'category)))"#,
        )
        .expect("move-to-column category rear-nonsticky boundary");
    assert_eq!(
        super::super::print::print_value(&value),
        r#"("x  " t nil nil)"#
    );
}

#[test]
fn eval_indent_to_rejects_non_fixnump_minimum() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = ev.eval_str_each(
        r#"(with-temp-buffer (condition-case err (indent-to 4 nil) (error err)))
           (with-temp-buffer (condition-case err (indent-to 4 "x") (error err)))
           (with-temp-buffer (condition-case err (indent-to 4 t) (error err)))
           (with-temp-buffer (condition-case err (indent-to "x") (error err)))"#,
    );
    let printed: Vec<String> = results
        .iter()
        .map(super::super::format_eval_result)
        .collect();

    assert_eq!(printed[0], "OK 4");
    assert_eq!(printed[1], r#"OK (wrong-type-argument fixnump "x")"#);
    assert_eq!(printed[2], "OK (wrong-type-argument fixnump t)");
    assert_eq!(printed[3], r#"OK (wrong-type-argument fixnump "x")"#);
}

#[test]
fn eval_indent_builtins_respect_dynamic_and_buffer_local_settings() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = ev.eval_str_each(
        r#"(let ((tab-width 4))
             (with-temp-buffer
               (insert "a\tb")
               (goto-char (point-min))
               (forward-char 2)
               (list (current-column)
                     (current-indentation)
                     (move-to-column 3)
                     (current-column))))
           (let ((tab-width 4) (indent-tabs-mode t))
             (with-temp-buffer
               (list (indent-to 6 1)
                     (current-column)
                     (append (buffer-string) nil))))
           (with-temp-buffer
             (setq tab-width 4)
             (insert "\tb")
             (goto-char (point-max))
             (list (current-indentation) (current-column)))
           (with-temp-buffer
             (setq tab-width 4)
             (list (local-variable-p 'tab-width (current-buffer))
                   tab-width
                   (default-value 'tab-width)))"#,
    );
    let printed: Vec<String> = results
        .iter()
        .map(super::super::format_eval_result)
        .collect();

    assert_eq!(printed[0], "OK (4 0 4 4)");
    assert_eq!(printed[1], "OK (6 6 (9 32 32))");
    assert_eq!(printed[2], "OK (4 5)");
    assert_eq!(printed[3], "OK (t 4 8)");
}

#[test]
fn indent_for_tab_command_normalizes_leading_whitespace_at_point() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_indent_el_eval();
    let value = ev
        .eval_str(
            r#"(with-temp-buffer
             (insert "  x")
             (goto-char 3)
             (list (indent-for-tab-command) (point) (append (buffer-string) nil)))"#,
        )
        .expect("eval");
    let printed = super::super::print::print_value(&value);
    assert_eq!(printed, "(nil 2 (9 120))");
}

#[test]
fn save_restriction_restores_full_buffer_after_widen_insert() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let value = ev
        .eval_str(
            r#"(with-temp-buffer
             (insert "x")
             (save-restriction
               (widen)
               (goto-char 1)
               (insert "\t"))
             (append (buffer-string) nil))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&value), "(9 120)");
}

// ---------------------------------------------------------------------------
// `(space ...)` and overlay `display` column width (GNU `check_display_width`).
// Expected values verified byte-exact against GNU Emacs 31.0.50 --batch.
// ---------------------------------------------------------------------------

#[test]
fn current_column_counts_display_space_width() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // a(1) + space(5) + c(1) = 7 at end; a(1)+space(5)=6 just before c.
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abc")
                 (put-text-property 2 3 'display '(space :width 5))
                 (list (current-column) (progn (goto-char 3) (current-column))))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "(7 6)");
}

#[test]
fn current_column_counts_display_space_align_to() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // a(1) -> align-to 10 adds (10-1)=9 -> 10, then c -> 11; goto 2 -> col 1.
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abc")
                 (put-text-property 2 3 'display '(space :align-to 10))
                 (list (current-column) (progn (goto-char 2) (current-column))))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "(11 1)");
}

#[test]
fn current_column_counts_display_space_relative_width() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // a(1) + relative-width 2 * width(b=1) = 2 -> 3, then c -> 4; goto 2 -> 1.
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abc")
                 (put-text-property 2 3 'display '(space :relative-width 2))
                 (list (current-column) (progn (goto-char 2) (current-column))))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "(4 1)");
}

#[test]
fn current_column_counts_display_image_as_tty_placeholder() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // GNU's display iterator gives image specs one canonical column on TTY,
    // while the display property still replaces the whole covered range.
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abcdef")
                 (put-text-property 2 4 'display '(image :type xpm :file "test.xpm" :width 10 :height 1))
                 (current-column))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "5");
}

#[test]
fn current_column_counts_display_slice_as_tty_placeholder() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abcdef")
                 (put-text-property 2 5 'display '(slice 0 0 3 1))
                 (list (current-column)
                       (progn (goto-char 5) (current-column))))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "(4 2)");
}

#[test]
fn current_column_float_width_is_not_honored_but_float_align_is() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // GNU: a float `:width` is NOT honored -> char b stays 1 -> a+b+c = 3.
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abc")
                 (put-text-property 2 3 'display '(space :width 2.4))
                 (current-column))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "3");
    // GNU: a float `:align-to` IS honored -> round(7.6)=8, 8-1=7 -> col 8, c -> 9.
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abc")
                 (put-text-property 2 3 'display '(space :align-to 7.6))
                 (current-column))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "9");
}

#[test]
fn move_to_column_stops_at_display_space_run_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // Column 4 falls inside the 5-wide space run; GNU does not split the run and
    // stops at its end (buffer pos 3).
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abc")
                 (put-text-property 2 3 'display '(space :width 5))
                 (move-to-column 4)
                 (point))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "3");
}

#[test]
fn current_column_counts_overlay_display_string() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // overlay display "ZZ" over char b: a(1)+"ZZ"(2)+c(1) = 4; goto 2 -> 1.
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abc")
                 (let ((ov (make-overlay 2 3))) (overlay-put ov 'display "ZZ"))
                 (list (current-column) (progn (goto-char 2) (current-column))))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "(4 1)");
}

#[test]
fn move_to_column_stops_at_overlay_display_run_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // overlay display "XX" over char b (pos 2..3): move-to-column 3 lands inside
    // the run -> stop at run end (pos 3); col 5 reaches char c's tail (pos 4).
    let v = ev
        .eval_str(
            r#"(with-temp-buffer
                 (insert "abcde")
                 (let ((ov (make-overlay 2 3))) (overlay-put ov 'display "XX"))
                 (list (progn (move-to-column 3) (point))
                       (progn (move-to-column 5) (point))))"#,
        )
        .expect("eval");
    assert_eq!(super::super::print::print_value(&v), "(3 5)");
}

// ---------------------------------------------------------------------------
// Ledger 216: the horizontal origin
// ---------------------------------------------------------------------------

/// GNU reaches a `vertical-motion` goal column at `first_x + to_x`
/// (`src/indent.c:2540`), `first_x` being `it.first_visible_x` (`:2321`), i.e.
/// the hscroll.  The scanner in `indent.rs` counts columns from the LINE start,
/// so the window-relative goal has to be moved into that space.
///
/// The table is GNU Emacs 31.0.90's own, 80x24 pty, `truncate-lines' t, a line
/// starting at 202 (`scripts/l216-hscroll-origin-probe.el`, PARTA): at hscroll
/// 5 the goals 0 / 10 / 40 / 79 answer 207 / 217 / 247 / 286, every one
/// `202 + hscroll + goal`.
#[test]
fn a_goal_column_is_window_relative_and_the_walk_is_line_relative() {
    let unscrolled = ScreenLineExtent {
        first_visible_col: 0,
        last_visible_col: 79,
    };
    let scrolled = ScreenLineExtent {
        first_visible_col: 5,
        last_visible_col: 84,
    };
    for goal in [0, 10, 40, 79] {
        assert_eq!(unscrolled.goal_col_in_line_space(goal), goal);
        assert_eq!(scrolled.goal_col_in_line_space(goal), goal + 5);
    }
    // GNU clamps a negative COLS to the row start, not to a negative column.
    assert_eq!(scrolled.goal_col_in_line_space(-3), 5);
    // The edge is the OTHER term and is not the goal's business: a goal past
    // it stays past it, and the walk clamps.
    assert_eq!(scrolled.goal_col_in_line_space(200), 205);
}

/// GNU's WORD_WRAP goal backtrack lives in `move_it_in_display_line`
/// (`src/xdisp.c:10859-10888`), the CALLER of the walk, and applies only to a
/// row the walk left by continuation.  Measured, GNU Emacs 31.0.90, 80x24 pty,
/// one 300-character line of `x' with no wrap opportunity in it
/// (`tmp/l216/wrapgoal-gnu-cold.txt`): goal 80 answers 80 with `word-wrap' nil
/// and 79 with it t, while goal 79 answers 80 under both.
#[test]
fn only_word_wrap_backs_a_goal_off_the_row_edge() {
    assert!(LineWrap::Truncate.goal_stops_at_row_edge());
    assert!(LineWrap::WindowWrap.goal_stops_at_row_edge());
    assert!(!LineWrap::WordWrap.goal_stops_at_row_edge());
}
