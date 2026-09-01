//! Complex combo batch 111 — `read` from buffer / stream / minibuffer
//! with malformed data, circular refs, char-read syntax (#), record syntax.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx111_read_from_buffer_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((alpha beta (gamma delta)) 27 trailing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(alpha beta (gamma delta)) trailing")
  (goto-char 1)
  (let ((obj (read (current-buffer))))
    (list obj (point) (read (current-buffer)))))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_special_syntaxes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([1 2 3] (:err . invalid-read-syntax) (:err . invalid-read-syntax) #s(record a b c) 65 1 134217825 65 233 16 15 10 1.5 1/2 1000.0 1000000000000000000000000 1e-06)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (car (read-from-string s))
            (error (cons :err (car e)))))
        '("[1 2 3]"
          "#(1 2 3)"
          "#(a b c)"
          "#s(record a b c)"
          "?A"
          "?\\C-a"
          "?\\M-a"
          "?\\x41"
          "?\\u00e9"
          "#x10"
          "#o17"
          "#b1010"
          "1.5"
          "1/2"
          "1.0e3"
          "1000000000000000000000000"
          "0.000001"))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_with_circular_ref_when_print_circle_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK (\"#1=(1 2 3 . #1#)\" 1 2 3)""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((obj (list 1 2 3)))
  (setcdr (cddr obj) obj)
  (let* ((printed (let ((print-circle t)) (prin1-to-string obj)))
         (read-back (let ((read-circle t))
                      (car (read-from-string printed)))))
    (list printed
          (car read-back)
          (cadr read-back)
          (caddr read-back))))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_with_shared_refs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"(#1=(1 2 3) #1# #1#)\" ((1 2 3) (1 2 3) (1 2 3)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((shared (list 1 2 3))
       (container (list shared shared shared)))
  (let ((printed (let ((print-circle t)) (prin1-to-string container))))
    (list printed
          (car (read-from-string printed)))))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_invalid_syntax_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (end-of-file end-of-file {open invalid-read-syntax end-of-file invalid-read-syntax end-of-file 1.2.3 invalid-read-syntax)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (car (read-from-string s))
            (error (car e))))
        '("(open paren"
          "[open bracket"
          "{open brace"
          "."
          "#[invalid"
          "#<<unknown"
          "\"unterminated string"
          "1.2.3"
          "#xZZZ"))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_with_comments_and_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK symbol""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (car (read-from-string "  ; comment\n  symbol  ; another\n  after"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_multiple_objects_from_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (alpha beta gamma delta)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "alpha beta gamma delta")
      (pos 0)
      (results nil))
  (while (< pos (length s))
    (let ((res (read-from-string s pos)))
      (push (car res) results)
      (setq pos (cdr res))))
  (nreverse results))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_multibyte_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((α symbol) (Ω-symbol symbol) (世界 symbol) (café symbol) (\"café\" string))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (let ((r (read-from-string s)))
                (list (car r) (type-of (car r))))
            (error (cons :err (car e)))))
        '("α"
          "Ω-symbol"
          "世界"
          "café"
          "\"café\""))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_string_escaped_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"simple\" \"with \\\"escaped\\\"\" \"with \\\\ backslash\" \"with \\n newline\" \"with \t tab\" \"\" \"é\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (car (read-from-string s))
            (error (cons :err (car e)))))
        '("\"simple\""
          "\"with \\\"escaped\\\"\""
          "\"with \\\\ backslash\""
          "\"with \n newline\""
          "\"with \\t tab\""
          "\"\""
          "\"\\u00e9\""))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_kv_cons_cells_and_dotted_pairs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((a . b) (a b . c) (a b c . d) ((a . 1) (b . 2)) (nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (car (read-from-string s))
            (error (cons :err (car e)))))
        '("(a . b)"
          "(a b . c)"
          "(a b c . d)"
          "((a . 1) (b . 2))"
          "(nil . nil)"))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_byte_code_function_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored invalid-read-syntax)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (car (read-from-string "#[1 \"abc\" nil nil nil]"))
          (car (read-from-string "#s(hash-table size 10 test eq data (a 1 b 2))")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx111_read_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((data "((name . \"alpha\") (value . 42) (tags . (a b c)))")
       (parsed (car (read-from-string data))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert data)
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 20)
      (goto-char 1)
      (let ((obj (read (current-buffer))))
        (let ((state (list parsed obj
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1))))))
"##,
        expect,
    );
}
