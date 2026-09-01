//! Combo: cl-eieio deep undo chains + overlays + markers + textprop
//! + buflocal + narrow + undo.
//! Tests complex multi-step undo sequences with interleaved edits,
//! overlay changes, text property changes, and narrowing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_deep_undo_edit_overlay_tp_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-chain-snap ()
    ((step :initarg :step :accessor ucs-step :initform "")
     (buf-string :initarg :bs :accessor ucs-bs :initform "")
     (ov-bounds :initarg :ov :accessor ucs-ov :initform nil)
     (m-pos :initarg :m-pos :accessor ucs-mp :initform 0)
     (tp-face-at-8 :initarg :tp :accessor ucs-tp :initform nil)))
  (let* ((buf (generate-new-buffer "uc1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-uc-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil)
             (snap-state
              (lambda (step)
                (push (undo-chain-snap :step step
                                      :bs (buffer-string)
                                      :ov (list (overlay-start ov) (overlay-end ov))
                                      :m-pos (marker-position m)
                                      :tp (get-text-property 8 'face)) snaps))))
        (setq buffer-undo-list nil)
        (funcall snap-state "init")
        (goto-char 8)
        (insert "XXX")
        (undo-boundary)
        (setq my-uc-log (cons "ins@8" my-uc-log))
        (funcall snap-state "edit1")
        (put-text-property 6 12 'face 'error)
        (undo-boundary)
        (setq my-uc-log (cons "tp-error@6-12" my-uc-log))
        (funcall snap-state "tp-change")
        (overlay-put ov 'face 'bold)
        (overlay-put ov 'priority 20)
        (undo-boundary)
        (setq my-uc-log (cons "ov-face-bold" my-uc-log))
        (funcall snap-state "ov-change")
        (goto-char 15)
        (insert "YYY")
        (undo-boundary)
        (setq my-uc-log (cons "ins@15" my-uc-log))
        (funcall snap-state "edit2")
        (primitive-undo 1 buffer-undo-list)
        (funcall snap-state "undo-edit2")
        (primitive-undo 1 buffer-undo-list)
        (funcall snap-state "undo-ov")
        (primitive-undo 1 buffer-undo-list)
        (funcall snap-state "undo-tp")
        (primitive-undo 1 buffer-undo-list)
        (funcall snap-state "undo-edit1")
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ucs-step s) (ucs-mp s)
                                                (ucs-tp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S uc-log=%S"
                       results (reverse my-uc-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ucs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (overlay-get ov 'face)
              my-uc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_deep_undo_narrow_interleave() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-narrow-snap ()
    ((step :initarg :step :accessor uns-step :initform "")
     (buf-len :initarg :bl :accessor uns-bl :initform 0)
     (narrow-bounds :initarg :narrow :accessor uns-narrow :initform nil)
     (m-pos :initarg :m-pos :accessor uns-mp :initform 0)))
  (let* ((buf (generate-new-buffer "uc2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (setq-local my-un-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 18))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (undo-narrow-snap :step "init"
                               :bl (point-max)
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps)
        (goto-char 12)
        (insert "XXX")
        (undo-boundary)
        (setq my-un-log (cons "ins@12" my-un-log))
        (push (undo-narrow-snap :step "edit1"
                               :bl (point-max)
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 8 25)
          (goto-char 10)
          (insert "YY")
          (undo-boundary)
          (setq my-un-log (cons "ins-narrow@10" my-un-log))
          (push (undo-narrow-snap :step "narrow-edit"
                                 :bl (point-max)
                                 :narrow (list (point-min) (point-max))
                                 :m-pos (marker-position m)) snaps))
        (push (undo-narrow-snap :step "widen"
                               :bl (point-max)
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-narrow-snap :step "undo-narrow-edit"
                               :bl (point-max)
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-narrow-snap :step "undo-edit1"
                               :bl (point-max)
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (uns-step s) (uns-bl s)
                                                (uns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S un-log=%S"
                       results (reverse my-un-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'uns-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-un-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_deep_undo_multi_ov_evaporate_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-evap-snap ()
    ((step :initarg :step :accessor ues-step :initform "")
     (ov1-alive :initarg :a1 :accessor ues-a1 :initform nil)
     (ov2-alive :initarg :a2 :accessor ues-a2 :initform nil)
     (ov3-alive :initarg :a3 :accessor ues-a3 :initform nil)
     (m-pos :initarg :m-pos :accessor ues-mp :initform 0)))
  (let* ((buf (generate-new-buffer "uc3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (setq-local my-ue-log nil)
      (let* ((ov1 (make-overlay 6 10))
             (ov2 (make-overlay 16 25))
             (ov3 (make-overlay 26 35))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'evaporate t))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'evaporate t))
             (_ (overlay-put ov3 'face 'underline))
             (_ (overlay-put ov3 'evaporate t))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (undo-evap-snap :step "init"
                             :a1 (overlay-live-p ov1)
                             :a2 (overlay-live-p ov2)
                             :a3 (overlay-live-p ov3)
                             :m-pos (marker-position m)) snaps)
        (delete-region 6 10)
        (undo-boundary)
        (setq my-ue-log (cons "del-b" my-ue-log))
        (push (undo-evap-snap :step "del-b"
                             :a1 (overlay-live-p ov1)
                             :a2 (overlay-live-p ov2)
                             :a3 (overlay-live-p ov3)
                             :m-pos (marker-position m)) snaps)
        (delete-region 16 25)
        (undo-boundary)
        (setq my-ue-log (cons "del-de" my-ue-log))
        (push (undo-evap-snap :step "del-de"
                             :a1 (overlay-live-p ov1)
                             :a2 (overlay-live-p ov2)
                             :a3 (overlay-live-p ov3)
                             :m-pos (marker-position m)) snaps)
        (delete-region 21 30)
        (undo-boundary)
        (setq my-ue-log (cons "del-fg" my-ue-log))
        (push (undo-evap-snap :step "del-fg"
                             :a1 (overlay-live-p ov1)
                             :a2 (overlay-live-p ov2)
                             :a3 (overlay-live-p ov3)
                             :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-evap-snap :step "undo-del-fg"
                             :a1 (overlay-live-p ov1)
                             :a2 (overlay-live-p ov2)
                             :a3 (overlay-live-p ov3)
                             :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-evap-snap :step "undo-del-de"
                             :a1 (overlay-live-p ov1)
                             :a2 (overlay-live-p ov2)
                             :a3 (overlay-live-p ov3)
                             :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-evap-snap :step "undo-del-b"
                             :a1 (overlay-live-p ov1)
                             :a2 (overlay-live-p ov2)
                             :a3 (overlay-live-p ov3)
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ues-step s) (ues-a1 s)
                                                (ues-a2 s) (ues-a3 s)
                                                (ues-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ue-log=%S"
                       results (reverse my-ue-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ues-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-live-p ov1) (overlay-live-p ov2) (overlay-live-p ov3)
              my-ue-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_deep_undo_buflocal_tp_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-buflocal-snap ()
    ((step :initarg :step :accessor ubs-step :initform "")
     (buf-len :initarg :bl :accessor ubs-bl :initform 0)
     (tp-at-8 :initarg :tp :accessor ubs-tp :initform nil)
     (m-pos :initarg :m-pos :accessor ubs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "uc4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local tab-width 4)
      (setq-local my-ubl-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (undo-buflocal-snap :step "init"
                                 :bl (point-max)
                                 :tp (get-text-property 8 'face)
                                 :m-pos (marker-position m)) snaps)
        (put-text-property 6 20 'face 'error)
        (undo-boundary)
        (setq my-ubl-log (cons "tp-error@6-20" my-ubl-log))
        (push (undo-buflocal-snap :step "tp-change"
                                 :bl (point-max)
                                 :tp (get-text-property 8 'face)
                                 :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "QQQQ")
        (undo-boundary)
        (setq my-ubl-log (cons "ins@8" my-ubl-log))
        (push (undo-buflocal-snap :step "edit"
                                 :bl (point-max)
                                 :tp (get-text-property 8 'face)
                                 :m-pos (marker-position m)) snaps)
        (setq-local tab-width 8)
        (push (undo-buflocal-snap :step "buflocal-change"
                                 :bl (point-max)
                                 :tp (get-text-property 8 'face)
                                 :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-buflocal-snap :step "undo-edit"
                                 :bl (point-max)
                                 :tp (get-text-property 8 'face)
                                 :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-buflocal-snap :step "undo-tp"
                                 :bl (point-max)
                                 :tp (get-text-property 8 'face)
                                 :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ubs-step s) (ubs-bl s)
                                                (ubs-tp s) (ubs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ubl-log=%S"
                       results (reverse my-ubl-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ubs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              tab-width my-ubl-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_deep_undo_marker_advance_stay_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-mtype-snap ()
    ((step :initarg :step :accessor ums-step :initform "")
     (adv-pos :initarg :adv :accessor ums-adv :initform 0)
     (stay-pos :initarg :stay :accessor ums-stay :initform 0)
     (buf-string :initarg :bs :accessor ums-bs :initform "")))
  (let* ((buf (generate-new-buffer "uc5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-umt-log nil)
      (let* ((m-adv (set-marker (make-marker) 10))
             (m-stay (set-marker (make-marker) 10))
             (_ (set-marker-insertion-type m-adv t))
             (_ (set-marker-insertion-type m-stay nil))
             (ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (undo-mtype-snap :step "init"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (goto-char 10)
        (insert "XXX")
        (undo-boundary)
        (setq my-umt-log (cons "ins@10" my-umt-log))
        (push (undo-mtype-snap :step "edit1"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (goto-char 5)
        (insert "YYY")
        (undo-boundary)
        (setq my-umt-log (cons "ins@5" my-umt-log))
        (push (undo-mtype-snap :step "edit2"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (delete-region 8 14)
        (undo-boundary)
        (setq my-umt-log (cons "del@8-14" my-umt-log))
        (push (undo-mtype-snap :step "delete"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-mtype-snap :step "undo-del"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-mtype-snap :step "undo-edit2"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (undo-mtype-snap :step "undo-edit1"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :bs (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ums-step s) (ums-adv s)
                                                (ums-stay s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S umt-log=%S"
                       results (reverse my-umt-log)))
        (put-text-property (1- (point-max)) (point-max) 'ums-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m-adv) (marker-position m-stay)
              (marker-insertion-type m-adv) (marker-insertion-type m-stay)
              (overlay-start ov) (overlay-end ov)
              my-umt-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
