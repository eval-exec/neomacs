//! Deep combo: sort-lines × sort-paragraphs × sort-regexp-fields ×
//! sort × sort-fields × marker × overlay × textprop × undo ×
//! buffer-local × narrow.
//!
//! Stresses sorting commands with buffer state: line-level, paragraph-level,
//! and regexp-based sorting. Sorting is tricky because it reorders buffer
//! content and must correctly track markers, overlays, text properties,
//! and undo state through the reordering.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_sort_lines_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-sl")))
    (with-current-buffer buf
      (insert "delta\nalpha\ngamma\nbeta\nepsilon")
      (put-text-property 1 6 'line 'delta)
      (put-text-property 7 12 'line 'alpha)
      (put-text-property 13 19 'line 'gamma)
      (put-text-property 20 25 'line 'beta)
      (put-text-property 26 34 'line 'epsilon)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 34)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (sort-lines nil 1 34)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'line)
                           (get-text-property 7 'line)
                           (get-text-property 13 'line)
                           (get-text-property 20 'line)
                           (get-text-property 26 'line))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'line)
                                (get-text-property 7 'line)
                                (get-text-property 13 'line)
                                (get-text-property 20 'line)
                                (get-text-property 26 'line))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_sort_lines_reverse_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-slr")))
    (with-current-buffer buf
      (insert "alpha\nbeta\ngamma\ndelta\nepsilon")
      (put-text-property 1 6 'line 'alpha)
      (put-text-property 7 12 'line 'beta)
      (put-text-property 13 19 'line 'gamma)
      (put-text-property 20 25 'line 'delta)
      (put-text-property 26 34 'line 'epsilon)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 34)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (sort-lines t 1 34)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'line)
                           (get-text-property 7 'line)
                           (get-text-property 13 'line)
                           (get-text-property 20 'line)
                           (get-text-property 26 'line))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'line)
                                (get-text-property 7 'line)
                                (get-text-property 13 'line)
                                (get-text-property 20 'line)
                                (get-text-property 26 'line))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_sort_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-snar")))
    (with-current-buffer buf
      (insert "AAAA\ndelta\nalpha\ngamma\nbeta\nEEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 12 'sect 'delta)
      (put-text-property 13 18 'sect 'alpha)
      (put-text-property 19 25 'sect 'gamma)
      (put-text-property 26 31 'sect 'beta)
      (put-text-property 32 37 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 6 31)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 6 31)
        (sort-lines nil (point-min) (point-max))
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 13 'sect)
                           (get-text-property 19 'sect)
                           (get-text-property 26 'sect)
                           (get-text-property 32 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 13 'sect)
                                (get-text-property 19 'sect)
                                (get-text-property 26 'sect)
                                (get-text-property 32 'sect))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_sort_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-sbl")))
    (with-current-buffer buf
      (make-local-variable 'sort-local)
      (setq sort-local 'buffer-specific)
      (insert "delta\nalpha\ngamma\nbeta")
      (put-text-property 1 6 'line 'delta)
      (put-text-property 7 12 'line 'alpha)
      (put-text-property 13 19 'line 'gamma)
      (put-text-property 20 25 'line 'beta)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (sort-lines nil 1 25)
        (let ((after (list (buffer-string)
                           sort-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'line)
                           (get-text-property 7 'line)
                           (get-text-property 13 'line)
                           (get-text-property 20 'line))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                sort-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'line)
                                (get-text-property 7 'line)
                                (get-text-property 13 'line)
                                (get-text-property 20 'line))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_sort_fields_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-sf")))
    (with-current-buffer buf
      (insert "ccc 333\naaa 111\nbbb 222\nddd 444")
      (put-text-property 1 8 'entry 'one)
      (put-text-property 9 16 'entry 'two)
      (put-text-property 17 24 'entry 'three)
      (put-text-property 25 32 'entry 'four)
      (let ((m1 (copy-marker 8 nil))
            (m2 (copy-marker 16 t))
            (ov (make-overlay 1 32)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (sort-fields 1 1 32)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'entry)
                           (get-text-property 9 'entry)
                           (get-text-property 17 'entry)
                           (get-text-property 25 'entry))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'entry)
                                (get-text-property 9 'entry)
                                (get-text-property 17 'entry)
                                (get-text-property 25 'entry))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}
