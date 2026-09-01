//! Oracle parity tests for GNU marker edge semantics.
//!
//! GNU implements marker primitives in `src/marker.c`.  These tests focus on
//! clipping, detachment, dead-buffer handling, insertion-type return values, and
//! copy-marker edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_marker_set_clips_to_accessible_buffer_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc")
  (let ((m (make-marker)))
    (list
     (set-marker m -10)
     (marker-position m)
     (set-marker m 999)
     (marker-position m)
     (point-min)
     (point-max))))
"#;

    let expect =
        expect_test::expect![[r#""OK (#<marker in no buffer> 1 #<marker in no buffer> 4 1 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_marker_detach_and_last_position_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef")
  (let ((m (copy-marker 4 t)))
    (list
     (marker-position m)
     (marker-buffer m)
     (marker-last-position m)
     (set-marker m nil)
     (marker-position m)
     (marker-buffer m)
     (marker-last-position m)
     (marker-insertion-type m))))
"#;

    let expect = expect_test::expect![[
        r#""OK (4 #<killed buffer> 4 #<marker (moves after insertion) in no buffer> nil nil 4 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_marker_dead_buffer_becomes_nowhere_but_keeps_last_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((buf (generate-new-buffer " *marker-dead-oracle*"))
      marker before-pos before-last)
  (with-current-buffer buf
    (insert "abcdef")
    (setq marker (copy-marker 5)
          before-pos (marker-position marker)
          before-last (marker-last-position marker)))
  (kill-buffer buf)
  (list
   before-pos
   before-last
   (marker-buffer marker)
   (marker-position marker)
   (marker-last-position marker)))
"#;

    let expect = expect_test::expect![[r#""OK (5 5 nil nil 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_marker_nil_number_marker_and_insertion_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc")
  (let* ((nowhere (copy-marker))
         (num (copy-marker 2 t))
         (copy (copy-marker num nil)))
    (list
     (marker-position nowhere)
     (marker-buffer nowhere)
     (marker-insertion-type nowhere)
     (marker-position num)
     (marker-insertion-type num)
     (marker-position copy)
     (marker-insertion-type copy)
     (eq (marker-buffer num) (current-buffer))
     (eq (marker-buffer copy) (current-buffer)))))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil nil 2 t 2 nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_marker_number_clips_to_buffer_bounds_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs marker.c:Fcopy_marker creates a marker and delegates numeric
    // positions to Fset_marker, so out-of-range fixnums are clipped to the
    // current buffer's full bounds.
    let form = r#"
(with-temp-buffer
  (insert "abc")
  (let ((low (copy-marker -10))
        (high (copy-marker 999)))
    (list
     (marker-position low)
     (marker-position high)
     (point-min)
     (point-max))))
"#;

    let expect = expect_test::expect![[r#""OK (1 4 1 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_set_marker_insertion_type_returns_requested_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((m (make-marker)))
  (list
   (set-marker-insertion-type m 'side)
   (marker-insertion-type m)
   (set-marker-insertion-type m nil)
   (marker-insertion-type m)
   (condition-case err
       (set-marker-insertion-type 42 t)
     (error (list (car err) (cdr err))))))
"#;

    let expect =
        expect_test::expect![[r#""OK (side t nil nil (wrong-type-argument (markerp 42)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_set_marker_from_marker_in_other_buffer_recomputes_target_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs marker.c:set_marker_internal copies charpos from a marker in
    // another buffer, but recomputes bytepos in the target buffer.
    let form = r#"
(let ((src-buf (generate-new-buffer " *marker-src-oracle*"))
      (dst-buf (generate-new-buffer " *marker-dst-oracle*"))
      src dst)
  (unwind-protect
      (progn
        (with-current-buffer src-buf
          (insert "aébc")
          (setq src (copy-marker 4)))
        (with-current-buffer dst-buf
          (insert "αβγδε")
          (setq dst (make-marker))
          (set-marker dst src (current-buffer))
          (list (marker-position src)
                (eq (marker-buffer src) src-buf)
                (marker-position dst)
                (eq (marker-buffer dst) dst-buf)
                (char-after dst))))
    (kill-buffer src-buf)
    (kill-buffer dst-buf)))
"#;

    let expect = expect_test::expect![[r#""OK (4 t 4 t 948)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
