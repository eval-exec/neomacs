//! Complex combo batch 325 — `print`/`read` engine ultimate: print-circle,
//! print-gensym, print-length, print-level, print-quoted, print-escape
//! variants with deeply shared/circular structures and uninterned symbols.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx325_print_circle_deeply_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"(#1=(1 2 3) #1# #1#)\" \"((1 2 3) (1 2 3) (1 2 3))\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((inner (list 1 2 3))
       (data (list inner inner inner)))
  (list (let ((print-circle t)) (prin1-to-string data))
        (let ((print-circle nil))
          (condition-case e (prin1-to-string data) (error (car e))))))
"##,
        expect,
    )
}

#[test]
fn div_cx325_print_circle_circular_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK (\"#1=(1 2 3 . #1#)\" \"(1 2 3 1 2 . #2)\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((circular (list 1 2 3)))
  (setcdr (cddr circular) circular)
  (list (let ((print-circle t)) (prin1-to-string circular))
        (let ((print-circle nil))
          (condition-case e (prin1-to-string circular) (error (car e))))))
"##,
        expect,
    )
}

#[test]
fn div_cx325_print_gensym_uninterned_in_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK (\"G-0\" \"#:G-0\" \"G-0\" \"(#1=#:G-0 #1#)\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((gs (gensym "G-")))
  (list (symbol-name gs)
        (let ((print-gensym t)) (prin1-to-string gs))
        (let ((print-gensym nil)) (prin1-to-string gs))
        (let ((print-gensym t) (print-circle t))
          (prin1-to-string (list gs gs)))))
"##,
        expect,
    )
}

#[test]
fn div_cx325_print_length_and_level_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"((...) (1 2 3 ...))\" \"...\" \"(((((\\\"deep\\\")))) (1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((deep '(((("deep")))))
      (long (number-sequence 1 50)))
  (list (let ((print-length 3) (print-level 2))
          (prin1-to-string (list deep long)))
        (let ((print-length 0) (print-level 0))
          (prin1-to-string (list deep long)))
        (let ((print-length nil) (print-level nil))
          (prin1-to-string (list deep long)))))
"##,
        expect,
    )
}

#[test]
fn div_cx325_print_quoted_emits_quote_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(alpha (beta (gamma delta)))\" \"(alpha (beta (gamma delta)))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(alpha (beta (gamma delta)))))
  (list (let ((print-quoted t)) (prin1-to-string data))
        (let ((print-quoted nil)) (prin1-to-string data))))
"##,
        expect,
    )
}

#[test]
fn div_cx325_print_escape_nonascii_and_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\"\\\\377\\\\376\\\"\" \"\\\"\\\\377\\\\376\\\"\" \"\\\"\\\\377\\\\376\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (decode-coding-string (unibyte-string #xff #xfe) 'utf-8-unix t)))
  (list (prin1-to-string s)
        (let ((print-escape-nonascii t)) (prin1-to-string s))
        (let ((print-escape-multibyte t)) (prin1-to-string s))))
"##,
        expect,
    )
}

#[test]
fn div_cx325_read_circle_shared_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(#1=(1 2 3) #1#)\" ((1 2 3) (1 2 3)) t (:err . invalid-read-syntax))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((shared (list 1 2 3))
       (data (list shared shared))
       (printed (let ((print-circle t)) (prin1-to-string data)))
       (read-with (let ((read-circle t)) (read-from-string printed)))
       (read-without (let ((read-circle nil))
                       (condition-case e (read-from-string printed)
                         (error (cons :err (car e)))))))
  (list printed
        (car read-with)
        (eq (car (car read-with)) (cadr (car read-with)))
        read-without))
"##,
        expect,
    )
}

#[test]
fn div_cx325_prin1_vs_princ_with_strings_and_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function princ-to-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "with \"quotes\" and \\ backslash")
      (lst '(1 "two" (3 4))))
  (list (prin1-to-string s)
        (princ-to-string s)
        (prin1-to-string lst)
        (princ-to-string lst)
        (length (prin1-to-string s))
        (length (princ-to-string s))))
"##,
        expect,
    )
}

#[test]
fn div_cx325_pp_to_string_with_deep_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t nil \"(:config (:option-a \\\"value\\\" :option-b (:nested-a 1 :nested-b 2))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(:config
              (:option-a "value"
               :option-b (:nested-a 1
                          :nested-b 2))
              :option-c (1 2 3))))
  (let ((pp-str (pp-to-string data))
        (p1-str (prin1-to-string data)))
    (list (> (length pp-str) (length p1-str))
          (> (length (split-string pp-str "\n")) 3)
          (car (split-string pp-str "\n")))))
"##,
        expect,
    )
}

#[test]
fn div_cx325_print_read_with_marker_overlay_undo_narrow_mega() {
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
      (narrow-to-region 2 18)
      (let ((state (list printed
                         (eq (car (car data)) (cadr data))
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}
