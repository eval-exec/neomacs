//! Deep combo: buffer-local × kill-buffer × indirect-buffer × undo ×
//! text-prop × marker × overlay evaporate × regex × narrow.
//!
//! Stresses buffer lifecycle: buffer-local variables in killed buffers,
//! indirect buffer sharing, overlay evaporation on deletion, and undo
//! across buffer boundaries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_buflocal_indirect_kill_undo_marker_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq bl-global 'outside)
  (let ((base (generate-new-buffer " combo-base")))
    (with-current-buffer base
      (insert "AAAAAAAAAAAAAAAAAAAAAAAAAAAA")
      (make-local-variable 'bl-global)
      (setq bl-global 'in-base)
      (put-text-property 1 10 'zone 'first)
      (put-text-property 11 20 'zone 'second)
      (put-text-property 21 29 'zone 'third)
      (let ((ov1 (make-overlay 1 10))
            (ov2 (make-overlay 15 25))
            (m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t)))
        (overlay-put ov1 'kind 'head)
        (overlay-put ov2 'kind 'body)
        (overlay-put ov2 'evaporate t)
        (let ((ind (make-indirect-buffer base " combo-ind")))
          (with-current-buffer ind
            (let ((ind-bl bl-global)
                  (ind-zone-1 (get-text-property 1 'zone))
                  (ind-zone-15 (get-text-property 15 'zone)))
              (make-local-variable 'bl-global)
              (setq bl-global 'in-indirect)
              (undo-boundary)
              (delete-region 5 20)
              (let ((after-del (list (buffer-string)
                                     bl-global
                                     (marker-position m1)
                                     (marker-position m2)
                                     (and (overlay-start ov1) t)
                                     (and (overlay-start ov2) t)
                                     (get-text-property 1 'zone))))
                (primitive-undo 1 buffer-undo-list)
                (let ((after-undo (list (buffer-string)
                                        bl-global
                                        (marker-position m1)
                                        (marker-position m2)
                                        (and (overlay-start ov1) t)
                                        (and (overlay-start ov2) t)
                                        (get-text-property 15 'zone))))
                  (kill-buffer ind)
                  (let ((base-after-kill (list (with-current-buffer base bl-global)
                                               (with-current-buffer base
                                                 (get-text-property 1 'zone)))))
                    (kill-buffer base)
                    (list ind-bl ind-zone-1 ind-zone-15
                          after-del after-undo base-after-kill
                          bl-global)))))))))) "#,
        expect,
    );
}

#[test]
fn combo_buflocal_multi_buffer_switch_undo_prop_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq switch-var 'global)
  (let ((b1 (generate-new-buffer " combo-sw1"))
        (b2 (generate-new-buffer " combo-sw2"))
        (b3 (generate-new-buffer " combo-sw3")))
    (with-current-buffer b1
      (insert "B1-AAAAAAAA-B1")
      (make-local-variable 'switch-var)
      (setq switch-var 'buf1)
      (put-text-property 1 4 'src 'b1))
    (with-current-buffer b2
      (insert "B2-BBBBBBBBB-B2")
      (make-local-variable 'switch-var)
      (setq switch-var 'buf2)
      (put-text-property 1 4 'src 'b2))
    (with-current-buffer b3
      (insert "B3-CCCCCCCCC-B3")
      (make-local-variable 'switch-var)
      (setq switch-var 'buf3)
      (put-text-property 1 4 'src 'b3))
    (let ((results nil))
      ;; Switch to b1, edit, record state
      (with-current-buffer b1
        (let ((m (copy-marker 5 nil)))
          (undo-boundary)
          (goto-char 5)
          (insert "X")
          (push (list switch-var
                      (marker-position m)
                      (buffer-string)
                      (get-text-property 1 'src))
                results)
          (primitive-undo 1 buffer-undo-list)
          (push (list switch-var
                      (marker-position m)
                      (buffer-string))
                results)))
      ;; Switch to b2, edit, record state
      (with-current-buffer b2
        (let ((m (copy-marker 5 t)))
          (undo-boundary)
          (goto-char 5)
          (insert "Y")
          (push (list switch-var
                      (marker-position m)
                      (buffer-string)
                      (get-text-property 1 'src))
                results)
          (primitive-undo 1 buffer-undo-list)
          (push (list switch-var
                      (marker-position m)
                      (buffer-string))
                results)))
      ;; Check b3 untouched
      (with-current-buffer b3
        (push (list switch-var (buffer-string)) results))
      (kill-buffer b1)
      (kill-buffer b2)
      (kill-buffer b3)
      (list (nreverse results) switch-var)))) "#,
        expect,
    );
}

#[test]
fn combo_buflocal_indirect_shared_textprop_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((base (generate-new-buffer " combo-shbase")))
    (with-current-buffer base
      (insert "SHARED-TEXT-CONTENT-HERE")
      (put-text-property 1 7 'kind 'shared)
      (put-text-property 8 12 'kind 'text)
      (put-text-property 13 20 'kind 'content)
      (put-text-property 21 25 'kind 'here))
    (let ((ind1 (make-indirect-buffer base " combo-shind1"))
          (ind2 (make-indirect-buffer base " combo-shind2")))
      ;; Edit via ind1, check in ind2
      (with-current-buffer ind1
        (let ((m (copy-marker 8 t)))
          (undo-boundary)
          (goto-char 8)
          (insert "INSERTED-")
          (let ((ind2-sees (with-current-buffer ind2
                             (buffer-string)))
                (ind2-prop (with-current-buffer ind2
                             (get-text-property 1 'kind)))
                (m-pos (marker-position m)))
            (primitive-undo 1 buffer-undo-list)
            (let ((ind2-after-undo (with-current-buffer ind2
                                     (buffer-string)))
                  (ind2-prop-undo (with-current-buffer ind2
                                     (get-text-property 8 'kind)))
                  (m-after (marker-position m)))
              (kill-buffer ind1)
              (kill-buffer ind2)
              (kill-buffer base)
              (list ind2-sees ind2-prop m-pos
                    ind2-after-undo ind2-prop-undo m-after))))))) "#,
        expect,
    );
}

#[test]
fn combo_buflocal_kill_restore_default_overlay_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq kill-test-var 'original)
  (let ((buf (generate-new-buffer " combo-kill")))
    (with-current-buffer buf
      (insert "EEEEEEEEEEEEEEEEEEEEEEEE")
      (make-local-variable 'kill-test-var)
      (setq kill-test-var 'local-value)
      (let ((ov1 (make-overlay 1 8))
            (ov2 (make-overlay 9 16))
            (ov3 (make-overlay 17 24)))
        (overlay-put ov1 'kind 'first)
        (overlay-put ov2 'kind 'second)
        (overlay-put ov3 'kind 'third)
        (overlay-put ov3 'evaporate t)
        (put-text-property 1 9 'sect 'head)
        (put-text-property 9 17 'sect 'body)
        (put-text-property 17 25 'sect 'tail)
        (let ((pre-kill (list kill-test-var
                              (get-text-property 1 'sect)
                              (overlay-get ov1 'kind)
                              (overlay-get ov3 'kind))))
          (kill-buffer buf)
          (list pre-kill kill-test-var))))) "#,
        expect,
    );
}

#[test]
fn combo_buflocal_narrow_regex_replace_undo_marker_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq chain-var 'global)
  (let ((buf (generate-new-buffer " combo-chain")))
    (with-current-buffer buf
      (insert "item-1:alpha item-2:beta item-3:gamma item-4:delta")
      (make-local-variable 'chain-var)
      (setq chain-var 'buffer-val)
      (put-text-property 1 12 'item 'one)
      (put-text-property 13 24 'item 'two)
      (put-text-property 25 37 'item 'three)
      (put-text-property 38 50 'item 'four)
      (let ((m1 (copy-marker 8 nil))
            (m2 (copy-marker 20 t))
            (m3 (copy-marker 32 nil))
            (ov (make-overlay 13 37)))
        (overlay-put ov 'scope 'middle)
        (undo-boundary)
        (narrow-to-region 13 37)
        (goto-char (point-min))
        (let ((matches nil))
          (while (re-search-forward "item-\\([0-9]+\\):\\([a-z]+\\)" nil t)
            (push (list (match-string 1) (match-string 2)
                        (match-beginning 0) (match-end 0))
                  matches))
          (setq matches (nreverse matches))
          (undo-boundary)
          (goto-char (point-min))
          (while (re-search-forward "item-" nil t)
            (replace-match "ENTRY-"))
          (let ((narrowed (buffer-string))
                (chain-in-narrow chain-var))
            (widen)
            (let ((m1-pos (marker-position m1))
                  (m2-pos (marker-position m2))
                  (m3-pos (marker-position m3))
                  (full (buffer-string))
                  (ov-range (list (overlay-start ov) (overlay-end ov)))
                  (prop-1 (get-text-property 1 'item))
                  (prop-13 (get-text-property 13 'item)))
              (primitive-undo 2 buffer-undo-list)
              (let ((restored (buffer-string))
                    (m1-after (marker-position m1))
                    (m2-after (marker-position m2))
                    (m3-after (marker-position m3))
                    (prop-1-restored (get-text-property 1 'item))
                    (prop-13-restored (get-text-property 13 'item)))
                (kill-buffer buf)
                (list matches chain-in-narrow narrowed
                      m1-pos m2-pos m3-pos full ov-range
                      prop-1 prop-13
                      restored m1-after m2-after m3-after
                      prop-1-restored prop-13-restored))))))))) "#,
        expect,
    );
}
