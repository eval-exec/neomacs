//! Deep combo: replace-match × re-search-forward × re-search-backward ×
//! looking-at × match-beginning × match-end × match-string ×
//! marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses regex match/replace with buffer state: re-search-forward,
//! re-search-backward, looking-at, replace-match with various options,
//! and match data preservation across operations. Regex operations are
//! tricky because they modify global match data and must interact
//! correctly with markers, overlays, text properties, and undo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_re_search_forward_backward_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 31 40)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-rsfb")))
    (with-current-buffer buf
      (insert "alpha:100 beta:200 gamma:300 delta:400")
      (put-text-property 1 10 'grp 'g1)
      (put-text-property 11 20 'grp 'g2)
      (put-text-property 21 30 'grp 'g3)
      (put-text-property 31 40 'grp 'g4)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 20 t))
            (m3 (copy-marker 30 nil))
            (ov (make-overlay 1 40)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((fwd1 (progn (re-search-forward "[a-z]+:[0-9]+" nil t)
                           (list (match-string 0) (match-beginning 0) (match-end 0))))
              (fwd2 (progn (re-search-forward "[a-z]+:[0-9]+" nil t)
                           (list (match-string 0) (match-beginning 0) (match-end 0)))))
          (goto-char (point-max))
          (let ((bwd1 (progn (re-search-backward "[a-z]+:[0-9]+" nil t)
                             (list (match-string 0) (match-beginning 0) (match-end 0))))
                (bwd2 (progn (re-search-backward "[a-z]+:[0-9]+" nil t)
                             (list (match-string 0) (match-beginning 0) (match-end 0)))))
            (goto-char 20)
            (insert "-INSERTED-")
            (let ((after (list (buffer-string)
                               fwd1 fwd2 bwd1 bwd2
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
                (list after restored)))))))))) "#,
        expect,
    );
}

#[test]
fn combo_replace_match_fixedcase_literal_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
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
        ;; Replace with fixedcase=nil, literal=t (preserve case)
        (while (re-search-forward "hello" nil t)
          (replace-match "goodbye" nil t))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'case)
                           (get-text-property 7 'case)
                           (get-text-property 13 'case)
                           (get-text-property 19 'case)
                           (get-text-property 25 'case)
                           (get-text-property 31 'case))))
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
fn combo_replace_match_backreference_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-rmbr")))
    (with-current-buffer buf
      (insert "item-1:aaa item-2:bbb item-3:ccc")
      (put-text-property 1 11 'item 'one)
      (put-text-property 12 22 'item 'two)
      (put-text-property 23 33 'item 'three)
      (let ((m1 (copy-marker 11 nil))
            (m2 (copy-marker 22 t))
            (ov (make-overlay 1 33)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        ;; Replace with backreference
        (while (re-search-forward "item-\\([0-9]+\\):\\([a-z]+\\)" nil t)
          (replace-match "ENTRY-\\1=\\2" t))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'item)
                           (get-text-property 12 'item)
                           (get-text-property 23 'item))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'item)
                                (get-text-property 12 'item)
                                (get-text-property 23 'item))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_re_search_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable matches)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-rsnar")))
    (with-current-buffer buf
      (insert "alpha:100 beta:200 gamma:300 delta:400 epsilon:500")
      (put-text-property 1 10 'grp 'g1)
      (put-text-property 11 20 'grp 'g2)
      (put-text-property 21 30 'grp 'g3)
      (put-text-property 31 40 'grp 'g4)
      (put-text-property 41 51 'grp 'g5)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 20 t))
            (m3 (copy-marker 30 nil))
            (ov (make-overlay 11 40)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 11 40)
        (goto-char (point-min))
        (let ((matches nil))
          (while (re-search-forward "[a-z]+:[0-9]+" nil t)
            (push (list (match-string 0) (match-beginning 0) (match-end 0))
                  matches))
          (setq matches (nreverse matches)))
        (widen)
        (goto-char 20)
        (insert "-INSERTED-")
        (let ((after (list (buffer-string)
                           matches
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 11 'grp)
                           (get-text-property 21 'grp)
                           (get-text-property 31 'grp)
                           (get-text-property 41 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'grp)
                                (get-text-property 11 'grp)
                                (get-text-property 21 'grp)
                                (get-text-property 31 'grp)
                                (get-text-property 41 'grp))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_looking_at_replace_match_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-larm")))
    (with-current-buffer buf
      (insert "aaa:111 bbb:222 ccc:333 ddd:444")
      (put-text-property 1 8 'grp 'g1)
      (put-text-property 9 16 'grp 'g2)
      (put-text-property 17 24 'grp 'g3)
      (put-text-property 25 32 'grp 'g4)
      (let ((m1 (copy-marker 8 nil))
            (m2 (copy-marker 16 t))
            (ov (make-overlay 1 32)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((la1 (looking-at "[a-z]+:[0-9]+")))
          (re-search-forward "[a-z]+:[0-9]+" nil t)
          (replace-match "REPLACED" t)
          (let ((la2 (looking-at " [a-z]+:[0-9]+")))
            (re-search-forward "[a-z]+:[0-9]+" nil t)
            (replace-match "REPLACED" t)
            (let ((after (list (buffer-string)
                               la1 la2
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'grp)
                               (get-text-property 9 'grp)
                               (get-text-property 17 'grp)
                               (get-text-property 25 'grp))))
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'grp)
                                    (get-text-property 9 'grp)
                                    (get-text-property 17 'grp)
                                    (get-text-property 25 'grp))))
                (kill-buffer buf)
                (list after restored)))))))))) "#,
        expect,
    );
}
