//! Combo: cl-eieio region operations + kill-ring + overlays + markers
//! + textprop + buflocal + narrow + undo.
//! Tests complex region operations (kill-region, yank, yank-pop, exchange-point-and-mark)
//! with EIEIO objects, overlays, markers, and narrowing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_region_kill_yank_overlay_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ry-snap ()
    ((step :initarg :step :accessor rys-step :initform "")
     (buf-string :initarg :bs :accessor rys-bs :initform "")
     (m-pos :initarg :m-pos :accessor rys-mp :initform 0)
     (mark-active :initarg :ma :accessor rys-ma :initform nil)))
  (let* ((buf (generate-new-buffer "ry1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-ry-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 10))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ry-snap :step "init"
                      :bs (buffer-string)
                      :m-pos (marker-position m)
                      :ma mark-active) snaps)
        (set-mark 6)
        (goto-char 15)
        (kill-region (mark) (point))
        (setq my-ry-log (cons "kill@6-15" my-ry-log))
        (push (ry-snap :step "kill"
                      :bs (buffer-string)
                      :m-pos (marker-position m)
                      :ma mark-active) snaps)
        (goto-char (point-max))
        (let ((inhibit-message t)) (yank))
        (setq my-ry-log (cons "yank@eob" my-ry-log))
        (push (ry-snap :step "yank"
                      :bs (buffer-string)
                      :m-pos (marker-position m)
                      :ma mark-active) snaps)
        (undo-boundary)
        (primitive-undo 2 buffer-undo-list)
        (push (ry-snap :step "undo-kill-yank"
                      :bs (buffer-string)
                      :m-pos (marker-position m)
                      :ma mark-active) snaps)
        (set-marker m 3)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rys-step s) (rys-mp s)
                                                (length (rys-bs s)))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ry-log=%S"
                       results (reverse my-ry-log)))
        (put-text-property (1- (point-max)) (point-max) 'rys-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ry-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_region_kill_yank_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ry-narrow-snap ()
    ((step :initarg :step :accessor ryns-step :initform "")
     (buf-string :initarg :bs :accessor ryns-bs :initform "")
     (m-pos :initarg :m-pos :accessor ryns-mp :initform 0)
     (narrow-bounds :initarg :narrow :accessor ryns-narrow :initform nil)))
  (let* ((buf (generate-new-buffer "ry2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-ryn-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 18))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ry-narrow-snap :step "init"
                             :bs (buffer-string)
                             :m-pos (marker-position m)
                             :narrow (list (point-min) (point-max))) snaps)
        (save-restriction
          (narrow-to-region 8 28)
          (push (ry-narrow-snap :step "narrow"
                               :bs (buffer-substring-no-properties
                                    (point-min) (point-max))
                               :m-pos (marker-position m)
                               :narrow (list (point-min) (point-max))) snaps)
          (set-mark 12)
          (goto-char 22)
          (kill-region (mark) (point))
          (setq my-ryn-log (cons "kill-narrow@12-22" my-ryn-log))
          (push (ry-narrow-snap :step "kill-in-narrow"
                               :bs (buffer-substring-no-properties
                                    (point-min) (point-max))
                               :m-pos (marker-position m)
                               :narrow (list (point-min) (point-max))) snaps)
          (goto-char (point-max))
          (let ((inhibit-message t)) (yank))
          (setq my-ryn-log (cons "yank-in-narrow" my-ryn-log))
          (push (ry-narrow-snap :step "yank-in-narrow"
                               :bs (buffer-substring-no-properties
                                    (point-min) (point-max))
                               :m-pos (marker-position m)
                               :narrow (list (point-min) (point-max))) snaps))
        (push (ry-narrow-snap :step "widen"
                             :bs (buffer-string)
                             :m-pos (marker-position m)
                             :narrow (list (point-min) (point-max))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ryns-step s) (ryns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ryn-log=%S"
                       results (reverse my-ryn-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ryns-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ryn-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_region_copy_region_yank_pop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ry-pop-snap ()
    ((step :initarg :step :accessor ryps-step :initform "")
     (buf-string :initarg :bs :accessor ryps-bs :initform "")
     (m-pos :initarg :m-pos :accessor ryps-mp :initform 0)
     (kill-ring-len :initarg :kr :accessor ryps-kr :initform 0)))
  (let* ((buf (generate-new-buffer "ry3"))
         (snaps nil)
         (saved-kill-ring kill-ring))
    (unwind-protect
        (progn
          (setq kill-ring nil)
          (with-current-buffer buf
            (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
            (put-text-property 1 5 'face 'bold)
            (put-text-property 6 10 'face 'italic)
            (put-text-property 11 15 'face 'underline)
            (put-text-property 16 20 'face 'default)
            (put-text-property 21 25 'face 'highlight)
            (setq-local my-pop-log nil)
            (let* ((ov (make-overlay 6 15))
                   (_ (overlay-put ov 'face 'shadow))
                   (_ (overlay-put ov 'priority 5))
                   (m (set-marker (make-marker) 10))
                   (results nil))
              (setq buffer-undo-list nil)
              (undo-boundary)
              (push (ry-pop-snap :step "init"
                                :bs (buffer-string)
                                :m-pos (marker-position m)
                                :kr (length kill-ring)) snaps)
              (set-mark 1)
              (goto-char 5)
              (copy-region-as-kill (mark) (point))
              (setq my-pop-log (cons "copy@1-5" my-pop-log))
              (push (ry-pop-snap :step "copy1"
                                :bs (buffer-string)
                                :m-pos (marker-position m)
                                :kr (length kill-ring)) snaps)
              (set-mark 11)
              (goto-char 15)
              (copy-region-as-kill (mark) (point))
              (setq my-pop-log (cons "copy@11-15" my-pop-log))
              (push (ry-pop-snap :step "copy2"
                                :bs (buffer-string)
                                :m-pos (marker-position m)
                                :kr (length kill-ring)) snaps)
              (goto-char (point-max))
              (insert (current-kill 0 t))
              (setq my-pop-log (cons "insert-kill0" my-pop-log))
              (push (ry-pop-snap :step "insert-kill0"
                                :bs (buffer-string)
                                :m-pos (marker-position m)
                                :kr (length kill-ring)) snaps)
              (delete-region (- (point) (length (current-kill 0 t))) (point))
              (insert (current-kill 1 t))
              (setq my-pop-log (cons "insert-kill1" my-pop-log))
              (push (ry-pop-snap :step "insert-kill1"
                                :bs (buffer-string)
                                :m-pos (marker-position m)
                                :kr (length kill-ring)) snaps)
              (setq snaps (reverse snaps))
              (setq results (mapcar (lambda (s) (list (ryps-step s) (ryps-mp s)
                                                      (ryps-kr s))) snaps))
              (goto-char (point-max))
              (insert (format " | results=%S pop-log=%S"
                             results (reverse my-pop-log)))
              (set-marker m 3)
              (put-text-property (1- (point-max)) (point-max) 'ryps-log t)
              (list (buffer-string)
                    (length snaps) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (length kill-ring)
                    my-pop-log))))
      (setq kill-ring saved-kill-ring))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_region_delete_dup_region_overlay_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ry-del-snap ()
    ((step :initarg :step :accessor ryds-step :initform "")
     (ov-alive :initarg :alive :accessor ryds-alive :initform nil)
     (buf-string :initarg :bs :accessor ryds-bs :initform "")
     (m-pos :initarg :m-pos :accessor ryds-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ry4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-del-log nil)
      (let* ((ov1 (make-overlay 6 15))
             (ov2 (make-overlay 16 25))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'evaporate t))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'evaporate t))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ry-del-snap :step "init"
                          :alive (and (overlay-live-p ov1) (overlay-live-p ov2))
                          :bs (buffer-string)
                          :m-pos (marker-position m)) snaps)
        (delete-region 6 15)
        (setq my-del-log (cons "del@6-15" my-del-log))
        (push (ry-del-snap :step "del1"
                          :alive (and (overlay-live-p ov1) (overlay-live-p ov2))
                          :bs (buffer-string)
                          :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (delete-region 11 20)
        (setq my-del-log (cons "del@11-20" my-del-log))
        (push (ry-del-snap :step "del2"
                          :alive (and (overlay-live-p ov1) (overlay-live-p ov2))
                          :bs (buffer-string)
                          :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (push (ry-del-snap :step "undo-del2"
                          :alive (and (overlay-live-p ov1) (overlay-live-p ov2))
                          :bs (buffer-string)
                          :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (ry-del-snap :step "undo-del1"
                          :alive (and (overlay-live-p ov1) (overlay-live-p ov2))
                          :bs (buffer-string)
                          :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ryds-step s) (ryds-alive s)
                                                (ryds-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S del-log=%S"
                       results (reverse my-del-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ryds-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-live-p ov1) (overlay-live-p ov2)
              my-del-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_region_kill_yank_with_props_preserved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ry-props-snap ()
    ((step :initarg :step :accessor ryps2-step :initform "")
     (face-at-yank-start :initarg :fy :accessor ryps2-fy :initform nil)
     (face-at-yank-end :initarg :fe :accessor ryps2-fe :initform nil)
     (m-pos :initarg :m-pos :accessor ryps2-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ry5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-ryp-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 10))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ry-props-snap :step "init"
                            :fy (get-text-property 1 'face)
                            :fe (get-text-property 25 'face)
                            :m-pos (marker-position m)) snaps)
        (set-mark 6)
        (goto-char 20)
        (kill-region (mark) (point))
        (setq my-ryp-log (cons "kill@6-20" my-ryp-log))
        (push (ry-props-snap :step "kill"
                            :fy (get-text-property 1 'face)
                            :fe (get-text-property 6 'face)
                            :m-pos (marker-position m)) snaps)
        (goto-char 5)
        (let ((inhibit-message t)) (yank))
        (setq my-ryp-log (cons "yank@5" my-ryp-log))
        (push (ry-props-snap :step "yank"
                            :fy (get-text-property 6 'face)
                            :fe (get-text-property 19 'face)
                            :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (push (ry-props-snap :step "undo-yank"
                            :fy (get-text-property 1 'face)
                            :fe (get-text-property 6 'face)
                            :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (ry-props-snap :step "undo-kill"
                            :fy (get-text-property 1 'face)
                            :fe (get-text-property 25 'face)
                            :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ryps2-step s) (ryps2-fy s)
                                                (ryps2-fe s) (ryps2-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ryp-log=%S"
                       results (reverse my-ryp-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ryps2-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ryp-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
