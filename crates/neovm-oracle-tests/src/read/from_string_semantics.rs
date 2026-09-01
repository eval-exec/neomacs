//! Oracle parity tests for GNU `read-from-string` edge semantics.
//!
//! GNU implements this in `src/lread.c` by validating START/END as substring
//! bounds, reading one object, and returning `(OBJECT . FINAL-STRING-INDEX)`.
//! These tests pin exact error payloads because callers can observe them with
//! `condition-case`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_read_from_string_start_end_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (read-from-string "  αβ 42 tail" 2)
 (read-from-string "xx (a b) yy" 3 8)
 (read-from-string "one two three" -9)
 (read-from-string "one two three" -9 -6)
 (condition-case err
     (read-from-string "abc" 4)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((αβ . 4) ((a b) . 8) (two . 7) (two . 7) (args-out-of-range (\"abc\" 4 nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_read_from_string_malformed_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r###"
(let ((cases
       '("(1 2 3"
         "\"unterminated"
         "[1 2"
         "#<buffer foo>"
         "#1=#1#"
         "#1#"
         "#@5abc")))
  (mapcar
   (lambda (input)
     (condition-case err
         (read-from-string input)
       (error (list input (car err) (cdr err)))))
   cases))
"###;

    let expect = expect_test::expect![[
        r##""OK ((\"(1 2 3\" end-of-file nil) (\"\\\"unterminated\" end-of-file nil) (\"[1 2\" end-of-file nil) (\"#<buffer foo>\" invalid-read-syntax (\"#<\")) (\"#1=#1#\" invalid-read-syntax (\"nonsensical self-reference\")) (\"#1#\" invalid-read-syntax (\"#1#\")) (\"#@5abc\" end-of-file nil))""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_read_from_string_hash_skip_lazy_string_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r###"
(let ((inputs '("#@"
                "#@x"
                "#@0x"
                "#@01abc"
                "#@4data42"
                "#@5abc"))
      (zero-zero "#@00abc"))
  (list
   (mapcar
    (lambda (input)
      (condition-case err
          (read-from-string input)
        (error (list input (car err) (cdr err)))))
    inputs)
   (read-from-string zero-zero)))
"###;

    let expect = expect_test::expect![[
        r##""OK (((\"#@\" end-of-file nil) (\"#@x\" end-of-file nil) (\"#@0x\" end-of-file nil) (\"#@01abc\" end-of-file nil) (\"#@4data42\" end-of-file nil) (\"#@5abc\" end-of-file nil)) (nil . 7))""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_read_from_string_read_circle_and_radix_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r###"
(let ((read-circle nil)
      (inputs '("#1=(a)"
                "#1#"
                "#2r101"
                "#36rZ"
                "#37r10")))
  (mapcar
   (lambda (input)
     (condition-case err
         (read-from-string input)
       (error (list input (car err) (cdr err)))))
   inputs))
"###;

    let expect = expect_test::expect![[
        r##""OK ((\"#1=(a)\" invalid-read-syntax (\"#1=\")) (\"#1#\" invalid-read-syntax (\"#1#\")) (5 . 6) (35 . 5) (\"#37r10\" invalid-read-syntax (\"integer, radix 37\")))""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_read_from_string_empty_and_whitespace_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((cases '("" "   " "\n\t ")))
  (mapcar
   (lambda (input)
     (condition-case err
         (read-from-string input)
       (error (list input (car err) (cdr err)))))
   cases))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"\" end-of-file nil) (\"   \" end-of-file nil) (\"\\n\t \" end-of-file nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_read_from_string_reader_macro_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r###"
(list
 (read-from-string "'foo rest")
 (read-from-string "#'car rest")
 (read-from-string "`(a ,b ,@c) tail")
 (read-from-string "#:uninterned tail")
 (let ((r (read-from-string "#:same #:same")))
   (list (car r) (cdr r) (eq (car r) (car (read-from-string "#:same"))))))
"###;

    let expect = expect_test::expect![[
        r#""OK (('foo . 4) (#'car . 5) (`(a ,b ,@c) . 11) (uninterned . 12) (same 6 nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_read_from_string_preserves_text_properties_on_read_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((src (copy-sequence "\"abc\" tail")))
  (put-text-property 1 4 'face 'bold src)
  (let ((r (read-from-string src)))
    (list r
          (text-properties-at 0 (car r))
          (text-properties-at 1 (car r))
          (substring-no-properties (car r)))))
"#;

    let expect = expect_test::expect![[r#""OK ((\"abc\" . 5) nil nil \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_unibyte_reader_sources_do_not_decode_valid_utf8_byte_runs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lread.c source_string_get/source_buffer_get turn each high byte into
    // BYTE8_TO_CHAR before parsing.  A byte sequence that happens to be valid
    // UTF-8 therefore remains raw bytes; only file sources decode encodings.
    let form = r#"
(let* ((source (unibyte-string ?\" #xce #xbb ?\"))
       (from-string (car (read-from-string source)))
       (from-buffer
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert source)
          (goto-char (point-min))
          (read (current-buffer)))))
  (list
   (multibyte-string-p source)
   (string-to-list source)
   (multibyte-string-p from-string)
   (string-to-list from-string)
   (multibyte-string-p from-buffer)
   (string-to-list from-buffer)))
"#;

    let expect =
        expect_test::expect![[r#""OK (nil (34 206 187 34) nil (206 187) nil (206 187))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
