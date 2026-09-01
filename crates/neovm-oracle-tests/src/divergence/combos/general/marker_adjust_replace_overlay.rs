//! Combo: marker adjustment during replace-match + overlays + narrow + undo.
//! Tests how markers adjust when replace-match changes buffer size.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_marker_adjust_replace_match_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mar")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (put-text-property 16 20 'seg 'd)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (m1 (make-marker))
             (m2 (make-marker))
             (m3 (make-marker))
             (_ (set-marker m1 6))
             (_ (set-marker m2 10))
             (_ (set-marker m3 15)))
        (undo-boundary)
        (narrow-to-region 6 15)
        (goto-char (point-min))
        (re-search-forward "BBBB")
        (replace-match "XX")
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (mp3 (marker-position m3))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp1 mp2 mp3 os oe bs
                (marker-position m1)
                (marker-position m2)
                (marker-position m3)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_marker_adjust_replace_longer_shorter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mrl")))
    (with-current-buffer buf
      (insert "aaa bbb ccc ddd eee")
      (put-text-property 1 4 'w 'a)
      (put-text-property 5 8 'w 'b)
      (put-text-property 9 12 'w 'c)
      (put-text-property 13 16 'w 'd)
      (put-text-property 17 20 'w 'e)
      (let* ((ov (make-overlay 5 16))
             (_ (overlay-put ov 'priority 5))
             (m1 (make-marker))
             (m2 (make-marker))
             (m3 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 9))
             (_ (set-marker m3 16)))
        (undo-boundary)
        (goto-char 5)
        (re-search-forward "bbb")
        (replace-match "BBBBB")
        (undo-boundary)
        (goto-char (marker-position m2))
        (re-search-forward "ccc")
        (replace-match "CC")
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (mp3 (marker-position m3))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (s (buffer-string)))
          (primitive-undo 2 buffer-undo-list)
          (list mp1 mp2 mp3 os oe s
                (marker-position m1)
                (marker-position m2)
                (marker-position m3)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_marker_adjust_multi_replace_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"baz\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mmr")))
    (with-current-buffer buf
      (insert "foo bar baz qux quux")
      (let* ((ov1 (make-overlay 1 8))
             (ov2 (make-overlay 8 16))
             (ov3 (make-overlay 16 21))
             (_ (overlay-put ov1 'zone 'first))
             (_ (overlay-put ov2 'zone 'second))
             (_ (overlay-put ov3 'zone 'third))
             (m1 (make-marker))
             (m2 (make-marker))
             (m3 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 12))
             (_ (set-marker m3 19)))
        (undo-boundary)
        (goto-char 1)
        (re-search-forward "foo")
        (replace-match "FOOFOO")
        (undo-boundary)
        (goto-char (marker-position m2))
        (re-search-forward "baz")
        (replace-match "B")
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (mp3 (marker-position m3))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (os3 (overlay-start ov3))
              (oe3 (overlay-end ov3))
              (s (buffer-string)))
          (primitive-undo 2 buffer-undo-list)
          (list mp1 mp2 mp3 os1 oe1 os2 oe2 os3 oe3 s
                (marker-position m1)
                (marker-position m2)
                (marker-position m3)
                (overlay-start ov1)
                (overlay-end ov3)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_marker_adjust_narrow_replace_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mnr")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 10)
        (undo-boundary)
        (goto-char (point-min))
        (re-search-forward "BBBB")
        (replace-match "XX")
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property 6 'zone))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k bs
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_marker_adjust_clone_replace_undo() {
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
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 13))
             (clone (clone-buffer "mcu-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (goto-char 7)
          (re-search-forward "world")
          (replace-match "W")
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
