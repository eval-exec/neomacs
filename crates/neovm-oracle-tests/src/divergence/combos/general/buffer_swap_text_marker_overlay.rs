//! Combo: buffer-swap-text + markers + overlays + textprop + undo.
//! Tests swapping buffer text between two buffers.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_buffer_swap_text_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-swap-text 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "bs1"))
        (b2 (generate-new-buffer "bs2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB")
      (put-text-property 1 5 'src 'b1a)
      (put-text-property 6 10 'src 'b1b))
    (with-current-buffer b2
      (insert "CCCC-DDDD")
      (put-text-property 1 5 'src 'b2c)
      (put-text-property 6 10 'src 'b2d))
    (let* ((m1 (with-current-buffer b1
                 (let ((m (make-marker)))
                   (set-marker m 6) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker)))
                   (set-marker m 4) m)))
           (ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 1 10)))
                    (overlay-put ov 'face 'bold) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 1 10)))
                    (overlay-put ov 'face 'italic) ov))))
      (buffer-swap-text b1 b2)
      (let ((s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string)))
            (mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2))
            (k1 (with-current-buffer b1 (get-text-property 1 'src)))
            (k2 (with-current-buffer b2 (get-text-property 1 'src))))
        (list s1 s2 mp1 mp2 os1 oe1 os2 oe2 k1 k2)))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buffer_swap_text_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-swap-text 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "su1"))
        (b2 (generate-new-buffer "su2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b))
    (with-current-buffer b2
      (insert "CCCC-DDDD")
      (put-text-property 1 5 'z 'c)
      (put-text-property 6 10 'z 'd))
    (let ((m1 (with-current-buffer b1
                (let ((m (make-marker)))
                  (set-marker m 6) m)))
          (m2 (with-current-buffer b2
                (let ((m (make-marker)))
                  (set-marker m 4) m))))
      (with-current-buffer b1 (undo-boundary))
      (buffer-swap-text b1 b2)
      (let ((s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string)))
            (mp1 (marker-position m1))
            (mp2 (marker-position m2)))
        (with-current-buffer b1
          (primitive-undo 1 buffer-undo-list))
        (list s1 s2 mp1 mp2
              (with-current-buffer b1 (buffer-string))
              (with-current-buffer b2 (buffer-string))
              (marker-position m1)
              (marker-position m2))))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buffer_swap_text_overlay_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-swap-text 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "sn1"))
        (b2 (generate-new-buffer "sn2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c))
    (with-current-buffer b2
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'z 'd)
      (put-text-property 6 10 'z 'e)
      (put-text-property 11 15 'z 'f))
    (let ((m1 (with-current-buffer b1
                (let ((m (make-marker)))
                  (set-marker m 8) m)))
          (ov1 (with-current-buffer b1
                 (let ((ov (make-overlay 1 15)))
                   (overlay-put ov 'face 'region) ov))))
      (with-current-buffer b1
        (narrow-to-region 6 10))
      (buffer-swap-text b1 b2)
      (let ((mp1 (marker-position m1))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string)))
            (k1 (with-current-buffer b1 (get-text-property 1 'z)))
            (k2 (with-current-buffer b2 (get-text-property 1 'z))))
        (list mp1 os1 oe1 s1 s2 k1 k2)))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buffer_swap_text_clone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-swap-text 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "sc1"))
        (b2 (generate-new-buffer "sc2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b))
    (with-current-buffer b2
      (insert "CCCC-DDDD")
      (put-text-property 1 5 'z 'c)
      (put-text-property 6 10 'z 'd))
    (let ((m1 (with-current-buffer b1
                (let ((m (make-marker)))
                  (set-marker m 6) m)))
          (ov1 (with-current-buffer b1
                 (let ((ov (make-overlay 1 10)))
                   (overlay-put ov 'face 'bold) ov))))
      (buffer-swap-text b1 b2)
      (let ((mp1 (marker-position m1))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string)))
            (k1 (with-current-buffer b1 (get-text-property 1 'z)))
            (k2 (with-current-buffer b2 (get-text-property 1 'z))))
        (list mp1 os1 oe1 s1 s2 k1 k2)))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buffer_swap_text_multi_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments put-text-property 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "sm1"))
        (b2 (generate-new-buffer "sm2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a 'p 1)
      (put-text-property 6 10 'z 'b 'p 2)
      (put-text-property 11 15 'z 'c 'p 3))
    (with-current-buffer b2
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'z 'd 'p 4)
      (put-text-property 6 10 'z 'e 'p 5)
      (put-text-property 11 15 'z 'f 'p 6))
    (let ((m1 (with-current-buffer b1
                (let ((m (make-marker)))
                  (set-marker m 8) m)))
          (ov1 (with-current-buffer b1
                 (let ((ov (make-overlay 6 10)))
                   (overlay-put ov 'face 'highlight) ov))))
      (buffer-swap-text b1 b2)
      (let ((mp1 (marker-position m1))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string)))
            (k1 (with-current-buffer b1 (get-text-property 1 'z)))
            (p1 (with-current-buffer b1 (get-text-property 1 'p))))
        (list mp1 os1 oe1 s1 s2 k1 p1)))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}
