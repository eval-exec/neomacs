//! Divergence tests: char-width + string-width + composition + display combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_char_width_various_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 13 43)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (char-width ?A)
        (= (char-width ?A) 1)
        (char-width ? )
        (= (char-width ? ) 1)
        (char-width ?\t)
        (>= (char-width ?\t) 1)
        (char-width ?\n)
        (= (char-width ?\n) 1)
        (string-width "hello")
        (= (string-width "hello") 5)
        (string-width "abc\ndef")
        (= (string-width "abc\ndef") 6))) #"#,
        expect,
    );
}

#[test]
fn divergence_truncate_string_to_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (truncate-string-to-width "hello world" 5)
        (string= (truncate-string-to-width "hello world" 5) "hello")
        (truncate-string-to-width "hello world" 8 nil nil t)
        (string= (truncate-string-to-width "hello world" 8 nil nil t) "hello...")
        (truncate-string-to-width "hi" 5)
        (string= (truncate-string-to-width "hi" 5) "hi   ")
        (truncate-string-to-width "hello" 3 t)
        (string= (truncate-string-to-width "hello" 3 t) "hel"))) #"#,
        expect,
    );
}

#[test]
fn divergence_string_pad_alignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 9 48)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (format "%-10s" "left")
        (string= (format "%-10s" "left") "left      ")
        (format "%10s" "right")
        (string= (format "%10s" "right") "     right")
        (format "%-5d" 42)
        (string= (format "%-5d" 42) "42   ")
        (format "%05d" 42)
        (string= (format "%05d" 42) "00042"))) #"#,
        expect,
    );
}

#[test]
fn divergence_buffer_display_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable bs1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((w1 (string-width (buffer-string)))
        (bs (buffer-size)))
    (goto-char 6)
    (insert "Beautiful ")
    (let ((w2 (string-width (buffer-string)))
          (bs2 (buffer-size)))
      (list w1 bs1 w2 bs2
            (= w1 11)
            (= bs1 11)
            (= w2 21)
            (= bs2 21)
            (buffer-string)
            (string= (buffer-string) "Hello Beautiful World"))))) "#,
        expect,
    );
}

#[test]
fn divergence_string_make_multibyte_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 14 35)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ascii "hello")
        (multi "\xc3\xa9\xc3\xa0"))
    (list (multibyte-string-p ascii)
          (null (multibyte-string-p ascii))
          (multibyte-string-p multi)
          (string-bytes ascii)
          (= (string-bytes ascii) 5)
          (length ascii)
          (= (length ascii) 5)
          (string-bytes multi)
          (>= (string-bytes multi) 2)
          (length multi)
          (= (length multi) 2)))) #"#,
        expect,
    );
}

#[test]
fn divergence_string_composition_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function compose-region-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (compose-region-p 1 1)
        (null (compose-region-p 1 1))
        (stringp (buffer-string))
        (buffer-string)
        (= (buffer-size) 0))) #"#,
        expect,
    );
}

#[test]
fn divergence_char_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 10 46)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (charsetp 'ascii)
        (charsetp 'unicode)
        (not (charsetp 'nonexistent))
        (encode-char ?A 'unicode)
        (= (encode-char ?A 'unicode) 65)
        (decode-char 'unicode 65)
        (= (decode-char 'unicode 65) 65)
        (decode-char 'unicode 233)
        (= (decode-char 'unicode 233) 233))) #"#,
        expect,
    );
}

#[test]
fn divergence_fill_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "This is a long line of text that should be filled at the fill column boundary for testing purposes.")
  (let ((fill-column 30))
    (fill-region 1 (point-max)))
  (let ((lines (split-string (buffer-string) "\n" t)))
    (list (>= (length lines) 2)
          (every (lambda (l) (<= (length l) 35)) lines)
          (buffer-string)
          (> (buffer-size) 50)
          (= (length lines) (length lines))))) #"#,
        expect,
    );
}

#[test]
fn divergence_indent_rigidly_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 12 49)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "    line1\n    line2\n    line3\n")
  (indent-rigidly 1 30 -2)
  (let ((s1 (buffer-string)))
    (indent-rigidly 1 30 -10)
    (let ((s2 (buffer-string)))
      (list s1
            (string-match "  line1" s1)
            (= (string-match "  line1" s1) 0)
            s2
            (string-match "line1" s2)
            (= (string-match "line1" s2) 0))))) #"#,
        expect,
    );
}

#[test]
fn divergence_current_column_with_tabs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 15 41)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello\tworld")
  (goto-char 6)
  (let ((col1 (current-column)))
    (goto-char 1)
    (forward-char 5)
    (let ((col2 (current-column)))
      (end-of-line)
      (let ((col3 (current-column)))
        (list col1 col2 col3
              (= col1 8)
              (= col2 5)
              (> col3 col1)
              (buffer-string)
              (= (buffer-size) 11)))))) #"#,
        expect,
    );
}
