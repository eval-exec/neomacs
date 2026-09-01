//! Complex combo batch 121 — `read` / `parse` / `pp` of large nested
//! structures, hash-tables inside hash-tables, plists inside alists,
//! `read-circle` interaction with `prin1` escaping.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx121_print_read_roundtrip_nested_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#s(hash-table test equal data (\\\"inner\\\" #s(hash-table test equal data (\\\"a\\\" 1 \\\"b\\\" 2)) \\\"scalar\\\" :val))\" t 2)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((inner (make-hash-table :test 'equal))
       (outer (make-hash-table :test 'equal)))
  (puthash "a" 1 inner)
  (puthash "b" 2 inner)
  (puthash "inner" inner outer)
  (puthash "scalar" :val outer)
  (let* ((printed (prin1-to-string outer))
         (read-back (car (read-from-string printed))))
    (list printed
          (hash-table-p read-back)
          (hash-table-count read-back))))
"##,
        expect,
    );
}

#[test]
fn div_cx121_print_read_roundtrip_plist_inside_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"((name . \\\"alpha\\\") (props :a 1 :b 2 :c 3) (tags \\\"x\\\" \\\"y\\\" \\\"z\\\"))\" ((name . \"alpha\") (props :a 1 :b 2 :c 3) (tags \"x\" \"y\" \"z\")) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '((name . "alpha")
              (props :a 1 :b 2 :c 3)
              (tags "x" "y" "z"))))
  (let* ((printed (prin1-to-string data))
         (read-back (car (read-from-string printed))))
    (list printed read-back (equal data read-back))))
"##,
        expect,
    );
}

#[test]
fn div_cx121_print_with_print_length_truncates_long_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20)\" \"(1 2 3 ...)\" \"(...)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((long-list (number-sequence 1 20)))
  (list (prin1-to-string long-list)
        (let ((print-length 3)) (prin1-to-string long-list))
        (let ((print-length 0)) (prin1-to-string long-list))))
"##,
        expect,
    );
}

#[test]
fn div_cx121_print_with_print_level_truncates_deep_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"((((\\\"level4\\\"))))\" \"(...)\" \"((...))\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((deep '(((("level4"))))))
  (list (prin1-to-string deep)
        (let ((print-level 1)) (prin1-to-string deep))
        (let ((print-level 2)) (prin1-to-string deep))))
"##,
        expect,
    );
}

#[test]
fn div_cx121_pp_to_string_with_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"((a . 1) (b . 2) (c sub (deep nested)))\\n\" \"((a . 1) (b . 2) (c sub (deep nested)))\" 2 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '((a . 1) (b . 2) (c . (sub (deep nested))))))
  (let ((pp-str (pp-to-string data))
        (p1-str (prin1-to-string data)))
    (list pp-str p1-str
          (length (split-string pp-str "\n"))
          (> (length pp-str) (length p1-str)))))
"##,
        expect,
    );
}

#[test]
fn div_cx121_print_escape_nonascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\"café 世界\\\"\" \"\\\"café 世界\\\"\" \"\\\"line\\\\nbreak\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café 世界"))
  (list (prin1-to-string s)
        (let ((print-escape-nonascii t)) (prin1-to-string s))
        (let ((print-escape-newlines t)) (prin1-to-string "line\nbreak"))))
"##,
        expect,
    );
}

#[test]
fn div_cx121_print_quoted_false_for_special_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function princ-to-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (string ?\x00 ?a ?\x01 ?b ?\n)))
  (list (prin1-to-string s)
        (let ((print-quoted t)) (prin1-to-string s))
        (princ-to-string s)
        (let ((print-escape-control-characters t)) (prin1-to-string s))))
"##,
        expect,
    );
}

#[test]
fn div_cx121_read_with_integers_floats_and_bignums() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((\"42\" 42 integer) (\"-42\" -42 integer) (\"1.5\" 1.5 float) (\"1/3\" 1/3 symbol) (\"0.001\" 0.001 float) (\"1000000000000000000000\" 1000000000000000000000 integer) (\"#x10\" 16 integer) (\"#o17\" 15 integer) (\"#b1010\" 10 integer) (\"1e10\" 10000000000.0 float))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (let ((v (car (read-from-string s))))
                (list s v (type-of v)))
            (error (list s :err (car e)))))
        '("42"
          "-42"
          "1.5"
          "1/3"
          "0.001"
          "1000000000000000000000"
          "#x10"
          "#o17"
          "#b1010"
          "1e10"))
"##,
        expect,
    );
}

#[test]
fn div_cx121_print_charset_qualified_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 2 37)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((chars '(?a ?A ?0 ?\n ?\t ?  ?café)))
  (mapcar (lambda (c) (prin1-to-string c)) chars))
"##,
        expect,
    );
}

#[test]
fn div_cx121_print_objects_via_format_spec_S() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%S" 'sym)
      (format "%S" "str")
      (format "%S" '(1 2 3))
      (format "%S" [1 2 3])
      (format "%S" ?A)
      (format "%S" 1/2)
      (format "%S" '("quoted" "list" "with" "spaces")))
"##,
        expect,
    );
}

#[test]
fn div_cx121_princ_vs_prin1_string_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function princ-to-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (propertize "hello" 'face 'bold)))
  (list (prin1-to-string s)
        (princ-to-string s)
        (length (prin1-to-string s))
        (length (princ-to-string s))))
"##,
        expect,
    );
}

#[test]
fn div_cx121_print_read_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((data '((name . "alpha")
                (value . 42)
                (tags . ("x" "y" "z"))))
       (printed (prin1-to-string data)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert printed)
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((read-back (read (current-buffer))))
        (let ((state (list printed read-back
                           (equal data read-back)
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
