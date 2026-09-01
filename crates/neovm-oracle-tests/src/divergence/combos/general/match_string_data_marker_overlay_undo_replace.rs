//! Deep combo: match-string × match-data × marker × overlay × text-prop ×
//! undo × regex × buffer-local × narrow × replace-match × fixedcase ×
//! literal × submatch × group.
//!
//! Stresses match-string and match-data interaction: accessing match groups,
//! preserving match data across operations, and replacing with backreferences.
//! This is tricky in a Rust rewrite because match data is global state that
//! must be preserved correctly across function calls and callbacks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_match_string_groups_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Access match groups after re-search-forward; markers track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-msg")))
    (with-current-buffer buf
      (insert "alpha:100 beta:200 gamma:300")
      (put-text-property 1 10 'grp 'g1)
      (put-text-property 11 20 'grp 'g2)
      (put-text-property 21 29 'grp 'g3)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 20 t))
            (ov (make-overlay 1 29)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((matches nil))
          (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
            (push (list (match-string 0)
                        (match-string 1)
                        (match-string 2)
                        (match-beginning 0)
                        (match-end 0)
                        (match-beginning 1)
                        (match-end 1)
                        (match-beginning 2)
                        (match-end 2))
                  matches))
          (setq matches (nreverse matches))
          (let ((after (list matches
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'grp)
                             (get-text-property 11 'grp)
                             (get-text-property 21 'grp))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'grp)
                                  (get-text-property 11 'grp)
                                  (get-text-property 21 'grp))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_replace_match_backreference_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Replace with backreferences; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-rmb")))
    (with-current-buffer buf
      (insert "hello-world foo-bar baz-qux")
      (put-text-property 1 11 'word 'hello-world)
      (put-text-property 13 21 'word 'foo-bar)
      (put-text-property 23 31 'word 'baz-qux)
      (let ((m1 (copy-marker 11 nil))
            (m2 (copy-marker 21 t))
            (ov (make-overlay 1 31)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "\\([a-z]+\\)-\\([a-z]+\\)" nil t)
          (replace-match "\\2_\\1" t))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'word)
                           (get-text-property 13 'word)
                           (get-text-property 23 'word))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'word)
                                (get-text-property 13 'word)
                                (get-text-property 23 'word))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_replace_match_fixedcase_literal_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Replace with fixedcase and literal flags; markers track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-rmfl")))
    (with-current-buffer buf
      (insert "Hello World HELLO WORLD hello world")
      (put-text-property 1 6 'case 'mixed1)
      (put-text-property 7 12 'case 'upper1)
      (put-text-property 13 18 'case 'lower1)
      (put-text-property 19 24 'case 'mixed2)
      (put-text-property 25 30 'case 'upper2)
      (put-text-property 31 36 'case 'lower2)
      (let ((m1 (copy-marker 12 nil))
            (m2 (copy-marker 24 t))
            (ov (make-overlay 1 36)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "hello" nil t)
          (replace-match "GOODBYE" t t))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'case)
                                (get-text-property 7 'case)
                                (get-text-property 13 'case)
                                (get-text-property 19 'case)
                                (get-text-property 25 'case)
                                (get-text-property 31 'case))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_match_data_save_restore_across_call_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Save/restore match-data across function calls.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mdsr")))
    (with-current-buffer buf
      (insert "aaa:111 bbb:222 ccc:333")
      (put-text-property 1 8 'grp 'g1)
      (put-text-property 9 16 'grp 'g2)
      (put-text-property 17 24 'grp 'g3)
      (let ((m1 (copy-marker 8 nil))
            (m2 (copy-marker 16 t))
            (ov (make-overlay 1 24)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)")
        (let ((saved-match (match-data)))
          (goto-char 9)
          (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)")
          (let ((inner-match (list (match-string 1) (match-string 2))))
            (set-match-data saved-match)
            (let ((restored-match (list (match-string 1) (match-string 2)))
                  (after (list (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'grp)
                               (get-text-property 9 'grp)
                               (get-text-property 17 'grp))))
              (primitive-undo 1 buffer-undo-list)
              (let ((after-undo (list (buffer-string)
                                      (marker-position m1)
                                      (marker-position m2)
                                      (overlay-start ov) (overlay-end ov)
                                      (get-text-property 1 'grp)
                                      (get-text-property 9 'grp)
                                      (get-text-property 17 'grp))))
                (kill-buffer buf)
                (list inner-match restored-match after after-undo))))))))) "#,
        expect,
    );
}

#[test]
fn combo_match_data_with_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 31 40)""#]];
    // Match data in narrowed buffer; markers track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mdnar")))
    (with-current-buffer buf
      (insert "alpha:100 beta:200 gamma:300 delta:400")
      (put-text-property 1 10 'grp 'g1)
      (put-text-property 11 20 'grp 'g2)
      (put-text-property 21 30 'grp 'g3)
      (put-text-property 31 40 'grp 'g4)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 20 t))
            (m3 (copy-marker 30 nil))
            (ov (make-overlay 11 30)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 11 30)
        (goto-char (point-min))
        (let ((matches nil))
          (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
            (push (list (match-string 1) (match-string 2)
                        (match-beginning 0) (match-end 0))
                  matches))
          (setq matches (nreverse matches))
          (widen)
          (let ((after (list matches
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'grp)
                             (get-text-property 11 'grp)
                             (get-text-property 21 'grp)
                             (get-text-property 31 'grp))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (marker-position m3)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'grp)
                                  (get-text-property 11 'grp)
                                  (get-text-property 21 'grp)
                                  (get-text-property 31 'grp))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}
