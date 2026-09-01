//! Complex combo batch 370 — `read`/`print` engine ultimate: reader macros
//! (#. eval, #_ skip, #s record, #N= #N# shared/circular, #[...] bytecode),
//! print-circle/print-gensym/print-length/print-level combinations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx370_read_reader_macros_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:err . invalid-read-syntax) (:err . invalid-read-syntax) skipped (a . b))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (car (read-from-string "#.(+ 1 2)")) (error (cons :err (car e))))
      (condition-case e (car (read-from-string "#.(* 6 7)")) (error (cons :err (car e))))
      (condition-case e (car (read-from-string "#_skipped actual-value")) (error (cons :err (car e))))
      (condition-case e (car (read-from-string "#1=(a . b) #1#")) (error (cons :err (car e)))))
"##,
        expect,
    )
}

#[test]
fn div_cx370_print_circle_deeply_shared_and_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"(#1=(1 2 3) #1# #1#)\" \"((1 2 3) (1 2 3) (1 2 3))\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((inner (list 1 2 3))
       (shared (list inner inner inner)))
  (list (let ((print-circle t)) (prin1-to-string shared))
        (let ((print-circle nil))
          (condition-case e (prin1-to-string shared) (error (car e))))))
"##,
        expect,
    )
}

#[test]
fn div_cx370_print_circle_circular_list_print_round_trip() {
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
fn div_cx370_print_gensym_uninterned_in_shared() {
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
fn div_cx370_print_length_and_level_combined() {
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
fn div_cx370_read_circle_shared_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(#1=(1 2 3) #1#)\" ((1 2 3) (1 2 3)) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((shared (list 1 2 3))
       (data (list shared shared))
       (printed (let ((print-circle t)) (prin1-to-string data)))
       (read-with (let ((read-circle t)) (read-from-string printed))))
  (list printed
        (car read-with)
        (eq (car (car read-with)) (cadr (car read-with)))))
"##,
        expect,
    )
}

#[test]
fn div_cx370_prin1_vs_princ_with_strings_and_structures() {
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
fn div_cx370_pp_to_string_with_deep_indent() {
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
fn div_cx370_read_special_syntaxes_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((\"[1 2 3]\" [1 2 3] vector) (\"#(1 2 3)\" :err invalid-read-syntax) (\"#s(record a b c)\" #s(record a b c) record) (\"?A\" 65 integer) (\"#x10\" 16 integer) (\"#o17\" 15 integer) (\"#b1010\" 10 integer) (\"1.5\" 1.5 float) (\"1/2\" 1/2 symbol) (\"1000000000000000000000\" 1000000000000000000000 integer))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (let ((v (car (read-from-string s))))
                (list s v (type-of v)))
            (error (list s :err (car e)))))
        '("[1 2 3]"
          "#(1 2 3)"
          "#s(record a b c)"
          "?A"
          "#x10"
          "#o17"
          "#b1010"
          "1.5"
          "1/2"
          "1000000000000000000000"))
"##,
        expect,
    )
}

#[test]
fn div_cx370_print_read_with_marker_overlay_undo_narrow_mega() {
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
