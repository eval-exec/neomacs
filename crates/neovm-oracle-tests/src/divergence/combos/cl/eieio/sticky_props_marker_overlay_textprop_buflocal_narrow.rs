//! Combo: cl-eieio rear-nonsticky / front-sticky / insert-behind
//! + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests text property stickiness, rear-nonsticky, front-sticky
//! behavior with complex editing operations and EIEIO objects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_sticky_prop_insert_at_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass sticky-snap ()
    ((step :initarg :step :accessor sts-step :initform "")
     (face-before :initarg :fb :accessor sts-fb :initform nil)
     (face-at :initarg :fa :accessor sts-fa :initform nil)
     (face-after :initarg :ft :accessor sts-ft :initform nil)
     (m-pos :initarg :m-pos :accessor sts-mp :initform 0)))
  (let* ((buf (generate-new-buffer "sk1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (setq-local my-sk-log nil)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 8))
             (results nil)
             (snap-faces
              (lambda ()
                (list (get-text-property 5 'face)
                      (get-text-property 6 'face)
                      (get-text-property 8 'face)
                      (get-text-property 10 'face)
                      (get-text-property 11 'face)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (put-text-property 1 5 'face 'italic)
        (put-text-property 6 10 'face 'underline)
        (put-text-property 11 15 'face 'default)
        (put-text-property 5 6 'rear-nonsticky t)
        (setq my-sk-log (cons "setup-nonsticky" my-sk-log))
        (push (sticky-snap :step "init"
                          :fb (get-text-property 5 'face)
                          :fa (get-text-property 6 'face)
                          :ft (get-text-property 10 'face)
                          :m-pos (marker-position m)) snaps)
        (goto-char 6)
        (insert "XX")
        (setq my-sk-log (cons "ins@6" my-sk-log))
        (push (sticky-snap :step "ins@boundary"
                          :fb (get-text-property 5 'face)
                          :fa (get-text-property 6 'face)
                          :ft (get-text-property 10 'face)
                          :m-pos (marker-position m)) snaps)
        (goto-char 10)
        (insert "YY")
        (setq my-sk-log (cons "ins@10" my-sk-log))
        (push (sticky-snap :step "ins@end"
                          :fb (get-text-property 5 'face)
                          :fa (get-text-property 6 'face)
                          :ft (get-text-property 12 'face)
                          :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sts-step s) (sts-fb s)
                                                (sts-fa s) (sts-ft s)
                                                (sts-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S sk-log=%S faces=%S"
                       results (reverse my-sk-log) (funcall snap-faces)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sts-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-sk-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_sticky_front_sticky_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass front-sticky-snap ()
    ((step :initarg :step :accessor fss-step :initform "")
     (face-at-5 :initarg :f5 :accessor fss-f5 :initform nil)
     (face-at-6 :initarg :f6 :accessor fss-f6 :initform nil)
     (m-pos :initarg :m-pos :accessor fss-mp :initform 0)))
  (let* ((buf (generate-new-buffer "sk2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (setq-local my-fs-log nil)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (put-text-property 1 5 'face 'italic)
        (put-text-property 6 10 'face 'underline)
        (put-text-property 11 15 'face 'default)
        (put-text-property 16 20 'face 'highlight)
        (put-text-property 6 7 'front-sticky t)
        (setq my-fs-log (cons "front-sticky@6" my-fs-log))
        (push (front-sticky-snap :step "init"
                                :f5 (get-text-property 5 'face)
                                :f6 (get-text-property 6 'face)
                                :m-pos (marker-position m)) snaps)
        (goto-char 6)
        (insert "XX")
        (setq my-fs-log (cons "ins@6" my-fs-log))
        (push (front-sticky-snap :step "ins@6"
                                :f5 (get-text-property 5 'face)
                                :f6 (get-text-property 6 'face)
                                :m-pos (marker-position m)) snaps)
        (goto-char 11)
        (insert "YY")
        (setq my-fs-log (cons "ins@11" my-fs-log))
        (push (front-sticky-snap :step "ins@11"
                                :f5 (get-text-property 5 'face)
                                :f6 (get-text-property 6 'face)
                                :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 4 18)
          (goto-char 8)
          (insert "ZZ")
          (setq my-fs-log (cons "ins-narrow@8" my-fs-log))
          (push (front-sticky-snap :step "narrow-edit"
                                  :f5 (get-text-property 5 'face)
                                  :f6 (get-text-property 6 'face)
                                  :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fss-step s) (fss-f5 s)
                                                (fss-f6 s) (fss-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S fs-log=%S"
                       results (reverse my-fs-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fss-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-fs-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_sticky_multiple_boundaries_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 29 29)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass multi-bnd-snap ()
    ((step :initarg :step :accessor mbs-step :initform "")
     (faces :initarg :faces :accessor mbs-faces :initform nil)
     (m-pos :initarg :m-pos :accessor mbs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "sk3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (setq-local my-mb-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 10))
             (m (set-marker (make-marker) 10))
             (results nil)
             (snap-all-faces
              (lambda ()
                (let ((faces nil))
                  (dotimes (i 30)
                    (push (get-text-property (1+ i) 'face) faces))
                  (reverse faces)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (put-text-property 1 5 'face 'bold)
        (put-text-property 6 10 'face 'italic)
        (put-text-property 11 15 'face 'underline)
        (put-text-property 16 20 'face 'default)
        (put-text-property 21 25 'face 'highlight)
        (put-text-property 26 30 'face 'error)
        (put-text-property 5 6 'rear-nonsticky t)
        (put-text-property 10 11 'rear-nonsticky t)
        (put-text-property 15 16 'rear-nonsticky t)
        (put-text-property 20 21 'rear-nonsticky t)
        (setq my-mb-log (cons "setup-nonsticky" my-mb-log))
        (push (multi-bnd-snap :step "init"
                             :faces (funcall snap-all-faces)
                             :m-pos (marker-position m)) snaps)
        (goto-char 6)
        (insert "XX")
        (setq my-mb-log (cons "ins@6" my-mb-log))
        (push (multi-bnd-snap :step "ins@6"
                             :faces (funcall snap-all-faces)
                             :m-pos (marker-position m)) snaps)
        (goto-char 15)
        (insert "YY")
        (setq my-mb-log (cons "ins@15" my-mb-log))
        (push (multi-bnd-snap :step "ins@15"
                             :faces (funcall snap-all-faces)
                             :m-pos (marker-position m)) snaps)
        (delete-region 10 16)
        (setq my-mb-log (cons "del@10-16" my-mb-log))
        (push (multi-bnd-snap :step "delete"
                             :faces (funcall snap-all-faces)
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mbs-step s) (length (mbs-faces s))
                                                (mbs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S mb-log=%S"
                       results (reverse my-mb-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mbs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-mb-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_sticky_undo_restore_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass sticky-undo-snap ()
    ((step :initarg :step :accessor sus-step :initform "")
     (face-at-6 :initarg :f6 :accessor sus-f6 :initform nil)
     (face-at-11 :initarg :f11 :accessor sus-f11 :initform nil)
     (m-pos :initarg :m-pos :accessor sus-mp :initform 0)))
  (let* ((buf (generate-new-buffer "sk4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (setq-local my-su-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (put-text-property 1 5 'face 'italic)
        (put-text-property 6 10 'face 'underline)
        (put-text-property 11 15 'face 'default)
        (put-text-property 16 20 'face 'highlight)
        (put-text-property 5 6 'rear-nonsticky t)
        (put-text-property 10 11 'rear-nonsticky t)
        (push (sticky-undo-snap :step "init"
                               :f6 (get-text-property 6 'face)
                               :f11 (get-text-property 11 'face)
                               :m-pos (marker-position m)) snaps)
        (goto-char 6)
        (insert "XXX")
        (undo-boundary)
        (setq my-su-log (cons "ins@6" my-su-log))
        (push (sticky-undo-snap :step "edit"
                               :f6 (get-text-property 6 'face)
                               :f11 (get-text-property 11 'face)
                               :m-pos (marker-position m)) snaps)
        (put-text-property 6 10 'face 'error)
        (undo-boundary)
        (setq my-su-log (cons "face-change" my-su-log))
        (push (sticky-undo-snap :step "face-change"
                               :f6 (get-text-property 6 'face)
                               :f11 (get-text-property 11 'face)
                               :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (sticky-undo-snap :step "undo-face"
                               :f6 (get-text-property 6 'face)
                               :f11 (get-text-property 11 'face)
                               :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (sticky-undo-snap :step "undo-edit"
                               :f6 (get-text-property 6 'face)
                               :f11 (get-text-property 11 'face)
                               :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sus-step s) (sus-f6 s)
                                                (sus-f11 s) (sus-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S su-log=%S"
                       results (reverse my-su-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sus-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-su-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_sticky_overlay_insert_in_front_behind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass insert-type-snap ()
    ((step :initarg :step :accessor its-step :initform "")
     (face-at-5 :initarg :f5 :accessor its-f5 :initform nil)
     (face-at-6 :initarg :f6 :accessor its-f6 :initform nil)
     (face-at-7 :initarg :f7 :accessor its-f7 :initform nil)
     (m-pos :initarg :m-pos :accessor its-mp :initform 0)))
  (let* ((buf (generate-new-buffer "sk5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (setq-local my-it-log nil)
      (let* ((ov1 (make-overlay 5 6 nil t nil))
             (ov2 (make-overlay 6 7 nil nil t))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 10))
             (m (set-marker (make-marker) 6))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (put-text-property 1 5 'face 'error)
        (put-text-property 6 10 'face 'underline)
        (put-text-property 11 15 'face 'default)
        (push (insert-type-snap :step "init"
                               :f5 (get-text-property 5 'face)
                               :f6 (get-text-property 6 'face)
                               :f7 (get-text-property 7 'face)
                               :m-pos (marker-position m)) snaps)
        (goto-char 6)
        (insert "XX")
        (setq my-it-log (cons "ins@6" my-it-log))
        (push (insert-type-snap :step "ins@6"
                               :f5 (get-text-property 5 'face)
                               :f6 (get-text-property 6 'face)
                               :f7 (get-text-property 7 'face)
                               :m-pos (marker-position m)) snaps)
        (goto-char 5)
        (insert "YY")
        (setq my-it-log (cons "ins@5" my-it-log))
        (push (insert-type-snap :step "ins@5"
                               :f5 (get-text-property 5 'face)
                               :f6 (get-text-property 6 'face)
                               :f7 (get-text-property 7 'face)
                               :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (its-step s) (its-f5 s)
                                                (its-f6 s) (its-f7 s)
                                                (its-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S it-log=%S"
                       results (reverse my-it-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'its-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              my-it-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
