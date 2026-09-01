//! Combo: cl-eieio replace-regexp / replace-string + overlay interaction
//! + markers + textprop + buflocal + narrow + undo.
//! Tests complex replacement operations with overlays that have modification
//! hooks, invisible properties, and face properties during replacement.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_replace_string_overlay_marker_track() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 31 36)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass repl-snap ()
    ((step :initarg :step :accessor rs-step :initform "")
     (buf-string :initarg :bs :accessor rs-bs :initform "")
     (ov-start :initarg :ovs :accessor rs-ovs :initform 0)
     (ov-end :initarg :ove :accessor rs-ove :initform 0)
     (m-pos :initarg :m-pos :accessor rs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "rp1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "alpha-BETA-alpha-BETA-alpha-BETA")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 12 'zone 'b)
      (put-text-property 13 18 'zone 'a)
      (put-text-property 19 24 'zone 'b)
      (put-text-property 25 30 'zone 'a)
      (put-text-property 31 36 'zone 'b)
      (setq-local my-repl-log nil)
      (setq-local repl-hook-count 0)
      (let* ((ov (make-overlay 7 24))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (_ (overlay-put ov 'modification-hooks
                           (list (lambda (ov after-p beg end &optional _len)
                                   (when after-p
                                     (setq repl-hook-count
                                           (1+ repl-hook-count)))))))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (repl-snap :step "init"
                        :bs (buffer-string)
                        :ovs (overlay-start ov)
                        :ove (overlay-end ov)
                        :m-pos (marker-position m)) snaps)
        (goto-char 1)
        (replace-string "alpha" "ALPHA" nil (point-min) (point-max))
        (setq my-repl-log (cons "alpha->ALPHA" my-repl-log))
        (push (repl-snap :step "replace"
                        :bs (buffer-string)
                        :ovs (overlay-start ov)
                        :ove (overlay-end ov)
                        :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (goto-char 1)
        (replace-string "BETA" "beta" nil (point-min) (point-max))
        (setq my-repl-log (cons "BETA->beta" my-repl-log))
        (push (repl-snap :step "replace2"
                        :bs (buffer-string)
                        :ovs (overlay-start ov)
                        :ove (overlay-end ov)
                        :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (push (repl-snap :step "undo-repl2"
                        :bs (buffer-string)
                        :ovs (overlay-start ov)
                        :ove (overlay-end ov)
                        :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (repl-snap :step "undo-repl1"
                        :bs (buffer-string)
                        :ovs (overlay-start ov)
                        :ove (overlay-end ov)
                        :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rs-step s) (rs-mp s)
                                                (rs-ovs s) (rs-ove s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S repl-log=%S hooks=%d"
                       results (reverse my-repl-log) repl-hook-count))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              repl-hook-count my-repl-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_replace_regexp_narrow_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 49 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass rrepl-snap ()
    ((step :initarg :step :accessor rrs-step :initform "")
     (buf-string :initarg :bs :accessor rrs-bs :initform "")
     (narrow-bounds :initarg :narrow :accessor rrs-narrow :initform nil)
     (m-pos :initarg :m-pos :accessor rrs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "rp2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "line1-aaa-line2-bbb-line3-aaa-line4-bbb-line5-aaa")
      (put-text-property 1 6 'zone 1)
      (put-text-property 7 12 'zone 2)
      (put-text-property 13 18 'zone 3)
      (put-text-property 19 24 'zone 4)
      (put-text-property 25 30 'zone 5)
      (put-text-property 31 36 'zone 6)
      (put-text-property 37 42 'zone 7)
      (put-text-property 43 48 'zone 8)
      (put-text-property 49 54 'zone 9)
      (setq-local my-rrepl-log nil)
      (let* ((ov (make-overlay 13 36))
             (_ (overlay-put ov 'face 'italic))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (rrepl-snap :step "init"
                         :bs (buffer-string)
                         :narrow (list (point-min) (point-max))
                         :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 10 40)
          (push (rrepl-snap :step "narrow"
                           :bs (buffer-substring-no-properties
                                (point-min) (point-max))
                           :narrow (list (point-min) (point-max))
                           :m-pos (marker-position m)) snaps)
          (goto-char (point-min))
          (replace-regexp "aaa" "XXX" nil (point-min) (point-max))
          (setq my-rrepl-log (cons "aaa->XXX-narrow" my-rrepl-log))
          (push (rrepl-snap :step "replaced-narrow"
                           :bs (buffer-substring-no-properties
                                (point-min) (point-max))
                           :narrow (list (point-min) (point-max))
                           :m-pos (marker-position m)) snaps))
        (push (rrepl-snap :step "widen"
                         :bs (buffer-string)
                         :narrow (list (point-min) (point-max))
                         :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rrs-step s) (rrs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S rrepl-log=%S"
                       results (reverse my-rrepl-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rrs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-rrepl-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_replace_invisible_overlay_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass repl-inv-snap ()
    ((step :initarg :step :accessor ris-step :initform "")
     (buf-string :initarg :bs :accessor ris-bs :initform "")
     (ov-face :initarg :ovf :accessor ris-ovf :initform nil)
     (m-pos :initarg :m-pos :accessor ris-mp :initform 0)))
  (let* ((buf (generate-new-buffer "rp3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "SHOW-HIDE-SHOW-HIDE-SHOW-HIDE-SHOW")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (put-text-property 31 35 'face 'success)
      (setq-local my-ri-log nil)
      (let* ((ov1 (make-overlay 6 10))
             (ov2 (make-overlay 16 20))
             (ov3 (make-overlay 26 30))
             (_ (overlay-put ov1 'invisible 'hide-zone))
             (_ (overlay-put ov1 'face 'shadow))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov2 'invisible 'hide-zone))
             (_ (overlay-put ov2 'face 'shadow))
             (_ (overlay-put ov2 'priority 5))
             (_ (overlay-put ov3 'invisible 'hide-zone))
             (_ (overlay-put ov3 'face 'shadow))
             (_ (overlay-put ov3 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (repl-inv-snap :step "init"
                            :bs (buffer-string)
                            :ovf (overlay-get ov1 'face)
                            :m-pos (marker-position m)) snaps)
        (add-to-invisibility-spec 'hide-zone)
        (setq my-ri-log (cons "hide-zone" my-ri-log))
        (push (repl-inv-snap :step "hidden"
                            :bs (buffer-string)
                            :ovf (overlay-get ov1 'face)
                            :m-pos (marker-position m)) snaps)
        (goto-char 1)
        (replace-string "SHOW" "VIEW" nil (point-min) (point-max))
        (setq my-ri-log (cons "SHOW->VIEW" my-ri-log))
        (push (repl-inv-snap :step "replaced"
                            :bs (buffer-string)
                            :ovf (overlay-get ov1 'face)
                            :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (remove-from-invisibility-spec 'hide-zone)
        (setq my-ri-log (cons "show-zone" my-ri-log))
        (push (repl-inv-snap :step "visible"
                            :bs (buffer-string)
                            :ovf (overlay-get ov1 'face)
                            :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (repl-inv-snap :step "undo-repl"
                            :bs (buffer-string)
                            :ovf (overlay-get ov1 'face)
                            :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ris-step s) (ris-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ri-log=%S inv-spec=%S"
                       results (reverse my-ri-log)
                       buffer-invisibility-spec))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ris-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              (overlay-start ov3) (overlay-end ov3)
              buffer-invisibility-spec my-ri-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_replace_with_marker_insertion_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass repl-mtype-snap ()
    ((step :initarg :step :accessor rms-step :initform "")
     (adv-pos :initarg :adv :accessor rms-adv :initform 0)
     (stay-pos :initarg :stay :accessor rms-stay :initform 0)
     (buf-string :initarg :bs :accessor rms-bs :initform "")))
  (let* ((buf (generate-new-buffer "rp4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "target-mid-target-end-target-start-target")
      (put-text-property 1 7 'zone 'a)
      (put-text-property 8 11 'zone 'b)
      (put-text-property 12 18 'zone 'c)
      (put-text-property 19 22 'zone 'd)
      (put-text-property 23 29 'zone 'e)
      (put-text-property 30 35 'zone 'f)
      (put-text-property 36 42 'zone 'g)
      (setq-local my-rm-log nil)
      (let* ((m-adv (set-marker (make-marker) 12))
             (m-stay (set-marker (make-marker) 12))
             (_ (set-marker-insertion-type m-adv t))
             (_ (set-marker-insertion-type m-stay nil))
             (ov (make-overlay 8 22))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (repl-mtype-snap :step "init"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (goto-char 1)
        (replace-string "target" "REPLACED" nil (point-min) (point-max))
        (setq my-rm-log (cons "target->REPLACED" my-rm-log))
        (push (repl-mtype-snap :step "replace"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (push (repl-mtype-snap :step "undo"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rms-step s) (rms-adv s)
                                                (rms-stay s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S rm-log=%S"
                       results (reverse my-rm-log)))
        (put-text-property (1- (point-max)) (point-max) 'rms-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m-adv) (marker-position m-stay)
              (overlay-start ov) (overlay-end ov)
              my-rm-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_replace_multi_step_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass repl-chain-snap ()
    ((step :initarg :step :accessor rcs-step :initform "")
     (buf-string :initarg :bs :accessor rcs-bs :initform "")
     (ov-bounds :initarg :ov :accessor rcs-ov :initform nil)
     (m-pos :initarg :m-pos :accessor rcs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "rp5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "aaa-bbb-ccc-ddd-eee-fff-ggg-hhh")
      (put-text-property 1 4 'zone 'a)
      (put-text-property 5 8 'zone 'b)
      (put-text-property 9 12 'zone 'c)
      (put-text-property 13 16 'zone 'd)
      (put-text-property 17 20 'zone 'e)
      (put-text-property 21 24 'zone 'f)
      (put-text-property 25 28 'zone 'g)
      (put-text-property 29 32 'zone 'h)
      (setq-local my-rc-log nil)
      (let* ((ov (make-overlay 5 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (repl-chain-snap :step "init"
                              :bs (buffer-string)
                              :ov (list (overlay-start ov) (overlay-end ov))
                              :m-pos (marker-position m)) snaps)
        (goto-char 1)
        (replace-string "aaa" "AAA" nil (point-min) (point-max))
        (undo-boundary)
        (setq my-rc-log (cons "aaa->AAA" my-rc-log))
        (push (repl-chain-snap :step "repl1"
                              :bs (buffer-string)
                              :ov (list (overlay-start ov) (overlay-end ov))
                              :m-pos (marker-position m)) snaps)
        (goto-char 1)
        (replace-string "bbb" "BBB" nil (point-min) (point-max))
        (undo-boundary)
        (setq my-rc-log (cons "bbb->BBB" my-rc-log))
        (push (repl-chain-snap :step "repl2"
                              :bs (buffer-string)
                              :ov (list (overlay-start ov) (overlay-end ov))
                              :m-pos (marker-position m)) snaps)
        (goto-char 1)
        (replace-string "ccc" "CCC" nil (point-min) (point-max))
        (undo-boundary)
        (setq my-rc-log (cons "ccc->CCC" my-rc-log))
        (push (repl-chain-snap :step "repl3"
                              :bs (buffer-string)
                              :ov (list (overlay-start ov) (overlay-end ov))
                              :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (repl-chain-snap :step "undo-repl3"
                              :bs (buffer-string)
                              :ov (list (overlay-start ov) (overlay-end ov))
                              :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (repl-chain-snap :step "undo-repl2"
                              :bs (buffer-string)
                              :ov (list (overlay-start ov) (overlay-end ov))
                              :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (repl-chain-snap :step "undo-repl1"
                              :bs (buffer-string)
                              :ov (list (overlay-start ov) (overlay-end ov))
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rcs-step s) (rcs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S rc-log=%S"
                       results (reverse my-rc-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rcs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-rc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
