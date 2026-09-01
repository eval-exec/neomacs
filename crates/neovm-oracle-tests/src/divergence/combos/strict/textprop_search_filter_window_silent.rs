//! Strict combo oracle probes, batch 32: text-property-search-forward,
//! filter-buffer-substring, set/window-point roundtrip, re-search noerror
//! variants, and with-silent-modifications.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_g7_text_property_search_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaaabbbbcccc")
  (put-text-property 1 5 'face 'a)
  (put-text-property 5 9 'face 'b)
  (goto-char 1)
  (let ((m (text-property-search-forward 'face 'b t)))
    (list (point)
          (and m (point)))))
"##,
        expect,
    );
}

#[test]
fn div_g7_filter_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"hello\" 0 5 (face bold)) #(\"hello\" 0 5 (face bold)) #(\"hello\" 0 5 (face bold)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (add-text-properties 1 6 '(face bold))
  (list (buffer-substring 1 6)
        (filter-buffer-substring 1 6 nil)
        (filter-buffer-substring 1 6 'noprops)))
"##,
        expect,
    );
}

#[test]
fn div_g7_set_window_point_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-wp3*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b (insert "abcdef"))
        (set-window-point nil 4)
        (list (window-point) (with-current-buffer b (point))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_g7_re_search_noerror_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 nil search-failed 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "foo bar baz")
  (goto-char 1)
  (let ((r1 (re-search-forward "bar" nil t))
        (r2 (progn (goto-char 1) (re-search-forward "xxx" nil t)))
        (r3 (condition-case err
                (progn (goto-char 1) (re-search-forward "xxx"))
              (search-failed (car err)))))
    (list r1 r2 r3 (point))))
"##,
        expect,
    );
}

#[test]
fn div_g7_with_silent_modifications() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abcdef\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (set-buffer-modified-p nil)
  (with-silent-modifications
    (insert "def"))
  (list (buffer-string) (buffer-modified-p)))
"##,
        expect,
    );
}
