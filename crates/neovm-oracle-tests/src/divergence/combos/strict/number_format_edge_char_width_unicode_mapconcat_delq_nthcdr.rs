//! Strict combo oracle probes, batch 123: number formatting edges, char-width
//! of unusual Unicode categories, mapconcat/delq/nthcdr edge cases, and
//! marker in killed buffer. Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_t7_number_format_extreme_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(list (number-to-string most-negative-fixnum)
      (format "%d" (1- most-negative-fixnum))
      (format "%d" (expt 2 70))
      (format "%o" 8)
      (format "%x" 255)
      (format "%X" 255)
      (format "%#o" 64)
      (format "%#x" 255)
      (format "%b" 10)
      (format "%+d" 0)
      (format "% d" 0)
      (format "%05.2f" 0)
      (format "%-10.3e|" 3.14159)
      (number-to-string -0.0)
      (format "%s" 1e20)
      (format "%g" 0.0001)
      (format "%g" 0.00001)
      (format "%.15g" (/ 1.0 7.0)))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"-2305843009213693952\" \"-2305843009213693953\" \"1180591620717411303424\" \"10\" \"ff\" \"FF\" \"0100\" \"0xff\" \"1010\" \"+0\" \" 0\" \"00.00\" \"3.142e+00 |\" \"-0.0\" \"1e+20\" \"0.0001\" \"1e-05\" \"0.142857142857143\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t7_char_width_unusual_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(list (char-width 160)
      (char-width 173)
      (char-width 8203)
      (char-width 8204)
      (char-width 8205)
      (char-width 8237)
      (char-width 8238)
      (char-width 8288)
      (char-width 12288)
      (char-width 65279)
      (string-width (string 65 160 66))
      (string-width "a​b")
      (string-width "a‌b")
      (string-width "a‍b")
      (string-width (string 65 8237 66))
      (string-width "a　b"))
"#;
    let expect = expect_test::expect![[r#""OK (1 1 0 0 0 0 0 0 2 0 3 2 2 2 2 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t7_mapconcat_delq_nthcdr_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(list (mapconcat #'identity '() "-")
      (mapconcat #'identity '("a") "-")
      (mapconcat #'identity '("a" "b" "c") "")
      (mapconcat #'number-to-string '(1 2 3) ", ")
      (delq nil '(1 nil 2 nil 3))
      (delq 'a '(a b a c a))
      (remq 'a '(1 a 2 a 3))
      (nthcdr 0 '(a b c))
      (nthcdr 2 '(a b c))
      (nthcdr 5 '(a b c))
      (nthcdr 0 nil)
      (last '(a b c))
      (last '(a b c) 2)
      (last nil)
      (butlast '(a b c d))
      (butlast '(a b c d) 2))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"a\" \"abc\" \"1, 2, 3\" (1 2 3) (b c) (1 2 3) (a b c) (c) nil nil (c) (b c) nil (a b c) (a b))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t7_marker_killed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let* ((b (generate-new-buffer " *probe-mk*"))
       (m (set-marker (make-marker) 3 b)))
  (list (marker-position m)
        (eq (marker-buffer m) b)
        (progn (kill-buffer b)
               (marker-position m))
        (marker-buffer m)
        (markerp m)
        (progn (set-marker m 5) (marker-position m))))
"#;
    let expect = expect_test::expect![[r#""OK (1 t nil nil t 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t7_assq_delete_all_duplicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(list (assq-delete-all 'a '((a . 1) (b . 2) (a . 3) (c . 4) (a . 5)))
      (assq-delete-all 'z '((a . 1) (b . 2)))
      (assoc-delete-all "a" '(("a" . 1) ("b" . 2) ("a" . 3)))
      (let ((al '((a . 1) (b . 2) (a . 3) (c . 4))))
        (assq-delete-all 'a al))
      (rassq-delete-all 2 '((a . 1) (b . 2) (c . 2) (d . 3))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((b . 2) (c . 4)) ((a . 1) (b . 2)) ((\"b\" . 2)) ((b . 2) (c . 4)) ((a . 1) (d . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
