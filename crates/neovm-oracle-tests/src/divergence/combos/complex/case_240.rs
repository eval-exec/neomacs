//! Complex combo batch 240 — `read` reader macros deep: `#.`
//! (eval), `#_` (skip), `#@` (count-chars), `#s` (record),
//! `#N=` / `#N#` (circular/shared), `#[...]` (bytecode).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx240_read_eval_reader_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:err . invalid-read-syntax) (:err . invalid-read-syntax))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (car (read-from-string "#.(+ 1 2)")) (error (cons :err (car e))))
      (condition-case e (car (read-from-string "#.(* 6 7)")) (error (cons :err (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx240_read_skip_reader_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK skipped""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (car (read-from-string "#_skipped actual-value"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx240_read_record_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#s(hash-table test eq data (a 1 b 2)) t 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((result (condition-case e
                  (car (read-from-string "#s(hash-table size 10 test eq data (a 1 b 2))"))
                (error (cons :err (car e))))))
  (list result
        (hash-table-p result)
        (when (hash-table-p result) (hash-table-count result))))
"##,
        expect,
    );
}

#[test]
fn div_cx240_read_shared_reference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(#1=(1 2 3) #1# #1#)\" (1 2 3) t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((shared (list 1 2 3))
       (data (list shared shared shared))
       (printed (let ((print-circle t)) (prin1-to-string data)))
       (read-back (car (read-from-string printed))))
  (list printed
        (car read-back)
        (eq (car read-back) (cadr read-back))
        (eq (cadr read-back) (caddr read-back))))
"##,
        expect,
    );
}

#[test]
fn div_cx240_read_circular_reference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK (\"#1=(1 2 3 . #1#)\" 1 2 3 t)""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((circular (list 1 2 3))
       (data (progn (setcdr (cddr circular) circular) circular))
       (printed (let ((print-circle t)) (prin1-to-string data)))
       (read-back (car (read-from-string printed))))
  (list printed
        (car read-back)
        (cadr read-back)
        (caddr read-back)
        (eq read-back (cdddr read-back))))
"##,
        expect,
    );
}

#[test]
fn div_cx240_read_bytecode_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 25 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((lex (lambda (x) (* x x)))
           (bc (byte-compile lex))
           (printed (prin1-to-string bc))
           (read-back (car (read-from-string printed))))
      (list (byte-code-function-p bc)
            (byte-code-function-p read-back)
            (funcall bc 5)
            (funcall read-back 5)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx240_read_radix_integers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((\"#x10\" 16 integer) (\"#o17\" 15 integer) (\"#b1010\" 10 integer) (\"#xdeadbeef\" 3735928559 integer) (\"#o777\" 511 integer) (\"#b11111111\" 255 integer) (\"1000000000000000000000\" 1000000000000000000000 integer))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (let ((v (car (read-from-string s))))
            (list s v (type-of v))))
        '("#x10" "#o17" "#b1010" "#xdeadbeef"
          "#o777" "#b11111111" "1000000000000000000000"))
"##,
        expect,
    );
}

#[test]
fn div_cx240_read_char_syntax_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"?A\" 65 t) (\"?\\\\C-a\" 1 t) (\"?\\\\M-a\" 134217825 t) (\"?\\\\C-M-a\" :err invalid-read-syntax) (\"?\\\\x41\" 65 t) (\"?\\\\u00e9\" 233 t) (\"?\\\\N{LATIN SMALL LETTER E WITH ACUTE}\" 233 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (let ((v (car (read-from-string s))))
                (list s v (integerp v)))
            (error (list s :err (car e)))))
        '("?A" "?\\C-a" "?\\M-a" "?\\C-M-a"
          "?\\x41" "?\\u00e9" "?\\N{LATIN SMALL LETTER E WITH ACUTE}"))
"##,
        expect,
    );
}

#[test]
fn div_cx240_read_string_with_all_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"plain\" \"with \\\"escaped\\\"\" \"with \\\\ backslash\" \"with \t tab and \\n newline\" \"with é unicode\" \"with A hex\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (car (read-from-string s))
            (error (cons :err (car e)))))
        '("\"plain\""
          "\"with \\\"escaped\\\"\""
          "\"with \\\\ backslash\""
          "\"with \\t tab and \\n newline\""
          "\"with \\u00e9 unicode\""
          "\"with \\x41 hex\""))
"##,
        expect,
    );
}

#[test]
fn div_cx240_read_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((shared (list 1 2 3))
       (data (list shared shared (list :a :b)))
       (printed (let ((print-circle t)) (prin1-to-string data))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert printed)
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 20)
      (goto-char 1)
      (let ((read-back (read (current-buffer))))
        (let ((state (list printed read-back
                           (eq (car read-back) (cadr read-back))
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
