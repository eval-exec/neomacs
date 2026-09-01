//! Combo: regex multi-match + markers + overlays + undo + narrow.
//! Tests multiple regex matches with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_regex_multi_match_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rmm")))
    (with-current-buffer buf
      (insert "foo bar baz foo bar baz")
      (put-text-property 1 4 'w 'foo1)
      (put-text-property 5 8 'w 'bar1)
      (put-text-property 9 12 'w 'baz1)
      (put-text-property 13 16 'w 'foo2)
      (put-text-property 17 20 'w 'bar2)
      (put-text-property 21 24 'w 'baz2)
      (let* ((ov1 (make-overlay 1 12))
             (ov2 (make-overlay 13 24))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 17)))
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "foo" nil t)
          (replace-match "FOO"))
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp1 mp2 os1 oe1 os2 oe2 s
                (marker-position m1)
                (marker-position m2)
                (overlay-start ov1)
                (overlay-end ov2)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_regex_multi_match_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rmn")))
    (with-current-buffer buf
      (insert "aaa bbb ccc ddd eee")
      (put-text-property 1 4 'p 1)
      (put-text-property 5 8 'p 2)
      (put-text-property 9 12 'p 3)
      (put-text-property 13 16 'p 4)
      (put-text-property 17 20 'p 5)
      (let* ((ov (make-overlay 5 16))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 10)))
        (narrow-to-region 5 16)
        (undo-boundary)
        (goto-char (point-min))
        (while (re-search-forward "bbb\\|ddd" nil t)
          (replace-match "XX"))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe bs
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_regex_multi_match_clone_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rmc")))
    (with-current-buffer buf
      (insert "abc def ghi abc def ghi")
      (put-text-property 1 4 'w 'abc1)
      (put-text-property 5 8 'w 'def1)
      (put-text-property 9 12 'w 'ghi1)
      (put-text-property 13 16 'w 'abc2)
      (put-text-property 17 20 'w 'def2)
      (put-text-property 21 24 'w 'ghi2)
      (let* ((ov (make-overlay 5 20))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 10))
             (clone (clone-buffer "rmc-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (goto-char 1)
          (while (re-search-forward "abc" nil t)
            (replace-match "ABC"))
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe s
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_regex_multi_match_multi_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rmo")))
    (with-current-buffer buf
      (insert "aa1 bb2 cc3 dd4 ee5")
      (put-text-property 1 4 'z 'a)
      (put-text-property 5 8 'z 'b)
      (put-text-property 9 12 'z 'c)
      (put-text-property 13 16 'z 'd)
      (put-text-property 17 20 'z 'e)
      (let* ((ov1 (make-overlay 1 12))
             (ov2 (make-overlay 13 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 15)))
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "[a-z][a-z][0-9]" nil t)
          (replace-match "XX0"))
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp1 mp2 os1 oe1 os2 oe2 s
                (marker-position m1)
                (marker-position m2)
                (overlay-start ov1)
                (overlay-end ov2)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_regex_multi_match_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rmt")))
    (with-current-buffer buf
      (insert "hello world hello world")
      (put-text-property 1 6 'w 'hello1)
      (put-text-property 7 12 'w 'world1)
      (put-text-property 13 18 'w 'hello2)
      (put-text-property 19 24 'w 'world2)
      (let* ((ov (make-overlay 1 24))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "hello" nil t)
          (replace-match "HELLO"))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k1 (get-text-property 1 'w))
              (k13 (get-text-property 13 'w))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe k1 k13 s
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
