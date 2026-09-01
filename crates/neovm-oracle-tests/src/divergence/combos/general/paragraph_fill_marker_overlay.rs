//! Combo: forward-paragraph/backward-paragraph + fill-prefix + markers + overlays + undo.
//! Tests paragraph navigation with fill-prefix and buffer state interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_paragraph_nav_fill_prefix_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "pfn")))
    (with-current-buffer buf
      (insert "para1 line2\n\npara2 line2\n\npara3 line2")
      (put-text-property 1 10 'para 1)
      (put-text-property 12 22 'para 2)
      (put-text-property 24 34 'para 3)
      (let* ((ov (make-overlay 1 22))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 15))
             (fill-prefix "  "))
        (goto-char (point-min))
        (forward-paragraph)
        (let ((p1 (point))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k1 (get-text-property 1 'para)))
          (forward-paragraph)
          (let ((p2 (point))
                (k2 (get-text-property 12 'para)))
            (backward-paragraph)
            (let ((p3 (point)))
              (list p1 p2 p3 mp os oe k1 k2))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_paragraph_narrow_overlay_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "pnu")))
    (with-current-buffer buf
      (insert "first paragraph here\n\nsecond paragraph here\n\nthird paragraph")
      (put-text-property 1 21 'p 1)
      (put-text-property 23 43 'p 2)
      (put-text-property 45 60 'p 3)
      (let* ((ov (make-overlay 23 43))
             (_ (overlay-put ov 'priority 10))
             (m (make-marker))
             (_ (set-marker m 30)))
        (narrow-to-region 23 43)
        (undo-boundary)
        (goto-char (point-min))
        (insert "XX ")
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'p))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k bs
                (buffer-string)
                (marker-position m)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_paragraph_fill_prefix_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 31 41)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ppt")))
    (with-current-buffer buf
      (insert "line one\nline two\n\nline three\nline four")
      (put-text-property 1 9 'sec 'a)
      (put-text-property 10 18 'sec 'a)
      (put-text-property 20 30 'sec 'b)
      (put-text-property 31 41 'sec 'b)
      (let* ((ov (make-overlay 1 18))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 5))
             (fill-prefix "> ")
             (sentence-end-double-space nil))
        (goto-char (point-min))
        (forward-paragraph)
        (let ((p1 (point))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (sa (get-text-property 1 'sec))
              (sb (get-text-property 20 'sec)))
          (forward-paragraph)
          (let ((p2 (point)))
            (backward-paragraph)
            (let ((p3 (point)))
              (list p1 p2 p3 mp os oe sa sb))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_paragraph_undo_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "pum")))
    (with-current-buffer buf
      (insert "aaa bbb ccc\n\nddd eee fff")
      (put-text-property 1 12 'para 'first)
      (put-text-property 14 25 'para 'second)
      (let* ((ov (make-overlay 14 25))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 18)))
        (undo-boundary)
        (goto-char 14)
        (insert "XX ")
        (undo-boundary)
        (goto-char (point-min))
        (forward-paragraph)
        (let ((p1 (point))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (goto-char (point-min))
          (forward-paragraph)
          (list p1 mp os oe s
                (point)
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_paragraph_fill_prefix_narrow_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "pno")))
    (with-current-buffer buf
      (insert "aaa bbb\nccc ddd\n\neee fff\nggg hhh")
      (put-text-property 1 8 'zone 'x)
      (put-text-property 9 16 'zone 'y)
      (put-text-property 18 25 'zone 'z)
      (put-text-property 26 33 'zone 'w)
      (let* ((ov (make-overlay 1 16))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 5))
             (fill-prefix ">> "))
        (narrow-to-region 1 16)
        (goto-char (point-min))
        (forward-paragraph)
        (let ((p1 (point))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property 1 'zone)))
          (widen)
          (list p1 mp os oe k
                (buffer-string)
                (marker-position m)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
