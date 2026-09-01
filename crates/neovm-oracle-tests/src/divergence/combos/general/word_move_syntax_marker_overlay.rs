//! Combo: forward-word/backward-word + syntax table + markers + overlays + undo + narrow.
//! Tests word movement across syntax-modified boundaries with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_word_move_syntax_marker_overlay_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "wsm")))
    (with-current-buffer buf
      (insert "hello-world_test.foo bar")
      (let ((st (copy-syntax-table)))
        (modify-syntax-entry ?- "w" st)
        (modify-syntax-entry ?_ "w" st)
        (with-syntax-table st
          (let* ((ov (make-overlay 1 20))
                 (_ (overlay-put ov 'face 'bold))
                 (m (make-marker))
                 (_ (set-marker m 1))
                 (narrow-to-region 1 20))
            (goto-char (point-min))
            (forward-word)
            (let ((p1 (point))
                  (mp1 (marker-position m)))
              (forward-word)
              (let ((p2 (point)))
                (backward-word)
                (let ((p3 (point))
                      (os (overlay-start ov))
                      (oe (overlay-end ov)))
                  (widen)
                  (list p1 p2 p3 mp1 os oe)))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_word_move_syntax_change_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "wcs")))
    (with-current-buffer buf
      (insert "foo-bar baz-qux")
      (let ((st (copy-syntax-table))
            (m (make-marker)))
        (modify-syntax-entry ?- "_" st)
        (set-marker m 1)
        (with-syntax-table st
          (goto-char (point-min))
          (forward-word)
          (let ((p1 (point))
                (mp1 (marker-position m)))
            (backward-word)
            (let ((p2 (point)))
              (forward-word 2)
              (let ((p3 (point)))
                (list p1 p2 p3 mp1)))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_word_move_narrow_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "wnu")))
    (with-current-buffer buf
      (insert "alpha beta gamma delta epsilon")
      (put-text-property 1 6 'w 'a)
      (put-text-property 7 11 'w 'b)
      (put-text-property 12 17 'w 'c)
      (put-text-property 18 23 'w 'd)
      (put-text-property 24 31 'w 'e)
      (let* ((ov (make-overlay 7 23))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 12)))
        (narrow-to-region 7 23)
        (goto-char (point-min))
        (forward-word)
        (forward-word)
        (let ((p1 (point))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'w)))
          (widen)
          (list p1 mp os oe k
                (buffer-string)
                (marker-position m)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_word_move_undo_insert_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "wui")))
    (with-current-buffer buf
      (insert "one two three four")
      (let* ((ov (make-overlay 1 18))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 5)))
        (undo-boundary)
        (goto-char 4)
        (insert "-X-")
        (undo-boundary)
        (goto-char (point-min))
        (forward-word)
        (let ((p1 (point))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (goto-char (point-min))
          (forward-word)
          (list p1 mp os oe s
                (point)
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_word_move_textprop_overlay_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "wtn")))
    (with-current-buffer buf
      (insert "aaa-bbb ccc-ddd eee")
      (let ((st (copy-syntax-table)))
        (modify-syntax-entry ?- "w" st)
        (put-text-property 1 7 'part 'first)
        (put-text-property 8 15 'part 'second)
        (put-text-property 16 19 'part 'third)
        (with-syntax-table st
          (let* ((ov (make-overlay 1 15))
                 (_ (overlay-put ov 'face 'bold))
                 (m (make-marker))
                 (_ (set-marker m 4)))
            (narrow-to-region 1 15)
            (goto-char (point-min))
            (forward-word)
            (let ((p1 (point))
                  (k1 (get-text-property 1 'part))
                  (mp (marker-position m))
                  (os (overlay-start ov))
                  (oe (overlay-end ov)))
              (forward-word)
              (let ((p2 (point))
                    (k2 (get-text-property 8 'part)))
                (backward-word)
                (let ((p3 (point)))
                  (widen)
                  (list p1 p2 p3 k1 k2 mp os oe))))))))
    (kill-buffer buf)))"#,
        expect,
    );
}
