//! Combo: cl-eieio save-excursion / save-restriction / save-window-excursion
//! + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests complex nesting of save-excursion/save-restriction with EIEIO objects,
//! overlays, markers, and editing operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_save_excursion_nested_overlay_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass save-snap ()
    ((step :initarg :step :accessor svs-step :initform "")
     (point :initarg :pt :accessor svs-pt :initform 1)
     (m-pos :initarg :m-pos :accessor svs-mp :initform 0)
     (buf-string :initarg :bs :accessor svs-bs :initform "")))
  (let* ((buf (generate-new-buffer "sv1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-save-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 3)
        (push (save-snap :step "outer-init"
                        :pt (point)
                        :m-pos (marker-position m)
                        :bs (buffer-string)) snaps)
        (save-excursion
          (goto-char 8)
          (insert "XXX")
          (setq my-save-log (cons "inner1-ins@8" my-save-log))
          (push (save-snap :step "inner1"
                          :pt (point)
                          :m-pos (marker-position m)
                          :bs (buffer-string)) snaps)
          (save-excursion
            (goto-char 15)
            (insert "YYY")
            (setq my-save-log (cons "inner2-ins@15" my-save-log))
            (push (save-snap :step "inner2"
                            :pt (point)
                            :m-pos (marker-position m)
                            :bs (buffer-string)) snaps))
          (push (save-snap :step "inner1-restore"
                          :pt (point)
                          :m-pos (marker-position m)
                          :bs (buffer-string)) snaps))
        (push (save-snap :step "outer-restore"
                        :pt (point)
                        :m-pos (marker-position m)
                        :bs (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (svs-step s) (svs-pt s)
                                                (svs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S save-log=%S"
                       results (reverse my-save-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'svs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-save-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_save_restriction_nested_with_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass savenar-snap ()
    ((step :initarg :step :accessor sns-step :initform "")
     (narrow-min :initarg :nmin :accessor sns-nmin :initform 1)
     (narrow-max :initarg :nmax :accessor sns-nmax :initform 0)
     (m-pos :initarg :m-pos :accessor sns-mp :initform 0)
     (buf-string :initarg :bs :accessor sns-bs :initform "")))
  (let* ((buf (generate-new-buffer "sv2"))
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
      (setq-local my-sn-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 18))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (savenar-snap :step "init"
                           :nmin (point-min) :nmax (point-max)
                           :m-pos (marker-position m)
                           :bs (buffer-string)) snaps)
        (save-restriction
          (narrow-to-region 8 28)
          (push (savenar-snap :step "narrow1"
                             :nmin (point-min) :nmax (point-max)
                             :m-pos (marker-position m)
                             :bs (buffer-substring-no-properties
                                  (point-min) (point-max))) snaps)
          (save-excursion
            (save-restriction
              (narrow-to-region 12 22)
              (push (savenar-snap :step "narrow2"
                                 :nmin (point-min) :nmax (point-max)
                                 :m-pos (marker-position m)
                                 :bs (buffer-substring-no-properties
                                      (point-min) (point-max))) snaps)
              (goto-char 15)
              (insert "QQ")
              (setq my-sn-log (cons "ins-narrow2@15" my-sn-log))
              (push (savenar-snap :step "edit-narrow2"
                                 :nmin (point-min) :nmax (point-max)
                                 :m-pos (marker-position m)
                                 :bs (buffer-substring-no-properties
                                      (point-min) (point-max))) snaps)))
          (push (savenar-snap :step "narrow1-restore"
                             :nmin (point-min) :nmax (point-max)
                             :m-pos (marker-position m)
                             :bs (buffer-substring-no-properties
                                  (point-min) (point-max))) snaps))
        (push (savenar-snap :step "widen"
                           :nmin (point-min) :nmax (point-max)
                           :m-pos (marker-position m)
                           :bs (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sns-step s) (sns-nmin s)
                                                (sns-nmax s) (sns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S sn-log=%S"
                       results (reverse my-sn-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sns-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-sn-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_save_excursion_with_marker_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass save-mtype-snap ()
    ((step :initarg :step :accessor sms-step :initform "")
     (adv-pos :initarg :adv :accessor sms-adv :initform 0)
     (stay-pos :initarg :stay :accessor sms-stay :initform 0)
     (point :initarg :pt :accessor sms-pt :initform 1)
     (m-pos :initarg :m-pos :accessor sms-mp :initform 0)))
  (let* ((buf (generate-new-buffer "sv3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-smt-log nil)
      (let* ((m-adv (set-marker (make-marker) 10))
             (m-stay (set-marker (make-marker) 10))
             (_ (set-marker-insertion-type m-adv t))
             (_ (set-marker-insertion-type m-stay nil))
             (ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 3)
        (push (save-mtype-snap :step "init"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :pt (point)
                              :m-pos (marker-position m)) snaps)
        (save-excursion
          (goto-char 10)
          (insert "XXX")
          (setq my-smt-log (cons "ins@10" my-smt-log))
          (push (save-mtype-snap :step "inner-edit"
                                :adv (marker-position m-adv)
                                :stay (marker-position m-stay)
                                :pt (point)
                                :m-pos (marker-position m)) snaps)
          (save-excursion
            (goto-char 5)
            (insert "YYY")
            (setq my-smt-log (cons "ins@5" my-smt-log))
            (push (save-mtype-snap :step "inner-inner"
                                  :adv (marker-position m-adv)
                                  :stay (marker-position m-stay)
                                  :pt (point)
                                  :m-pos (marker-position m)) snaps)))
        (push (save-mtype-snap :step "restore"
                              :adv (marker-position m-adv)
                              :stay (marker-position m-stay)
                              :pt (point)
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sms-step s) (sms-adv s)
                                                (sms-stay s) (sms-pt s)
                                                (sms-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S smt-log=%S"
                       results (reverse my-smt-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sms-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (marker-position m-adv) (marker-position m-stay)
              (overlay-start ov) (overlay-end ov)
              my-smt-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_save_excursion_with_overlay_mod() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass save-ov-snap ()
    ((step :initarg :step :accessor sos-step :initform "")
     (ov-face :initarg :ovf :accessor sos-ovf :initform nil)
     (ov-priority :initarg :ovp :accessor sos-ovp :initform 0)
     (m-pos :initarg :m-pos :accessor sos-mp :initform 0)
     (buf-string :initarg :bs :accessor sos-bs :initform "")))
  (let* ((buf (generate-new-buffer "sv4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-so-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (save-ov-snap :step "init"
                           :ovf (overlay-get ov 'face)
                           :ovp (overlay-get ov 'priority)
                           :m-pos (marker-position m)
                           :bs (buffer-string)) snaps)
        (save-excursion
          (overlay-put ov 'face 'error)
          (overlay-put ov 'priority 100)
          (setq my-so-log (cons "ov-change-inner" my-so-log))
          (push (save-ov-snap :step "inner-ov-change"
                             :ovf (overlay-get ov 'face)
                             :ovp (overlay-get ov 'priority)
                             :m-pos (marker-position m)
                             :bs (buffer-string)) snaps)
          (goto-char 8)
          (insert "MMM")
          (setq my-so-log (cons "ins@8-inner" my-so-log))
          (push (save-ov-snap :step "inner-edit"
                             :ovf (overlay-get ov 'face)
                             :ovp (overlay-get ov 'priority)
                             :m-pos (marker-position m)
                             :bs (buffer-string)) snaps))
        (push (save-ov-snap :step "restore"
                           :ovf (overlay-get ov 'face)
                           :ovp (overlay-get ov 'priority)
                           :m-pos (marker-position m)
                           :bs (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sos-step s) (sos-ovf s)
                                                (sos-ovp s) (sos-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S so-log=%S"
                       results (reverse my-so-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sos-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (overlay-get ov 'face)
              my-so-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_save_buflocal_with_excursion_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass save-bl-snap ()
    ((step :initarg :step :accessor sbls-step :initform "")
     (tab-w :initarg :tw :accessor sbls-tw :initform 8)
     (fill-col :initarg :fc :accessor sbls-fc :initform 70)
     (narrow-bounds :initarg :narrow :accessor sbls-narrow :initform nil)
     (m-pos :initarg :m-pos :accessor sbls-mp :initform 0)))
  (let* ((buf (generate-new-buffer "sv5"))
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
      (setq-local tab-width 4)
      (setq-local fill-column 40)
      (setq-local my-sbl-log nil)
      (let* ((ov (make-overlay 6 25))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (save-bl-snap :step "init"
                           :tw tab-width
                           :fc fill-column
                           :narrow (list (point-min) (point-max))
                           :m-pos (marker-position m)) snaps)
        (save-excursion
          (setq-local tab-width 8)
          (setq-local fill-column 80)
          (setq my-sbl-log (cons "buflocal-change" my-sbl-log))
          (push (save-bl-snap :step "inner-bl"
                             :tw tab-width
                             :fc fill-column
                             :narrow (list (point-min) (point-max))
                             :m-pos (marker-position m)) snaps)
          (save-restriction
            (narrow-to-region 5 25)
            (push (save-bl-snap :step "inner-narrow"
                               :tw tab-width
                               :fc fill-column
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps)
            (goto-char 10)
            (insert "NN")
            (setq my-sbl-log (cons "ins-narrow@10" my-sbl-log))
            (push (save-bl-snap :step "inner-edit"
                               :tw tab-width
                               :fc fill-column
                               :narrow (list (point-min) (point-max))
                               :m-pos (marker-position m)) snaps)))
        (push (save-bl-snap :step "restore"
                           :tw tab-width
                           :fc fill-column
                           :narrow (list (point-min) (point-max))
                           :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sbls-step s) (sbls-tw s)
                                                (sbls-fc s) (sbls-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S sbl-log=%S"
                       results (reverse my-sbl-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sbls-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              tab-width fill-column
              my-sbl-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
