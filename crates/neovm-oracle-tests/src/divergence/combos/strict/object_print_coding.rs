//! Strict combo oracle probes: object print forms (marker/overlay/frame/
//! display-table/standard tables) and coding-system introspection.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_obj_object_print_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#<marker at 2 in  *temp*>\" \"#<overlay from 1 to 3 in  *temp*>\" \"#^[nil nil display-table nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil]\" t t t t)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((m (make-marker))
        (o (make-overlay 1 3)))
    (set-marker m 2)
    (list (format "%s" m)
          (format "%s" o)
          (format "%s" (make-display-table))
          (markerp m)
          (overlayp o)
          (framep (selected-frame))
          (windowp (selected-window)))))
"##,
        expect,
    );
}

#[test]
fn div_obj_standard_table_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 0 category-table t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-table-p (standard-category-table))
      (char-table-extra-slot (make-char-table 'category-table 0) 0)
      (char-table-subtype (standard-category-table))
      (syntax-table-p (standard-syntax-table))
      (bool-vector-p (make-bool-vector 8 nil)))
"##,
        expect,
    );
}

#[test]
fn div_obj_coding_system_introspect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t t nil utf-8 iso-2022 emacs-mule 0 1 2 utf-8 utf-8 utf-8 utf-8)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-p 'utf-8)
      (coding-system-p 'undecided)
      (coding-system-p 'nonexistent-probe-cs)
      (coding-system-type 'utf-8)
      (coding-system-type 'iso-2022-jp)
      (coding-system-type 'emacs-mule)
      (coding-system-eol-type 'utf-8-unix)
      (coding-system-eol-type 'utf-8-dos)
      (coding-system-eol-type 'utf-8-mac)
      (coding-system-base 'utf-8-unix)
      (coding-system-get 'utf-8 :mime-charset)
      (coding-system-get 'utf-8 :name)
      (check-coding-system 'utf-8))
"##,
        expect,
    );
}

#[test]
fn div_obj_check_coding_systems_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (check-coding-systems-region 1 1 '(utf-8 latin-1 iso-8859-1))
      (check-coding-systems-region (point-min) (point-min) '(utf-8-unix))
      (let ((s "abc"))
        (with-temp-buffer
          (insert s)
          (check-coding-systems-region 1 4 '(ascii)))))
"##,
        expect,
    );
}
