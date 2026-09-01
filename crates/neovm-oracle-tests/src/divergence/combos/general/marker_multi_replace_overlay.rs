//! Combo: marker adjustment during multiple replace-match + overlays + narrow + undo.
//! Tests complex marker adjustment scenarios with multiple replacements.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_marker_multi_replace_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mmr")))
    (with-current-buffer buf
      (insert "foo bar baz qux quux")
      (put-text-property 1 4 'w 'foo)
      (put-text-property 5 8 'w 'bar)
      (put-text-property 9 12 'w 'baz)
      (put-text-property 13 16 'w 'qux)
      (put-text-property 17 21 'w 'quux)
      (let* ((ov1 (make-overlay 1 12))
             (ov2 (make-overlay 13 21))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m1 (make-marker))
             (m2 (make-marker))
             (m3 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 13))
             (_ (set-marker m3 20)))
        (undo-boundary)
        (goto-char 1)
        (re-search-forward "foo")
        (replace-match "FOOFOO")
        (undo-boundary)
        (goto-char (marker-position m2))
        (re-search-forward "qux")
        (replace-match "Q")
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (mp3 (marker-position m3))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (s (buffer-string)))
          (primitive-undo 2 buffer-undo-list)
          (list mp1 mp2 mp3 os1 oe1 os2 oe2 s
                (marker-position m1)
                (marker-position m2)
                (marker-position m3)
                (overlay-start ov1)
                (overlay-end ov2)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_marker_multi_replace_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mnr")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (put-text-property 21 25 'z 'e)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 8))
             (_ (set-marker m2 18)))
        (narrow-to-region 6 20)
        (undo-boundary)
        (goto-char (point-min))
        (re-search-forward "BBBB")
        (replace-match "XX")
        (undo-boundary)
        (goto-char (point-min))
        (re-search-forward "DDDD")
        (replace-match "YY")
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 2 buffer-undo-list)
          (widen)
          (list mp1 mp2 os oe bs
                (marker-position m1)
                (marker-position m2)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_marker_multi_replace_clone_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mcu")))
    (with-current-buffer buf
      (insert "hello world test foo bar")
      (put-text-property 1 6 'w 'hello)
      (put-text-property 7 12 'w 'world)
      (put-text-property 13 17 'w 'test)
      (put-text-property 18 21 'w 'foo)
      (put-text-property 22 25 'w 'bar)
      (let* ((ov (make-overlay 7 17))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 13))
             (clone (clone-buffer "mcu-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (goto-char 7)
          (re-search-forward "world")
          (replace-match "W")
          (undo-boundary)
          (goto-char 13)
          (re-search-forward "test")
          (replace-match "T")
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (s (buffer-string)))
            (primitive-undo 2 buffer-undo-list)
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
fn combo_marker_multi_replace_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mtu")))
    (with-current-buffer buf
      (insert "aaa bbb ccc ddd eee")
      (put-text-property 1 4 'p 1)
      (put-text-property 5 8 'p 2)
      (put-text-property 9 12 'p 3)
      (put-text-property 13 16 'p 4)
      (put-text-property 17 20 'p 5)
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
        (re-search-forward "aaa")
        (replace-match "AAA")
        (undo-boundary)
        (goto-char 13)
        (re-search-forward "ddd")
        (replace-match "DDD")
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (k1 (get-text-property 1 'p))
              (k13 (get-text-property 13 'p))
              (s (buffer-string)))
          (primitive-undo 2 buffer-undo-list)
          (list mp1 mp2 os1 oe1 os2 oe2 k1 k13 s
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
fn combo_marker_multi_replace_multi_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mmo")))
    (with-current-buffer buf
      (insert "aa-bb cc-dd ee-ff")
      (put-text-property 1 3 'z 'a)
      (put-text-property 4 6 'z 'b)
      (put-text-property 7 9 'z 'c)
      (put-text-property 10 12 'z 'd)
      (put-text-property 13 15 'z 'e)
      (put-text-property 16 18 'z 'f)
      (let* ((ov1 (make-overlay 1 9))
             (ov2 (make-overlay 10 18))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m1 (make-marker))
             (m2 (make-marker))
             (m3 (make-marker))
             (_ (set-marker m1 4))
             (_ (set-marker m2 10))
             (_ (set-marker m3 16)))
        (undo-boundary)
        (goto-char 1)
        (re-search-forward "aa")
        (replace-match "AA")
        (undo-boundary)
        (goto-char (marker-position m2))
        (re-search-forward "dd")
        (replace-match "DD")
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (mp3 (marker-position m3))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (s (buffer-string)))
          (primitive-undo 2 buffer-undo-list)
          (list mp1 mp2 mp3 os1 oe1 os2 oe2 s
                (marker-position m1)
                (marker-position m2)
                (marker-position m3)
                (overlay-start ov1)
                (overlay-end ov2)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
