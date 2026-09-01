//! Combo: cl-eieio post/pre-command-hook + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests command hooks with EIEIO objects tracking buffer state changes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_post_command_hook_point_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass command-snap ()
    ((cmd :initarg :cmd :accessor cs-cmd :initform "")
     (point-before :initarg :point-before :accessor cs-before :initform 0)
     (point-after :initarg :point-after :accessor cs-after :initform 0)
     (buf-string :initarg :buf-string :accessor cs-bs :initform "")))
  (let* ((buf (generate-new-buffer "ch1"))
         (point-before 0)
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 15 'zone 'z3)
      (put-text-property 16 20 'zone 'z4)
      (setq-local my-point-before point-before
                  my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (add-hook 'pre-command-hook
                  (lambda ()
                    (setq point-before (point)))
                  nil t)
        (add-hook 'post-command-hook
                  (lambda ()
                    (push (command-snap :cmd (or (this-command-keys-vector "") "")
                                       :point-before point-before
                                       :point-after (point)
                                       :buf-string (buffer-string))
                          snaps))
                  nil t)
        (undo-boundary)
        (goto-char 3)
        (insert "XX")
        (goto-char 10)
        (insert "YY")
        (delete-region 5 8)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cs-before s) (cs-after s))) snaps))
        (remove-hook 'pre-command-hook (car (default-value 'pre-command-hook)) t)
        (remove-hook 'post-command-hook (car (default-value 'post-command-hook)) t)
        (goto-char (point-max))
        (insert (format " | results=%s snaps=%d m=%d ov=[%d,%d]"
                       results (length snaps) (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'ch-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_post_command_overlay_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ov-state-snap ()
    ((cmd-name :initarg :cmd :accessor oss-cmd :initform "")
     (ov-start :initarg :ov-start :accessor oss-start :initform 0)
     (ov-end :initarg :ov-end :accessor oss-end :initform 0)
     (point :initarg :point :accessor oss-point :initform 0)
     (ov-props :initarg :ov-props :accessor oss-props :initform nil)))
  (let* ((buf (generate-new-buffer "ch2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAAAAAA")
      (put-text-property 1 6 'section 's1)
      (put-text-property 7 12 'section 's2)
      (put-text-property 13 20 'section 's3)
      (setq-local my-snaps snaps)
      (let* ((ov1 (make-overlay 3 8))
             (ov2 (make-overlay 12 18))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov2 'invisible t))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (add-hook 'post-command-hook
                  (lambda ()
                    (push (ov-state-snap :cmd "post"
                                        :ov-start (overlay-start ov1)
                                        :ov-end (overlay-end ov1)
                                        :point (point)
                                        :ov-props (list (overlay-get ov1 'priority)
                                                       (overlay-get ov2 'invisible)))
                          snaps))
                  nil t)
        (undo-boundary)
        (goto-char 5)
        (insert "PPP")
        (overlay-put ov1 'priority 5)
        (goto-char 14)
        (insert "QQQ")
        (overlay-put ov2 'invisible nil)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (oss-start s) (oss-end s) (oss-point s)))
                             snaps))
        (remove-hook 'post-command-hook (car (default-value 'post-command-hook)) t)
        (goto-char (point-max))
        (insert (format " | results=%d m=%d ov1=[%d,%d] ov2=[%d,%d]"
                       (length results) (marker-position m)
                       (overlay-start ov1) (overlay-end ov1)
                       (overlay-start ov2) (overlay-end ov2)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'oss-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov1) (overlay-end ov1)
                (overlay-start ov2) (overlay-end ov2)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_command_hook_narrow_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass excursion-snap ()
    ((narrow-min :initarg :nmin :accessor es-nmin :initform 1)
     (narrow-max :initarg :nmax :accessor es-nmax :initform 1)
     (point :initarg :point :accessor es-point :initform 1)
     (buf-sub :initarg :buf-sub :accessor es-sub :initform "")))
  (let* ((buf (generate-new-buffer "ch3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'region 'r1)
      (put-text-property 6 10 'region 'r2)
      (put-text-property 11 15 'region 'r3)
      (put-text-property 16 20 'region 'r4)
      (put-text-property 21 25 'region 'r5)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (add-hook 'post-command-hook
                  (lambda ()
                    (push (excursion-snap :nmin (point-min)
                                         :nmax (point-max)
                                         :point (point)
                                         :buf-sub (buffer-substring-no-properties
                                                  (point-min) (point-max)))
                          snaps))
                  nil t)
        (undo-boundary)
        (save-excursion
          (save-restriction
            (narrow-to-region 6 15)
            (goto-char 8)
            (insert "XX")))
        (save-excursion
          (goto-char 3)
          (insert "YY"))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (es-nmin s) (es-nmax s) (es-point s)))
                             snaps))
        (remove-hook 'post-command-hook (car (default-value 'post-command-hook)) t)
        (goto-char (point-max))
        (insert (format " | results=%d snaps=%d m=%d ov=[%d,%d]"
                       (length results) (length snaps) (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'es-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_command_hook_textprop_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass prop-change-snap ()
    ((prop-name :initarg :prop :accessor pcs-prop :initform "")
     (range :initarg :range :accessor pcs-range :initform nil)
     (value :initarg :value :accessor pcs-val :initform nil)
     (buf-string :initarg :buf-string :accessor pcs-bs :initform "")))
  (let* ((buf (generate-new-buffer "ch4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (add-hook 'post-command-hook
                  (lambda ()
                    (push (prop-change-snap :prop "zone"
                                           :range (list (point) (marker-position m))
                                           :value (get-text-property (point) 'zone)
                                           :buf-string (buffer-string))
                          snaps))
                  nil t)
        (undo-boundary)
        (goto-char 3)
        (put-text-property 1 10 'zone 'modified)
        (goto-char 12)
        (put-text-property 11 20 'zone 'changed)
        (remove-text-properties 1 10 'zone)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (pcs-range s) (pcs-val s))) snaps))
        (remove-hook 'post-command-hook (car (default-value 'post-command-hook)) t)
        (goto-char (point-max))
        (insert (format " | results=%d m=%d ov=[%d,%d]"
                       (length results) (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'pcs-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_command_hook_marker_relocation_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass marker-cmd-snap ()
    ((m1-pos :initarg :m1 :accessor mcs-m1 :initform 0)
     (m2-pos :initarg :m2 :accessor mcs-m2 :initform 0)
     (point :initarg :point :accessor mcs-point :initform 0)
     (buf-len :initarg :buf-len :accessor mcs-blen :initform 0)))
  (let* ((buf (generate-new-buffer "ch5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 15 'zone 'z3)
      (put-text-property 16 20 'zone 'z4)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 15))
             (_ (set-marker-insertion-type m1 t))
             (results nil))
        (add-hook 'post-command-hook
                  (lambda ()
                    (push (marker-cmd-snap :m1 (marker-position m1)
                                          :m2 (marker-position m2)
                                          :point (point)
                                          :buf-len (buffer-size))
                          snaps))
                  nil t)
        (undo-boundary)
        (goto-char 3)
        (insert "XXX")
        (delete-region 8 12)
        (goto-char (point-max))
        (insert "END")
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mcs-m1 s) (mcs-m2 s) (mcs-point s)))
                             snaps))
        (remove-hook 'post-command-hook (car (default-value 'post-command-hook)) t)
        (goto-char (point-max))
        (insert (format " | results=%s m1=%d m2=%d ov=[%d,%d]"
                       results (marker-position m1) (marker-position m2)
                       (overlay-start ov) (overlay-end ov)))
        (put-text-property (1- (point-max)) (point-max) 'mcs-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m1)
                (marker-position m2)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
