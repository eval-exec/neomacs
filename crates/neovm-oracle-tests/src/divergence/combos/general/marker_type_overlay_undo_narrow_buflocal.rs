//! Deep combo: marker insertion-type × overlay × undo × narrow × buffer-local
//! × regex-replace × buffer-switch × text-prop × evaporate.
//!
//! Stresses the full edit pipeline: markers with different insertion types
//! interact with overlays, text properties, narrowing, undo, and cross-buffer
//! operations in a single evaluation chain.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_marker_types_overlay_undo_narrow_regex_bufswitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf-a (generate-new-buffer " combo-mtoa"))
        (buf-b (generate-new-buffer " combo-mtob")))
    (with-current-buffer buf-a
      (insert "AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH")
      (let ((m-nil (copy-marker 5 nil))
            (m-t   (copy-marker 13 t))
            (m-nil2 (copy-marker 21 nil))
            (ov1 (make-overlay 1 10))
            (ov2 (make-overlay 14 22))
            (ov3 (make-overlay 23 32)))
        (overlay-put ov1 'zone 'alpha)
        (overlay-put ov2 'zone 'beta)
        (overlay-put ov3 'zone 'gamma)
        (overlay-put ov3 'evaporate t)
        (put-text-property 1 4 'sect 'a)
        (put-text-property 5 8 'sect 'b)
        (put-text-property 9 12 'sect 'c)
        (put-text-property 13 16 'sect 'd)
        (put-text-property 17 20 'sect 'e)
        (put-text-property 21 24 'sect 'f)
        (put-text-property 25 28 'sect 'g)
        (put-text-property 29 32 'sect 'h)
        (undo-boundary)
        (narrow-to-region 5 28)
        (goto-char (point-min))
        (insert "XX")
        (undo-boundary)
        (goto-char 1)
        (re-search-forward "CCC" nil t)
        (replace-match "ZZZZ")
        (undo-boundary)
        (let* ((narrowed (buffer-string))
               (m-nil-pos (marker-position m-nil))
               (m-t-pos   (marker-position m-t))
               (m-nil2-pos (marker-position m-nil2))
               (ov1-se (list (overlay-start ov1) (overlay-end ov1)))
               (ov2-se (list (overlay-start ov2) (overlay-end ov2)))
               (ov3-alive (and (overlay-start ov3) t))
               (sect-at-1 (get-text-property 1 'sect))
               (sect-at-5 (get-text-property 5 'sect)))
          (primitive-undo 2 buffer-undo-list)
          (widen)
          (let ((restored (buffer-string))
                (m-nil-restored (marker-position m-nil))
                (m-t-restored   (marker-position m-t))
                (m-nil2-restored (marker-position m-nil2)))
            (with-current-buffer buf-b
              (insert narrowed)
              (let ((b-sect (get-text-property 1 'sect)))
                (kill-buffer buf-a)
                (kill-buffer buf-b)
                (list narrowed m-nil-pos m-t-pos m-nil2-pos
                      ov1-se ov2-se ov3-alive
                      sect-at-1 sect-at-5
                      restored m-nil-restored m-t-restored m-nil2-restored
                      b-sect))))))))) "#,
        expect,
    );
}

#[test]
fn combo_marker_types_multi_insert_delete_undo_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"AAAA-TEXTAAAAAAAAAAAAAAAAAAA\" 0 4 (half left) 9 14 (half left) 14 28 (half right)) 5 10 15 20 25 left ((1 4 0) (5 5 1) (5 12 2) (13 16 3) (17 20 4) (21 24 5) (25 28 6))) (#(\"AAAAAAAAAINSERTED-TEXTAAAAAAAAAAAAAAAAAAA\" 0 4 (half left) 4 9 (half left) 22 27 (half left) 27 41 (half right)) 5 23 28 33 38 left nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAAAAAAAAAAAAAAAAAAAAAAAAAA")
  (let ((m1 (copy-marker 5 nil))
        (m2 (copy-marker 10 t))
        (m3 (copy-marker 15 nil))
        (m4 (copy-marker 20 t))
        (m5 (copy-marker 25 nil))
        (ovs nil))
    (dotimes (i 7)
      (let ((ov (make-overlay (+ 1 (* i 4)) (+ 4 (* i 4)))))
        (overlay-put ov 'idx i)
        (when (= (mod i 2) 0)
          (overlay-put ov 'evaporate t))
        (push ov ovs)))
    (put-text-property 1 15 'half 'left)
    (put-text-property 15 29 'half 'right)
    (undo-boundary)
    (goto-char 10)
    (insert "INSERTED-TEXT")
    (undo-boundary)
    (delete-region 5 18)
    (undo-boundary)
    (let ((state-after-delete
            (list (buffer-string)
                  (marker-position m1) (marker-position m2)
                  (marker-position m3) (marker-position m4)
                  (marker-position m5)
                  (get-text-property 1 'half)
                  (mapcar (lambda (ov)
                            (and (overlay-start ov)
                                 (list (overlay-start ov)
                                       (overlay-end ov)
                                       (overlay-get ov 'idx))))
                          (nreverse ovs)))))
      (primitive-undo 2 buffer-undo-list)
      (let ((state-after-undo
              (list (buffer-string)
                    (marker-position m1) (marker-position m2)
                    (marker-position m3) (marker-position m4)
                    (marker-position m5)
                    (get-text-property 1 'half)
                    (get-text-property 16 'half))))
        (list state-after-delete state-after-undo))))) "#,
        expect,
    );
}

#[test]
fn combo_marker_types_narrow_regex_match_data_prop_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((#(\"beta\" 0 4 (word beta)) #(\"200\" 0 2 (word beta)) 11 19) (#(\"gamma\" 0 5 (word gamma)) #(\"300\" 0 1 (word gamma) 2 3 (word delta)) 20 29)) (1 10 19 28 37) (t nil t nil t) (10 36) alpha epsilon)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "alpha:100 beta:200 gamma:300 delta:400 epsilon:500")
  (let ((m-alpha (copy-marker 1 t))
        (m-beta  (copy-marker 10 nil))
        (m-gamma (copy-marker 19 t))
        (m-delta (copy-marker 28 nil))
        (m-eps   (copy-marker 37 t)))
    (put-text-property 1 9 'word 'alpha)
    (put-text-property 10 18 'word 'beta)
    (put-text-property 19 27 'word 'gamma)
    (put-text-property 28 36 'word 'delta)
    (put-text-property 37 46 'word 'epsilon)
    (let ((ov (make-overlay 10 36)))
      (overlay-put ov 'scope 'middle)
      (undo-boundary)
      (narrow-to-region 10 36)
      (goto-char (point-min))
      (let ((results nil))
        (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
          (push (list (match-string 1)
                      (match-string 2)
                      (match-beginning 0)
                      (match-end 0))
                results))
        (setq results (nreverse results))
        (widen)
        (let ((marker-positions
                (list (marker-position m-alpha)
                      (marker-position m-beta)
                      (marker-position m-gamma)
                      (marker-position m-delta)
                      (marker-position m-eps)))
              (marker-types
                (list (marker-insertion-type m-alpha)
                      (marker-insertion-type m-beta)
                      (marker-insertion-type m-gamma)
                      (marker-insertion-type m-delta)
                      (marker-insertion-type m-eps)))
              (overlay-range
                (list (overlay-start ov) (overlay-end ov)))
              (prop-at-start (get-text-property 1 'word))
              (prop-at-end (get-text-property 37 'word)))
          (list results marker-positions marker-types
                overlay-range prop-at-start prop-at-end)))))) "#,
        expect,
    );
}

#[test]
fn combo_marker_types_cross_buffer_copy_props_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((src (generate-new-buffer " combo-cpsrc"))
        (dst (generate-new-buffer " combo-cpdst")))
    (with-current-buffer src
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'tag 'a)
      (put-text-property 6 10 'tag 'b)
      (put-text-property 11 15 'tag 'c)
      (put-text-property 16 20 'tag 'd)
      (let ((ov (make-overlay 6 15)))
        (overlay-put ov 'region 'middle)
        (let ((m1 (copy-marker 5 nil))
              (m2 (copy-marker 10 t))
              (text (buffer-substring 1 20)))
          (with-current-buffer dst
            (insert text)
            (let ((dst-tag-1 (get-text-property 1 'tag))
                  (dst-tag-6 (get-text-property 6 'tag))
                  (dst-tag-11 (get-text-property 11 'tag))
                  (dst-tag-16 (get-text-property 16 'tag)))
              (undo-boundary)
              (goto-char 10)
              (insert "XXXX")
              (let ((src-m1 (marker-position m1))
                    (src-m2 (marker-position m2))
                    (dst-after-insert (buffer-string)))
                (primitive-undo 1 buffer-undo-list)
                (let ((dst-after-undo (buffer-string)))
                  (kill-buffer src)
                  (kill-buffer dst)
                  (list dst-tag-1 dst-tag-6 dst-tag-11 dst-tag-16
                        src-m1 src-m2
                        dst-after-insert dst-after-undo))))))))) "#,
        expect,
    );
}

#[test]
fn combo_marker_types_let_binding_buffer_local_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq combo-global 'default)
  (let ((buf (generate-new-buffer " combo-bl")))
    (with-current-buffer buf
      (insert "12345678901234567890")
      (make-local-variable 'combo-global)
      (setq combo-global 'buffer-local)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (put-text-property 1 10 'half 'left)
        (put-text-property 11 21 'half 'right)
        (undo-boundary)
        (let ((combo-global 'let-bound))
          (goto-char 10)
          (insert "INSERT")
          (let ((in-let (list combo-global
                              (marker-position m1)
                              (marker-position m2)
                              (get-text-property 1 'half)
                              (overlay-start ov)
                              (overlay-end ov))))
            (undo-boundary)
            (delete-region 5 20)
            (let ((after-delete (list combo-global
                                      (marker-position m1)
                                      (marker-position m2)
                                      (buffer-string))))
              (primitive-undo 2 buffer-undo-list)
              (let ((after-undo (list combo-global
                                      (marker-position m1)
                                      (marker-position m2)
                                      (buffer-string)
                                      (get-text-property 1 'half)
                                      (get-text-property 11 'half))))
                (kill-buffer buf)
                (list in-let after-delete after-undo
                      (default-value 'combo-global))))))))) "#,
        expect,
    );
}
