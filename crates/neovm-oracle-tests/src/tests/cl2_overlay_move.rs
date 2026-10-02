//! GNU-refreshed observations for real overlay relocation.
//!
//! Populate expectations with NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.
//! These forms exercise the optimized branch even while its shipped default
//! is off, and retain GNU's attachment order and endpoint gravity cases.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

fn force_local_move_knob_before_runtime_initialization() {
    // SAFETY: nextest runs each test in its own process. Set this non-Lisp
    // configuration before the oracle harness initializes runtime workers.
    unsafe {
        std::env::set_var("NEOVM_OVERLAY_LOCAL_MOVE", "on");
    }
}

#[test]
fn oracle_prop_cl2_overlay_move_attachment_order_and_front_advance() {
    force_local_move_knob_before_runtime_initialization();
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (insert "abcdefghij")
  (let ((a (make-overlay 2 7 nil t nil))
        (b (make-overlay 2 6 nil nil t))
        (c (make-overlay 2 7 nil t t))
        (d (make-overlay 2 6 nil nil nil)) out)
    (dolist (item (list (cons a 'a) (cons b 'b) (cons c 'c) (cons d 'd)))
      (overlay-put (car item) 'tag (cdr item)))
    (let ((snapshot
           (lambda ()
             (list (mapcar (lambda (ov)
                             (list (overlay-get ov 'tag) (overlay-start ov) (overlay-end ov)))
                           (overlays-at 3))
                   (next-overlay-change 1) (next-overlay-change 3)
                   (previous-overlay-change 9)))))
      (push (funcall snapshot) out)
      (move-overlay a 2 8)
      (push (funcall snapshot) out)
      (move-overlay a 1 8)
      (move-overlay a 2 7)
      (push (funcall snapshot) out)
      (dotimes (j 8)
        (move-overlay c (+ 2 (mod j 2)) (+ 7 (mod j 2))))
      (push (funcall snapshot) out)
      (goto-char 2) (insert "X")
      (push (funcall snapshot) out)
      (goto-char 7) (insert "Y")
      (push (funcall snapshot) out)
      (nreverse out))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((((d 2 6) (c 2 7) (b 2 6) (a 2 7)) 2 6 7) (((d 2 6) (c 2 7) (b 2 6) (a 2 8)) 2 6 8) (((a 2 7) (d 2 6) (c 2 7) (b 2 6)) 2 6 7) (((a 2 7) (d 2 6) (b 2 6) (c 3 8)) 2 6 8) (((d 2 7) (b 2 7) (a 3 8)) 2 4 8) (((d 2 7) (b 2 8) (a 3 9)) 2 4 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_cl2_overlay_move_multibyte_unibyte_raw_and_narrowing() {
    force_local_move_knob_before_runtime_initialization();
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let (out)
  (dolist (text (list "aж😀中yz" (unibyte-string 65 128 255 66 254 67)
                      (concat "a" (string-as-multibyte (unibyte-string 128 255)) "ж😀z")))
    (with-temp-buffer
      (set-buffer-multibyte (multibyte-string-p text))
      (insert text)
      (let ((a (make-overlay 2 5 nil nil t)) (b (make-overlay 3 6 nil t nil)) stages)
        (overlay-put a 'tag 'a) (overlay-put b 'tag 'b)
        (let ((snapshot
               (lambda ()
                 (list (overlay-start a) (overlay-end a) (overlay-start b) (overlay-end b)
                       (mapcar (lambda (pos)
                                 (mapcar (lambda (ov) (overlay-get ov 'tag)) (overlays-at pos)))
                               '(1 2 3 4 5 6))
                       (next-overlay-change 1) (previous-overlay-change (point-max))))))
          (next-overlay-change 1)
          (goto-char 1) (insert "x")
          (goto-char 2) (delete-char 1)
          (push (funcall snapshot) stages)
          (move-overlay a 3 6)
          (push (funcall snapshot) stages)
          (move-overlay a (copy-marker 5) (copy-marker 2))
          (push (funcall snapshot) stages)
          (save-restriction
            (narrow-to-region 2 5)
            (move-overlay b -100 100)
            (push (list (point-min) (point-max) (funcall snapshot)) stages))
          (goto-char 4) (insert "q")
          (push (funcall snapshot) stages)
          (push (list (multibyte-string-p text) (string-to-list text) (nreverse stages)) out)))))
  (nreverse out))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t (97 1078 128512 20013 121 122) ((2 5 3 6 (nil (a) (a b) (a b) (b) nil) 2 6) (3 6 3 6 (nil nil (a b) (a b) (a b) nil) 3 6) (2 5 3 6 (nil (a) (a b) (a b) (b) nil) 2 6) (2 5 (2 5 1 7 ((b) (b a) (b a) (b a) (b) (b)) 2 2)) (2 6 1 8 ((b) (b a) (b a) (b a) (b a) (b)) 2 6))) (nil (65 128 255 66 254 67) ((2 5 3 6 (nil (a) (a b) (a b) (b) nil) 2 6) (3 6 3 6 (nil nil (a b) (a b) (a b) nil) 3 6) (2 5 3 6 (nil (a) (a b) (a b) (b) nil) 2 6) (2 5 (2 5 1 7 ((b) (b a) (b a) (b a) (b) (b)) 2 2)) (2 6 1 8 ((b) (b a) (b a) (b a) (b a) (b)) 2 6))) (t (97 4194176 4194303 1078 128512 122) ((2 5 3 6 (nil (a) (a b) (a b) (b) nil) 2 6) (3 6 3 6 (nil nil (a b) (a b) (a b) nil) 3 6) (2 5 3 6 (nil (a) (a b) (a b) (b) nil) 2 6) (2 5 (2 5 1 7 ((b) (b a) (b a) (b a) (b) (b)) 2 2)) (2 6 1 8 ((b) (b a) (b a) (b a) (b a) (b)) 2 6))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_cl2_overlay_move_cross_buffer_markers_detachment_and_evaporation() {
    force_local_move_knob_before_runtime_initialization();
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((a (generate-new-buffer " *cl2-move-a*"))
      (b (generate-new-buffer " *cl2-move-b*")) out)
  (unwind-protect
      (progn
        (with-current-buffer a (insert "abcdef"))
        (with-current-buffer b (insert "ж😀abcd"))
        (let ((ov (make-overlay 2 5 a))
              (wrong (with-current-buffer a (copy-marker 3)))
              (left (with-current-buffer b (copy-marker 2)))
              (right (with-current-buffer b (copy-marker 5))))
          (overlay-put ov 'payload '(kept))
          (push (condition-case err (move-overlay ov wrong right b)
                  (error (car err))) out)
          (move-overlay ov right left b)
          (push (list (eq (overlay-buffer ov) b) (overlay-start ov) (overlay-end ov)
                      (with-current-buffer a (length (overlays-in 1 (point-max))))
                      (with-current-buffer b (length (overlays-in 1 (point-max))))) out)
          (delete-overlay ov)
          (with-current-buffer a (move-overlay ov 3 6))
          (push (list (eq (overlay-buffer ov) a) (overlay-start ov) (overlay-end ov)
                      (overlay-get ov 'payload)) out)
          (overlay-put ov 'evaporate t)
          (with-current-buffer a (move-overlay ov 100 200))
          (push (list (overlay-buffer ov) (overlay-start ov) (overlay-end ov)
                      (overlay-get ov 'payload)) out))
        (nreverse out))
    (kill-buffer b) (kill-buffer a)))
"#;
    let expect =
        expect_test::expect![[r#""OK (error (t 2 5 0 1) (t 3 6 (kept)) (nil nil nil (kept)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_cl2_overlay_move_indirect_buffers_share_text_and_keep_overlay_ownership() {
    force_local_move_knob_before_runtime_initialization();
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((base (generate-new-buffer " *cl2-move-base*")) indirect out)
  (unwind-protect
      (progn
        (with-current-buffer base (insert "aж😀bcdef"))
        (setq indirect (make-indirect-buffer base " *cl2-move-indirect*" nil))
        (let ((a (make-overlay 2 5 base)) (b (make-overlay 3 7 indirect)))
          (overlay-put a 'tag 'a) (overlay-put b 'tag 'b)
          (with-current-buffer indirect
            (next-overlay-change 1)
            (save-restriction
              (narrow-to-region 3 6)
              (move-overlay b 5 2)
              (push (list (point-min) (point-max) (overlay-start b) (overlay-end b)) out)))
          (move-overlay a 3 6 indirect)
          (with-current-buffer base (goto-char 1) (insert "X"))
          (push (list (eq (overlay-buffer a) indirect) (eq (overlay-buffer b) indirect)
                      (overlay-start a) (overlay-end a) (overlay-start b) (overlay-end b)
                      (with-current-buffer base (length (overlays-in 1 (point-max))))
                      (with-current-buffer indirect
                        (mapcar (lambda (ov) (overlay-get ov 'tag)) (overlays-at 4)))) out)
          (with-current-buffer indirect (goto-char 6) (insert "Y"))
          (push (list (overlay-start a) (overlay-end a) (overlay-start b) (overlay-end b)
                      (with-current-buffer base (buffer-substring-no-properties 1 (point-max)))
                      (with-current-buffer indirect (next-overlay-change 1))) out))
        (nreverse out))
    (when (buffer-live-p indirect) (kill-buffer indirect))
    (kill-buffer base)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((3 6 2 5) (t t 4 7 3 6 0 (b a)) (4 8 3 6 \"Xaж😀bYcdef\" 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_cl2_overlay_move_sparse_leaf_boundaries_and_long_relocations() {
    force_local_move_knob_before_runtime_initialization();
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (insert (make-string 1100 ?a))
  (let ((ovs (make-vector 257 nil)) out)
    (dotimes (j 257)
      (let ((ov (make-overlay (+ 1 (* j 4)) (+ 3 (* j 4)))))
        (overlay-put ov 'tag j) (aset ovs j ov)))
    (next-overlay-change 1)
    (dolist (j '(0 15 16 31 32 64 127 128 255 256))
      (let* ((ov (aref ovs j)) (start (overlay-start ov)) (end (overlay-end ov)))
        (move-overlay ov (1+ start) (1+ end))
        (push (list j (overlay-start ov) (overlay-end ov)
                    (mapcar (lambda (item) (overlay-get item 'tag)) (overlays-at (1+ start)))
                    (next-overlay-change start) (previous-overlay-change (1+ end))) out)
        (move-overlay ov start end)))
    (move-overlay (aref ovs 64) 900 905)
    (move-overlay (aref ovs 128) 10 5)
    (move-overlay (aref ovs 256) -100 5000)
    (push (mapcar (lambda (j)
                    (let ((ov (aref ovs j)))
                      (list j (overlay-start ov) (overlay-end ov))))
                  '(64 128 256)) out)
    (nreverse out)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((0 2 4 (0) 2 2) (15 62 64 (15) 62 62) (16 66 68 (16) 66 66) (31 126 128 (31) 126 126) (32 130 132 (32) 130 130) (64 258 260 (64) 258 258) (127 510 512 (127) 510 510) (128 514 516 (128) 514 514) (255 1022 1024 (255) 1022 1022) (256 1026 1028 (256) 1026 1026) ((64 900 905) (128 5 10) (256 1 1101)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
