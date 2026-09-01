//! Complex combo batch 421 — 20 probes into bignum/float arithmetic,
//! read-from-string edges, eval context, save-excursion deep,
//! narrowing nested, with-output-to-string, format %d on bignums,
//! ceiling/floor/truncate/round on floats, logand/lsh on bignums,
//! 1+/1- on most-positive-fixnum, mod/rem edge, and window-buffer deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// bignum arithmetic: large integer operations.
#[test]
fn div_cx421_bignum_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1208925819614629174706177 2417851639229258349412352 402975273204876391568725 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 80)))
  (list (+ big 1)
        (* big 2)
        (/ big 3)
        (% big 7)))
"##,
        expect,
    );
}

/// read-from-string with start/end boundaries.
#[test]
fn div_cx421_read_from_string_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "(a b c) (d e f) (g h i)"))
  (list (read-from-string s)
        (read-from-string s 6)
        (read-from-string s 6 11)))
"##,
        expect,
    );
}

/// eval in different buffer contexts.
#[test]
fn div_cx421_eval_buffer_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"local\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "local")
  (eval '(buffer-string)))
"##,
        expect,
    );
}

/// save-excursion with marker restoration across buffer switches.
#[test]
fn div_cx421_save_excursion_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#<buffer *scratch*> 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (get-buffer-create " *cx421-a*"))
      (b (get-buffer-create " *cx421-b*"))
      (m (make-marker)))
  (with-current-buffer a (insert "aaaa") (set-marker m 3))
  (with-current-buffer b (insert "bbbb"))
  (save-excursion
    (set-buffer a)
    (goto-char 2)
    (set-buffer b)
    (goto-char 3))
  (list (current-buffer) (point)))
"##,
        expect,
    );
}

/// nested narrowing / save-restriction.
#[test]
fn div_cx421_narrowing_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 8 \"cdefg\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (save-restriction
    (narrow-to-region 3 8)
    (save-restriction
      (narrow-to-region 4 6)
      (list (point-min) (point-max) (buffer-string)))
    (list (point-min) (point-max) (buffer-string))))
"##,
        expect,
    );
}

/// with-output-to-string / with-output-to-temp-buffer.
#[test]
fn div_cx421_with_output_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello world\" \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-output-to-string (princ "hello") (princ " world"))
      (condition-case e
          (with-output-to-temp-buffer "*cx421-out*"
            (princ "test"))
        (error (car e))))
"##,
        expect,
    );
}

/// format %d on bignums and negatives.
#[test]
fn div_cx421_format_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"717897987691852588770249\" \"980553f0db2fd09de3c9\" \"-1\" \"-1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 3 50)))
  (list (format "%d" big)
        (format "%x" big)
        (format "%d" -1)
        (format "%x" -1)))
"##,
        expect,
    );
}

/// ceiling / floor / truncate / round on float edge cases.
#[test]
fn div_cx421_float_rounding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 3 3 4 -3 -4 -3 -4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (ceiling 3.5) (floor 3.5) (truncate 3.5) (round 3.5)
      (ceiling -3.5) (floor -3.5) (truncate -3.5) (round -3.5))
"##,
        expect,
    );
}

/// logand / logior / logxor on bignums.
#[test]
fn div_cx421_logical_ops_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 3458764513820540928 3458764513820540928 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (expt 2 60)) (b (expt 2 61)))
  (list (logand a b)
        (logior a b)
        (logxor a b)
        (lognot a)))
"##,
        expect,
    );
}

/// ash / lsh on negative shifts.
#[test]
fn div_cx421_ash_lsh_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1024 32 -1024 1024 32)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (ash 1 10)
      (ash 1024 -5)
      (ash -1 10)
      (lsh 1 10)
      (lsh 1024 -5))
"##,
        expect,
    );
}

/// 1+ / 1- on most-positive-fixnum (should wrap to bignum).
#[test]
fn div_cx421_one_plus_minus_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (2305843009213693952 -2305843009213693953 integer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (1+ most-positive-fixnum)
      (1- most-negative-fixnum)
      (type-of (1+ most-positive-fixnum)))
"##,
        expect,
    );
}

/// mod / rem edge cases with negative and float.
#[test]
fn div_cx421_mod_rem_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function rem)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (mod 7 3) (rem 7 3)
      (mod -7 3) (rem -7 3)
      (mod 7 -3) (rem 7 -3))
"##,
        expect,
    );
}

/// window-buffer / set-window-buffer deep.
#[test]
fn div_cx421_window_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *cx421-wb*")))
  (with-current-buffer buf (insert "wb"))
  (set-window-buffer (selected-window) buf)
  (list (eq (window-buffer) buf)
        (buffer-string)))
"##,
        expect,
    );
}

/// with-temp-buffer nested with local variables.
#[test]
fn div_cx421_with_temp_buffer_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"outer\" \"inner\" \"outer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "outer")
  (list (buffer-string)
        (with-temp-buffer
          (insert "inner")
          (buffer-string))
        (buffer-string)))
"##,
        expect,
    );
}

/// prin1 with different print-readably and print-escape flags.
#[test]
fn div_cx421_prin1_escape_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"\\\"hello\\\\ncaf\\\\x00e9\\\\n\\\\x4e16\\\\x754c\\\"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-escape-newlines t)
      (print-escape-nonascii t)
      (print-escape-multibyte t))
  (prin1-to-string "hello\ncafé\n世界"))
"##,
        expect,
    );
}

/// format with %S on self-referencing and circular structures.
#[test]
fn div_cx421_format_self_referencing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#1=(#1#)\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((x (list 1))
       (print-circle t))
  (setcar x x)
  (format "%S" x))
"##,
        expect,
    );
}

/// eval-region / eval-buffer in temp buffer.
#[test]
fn div_cx421_eval_region_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(+ 1 2)")
  (eval-region (point-min) (point-max) nil))
"##,
        expect,
    );
}

/// set-buffer with dead buffer.
#[test]
fn div_cx421_set_buffer_dead() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *cx421-dead*")))
  (kill-buffer buf)
  (condition-case e
      (set-buffer buf)
    (error (car e))))
"##,
        expect,
    );
}

/// buffer-size in narrowed buffers with invisible text.
#[test]
fn div_cx421_buffer_size_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (narrow-to-region 3 8)
  (buffer-size))
"##,
        expect,
    );
}

/// float / truncate on bignum values.
#[test]
fn div_cx421_float_truncate_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1.2676506002282294e+30 1267650600228229401496703205376 1267650600228229401496703205376)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 100)))
  (list (condition-case e (float big) (error (car e)))
        (condition-case e (truncate big) (error (car e)))
        (condition-case e (floor big) (error (car e)))))
"##,
        expect,
    );
}
