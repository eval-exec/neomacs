use super::*;
use crate::emacs_core::autoload::is_autoload_value;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::format_eval_result;
use crate::test_utils::{
    eval_with_ldefs_boot_autoloads, runtime_startup_context, runtime_startup_eval_all,
};
use std::fs;
use std::path::PathBuf;

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

#[test]
fn rectangle_state_default() {
    crate::test_utils::init_test_tracing();
    let state = RectangleState::new();
    assert!(state.killed.is_empty());
}

#[test]
fn rectangle_state_default_trait() {
    crate::test_utils::init_test_tracing();
    let state = RectangleState::default();
    assert!(state.killed.is_empty());
}

#[test]
fn extract_rectangle_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["extract-rectangle"]);
    let function = eval
        .obarray
        .symbol_function("extract-rectangle")
        .expect("missing extract-rectangle startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn extract_rectangle_loads_from_gnu_rect_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef\n123456\n")
             (list (extract-rectangle 1 9)
                   (subrp (symbol-function 'extract-rectangle))))"#,
    );
    assert_eq!(result[0], r#"OK (("a" "1") nil)"#);
}

#[test]
fn extract_rectangle_line_returns_string() {
    crate::test_utils::init_test_tracing();
    let result = builtin_extract_rectangle_line(vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();
    assert_eq!(result.as_utf8_str(), Some(""));
}

#[test]
fn extract_rectangle_line_with_line_argument() {
    crate::test_utils::init_test_tracing();
    let result = builtin_extract_rectangle_line(vec![
        Value::fixnum(1),
        Value::fixnum(3),
        Value::string("abcdef"),
    ])
    .unwrap();
    assert_eq!(result.as_utf8_str(), Some("bc"));
}

#[test]
fn extract_rectangle_line_swapped_columns() {
    crate::test_utils::init_test_tracing();
    let result = builtin_extract_rectangle_line(vec![
        Value::fixnum(3),
        Value::fixnum(1),
        Value::string("abcdef"),
    ])
    .unwrap();
    assert_eq!(result.as_utf8_str(), Some("bc"));
}

#[test]
fn extract_rectangle_line_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let result = builtin_extract_rectangle_line(vec![
        Value::fixnum(0),
        Value::fixnum(2),
        Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
            0xFF, b'A', b'B',
        ])),
    ])
    .unwrap();
    let string = result
        .as_lisp_string()
        .expect("extract-rectangle-line should return a LispString");
    assert!(!string.is_multibyte());
    assert_eq!(string.as_bytes(), &[0xFF, b'A']);
}

#[test]
fn extract_rectangle_line_negative_column_errors() {
    crate::test_utils::init_test_tracing();
    assert!(
        builtin_extract_rectangle_line(vec![
            Value::fixnum(-1),
            Value::fixnum(1),
            Value::string("abc"),
        ])
        .is_err()
    );
}

#[test]
fn extract_rectangle_line_validates_args() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_extract_rectangle_line(vec![]).is_err());
    assert!(builtin_extract_rectangle_line(vec![Value::fixnum(1)]).is_err());
    assert!(
        builtin_extract_rectangle_line(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)])
            .is_err()
    );
}

#[test]
fn delete_extract_rectangle_from_text_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let source = crate::heap_types::LispString::from_unibyte(vec![0xFF, b'A', b'\n', 0xFE, b'B']);
    let (extracted, rewritten) = delete_extract_rectangle_from_text(&source, 0, 1, 0, 1);

    assert_eq!(extracted.len(), 2);
    assert_eq!(extracted[0].as_bytes(), &[0xFF]);
    assert_eq!(extracted[1].as_bytes(), &[0xFE]);
    assert!(!extracted[0].is_multibyte());
    assert!(!rewritten.is_multibyte());
    assert_eq!(rewritten.as_bytes(), &[b'A', b'\n', b'B']);
}

#[test]
fn delete_rectangle_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["delete-rectangle"]);
    let function = eval
        .obarray
        .symbol_function("delete-rectangle")
        .expect("missing delete-rectangle startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn delete_rectangle_loads_from_gnu_rect_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef\n123456\n")
             (delete-rectangle 1 9)
             (list (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                   (point)
                   (subrp (symbol-function 'delete-rectangle))))"#,
    );
    assert_eq!(result[0], r#"OK ("bcdef|23456|" 13 nil)"#);
}

#[test]
fn kill_rectangle_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["kill-rectangle"]);
    let function = eval
        .obarray
        .symbol_function("kill-rectangle")
        .expect("missing kill-rectangle startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn kill_rectangle_loads_from_gnu_rect_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef\n123456\n")
             (list (kill-rectangle 1 9)
                   (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                   (point)
                   killed-rectangle
                   (subrp (symbol-function 'kill-rectangle))))"#,
    );
    assert_eq!(
        result[0],
        r#"OK (("a" "1") "bcdef|23456|" 13 ("a" "1") nil)"#
    );
}

#[test]
fn yank_rectangle_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["yank-rectangle"]);
    let function = eval
        .obarray
        .symbol_function("yank-rectangle")
        .expect("missing yank-rectangle startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn yank_rectangle_loads_from_gnu_rect_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(progn
             (setq killed-rectangle '("X" "Y"))
             (with-temp-buffer
               (insert "abc\ndef\n")
               (goto-char (point-min))
               (list (yank-rectangle)
                     (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                     (point)
                     (subrp (symbol-function 'yank-rectangle)))))"#,
    );
    assert_eq!(result[0], r#"OK (nil "Xabc|Ydef|" 7 nil)"#);
}

#[test]
fn yank_rectangle_loaded_rejects_non_list_killed_rectangle() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(progn
             (setq killed-rectangle 1)
             (condition-case err
                 (yank-rectangle)
               (error (list 'err (car err)))))"#,
    );
    assert_eq!(result[0], r#"OK (err wrong-type-argument)"#);
}

#[test]
fn yank_rectangle_loaded_function_is_simple_bytecode_call() {
    crate::test_utils::init_test_tracing();
    let mut eval = runtime_startup_context();
    let results = eval.eval_str_each(r#"(load "rect")"#);
    assert!(
        results.iter().all(Result::is_ok),
        "rect load failed: {:?}",
        results.iter().map(format_eval_result).collect::<Vec<_>>()
    );

    let function = eval
        .obarray
        .symbol_function("yank-rectangle")
        .expect("loaded yank-rectangle function cell");
    // When loading .el source (not .elc), the function is interpreted,
    // not byte-compiled.  Just verify it's callable.  When loaded from
    // .elc, verify bytecode structure.
    if let Some(bytecode) = function.get_bytecode_data() {
        assert_eq!(
            bytecode
                .constants
                .iter()
                .map(|v| v.as_symbol_name())
                .collect::<Vec<_>>(),
            vec![Some("killed-rectangle"), Some("insert-rectangle")]
        );
        assert_eq!(
            bytecode.executable_ops(),
            vec![Op::Constant(1), Op::VarRef(0), Op::Call(1), Op::Return]
        );
    }
    // Either way, the function should be callable
    assert!(!function.is_nil());
}

#[test]
fn insert_rectangle_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["insert-rectangle"]);
    let function = eval
        .obarray
        .symbol_function("insert-rectangle")
        .expect("missing insert-rectangle startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn insert_rectangle_loads_from_gnu_rect_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abc\ndef\n")
             (goto-char (point-min))
             (list (insert-rectangle '("hello" "world"))
                   (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                   (point)
                   (subrp (symbol-function 'insert-rectangle))))"#,
    );
    assert_eq!(result[0], r#"OK (nil "helloabc|worlddef|" 15 nil)"#);
}

#[test]
fn insert_rectangle_loaded_rejects_non_list_argument() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(condition-case err
               (insert-rectangle 42)
             (error (list 'err (car err))))"#,
    );
    assert_eq!(result[0], r#"OK (err wrong-type-argument)"#);
}

#[test]
fn insert_rectangle_loaded_rejects_non_string_elements() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(condition-case err
               (insert-rectangle '("a" 42))
             (error (list 'err (car err))))"#,
    );
    assert_eq!(result[0], r#"OK (err wrong-type-argument)"#);
}

#[test]
fn open_rectangle_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["open-rectangle"]);
    let function = eval
        .obarray
        .symbol_function("open-rectangle")
        .expect("missing open-rectangle startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn open_rectangle_loads_from_gnu_rect_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef\n123456\n")
             (list (open-rectangle 1 9)
                   (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                   (point)
                   (subrp (symbol-function 'open-rectangle))))"#,
    );
    assert_eq!(result[0], r#"OK (1 " abcdef| 123456|" 1 nil)"#);
}

#[test]
fn clear_rectangle_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["clear-rectangle"]);
    let function = eval
        .obarray
        .symbol_function("clear-rectangle")
        .expect("missing clear-rectangle startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn clear_rectangle_loads_from_gnu_rect_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef\n123456\n")
             (clear-rectangle 1 9)
             (list (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                   (point)
                   (subrp (symbol-function 'clear-rectangle))))"#,
    );
    assert_eq!(result[0], r#"OK (" bcdef| 23456|" 15 nil)"#);
}

#[test]
fn string_rectangle_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["string-rectangle"]);
    let function = eval
        .obarray
        .symbol_function("string-rectangle")
        .expect("missing string-rectangle startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn string_rectangle_loads_from_gnu_rect_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef\n123456\n")
             (list (string-rectangle 2 10 "XX")
                   (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                   (point)
                   (subrp (symbol-function 'string-rectangle))))"#,
    );
    assert_eq!(result[0], r#"OK (12 "aXXcdef|1XX3456|" 12 nil)"#);
}

#[test]
fn string_rectangle_loaded_rejects_non_char_or_string() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(condition-case err
               (string-rectangle 1 10 1.5)
             (error (list 'err (car err))))"#,
    );
    assert_eq!(result[0], r#"OK (err wrong-type-argument)"#);
}

#[test]
fn delete_extract_rectangle_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["delete-extract-rectangle"]);
    let function = eval
        .obarray
        .symbol_function("delete-extract-rectangle")
        .expect("missing delete-extract-rectangle startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn delete_extract_rectangle_loads_from_gnu_rect_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef\n123456\n")
             (list (delete-extract-rectangle 1 9)
                   (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                   (point)
                   (subrp (symbol-function 'delete-extract-rectangle))))"#,
    );
    assert_eq!(result[0], r#"OK (("a" "1") "bcdef|23456|" 13 nil)"#);
}

#[test]
fn delete_extract_rectangle_after_explicit_rect_load_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(progn
             (load "rect")
             (defvar neovm--orig-derl nil)
             (defvar neovm--trace nil)
             (setq neovm--orig-derl (symbol-function 'delete-extract-rectangle-line))
             (fset 'delete-extract-rectangle-line
                   (lambda (startcol endcol lines fill)
                     (setq neovm--trace
                           (cons (list :before
                                       (point)
                                       startcol
                                       endcol
                                       (replace-regexp-in-string "\n" "|" (buffer-string) nil t))
                                 neovm--trace))
                     (prog1
                         (funcall neovm--orig-derl startcol endcol lines fill)
                       (setq neovm--trace
                             (cons (list :after
                                         (point)
                                         startcol
                                         endcol
                                         (car (cdr lines))
                                         (replace-regexp-in-string "\n" "|" (buffer-string) nil t))
                                   neovm--trace)))))
             (with-temp-buffer
               (insert "abcdef\n123456\n")
               (list (delete-extract-rectangle 1 9)
                     (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                     (point)
                     (nreverse neovm--trace))))"#,
    );
    assert_eq!(
        result[0],
        r#"OK (("a" "1") "bcdef|23456|" 13 ((:before 1 0 1 "abcdef|123456|") (:after 1 0 1 "a" "bcdef|123456|") (:before 7 0 1 "bcdef|123456|") (:after 7 0 1 "1" "bcdef|23456|")))"#
    );
}

#[test]
fn delete_extract_rectangle_line_loaded_state_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(progn
             (load "rect")
             (with-temp-buffer
               (insert "abcdef\n123456\n")
               (let* ((startcol 0)
                      (endcol 1)
                      (startpt (progn (goto-char 1) (line-beginning-position)))
                      (endpt (progn (goto-char 9) (copy-marker (line-end-position))))
                      (states nil)
                      (lines (list nil)))
                 (goto-char startpt)
                 (while (progn
                          (setq states (cons (list :before (point) (marker-position endpt)) states))
                          (delete-extract-rectangle-line startcol endcol lines nil)
                          (setq states
                                (cons (list :after
                                            (point)
                                            (marker-position endpt)
                                            (car (cdr lines))
                                            (replace-regexp-in-string "\n" "|" (buffer-string) nil t))
                                      states))
                          (and (= 0 (forward-line 1)) (bolp) (<= (point) endpt))))
                 (list (nreverse states) (nreverse (cdr lines))))))"#,
    );
    assert_eq!(
        result[0],
        r#"OK (((:before 1 14) (:after 1 13 "a" "bcdef|123456|") (:before 7 13) (:after 7 12 "1" "bcdef|23456|")) ("a" "1"))"#
    );
}

#[test]
fn raw_context_does_not_prebind_replace_rectangle_alias() {
    crate::test_utils::init_test_tracing();
    let eval = super::super::eval::Context::new();
    assert!(
        eval.obarray.symbol_function("replace-rectangle").is_none(),
        "replace-rectangle should come from GNU rect.el/loaddefs, not Context::new"
    );
}

#[test]
fn gnu_ldefs_boot_defines_replace_rectangle_alias() {
    crate::test_utils::init_test_tracing();
    let source =
        fs::read_to_string(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("lisp/ldefs-boot.el"))
            .expect("read ldefs-boot");
    assert!(
        source.contains(
            "(define-obsolete-function-alias 'replace-rectangle #'string-rectangle \"29.1\")",
        ),
        "GNU ldefs-boot.el should own the replace-rectangle alias",
    );
}

#[test]
fn replace_rectangle_uses_runtime_alias_behavior() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef\n123456\n")
             (replace-rectangle 1 9 "ZZ")
             (list (replace-regexp-in-string "\n" "|" (buffer-string) nil t)
                   (point)
                   (symbol-function 'replace-rectangle)))"#,
    );
    assert_eq!(result[0], r#"OK ("ZZbcdef|ZZ23456|" 11 string-rectangle)"#);
}
