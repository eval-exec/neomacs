//! Complex combo batch 191 — `format` / `format-message` / `format-spec`
//! extreme edge cases: positional args, multibyte width, special floats.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx191_format_all_specifiers_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"42\" \"   42\" \"42   |\" \"00042\" \"+42\" \"100\" \"ff\" \"FF\" \"1010\" \"A\" \"β\" \"1.234568e+04\" \"12345.678900\" \"1e-05\" \"3.14\" \"     3.142\" \"hello\" \"        hi|\" \"hi        |\" \"(1 \\\"two\\\" 3)\" \"%\" \"a b c\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%d" 42)
      (format "%5d" 42)
      (format "%-5d|" 42)
      (format "%05d" 42)
      (format "%+d" 42)
      (format "%o" 64)
      (format "%x" 255)
      (format "%X" 255)
      (format "%b" 10)
      (format "%c" 65)
      (format "%c" 946)
      (format "%e" 12345.6789)
      (format "%f" 12345.6789)
      (format "%g" 0.00001)
      (format "%.2f" 3.14159)
      (format "%10.3f" 3.14159)
      (format "%s" "hello")
      (format "%10s|" "hi")
      (format "%-10s|" "hi")
      (format "%S" '(1 "two" 3))
      (format "%%")
      (format "%3$s %2$s %1$s" "c" "b" "a"))
"##,
        expect,
    );
}

#[test]
fn div_cx191_format_message_with_backtick_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"plain text\" \"with ‘quotes’ here\" \"value: 42\" \"val1 ‘a’ val2 ‘b’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format-message "plain text")
      (format-message "with `quotes' here")
      (format-message "value: %d" 42)
      (format-message "val1 `%s' val2 `%s'" "a" "b"))
"##,
        expect,
    );
}

#[test]
fn div_cx191_format_with_multibyte_padding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"               hello|\" \"hello               |\" \"                café|\" \"café                |\" \"                世界|\" \"世界                |\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%20s|" "hello")
      (format "%-20s|" "hello")
      (format "%20s|" "café")
      (format "%-20s|" "café")
      (format "%20s|" "世界")
      (format "%-20s|" "世界"))
"##,
        expect,
    );
}

#[test]
fn div_cx191_format_with_special_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"inf\" \"inf\" \"-nan\" \"-inf\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (format "%f" (/ 1.0 0.0)) (error (cons :err (car e))))
      (condition-case e (format "%d" (/ 1.0 0.0)) (error (cons :err (car e))))
      (condition-case e (format "%e" (/ 0.0 0.0)) (error (cons :err (car e))))
      (condition-case e (format "%g" (/ -1.0 0.0)) (error (cons :err (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx191_format_spec_make_and_use() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function format-spec-make)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((spec (format-spec-make ?a "alpha" ?b "beta" ?c "gamma" ?d "delta")))
  (list (format-spec "%a-%b-%c-%d" spec)
        (format-spec "%d-%c-%b-%a" spec)
        (length (format-spec "%a-%b-%c-%d" spec))
        (condition-case e (format-spec "%z-missing" spec) (error (car e)))
        (format-spec "%%literal" spec)))
"##,
        expect,
    );
}

#[test]
fn div_cx191_format_with_bignum_and_ratio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 355/113)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 128))
      (ratio 355/113))
  (list (format "%d" big)
        (format "%x" big)
        (format "%o" big)
        (format "%b" big)
        (format "%S" ratio)
        (format "%d" ratio)
        (format "%.10f" ratio)))
"##,
        expect,
    );
}

#[test]
fn div_cx191_number_to_string_with_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (number-to-string 42)
      (number-to-string -42)
      (number-to-string 3.14)
      (number-to-string 1/3)
      (number-to-string (expt 2 64))
      (number-to-string -1/7))
"##,
        expect,
    );
}

#[test]
fn div_cx191_string_to_number_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 3.14 1 0 0 0 0 42 0 -314.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-to-number "42")
      (string-to-number "3.14")
      (string-to-number "1/3")
      (string-to-number "0x1A")
      (string-to-number "0o17")
      (string-to-number "0b1010")
      (string-to-number "not-a-number")
      (string-to-number "42abc")
      (string-to-number "")
      (string-to-number "-3.14e2"))
"##,
        expect,
    );
}

#[test]
fn div_cx191_format_positional_args_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Not enough arguments for format string\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%2$s %1$s" "world" "hello")
      (format "%1$d + %2$d = %3$d" 2 3 5)
      (format "%s = %2$d (or %d)" "x" 99)
      (format "%3$-10s|" "a" "b" "c"))
"##,
        expect,
    );
}

#[test]
fn div_cx191_format_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function format-spec-make)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((spec (format-spec-make ?a "alpha" ?b "beta"))
      (big (expt 2 64)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Format mega: %s %d %s" (format-spec "%a-%b" spec) big "end"))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list (format-spec "%a-%b" spec)
                         (format "%d" big)
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
